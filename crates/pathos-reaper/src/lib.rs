//! The comparison of pathos with REAPER (ADR 7, ADR 33): a project converted
//! into a REAPER project, whose tracks play a JSFX copy of the built-in
//! instrument (ADR 15).

use std::fmt::Write as _;
use std::num::NonZeroU32;

use pathos_core::{CENTER_PAN, Clip, PPQ, Project, Tick, UNITY_VOLUME};
use pathos_render::FREQUENCIES;

/// The file name of the instrument that sounds like pathos's.
pub const INSTRUMENT: &str = "pathos-instrument.jsfx";

/// The file name of the instrument that outputs an impulse at each note-on and
/// note-off.
pub const IMPULSES: &str = "pathos-impulses.jsfx";

/// The directory, next to the project file, and the name of the rendered WAV
/// file, without its extension.
pub const RENDER_DIRECTORY: &str = "render";
pub const RENDER_NAME: &str = "render";

/// What the tracks of a converted project play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The sound of the project, as pathos renders it.
    Mix,
    /// An impulse of 1 on the left channel at each note-on and on the right at
    /// each note-off, with every gain at unity.
    Position,
}

/// The source of the instrument file `name`, one of [`INSTRUMENT`] and
/// [`IMPULSES`], with pathos's frequency table.
pub fn instrument(name: &str) -> String {
    let impulses = u8::from(name == IMPULSES);
    let mut table = String::new();
    for (pitch, frequency) in FREQUENCIES.iter().enumerate() {
        // Every f32 is exact as an f64, and Debug prints an f64 so that it
        // reads back to the same value.
        writeln!(table, "freq[{pitch}] = {:?};", f64::from(*frequency)).unwrap();
    }
    include_str!("instrument.jsfx")
        .replace("IMPULSES", &impulses.to_string())
        .replace("//FREQUENCIES\n", &table)
}

/// The exact time of `tick` in microseconds times [`PPQ`].
fn time(project: &Project, tick: Tick) -> u128 {
    let tempo = project.tempo();
    let mut time = 0;
    let mut current = tempo[0];
    for &change in &tempo[1..] {
        if change.tick > tick {
            break;
        }
        time += u128::from(change.tick - current.tick) * u128::from(current.us_per_quarter);
        current = change;
    }
    time + u128::from(tick - current.tick) * u128::from(current.us_per_quarter)
}

/// The seconds from tick `from` to tick `to`, rounded once.
fn seconds(project: &Project, from: Tick, to: Tick) -> f64 {
    (time(project, to) - time(project, from)) as f64 / (PPQ as f64 * 1e6)
}

fn bpm(us_per_quarter: u64) -> f64 {
    60e6 / us_per_quarter as f64
}

