//! The engine plays the offline render (ADR 15, ADR 18) and follows the
//! transport.

#![cfg(not(loom))]

use std::num::NonZeroU32;
use std::sync::Arc;

use pathos_core::{Command, Edit, History, PPQ};
use pathos_playback::engine::{Engine, Transport};
use pathos_playback::handoff::{Publisher, handoff};
use pathos_render::{Score, render};

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

/// Fills `frames` frames in blocks of `block` frames and returns the first
/// channel, checking that every channel is the same.
fn play(engine: &mut Engine, frames: usize, block: usize) -> Vec<f32> {
    let mut mono = Vec::new();
    let mut out = vec![1.0; block * CHANNELS];
    while mono.len() < frames {
        let size = block.min(frames - mono.len());
        let out = &mut out[..size * CHANNELS];
        engine.fill(out, CHANNELS);
        for frame in out.chunks(CHANNELS) {
            assert!(
                frame
                    .iter()
                    .all(|sample| sample.to_bits() == frame[0].to_bits())
            );
            mono.push(frame[0]);
        }
    }
    mono
}

fn bits(samples: &[f32]) -> Vec<u32> {
    samples.iter().map(|sample| sample.to_bits()).collect()
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
            .all(|&sample| sample == 0.0)
    );
    assert_eq!(transport.position(), 0);

    publisher.publish(Score::new(&project(60), RATE));
    transport.stop();
    assert!(
        play(&mut engine, 1000, 100)
            .iter()
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
