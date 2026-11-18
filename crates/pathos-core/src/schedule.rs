//! The samples at which notes start and stop in a render (ADR 15), as
//! `spec/Pathos/Schedule.lean` specifies them.

use crate::project::{PPQ, Project, TempoChange, Tick};

/// A note starting or stopping at a sample of the render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Off {
        sample: u128,
        track: u64,
        pitch: u8,
    },
    On {
        sample: u128,
        track: u64,
        pitch: u8,
        velocity: u8,
    },
}

impl Event {
    /// The order of events: by sample, then a note-off before a note-on, so
    /// that a note may start where another of the same pitch stops, then by
    /// track, pitch and velocity. Two events with the same key are equal.
    fn key(&self) -> (u128, u8, u64, u8, u8) {
        match *self {
            Event::Off {
                sample,
                track,
                pitch,
            } => (sample, 0, track, pitch, 0),
            Event::On {
                sample,
                track,
                pitch,
                velocity,
            } => (sample, 1, track, pitch, velocity),
        }
    }
}

/// The tempo changes of a project, each with the time at which it starts, so
/// that the sample of a tick takes a search rather than a walk through every
/// change. It is built once per schedule or score.
#[derive(Clone, Debug)]
pub struct TempoMap {
    /// Each change with [`PPQ`] times the microseconds from the start to it.
    changes: Vec<(TempoChange, u128)>,
}

impl TempoMap {
    /// The sample of `tick` at `rate` samples per second, rounded down once.
    /// It does not allocate. It does not overflow for a tick up to a beat past
    /// [`MAX_TICK`](crate::MAX_TICK).
    pub fn sample_of(&self, rate: u64, tick: Tick) -> u128 {
        // The first change is at tick 0, so one starts on or before `tick`.
        let index = self
            .changes
            .partition_point(|(change, _)| change.tick <= tick)
            - 1;
        let (change, start) = self.changes[index];
        let time = start + u128::from(tick - change.tick) * u128::from(change.us_per_quarter);
        // The product of the time and the rate can exceed `u128`, so the time
        // is split by the divisor first; the result is the same.
        let divisor = u128::from(PPQ) * 1_000_000;
        let rate = u128::from(rate);
        time / divisor * rate + time % divisor * rate / divisor
    }
}

impl Project {
    /// The tempo changes, ready to give the sample of a tick.
    pub fn tempo_map(&self) -> TempoMap {
        let mut changes: Vec<(TempoChange, u128)> = Vec::with_capacity(self.tempo().len());
        for &change in self.tempo() {
            // Each tempo applies up to the next change.
            let start = changes.last().map_or(0, |&(before, start)| {
                start + u128::from(change.tick - before.tick) * u128::from(before.us_per_quarter)
            });
            changes.push((change, start));
        }
        TempoMap { changes }
    }

    /// The events of every note at `rate` samples per second, in order. A note
    /// that starts and stops at the same sample is not heard and has no
    /// events, since its note-off would come before its note-on. Nor has a
    /// note of a track that is not heard: a muted one (ADR 20), or one not
    /// soloed while another is (ADR 21).
    pub fn schedule(&self, rate: u64) -> Vec<Event> {
        let tempo = self.tempo_map();
        let mut events = Vec::new();
        for clip in self.clips() {
            if !self.heard(clip.track) {
                continue;
            }
            for note in self.notes(clip.id) {
                let on = tempo.sample_of(rate, clip.start + note.start);
                let off = tempo.sample_of(rate, clip.start + note.stop());
                if on == off {
                    continue;
                }
                events.push(Event::On {
                    sample: on,
                    track: clip.track,
                    pitch: note.pitch,
                    velocity: note.velocity,
                });
                events.push(Event::Off {
                    sample: off,
                    track: clip.track,
                    pitch: note.pitch,
                });
            }
        }
        events.sort_by_key(Event::key);
        events
    }
}
