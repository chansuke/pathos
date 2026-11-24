//! Measurements of a project's render for a review (ADR 6, ADR 36): loudness,
//! true peak, octave bands and phase correlation, per track and on the
//! master, over the whole song and bar by bar.

mod signal;

use std::num::NonZeroU32;
use std::ops::Range;

use pathos_core::{Command, Event, PPQ, Project, Tick};
use pathos_render::Frame;
use serde::{Deserialize, Serialize};

pub use signal::BANDS;

/// The rate at which a project is rendered to be measured.
pub const SAMPLE_RATE: u32 = 48000;

/// The longest song that is measured, in seconds.
pub const MAX_SECONDS: u64 = 600;

/// The beats in a bar, as the window and the metronome count them.
const BEATS_PER_BAR: u64 = 4;

/// The measurements of a project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub sample_rate: u32,
    /// The length of the song: up to the end of its last clip or its last
    /// note, whichever is later.
    pub seconds: f64,
    pub master: Measurements,
    /// Each track as it sounds alone, soloed and unmuted, at its volume and
    /// pan.
    pub tracks: Vec<TrackMeasurements>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackMeasurements {
    pub track: u64,
    pub muted: bool,
    #[serde(flatten)]
    pub measurements: Measurements,
}

/// Levels are in decibels rounded to hundredths, and absent for silence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurements {
    /// The integrated loudness of ITU-R BS.1770, in LUFS.
    pub loudness: Option<f64>,
    /// The true peak, in dBTP.
    pub true_peak: Option<f64>,
    /// The phase correlation of the left and the right channel, from -1 to 1.
    pub correlation: Option<f64>,
    /// The level of each octave band, in dBFS.
    pub bands: Vec<BandLevel>,
    pub bars: Vec<BarMeasurements>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandLevel {
    pub hz: f64,
    pub level: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarMeasurements {
    /// Counted from 1.
    pub bar: u64,
    pub start_tick: Tick,
    /// The loudness over the bar, ungated, in LUFS.
    pub loudness: Option<f64>,
    pub true_peak: Option<f64>,
    pub correlation: Option<f64>,
}

/// A song longer than [`MAX_SECONDS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooLong {
    pub seconds: f64,
}

impl std::fmt::Display for TooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the song lasts {:.0} seconds, more than the {MAX_SECONDS} that are measured",
            self.seconds
        )
    }
}

impl std::error::Error for TooLong {}

/// Renders `project` at [`SAMPLE_RATE`] and measures the master and each
/// track.
pub fn measure(project: &Project) -> Result<Report, TooLong> {
    const RATE: NonZeroU32 = match NonZeroU32::new(SAMPLE_RATE) {
        Some(rate) => rate,
        None => panic!("SAMPLE_RATE is not zero"),
    };
    let frames = song_frames(project);
    let seconds = frames as f64 / f64::from(SAMPLE_RATE);
    if frames > MAX_SECONDS * u64::from(SAMPLE_RATE) {
        return Err(TooLong { seconds });
    }
    let frames = frames as usize;
    let bars = bars(project, frames);
    let measured =
        |project: &Project| measurements(&pathos_render::render(project, RATE, frames), &bars);
    let tracks = (0..project.track_count())
        .map(|track| TrackMeasurements {
            track,
            muted: project.track(track).is_some_and(|settings| settings.muted),
            measurements: measured(&alone(project, track)),
        })
        .collect();
    Ok(Report {
        sample_rate: SAMPLE_RATE,
        seconds: round(seconds),
        master: measured(project),
        tracks,
    })
}

/// `project` with only `track` heard: soloed, unmuted, and every other track
/// unsoloed, by the commands any edit uses.
fn alone(project: &Project, track: u64) -> Project {
    let mut alone = project.clone();
    let mut apply = |command| {
        // These commands name an existing track with a valid setting, so the
        // core accepts them; no clip is created, so the next id is unused.
        if let Ok((next, _)) = alone.apply(0, command) {
            alone = next;
        }
    };
    for other in 0..project.track_count() {
        apply(Command::SetTrackSolo {
            track: other,
            solo: other == track,
        });
    }
    apply(Command::SetTrackMute {
        track,
        muted: false,
    });
    alone
}

/// The frames of the song: up to the end of the last clip, or of the last
/// note if that is later. A note fades out within its own length (ADR 15).
fn song_frames(project: &Project) -> u64 {
    let rate = u64::from(SAMPLE_RATE);
    let tempo = project.tempo_map();
    let clips = project
        .clips()
        .iter()
        .map(|clip| tempo.sample_of(rate, clip.stop()))
        .max()
        .unwrap_or(0);
    // Every track counts, heard or not, since each is measured.
    let mut everything = project.clone();
    for track in 0..project.track_count() {
        if let Ok((next, _)) = everything.apply(
            0,
            Command::SetTrackMute {
                track,
                muted: false,
            },
        ) {
            everything = next;
        }
        if let Ok((next, _)) = everything.apply(0, Command::SetTrackSolo { track, solo: false }) {
            everything = next;
        }
    }
    let notes = everything
        .schedule(rate)
        .last()
        .map_or(0, |event| match *event {
            Event::On { sample, .. } | Event::Off { sample, .. } => sample,
        });
    u64::try_from(clips.max(notes)).unwrap_or(u64::MAX)
}

