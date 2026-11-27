//! Expected values written down by hand, so that a mistake shared by this crate
//! and the model does not go unnoticed.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

use pathos_core::{
    CENTER_PAN, Clip, Command, Edit, EditError, Event, History, MAX_PAN, MAX_TICK,
    MAX_US_PER_QUARTER, MAX_VOLUME, Note, TempoChange, Track, UNITY_VOLUME,
};

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

/// `SetTempo` inserts in tick order or overwrites an existing change at the
/// same tick; `RemoveTempo` removes a non-initial change and rejects missing
/// or initial ones.
#[test]
fn set_tempo_inserts_overwrites_and_remove_tempo_enforces_rules() {
    let mut history = History::default();
    assert_eq!(
        history.present().tempo(),
        [TempoChange {
            tick: 0,
            us_per_quarter: 500_000,
        }]
    );

    // Out-of-order inserts stay sorted by tick.
    apply(
        &mut history,
        Command::SetTempo {
            tick: 1_920,
            us_per_quarter: 400_000,
        },
    );
    apply(
        &mut history,
        Command::SetTempo {
            tick: 960,
            us_per_quarter: 600_000,
        },
    );
    assert_eq!(
        history.present().tempo(),
        [
            TempoChange {
                tick: 0,
                us_per_quarter: 500_000,
            },
            TempoChange {
                tick: 960,
                us_per_quarter: 600_000,
            },
            TempoChange {
                tick: 1_920,
                us_per_quarter: 400_000,
            },
        ]
    );

    // Overwriting an existing change replaces it in place.
    apply(
        &mut history,
        Command::SetTempo {
            tick: 960,
            us_per_quarter: 300_000,
        },
    );
    apply(
        &mut history,
        Command::SetTempo {
            tick: 0,
            us_per_quarter: 450_000,
        },
    );
    assert_eq!(
        history.present().tempo(),
        [
            TempoChange {
                tick: 0,
                us_per_quarter: 450_000,
            },
            TempoChange {
                tick: 960,
                us_per_quarter: 300_000,
            },
            TempoChange {
                tick: 1_920,
                us_per_quarter: 400_000,
            },
        ]
    );

    // Boundaries of SetTempo.
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTempo {
            tick: 480,
            us_per_quarter: 0,
        })),
        Err(EditError::InvalidTempo)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTempo {
            tick: 480,
            us_per_quarter: MAX_US_PER_QUARTER + 1,
        })),
        Err(EditError::InvalidTempo)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTempo {
            tick: MAX_TICK + 1,
            us_per_quarter: 500_000,
        })),
        Err(EditError::OutOfRange)
    );
    apply(
        &mut history,
        Command::SetTempo {
            tick: MAX_TICK,
            us_per_quarter: MAX_US_PER_QUARTER,
        },
    );

    // RemoveTempo rejects tick 0 and ticks without a change.
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveTempo { tick: 0 })),
        Err(EditError::InitialTempo)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveTempo { tick: 480 })),
        Err(EditError::NoTempoChange)
    );

    apply(&mut history, Command::RemoveTempo { tick: 960 });
    assert_eq!(
        history.present().tempo(),
        [
            TempoChange {
                tick: 0,
                us_per_quarter: 450_000,
            },
            TempoChange {
                tick: 1_920,
                us_per_quarter: 400_000,
            },
            TempoChange {
                tick: MAX_TICK,
                us_per_quarter: MAX_US_PER_QUARTER,
            },
        ]
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveTempo { tick: 960 })),
        Err(EditError::NoTempoChange)
    );

    history.edit(Edit::Undo).expect("undo restores tempo");
    assert_eq!(history.present().tempo()[1].tick, 960);
    assert_eq!(history.present().tempo()[1].us_per_quarter, 300_000);
}

