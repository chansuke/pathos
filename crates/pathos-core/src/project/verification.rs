//! Kani proof harnesses (ADR 11). Each harness checks this crate's code for
//! every value of its inputs, including that no arithmetic overflows and
//! nothing panics, on a project with a fixed number of tempo changes, clips
//! and notes.
//!
//! Lengths are fixed per harness rather than chosen by Kani, which keeps each
//! problem small enough for the solver. Clip commands leave the tempo alone and
//! tempo commands leave the clips alone, so each harness varies only the list
//! its command reads. Only the commands that read or write notes are given
//! notes.
//!
//! The harnesses use MiniSat. On `split_clip_of_1` in CI, CaDiCaL (Kani's
//! default) took 877 s and 16 GB, the whole memory of the runner; Kissat took
//! 194 s and 2.9 GB; MiniSat took 26 s and 2.3 GB.

use super::*;

/// `Project.wellFormed` of the Lean model without the notes, written so that
/// it does not overflow on the arbitrary values it is asked to reject.
fn well_formed(p: &Project, next_clip_id: u64) -> bool {
    let tempo_valid = |change: &TempoChange| {
        change.us_per_quarter > 0
            && change.us_per_quarter <= MAX_US_PER_QUARTER
            && change.tick <= MAX_TICK
    };
    let tempo = p.tempo.first().is_some_and(|first| first.tick == 0)
        && p.tempo.iter().all(tempo_valid)
        && p.tempo.windows(2).all(|pair| pair[0].tick < pair[1].tick);
    // Check the range first: `Clip::stop` and the overlap check assume it.
    let in_range = p.clips.iter().all(|clip| {
        clip.length > 0
            && clip
                .start
                .checked_add(clip.length)
                .is_some_and(|stop| stop <= MAX_TICK)
    });
    tempo
        && in_range
        && p.clips
            .iter()
            .all(|clip| clip.track < p.track_count && clip.id < next_clip_id && !p.collides(clip))
        && p.clips.windows(2).all(|pair| pair[0].id < pair[1].id)
}

/// `notesWellFormed` of the Lean model for every clip, together with the order
/// of the one list that holds the notes. Assumes `well_formed`. A note's clip
/// is found by a linear search: a binary search here made `move_clip_of_2`
/// take ten times as long.
fn notes_well_formed(p: &Project) -> bool {
    let key = |(id, note): &(u64, Note)| (*id, note.key());
    let valid = |(id, note): &(u64, Note)| {
        note.length > 0
            && note.pitch <= MAX_PITCH
            && (1..=MAX_VELOCITY).contains(&note.velocity)
            && p.clips.iter().any(|clip| {
                clip.id == *id
                    && note
                        .start
                        .checked_add(note.length)
                        .is_some_and(|stop| stop <= clip.length)
            })
    };
    let notes = &p.notes;
    // Check the range first: the overlap check assumes it.
    if !(notes.iter().all(valid) && notes.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))) {
        return false;
    }
    for i in 0..notes.len() {
        for j in i + 1..notes.len() {
            if notes[i].0 == notes[j].0 && notes[i].1.overlaps(&notes[j].1) {
                return false;
            }
        }
    }
    true
}

/// Any well-formed project with `TEMPO` tempo changes, `CLIPS` clips and
/// `NOTES` notes, with the id the next created clip receives.
fn any_project<const TEMPO: usize, const CLIPS: usize, const NOTES: usize>() -> (Project, u64) {
    let tempo: [TempoChange; TEMPO] = kani::any();
    let clips: [Clip; CLIPS] = kani::any();
    let notes: [(u64, Note); NOTES] = kani::any();
    let project = Project {
        track_count: kani::any(),
        tempo: tempo.to_vec(),
        clips: clips.to_vec(),
        notes: notes.to_vec(),
    };
    let next_clip_id = kani::any();
    kani::assume(well_formed(&project, next_clip_id) && notes_well_formed(&project));
    (project, next_clip_id)
}

