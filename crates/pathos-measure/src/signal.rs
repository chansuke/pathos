//! Measurements of a stereo signal: loudness after ITU-R BS.1770, true peak,
//! octave bands and phase correlation.

use std::f64::consts::PI;

use pathos_render::Frame;

/// A biquad filter in direct form I, run on one channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b: [f64; 3],
    /// `a[0]` is 1 and left out.
    pub a: [f64; 2],
}

impl Biquad {
    /// The output of the filter for `input`, from silence.
    pub fn run(&self, input: impl IntoIterator<Item = f64>) -> impl Iterator<Item = f64> {
        let filter = *self;
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        input.into_iter().map(move |x| {
            let y = filter.b[0] * x + filter.b[1] * x1 + filter.b[2] * x2
                - filter.a[0] * y1
                - filter.a[1] * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            y
        })
    }
}

/// The two stages of the K-weighting filter of BS.1770 at `rate`: a high shelf
/// for the head, then a high pass. The parameters are those of the analog
/// filters whose bilinear transform at 48 kHz gives the coefficients that
/// BS.1770 lists, so that other rates get the same response.
pub fn k_weighting(rate: f64) -> [Biquad; 2] {
    let shelf = {
        let (f0, gain, q) = (
            1_681.974_450_955_533,
            3.999_843_853_973_347,
            0.707_175_236_955_419_6,
        );
        let k = (PI * f0 / rate).tan();
        let vh = 10f64.powf(gain / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    };
    let high_pass = {
        let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
        let k = (PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [1.0, -2.0, 1.0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    };
    [shelf, high_pass]
}

/// Each channel of `frames` through the K-weighting filter, squared.
pub fn k_weighted_squares(frames: &[Frame], rate: f64) -> [Vec<f64>; 2] {
    let [shelf, high_pass] = k_weighting(rate);
    [0, 1].map(|channel| {
        let input = frames.iter().map(|frame| f64::from(frame[channel]));
        high_pass.run(shelf.run(input)).map(|y| y * y).collect()
    })
}

/// The loudness of a mean square summed over the channels, in LUFS.
fn lufs(power: f64) -> Option<f64> {
    (power > 0.0).then(|| -0.691 + 10.0 * power.log10())
}

/// The mean of `squares` over `range`.
fn mean(squares: &[f64], range: std::ops::Range<usize>) -> f64 {
    let length = range.len().max(1) as f64;
    squares[range].iter().sum::<f64>() / length
}

/// The absolute gate of BS.1770: anything quieter counts as silence. It
/// also hides the tail of the filter after the last note.
const ABSOLUTE_GATE: f64 = -70.0;

/// The loudness of `range` in LUFS, gated only at -70 LUFS. None for
/// silence.
pub fn loudness(squares: &[Vec<f64>; 2], range: std::ops::Range<usize>) -> Option<f64> {
    lufs(
        squares
            .iter()
            .map(|channel| mean(channel, range.clone()))
            .sum(),
    )
    .filter(|&loudness| loudness > ABSOLUTE_GATE)
}

/// The integrated loudness of BS.1770, in LUFS: blocks of 400 ms that
/// overlap by three quarters, gated at -70 LUFS and then 10 LU below the
/// loudness of the blocks that pass. None when no block passes, as for
/// silence or a signal shorter than a block.
pub fn integrated_loudness(squares: &[Vec<f64>; 2], rate: f64) -> Option<f64> {
    let frames = squares[0].len();
    let block = (rate * 0.4).round() as usize;
    let step = (rate * 0.1).round() as usize;
    if frames < block {
        return None;
    }
    let powers: Vec<f64> = (0..=(frames - block) / step)
        .map(|index| {
            let start = index * step;
            squares
                .iter()
                .map(|channel| mean(channel, start..start + block))
                .sum()
        })
        .collect();
    let gated = |threshold: f64| {
        let passed: Vec<f64> = powers
            .iter()
            .copied()
            .filter(|&power| lufs(power).is_some_and(|loudness| loudness > threshold))
            .collect();
        (!passed.is_empty()).then(|| passed.iter().sum::<f64>() / passed.len() as f64)
    };
    let relative = lufs(gated(ABSOLUTE_GATE)?)? - 10.0;
    lufs(gated(relative)?)
}

/// The phases of the filter that oversamples four times for the true peak,
/// each with the taps around a frame.
const PHASES: usize = 4;
const TAPS: usize = 32;

/// The windowed sinc that interpolates a signal at `PHASES` points per frame.
/// Each phase sums to 1, and the first is the frame itself.
fn interpolation() -> [[f64; TAPS]; PHASES] {
    let half = (TAPS / 2) as f64;
    std::array::from_fn(|phase| {
        let mut taps: [f64; TAPS] = std::array::from_fn(|tap| {
            // The distance from the point to the frame of this tap.
            let x = phase as f64 / PHASES as f64 + half - 1.0 - tap as f64;
            let sinc = if x == 0.0 {
                1.0
            } else {
                (PI * x).sin() / (PI * x)
            };
            let window = 0.42
                + 0.5 * (2.0 * PI * x / (2.0 * half)).cos()
                + 0.08 * (4.0 * PI * x / (2.0 * half)).cos();
            sinc * window
        });
        let sum: f64 = taps.iter().sum();
        taps.iter_mut().for_each(|tap| *tap /= sum);
        taps
    })
}

/// The largest absolute value of `frames` oversampled four times, in either
/// channel, for each range of `ranges` and over all of them.
pub fn true_peaks(frames: &[Frame], ranges: &[std::ops::Range<usize>]) -> (f64, Vec<f64>) {
    let filter = interpolation();
    let before = TAPS / 2 - 1;
    // No point interpolated near a frame exceeds the largest sample around it
    // times the largest sum of absolute taps, so the points of a frame whose
    // bound is below the peak so far are not computed. The largest samples
    // are kept per block of TAPS frames; the taps of a frame span at most
    // the blocks beside its own.
    let gain = filter
        .iter()
        .map(|taps| taps.iter().map(|tap| tap.abs()).sum::<f64>())
        .fold(0.0, f64::max);
    let blocks: Vec<f64> = frames
        .chunks(TAPS)
        .map(|block| {
            block
                .iter()
                .map(|frame| f64::from(frame[0].abs().max(frame[1].abs())))
                .fold(0.0, f64::max)
        })
        .collect();
    let bound = |index: usize| {
        let block = index / TAPS;
        let around = block.saturating_sub(1)..(block + 2).min(blocks.len());
        gain * blocks[around].iter().copied().fold(0.0, f64::max)
    };
    let sample = |channel: usize, index: isize| {
        usize::try_from(index)
            .ok()
            .and_then(|index| frames.get(index))
            .map_or(0.0, |frame| f64::from(frame[channel]))
    };
    let peak_at = |index: usize| {
        let mut peak: f64 = 0.0;
        for channel in 0..2 {
            for taps in &filter {
                let value: f64 = taps
                    .iter()
                    .enumerate()
                    .map(|(tap, weight)| {
                        weight * sample(channel, (index + tap) as isize - before as isize)
                    })
                    .sum();
                peak = peak.max(value.abs());
            }
        }
        peak
    };
    let peaks: Vec<f64> = ranges
        .iter()
        .map(|range| {
            range.clone().fold(0.0, |peak: f64, index| {
                if bound(index) <= peak {
                    peak
                } else {
                    peak.max(peak_at(index))
                }
            })
        })
        .collect();
    (peaks.iter().copied().fold(0.0, f64::max), peaks)
}

/// The centers of the octave bands, in hertz.
pub const BANDS: [f64; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// The level of each octave band of [`BANDS`] below the Nyquist frequency,
/// in dB relative to full scale: the mean square of both channels through a
/// band pass one octave wide, with 0 dB at its center. None for silence.
pub fn band_levels(frames: &[Frame], rate: f64) -> Vec<(f64, Option<f64>)> {
    BANDS
        .iter()
        .filter(|&&center| center < rate / 2.0)
        .map(|&center| {
            // A Q of √2 makes the band an octave wide between its -3 dB points.
            let q = std::f64::consts::SQRT_2;
            let w = 2.0 * PI * center / rate;
            let alpha = w.sin() / (2.0 * q);
            let a0 = 1.0 + alpha;
            let filter = Biquad {
                b: [alpha / a0, 0.0, -alpha / a0],
                a: [-2.0 * w.cos() / a0, (1.0 - alpha) / a0],
            };
            let power: f64 = (0..2)
                .map(|channel| {
                    let input = frames.iter().map(|frame| f64::from(frame[channel]));
                    filter.run(input).map(|y| y * y).sum::<f64>()
                })
                .sum::<f64>()
                / (2 * frames.len().max(1)) as f64;
            // A full-scale sine has a mean square of a half.
            (center, (power > 0.0).then(|| 10.0 * (2.0 * power).log10()))
        })
        .collect()
}

/// The correlation of the left and the right channel over `range`, from -1
/// for opposite to 1 for the same signal. None when a channel is silent.
pub fn correlation(frames: &[Frame], range: std::ops::Range<usize>) -> Option<f64> {
    let (mut lr, mut ll, mut rr) = (0.0, 0.0, 0.0);
    for frame in &frames[range] {
        let (l, r) = (f64::from(frame[0]), f64::from(frame[1]));
        (lr, ll, rr) = (lr + l * r, ll + l * l, rr + r * r);
    }
    (ll > 0.0 && rr > 0.0).then(|| (lr / (ll * rr).sqrt()).clamp(-1.0, 1.0))
}

/// A linear level in decibels relative to full scale. None for silence.
pub fn decibels(level: f64) -> Option<f64> {
    (level > 0.0).then(|| 20.0 * level.log10())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48000.0;

    /// A sine of `amplitude` at `frequency` in both channels, the right one
    /// multiplied by `right`, starting at `phase` radians.
    fn sine(frequency: f64, amplitude: f64, phase: f64, right: f64, seconds: f64) -> Vec<Frame> {
        (0..(RATE * seconds) as usize)
            .map(|index| {
                let value = amplitude * (2.0 * PI * frequency * index as f64 / RATE + phase).sin();
                [value as f32, (value * right) as f32]
            })
            .collect()
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} is not within {tolerance} of {expected}"
        );
    }

    #[test]
    fn k_weighting_at_48_khz_has_the_coefficients_of_bs_1770() {
        let [shelf, high_pass] = k_weighting(RATE);
        let expected_shelf = [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
            -1.690_659_293_182_41,
            0.732_480_774_215_85,
        ];
        let actual_shelf = [shelf.b[0], shelf.b[1], shelf.b[2], shelf.a[0], shelf.a[1]];
        for (actual, expected) in actual_shelf.into_iter().zip(expected_shelf) {
            close(actual, expected, 1e-9);
        }
        assert_eq!(high_pass.b, [1.0, -2.0, 1.0]);
        close(high_pass.a[0], -1.990_047_454_833_98, 1e-9);
        close(high_pass.a[1], 0.990_072_250_366_21, 1e-9);
    }

    #[test]
    fn a_full_scale_sine_at_1_khz_in_both_channels_is_0_lufs() {
        // BS.1770: a full-scale 1 kHz sine in one channel reads -3.01 LUFS,
        // so in two it reads 0.
        let frames = sine(997.0, 1.0, 0.0, 1.0, 5.0);
        let squares = k_weighted_squares(&frames, RATE);
        close(integrated_loudness(&squares, RATE).unwrap(), 0.0, 0.05);
        let left_only: Vec<Frame> = frames.iter().map(|frame| [frame[0], 0.0]).collect();
        let squares = k_weighted_squares(&left_only, RATE);
        close(integrated_loudness(&squares, RATE).unwrap(), -3.01, 0.05);
        close(loudness(&squares, 48000..96000).unwrap(), -3.01, 0.05);
    }

    #[test]
    fn quiet_blocks_are_gated_out_of_the_integrated_loudness() {
        // Four seconds at -20 dB and four of silence read as the four alone.
        let mut frames = sine(997.0, 0.1, 0.0, 1.0, 4.0);
        let loud = integrated_loudness(&k_weighted_squares(&frames, RATE), RATE).unwrap();
        frames.extend(vec![[0.0, 0.0]; 4 * 48000]);
        let squares = k_weighted_squares(&frames, RATE);
        // The few blocks that straddle the end are partly silent but pass
        // the gates, so the loudness is a little lower than the sine's.
        close(
            integrated_loudness(&squares, RATE).unwrap(),
            loud - 0.1,
            0.1,
        );
        // The ungated loudness of all eight seconds is 3 dB lower.
        close(
            loudness(&squares, 0..frames.len()).unwrap(),
            loud - 3.01,
            0.05,
        );
        let silence = vec![[0.0, 0.0]; 48000];
        let squares = k_weighted_squares(&silence, RATE);
        assert_eq!(integrated_loudness(&squares, RATE), None);
        assert_eq!(loudness(&squares, 0..48000), None);
        let short = sine(997.0, 1.0, 0.0, 1.0, 0.3);
        assert_eq!(
            integrated_loudness(&k_weighted_squares(&short, RATE), RATE),
            None
        );
    }

    #[test]
    fn the_true_peak_finds_a_peak_between_frames() {
        // A sine at a quarter of the rate, sampled 45 degrees off its peaks,
        // has samples of ±0.707 but peaks at 1.
        let frames = sine(12000.0, 1.0, PI / 4.0, 1.0, 0.1);
        let sample_peak = frames
            .iter()
            .map(|frame| frame[0].abs())
            .fold(0.0, f32::max);
        close(f64::from(sample_peak), 0.5f64.sqrt(), 1e-6);
        // Away from where the sine starts and stops abruptly, which rings.
        let (peak, bars) = true_peaks(&frames, &[1000..1100, 500..4000]);
        close(decibels(peak).unwrap(), 0.0, 0.02);
        close(decibels(bars[0]).unwrap(), 0.0, 0.02);
        // The abrupt start overshoots, as it would through a converter.
        assert!(true_peaks(&frames, std::slice::from_ref(&(0..frames.len()))).0 > peak);
        // A low sine peaks where its samples do.
        let frames = sine(100.0, 0.5, 0.0, -1.0, 0.1);
        let (peak, _) = true_peaks(&frames, std::slice::from_ref(&(0..frames.len())));
        close(peak, 0.5, 1e-3);
        assert_eq!(
            true_peaks(&[[0.0; 2]; 10], std::slice::from_ref(&(0..10))).0,
            0.0
        );
    }

    #[test]
    fn a_sine_is_loudest_in_its_own_band() {
        let frames = sine(1000.0, 1.0, 0.0, 1.0, 1.0);
        let levels = band_levels(&frames, RATE);
        assert_eq!(levels.len(), 10);
        let level = |hz: f64| {
            levels
                .iter()
                .find(|(center, _)| *center == hz)
                .unwrap()
                .1
                .unwrap()
        };
        close(level(1000.0), 0.0, 0.1);
        assert!(level(500.0) < -6.0 && level(2000.0) < -6.0);
        assert!(level(62.5) < -20.0 && level(16000.0) < -20.0);
        // A band above the Nyquist frequency is left out.
        assert_eq!(band_levels(&frames, 32000.0).len(), 9);
        assert!(
            band_levels(&[[0.0; 2]; 100], RATE)
                .iter()
                .all(|(_, level)| level.is_none())
        );
    }

    #[test]
    fn relative_gate_excludes_quiet_sections_above_the_absolute_gate() {
        // Four seconds at -20 dB followed by four seconds at -35 dB.
        // The -35 dB section easily passes the -70 LUFS absolute gate, but
        // sits more than 10 LU below the first-pass loudness (~-22.9 LUFS)
        // and must be excluded by the BS.1770 -10 LU relative gate.
        let loud_only = sine(997.0, 0.1, 0.0, 1.0, 4.0);
        let loud = integrated_loudness(&k_weighted_squares(&loud_only, RATE), RATE).unwrap();

        let quiet_35_amp = 10f64.powf(-35.0 / 20.0);
        let mut mixed_35 = loud_only.clone();
        mixed_35.extend(sine(997.0, quiet_35_amp, 0.0, 1.0, 4.0));
        let squares_35 = k_weighted_squares(&mixed_35, RATE);

        // Confirm the quiet half is well above the absolute gate on its own.
        let quiet_half = loudness(&squares_35, 4 * 48000..8 * 48000).unwrap();
        close(quiet_half, -35.0, 0.1);
        assert!(quiet_half > ABSOLUTE_GATE + 30.0);

        // Without the relative gate (or with a weakened gate), all blocks
        // would average to roughly loud - 2.88 LUFS.
        let ungated = loudness(&squares_35, 0..mixed_35.len()).unwrap();
        close(ungated, loud - 2.88, 0.1);

        // With the -10 LU relative gate, only the -20 dB blocks and the three
        // straddling blocks pass, keeping integrated loudness near `loud`.
        let integrated = integrated_loudness(&squares_35, RATE).unwrap();
        close(integrated, loud - 0.17, 0.1);
        assert!(integrated - ungated > 2.0);

        // A -28 dB section is only 8 dB below -20 dB, so it sits above the
        // -10 LU relative gate (~-32.4 LUFS) and is NOT excluded.
        let quiet_28_amp = 10f64.powf(-28.0 / 20.0);
        let mut mixed_28 = loud_only;
        mixed_28.extend(sine(997.0, quiet_28_amp, 0.0, 1.0, 4.0));
        let squares_28 = k_weighted_squares(&mixed_28, RATE);
        let ungated_28 = loudness(&squares_28, 0..mixed_28.len()).unwrap();
        let integrated_28 = integrated_loudness(&squares_28, RATE).unwrap();
        close(integrated_28, ungated_28, 0.15);
        assert!(loud - integrated_28 > 2.0);
    }

    #[test]
    fn integrated_loudness_block_length_and_gate_boundaries() {
        // Exact 400 ms block length (frames == block) succeeds; one frame
        // shorter (frames == block - 1) returns None.
        let block = (RATE * 0.4).round() as usize;
        let frames = sine(997.0, 0.1, 0.0, 1.0, 0.4);
        assert_eq!(frames.len(), block);
        let squares = k_weighted_squares(&frames, RATE);
        close(integrated_loudness(&squares, RATE).unwrap(), -20.0, 0.1);

        let one_short = [
            squares[0][..block - 1].to_vec(),
            squares[1][..block - 1].to_vec(),
        ];
        assert_eq!(integrated_loudness(&one_short, RATE), None);

        // Exact gate threshold boundary: loudness == ABSOLUTE_GATE (-70.0)
        // must be rejected (strictly greater than threshold), while the next
        // louder representable power must pass.
        // Rate 10 Hz gives block = 4 (a power of two), so mean of 4 equal
        // values is exact in IEEE 754.
        let mut at_gate = 10f64.powf((ABSOLUTE_GATE + 0.691) / 10.0);
        while lufs(at_gate).unwrap() < ABSOLUTE_GATE {
            at_gate = f64::from_bits(at_gate.to_bits() + 1);
        }
        while lufs(at_gate).unwrap() > ABSOLUTE_GATE {
            at_gate = f64::from_bits(at_gate.to_bits() - 1);
        }
        assert_eq!(lufs(at_gate), Some(ABSOLUTE_GATE));

        let exact_squares = [vec![at_gate; 4], vec![0.0; 4]];
        assert_eq!(integrated_loudness(&exact_squares, 10.0), None);
        assert_eq!(loudness(&exact_squares, 0..4), None);

        let mut above_gate = at_gate;
        while lufs(above_gate).unwrap() <= ABSOLUTE_GATE {
            above_gate = f64::from_bits(above_gate.to_bits() + 1);
        }
        let above_squares = [vec![above_gate; 4], vec![0.0; 4]];
        assert!(integrated_loudness(&above_squares, 10.0).is_some());
        assert!(loudness(&above_squares, 0..4).is_some());
    }

    /// Reference implementation of `true_peaks` without block-bound pruning.
    fn naive_true_peaks(frames: &[Frame], ranges: &[std::ops::Range<usize>]) -> (f64, Vec<f64>) {
        let filter = interpolation();
        let before = TAPS / 2 - 1;
        let sample = |channel: usize, index: isize| {
            usize::try_from(index)
                .ok()
                .and_then(|index| frames.get(index))
                .map_or(0.0, |frame| f64::from(frame[channel]))
        };
        let peaks: Vec<f64> = ranges
            .iter()
            .map(|range| {
                let mut peak: f64 = 0.0;
                for index in range.clone() {
                    for channel in 0..2 {
                        for taps in &filter {
                            let value: f64 = taps
                                .iter()
                                .enumerate()
                                .map(|(tap, weight)| {
                                    weight
                                        * sample(channel, (index + tap) as isize - before as isize)
                                })
                                .sum();
                            peak = peak.max(value.abs());
                        }
                    }
                }
                peak
            })
            .collect();
        (peaks.iter().copied().fold(0.0, f64::max), peaks)
    }

    #[test]
    fn true_peaks_pruning_matches_naive_reference() {
        // Deterministic xorshift64 PRNG for varied waveforms without extra deps.
        let mut state = 0x5eed_cafe_1234_5678u64;
        let mut next_sample = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state as f64 / u64::MAX as f64) * 2.0 - 1.0) as f32
        };

        // Varied random stereo waveform with multiple overlapping/disjoint ranges.
        let random_frames: Vec<Frame> = (0..220).map(|_| [next_sample(), next_sample()]).collect();
        let ranges = [0..220, 0..32, 31..65, 64..128, 100..100, 150..220];
        let (actual_max, actual_ranges) = true_peaks(&random_frames, &ranges);
        let (expected_max, expected_ranges) = naive_true_peaks(&random_frames, &ranges);
        close(actual_max, expected_max, 1e-12);
        for (actual, expected) in actual_ranges.iter().zip(&expected_ranges) {
            close(*actual, *expected, 1e-12);
        }

        // Quiet background with sparse spikes so pruning skips most frames.
        let mut sparse = vec![[0.001f32, -0.001f32]; 256];
        sparse[10] = [0.3, -0.2];
        sparse[95] = [-0.95, 0.4];
        sparse[200] = [0.1, 0.6];
        let sparse_ranges = [0..256, 0..128, 90..220, 130..256];
        let (actual_max, actual_ranges) = true_peaks(&sparse, &sparse_ranges);
        let (expected_max, expected_ranges) = naive_true_peaks(&sparse, &sparse_ranges);
        close(actual_max, expected_max, 1e-12);
        for (actual, expected) in actual_ranges.iter().zip(&expected_ranges) {
            close(*actual, *expected, 1e-12);
        }
    }

    #[test]
    fn true_peaks_handles_right_channel_negative_peaks_and_block_boundaries() {
        // Right-channel-only inter-sample peak (left channel silent).
        let right_only: Vec<Frame> = sine(12000.0, 1.0, PI / 4.0, 1.0, 0.05)
            .into_iter()
            .map(|frame| [0.0, frame[1]])
            .collect();
        let ranges = std::slice::from_ref(&(500..1500));
        let (peak, _) = true_peaks(&right_only, ranges);
        let (expected, _) = naive_true_peaks(&right_only, ranges);
        close(peak, expected, 1e-12);
        close(decibels(peak).unwrap(), 0.0, 0.02);

        // Negative-only waveform: peak must report the positive magnitude.
        let mut negative = vec![[0.0f32, 0.0]; 128];
        negative[63] = [-0.8, 0.0];
        negative[64] = [-0.8, 0.0];
        let neg_ranges = std::slice::from_ref(&(0..128));
        let (neg_peak, _) = true_peaks(&negative, neg_ranges);
        let (neg_expected, _) = naive_true_peaks(&negative, neg_ranges);
        close(neg_peak, neg_expected, 1e-12);
        assert!(neg_peak > 0.8);

        // TAPS=32 block boundaries: a spike in block 1 (index TAPS) contributes
        // via sinc taps to frames at the end of block 0 (range 0..TAPS), and
        // symmetrically a spike at TAPS-1 contributes to block 1 (TAPS..2*TAPS).
        let mut spike_at_taps = vec![[0.0f32, 0.0]; 3 * TAPS + 5];
        spike_at_taps[0] = [0.01, 0.0]; // seeds a small peak so pruning is active
        spike_at_taps[TAPS] = [0.0, 1.0];
        let boundary_ranges = [0..TAPS, TAPS..2 * TAPS, 0..spike_at_taps.len()];
        let (actual_max, actual_ranges) = true_peaks(&spike_at_taps, &boundary_ranges);
        let (expected_max, expected_ranges) = naive_true_peaks(&spike_at_taps, &boundary_ranges);
        close(actual_max, expected_max, 1e-12);
        assert!(actual_ranges[0] > 0.1, "block 0 sees spike at index TAPS");
        for (actual, expected) in actual_ranges.iter().zip(&expected_ranges) {
            close(*actual, *expected, 1e-12);
        }

        let mut spike_before_taps = vec![[0.0f32, 0.0]; 3 * TAPS + 5];
        spike_before_taps[TAPS - 1] = [-1.0, 0.0];
        spike_before_taps[2 * TAPS - 1] = [0.0, 0.01];
        let (actual_max, actual_ranges) = true_peaks(&spike_before_taps, &boundary_ranges);
        let (expected_max, expected_ranges) =
            naive_true_peaks(&spike_before_taps, &boundary_ranges);
        close(actual_max, expected_max, 1e-12);
        assert!(
            actual_ranges[1] > 0.1,
            "block 1 sees spike at index TAPS - 1"
        );
        for (actual, expected) in actual_ranges.iter().zip(&expected_ranges) {
            close(*actual, *expected, 1e-12);
        }
    }

    #[test]
    fn band_levels_cover_octave_bands_half_power_points_and_stereo_channels() {
        let level_of = |levels: &[(f64, Option<f64>)], hz: f64| {
            levels
                .iter()
                .find(|(center, _)| *center == hz)
                .unwrap()
                .1
                .unwrap()
        };

        // Test multiple octave bands across the spectrum.
        for center in [62.5, 250.0, 1000.0, 4000.0, 16000.0] {
            let frames = sine(center, 1.0, 0.0, 1.0, 1.0);
            let levels = band_levels(&frames, RATE);
            let at_center = level_of(&levels, center);
            close(at_center, 0.0, 0.15);
            let loudest = levels
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap()
                .0;
            assert_eq!(loudest, center);
        }

        // An octave-wide bandpass (Q = sqrt(2)) has its -3 dB points at
        // center / sqrt(2) and center * sqrt(2).
        for edge in [
            1000.0 / std::f64::consts::SQRT_2,
            1000.0 * std::f64::consts::SQRT_2,
        ] {
            let frames = sine(edge, 1.0, 0.0, 1.0, 1.0);
            let levels = band_levels(&frames, RATE);
            close(level_of(&levels, 1000.0), -3.01, 0.15);
        }

        // One channel silent reduces the stereo mean power by half (-3.01 dB),
        // whether left-only or right-only; opposite phase keeps full power (0 dB).
        let left_only = sine(1000.0, 1.0, 0.0, 0.0, 1.0);
        close(level_of(&band_levels(&left_only, RATE), 1000.0), -3.01, 0.1);
        let right_only: Vec<Frame> = left_only.iter().map(|f| [0.0, f[0]]).collect();
        close(
            level_of(&band_levels(&right_only, RATE), 1000.0),
            -3.01,
            0.1,
        );
        let opposite = sine(1000.0, 1.0, 0.0, -1.0, 1.0);
        close(level_of(&band_levels(&opposite, RATE), 1000.0), 0.0, 0.1);

        // Empty input produces None for every band.
        assert!(
            band_levels(&[], RATE)
                .iter()
                .all(|(_, level)| level.is_none())
        );
    }

    #[test]
    fn correlation_tells_same_opposite_and_unrelated_channels_apart() {
        let same = sine(440.0, 0.5, 0.0, 1.0, 0.5);
        close(correlation(&same, 0..same.len()).unwrap(), 1.0, 1e-9);
        let opposite = sine(440.0, 0.5, 0.0, -1.0, 0.5);
        close(
            correlation(&opposite, 0..opposite.len()).unwrap(),
            -1.0,
            1e-9,
        );
        let left = sine(440.0, 0.5, 0.0, 0.0, 0.5);
        assert_eq!(correlation(&left, 0..left.len()), None);
        // A sine and its cosine are unrelated over whole cycles.
        let quadrature: Vec<Frame> = sine(480.0, 0.5, 0.0, 1.0, 0.5)
            .iter()
            .zip(sine(480.0, 0.5, PI / 2.0, 1.0, 0.5))
            .map(|(a, b)| [a[0], b[0]])
            .collect();
        close(
            correlation(&quadrature, 0..quadrature.len()).unwrap(),
            0.0,
            1e-3,
        );
    }
}
