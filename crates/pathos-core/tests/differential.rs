//! Differential tests against the Lean model in `spec/`.
//!
//! Each generated trace is applied to this crate and sent to the model's
//! executable. Every reply must be equal, including errors. Build the model
//! first with `lake -d spec build`, or point `PATHOS_MODEL` at the executable.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

use pathos_core::{
    Command, Document, Edit, EditError, Event, History, MAX_PITCH, MAX_TICK, MAX_US_PER_QUARTER,
    MAX_VELOCITY,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

#[derive(Clone, Copy, Debug)]
enum Request {
    Edit(Edit),
    Micros(u64),
    Schedule(u64),
}

impl Request {
    /// The request in the model's trace format (`spec/Pathos/Trace.lean`).
    fn encode(&self) -> String {
        match *self {
            Request::Micros(tick) => format!("micros {tick}"),
            Request::Schedule(rate) => format!("schedule {rate}"),
            Request::Edit(edit) => edit.to_string(),
        }
    }
}

/// Declares `ERRORS` and `error_name` from one list, so that the exhaustive
/// match in `error_name` also keeps `ERRORS` complete.
macro_rules! errors {
    ($($variant:ident => $name:literal,)*) => {
        /// Every error, so that the generators are required to reach each one.
        const ERRORS: &[EditError] = &[$(EditError::$variant),*];

        fn error_name(error: EditError) -> &'static str {
            match error {
                $(EditError::$variant => $name,)*
            }
        }
    };
}

errors! {
    EmptyClip => "empty-clip",
    UnknownTrack => "unknown-track",
    UnknownClip => "unknown-clip",
    OutOfRange => "out-of-range",
    Overlap => "overlap",
    SplitOutsideClip => "split-outside-clip",
    InvalidTempo => "invalid-tempo",
    InitialTempo => "initial-tempo",
    NoTempoChange => "no-tempo-change",
    EmptyNote => "empty-note",
    InvalidNote => "invalid-note",
    NoteOutsideClip => "note-outside-clip",
    NoteOverlap => "note-overlap",
    UnknownNote => "unknown-note",
    NothingToUndo => "nothing-to-undo",
    NothingToRedo => "nothing-to-redo",
}

/// Each error explains itself to the user, which the model does not specify.
#[test]
fn every_error_has_a_message() {
    for &error in ERRORS {
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
}

/// Declares `REQUEST_KINDS` and `Request::kind` from one list, so that the
/// exhaustive match in `Request::kind` also keeps `REQUEST_KINDS` complete.
macro_rules! request_kinds {
    ($($pattern:pat => $kind:literal,)*) => {
        /// Every kind of request, so that the generators are required to
        /// reach each one.
        const REQUEST_KINDS: &[&str] = &[$($kind),*];

        impl Request {
            /// The first word of the request in the model's trace format.
            fn kind(&self) -> &'static str {
                match self {
                    $($pattern => $kind,)*
                }
            }
        }
    };
}

request_kinds! {
    Request::Edit(Edit::Apply(Command::AddTrack)) => "add-track",
    Request::Edit(Edit::Apply(Command::AddClip { .. })) => "add-clip",
    Request::Edit(Edit::Apply(Command::MoveClip { .. })) => "move-clip",
    Request::Edit(Edit::Apply(Command::SplitClip { .. })) => "split-clip",
    Request::Edit(Edit::Apply(Command::DeleteClip { .. })) => "delete-clip",
    Request::Edit(Edit::Apply(Command::SetTempo { .. })) => "set-tempo",
    Request::Edit(Edit::Apply(Command::RemoveTempo { .. })) => "remove-tempo",
    Request::Edit(Edit::Apply(Command::AddNote { .. })) => "add-note",
    Request::Edit(Edit::Apply(Command::RemoveNote { .. })) => "remove-note",
    Request::Edit(Edit::Undo) => "undo",
    Request::Edit(Edit::Redo) => "redo",
    Request::Micros(_) => "micros",
    Request::Schedule(_) => "schedule",
}

