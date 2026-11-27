//! A render as a WAV file: two channels of 32-bit IEEE float samples (ADR 21),
//! so that the file holds the render bit for bit.

use std::io::{self, Write};
use std::num::NonZeroU32;

use crate::Frame;

const FLOAT: u16 = 3;
const CHANNELS: u16 = 2;
const BYTES_PER_SAMPLE: u32 = 4;
const BYTES_PER_FRAME: u32 = CHANNELS as u32 * BYTES_PER_SAMPLE;
/// The bytes after the size field of the RIFF chunk and before the samples:
/// the `WAVE` tag, an 18-byte `fmt ` chunk, a 4-byte `fact` chunk and the
/// header of the `data` chunk.
const HEADER: u32 = 4 + (8 + 18) + (8 + 4) + 8;

/// The size fields of a stereo float WAV header: the frame count in the `fact`
/// chunk, the byte length of the `data` chunk, and the byte length of the
/// `RIFF` chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sizes {
    /// The number of frames, stored in the `fact` chunk.
    pub count: u32,
    /// The byte length of the `data` chunk payload.
    pub data: u32,
    /// The byte length of the `RIFF` chunk payload.
    pub riff: u32,
}

/// Computes the header sizes for a WAV file holding `frames` frames. Fails with
/// [`io::ErrorKind::InvalidInput`] when the file would exceed the 4 GiB that a
/// WAV file can hold.
pub fn sizes(frames: u64) -> io::Result<Sizes> {
    let too_long = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a WAV file holds at most 4 GiB",
        )
    };
    let count = u32::try_from(frames).map_err(|_| too_long())?;
    let data = count.checked_mul(BYTES_PER_FRAME).ok_or_else(too_long)?;
    let riff = data.checked_add(HEADER).ok_or_else(too_long)?;
    Ok(Sizes { count, data, riff })
}

/// Writes `frames` at `sample_rate` as a WAV file. Fails with
/// [`io::ErrorKind::InvalidInput`] when the file would exceed the 4 GiB that a
/// WAV file can hold.
pub fn write(out: &mut impl Write, sample_rate: NonZeroU32, frames: &[Frame]) -> io::Result<()> {
    let Sizes { count, data, riff } = sizes(frames.len() as u64)?;
    let rate = sample_rate.get();
    let byte_rate = rate.checked_mul(BYTES_PER_FRAME).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the sample rate is too high for WAV",
        )
    })?;

    let mut header = Vec::with_capacity(8 + HEADER as usize);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&riff.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&18u32.to_le_bytes());
    header.extend_from_slice(&FLOAT.to_le_bytes());
    header.extend_from_slice(&CHANNELS.to_le_bytes());
    header.extend_from_slice(&rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&(BYTES_PER_FRAME as u16).to_le_bytes());
    header.extend_from_slice(&(8 * BYTES_PER_SAMPLE as u16).to_le_bytes());
    // A format other than PCM has an extension, here empty, and a `fact` chunk
    // with the number of frames.
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4u32.to_le_bytes());
    header.extend_from_slice(&count.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data.to_le_bytes());
    out.write_all(&header)?;

    let body: Vec<u8> = frames
        .iter()
        .flatten()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    out.write_all(&body)
}
