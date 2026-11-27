//! The audio device: a cpal output stream whose callback runs the [`Engine`].

use std::fmt;
use std::num::NonZeroU32;
#[cfg(not(loom))]
use std::sync::Arc;

#[cfg(loom)]
use loom::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use pathos_render::Score;

use crate::engine::{Engine, Transport};
use crate::handoff::{Publisher, handoff};

/// The samples converted at once from the engine's `f32` to the device's
/// format, allocated before the stream starts.
const BUFFER: usize = 8192;

#[derive(Debug)]
pub struct OutputError(String);

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OutputError {}

fn error(context: &str) -> impl FnOnce(cpal::Error) -> OutputError {
    move |error| OutputError(format!("{context}: {error}"))
}

/// Playback on the default output device. Dropping it stops the stream.
pub struct Output {
    // Dropped first, so that the audio thread is gone before the publisher.
    _stream: cpal::Stream,
    publisher: Publisher<Score>,
    transport: Arc<Transport>,
    sample_rate: NonZeroU32,
}

impl Output {
    pub fn open() -> Result<Self, OutputError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| OutputError("no output device".into()))?;
        let supported = device
            .default_output_config()
            .map_err(error("cannot read the output configuration"))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let sample_rate = NonZeroU32::new(config.sample_rate)
            .ok_or_else(|| OutputError("the device has no sample rate".into()))?;
        let (publisher, player) = handoff();
        let transport = Arc::new(Transport::default());
        let engine = Engine::new(player, transport.clone());
        let stream = match sample_format {
            SampleFormat::F32 => build::<f32>(&device, &config, engine),
            SampleFormat::I16 => build::<i16>(&device, &config, engine),
            SampleFormat::I32 => build::<i32>(&device, &config, engine),
            SampleFormat::U16 => build::<u16>(&device, &config, engine),
            format => return Err(OutputError(format!("unsupported sample format {format}"))),
        }?;
        stream.play().map_err(error("cannot start the stream"))?;
        Ok(Self {
            _stream: stream,
            publisher,
            transport,
            sample_rate,
        })
    }

    /// The device's rate, at which every published score must be built.
    pub fn sample_rate(&self) -> NonZeroU32 {
        self.sample_rate
    }

    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Plays `score` from the next block on.
    pub fn publish(&self, score: Score) {
        self.publisher.publish(score);
        self.collect();
    }

    /// Frees the score the audio thread is done with. Call it now and then,
    /// such as once a frame, so that a newer score is not held back.
    pub fn collect(&self) {
        self.publisher.collect();
    }
}

/// Number of `f32` samples to preallocate for converting a device with
/// `channels` channels: whole frames up to `BUFFER`, or one whole frame when
/// a single frame is larger than `BUFFER`, and `0` when `channels` is zero.
pub fn buffer_len(channels: usize) -> usize {
    BUFFER
        .checked_div(channels)
        .map_or(0, |frames| frames.max(1) * channels)
}

