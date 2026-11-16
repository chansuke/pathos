//! The deterministic offline render of pathos (ADR 15): the note schedule of a
//! project, played by a built-in instrument.
//!
//! The arithmetic uses only addition, subtraction, multiplication, division,
//! comparison and the absolute value of `f32`, which IEEE 754 defines exactly,
//! and no function of the platform's math library. A render is therefore the
//! same bit for bit on every platform.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use pathos_core::{Event, Project};

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
/// Every track is added with the same weight until the project has a mixer.
pub fn render(project: &Project, sample_rate: NonZeroU32, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    let mut sounding = BTreeMap::new();
    for event in project.schedule(sample_rate.get().into()) {
        match event {
            Event::On {
                sample,
                track,
                pitch,
                velocity,
            } => {
                sounding.insert((track, pitch), (sample, velocity));
            }
            Event::Off {
                sample,
                track,
                pitch,
            } => {
                // Notes of one pitch on one track never overlap, and a note-on
                // comes before its note-off, so the note is sounding.
                let (on, velocity) = sounding
                    .remove(&(track, pitch))
                    .expect("a note-off follows the note-on of its note");
                play(&mut out, sample_rate, pitch, velocity, on, sample);
            }
        }
    }
    out
}

/// Adds a note from frame `on` up to `off` to `out`: a triangle wave whose
/// level rises over its first 5 ms and falls over its last 5 ms, so that it
/// starts and stops without a click and is silent outside the note.
fn play(out: &mut [f32], sample_rate: NonZeroU32, pitch: u8, velocity: u8, on: u128, off: u128) {
    let rate = sample_rate.get();
    let ramp = (rate / 200).max(1) as f32;
    // Only the fraction of a cycle per frame matters, and keeping it below one
    // lets a single subtraction keep the phase below one.
    let cycles = FREQUENCIES[usize::from(pitch)] / rate as f32;
    let step = cycles - (cycles as u64) as f32;
    let level = f32::from(velocity) / 127.0;
    let end = off.min(out.len() as u128);
    let mut phase: f32 = 0.0;
    for frame in on..end {
        let since = (frame - on + 1) as f32;
        let until = (off - frame) as f32;
        let envelope = (since / ramp).min(until / ramp).min(1.0);
        let triangle = 4.0 * (phase - 0.5).abs() - 1.0;
        out[frame as usize] += level * envelope * triangle;
        phase += step;
        if phase >= 1.0 {
            phase -= 1.0;
        }
    }
}
