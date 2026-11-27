//! The conversion into a REAPER project, checked without REAPER.

use std::num::NonZeroU32;

use pathos_core::{Command, History, Project};
use pathos_reaper::{IMPULSES, INSTRUMENT, Layer, convert, instrument};

fn project(commands: &[Command]) -> History {
    let mut history = History::default();
    for &command in commands {
        history
            .edit(pathos_core::Edit::Apply(command))
            .expect("the command is accepted");
    }
    history
}

const RATE: NonZeroU32 = NonZeroU32::new(48_000).unwrap();

fn lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).collect()
}

/// A track with a clip of two notes, at two tempos, with its mix.
fn mixed() -> History {
    project(&[
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 960,
            length: 1920,
        },
        Command::AddNote {
            id: 0,
            start: 0,
            length: 480,
            pitch: 69,
            velocity: 127,
        },
        Command::AddNote {
            id: 0,
            start: 480,
            length: 960,
            pitch: 69,
            velocity: 64,
        },
        Command::SetTrackVolume {
            track: 0,
            volume: 500,
        },
        Command::SetTrackPan { track: 0, pan: 150 },
        Command::SetTrackMute {
            track: 0,
            muted: true,
        },
        Command::SetMasterVolume { volume: 1250 },
        Command::SetTempo {
            tick: 1920,
            us_per_quarter: 400_000,
        },
    ])
}

#[test]
fn the_mix_takes_the_volumes_pans_mutes_and_tempo() {
    let rpp = convert(mixed().present(), RATE, 96_000, Layer::Mix);
    let lines = lines(&rpp);
    for expected in [
        "SAMPLERATE 48000 1 0",
        "TEMPO 120 4 4",
        "RENDER_RANGE 0 0 2 0 0",
        "MASTER_VOLUME 1.25 0 -1 -1 1",
        "PT 0 120 1",
        // Tick 1920 lies one second in, at 120 BPM.
        "PT 1 150 1",
        "VOLPAN 0.5 0.5 -1 -1 1",
        "MUTESOLO 1 0 0",
        "<JS pathos-instrument.jsfx \"\"",
        "POSITION 0",
        // Two quarters at 120 BPM, then one at 150 BPM.
        "LENGTH 1.4",
        "HASDATA 1 960 QN",
    ] {
        assert!(lines.contains(&expected), "{expected:?} in\n{rpp}");
    }
}

/// A note-off comes before a note-on on the same tick, each event a delta from
/// the one before, from the start of the project, and the item ends with all
/// notes off at the end of the last clip.
#[test]
fn notes_become_events_in_order() {
    let rpp = convert(mixed().present(), RATE, 96_000, Layer::Mix);
    let events: Vec<&str> = lines(&rpp)
        .into_iter()
        .filter(|line| line.starts_with("E "))
        .collect();
    assert_eq!(
        events,
        [
            "E 960 90 45 7f",
            "E 480 80 45 00",
            "E 0 90 45 40",
            "E 960 80 45 00",
            "E 480 b0 7b 00",
        ]
    );
}

/// The position layer keeps mutes and solos, which decide what is heard, but
/// puts every gain at unity and plays impulses.
#[test]
fn the_position_layer_plays_impulses_at_unity() {
    let rpp = convert(mixed().present(), RATE, 96_000, Layer::Position);
    let lines = lines(&rpp);
    for expected in [
        "MASTER_VOLUME 1 0 -1 -1 1",
        "VOLPAN 1 0 -1 -1 1",
        "MUTESOLO 1 0 0",
        "<JS pathos-impulses.jsfx \"\"",
    ] {
        assert!(lines.contains(&expected), "{expected:?} in\n{rpp}");
    }
}

