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

/// The size computation accepts the largest frame count that fits in a 4 GiB
/// WAV file and refuses every overflow boundary with `InvalidInput`.
#[test]
fn wav_sizes_enforce_the_4_gib_limit() {
    // WAVE tag (4) + fmt chunk (8 + 18) + fact chunk (8 + 4) + data header (8).
    const HEADER: u32 = 4 + (8 + 18) + (8 + 4) + 8;
    const BYTES_PER_FRAME: u32 = 8;
    let max_frames = (u32::MAX - HEADER) / BYTES_PER_FRAME;

    assert_eq!(
        wav::sizes(0).unwrap(),
        wav::Sizes {
            count: 0,
            data: 0,
            riff: HEADER,
        }
    );

    let max = wav::sizes(u64::from(max_frames)).unwrap();
    assert_eq!(
        max,
        wav::Sizes {
            count: max_frames,
            data: max_frames * BYTES_PER_FRAME,
            riff: max_frames * BYTES_PER_FRAME + HEADER,
        }
    );
    assert!(u32::MAX - max.riff < BYTES_PER_FRAME);

    for too_many in [
        // `data` fits in u32, `data + HEADER` overflows u32.
        u64::from(max_frames) + 1,
        // `count` fits in u32, `count * BYTES_PER_FRAME` overflows u32.
        u64::from(u32::MAX / BYTES_PER_FRAME) + 1,
        // `count` itself exceeds u32.
        u64::from(u32::MAX) + 1,
        u64::MAX,
    ] {
        let error = wav::sizes(too_many).unwrap_err();
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "frames = {too_many}"
        );
        assert_eq!(error.to_string(), "a WAV file holds at most 4 GiB");
    }
}
