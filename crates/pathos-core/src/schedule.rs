//! The samples at which notes start and stop in a render (ADR 15), as
//! `spec/Pathos/Schedule.lean` specifies them.

use crate::project::{PPQ, Project, Tick};

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

impl Project {
    /// The sample of `tick` at `rate` samples per second, rounded down once.
    /// `tick` is at most [`MAX_TICK`](crate::MAX_TICK).
    fn sample_of(&self, rate: u64, tick: Tick) -> u128 {
        // The product of the time and the rate can exceed `u128`, so the time
        // is split by the divisor first; the result is the same.
        let divisor = u128::from(PPQ) * 1_000_000;
        let time = self.micros_times_ppq(tick);
        let rate = u128::from(rate);
        time / divisor * rate + time % divisor * rate / divisor
    }

    /// The events of every note at `rate` samples per second, in order. A note
    /// that starts and stops at the same sample is not heard and has no
    /// events, since its note-off would come before its note-on. Nor has a
    /// note of a muted track (ADR 20).
    pub fn schedule(&self, rate: u64) -> Vec<Event> {
        let mut events = Vec::new();
        for clip in self.clips() {
            if self.track(clip.track).is_some_and(|track| track.muted) {
                continue;
            }
            for note in self.notes(clip.id) {
                let on = self.sample_of(rate, clip.start + note.start);
                let off = self.sample_of(rate, clip.start + note.stop());
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
