//! The WAV file of a render holds its samples bit for bit and is read by
//! another WAV library.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

use std::io::Cursor;
use std::num::NonZeroU32;

use pathos_render::{Frame, wav};
use proptest::prelude::*;

fn wav_of(rate: u32, frames: &[Frame]) -> Vec<u8> {
    let mut out = Vec::new();
    wav::write(&mut out, NonZeroU32::new(rate).unwrap(), frames).unwrap();
    out
}

proptest! {
    /// hound reads back the sample rate and every sample of both channels,
    /// bit for bit.
    #[test]
    fn a_reader_gets_the_samples_back(
        rate in 1u32..500_000,
        frames in prop::collection::vec(any::<Frame>(), 0..2000),
    ) {
        let mut reader = hound::WavReader::new(Cursor::new(wav_of(rate, &frames))).unwrap();
        let spec = reader.spec();
        prop_assert_eq!(spec.channels, 2);
        prop_assert_eq!(spec.sample_rate, rate);
        prop_assert_eq!(spec.bits_per_sample, 32);
        prop_assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        prop_assert_eq!(reader.duration() as usize, frames.len());
        let read: Vec<u32> = reader.samples::<f32>().map(|sample| sample.unwrap().to_bits()).collect();
        let written: Vec<u32> = frames.iter().flatten().map(|sample| sample.to_bits()).collect();
        prop_assert_eq!(read, written);
    }
}

/// The file has the chunks and fields of the WAV format for float samples:
/// `fmt ` with an empty extension, `fact` with the number of frames, and
/// `data`.
#[test]
fn a_file_has_the_float_wav_layout() {
    let mut expected = Vec::new();
    expected.extend_from_slice(b"RIFF");
    expected.extend_from_slice(&66u32.to_le_bytes());
    expected.extend_from_slice(b"WAVEfmt ");
    expected.extend_from_slice(&18u32.to_le_bytes());
    expected.extend_from_slice(&3u16.to_le_bytes());
    expected.extend_from_slice(&2u16.to_le_bytes());
    expected.extend_from_slice(&48_000u32.to_le_bytes());
    expected.extend_from_slice(&384_000u32.to_le_bytes());
    expected.extend_from_slice(&8u16.to_le_bytes());
    expected.extend_from_slice(&32u16.to_le_bytes());
    expected.extend_from_slice(&0u16.to_le_bytes());
    expected.extend_from_slice(b"fact");
    expected.extend_from_slice(&4u32.to_le_bytes());
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(b"data");
    expected.extend_from_slice(&16u32.to_le_bytes());
    for sample in [0.5f32, -1.0, 0.25, 0.0] {
        expected.extend_from_slice(&sample.to_le_bytes());
    }
    assert_eq!(wav_of(48_000, &[[0.5, -1.0], [0.25, 0.0]]), expected);
}

/// A sample rate whose byte rate does not fit in the header is refused.
#[test]
fn a_rate_too_high_for_wav_is_refused() {
    let mut out = Vec::new();
    let rate = NonZeroU32::new(u32::MAX / 8 + 1).unwrap();
    let error = wav::write(&mut out, rate, &[[0.0; 2]]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(out.is_empty());
}
