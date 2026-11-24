//! The engine plays the offline render (ADR 15, ADR 18) and follows the
//! transport.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]
#![cfg(not(loom))]

use std::num::NonZeroU32;
use std::sync::Arc;

use pathos_core::{Command, Edit, History, PPQ};
use pathos_playback::engine::{Engine, Transport};
use pathos_playback::handoff::{Publisher, handoff};
use pathos_render::{Frame, Score, render};

const RATE: NonZeroU32 = NonZeroU32::new(8000).unwrap();
const CHANNELS: usize = 2;

/// One clip of a bar with a note of `pitch` on each beat.
fn project(pitch: u64) -> pathos_core::Project {
    let mut history = History::default();
    let mut commands = vec![
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 4 * PPQ,
        },
    ];
    commands.extend((0..4).map(|beat| Command::AddNote {
        id: 0,
        start: beat * PPQ,
        length: PPQ / 2,
        pitch,
        velocity: 100,
    }));
    for command in commands {
        history.edit(Edit::Apply(command)).unwrap();
    }
    history.present().clone()
}

fn engine() -> (Publisher<Score>, Arc<Transport>, Engine) {
    let (publisher, player) = handoff();
    let transport = Arc::new(Transport::default());
    let engine = Engine::new(player, transport.clone());
    (publisher, transport, engine)
}

/// Fills `frames` frames of `channels` samples in blocks of `block` frames.
fn play_on(engine: &mut Engine, frames: usize, block: usize, channels: usize) -> Vec<f32> {
    let mut played = Vec::new();
    let mut out = vec![1.0; block * channels];
    while played.len() < frames * channels {
        let size = block.min(frames - played.len() / channels);
        let out = &mut out[..size * channels];
        engine.fill(out, channels);
        played.extend_from_slice(out);
    }
    played
}

/// Fills `frames` stereo frames in blocks of `block` frames.
fn play(engine: &mut Engine, frames: usize, block: usize) -> Vec<Frame> {
    play_on(engine, frames, block, CHANNELS)
        .chunks(CHANNELS)
        .map(|frame| [frame[0], frame[1]])
        .collect()
}

fn bits(frames: &[Frame]) -> Vec<u32> {
    frames
        .iter()
        .flatten()
        .map(|sample| sample.to_bits())
        .collect()
}

#[test]
fn playback_is_the_offline_render() {
    let project = project(60);
    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project, RATE));
    transport.play();
    let frames = 2 * RATE.get() as usize;
    // 5000 frames is longer than the engine renders at once.
    for block in [1, 64, 441, 5000] {
        transport.seek(0);
        let played = play(&mut engine, frames, block);
        assert_eq!(
            bits(&played),
            bits(&render(&project, RATE, frames)),
            "block {block}"
        );
        assert_eq!(transport.position(), frames as u64);
    }
}

#[test]
fn nothing_sounds_before_a_score_or_while_stopped() {
    let (publisher, transport, mut engine) = engine();
    transport.play();
    assert!(
        play(&mut engine, 1000, 100)
            .iter()
            .flatten()
            .all(|&sample| sample == 0.0)
    );
    assert_eq!(transport.position(), 0);

    publisher.publish(Score::new(&project(60), RATE));
    transport.stop();
    assert!(
        play(&mut engine, 1000, 100)
            .iter()
            .flatten()
            .all(|&sample| sample == 0.0)
    );
    assert_eq!(transport.position(), 0);
}

#[test]
fn playback_continues_from_a_seek() {
    let project = project(60);
    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project, RATE));
    transport.play();
    play(&mut engine, 300, 100);
    transport.seek(3000);
    let played = play(&mut engine, 2000, 100);
    let whole = render(&project, RATE, 5000);
    assert_eq!(bits(&played), bits(&whole[3000..]));
    assert_eq!(transport.position(), 5000);
}

#[test]
fn an_edit_is_heard_from_the_next_block() {
    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project(60), RATE));
    transport.play();
    let before = play(&mut engine, 1000, 100);
    publisher.publish(Score::new(&project(72), RATE));
    publisher.collect();
    let after = play(&mut engine, 1000, 100);
    assert_eq!(bits(&before), bits(&render(&project(60), RATE, 1000)));
    assert_eq!(
        bits(&after),
        bits(&render(&project(72), RATE, 2000)[1000..])
    );
}

/// A device with one channel gets the mean of both channels, and one with more
/// than two gets silence beyond the second (ADR 21).
#[test]
fn a_device_gets_both_channels_whatever_its_channels() {
    let project = project(60)
        .apply(0, Command::SetTrackPan { track: 0, pan: 150 })
        .unwrap()
        .0;
    let whole = render(&project, RATE, 6000);
    assert!(whole.iter().any(|[left, right]| left != right), "panned");
    // A block of 5000 frames is longer than the engine renders at once.
    for (channels, block) in [(1, 100), (1, 5000), (4, 100), (4, 5000)] {
        let (publisher, transport, mut engine) = engine();
        publisher.publish(Score::new(&project, RATE));
        transport.play();
        let played = play_on(&mut engine, 6000, block, channels);
        let expected: Vec<f32> = whole
            .iter()
            .flat_map(|&[left, right]| match channels {
                1 => vec![(left + right) * 0.5],
                _ => vec![left, right, 0.0, 0.0],
            })
            .collect();
        let bits = |samples: &[f32]| samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&played),
            bits(&expected),
            "{channels} channels, block {block}"
        );
    }
}