/// `MoveClip` carries notes with the clip, moves across tracks, allows
/// overlapping its own previous range, and rejects overlaps, out-of-range
/// destinations, unknown tracks and unknown clips.
#[test]
fn move_clip_carries_notes_moves_across_tracks_and_checks_rules() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
    );
    let n0 = note(0, 480, 60);
    let n1 = note(480, 240, 64);
    add_note(&mut history, 0, n0);
    add_note(&mut history, 0, n1);
    apply(
        &mut history,
        Command::AddClip {
            track: 1,
            start: 960,
            length: 960,
        },
    );

    // Moving onto its own former position succeeds and shifts scheduled note times.
    apply(
        &mut history,
        Command::MoveClip {
            id: 0,
            track: 0,
            start: 480,
        },
    );
    assert_eq!(
        history.present().clips()[0],
        Clip {
            id: 0,
            track: 0,
            start: 480,
            length: 960,
        }
    );
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [n0, n1]
    );
    // At 120 BPM and 48 kHz: 480 ticks = 12 000 samples, 960 = 24 000, 1200 = 30 000.
    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 12_000,
                track: 0,
                pitch: 60,
                velocity: 100,
            },
            Event::Off {
                sample: 24_000,
                track: 0,
                pitch: 60,
            },
            Event::On {
                sample: 24_000,
                track: 0,
                pitch: 64,
                velocity: 100,
            },
            Event::Off {
                sample: 30_000,
                track: 0,
                pitch: 64,
            },
        ]
    );

    // Cross-track move touching clip 1 on the left (end == 960 == clip 1 start).
    apply(
        &mut history,
        Command::MoveClip {
            id: 0,
            track: 1,
            start: 0,
        },
    );
    assert_eq!(history.present().clips()[0].track, 1);
    assert_eq!(history.present().clips()[0].start, 0);
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [n0, n1]
    );

    // 1-tick overlap on the left and right edges of clip 1 [960, 1920) are rejected.
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 0,
            track: 1,
            start: 1,
        })),
        Err(EditError::Overlap)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 0,
            track: 1,
            start: 1_919,
        })),
        Err(EditError::Overlap)
    );

    // Touching clip 1 on the right (start == 1920 == clip 1 end) succeeds.
    apply(
        &mut history,
        Command::MoveClip {
            id: 0,
            track: 1,
            start: 1_920,
        },
    );

    // Out-of-range checks and exact MAX_TICK boundary.
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 0,
            track: 1,
            start: MAX_TICK - 959,
        })),
        Err(EditError::OutOfRange)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 0,
            track: 1,
            start: u64::MAX,
        })),
        Err(EditError::OutOfRange)
    );
    apply(
        &mut history,
        Command::MoveClip {
            id: 0,
            track: 1,
            start: MAX_TICK - 960,
        },
    );
    assert_eq!(history.present().clips()[0].stop(), MAX_TICK);

    // Unknown track and unknown clip.
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 0,
            track: 2,
            start: 0,
        })),
        Err(EditError::UnknownTrack)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 99,
            track: 0,
            start: 0,
        })),
        Err(EditError::UnknownClip)
    );
}

/// Deleting a clip removes its notes while leaving neighbouring clips' notes
/// untouched, and undoing restores both the clip and its notes.
#[test]
fn delete_clip_drops_its_notes_and_undo_restores_them() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    for start in [0, 960, 1_920] {
        apply(
            &mut history,
            Command::AddClip {
                track: 0,
                start,
                length: 960,
            },
        );
    }
    let left_note = note(0, 240, 60);
    let mid_notes = [note(0, 480, 64), note(480, 240, 67)];
    let right_note = note(120, 360, 72);
    add_note(&mut history, 0, left_note);
    for mid in mid_notes {
        add_note(&mut history, 1, mid);
    }
    add_note(&mut history, 2, right_note);

    apply(&mut history, Command::DeleteClip { id: 1 });
    assert_eq!(ids(&history), [0, 2]);
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [left_note]
    );
    assert_eq!(history.present().notes(1).count(), 0);
    assert_eq!(
        history.present().notes(2).copied().collect::<Vec<_>>(),
        [right_note]
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::DeleteClip { id: 1 })),
        Err(EditError::UnknownClip)
    );

    history.edit(Edit::Undo).expect("undo restores clip 1");
    assert_eq!(ids(&history), [0, 1, 2]);
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [left_note]
    );
    assert_eq!(
        history.present().notes(1).copied().collect::<Vec<_>>(),
        mid_notes
    );
    assert_eq!(
        history.present().notes(2).copied().collect::<Vec<_>>(),
        [right_note]
    );

    history.edit(Edit::Redo).expect("redo removes clip 1 again");
    assert_eq!(ids(&history), [0, 2]);
    assert_eq!(history.present().notes(1).count(), 0);
}

