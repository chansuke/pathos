//! The position, time and tempo that the top bar shows while the playhead
//! moves (ADR 27).

use std::num::NonZeroU32;

use pathos_core::{PPQ, Project, Tick};

/// The beats in a bar, as the lanes draw bars, until a project has a meter.
const BEATS_PER_BAR: u64 = 4;

/// `tick` as bar, beat and sixteenth, each counted from 1: "3.2.1".
pub fn position_label(tick: Tick) -> String {
    let bar = tick / (BEATS_PER_BAR * PPQ) + 1;
    let beat = tick / PPQ % BEATS_PER_BAR + 1;
    let sixteenth = tick % PPQ / (PPQ / 4) + 1;
    format!("{bar}.{beat}.{sixteenth}")
}

/// The time of `frame` at `sample_rate` in minutes, seconds and milliseconds,
/// rounded down: "0:04.500".
pub fn time_label(frame: u64, sample_rate: NonZeroU32) -> String {
    let millis = u128::from(frame) * 1000 / u128::from(sample_rate.get());
    let (minutes, millis) = (millis / 60_000, millis % 60_000);
    format!("{minutes}:{:02}.{:03}", millis / 1000, millis % 1000)
}

/// The tempo in force at `tick` in beats per minute, to a tenth: "120.0".
pub fn tempo_label(project: &Project, tick: Tick) -> String {
    let us_per_quarter = project
        .tempo()
        .iter()
        .take_while(|change| change.tick <= tick)
        .last()
        .map_or(500_000, |change| change.us_per_quarter);
    let tenths = (600_000_000 + us_per_quarter / 2) / us_per_quarter;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use pathos_core::{Command, History};

    use super::*;

    #[test]
    fn a_position_counts_bars_beats_and_sixteenths_from_one() {
        assert_eq!(position_label(0), "1.1.1");
        assert_eq!(position_label(PPQ / 4 - 1), "1.1.1");
        assert_eq!(position_label(PPQ / 4), "1.1.2");
        assert_eq!(position_label(PPQ), "1.2.1");
        assert_eq!(position_label(4 * PPQ + PPQ + PPQ / 2), "2.2.3");
        assert_eq!(position_label(12 * PPQ - 1), "3.4.4");
        assert_eq!(position_label(12 * PPQ), "4.1.1");
    }

    #[test]
    fn a_time_shows_minutes_seconds_and_milliseconds() {
        let rate = NonZeroU32::new(48_000).unwrap();
        assert_eq!(time_label(0, rate), "0:00.000");
        assert_eq!(time_label(47, rate), "0:00.000");
        assert_eq!(time_label(48, rate), "0:00.001");
        assert_eq!(time_label(216_000, rate), "0:04.500");
        assert_eq!(time_label(48_000 * 61 + 24_000, rate), "1:01.500");
        assert_eq!(time_label(48_000 * 600, rate), "10:00.000");
    }

    #[test]
    fn a_tempo_is_the_one_in_force_at_the_tick() {
        let mut history = History::default();
        for command in [
            Command::SetTempo {
                tick: 4 * PPQ,
                us_per_quarter: 1_000_000,
            },
            Command::SetTempo {
                tick: 8 * PPQ,
                us_per_quarter: 461_538,
            },
            Command::SetTempo {
                tick: 12 * PPQ,
                us_per_quarter: 461_539,
            },
        ] {
            history.edit(pathos_core::Edit::Apply(command)).unwrap();
        }
        let project = history.present();
        assert_eq!(tempo_label(project, 0), "120.0");
        assert_eq!(tempo_label(project, 4 * PPQ - 1), "120.0");
        assert_eq!(tempo_label(project, 4 * PPQ), "60.0");
        // 130.0000867 BPM shows as 130.0.
        assert_eq!(tempo_label(project, 8 * PPQ), "130.0");
        // 129.99978 BPM rounds to the nearest tenth, not down.
        assert_eq!(tempo_label(project, 12 * PPQ), "130.0");
    }
}