/// Each bar that starts within `frames`, with its start tick and its frames.
fn bars(project: &Project, frames: usize) -> Vec<(Tick, Range<usize>)> {
    let tempo = project.tempo_map();
    let rate = u64::from(SAMPLE_RATE);
    let ticks = BEATS_PER_BAR * PPQ;
    let frame_of = |tick: Tick| usize::try_from(tempo.sample_of(rate, tick)).unwrap_or(usize::MAX);
    let mut bars = Vec::new();
    let mut start: Tick = 0;
    while frame_of(start) < frames {
        bars.push((start, frame_of(start)..frame_of(start + ticks).min(frames)));
        start += ticks;
    }
    bars
}

fn measurements(frames: &[Frame], bars: &[(Tick, Range<usize>)]) -> Measurements {
    let rate = f64::from(SAMPLE_RATE);
    let squares = signal::k_weighted_squares(frames, rate);
    let ranges: Vec<Range<usize>> = bars.iter().map(|(_, range)| range.clone()).collect();
    let (peak, bar_peaks) = signal::true_peaks(frames, &ranges);
    Measurements {
        loudness: signal::integrated_loudness(&squares, rate).map(round),
        true_peak: signal::decibels(peak).map(round),
        correlation: signal::correlation(frames, 0..frames.len()).map(round),
        bands: signal::band_levels(frames, rate)
            .into_iter()
            .map(|(hz, level)| BandLevel {
                hz,
                level: level.map(round),
            })
            .collect(),
        bars: bars
            .iter()
            .zip(bar_peaks)
            .enumerate()
            .map(|(index, ((start_tick, range), peak))| BarMeasurements {
                bar: index as u64 + 1,
                start_tick: *start_tick,
                loudness: signal::loudness(&squares, range.clone()).map(round),
                true_peak: signal::decibels(peak).map(round),
                correlation: signal::correlation(frames, range.clone()).map(round),
            })
            .collect(),
    }
}

/// `value` to hundredths, which is finer than a review needs.
fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0 + 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathos_core::{Document, Edit};

    fn project(lines: &[&str]) -> Project {
        let mut document = Document::default();
        for line in lines {
            document.edit(line.parse::<Edit>().unwrap()).unwrap();
        }
        document.history().present().clone()
    }

    #[test]
    fn an_empty_project_has_nothing_to_measure() {
        let report = measure(&Project::default()).unwrap();
        assert_eq!(report.seconds, 0.0);
        assert_eq!(report.master.loudness, None);
        assert_eq!(report.master.true_peak, None);
        assert!(report.master.bars.is_empty());
        assert!(report.tracks.is_empty());
    }

    #[test]
    fn each_track_is_measured_alone_and_the_master_as_heard() {
        // At 120 BPM a bar lasts two seconds. Track 1 plays A4 for a bar,
        // track 2 plays A5 in the second bar, panned hard left and muted.
        let project = project(&[
            "add-track",
            "add-track",
            "add-clip 0 0 3840",
            "add-note 0 0 3840 69 127",
            "add-clip 1 3840 3840",
            "add-note 1 0 3840 81 127",
            "set-track-pan 1 0",
            "mute-track 1",
        ]);
        let report = measure(&project).unwrap();
        assert_eq!(report.seconds, 4.0);
        let [first, second] = &report.tracks[..] else {
            panic!("two tracks");
        };
        assert_eq!((first.track, first.muted, second.muted), (0, false, true));
        // The muted track is measured all the same, in its own bar.
        assert!(second.measurements.loudness.is_some());
        assert_eq!(second.measurements.bars[0].loudness, None);
        assert!(second.measurements.bars[1].loudness.is_some());
        assert_eq!(second.measurements.correlation, None);
        // The master hears only the first track, centered.
        assert_eq!(report.master.loudness, first.measurements.loudness);
        assert_eq!(report.master.correlation, Some(1.0));
        assert_eq!(report.master.bars.len(), 2);
        assert_eq!(report.master.bars[1].loudness, None);
        assert_eq!(
            (report.master.bars[1].bar, report.master.bars[1].start_tick),
            (2, 3840)
        );
        // A4 is in the band around 500 Hz, A5 around 1 kHz.
        let loudest = |measurements: &Measurements| {
            measurements
                .bands
                .iter()
                .max_by(|a, b| a.level.partial_cmp(&b.level).unwrap())
                .unwrap()
                .hz
        };
        assert_eq!(loudest(&first.measurements), 500.0);
        assert_eq!(loudest(&second.measurements), 1000.0);
        let peak = first.measurements.true_peak.unwrap();
        assert!(peak <= 0.1 && peak > -12.0, "{peak}");
    }

    #[test]
    fn a_song_past_the_limit_is_refused() {
        // A clip starting at eleven minutes, at 120 BPM.
        let start = 11 * 60 * 2 * PPQ;
        let project = project(&["add-track", &format!("add-clip 0 {start} 1920")]);
        let error = measure(&project).unwrap_err();
        assert!(error.seconds > 660.0);
        assert!(error.to_string().starts_with("the song lasts 661 seconds"));
    }
}