/// `step_preserves_wellFormed` and `step_nextClipId_le` of the Lean model, for
/// the Rust code: the command either fails or keeps the project well-formed,
/// and never lowers the next clip id.
fn check_apply<const TEMPO: usize, const CLIPS: usize, const NOTES: usize>(command: Command) {
    let (project, next_clip_id) = any_project::<TEMPO, CLIPS, NOTES>();
    // Each increment of a counter takes one accepted edit, so a history cannot
    // reach `u64::MAX` of either.
    kani::assume(project.track_count < u64::MAX && next_clip_id < u64::MAX);
    let result = project.apply(next_clip_id, command);
    // The assertions below prove nothing unless some input gets here and is
    // accepted. CI fails a harness whose cover is not satisfied.
    kani::cover!(result.is_ok(), "the command is accepted");
    if let Ok((next, next_clip_id_after)) = result {
        assert!(next_clip_id <= next_clip_id_after);
        assert!(well_formed(&next, next_clip_id_after));
        // Checking the notes is most of the cost of a harness. Without notes
        // to start from, only `AddNote` can produce one, so the check would
        // only show that other commands do not.
        if NOTES > 0 || matches!(command, Command::AddNote { .. }) {
            assert!(notes_well_formed(&next));
        }
    }
}

/// Declares a harness that applies `$command` to a project with `$tempo`
/// tempo changes, `$clips` clips and `$notes` notes, none when omitted.
/// A command adds at most one element to a list of at most two, so loops run
/// at most three times.
macro_rules! apply_harness {
    ($name:ident, tempo = $tempo:literal, clips = $clips:literal, $command:expr) => {
        apply_harness!($name, tempo = $tempo, clips = $clips, notes = 0, $command);
    };
    (
        $name:ident,
        tempo = $tempo:literal,
        clips = $clips:literal,
        notes = $notes:literal,
        $command:expr
    ) => {
        #[kani::proof]
        #[kani::unwind(4)]
        #[kani::solver(minisat)]
        fn $name() {
            check_apply::<$tempo, $clips, $notes>($command);
        }
    };
}

apply_harness!(add_track, tempo = 1, clips = 0, Command::AddTrack);

apply_harness!(
    add_clip_to_0_clips,
    tempo = 1,
    clips = 0,
    Command::AddClip {
        track: kani::any(),
        start: kani::any(),
        length: kani::any(),
    }
);
apply_harness!(
    add_clip_to_1_clip,
    tempo = 1,
    clips = 1,
    Command::AddClip {
        track: kani::any(),
        start: kani::any(),
        length: kani::any(),
    }
);
apply_harness!(
    add_clip_to_2_clips,
    tempo = 1,
    clips = 2,
    Command::AddClip {
        track: kani::any(),
        start: kani::any(),
        length: kani::any(),
    }
);

apply_harness!(
    move_clip_of_1,
    tempo = 1,
    clips = 1,
    Command::MoveClip {
        id: kani::any(),
        track: kani::any(),
        start: kani::any(),
    }
);
apply_harness!(
    move_clip_of_2,
    tempo = 1,
    clips = 2,
    Command::MoveClip {
        id: kani::any(),
        track: kani::any(),
        start: kani::any(),
    }
);

apply_harness!(
    split_clip_of_1,
    tempo = 1,
    clips = 1,
    Command::SplitClip {
        id: kani::any(),
        tick: kani::any(),
    }
);
apply_harness!(
    split_clip_of_2,
    tempo = 1,
    clips = 2,
    Command::SplitClip {
        id: kani::any(),
        tick: kani::any(),
    }
);

apply_harness!(
    split_clip_with_1_note,
    tempo = 1,
    clips = 1,
    notes = 1,
    Command::SplitClip {
        id: kani::any(),
        tick: kani::any(),
    }
);
apply_harness!(
    split_clip_with_2_notes,
    tempo = 1,
    clips = 1,
    notes = 2,
    Command::SplitClip {
        id: kani::any(),
        tick: kani::any(),
    }
);

