//! Differential tests against the Lean model in `spec/`.
//!
//! Each generated trace is applied to this crate and sent to the model's
//! executable. Every reply must be equal, including errors. Build the model
//! first with `lake -d spec build`, or point `PATHOS_MODEL` at the executable.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};

use pathos_core::{Command, Edit, EditError, History, MAX_TICK, MAX_US_PER_QUARTER};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

#[derive(Clone, Copy, Debug)]
enum Request {
    Edit(Edit),
    Micros(u64),
}

impl Request {
    /// The request in the model's trace format (`spec/Pathos/Trace.lean`).
    fn encode(&self) -> String {
        match *self {
            Request::Micros(tick) => format!("micros {tick}"),
            Request::Edit(Edit::Undo) => "undo".into(),
            Request::Edit(Edit::Redo) => "redo".into(),
            Request::Edit(Edit::Apply(command)) => match command {
                Command::AddTrack => "add-track".into(),
                Command::AddClip {
                    track,
                    start,
                    length,
                } => format!("add-clip {track} {start} {length}"),
                Command::MoveClip { id, track, start } => format!("move-clip {id} {track} {start}"),
                Command::SplitClip { id, tick } => format!("split-clip {id} {tick}"),
                Command::DeleteClip { id } => format!("delete-clip {id}"),
                Command::SetTempo {
                    tick,
                    us_per_quarter,
                } => format!("set-tempo {tick} {us_per_quarter}"),
                Command::RemoveTempo { tick } => format!("remove-tempo {tick}"),
            },
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
    Request::Edit(Edit::Undo) => "undo",
    Request::Edit(Edit::Redo) => "redo",
    Request::Micros(_) => "micros",
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
        .map(|clip| format!("{}:{}:{}:{}", clip.id, clip.track, clip.start, clip.length))
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

/// The replies of this crate to a trace, starting from an empty project.
fn core_replies(trace: &[Request]) -> Vec<String> {
    let mut history = History::default();
    trace
        .iter()
        .map(|request| match *request {
            Request::Micros(tick) => format!("micros {}", history.present().tick_to_micros(tick)),
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
    let input: String = trace
        .iter()
        .map(|request| request.encode() + "\n")
        .collect();
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

fn command() -> impl Strategy<Value = Command> {
    let track = 0u64..3;
    // Few clips exist in a trace, so small ids mostly name one, and the rest
    // are unknown.
    let id = 0u64..4;
    prop_oneof![
        3 => Just(Command::AddTrack),
        6 => (track.clone(), tick(), length())
            .prop_map(|(track, start, length)| Command::AddClip { track, start, length }),
        4 => (id.clone(), track, tick())
            .prop_map(|(id, track, start)| Command::MoveClip { id, track, start }),
        4 => (id.clone(), tick()).prop_map(|(id, tick)| Command::SplitClip { id, tick }),
        2 => id.prop_map(|id| Command::DeleteClip { id }),
        3 => (tick(), us_per_quarter())
            .prop_map(|(tick, us_per_quarter)| Command::SetTempo { tick, us_per_quarter }),
        3 => tick().prop_map(|tick| Command::RemoveTempo { tick }),
    ]
}

fn request() -> impl Strategy<Value = Request> {
    prop_oneof![
        12 => command().prop_map(|command| Request::Edit(Edit::Apply(command))),
        2 => Just(Request::Edit(Edit::Undo)),
        1 => Just(Request::Edit(Edit::Redo)),
        2 => tick().prop_map(Request::Micros),
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
    prop::collection::vec(request(), 1..80)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn core_agrees_with_model(trace in trace()) {
        prop_assert_eq!(core_replies(&trace), model_replies(&trace));
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
    for trace in traces {
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
