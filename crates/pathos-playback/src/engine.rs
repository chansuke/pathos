//! What the audio thread does each block, apart from the device: take the
//! newest score, follow the transport and render the frames (ADR 18).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use pathos_render::{Frame, Score};

use crate::handoff::Player;

/// The most frames rendered at once. A longer block is rendered in parts, so
/// the buffer is allocated once, before playback starts.
const CHUNK: usize = 4096;

const NO_SEEK: u64 = u64::MAX;

/// Play, stop and seek, set by the UI thread and followed by the audio thread
/// at the start of each block, and the frame the audio thread has reached.
#[derive(Debug)]
pub struct Transport {
    playing: AtomicBool,
    /// The frame to continue from, or `NO_SEEK`. A later seek replaces an
    /// earlier one the audio thread has not yet followed.
    seek: AtomicU64,
    position: AtomicU64,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: AtomicBool::new(false),
            seek: AtomicU64::new(NO_SEEK),
            position: AtomicU64::new(0),
        }
    }
}

impl Transport {
    pub fn play(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self, frame: u64) {
        self.seek.store(frame.min(NO_SEEK - 1), Ordering::Relaxed);
    }

    /// The frame that plays next, as of the last block.
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
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
    /// device with one channel (ADR 21). It is silent while stopped or before
    /// a score is published. It does not wait, allocate or free.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        let seek = self.transport.seek.swap(NO_SEEK, Ordering::Relaxed);
        if seek != NO_SEEK {
            self.position = seek;
        }
        let playing = self.transport.is_playing();
        match self.player.current() {
            Some(score) if playing => {
                for block in out.chunks_mut(CHUNK * channels) {
                    let frames = &mut self.frames[..block.len() / channels];
                    score.render(self.position, frames);
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
                    self.position += frames.len() as u64;
                }
            }
            _ => out.fill(0.0),
        }
        self.transport
            .position
            .store(self.position, Ordering::Relaxed);
    }
}
