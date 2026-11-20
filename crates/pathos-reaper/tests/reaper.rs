//! The comparison with REAPER (ADR 7, ADR 33). Each scenario is rendered by
//! pathos and by REAPER, and must agree in the position and the mix layers.
//!
//! The tests run REAPER and are ignored by default. Run them in the dev shell
//! `.#reaper` with `cargo test -p pathos-reaper -- --ignored`. `PATHOS_REAPER`
//! is the command that runs REAPER, split at spaces; it defaults to `reaper`.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use pathos_core::{Command, Document, Edit, Event, Project};
use pathos_reaper::{IMPULSES, INSTRUMENT, Layer, RENDER_DIRECTORY, RENDER_NAME};
use pathos_render::Frame;
use proptest::prelude::*;

const RATES: [u32; 2] = [44_100, 48_000];

/// The largest difference of a sample in the mix layer: -80 dBFS.
const MIX_TOLERANCE: f32 = 1e-4;

/// A directory of its own for each render, removed when the render is read.
fn scratch_directory() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "pathos-reaper-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(directory.join("resources/Effects")).unwrap();
    directory
}

/// Held while REAPER runs. Two REAPERs rendering at once sometimes left a WAV
/// file cut short, and the license allows one computer at a time (ADR 34).
static REAPER: Mutex<()> = Mutex::new(());