/// Touching clips (`end == start`) are accepted, a 1-tick overlap is rejected,
/// and a clip may end on `MAX_TICK` but not one tick past it.
#[test]
fn add_clip_touching_overlap_and_max_tick_boundaries() {
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

    // 1-tick overlap on left and right edges.
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: 0,
            length: 961,
        })),
        Err(EditError::Overlap)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: 1_919,
            length: 960,
        })),
        Err(EditError::Overlap)
    );

    // Touching on both sides succeeds.
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
    );
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 1_920,
            length: 960,
        },
    );
    assert_eq!(ids(&history), [0, 1, 2]);

    // Exact MAX_TICK end is accepted; one tick past or overflow is rejected.
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: MAX_TICK - 10,
            length: 11,
        })),
        Err(EditError::OutOfRange)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: MAX_TICK,
            length: 1,
        })),
        Err(EditError::OutOfRange)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: u64::MAX,
            length: 1,
        })),
        Err(EditError::OutOfRange)
    );
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: MAX_TICK - 10,
            length: 10,
        },
    );
    assert_eq!(history.present().clips()[3].stop(), MAX_TICK);
}

/// `RemoveNote` removes only the note with matching clip, start and pitch,
/// rejects missing notes and clips, and undo puts it back in order.
#[test]
fn remove_note_removes_only_the_named_note_and_undo_restores_it() {
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
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 960,
            length: 960,
        },
    );
    let a = note(0, 480, 60);
    let b = note(0, 480, 64);
    let c = note(480, 480, 60);
    for added in [a, b, c] {
        add_note(&mut history, 0, added);
    }
    add_note(&mut history, 1, a);

    apply(
        &mut history,
        Command::RemoveNote {
            id: 0,
            start: 0,
            pitch: 60,
        },
    );
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [b, c]
    );
    assert_eq!(history.present().notes(1).copied().collect::<Vec<_>>(), [a]);

    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveNote {
            id: 0,
            start: 0,
            pitch: 60,
        })),
        Err(EditError::UnknownNote)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveNote {
            id: 0,
            start: 1,
            pitch: 64,
        })),
        Err(EditError::UnknownNote)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveNote {
            id: 0,
            start: 0,
            pitch: 200,
        })),
        Err(EditError::UnknownNote)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveNote {
            id: 99,
            start: 0,
            pitch: 60,
        })),
        Err(EditError::UnknownClip)
    );

    history.edit(Edit::Undo).expect("undo restores note");
    assert_eq!(
        history.present().notes(0).copied().collect::<Vec<_>>(),
        [a, b, c]
    );
}

