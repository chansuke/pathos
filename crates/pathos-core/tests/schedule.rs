//! Expected schedules written down by hand, so that a mistake shared by this
//! crate and the model does not go unnoticed.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

use pathos_core::{Command, Edit, Event, History};

fn apply(history: &mut History, command: Command) {
    history.edit(Edit::Apply(command)).unwrap();
}

fn add_note(history: &mut History, id: u64, start: u64, length: u64, pitch: u64, velocity: u64) {
    apply(
        history,
        Command::AddNote {
            id,
            start,
            length,
            pitch,
            velocity,
        },
    );
}

/// At 120 BPM a quarter note of 960 ticks lasts half a second, 24 000 samples
/// at 48 kHz. A note-off comes before a note-on at the same sample.
#[test]
fn notes_start_and_stop_at_their_samples() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 960,
            length: 960,
        },
    );
    add_note(&mut history, 0, 0, 480, 60, 100);
    add_note(&mut history, 0, 480, 480, 60, 90);

    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 24_000,
                track: 0,
                pitch: 60,
                velocity: 100,
            },
            Event::Off {
                sample: 36_000,
                track: 0,
                pitch: 60,
            },
            Event::On {
                sample: 36_000,
                track: 0,
                pitch: 60,
                velocity: 90,
            },
            Event::Off {
                sample: 48_000,
                track: 0,
                pitch: 60,
            },
        ]
    );
}

/// A tick lies at the nearest sample, and an exact half rounds up (ADR 32).
/// At 44.1 kHz and 120 BPM a tick lasts 22.96875 samples.
#[test]
fn a_tick_rounds_to_the_nearest_sample() {
    let tempo = History::default().present().tempo_map();
    // 344.53 samples.
    assert_eq!(tempo.sample_of(44_100, 15), 345);
    // 367.5 samples.
    assert_eq!(tempo.sample_of(44_100, 16), 368);
    // 390.47 samples.
    assert_eq!(tempo.sample_of(44_100, 17), 390);
}

/// A note shorter than a sample is not heard.
#[test]
fn a_note_within_one_sample_has_no_events() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 960,
        },
    );
    add_note(&mut history, 0, 0, 480, 60, 100);
    // At 1 Hz both ends of the note, a quarter of a second long, round to the
    // first sample.
    assert_eq!(history.present().schedule(1), []);
    // At 2 Hz its end rounds up to the second sample, so it covers the first.
    assert_eq!(history.present().schedule(2).len(), 2);
}

/// Across two tempo changes (120 BPM on `[0, 480)`, 100 BPM on `[480, 1440)`,
/// 150 BPM from 1440 onwards), `tick_to_micros` rounds down and `sample_of`
/// rounds once to the nearest sample with exact halves rounding up (ADR 32).
#[test]
fn multi_tempo_tick_to_micros_and_sample_of() {
    let mut history = History::default();
    apply(&mut history, Command::AddTrack);
    apply(
        &mut history,
        Command::SetTempo {
            tick: 480,
            us_per_quarter: 600_000,
        },
    );
    apply(
        &mut history,
        Command::SetTempo {
            tick: 1_440,
            us_per_quarter: 400_000,
        },
    );
    apply(
        &mut history,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 2_400,
        },
    );
    // Note spans both tempo changes: starts at tick 488, ends at tick 1444.
    add_note(&mut history, 0, 488, 956, 69, 127);

    let project = history.present();
    let tempo = project.tempo_map();

    // Segment 0: [0, 480) at 500 000 us/quarter (0.25 s total).
    // Segment 1: [480, 1440) at 600 000 us/quarter (0.60 s total, 27.5625 samples/tick @ 44.1 kHz).
    // Segment 2: [1440, ..) at 400 000 us/quarter (18.375 samples/tick @ 44.1 kHz, 20 samples/tick @ 48 kHz).
    let expected: &[(u64, u128, u128, u128)] = &[
        // (tick, micros_floor, sample_48k, sample_44_1k)
        (0, 0, 0, 0),
        (480, 250_000, 12_000, 11_025),
        // 0.25 s + 8/960 * 0.6 s = 0.255 s -> 11 245.5 samples at 44.1 kHz (half up -> 11 246).
        (488, 255_000, 12_240, 11_246),
        // 0.25 s + 0.60 s = 0.85 s.
        (1_440, 850_000, 40_800, 37_485),
        // 0.85 s + 400 000/960 us = 850 416.666... us; exact 40 820 @ 48 kHz, 37 503.375 @ 44.1 kHz.
        (1_441, 850_416, 40_820, 37_503),
        // 0.85 s + 3 * 18.375 = 37 540.125 @ 44.1 kHz.
        (1_443, 851_250, 40_860, 37_540),
        // 0.85 s + 4 * 18.375 = 37 558.5 @ 44.1 kHz (exact half -> 37 559).
        (1_444, 851_666, 40_880, 37_559),
        // 0.85 s + 5 * 18.375 = 37 576.875 @ 44.1 kHz.
        (1_445, 852_083, 40_900, 37_577),
        // 0.25 s + 0.60 s + 0.40 s = 1.25 s (exercises whole-second + remainder split).
        (2_400, 1_250_000, 60_000, 55_125),
        // 1.25 s + 4 * 18.375 = 55 198.5 @ 44.1 kHz (exact half -> 55 199).
        (2_404, 1_251_666, 60_080, 55_199),
    ];

    for &(tick, micros, sample_48k, sample_44_1k) in expected {
        assert_eq!(
            project.tick_to_micros(tick),
            micros,
            "tick_to_micros({tick})"
        );
        assert_eq!(tempo.micros(tick), micros, "TempoMap::micros({tick})");
        assert_eq!(
            tempo.sample_of(48_000, tick),
            sample_48k,
            "sample_of(48_000, {tick})"
        );
        assert_eq!(
            tempo.sample_of(44_100, tick),
            sample_44_1k,
            "sample_of(44_100, {tick})"
        );
    }

    assert_eq!(
        project.schedule(44_100),
        [
            Event::On {
                sample: 11_246,
                track: 0,
                pitch: 69,
                velocity: 127,
            },
            Event::Off {
                sample: 37_559,
                track: 0,
                pitch: 69,
            },
        ]
    );

    // Removing the middle tempo change lets 120 BPM run all the way to tick 1440
    // (1.5 quarters = 0.75 s = 36 000 samples @ 48 kHz, 33 075 @ 44.1 kHz).
    apply(&mut history, Command::RemoveTempo { tick: 480 });
    let after = history.present();
    assert_eq!(after.tick_to_micros(1_440), 750_000);
    assert_eq!(after.tempo_map().sample_of(48_000, 1_440), 36_000);
    assert_eq!(after.tempo_map().sample_of(44_100, 1_440), 33_075);
}

