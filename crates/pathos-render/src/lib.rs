//! The deterministic offline render of pathos (ADR 15): the note schedule of a
//! project, played by a built-in instrument.
//!
//! The arithmetic uses only addition, subtraction, multiplication, division,
//! comparison, the absolute value and conversions of `f32` and `f64`, which
//! IEEE 754 and Rust define exactly, and no function of the platform's math
//! library. A render is therefore the same bit for bit on every platform.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::ops::Range;

use pathos_core::{CENTER_PAN, Event, MAX_PAN, Project, Tick, Track, UNITY_VOLUME};

pub mod wav;

/// The frequency of each MIDI pitch in hertz, in equal temperament with A4
/// (pitch 69) at 440 Hz.
const FREQUENCIES: [f32; 128] = frequencies();

const fn frequencies() -> [f32; 128] {
    // 2^(1/12), rounded to the nearest f32.
    const SEMITONE: f32 = 1.059_463_1;
    let mut table = [0.0; 128];
    table[69] = 440.0;
    let mut pitch = 70;
    while pitch < 128 {
        table[pitch] = table[pitch - 1] * SEMITONE;
        pitch += 1;
    }
    let mut pitch = 69;
    while pitch > 0 {
        table[pitch - 1] = table[pitch] / SEMITONE;
        pitch -= 1;
    }
    table
}

/// One sample of the left and of the right channel (ADR 21).
pub type Frame = [f32; 2];

/// The first `frames` frames of the project at `sample_rate`. Each track is
/// weighted by its volume (ADR 20) and placed by its pan (ADR 21).
pub fn render(project: &Project, sample_rate: NonZeroU32, frames: usize) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; frames];
    Score::new(project, sample_rate).render(0, &mut out);
    out
}

/// The notes of a project at one sample rate, ready to play. Playback builds
/// one off the audio thread for each edit and renders it block by block
/// (ADR 18).
#[derive(Clone, Debug)]
pub struct Score {
    /// The frames over which a note fades in and out.
    ramp: f32,
    /// In the order of their note-on events, which fixes the order in which
    /// notes are added up and so the bits of each sample.
    notes: Vec<Sound>,
    /// The frames that playback repeats, if any (ADR 22).
    looped: Option<Range<u64>>,
}

/// A note as the instrument plays it: from frame `on` up to `off`.
#[derive(Clone, Copy, Debug)]
struct Sound {
    on: u128,
    off: u128,
    /// The fraction of a cycle the note advances each frame.
    step: f64,
    /// The level in the left and the right channel.
    levels: Frame,
}

/// The gains of a track in the left and the right channel: its volume, and a
/// linear balance that is 1 in both channels at the center (ADR 21). At volume
/// 1000 and the center both are exactly 1, which leaves a level as it was
/// before tracks had a volume and a pan.
fn gains(track: Track) -> Frame {
    let volume = f32::from(track.volume) / f32::from(UNITY_VOLUME);
    let center = f32::from(CENTER_PAN);
    let left = (f32::from(MAX_PAN - track.pan) / center).min(1.0);
    let right = (f32::from(track.pan) / center).min(1.0);
    [volume * left, volume * right]
}

impl Score {
    pub fn new(project: &Project, sample_rate: NonZeroU32) -> Self {
        let rate = sample_rate.get();
        let mut notes = Vec::new();
        let mut sounding = BTreeMap::new();
        for event in project.schedule(rate.into()) {
            match event {
                Event::On {
                    sample,
                    track,
                    pitch,
                    velocity,
                } => {
                    // Only the fraction of a cycle per frame matters.
                    let cycles = f64::from(FREQUENCIES[usize::from(pitch)]) / f64::from(rate);
                    let [left, right] = gains(project.track(track).unwrap_or_default());
                    let level = f32::from(velocity) / 127.0;
                    sounding.insert((track, pitch), notes.len());
                    notes.push(Sound {
                        on: sample,
                        off: sample,
                        step: cycles - (cycles as u64) as f64,
                        levels: [level * left, level * right],
                    });
                }
                Event::Off {
                    sample,
                    track,
                    pitch,
                } => {
                    // Notes of one pitch on one track never overlap, and a
                    // note-on comes before its note-off, so the note is
                    // sounding.
                    let index = sounding
                        .remove(&(track, pitch))
                        .expect("a note-off follows the note-on of its note");
                    notes[index].off = sample;
                }
            }
        }
        Self {
            ramp: (rate / 200).max(1) as f32,
            notes,
            looped: None,
        }
    }

    /// The score of `project`, whose playback repeats `ticks` (ADR 22). The
    /// range is turned into frames as the schedule turns ticks into samples,
    /// so it stays on the beat at any tempo. An empty range does not loop.
    pub fn looped(project: &Project, sample_rate: NonZeroU32, ticks: Range<Tick>) -> Self {
        let frame = |tick| {
            u64::try_from(project.sample_of(sample_rate.get().into(), tick)).unwrap_or(u64::MAX)
        };
        let frames = frame(ticks.start)..frame(ticks.end);
        Self {
            looped: (!frames.is_empty()).then_some(frames),
            ..Self::new(project, sample_rate)
        }
    }

    /// The frames that playback repeats, if any.
    pub fn looped_frames(&self) -> Option<Range<u64>> {
        self.looped.clone()
    }

    /// Writes the samples from frame `start` on to `out`. Each sample depends
    /// only on its frame, so blocks rendered one after another join into the
    /// render of the whole. It does not allocate, so it can run on the audio
    /// thread.
    pub fn render(&self, start: u64, out: &mut [Frame]) {
        out.fill([0.0; 2]);
        let start = u128::from(start);
        let end = start + out.len() as u128;
        for note in &self.notes {
            if note.on >= end {
                break;
            }
            for frame in note.on.max(start)..note.off.min(end) {
                let [left, right] = note.sample(frame, self.ramp);
                let out = &mut out[(frame - start) as usize];
                out[0] += left;
                out[1] += right;
            }
        }
    }
}

impl Sound {
    /// A triangle wave whose level rises over the first 5 ms of the note and
    /// falls over its last 5 ms, so that it starts and stops without a click.
    fn sample(&self, frame: u128, ramp: f32) -> Frame {
        let since = frame - self.on;
        let envelope = ((since + 1) as f32 / ramp)
            .min((self.off - frame) as f32 / ramp)
            .min(1.0);
        // The phase is computed from the frame, not accumulated, so that it
        // does not depend on where a block starts. f64 keeps it exact enough
        // for notes of hours.
        let cycles = since as f64 * self.step;
        let phase = (cycles - (cycles as u64) as f64) as f32;
        let triangle = 4.0 * (phase - 0.5).abs() - 1.0;
        self.levels.map(|level| level * envelope * triangle)
    }
}
