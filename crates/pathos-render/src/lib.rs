//! The deterministic offline render of pathos (ADR 15): the note schedule of a
//! project, played by a built-in instrument.
//!
//! The arithmetic uses only addition, subtraction, multiplication, division,
//! comparison, the absolute value and conversions of `f32` and `f64`, which
//! IEEE 754 and Rust define exactly, and no function of the platform's math
//! library. A render is therefore the same bit for bit on every platform.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use pathos_core::{Event, Project, UNITY_VOLUME};

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

/// The first `frames` samples of the project at `sample_rate`, in one channel.
/// Each track is weighted by its volume (ADR 20).
pub fn render(project: &Project, sample_rate: NonZeroU32, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
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
}

/// A note as the instrument plays it: from frame `on` up to `off`.
#[derive(Clone, Copy, Debug)]
struct Sound {
    on: u128,
    off: u128,
    /// The fraction of a cycle the note advances each frame.
    step: f64,
    level: f32,
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
                    // A volume of 1000 is a gain of exactly 1, which leaves
                    // the level as it was before tracks had a volume.
                    let volume = project.track(track).map_or(UNITY_VOLUME, |t| t.volume);
                    let gain = f32::from(volume) / f32::from(UNITY_VOLUME);
                    sounding.insert((track, pitch), notes.len());
                    notes.push(Sound {
                        on: sample,
                        off: sample,
                        step: cycles - (cycles as u64) as f64,
                        level: f32::from(velocity) / 127.0 * gain,
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
        }
    }

    /// Writes the samples from frame `start` on to `out`. Each sample depends
    /// only on its frame, so blocks rendered one after another join into the
    /// render of the whole. It does not allocate, so it can run on the audio
    /// thread.
    pub fn render(&self, start: u64, out: &mut [f32]) {
        out.fill(0.0);
        let start = u128::from(start);
        let end = start + out.len() as u128;
        for note in &self.notes {
            if note.on >= end {
                break;
            }
            for frame in note.on.max(start)..note.off.min(end) {
                out[(frame - start) as usize] += note.sample(frame, self.ramp);
            }
        }
    }
}

impl Sound {
    /// A triangle wave whose level rises over the first 5 ms of the note and
    /// falls over its last 5 ms, so that it starts and stops without a click.
    fn sample(&self, frame: u128, ramp: f32) -> f32 {
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
        self.level * envelope * triangle
    }
}