/// A REAPER project of `project` that renders its first `frames` frames at
/// `rate` into [`RENDER_DIRECTORY`]/[`RENDER_NAME`]`.wav`, next to the project
/// file, as 32-bit floats.
pub fn convert(project: &Project, rate: NonZeroU32, frames: u64, layer: Layer) -> String {
    let rate = rate.get();
    let length = frames as f64 / f64::from(rate);
    let mut out = String::new();
    // Rust writes a float in the fewest decimal digits that read back to it,
    // without an exponent.
    writeln!(out, "<REAPER_PROJECT 0.1 \"7.86\" 0").unwrap();
    writeln!(out, "  SAMPLERATE {rate} 1 0").unwrap();
    writeln!(
        out,
        "  TEMPO {} 4 4",
        bpm(project.tempo()[0].us_per_quarter)
    )
    .unwrap();
    // A pan of 0 dB at the center: a linear balance, as pathos pans (ADR 21).
    writeln!(out, "  PANLAW 1").unwrap();
    writeln!(out, "  RENDER_FILE \"{RENDER_DIRECTORY}\"").unwrap();
    writeln!(out, "  RENDER_PATTERN \"{RENDER_NAME}\"").unwrap();
    writeln!(out, "  RENDER_FMT 0 2 {rate}").unwrap();
    // A time range from 0 to `length`, with no tail.
    writeln!(out, "  RENDER_RANGE 0 0 {length} 0 0").unwrap();
    writeln!(out, "  RENDER_DITHER 0").unwrap();
    writeln!(out, "  RENDER_NORMALIZE 0 0 0").unwrap();
    writeln!(out, "  RENDER_STEMS 0").unwrap();
    // WAV of 32-bit floats.
    writeln!(out, "  <RENDER_CFG\n    ZXZhdyAAAQ==\n  >").unwrap();
    let master = match layer {
        Layer::Mix => f64::from(project.master_volume()) / f64::from(UNITY_VOLUME),
        Layer::Position => 1.0,
    };
    writeln!(out, "  MASTER_VOLUME {master} 0 -1 -1 1").unwrap();
    writeln!(out, "  <TEMPOENVEX\n    ACT 1 -1\n    DEFSHAPE 1 -1 -1").unwrap();
    for change in project.tempo() {
        let time = seconds(project, 0, change.tick);
        let bpm = bpm(change.us_per_quarter);
        // Shape 1 holds the tempo until the next point.
        writeln!(out, "    PT {time} {bpm} 1").unwrap();
    }
    writeln!(out, "  >").unwrap();
    let instrument = match layer {
        Layer::Mix => INSTRUMENT,
        Layer::Position => IMPULSES,
    };
    for index in 0..project.track_count() {
        let track = project.track(index).unwrap_or_default();
        let (volume, pan) = match layer {
            Layer::Mix => (
                f64::from(track.volume) / f64::from(UNITY_VOLUME),
                (f64::from(track.pan) - f64::from(CENTER_PAN)) / f64::from(CENTER_PAN),
            ),
            Layer::Position => (1.0, 0.0),
        };
        writeln!(out, "  <TRACK").unwrap();
        writeln!(out, "    VOLPAN {volume} {pan} -1 -1 1").unwrap();
        writeln!(
            out,
            "    MUTESOLO {} {} 0",
            u8::from(track.muted),
            u8::from(track.solo)
        )
        .unwrap();
        writeln!(
            out,
            "    <FXCHAIN\n      <JS {instrument} \"\"\n      >\n    >"
        )
        .unwrap();
        // REAPER plays a muted track that is soloed, which pathos does not
        // (ADR 21), so such a track keeps its solo but loses its notes.
        let silent = track.muted && track.solo;
        let clips: Vec<_> = project
            .clips()
            .iter()
            .filter(|clip| clip.track == index && !silent)
            .collect();
        if let Some(end) = clips.iter().map(|clip| clip.stop()).max() {
            item(&mut out, project, &clips, end);
        }
        writeln!(out, "  >").unwrap();
    }
    writeln!(out, ">").unwrap();
    out
}

/// One MIDI item from the start to `end` with the notes of `clips`. REAPER
/// puts an item's start at a whole sample and its notes from there, so an item
/// that started between samples could move its notes by a sample; from the
/// start, each note lies at its own nearest sample (ADR 32).
fn item(out: &mut String, project: &Project, clips: &[&Clip], end: Tick) {
    writeln!(out, "    <ITEM").unwrap();
    writeln!(out, "      POSITION 0").unwrap();
    writeln!(out, "      LENGTH {}", seconds(project, 0, end)).unwrap();
    writeln!(out, "      <SOURCE MIDI").unwrap();
    writeln!(out, "        HASDATA 1 {PPQ} QN").unwrap();
    // A note-off sorts before a note-on on the same tick.
    let mut events: Vec<(Tick, bool, u8, u8)> = Vec::new();
    for clip in clips {
        for note in project.notes(clip.id) {
            let start = clip.start + note.start;
            events.push((start, true, note.pitch, note.velocity));
            events.push((start + note.length, false, note.pitch, 0));
        }
    }
    events.sort_unstable();
    let mut at = 0;
    for (tick, on, pitch, velocity) in events {
        let status = if on { 0x90 } else { 0x80 };
        let delta = tick - at;
        writeln!(
            out,
            "        E {delta} {status:02x} {pitch:02x} {velocity:02x}"
        )
        .unwrap();
        at = tick;
    }
    // All notes off at the end of the item, as REAPER ends a MIDI item.
    writeln!(out, "        E {} b0 7b 00", end - at).unwrap();
    writeln!(out, "      >\n    >").unwrap();
}