apply_harness!(
    delete_clip_of_1,
    tempo = 1,
    clips = 1,
    Command::DeleteClip { id: kani::any() }
);
apply_harness!(
    delete_clip_with_1_note,
    tempo = 1,
    clips = 1,
    notes = 1,
    Command::DeleteClip { id: kani::any() }
);
apply_harness!(
    delete_clip_of_2,
    tempo = 1,
    clips = 2,
    Command::DeleteClip { id: kani::any() }
);

apply_harness!(
    set_tempo_with_1,
    tempo = 1,
    clips = 0,
    Command::SetTempo {
        tick: kani::any(),
        us_per_quarter: kani::any(),
    }
);
apply_harness!(
    set_tempo_with_2,
    tempo = 2,
    clips = 0,
    Command::SetTempo {
        tick: kani::any(),
        us_per_quarter: kani::any(),
    }
);

// With one tempo change, at tick 0, removing a tempo change always fails, and
// two changes reach both of its errors as well as success.
apply_harness!(
    remove_tempo_with_2,
    tempo = 2,
    clips = 0,
    Command::RemoveTempo { tick: kani::any() }
);

apply_harness!(
    add_note_to_0_notes,
    tempo = 1,
    clips = 1,
    notes = 0,
    Command::AddNote {
        id: kani::any(),
        start: kani::any(),
        length: kani::any(),
        pitch: kani::any(),
        velocity: kani::any(),
    }
);
apply_harness!(
    add_note_to_1_note,
    tempo = 1,
    clips = 1,
    notes = 1,
    Command::AddNote {
        id: kani::any(),
        start: kani::any(),
        length: kani::any(),
        pitch: kani::any(),
        velocity: kani::any(),
    }
);
apply_harness!(
    add_note_to_2_notes,
    tempo = 1,
    clips = 1,
    notes = 2,
    Command::AddNote {
        id: kani::any(),
        start: kani::any(),
        length: kani::any(),
        pitch: kani::any(),
        velocity: kani::any(),
    }
);

apply_harness!(
    remove_note_of_1,
    tempo = 1,
    clips = 1,
    notes = 1,
    Command::RemoveNote {
        id: kani::any(),
        start: kani::any(),
        pitch: kani::any(),
    }
);
apply_harness!(
    remove_note_of_2,
    tempo = 1,
    clips = 1,
    notes = 2,
    Command::RemoveNote {
        id: kani::any(),
        start: kani::any(),
        pitch: kani::any(),
    }
);

/// The harnesses of each command. A new command does not compile until it is
/// given one here.
#[allow(dead_code)]
fn harnesses(command: Command) -> &'static [fn()] {
    match command {
        Command::AddTrack => &[add_track],
        Command::AddClip { .. } => &[add_clip_to_0_clips, add_clip_to_1_clip, add_clip_to_2_clips],
        Command::MoveClip { .. } => &[move_clip_of_1, move_clip_of_2],
        Command::SplitClip { .. } => &[
            split_clip_of_1,
            split_clip_of_2,
            split_clip_with_1_note,
            split_clip_with_2_notes,
        ],
        Command::DeleteClip { .. } => {
            &[delete_clip_of_1, delete_clip_of_2, delete_clip_with_1_note]
        }
        Command::SetTempo { .. } => &[set_tempo_with_1, set_tempo_with_2],
        Command::RemoveTempo { .. } => &[remove_tempo_with_2],
        Command::AddNote { .. } => &[add_note_to_0_notes, add_note_to_1_note, add_note_to_2_notes],
        Command::RemoveNote { .. } => &[remove_note_of_1, remove_note_of_2],
    }
}

/// `tick_to_micros` neither overflows nor panics for any tick. Whether it is
/// monotone is proved on the model (`tickToMicros_monotone`); comparing two
/// products is too hard for the solver.
#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(minisat)]
fn tick_to_micros_does_not_overflow() {
    let tempo: [TempoChange; 2] = kani::any();
    let project = Project {
        track_count: 0,
        tempo: tempo.to_vec(),
        clips: Vec::new(),
        notes: Vec::new(),
    };
    kani::assume(well_formed(&project, 0));
    let tick: Tick = kani::any();
    kani::cover!(
        tick > project.tempo[1].tick,
        "the tick lies past the second tempo change"
    );
    project.tick_to_micros(tick);
}
