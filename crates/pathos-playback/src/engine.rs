//! What the audio thread does each block, apart from the device: take the
//! newest score, follow the transport and render the frames (ADR 18).

#[cfg(loom)]
use loom::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
#[cfg(not(loom))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

use pathos_render::{Frame, Score};

use crate::handoff::Player;

/// The most frames rendered at once. A longer block is rendered in parts, so
/// the buffer is allocated once, before playback starts.
const CHUNK: usize = 4096;

const NO_SEEK: u64 = u64::MAX;

/// Play, stop and seek, set by the UI thread and followed by the audio thread
/// at the start of each block, and what the audio thread reports back: the
/// frame it has reached and the meter.
#[derive(Debug)]
pub struct Transport {
    playing: AtomicBool,
    /// The frame to continue from, or `NO_SEEK`. A later seek replaces an
    /// earlier one the audio thread has not yet followed.
    seek: AtomicU64,
    position: AtomicU64,
    /// The bits of the loudest sample of each channel of the song since the
    /// UI thread last took them (ADR 25). The bits of non-negative floats are
    /// in the order of their values, so the larger bits are the louder peak.
    peaks: [AtomicU32; 2],
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: AtomicBool::new(false),
            seek: AtomicU64::new(NO_SEEK),
            position: AtomicU64::new(0),
            peaks: [AtomicU32::new(0), AtomicU32::new(0)],
        }
    }
}

impl Transport {
    /// Starts playback. The `Release` store pairs with the `Acquire` load in
    /// [`begin_block`](Self::begin_block), so any [`seek`](Self::seek) written
    /// before `play` is visible once playback is observed.
    pub fn play(&self) {
        self.playing.store(true, Ordering::Release);
    }

    pub fn stop(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self, frame: u64) {
        // A store would do, but loom lets a later swap in `begin_block` miss a
        // plain store (https://github.com/tokio-rs/loom/issues/254).
        self.seek.swap(frame.min(NO_SEEK - 1), Ordering::Relaxed);
    }

    /// Reads whether playback is running and consumes any pending seek for the
    /// audio thread's next block.
    ///
    /// Loads `playing` with `Acquire` before swapping `seek`, pairing with
    /// [`play`](Self::play)'s `Release` store so a seek made before `play` is
    /// always followed when `playing` is observed as `true`.
    pub fn begin_block(&self) -> (bool, Option<u64>) {
        let playing = self.playing.load(Ordering::Acquire);
        let seek = if self.seek.load(Ordering::Relaxed) != NO_SEEK {
            self.seek.swap(NO_SEEK, Ordering::Relaxed)
        } else {
            NO_SEEK
        };
        (playing, (seek != NO_SEEK).then_some(seek))
    }

    /// The frame that plays next, as of the last block.
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    /// The loudest sample of each channel of the song played since the last
    /// call, leaving silence for the next one (ADR 25).
    pub fn take_peaks(&self) -> Frame {
        self.peaks
            .each_ref()
            .map(|peak| f32::from_bits(peak.swap(0, Ordering::Relaxed)))
    }

    /// Raises the peaks to the loudest sample of each channel of `frames`.
    /// Called by the audio thread after rendering each block of the song
    /// (ADR 25).
    pub fn meter(&self, frames: &[Frame]) {
        let mut loudest = [0.0f32; 2];
        for frame in frames {
            for (loudest, sample) in loudest.iter_mut().zip(frame) {
                *loudest = loudest.max(sample.abs());
            }
        }
        for (peak, loudest) in self.peaks.iter().zip(loudest) {
            peak.fetch_max(loudest.to_bits(), Ordering::Relaxed);
        }
    }
}

/// The audio thread's state.
pub struct Engine {
    player: Player<Score>,
    transport: Arc<Transport>,
    position: u64,
    frames: Box<[Frame]>,
}

impl Engine {
    pub fn new(player: Player<Score>, transport: Arc<Transport>) -> Self {
        Self {
            player,
            transport,
            position: 0,
            frames: vec![[0.0; 2]; CHUNK].into_boxed_slice(),
        }
    }

    /// Fills `out`, whose frames each have `channels` samples, with the next
    /// frames of the newest score: the left and the right channel in the
    /// first two, silence in any further one, and the mean of both on a
    /// device with one channel (ADR 21). When the playhead reaches the end of
    /// the score's loop, it continues from the loop's start within the same
    /// block (ADR 22). It fills `out` with silence without advancing the
    /// playhead while stopped, before a score is published, or when `channels`
    /// is zero. It does not wait, allocate or free.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        let (playing, seek) = self.transport.begin_block();
        if let Some(frame) = seek {
            self.position = frame;
        }
        match self.player.current() {
            Some(score) if playing && channels > 0 => {
                let looped = score.looped_frames();
                let mut out = out;
                while out.len() >= channels {
                    let mut count = (out.len() / channels).min(CHUNK);
                    // Stop at the end of the loop if the playhead is before it.
                    if let Some(looped) = &looped
                        && self.position < looped.end
                    {
                        count = count.min((looped.end - self.position) as usize);
                    }
                    let (block, rest) = out.split_at_mut(count * channels);
                    out = rest;
                    let frames = &mut self.frames[..count];
                    // The meter shows the song, not the metronome (ADR 25).
                    score.render_song(self.position, frames);
                    self.transport.meter(frames);
                    score.add_clicks(self.position, frames);
                    for (out, &[left, right]) in block.chunks_mut(channels).zip(frames.iter()) {
                        match out {
                            [mono] => *mono = (left + right) * 0.5,
                            [out_left, out_right, rest @ ..] => {
                                *out_left = left;
                                *out_right = right;
                                rest.fill(0.0);
                            }
                            [] => {}
                        }
                    }
                    self.position += count as u64;
                    if let Some(looped) = &looped
                        && self.position == looped.end
                    {
                        self.position = looped.start;
                    }
                }
                // Samples short of a whole frame, which a device never asks for.
                out.fill(0.0);
            }
            _ => out.fill(0.0),
        }
        self.transport
            .position
            .store(self.position, Ordering::Relaxed);
    }
}