/// Mute silences a track, solo silences every non-soloed track, and a track
/// that is both muted and soloed is silent while its solo still silences
/// non-soloed tracks (ADR 20, ADR 21).
#[test]
fn schedule_applies_mute_solo_and_muted_solo_precedence() {
    let mut history = History::default();
    let specs = [(0, 60, 100), (1, 64, 90), (2, 67, 80)];
    for (track, pitch, velocity) in specs {
        apply(&mut history, Command::AddTrack);
        apply(
            &mut history,
            Command::AddClip {
                track,
                start: 0,
                length: 960,
            },
        );
        add_note(&mut history, track, 0, 480, pitch, velocity);
    }

    // All three tracks heard: On events ordered by track at 0, Off events at 12 000.
    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 0,
                track: 0,
                pitch: 60,
                velocity: 100,
            },
            Event::On {
                sample: 0,
                track: 1,
                pitch: 64,
                velocity: 90,
            },
            Event::On {
                sample: 0,
                track: 2,
                pitch: 67,
                velocity: 80,
            },
            Event::Off {
                sample: 12_000,
                track: 0,
                pitch: 60,
            },
            Event::Off {
                sample: 12_000,
                track: 1,
                pitch: 64,
            },
            Event::Off {
                sample: 12_000,
                track: 2,
                pitch: 67,
            },
        ]
    );

    // Muting track 0 drops only track 0.
    apply(
        &mut history,
        Command::SetTrackMute {
            track: 0,
            muted: true,
        },
    );
    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 0,
                track: 1,
                pitch: 64,
                velocity: 90,
            },
            Event::On {
                sample: 0,
                track: 2,
                pitch: 67,
                velocity: 80,
            },
            Event::Off {
                sample: 12_000,
                track: 1,
                pitch: 64,
            },
            Event::Off {
                sample: 12_000,
                track: 2,
                pitch: 67,
            },
        ]
    );

    // Unmute track 0 and solo track 1: only track 1 sounds.
    apply(
        &mut history,
        Command::SetTrackMute {
            track: 0,
            muted: false,
        },
    );
    apply(
        &mut history,
        Command::SetTrackSolo {
            track: 1,
            solo: true,
        },
    );
    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 0,
                track: 1,
                pitch: 64,
                velocity: 90,
            },
            Event::Off {
                sample: 12_000,
                track: 1,
                pitch: 64,
            },
        ]
    );

    // Track 1 is muted AND soloed: mute wins on track 1, while solo still
    // silences tracks 0 and 2, so the whole schedule is empty.
    apply(
        &mut history,
        Command::SetTrackMute {
            track: 1,
            muted: true,
        },
    );
    assert_eq!(history.present().schedule(48_000), []);

    // Soloing track 2 alongside muted+soloed track 1 lets only track 2 sound.
    apply(
        &mut history,
        Command::SetTrackSolo {
            track: 2,
            solo: true,
        },
    );
    assert_eq!(
        history.present().schedule(48_000),
        [
            Event::On {
                sample: 0,
                track: 2,
                pitch: 67,
                velocity: 80,
            },
            Event::Off {
                sample: 12_000,
                track: 2,
                pitch: 67,
            },
        ]
    );
}