/// Track volume, pan, mute, solo and master volume enforce their ranges and
/// `heard` gives mute precedence over solo while solo silences other tracks.
#[test]
fn track_mix_and_master_volume_enforce_ranges_and_heard_rules() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(&mut history, Command::AddTrack);

    assert_eq!(
        history.present().track(0),
        Some(Track {
            volume: UNITY_VOLUME,
            muted: false,
            solo: false,
            pan: CENTER_PAN,
        })
    );
    assert_eq!(history.present().track(2), None);
    assert_eq!(history.present().master_volume(), UNITY_VOLUME);

    // Track volume boundaries.
    apply(
        &mut history,
        Command::SetTrackVolume {
            track: 0,
            volume: 0,
        },
    );
    assert_eq!(history.present().track(0).expect("track 0").volume, 0);
    apply(
        &mut history,
        Command::SetTrackVolume {
            track: 0,
            volume: u64::from(MAX_VOLUME),
        },
    );
    assert_eq!(
        history.present().track(0).expect("track 0").volume,
        MAX_VOLUME
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackVolume {
            track: 0,
            volume: u64::from(MAX_VOLUME) + 1,
        })),
        Err(EditError::InvalidVolume)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackVolume {
            track: 0,
            volume: u64::MAX,
        })),
        Err(EditError::InvalidVolume)
    );

    // Track pan boundaries.
    apply(&mut history, Command::SetTrackPan { track: 0, pan: 0 });
    assert_eq!(history.present().track(0).expect("track 0").pan, 0);
    apply(
        &mut history,
        Command::SetTrackPan {
            track: 0,
            pan: u64::from(MAX_PAN),
        },
    );
    assert_eq!(history.present().track(0).expect("track 0").pan, MAX_PAN);
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackPan {
            track: 0,
            pan: u64::from(MAX_PAN) + 1,
        })),
        Err(EditError::InvalidPan)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackPan {
            track: 0,
            pan: u64::MAX,
        })),
        Err(EditError::InvalidPan)
    );

    // Returning a track to default settings leaves it readable as default.
    apply(
        &mut history,
        Command::SetTrackVolume {
            track: 0,
            volume: u64::from(UNITY_VOLUME),
        },
    );
    apply(
        &mut history,
        Command::SetTrackPan {
            track: 0,
            pan: u64::from(CENTER_PAN),
        },
    );
    assert_eq!(history.present().track(0), Some(Track::default()));

    // Master volume boundaries.
    apply(&mut history, Command::SetMasterVolume { volume: 0 });
    assert_eq!(history.present().master_volume(), 0);
    apply(
        &mut history,
        Command::SetMasterVolume {
            volume: u64::from(MAX_VOLUME),
        },
    );
    assert_eq!(history.present().master_volume(), MAX_VOLUME);
    assert_eq!(
        history.edit(Edit::Apply(Command::SetMasterVolume {
            volume: u64::from(MAX_VOLUME) + 1,
        })),
        Err(EditError::InvalidVolume)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetMasterVolume { volume: u64::MAX })),
        Err(EditError::InvalidVolume)
    );

    // Mute and solo on unknown tracks are rejected.
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackMute {
            track: 2,
            muted: true,
        })),
        Err(EditError::UnknownTrack)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackSolo {
            track: 2,
            solo: true,
        })),
        Err(EditError::UnknownTrack)
    );

    // Mute + solo precedence in `heard`: muted wins, while solo still silences others.
    assert!(history.present().heard(0));
    assert!(history.present().heard(1));
    apply(
        &mut history,
        Command::SetTrackMute {
            track: 0,
            muted: true,
        },
    );
    assert!(!history.present().heard(0));
    assert!(history.present().heard(1));
    apply(
        &mut history,
        Command::SetTrackSolo {
            track: 0,
            solo: true,
        },
    );
    assert!(!history.present().heard(0));
    assert!(!history.present().heard(1));
    apply(
        &mut history,
        Command::SetTrackMute {
            track: 0,
            muted: false,
        },
    );
    assert!(history.present().heard(0));
    assert!(!history.present().heard(1));
    apply(
        &mut history,
        Command::SetTrackSolo {
            track: 1,
            solo: true,
        },
    );
    assert!(history.present().heard(0));
    assert!(history.present().heard(1));
}