fn render(history: &History) -> String {
    let project = history.present();
    let tempo: Vec<String> = project
        .tempo()
        .iter()
        .map(|change| format!("{}:{}", change.tick, change.us_per_quarter))
        .collect();
    let clips: Vec<String> = project
        .clips()
        .iter()
        .map(|clip| {
            let notes: Vec<String> = project
                .notes(clip.id)
                .map(|note| {
                    format!(
                        "{}/{}/{}/{}",
                        note.start, note.length, note.pitch, note.velocity
                    )
                })
                .collect();
            format!(
                "{}:{}:{}:{}[{}]",
                clip.id,
                clip.track,
                clip.start,
                clip.length,
                notes.join(";")
            )
        })
        .collect();
    format!(
        "tracks={} next={} tempo={} clips={} undo={} redo={}",
        project.track_count(),
        history.next_clip_id(),
        tempo.join(","),
        clips.join(","),
        history.undo_depth(),
        history.redo_depth(),
    )
}

fn render_event(event: &Event) -> String {
    match *event {
        Event::Off {
            sample,
            track,
            pitch,
        } => format!("off:{sample}:{track}:{pitch}"),
        Event::On {
            sample,
            track,
            pitch,
            velocity,
        } => format!("on:{sample}:{track}:{pitch}:{velocity}"),
    }
}

/// The replies of this crate to a trace, starting from an empty project.
fn core_replies(trace: &[Request]) -> Vec<String> {
    let mut history = History::default();
    trace
        .iter()
        .map(|request| match *request {
            Request::Micros(tick) => format!("micros {}", history.present().tick_to_micros(tick)),
            Request::Schedule(rate) => {
                let events: Vec<String> = history
                    .present()
                    .schedule(rate)
                    .iter()
                    .map(render_event)
                    .collect();
                format!("schedule {}", events.join(","))
            }
            Request::Edit(edit) => match history.edit(edit) {
                Ok(()) => format!("ok {}", render(&history)),
                Err(error) => format!("err {}", error_name(error)),
            },
        })
        .collect()
}

fn model_path() -> PathBuf {
    match std::env::var_os("PATHOS_MODEL") {
        Some(path) => path.into(),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/.lake/build/bin/pathos-model"),
    }
}

/// The replies of the Lean model to a trace.
fn model_replies(trace: &[Request]) -> Vec<String> {
    let input: String = trace
        .iter()
        .map(|request| request.encode() + "\n")
        .collect();
    model_replies_to(&input)
}