/// The clips of a track become one item from the start of the project.
#[test]
fn each_track_gets_one_item_of_its_clips() {
    let history = project(&[
        Command::AddTrack,
        Command::AddTrack,
        Command::AddClip {
            track: 1,
            start: 0,
            length: 960,
        },
        Command::AddClip {
            track: 1,
            start: 960,
            length: 960,
        },
    ]);
    let rpp = convert(history.present(), RATE, 48_000, Layer::Mix);
    let tracks: Vec<&str> = rpp.split("<TRACK").skip(1).collect();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].matches("<ITEM").count(), 0);
    assert_eq!(tracks[1].matches("<ITEM").count(), 1);
    assert!(tracks[1].contains("LENGTH 1\n"), "{}", tracks[1]);
}

/// REAPER plays a muted track that is soloed, so it keeps its solo, which
/// silences the other tracks, but loses its notes (ADR 21).
#[test]
fn a_muted_solo_has_no_notes() {
    let history = project(&[
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
        Command::AddNote {
            id: 0,
            start: 0,
            length: 480,
            pitch: 60,
            velocity: 100,
        },
        Command::SetTrackSolo {
            track: 0,
            solo: true,
        },
        Command::SetTrackMute {
            track: 0,
            muted: true,
        },
    ]);
    let rpp = convert(history.present(), RATE, 48_000, Layer::Mix);
    assert!(lines(&rpp).contains(&"MUTESOLO 1 1 0"), "{rpp}");
    assert!(!rpp.contains("<ITEM"), "{rpp}");
}

/// The instruments carry pathos's frequency table, exactly, and only the
/// impulse one outputs impulses.
#[test]
fn the_instruments_carry_the_frequency_table() {
    for (name, impulses) in [(INSTRUMENT, "impulses = 0;"), (IMPULSES, "impulses = 1;")] {
        let source = instrument(name);
        assert!(source.contains(impulses), "{impulses} in {name}");
        assert!(source.contains("freq[69] = 440.0;"));
        let a0: f64 = source
            .lines()
            .find_map(|line| line.strip_prefix("freq[21] = "))
            .and_then(|value| value.strip_suffix(';'))
            .and_then(|value| value.parse().ok())
            .expect("a frequency for pitch 21");
        assert_eq!(a0, f64::from(pathos_render::FREQUENCIES[21]));
        assert!(!source.contains("//FREQUENCIES"));
    }
}

/// Every scenario of the comparison is a project file that loads.
#[test]
fn every_scenario_loads() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");
    let mut count = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "pathos")
        {
            let file = std::fs::read_to_string(&path).unwrap();
            if let Err(error) = pathos_core::Document::load(&file) {
                panic!("{}: {error}", path.display());
            }
            count += 1;
        }
    }
    assert!(count > 0, "scenarios in {}", directory.display());
}

/// An empty project converts into a valid REAPER project at 120 BPM with unity
/// master gain and no tracks or items.
#[test]
fn an_empty_project_converts_without_tracks() {
    let empty = Project::default();
    for layer in [Layer::Mix, Layer::Position] {
        let rpp = convert(&empty, RATE, 48_000, layer);
        let lines = lines(&rpp);
        for expected in [
            "<REAPER_PROJECT 0.1 \"7.86\" 0",
            "SAMPLERATE 48000 1 0",
            "TEMPO 120 4 4",
            "RENDER_RANGE 0 0 1 0 0",
            "MASTER_VOLUME 1 0 -1 -1 1",
            "PT 0 120 1",
        ] {
            assert!(lines.contains(&expected), "{expected:?} in\n{rpp}");
        }
        assert!(!rpp.contains("<TRACK"), "{rpp}");
        assert!(!rpp.contains("<ITEM"), "{rpp}");
        assert!(
            !lines.iter().any(|line| line.starts_with("E ")),
            "no MIDI events in\n{rpp}"
        );
        assert_eq!(lines.last().copied(), Some(">"));
    }

    let zero_length = convert(&empty, RATE, 0, Layer::Mix);
    assert!(
        lines(&zero_length).contains(&"RENDER_RANGE 0 0 0 0 0"),
        "{zero_length}"
    );
}