/// Plays `score` from frame `from` for `frames` frames in blocks of `block`.
fn play_score(score: Score, from: u64, frames: usize, block: usize) -> Vec<Frame> {
    let (publisher, transport, mut engine) = engine();
    publisher.publish(score);
    transport.seek(from);
    transport.play();
    play(&mut engine, frames, block)
}

/// A looped playback is the render of the loop's frames over and over, exact
/// to the frame wherever the blocks fall (ADR 22).
#[test]
fn a_loop_repeats_its_frames() {
    let project = project(60);
    // Beats 1 and 2: at 120 BPM and 8000 Hz, frames 4000 up to 12000.
    let score = || Score::new(&project, RATE).looped(PPQ..3 * PPQ);
    assert_eq!(score().looped_frames(), Some(4000..12_000));
    let whole = render(&project, RATE, 13_000);
    let mut expected = whole[..12_000].to_vec();
    while expected.len() < 40_000 {
        expected.extend_from_slice(&whole[4000..12_000]);
    }
    expected.truncate(40_000);
    for block in [1, 64, 441, 5000] {
        let played = play_score(score(), 0, 40_000, block);
        assert_eq!(bits(&played), bits(&expected), "block {block}");
    }
    // Inside the loop, playback starts where it is.
    let played = play_score(score(), 6000, 10_000, 441);
    let mut expected = whole[6000..12_000].to_vec();
    expected.extend_from_slice(&whole[4000..8000]);
    assert_eq!(bits(&played), bits(&expected));
}

/// Playback after the end of the loop runs on, and the playhead reports the
/// frame after the loop returns to its start.
#[test]
fn playback_after_a_loop_runs_on_and_the_playhead_returns_within_it() {
    let project = project(60);
    let whole = render(&project, RATE, 20_000);
    let played = play_score(
        Score::new(&project, RATE).looped(PPQ..3 * PPQ),
        12_000,
        8000,
        100,
    );
    assert_eq!(bits(&played), bits(&whole[12_000..20_000]));

    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project, RATE).looped(PPQ..3 * PPQ));
    transport.seek(11_000);
    transport.play();
    play(&mut engine, 1500, 1500);
    assert_eq!(transport.position(), 4500);
}

/// An empty range does not loop.
#[test]
fn an_empty_range_does_not_loop() {
    assert_eq!(
        Score::new(&project(60), RATE)
            .looped(PPQ..PPQ)
            .looped_frames(),
        None
    );
    assert_eq!(Score::new(&project(60), RATE).looped_frames(), None);
}

/// The loudest sample of each channel of `frames`.
fn peaks(frames: &[Frame]) -> Frame {
    frames.iter().fold([0.0; 2], |[left, right], frame| {
        [left.max(frame[0].abs()), right.max(frame[1].abs())]
    })
}

/// The meter reports the loudest sample of each channel played since it was
/// last read, and nothing once read (ADR 25).
#[test]
fn the_meter_reports_the_peak_of_each_channel_since_it_was_read() {
    let project = project(60)
        .apply(0, Command::SetTrackPan { track: 0, pan: 150 })
        .unwrap()
        .0;
    let whole = render(&project, RATE, 6000);
    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project, RATE));
    assert_eq!(transport.take_peaks(), [0.0; 2], "before playback");
    transport.play();
    play(&mut engine, 3000, 100);
    let first = transport.take_peaks();
    assert_eq!(first, peaks(&whole[..3000]));
    assert!(first[0] < first[1], "panned right");
    assert_eq!(transport.take_peaks(), [0.0; 2], "once read");
    // The first beat's note ends at frame 2000, so these blocks are silent.
    play(&mut engine, 1000, 100);
    let silence = transport.take_peaks();
    assert_eq!(silence, peaks(&whole[3000..4000]));
    play(&mut engine, 2000, 5000);
    assert_eq!(transport.take_peaks(), peaks(&whole[4000..6000]));
}

/// The meter shows the song: the metronome's clicks do not move it.
#[test]
fn the_meter_leaves_out_the_metronome() {
    let project = History::default().present().clone();
    let (publisher, transport, mut engine) = engine();
    publisher.publish(Score::new(&project, RATE).with_metronome());
    transport.play();
    let played = play(&mut engine, 1000, 100);
    assert!(peaks(&played)[0] > 0.0, "a click sounds");
    assert_eq!(transport.take_peaks(), [0.0; 2]);
}
