//! A render as a WAV file: one channel of 32-bit IEEE float samples, so that the
//! file holds the render bit for bit.

use std::io::{self, Write};
use std::num::NonZeroU32;

const FLOAT: u16 = 3;
const BYTES_PER_SAMPLE: u32 = 4;
/// The bytes after the size field of the RIFF chunk and before the samples:
/// the `WAVE` tag, an 18-byte `fmt ` chunk, a 4-byte `fact` chunk and the
/// header of the `data` chunk.
const HEADER: u32 = 4 + (8 + 18) + (8 + 4) + 8;

/// Writes `samples` at `sample_rate` as a WAV file. Fails with
/// [`io::ErrorKind::InvalidInput`] when the file would exceed the 4 GiB that a
/// WAV file can hold.
pub fn write(out: &mut impl Write, sample_rate: NonZeroU32, samples: &[f32]) -> io::Result<()> {
    let too_long = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a WAV file holds at most 4 GiB",
        )
    };
    let frames = u32::try_from(samples.len()).map_err(|_| too_long())?;
    let data = frames.checked_mul(BYTES_PER_SAMPLE).ok_or_else(too_long)?;
    let riff = data.checked_add(HEADER).ok_or_else(too_long)?;
    let rate = sample_rate.get();
    let byte_rate = rate.checked_mul(BYTES_PER_SAMPLE).ok_or_else(|| {
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
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&(BYTES_PER_SAMPLE as u16).to_le_bytes());
    header.extend_from_slice(&(8 * BYTES_PER_SAMPLE as u16).to_le_bytes());
    // A format other than PCM has an extension, here empty, and a `fact` chunk
    // with the number of frames.
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4u32.to_le_bytes());
    header.extend_from_slice(&frames.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data.to_le_bytes());
    out.write_all(&header)?;

    let body: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    out.write_all(&body)
}