/// Fills `data` in whole-frame chunks that fit in `buffer`, asking `fill` for
/// `f32` samples and converting each to `T`. Writes silence when `channels`
/// is zero or `buffer` cannot hold a whole frame, and never allocates.
pub fn fill_device<T: Sample + FromSample<f32>>(
    data: &mut [T],
    buffer: &mut [f32],
    channels: usize,
    mut fill: impl FnMut(&mut [f32], usize),
) {
    let chunk = buffer
        .len()
        .checked_div(channels)
        .map_or(0, |frames| frames * channels);
    if chunk == 0 {
        data.fill(T::EQUILIBRIUM);
        return;
    }
    for part in data.chunks_mut(chunk) {
        let samples = &mut buffer[..part.len()];
        fill(samples, channels);
        for (out, &sample) in part.iter_mut().zip(samples.iter()) {
            *out = T::from_sample(sample);
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut engine: Engine,
) -> Result<cpal::Stream, OutputError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let mut buffer = vec![0.0; buffer_len(channels)];
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                fill_device(data, &mut buffer, channels, |samples, channels| {
                    engine.fill(samples, channels);
                });
            },
            |error| eprintln!("audio output: {error}"),
            None,
        )
        .map_err(error("cannot open the output stream"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_len_handles_zero_divisors_and_wide_frames() {
        assert_eq!(buffer_len(0), 0);
        assert_eq!(buffer_len(1), BUFFER);
        assert_eq!(buffer_len(2), BUFFER);
        assert_eq!(buffer_len(3), (BUFFER / 3) * 3);
        assert_eq!(buffer_len(3) % 3, 0);
        assert_eq!(buffer_len(BUFFER), BUFFER);
        assert_eq!(buffer_len(BUFFER + 1), BUFFER + 1);
    }

    #[test]
    fn fill_device_splits_chunks_larger_than_buffer_and_non_multiples() {
        let mut buffer = vec![0.0; 6];
        let mut data = vec![-1.0f32; 17];
        let mut chunks = Vec::new();
        let mut next = 1usize;
        fill_device(&mut data, &mut buffer, 2, |samples, channels| {
            assert_eq!(channels, 2);
            chunks.push(samples.len());
            for sample in samples {
                *sample = next as f32;
                next += 1;
            }
        });
        assert_eq!(chunks, [6, 6, 5]);
        let expected: Vec<f32> = (1..=17).map(|i| i as f32).collect();
        assert_eq!(data, expected);

        // Unaligned scratch buffer rounds down to whole frames.
        let mut unaligned = vec![0.0; 5];
        let mut data = vec![0.0f32; 11];
        let mut chunks = Vec::new();
        fill_device(&mut data, &mut unaligned, 2, |samples, channels| {
            assert_eq!(channels, 2);
            chunks.push(samples.len());
            samples.fill(0.25);
        });
        assert_eq!(chunks, [4, 4, 3]);
        assert_eq!(data, vec![0.25; 11]);
    }

    #[test]
    fn fill_device_converts_f32_i16_i32_and_u16() {
        let input = [-1.0f32, -0.5, 0.0, 0.5, 1.0];

        fn run<T: Sample + FromSample<f32> + PartialEq + fmt::Debug>(input: &[f32], fill_value: T) {
            let mut buffer = vec![0.0; 2];
            let mut out = vec![fill_value; input.len()];
            let mut cursor = 0;
            fill_device(&mut out, &mut buffer, 1, |samples, channels| {
                assert_eq!(channels, 1);
                let end = cursor + samples.len();
                samples.copy_from_slice(&input[cursor..end]);
                cursor = end;
            });
            let expected: Vec<T> = input.iter().copied().map(T::from_sample).collect();
            assert_eq!(out, expected);
        }

        run::<f32>(&input, 0.123);
        run::<i16>(&input, 123);
        run::<i32>(&input, 123);
        run::<u16>(&input, 123);
    }

    #[test]
    fn fill_device_writes_silence_for_zero_channels_or_undersized_buffer() {
        let mut buffer = vec![0.0; buffer_len(0)];
        let mut f32_out = vec![0.5f32; 5];
        fill_device(&mut f32_out, &mut buffer, 0, |_, _| {
            panic!("fill must not run for 0 channels");
        });
        assert_eq!(f32_out, [0.0f32; 5]);

        let mut i16_out = vec![42i16; 4];
        fill_device(&mut i16_out, &mut buffer, 0, |_, _| {
            panic!("fill must not run for 0 channels");
        });
        assert_eq!(i16_out, [i16::EQUILIBRIUM; 4]);

        let mut i32_out = vec![42i32; 4];
        fill_device(&mut i32_out, &mut buffer, 0, |_, _| {
            panic!("fill must not run for 0 channels");
        });
        assert_eq!(i32_out, [i32::EQUILIBRIUM; 4]);

        let mut u16_out = vec![0u16; 4];
        fill_device(&mut u16_out, &mut buffer, 0, |_, _| {
            panic!("fill must not run for 0 channels");
        });
        assert_eq!(u16_out, [u16::EQUILIBRIUM; 4]);

        // Buffer smaller than one frame writes silence without panicking.
        let mut tiny = vec![0.0; 1];
        let mut stereo_out = vec![123u16; 4];
        fill_device(&mut stereo_out, &mut tiny, 2, |_, _| {
            panic!("fill must not run when buffer is smaller than one frame");
        });
        assert_eq!(stereo_out, [u16::EQUILIBRIUM; 4]);
    }

    #[test]
    fn fill_device_handles_one_two_and_more_than_buffer_channels() {
        for channels in [1, 2, BUFFER + 1] {
            let mut buffer = vec![0.0; buffer_len(channels)];
            let total = buffer.len() + channels + 3;
            let mut data = vec![0i16; total];
            let mut cursor = 0usize;
            fill_device(&mut data, &mut buffer, channels, |samples, ch| {
                assert_eq!(ch, channels);
                for sample in samples {
                    *sample = if cursor.is_multiple_of(2) { 0.5 } else { -0.5 };
                    cursor += 1;
                }
            });
            assert_eq!(cursor, total);
            assert_eq!(data[0], i16::from_sample(0.5f32));
            assert_eq!(data[1], i16::from_sample(-0.5f32));
            assert_eq!(
                data[total - 1],
                i16::from_sample(if (total - 1).is_multiple_of(2) {
                    0.5f32
                } else {
                    -0.5f32
                })
            );
        }
    }
}