/// When a command breaks more than one rule, `Project::apply` reports the
/// first check in the order fixed by `spec/Pathos/Project.lean`.
#[test]
fn commands_check_errors_in_specified_order() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    // Clip 0 sits near MAX_TICK so an out-of-range edit can also overlap it.
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: MAX_TICK - 960,
            length: 960,
        },
    );
    add_note(&mut history, 0, note(480, 480, 60));
    // Clip 1 sits at the start so MoveClip can test OutOfRange vs Overlap.
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
    );

    // AddClip: EmptyClip > UnknownTrack > OutOfRange > Overlap.
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 9,
            start: MAX_TICK,
            length: 0,
        })),
        Err(EditError::EmptyClip)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 9,
            start: MAX_TICK,
            length: 10,
        })),
        Err(EditError::UnknownTrack)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: MAX_TICK - 480,
            length: 960,
        })),
        Err(EditError::OutOfRange)
    );

    // MoveClip: UnknownClip > UnknownTrack > OutOfRange > Overlap.
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 99,
            track: 9,
            start: MAX_TICK,
        })),
        Err(EditError::UnknownClip)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 1,
            track: 9,
            start: MAX_TICK,
        })),
        Err(EditError::UnknownTrack)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::MoveClip {
            id: 1,
            track: 0,
            start: MAX_TICK - 480,
        })),
        Err(EditError::OutOfRange)
    );

    // SplitClip: UnknownClip > SplitOutsideClip.
    assert_eq!(
        history.edit(Edit::Apply(Command::SplitClip { id: 99, tick: 0 })),
        Err(EditError::UnknownClip)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SplitClip { id: 1, tick: 0 })),
        Err(EditError::SplitOutsideClip)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SplitClip { id: 1, tick: 960 })),
        Err(EditError::SplitOutsideClip)
    );

    // SetTempo: InvalidTempo > OutOfRange.
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTempo {
            tick: MAX_TICK + 1,
            us_per_quarter: 0,
        })),
        Err(EditError::InvalidTempo)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTempo {
            tick: MAX_TICK + 1,
            us_per_quarter: MAX_US_PER_QUARTER + 1,
        })),
        Err(EditError::InvalidTempo)
    );

    // AddNote: UnknownClip > EmptyNote > InvalidNote > NoteOutsideClip > NoteOverlap.
    assert_eq!(
        history.edit(Edit::Apply(Command::AddNote {
            id: 99,
            start: 2_000,
            length: 0,
            pitch: 200,
            velocity: 0,
        })),
        Err(EditError::UnknownClip)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: 2_000,
            length: 0,
            pitch: 200,
            velocity: 0,
        })),
        Err(EditError::EmptyNote)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: 2_000,
            length: 480,
            pitch: 128,
            velocity: 100,
        })),
        Err(EditError::InvalidNote)
    );
    assert_eq!(
        history.edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: 2_000,
            length: 480,
            pitch: 60,
            velocity: 0,
        })),
        Err(EditError::InvalidNote)
    );
    // Overlaps clip 0's note [480, 960) AND runs past clip 0's length 960.
    assert_eq!(
        history.edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: 480,
            length: 481,
            pitch: 60,
            velocity: 100,
        })),
        Err(EditError::NoteOutsideClip)
    );

    // RemoveNote: UnknownClip > UnknownNote.
    assert_eq!(
        history.edit(Edit::Apply(Command::RemoveNote {
            id: 99,
            start: 0,
            pitch: 60,
        })),
        Err(EditError::UnknownClip)
    );

    // SetTrackVolume: UnknownTrack > InvalidVolume.
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackVolume {
            track: 9,
            volume: u64::from(MAX_VOLUME) + 1,
        })),
        Err(EditError::UnknownTrack)
    );

    // SetTrackPan: UnknownTrack > InvalidPan.
    assert_eq!(
        history.edit(Edit::Apply(Command::SetTrackPan {
            track: 9,
            pan: u64::from(MAX_PAN) + 1,
        })),
        Err(EditError::UnknownTrack)
    );
}
