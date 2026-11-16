//! Expected schedules written down by hand, so that a mistake shared by this
//! crate and the model does not go unnoticed.

use pathos_core::{Command, Edit, Event, History};

fn apply(history: &mut History, command: Command) {
    history.edit(Edit::Apply(command)).unwrap();
}

fn add_note(history: &mut History, id: u64, start: u64, length: u64, pitch: u64, velocity: u64) {
    apply(
        history,
        Command::AddNote {
            id,
            start,
            length,
            pitch,
            velocity,
        },
    );
}

/// At 120 BPM a quarter note of 960 ticks lasts half a second, 24 000 samples
/// at 48 kHz. A note-off comes before a note-on at the same sample.
#[test]
fn notes_start_and_stop_at_their_samples() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 960,
            length: 960,
        },
    );
    add_note(&mut history, 0, 0, 480, 60, 100);
    add_note(&mut history, 0, 480, 480, 60, 90);

    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 24_000,
                track: 0,
                pitch: 60,
                velocity: 100,
            },
            Event::Off {
                sample: 36_000,
                track: 0,
                pitch: 60,
            },
            Event::On {
                sample: 36_000,
                track: 0,
                pitch: 60,
                velocity: 90,
            },
            Event::Off {
                sample: 48_000,
                track: 0,
                pitch: 60,
            },
        ]
    );
}

/// A note shorter than a sample is not heard.
#[test]
fn a_note_within_one_sample_has_no_events() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
    );
    add_note(&mut history, 0, 0, 960, 60, 100);
    // At 1 Hz the whole note lies within the first second.
    assert_eq!(history.present().schedule(1), []);
    // At 2 Hz it covers the first sample.
    assert_eq!(history.present().schedule(2).len(), 2);
}