/// The replies of the Lean model to lines in its trace format.
fn model_replies_to(input: &str) -> Vec<String> {
    let path = model_path();
    let mut model = Process::new(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| {
            panic!(
                "cannot run the model at {}: {error}; build it with `lake -d spec build`",
                path.display()
            )
        });
    model
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = model.wait_with_output().unwrap();
    assert!(output.status.success(), "the model rejected the trace");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The grid most ticks and lengths fall on, so that clips often touch, share
/// a start or end exactly where a tempo changes.
const GRID: u64 = 240;

/// Mostly small ticks so that clips and tempo changes meet, with the limits
/// of the supported range mixed in.
fn tick() -> impl Strategy<Value = u64> {
    prop_oneof![
        8 => (0u64..16).prop_map(|step| step * GRID),
        4 => 0u64..4000,
        1 => (0u64..4).prop_map(|step| MAX_TICK - step * GRID),
        1 => prop_oneof![Just(MAX_TICK - 1), Just(MAX_TICK + 1), Just(u64::MAX)],
    ]
}

fn length() -> impl Strategy<Value = u64> {
    prop_oneof![
        8 => (1u64..8).prop_map(|step| step * GRID),
        4 => 1u64..2000,
        1 => Just(0),
        1 => prop_oneof![Just(MAX_TICK), Just(u64::MAX)],
    ]
}

fn us_per_quarter() -> impl Strategy<Value = u64> {
    prop_oneof![
        12 => 1u64..2_000_000,
        1 => prop_oneof![
            Just(0),
            Just(MAX_US_PER_QUARTER),
            Just(MAX_US_PER_QUARTER + 1),
            Just(u64::MAX),
        ],
    ]
}

/// A start within a clip, mostly on the grid so that notes meet, end where a
/// clip ends or start where a split falls.
fn note_start() -> impl Strategy<Value = u64> {
    prop_oneof![
        8 => (0u64..3).prop_map(|step| step * GRID),
        2 => 0u64..2000,
        1 => Just(u64::MAX),
    ]
}

fn note_length() -> impl Strategy<Value = u64> {
    prop_oneof![
        8 => (1u64..3).prop_map(|step| step * GRID),
        2 => 1u64..1000,
        2 => prop_oneof![Just(0), Just(u64::MAX)],
    ]
}

/// Mostly a few pitches, so that notes of one pitch overlap and a removal
/// names an existing note.
fn pitch() -> impl Strategy<Value = u64> {
    let max = u64::from(MAX_PITCH);
    prop_oneof![
        12 => 60u64..62,
        1 => 0..=max,
        2 => prop_oneof![Just(max), Just(max + 1), Just(256), Just(u64::MAX)],
    ]
}

fn velocity() -> impl Strategy<Value = u64> {
    let max = u64::from(MAX_VELOCITY);
    prop_oneof![
        12 => 1..=max,
        2 => prop_oneof![Just(0), Just(max), Just(max + 1), Just(256), Just(u64::MAX)],
    ]
}

fn command() -> impl Strategy<Value = Command> {
    let track = 0u64..3;
    // Few clips exist in a trace, so small ids mostly name one, and the rest
    // are unknown.
    let id = 0u64..4;
    // Notes need a clip, and the first clips of a trace are the most likely to
    // exist.
    let note_clip = 0u64..2;
    prop_oneof![
        5 => Just(Command::AddTrack),
        6 => (track.clone(), tick(), length())
            .prop_map(|(track, start, length)| Command::AddClip { track, start, length }),
        4 => (id.clone(), track, tick())
            .prop_map(|(id, track, start)| Command::MoveClip { id, track, start }),
        4 => (id.clone(), tick()).prop_map(|(id, tick)| Command::SplitClip { id, tick }),
        2 => id.prop_map(|id| Command::DeleteClip { id }),
        3 => (tick(), us_per_quarter())
            .prop_map(|(tick, us_per_quarter)| Command::SetTempo { tick, us_per_quarter }),
        3 => tick().prop_map(|tick| Command::RemoveTempo { tick }),
        3 => (note_clip.clone(), note_start(), note_length(), pitch(), velocity()).prop_map(
            |(id, start, length, pitch, velocity)| Command::AddNote {
                id,
                start,
                length,
                pitch,
                velocity,
            }
        ),
        2 => (note_clip, note_start(), pitch())
            .prop_map(|(id, start, pitch)| Command::RemoveNote { id, start, pitch }),
    ]
}

/// A note followed by a request about the same note: its removal, or a note of
/// the same pitch starting during it. A state-free generator rarely names an
/// existing note, so this reaches those rules whenever the first note is added.
fn note_pair() -> impl Strategy<Value = Vec<Request>> {
    (
        0u64..2,
        note_start(),
        note_length(),
        pitch(),
        velocity(),
        0u64..GRID,
    )
        .prop_flat_map(|(id, start, length, pitch, velocity, offset)| {
            let add = |start, length| {
                Request::Edit(Edit::Apply(Command::AddNote {
                    id,
                    start,
                    length,
                    pitch,
                    velocity,
                }))
            };
            let first = add(start, length);
            let remove = Request::Edit(Edit::Apply(Command::RemoveNote { id, start, pitch }));
            let overlapping = add(start.saturating_add(offset), GRID);
            prop_oneof![Just(vec![first, remove]), Just(vec![first, overlapping])]
        })
}

/// Common rates, and low ones at which notes share a sample or fall within
/// one, with the limits of the range.
fn sample_rate() -> impl Strategy<Value = u64> {
    prop_oneof![
        4 => prop_oneof![Just(44_100), Just(48_000), Just(96_000)],
        4 => 1u64..2000,
        1 => prop_oneof![Just(0), Just(u64::MAX)],
    ]
}

/// A few notes added to one clip, then the schedule. Most schedules of a
/// trace are otherwise empty, which leaves the order of events barely tested.
fn notes_then_schedule() -> impl Strategy<Value = Vec<Request>> {
    let note = (note_start(), note_length(), pitch(), velocity());
    (0u64..2, prop::collection::vec(note, 1..6), sample_rate()).prop_map(|(id, notes, rate)| {
        let mut requests: Vec<Request> = notes
            .into_iter()
            .map(|(start, length, pitch, velocity)| {
                Request::Edit(Edit::Apply(Command::AddNote {
                    id,
                    start,
                    length,
                    pitch,
                    velocity,
                }))
            })
            .collect();
        requests.push(Request::Schedule(rate));
        requests
    })
}

fn request() -> impl Strategy<Value = Request> {
    prop_oneof![
        12 => command().prop_map(|command| Request::Edit(Edit::Apply(command))),
        2 => Just(Request::Edit(Edit::Undo)),
        1 => Just(Request::Edit(Edit::Redo)),
        2 => tick().prop_map(Request::Micros),
        1 => sample_rate().prop_map(Request::Schedule),
    ]
}

/// The number of traces to try: `PROPTEST_CASES`, or enough by default to
/// reach the rarer boundaries, such as a split exactly at a clip's start.
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(2000)
}

