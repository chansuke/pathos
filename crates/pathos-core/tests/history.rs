//! Expected values written down by hand, so that a mistake shared by this crate
//! and the model does not go unnoticed.

use pathos_core::{Command, Edit, History};

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
