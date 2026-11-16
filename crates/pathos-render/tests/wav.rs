//! The WAV file of a render holds its samples bit for bit and is read by
//! another WAV library.

use std::io::Cursor;
use std::num::NonZeroU32;

use pathos_render::wav;
use proptest::prelude::*;

fn wav_of(rate: u32, samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    wav::write(&mut out, NonZeroU32::new(rate).unwrap(), samples).unwrap();
    out
}

proptest! {
    /// hound reads back the sample rate and every sample, bit for bit.
    #[test]
    fn a_reader_gets_the_samples_back(
        rate in 1u32..1_000_000,
        samples in prop::collection::vec(any::<f32>(), 0..2000),
    ) {
        let mut reader = hound::WavReader::new(Cursor::new(wav_of(rate, &samples))).unwrap();
        let spec = reader.spec();
        prop_assert_eq!(spec.channels, 1);
        prop_assert_eq!(spec.sample_rate, rate);
        prop_assert_eq!(spec.bits_per_sample, 32);
        prop_assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        prop_assert_eq!(reader.duration() as usize, samples.len());
        let read: Vec<u32> = reader.samples::<f32>().map(|sample| sample.unwrap().to_bits()).collect();
        let written: Vec<u32> = samples.iter().map(|sample| sample.to_bits()).collect();
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
    expected.extend_from_slice(&58u32.to_le_bytes());
    expected.extend_from_slice(b"WAVEfmt ");
    expected.extend_from_slice(&18u32.to_le_bytes());
    expected.extend_from_slice(&3u16.to_le_bytes());
    expected.extend_from_slice(&1u16.to_le_bytes());
    expected.extend_from_slice(&48_000u32.to_le_bytes());
    expected.extend_from_slice(&192_000u32.to_le_bytes());
    expected.extend_from_slice(&4u16.to_le_bytes());
    expected.extend_from_slice(&32u16.to_le_bytes());
    expected.extend_from_slice(&0u16.to_le_bytes());
    expected.extend_from_slice(b"fact");
    expected.extend_from_slice(&4u32.to_le_bytes());
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(b"data");
    expected.extend_from_slice(&8u32.to_le_bytes());
    expected.extend_from_slice(&0.5f32.to_le_bytes());
    expected.extend_from_slice(&(-1.0f32).to_le_bytes());
    assert_eq!(wav_of(48_000, &[0.5, -1.0]), expected);
}

/// A sample rate whose byte rate does not fit in the header is refused.
#[test]
fn a_rate_too_high_for_wav_is_refused() {
    let mut out = Vec::new();
    let rate = NonZeroU32::new(u32::MAX / 4 + 1).unwrap();
    let error = wav::write(&mut out, rate, &[0.0]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(out.is_empty());
}
