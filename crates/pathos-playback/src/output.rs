//! The audio device: a cpal output stream whose callback runs the [`Engine`].

use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
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

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut engine: Engine,
) -> Result<cpal::Stream, OutputError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let mut buffer = vec![0.0; BUFFER / channels * channels];
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                for part in data.chunks_mut(buffer.len()) {
                    let samples = &mut buffer[..part.len()];
                    engine.fill(samples, channels);
                    for (out, &sample) in part.iter_mut().zip(samples.iter()) {
                        *out = T::from_sample(sample);
                    }
                }
            },
            |error| eprintln!("audio output: {error}"),
            None,
        )
        .map_err(error("cannot open the output stream"))
}