/// The first `frames` frames of `project` at `rate`, rendered by REAPER.
fn render_with_reaper(project: &Project, rate: u32, frames: u64, layer: Layer) -> Vec<Frame> {
    let directory = scratch_directory();
    // REAPER keeps its settings and finds the instruments in this resource
    // directory, so a render does not touch the user's own.
    let resources = directory.join("resources");
    for name in [INSTRUMENT, IMPULSES] {
        std::fs::write(
            resources.join("Effects").join(name),
            pathos_reaper::instrument(name),
        )
        .unwrap();
    }
    let rpp = directory.join("project.rpp");
    let rate = NonZeroU32::new(rate).unwrap();
    std::fs::write(&rpp, pathos_reaper::convert(project, rate, frames, layer)).unwrap();

    let command = std::env::var("PATHOS_REAPER").unwrap_or_else(|_| "reaper".into());
    let mut words = command.split_whitespace();
    // A test that failed while holding the lock leaves nothing to clean up.
    let running = REAPER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let output = Process::new(words.next().expect("a command for REAPER"))
        .args(words)
        .arg("-cfgfile")
        .arg(resources.join("reaper.ini"))
        .args(["-newinst", "-nosplash", "-ignoreerrors", "-renderproject"])
        .arg(&rpp)
        .current_dir(&directory)
        .output()
        .unwrap_or_else(|error| panic!("cannot run {command:?}: {error}"));
    drop(running);
    assert!(
        output.status.success(),
        "REAPER failed with {}:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let wav = directory
        .join(RENDER_DIRECTORY)
        .join(format!("{RENDER_NAME}.wav"));
    let frames = read_wav(&wav).unwrap_or_else(|| {
        panic!(
            "REAPER rendered no {}:\n{}",
            wav.display(),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    std::fs::remove_dir_all(&directory).unwrap();
    frames
}

fn read_wav(path: &Path) -> Option<Vec<Frame>> {
    let mut reader = hound::WavReader::open(path).ok()?;
    let samples: Vec<f32> = reader.samples::<f32>().map(Result::unwrap).collect();
    Some(samples.as_chunks::<2>().0.to_vec())
}

/// Whole seconds that hold every event of the project at `rate`, plus one.
fn frames(project: &Project, rate: u32) -> u64 {
    let last = project
        .schedule(rate.into())
        .last()
        .map_or(0, |event| match *event {
            Event::On { sample, .. } | Event::Off { sample, .. } => sample,
        });
    (last as u64 / u64::from(rate) + 1) * u64::from(rate)
}

/// The note-ons on the left channel and the note-offs on the right of each
/// frame, from the project's schedule.
fn expected_impulses(project: &Project, rate: u32, frames: u64) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; frames as usize];
    for event in project.schedule(rate.into()) {
        let (sample, channel) = match event {
            Event::On { sample, .. } => (sample, 0),
            Event::Off { sample, .. } => (sample, 1),
        };
        out[sample as usize][channel] += 1.0;
    }
    out
}

/// Both layers agree for `project` at every rate.
fn compare(name: &str, project: &Project) {
    for rate in RATES {
        let frames = frames(project, rate);

        let expected = expected_impulses(project, rate, frames);
        let actual = render_with_reaper(project, rate, frames, Layer::Position);
        assert_eq!(actual.len(), expected.len(), "{name} at {rate} Hz: frames");
        let misplaced: Vec<_> = (0..expected.len())
            .filter(|&frame| actual[frame] != expected[frame])
            .take(10)
            .map(|frame| (frame, expected[frame], actual[frame]))
            .collect();
        assert!(
            misplaced.is_empty(),
            "{name} at {rate} Hz: events (frame, [ons, offs] expected, actual): {misplaced:?}"
        );

        let rate_nz = NonZeroU32::new(rate).unwrap();
        let expected = pathos_render::render(project, rate_nz, frames as usize);
        let actual = render_with_reaper(project, rate, frames, Layer::Mix);
        assert_eq!(actual.len(), expected.len(), "{name} at {rate} Hz: frames");
        let (frame, difference) = (0..expected.len())
            .map(|frame| {
                let [l, r] = expected[frame];
                let [al, ar] = actual[frame];
                (frame, (l - al).abs().max((r - ar).abs()))
            })
            .fold(
                (0, 0.0),
                |worst, next| if next.1 > worst.1 { next } else { worst },
            );
        assert!(
            difference <= MIX_TOLERANCE,
            "{name} at {rate} Hz: the mix differs by {difference} at frame {frame}: \
             {:?} expected, {:?} rendered",
            expected[frame],
            actual[frame]
        );
    }
}

#[test]
#[ignore = "runs REAPER; see the module documentation"]
fn scenarios_agree_with_reaper() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pathos")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "scenarios in {}", directory.display());
    for path in paths {
        let file = std::fs::read_to_string(&path).unwrap();
        let document =
            Document::load(&file).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let name = path.file_stem().unwrap().to_string_lossy();
        compare(&name, document.history().present());
    }
}

/// A command for a project of up to three tracks and a few seconds. Some are
/// rejected, such as a note outside its clip, and leave the project as it is.
fn command() -> impl Strategy<Value = Command> {
    let track = 0..3u64;
    let id = 0..6u64;
    let tick = 0..7_680u64;
    prop_oneof![
        1 => Just(Command::AddTrack),
        2 => (track.clone(), tick.clone(), 1..3_840u64)
            .prop_map(|(track, start, length)| Command::AddClip { track, start, length }),
        8 => (id.clone(), 0..3_840u64, 1..960u64, 36..96u64, 1..128u64).prop_map(
            |(id, start, length, pitch, velocity)| Command::AddNote {
                id,
                start,
                length,
                pitch,
                velocity,
            }
        ),
        1 => (id.clone(), track.clone(), tick.clone())
            .prop_map(|(id, track, start)| Command::MoveClip { id, track, start }),
        1 => (id.clone(), tick.clone()).prop_map(|(id, tick)| Command::SplitClip { id, tick }),
        1 => (tick.clone(), 250_000..1_000_000u64).prop_map(|(tick, us_per_quarter)| {
            Command::SetTempo {
                tick,
                us_per_quarter,
            }
        }),
        1 => (track.clone(), 0..=2_000u64)
            .prop_map(|(track, volume)| Command::SetTrackVolume { track, volume }),
        1 => (track.clone(), 0..=200u64).prop_map(|(track, pan)| Command::SetTrackPan { track, pan }),
        1 => (track.clone(), any::<bool>())
            .prop_map(|(track, muted)| Command::SetTrackMute { track, muted }),
        1 => (track, any::<bool>()).prop_map(|(track, solo)| Command::SetTrackSolo { track, solo }),
        1 => (0..=2_000u64).prop_map(|volume| Command::SetMasterVolume { volume }),
    ]
}

/// A project built from generated commands, starting with a track and a clip.
fn generated_project() -> impl Strategy<Value = Document> {
    prop::collection::vec(command(), 1..40).prop_map(|commands| {
        let mut document = Document::default();
        let start = [
            Command::AddTrack,
            Command::AddClip {
                track: 0,
                start: 0,
                length: 3_840,
            },
        ];
        for command in start.into_iter().chain(commands) {
            let _ = document.edit(Edit::Apply(command));
        }
        document
    })
}

/// Each case runs REAPER four times, so the default number of cases is small;
/// `PROPTEST_CASES` sets it.
fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(8);
    ProptestConfig::with_cases(cases)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    #[ignore = "runs REAPER; see the module documentation"]
    fn generated_projects_agree_with_reaper(document in generated_project()) {
        eprintln!("project:\n{}", document.save());
        compare("generated", document.history().present());
    }
}
