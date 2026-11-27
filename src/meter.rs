//! The level meter as the window shows it (ADR 25): it jumps up to each peak
//! the audio thread reports and falls back slowly, so that a short peak stays
//! visible, and it holds each channel's recent peak for a while longer
//! (ADR 30).

use std::time::Duration;

use pathos_render::Frame;

/// How fast a shown level falls after its peak.
const FALL_DECIBELS_PER_SECOND: f32 = 24.0;
/// The quietest level the meter shows; below it, the meter is empty.
const FLOOR_DECIBELS: f32 = -60.0;
/// The loudest level the meter shows, the most a volume can raise a note.
const CEILING_DECIBELS: f32 = 6.0;
/// Where the meter's high zone begins, 6 dB below full scale.
const HIGH_DECIBELS: f32 = -6.0;
/// How long a held peak stays before it falls.
const HOLD: Duration = Duration::from_secs(2);

/// Which zone of the meter a level falls into (ADR 25, ADR 30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Below -6 dB.
    Low,
    /// From -6 dB up to full scale (0 dB).
    High,
    /// At or above full scale (0 dB), or non-finite.
    Clip,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meter {
    /// The level shown in each channel, as a gain.
    levels: Frame,
    holds: [Hold; 2],
}

/// The loudest level a channel has shown lately, and how long ago it was
/// reached.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Hold {
    level: f32,
    age: Duration,
}

impl Meter {
    /// Follows `peaks`, the loudest samples of each channel since the last
    /// update `elapsed` ago.
    pub fn update(&mut self, peaks: Frame, elapsed: Duration) {
        for ((level, hold), peak) in self.levels.iter_mut().zip(&mut self.holds).zip(peaks) {
            *level = fall(*level, elapsed).max(peak);
            // Only the part of `elapsed` past the hold's two seconds lowers it.
            let falling = (hold.age + elapsed).saturating_sub(hold.age.max(HOLD));
            *hold = Hold {
                level: fall(hold.level, falling),
                age: hold.age + elapsed,
            };
            if peak >= hold.level {
                *hold = Hold {
                    level: peak,
                    age: Duration::ZERO,
                };
            }
        }
    }

    pub fn levels(&self) -> Frame {
        self.levels
    }

    /// The level held in each channel, as a gain.
    pub fn holds(&self) -> Frame {
        self.holds.map(|hold| hold.level)
    }

    /// Whether the meter shows nothing, so that it needs no more frames.
    pub fn is_empty(&self) -> bool {
        self.levels == [0.0; 2] && self.holds() == [0.0; 2]
    }
}

/// `level` after falling for `time`, or nothing once below the floor.
fn fall(level: f32, time: Duration) -> f32 {
    let fallen = level * 10f32.powf(-FALL_DECIBELS_PER_SECOND * time.as_secs_f32() / 20.0);
    if fallen < floor() { 0.0 } else { fallen }
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

/// The meter zone that `level` falls into (ADR 25, ADR 30): low below -6 dB,
/// high from -6 dB up to full scale, and clip at full scale and above.
pub fn zone(level: f32) -> Zone {
    let high = 10f32.powf(HIGH_DECIBELS / 20.0);
    if level < high {
        Zone::Low
    } else if level < 1.0 {
        Zone::High
    } else {
        Zone::Clip
    }
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
    fn a_hold_stays_two_seconds_at_its_peak_then_falls_24_db_a_second() {
        let mut meter = Meter::default();
        meter.update([0.5, 1.0], FRAME);
        assert_eq!(meter.holds(), [0.5, 1.0]);
        // A quieter peak leaves the hold where it is.
        meter.update([0.25, 0.0], Duration::from_millis(1984));
        assert_eq!(meter.holds(), [0.5, 1.0]);
        // Two seconds after the peak, a quarter of a second later, 6 dB
        // lower: the hold falls only for the part past its two seconds.
        meter.update([0.0, 0.0], Duration::from_millis(266));
        let [left, right] = meter.holds();
        assert!(close(left, 0.5 * 0.501_187), "{left}");
        assert!(close(right, 0.501_187), "{right}");
        // A peak as loud as the falling hold holds again.
        meter.update([0.0, right], Duration::ZERO);
        meter.update([0.0, 0.0], Duration::from_millis(1999));
        assert_eq!(meter.holds()[1], right);
        assert!(meter.holds()[0] < left);
    }

    #[test]
    fn a_hold_below_60_db_empties_and_the_meter_with_it() {
        let mut meter = Meter::default();
        meter.update([1.0, 0.0], FRAME);
        // The bar is empty after two and a half seconds, the hold two seconds
        // later.
        meter.update([0.0, 0.0], Duration::from_millis(2600));
        assert_eq!(meter.levels(), [0.0; 2]);
        assert!(!meter.is_empty());
        meter.update([0.0, 0.0], Duration::from_millis(1800));
        assert!(!meter.is_empty());
        meter.update([0.0, 0.0], Duration::from_millis(200));
        assert!(meter.is_empty());
    }

    #[test]
    fn a_hold_is_never_below_the_bar() {
        let mut meter = Meter::default();
        // Peaks rising and falling at times of every length.
        for step in 0..2000_u32 {
            let peak = ((step * 7919) % 1000) as f32 / 1000.0 * (step % 3) as f32;
            let elapsed = Duration::from_millis(u64::from(step * 104_729 % 700));
            meter.update([peak, peak / 2.0], elapsed);
            let (levels, holds) = (meter.levels(), meter.holds());
            assert!(holds[0] >= levels[0] && holds[1] >= levels[1], "{step}");
        }
    }

    #[test]
    fn a_level_below_60_db_empties_the_meter() {
        let mut meter = Meter::default();
        meter.update([1.0, 0.0], FRAME);
        // 60 dB takes two and a half seconds.
        meter.update([0.0, 0.0], Duration::from_millis(2400));
        assert_ne!(meter.levels(), [0.0; 2]);
        meter.update([0.0, 0.0], Duration::from_millis(200));
        assert_eq!(meter.levels(), [0.0; 2]);
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

    #[test]
    fn zone_classifies_levels_at_the_exact_minus_6_db_and_full_scale_boundaries() {
        let high = 10f32.powf(-6.0 / 20.0);
        assert_eq!(zone(0.0), Zone::Low);
        assert_eq!(zone(0.5), Zone::Low);
        assert_eq!(zone(high * (1.0 - 1e-6)), Zone::Low);
        assert_eq!(zone(high), Zone::High);
        assert_eq!(zone(0.999), Zone::High);
        assert_eq!(zone(1.0), Zone::Clip);
        assert_eq!(zone(1.01), Zone::Clip);
        assert_eq!(zone(2.0), Zone::Clip);
        assert_eq!(zone(f32::INFINITY), Zone::Clip);
        assert_eq!(zone(f32::NAN), Zone::Clip);
    }
}
