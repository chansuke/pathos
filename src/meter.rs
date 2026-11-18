//! The level meter as the window shows it (ADR 25): it jumps up to each peak
//! the audio thread reports and falls back slowly, so that a short peak stays
//! visible.

use std::time::Duration;

use pathos_render::Frame;

/// How fast a shown level falls after its peak.
const FALL_DECIBELS_PER_SECOND: f32 = 24.0;
/// The quietest level the meter shows; below it, the meter is empty.
const FLOOR_DECIBELS: f32 = -60.0;
/// The loudest level the meter shows, the most a volume can raise a note.
const CEILING_DECIBELS: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meter {
    /// The level shown in each channel, as a gain.
    levels: Frame,
}

impl Meter {
    /// Follows `peaks`, the loudest samples of each channel since the last
    /// update `elapsed` ago.
    pub fn update(&mut self, peaks: Frame, elapsed: Duration) {
        let fall = 10f32.powf(-FALL_DECIBELS_PER_SECOND * elapsed.as_secs_f32() / 20.0);
        for (level, peak) in self.levels.iter_mut().zip(peaks) {
            let fallen = *level * fall;
            *level = if fallen < floor() { 0.0 } else { fallen }.max(peak);
        }
    }

    pub fn levels(&self) -> Frame {
        self.levels
    }

    /// Whether the meter shows nothing, so that it needs no more frames.
    pub fn is_empty(&self) -> bool {
        self.levels == [0.0; 2]
    }
}

fn floor() -> f32 {
    10f32.powf(FLOOR_DECIBELS / 20.0)
}

/// The part of the meter's length that `level` fills: none at -60 dB and
/// below, all of it at +6 dB and above, in proportion to the decibels between.
pub fn fraction(level: f32) -> f32 {
    if level <= floor() {
        return 0.0;
    }
    let decibels = 20.0 * level.log10();
    ((decibels - FLOOR_DECIBELS) / (CEILING_DECIBELS - FLOOR_DECIBELS)).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_level_jumps_to_a_peak_and_falls_24_db_a_second() {
        let mut meter = Meter::default();
        assert!(meter.is_empty());
        meter.update([0.5, 1.0], FRAME);
        assert_eq!(meter.levels(), [0.5, 1.0]);
        // A quarter of a second later, 6 dB lower: half the gain.
        meter.update([0.0, 0.0], Duration::from_millis(250));
        let [left, right] = meter.levels();
        assert!(close(left, 0.5 * 0.501_187), "{left}");
        assert!(close(right, 0.501_187), "{right}");
        // A peak below the falling level does not pull it down; one above
        // lifts it.
        meter.update([0.1, 0.9], Duration::ZERO);
        assert!(close(meter.levels()[0], 0.5 * 0.501_187));
        assert_eq!(meter.levels()[1], 0.9);
    }

    #[test]
    fn a_level_below_60_db_empties_the_meter() {
        let mut meter = Meter::default();
        meter.update([1.0, 0.0], FRAME);
        // 60 dB takes two and a half seconds.
        meter.update([0.0, 0.0], Duration::from_millis(2400));
        assert!(!meter.is_empty());
        meter.update([0.0, 0.0], Duration::from_millis(200));
        assert!(meter.is_empty());
    }

    #[test]
    fn a_level_at_60_db_still_shows_until_it_falls() {
        let mut meter = Meter::default();
        meter.update([floor(), 0.0], FRAME);
        meter.update([0.0, 0.0], Duration::ZERO);
        assert!(!meter.is_empty());
    }

    #[test]
    fn the_meter_spans_60_db_below_full_scale_to_6_db_above() {
        assert_eq!(fraction(0.0), 0.0);
        assert_eq!(fraction(0.001), 0.0);
        assert!(close(fraction(1.0), 60.0 / 66.0));
        assert!(close(fraction(0.5), (60.0 - 6.0206) / 66.0));
        assert_eq!(fraction(2.0), 1.0);
        assert_eq!(fraction(4.0), 1.0);
    }
}
