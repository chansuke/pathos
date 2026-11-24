//! Expected values written down by hand, so that a mistake shared by this crate
//! and the model does not go unnoticed.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

use pathos_core::{Command, Edit, History, Note};

fn apply(history: &mut History, command: Command) {
    history.edit(Edit::Apply(command)).unwrap();
}

fn ids(history: &History) -> Vec<u64> {
    history
        .present()
        .clips()
        .iter()
        .map(|clip| clip.id)
        .collect()
}

/// A clip created after an undo does not receive the id of the undone clip
/// (ADR 10).
#[test]
fn undo_does_not_reissue_a_clip_id() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 240,
        },
    );
    assert_eq!(ids(&history), [0]);

    history.edit(Edit::Undo).unwrap();
    assert_eq!(history.next_clip_id(), 1);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 480,
            length: 240,
        },
    );
    assert_eq!(ids(&history), [1]);

    apply(&mut history, Command::SplitClip { id: 1, tick: 600 });
    history.edit(Edit::Undo).unwrap();
    history.edit(Edit::Redo).unwrap();
    assert_eq!(ids(&history), [1, 2]);
    assert_eq!(history.next_clip_id(), 3);
}

/// A rejected command does not consume an id.
#[test]
fn rejected_command_keeps_the_next_clip_id() {
    let mut history = History::default();
    let rejected = history.edit(Edit::Apply(Command::AddClip {
        track: 0,
        start: 0,
        length: 240,
    }));
    assert!(rejected.is_err());
    assert_eq!(history.next_clip_id(), 0);
}

fn note(start: u64, length: u64, pitch: u8) -> Note {
    Note {
        start,
        length,
        pitch,
        velocity: 100,
    }
}

fn add_note(history: &mut History, id: u64, note: Note) {
    apply(
        history,
        Command::AddNote {
            id,
            start: note.start,
            length: note.length,
            pitch: note.pitch.into(),
            velocity: note.velocity.into(),
        },
    );
}

/// A split gives each note to the clip it starts in, relative to that clip,
/// and cuts a note that crosses it at the split (ADR 14).
#[test]
fn split_gives_each_note_to_the_clip_it_starts_in() {
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
    add_note(&mut history, 0, note(0, 480, 60));
    add_note(&mut history, 0, note(240, 480, 64));
    add_note(&mut history, 0, note(480, 240, 67));
    apply(&mut history, Command::SplitClip { id: 0, tick: 1440 });

    let project = history.present();
    let notes = |id| project.notes(id).copied().collect::<Vec<_>>();
    let clips = project.clips();
    assert_eq!(clips[0].length, 480);
    assert_eq!(notes(0), [note(0, 480, 60), note(240, 240, 64)]);
    assert_eq!((clips[1].start, clips[1].length), (1440, 480));
    assert_eq!(notes(1), [note(0, 240, 67)]);
}

/// Notes are ordered by start, then pitch, whatever the order they were added
/// in.
#[test]
fn notes_are_ordered_by_start_then_pitch() {
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
    for added in [note(240, 240, 60), note(0, 240, 64), note(0, 240, 60)] {
        add_note(&mut history, 0, added);
    }
    let notes: Vec<Note> = history.present().notes(0).copied().collect();
    assert_eq!(
        notes,
        [note(0, 240, 60), note(0, 240, 64), note(240, 240, 60)]
    );
}