fn trace() -> impl Strategy<Value = Vec<Request>> {
    let chunk = prop_oneof![
        12 => request().prop_map(|request| vec![request]),
        1 => note_pair(),
        1 => notes_then_schedule(),
    ];
    prop::collection::vec(chunk, 1..100).prop_map(|chunks| chunks.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn core_agrees_with_model(trace in trace()) {
        prop_assert_eq!(core_replies(&trace), model_replies(&trace));
    }

    /// The project file of the edits the core accepts loads into the same
    /// history in the core, and the model accepts every line of it and ends in
    /// the same state (ADR 19).
    #[test]
    fn a_saved_document_loads_the_same_in_the_core_and_the_model(trace in trace()) {
        let mut document = Document::default();
        for request in &trace {
            if let Request::Edit(edit) = *request {
                let _ = document.edit(edit);
            }
        }
        let file = document.save();
        prop_assert_eq!(&Document::load(&file).unwrap(), &document);
        let replies = model_replies_to(&file);
        prop_assert_eq!(replies.len(), file.lines().count());
        prop_assert!(replies.iter().all(|reply| reply.starts_with("ok ")), "{:?}", replies);
        if let Some(last) = replies.last() {
            prop_assert_eq!(last, &format!("ok {}", render(document.history())));
        }
    }
}

/// Traces that sit exactly on the boundary of a rule, where an off-by-one in
/// this crate would otherwise depend on the generators to be noticed.
#[test]
fn core_agrees_with_model_on_boundaries() {
    use Command::*;
    let apply = |command| Request::Edit(Edit::Apply(command));
    let add_clip = |start, length| {
        apply(AddClip {
            track: 0,
            start,
            length,
        })
    };
    let split = |id, tick| apply(SplitClip { id, tick });
    let set_tempo = |tick, us_per_quarter| {
        apply(SetTempo {
            tick,
            us_per_quarter,
        })
    };
    let traces = [
        // Clips that touch do not overlap; sharing one tick does.
        vec![
            apply(AddTrack),
            add_clip(240, 240),
            add_clip(480, 240),
            add_clip(0, 240),
            add_clip(239, 2),
            add_clip(719, 1),
        ],
        // A split point must be strictly inside the clip.
        vec![
            apply(AddTrack),
            add_clip(240, 240),
            split(0, 240),
            split(0, 480),
            split(0, 241),
            split(1, 479),
        ],
        // A clip may end at the last tick but not beyond it.
        vec![
            apply(AddTrack),
            add_clip(MAX_TICK - 240, 240),
            add_clip(MAX_TICK - 239, 240),
            add_clip(u64::MAX, 1),
            apply(MoveClip {
                id: 0,
                track: 0,
                start: MAX_TICK - 239,
            }),
            apply(MoveClip {
                id: 0,
                track: 0,
                start: MAX_TICK - 240,
            }),
        ],
        // The limits of tempo values and positions.
        vec![
            set_tempo(0, MAX_US_PER_QUARTER),
            set_tempo(0, MAX_US_PER_QUARTER + 1),
            set_tempo(0, 0),
            set_tempo(MAX_TICK, 1),
            set_tempo(MAX_TICK + 1, 1),
            apply(RemoveTempo { tick: 0 }),
            apply(RemoveTempo { tick: MAX_TICK }),
            apply(RemoveTempo { tick: MAX_TICK }),
        ],
        // Time across tempo changes, at and around each change.
        vec![
            set_tempo(960, 250_000),
            set_tempo(1920, 1),
            Request::Micros(0),
            Request::Micros(959),
            Request::Micros(960),
            Request::Micros(961),
            Request::Micros(1920),
            Request::Micros(1921),
            Request::Micros(2879),
            Request::Micros(2880),
            Request::Micros(u64::MAX),
        ],
        // Ids are not reused, and an accepted command clears redo.
        vec![
            apply(AddTrack),
            add_clip(0, 240),
            add_clip(240, 240),
            apply(DeleteClip { id: 1 }),
            add_clip(240, 240),
            Request::Edit(Edit::Undo),
            Request::Edit(Edit::Undo),
            Request::Edit(Edit::Redo),
            apply(AddTrack),
            Request::Edit(Edit::Redo),
        ],
        // Undo does not reissue an id (ADR 10).
        vec![
            apply(AddTrack),
            add_clip(0, 240),
            Request::Edit(Edit::Undo),
            add_clip(480, 240),
            split(1, 600),
            Request::Edit(Edit::Undo),
            Request::Edit(Edit::Redo),
            split(1, 540),
        ],
        // Which error is reported when several apply.
        vec![
            add_clip(0, 0),
            add_clip(u64::MAX, 1),
            apply(AddTrack),
            add_clip(u64::MAX, 0),
            apply(MoveClip {
                id: 0,
                track: 1,
                start: u64::MAX,
            }),
            Request::Edit(Edit::Undo),
            Request::Edit(Edit::Undo),
            Request::Edit(Edit::Redo),
            Request::Edit(Edit::Redo),
        ],
    ];
    let add_note = |start, length, pitch, velocity| {
        apply(AddNote {
            id: 0,
            start,
            length,
            pitch,
            velocity,
        })
    };
    let remove_note = |id, start, pitch| apply(RemoveNote { id, start, pitch });
    let max_pitch = u64::from(MAX_PITCH);
    let max_velocity = u64::from(MAX_VELOCITY);
    let note_traces = [
        // A note may end at the end of its clip but not beyond it, and notes
        // of one pitch may touch but not share a tick.
        vec![
            apply(AddTrack),
            add_clip(0, 480),
            add_note(240, 240, 60, 100),
            add_note(241, 240, 61, 100),
            add_note(0, 240, 60, 100),
            add_note(239, 2, 60, 100),
            add_note(239, 2, 62, 100),
            add_note(0, u64::MAX, 63, 100),
            add_note(u64::MAX, 1, 63, 100),
        ],
        // The limits of pitch and velocity.
        vec![
            apply(AddTrack),
            add_clip(0, 480),
            add_note(0, 1, max_pitch, max_velocity),
            add_note(0, 1, max_pitch + 1, 1),
            add_note(0, 1, 0, 1),
            add_note(1, 1, 0, 0),
            add_note(1, 1, 0, max_velocity + 1),
            add_note(1, 1, 256 + 60, 1),
            add_note(1, 1, 60, 256 + 1),
        ],
        // A split gives each note to the clip it starts in, and shortens a
        // note that crosses it (ADR 14).
        vec![
            apply(AddTrack),
            add_clip(240, 960),
            add_note(0, 240, 60, 100),
            add_note(240, 240, 61, 100),
            add_note(241, 480, 62, 100),
            add_note(480, 1, 63, 100),
            split(0, 480),
            split(0, 241),
            split(1, 721),
            remove_note(1, 0, 61),
            remove_note(3, 0, 62),
            remove_note(1, 240, 63),
        ],
        // Moving a clip keeps its notes, and undo restores a removed one.
        vec![
            apply(AddTrack),
            add_clip(0, 480),
            add_note(0, 240, 60, 100),
            apply(MoveClip {
                id: 0,
                track: 0,
                start: 960,
            }),
            remove_note(0, 0, 61),
            remove_note(0, 1, 60),
            remove_note(0, 0, 60),
            Request::Edit(Edit::Undo),
            apply(DeleteClip { id: 0 }),
            Request::Edit(Edit::Undo),
        ],
        // Which note error is reported when several apply.
        vec![
            add_note(0, 0, u64::MAX, 0),
            apply(AddTrack),
            add_clip(0, 240),
            add_note(u64::MAX, 0, u64::MAX, 0),
            add_note(u64::MAX, 1, u64::MAX, 0),
            add_note(u64::MAX, 1, 60, 100),
            add_note(0, 240, 60, 100),
            add_note(0, 240, 60, 100),
            remove_note(1, 0, 60),
        ],
    ];
    let add_note_to = |id, start, length, pitch| {
        apply(AddNote {
            id,
            start,
            length,
            pitch,
            velocity: 100,
        })
    };
    let schedule_traces = [
        // A note-off comes before a note-on at the same sample, a note within
        // one sample has no events, and no rate is too low or too high.
        vec![
            apply(AddTrack),
            add_clip(0, 960),
            add_note(0, 480, 60, 100),
            add_note(480, 480, 60, 90),
            add_note(0, 1, 62, 1),
            Request::Schedule(48_000),
            Request::Schedule(1),
            Request::Schedule(2),
            Request::Schedule(1920),
            Request::Schedule(0),
            Request::Schedule(u64::MAX),
        ],
        // Events at one sample are ordered by track, then pitch, and a tempo
        // change during a note moves only its note-off.
        vec![
            apply(AddTrack),
            apply(AddTrack),
            apply(AddClip {
                track: 1,
                start: 0,
                length: 960,
            }),
            add_clip(0, 960),
            add_note_to(0, 0, 960, 64),
            add_note_to(0, 0, 960, 60),
            add_note_to(1, 0, 480, 67),
            Request::Schedule(48_000),
            set_tempo(480, 1_000_000),
            Request::Schedule(48_000),
        ],
        // The last tick at the highest rate.
        vec![
            apply(AddTrack),
            add_clip(MAX_TICK - 960, 960),
            add_note(0, 960, 60, 100),
            set_tempo(0, MAX_US_PER_QUARTER),
            Request::Schedule(u64::MAX),
        ],
    ];
    for trace in traces.into_iter().chain(note_traces).chain(schedule_traces) {
        assert_eq!(core_replies(&trace), model_replies(&trace), "{trace:?}");
    }
}

/// The number of traces sampled to measure how often the generators reach each
/// rule, fixed so that the measurement does not depend on `PROPTEST_CASES`.
const COVERAGE_TRACES: u32 = 2000;

/// Every error, and success for every kind of request, must appear at least
/// this often in the sampled traces, or the differential tests barely check
/// that rule.
const MIN_HITS: u32 = 100;

/// How often the generated traces reach each rule. The traces come from a
/// fixed seed, so the counts change only when the generators or the rules do.
/// Run with `--nocapture` to print the counts as a Markdown table.
#[test]
fn generators_reach_every_rule() {
    let mut runner = TestRunner::deterministic();
    let mut hits: BTreeMap<(String, String), u32> = BTreeMap::new();
    for _ in 0..COVERAGE_TRACES {
        let trace = trace().new_tree(&mut runner).unwrap().current();
        for (request, reply) in trace.iter().zip(core_replies(&trace)) {
            let outcome = reply.strip_prefix("err ").unwrap_or("ok");
            *hits
                .entry((request.kind().into(), outcome.into()))
                .or_default() += 1;
        }
    }

    println!("| request | outcome | hits |");
    println!("| --- | --- | ---: |");
    for ((kind, outcome), count) in &hits {
        println!("| {kind} | {outcome} | {count} |");
    }

    let total = |matches: &dyn Fn(&str, &str) -> bool| -> u32 {
        hits.iter()
            .filter(|((kind, outcome), _)| matches(kind, outcome))
            .map(|(_, count)| count)
            .sum()
    };
    let errors = ERRORS.iter().map(|&error| {
        let name = error_name(error);
        let count = total(&|_, outcome| outcome == name);
        (name.to_owned(), count)
    });
    let successes = REQUEST_KINDS.iter().map(|&kind| {
        let count = total(&|k, outcome| k == kind && outcome == "ok");
        (format!("{kind} ok"), count)
    });
    let rare: Vec<_> = errors
        .chain(successes)
        .filter(|(_, count)| *count < MIN_HITS)
        .collect();
    assert!(
        rare.is_empty(),
        "rules reached fewer than {MIN_HITS} times in {COVERAGE_TRACES} traces: {rare:?}"
    );
}
