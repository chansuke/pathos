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

/// A tick lies at the nearest sample, and an exact half rounds up (ADR 32).
/// At 44.1 kHz and 120 BPM a tick lasts 22.96875 samples.
#[test]
fn a_tick_rounds_to_the_nearest_sample() {
    let tempo = History::default().present().tempo_map();
    // 344.53 samples.
    assert_eq!(tempo.sample_of(44_100, 15), 345);
    // 367.5 samples.
    assert_eq!(tempo.sample_of(44_100, 16), 368);
    // 390.47 samples.
    assert_eq!(tempo.sample_of(44_100, 17), 390);
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
    add_note(&mut history, 0, 0, 480, 60, 100);
    // At 1 Hz both ends of the note, a quarter of a second long, round to the
    // first sample.
    assert_eq!(history.present().schedule(1), []);
    // At 2 Hz its end rounds up to the second sample, so it covers the first.
    assert_eq!(history.present().schedule(2).len(), 2);
}
