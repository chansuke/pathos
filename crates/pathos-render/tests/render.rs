//! The properties ADR 15 asks of the render: silence where no note sounds, an
//! onset at the scheduled sample, and the same output on every run and
//! platform; the track volume and mute of ADR 20; and the stereo pan of
//! ADR 21.

use std::num::NonZeroU32;

use pathos_core::{Command, Edit, Event, History, MAX_TICK, PPQ, Project};
use pathos_render::{Frame, Score, render};
use proptest::prelude::*;

/// At 120 BPM, 960 ticks last half a second.
const SECOND: u64 = 1920;

#[derive(Clone, Copy, Debug)]
struct NoteSpec {
    track: u64,
    start: u64,
    length: u64,
    pitch: u64,
    velocity: u64,
}

/// A project with two tracks, one clip of three seconds on each, and the
/// notes that fit. Notes that a rule rejects, such as overlapping ones, are
/// left out.
fn project(notes: &[NoteSpec]) -> Project {
    let mut history = History::default();
    for track in 0..2 {
        history.edit(Edit::Apply(Command::AddTrack)).unwrap();
        history
            .edit(Edit::Apply(Command::AddClip {
                track,
                start: 0,
                length: 3 * SECOND,
            }))
            .unwrap();
    }
    for note in notes {
        let _ = history.edit(Edit::Apply(Command::AddNote {
            id: note.track,
            start: note.start,
            length: note.length,
            pitch: note.pitch,
            velocity: note.velocity,
        }));
    }
    history.present().clone()
}

/// The render of a project whose tracks are all centered, as one channel: both
/// channels are equal, bit for bit.
fn mono(project: &Project, rate: NonZeroU32, frames: usize) -> Vec<f32> {
    render(project, rate, frames)
        .into_iter()
        .map(|[left, right]| {
            assert_eq!(left.to_bits(), right.to_bits(), "a centered track");
            left
        })
        .collect()
}

/// The frames from the note-on of each note up to its note-off.
fn spans(project: &Project, rate: NonZeroU32) -> Vec<(usize, usize)> {
    let mut on = std::collections::BTreeMap::new();
    let mut spans = Vec::new();
    for event in project.schedule(rate.get().into()) {
        match event {
            Event::On {
                sample,
                track,
                pitch,
                ..
            } => {
                on.insert((track, pitch), sample as usize);
            }
            Event::Off {
                sample,
                track,
                pitch,
            } => spans.push((on.remove(&(track, pitch)).unwrap(), sample as usize)),
        }
    }
    spans
}

fn note() -> impl Strategy<Value = NoteSpec> {
    (0u64..2, 0..3 * SECOND, 1..SECOND, 0u64..128, 1u64..128).prop_map(
        |(track, start, length, pitch, velocity)| NoteSpec {
            track,
            start,
            length,
            pitch,
            velocity,
        },
    )
}

fn rate() -> impl Strategy<Value = NonZeroU32> {
    prop_oneof![
        Just(44_100),
        Just(48_000),
        Just(96_000),
        100u32..8000,
        Just(1)
    ]
    .prop_map(|rate| NonZeroU32::new(rate).unwrap())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// No frame outside every note sounds.
    #[test]
    fn silence_where_no_note_sounds(notes in prop::collection::vec(note(), 0..8), rate in rate()) {
        let project = project(&notes);
        // Past the end of the clips, so that the tail is checked too.
        let frames = 4 * rate.get() as usize;
        let out = mono(&project, rate, frames);
        let spans = spans(&project, rate);
        for (frame, sample) in out.iter().enumerate() {
            if !spans.iter().any(|&(on, off)| on <= frame && frame < off) {
                prop_assert_eq!(sample.to_bits(), 0.0f32.to_bits(), "frame {}", frame);
            }
        }
    }

    /// A note sounds from the sample the schedule gives it.
    #[test]
    fn a_note_starts_at_its_scheduled_sample(note in note(), rate in rate()) {
        let project = project(&[note]);
        let out = mono(&project, rate, 4 * rate.get() as usize);
        let first = out.iter().position(|&sample| sample != 0.0);
        let on = spans(&project, rate).first().map(|&(on, _)| on);
        prop_assert_eq!(first, on);
    }

    /// A note never exceeds its level, its velocity over 127, even when its
    /// frequency is above the sample rate.
    #[test]
    fn a_note_stays_within_its_level(note in note(), rate in rate()) {
        let out = mono(&project(&[note]), rate, 4 * rate.get() as usize);
        let level = note.velocity as f32 / 127.0;
        for (frame, sample) in out.iter().enumerate() {
            prop_assert!(sample.abs() <= level, "frame {}: {}", frame, sample);
        }
    }

    /// Blocks rendered one after another, as playback renders them, join into
    /// the render of the whole (ADR 18).
    #[test]
    fn blocks_join_into_the_render(
        notes in prop::collection::vec(note(), 0..8),
        rate in rate(),
        sizes in prop::collection::vec(1usize..5000, 1..40),
    ) {
        let project = project(&notes);
        let frames = 4 * rate.get() as usize;
        let whole = render(&project, rate, frames);
        let score = Score::new(&project, rate);
        let mut joined = Vec::new();
        let mut block = Vec::new();
        for size in sizes.iter().cycle() {
            if joined.len() >= frames {
                break;
            }
            // Leftover samples in the buffer must not leak into the block.
            block.clear();
            block.resize((*size).min(frames - joined.len()), [1.0; 2]);
            score.render(joined.len() as u64, &mut block);
            joined.extend_from_slice(&block);
        }
        let bits = |frames: &[Frame]| frames.iter().flatten().map(|sample| sample.to_bits()).collect::<Vec<_>>();
        prop_assert_eq!(bits(&joined), bits(&whole));
    }

    /// Notes on different tracks add up: the render of both is the sum of the
    /// render of each.
    #[test]
    fn notes_add_up(a in note(), b in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let b = NoteSpec { track: 1, ..b };
        let frames = 4 * rate.get() as usize;
        let both = mono(&project(&[a, b]), rate, frames);
        let a = mono(&project(&[a]), rate, frames);
        let b = mono(&project(&[b]), rate, frames);
        for frame in 0..frames {
            prop_assert_eq!(both[frame].to_bits(), (a[frame] + b[frame]).to_bits(), "frame {}", frame);
        }
    }
}

/// `project` with `command` applied, which must be accepted.
fn with(project: Project, command: Command) -> Project {
    project.apply(0, command).unwrap().0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A track at volume 500 plays at exactly half the level, and a track's
    /// volume leaves the other track alone.
    #[test]
    fn a_track_plays_at_its_volume(a in note(), b in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let b = NoteSpec { track: 1, ..b };
        let frames = 4 * rate.get() as usize;
        let full = project(&[a]);
        let half = with(full.clone(), Command::SetTrackVolume { track: 0, volume: 500 });
        let (full, half) = (mono(&full, rate, frames), mono(&half, rate, frames));
        for frame in 0..frames {
            prop_assert_eq!(half[frame].to_bits(), (full[frame] * 0.5).to_bits(), "frame {}", frame);
        }
        let both = project(&[a, b]);
        let other = with(both.clone(), Command::SetTrackVolume { track: 0, volume: 0 });
        prop_assert_eq!(mono(&other, rate, frames), mono(&project(&[b]), rate, frames));
    }

    /// A track panned hard left or right sounds in that channel only, at the
    /// level it has at the center; between the center and a side, the far
    /// channel falls linearly.
    #[test]
    fn a_track_sounds_where_it_is_panned(a in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let frames = 4 * rate.get() as usize;
        let centered = mono(&project(&[a]), rate, frames);
        let pan = |pan| render(&with(project(&[a]), Command::SetTrackPan { track: 0, pan }), rate, frames);
        let (left, right, quarter) = (pan(0), pan(200), pan(150));
        for frame in 0..frames {
            let at = centered[frame].to_bits();
            prop_assert_eq!(left[frame].map(f32::to_bits), [at, 0], "frame {}", frame);
            prop_assert_eq!(right[frame].map(f32::to_bits), [0, at], "frame {}", frame);
            let half = (centered[frame] * 0.5).to_bits();
            prop_assert_eq!(quarter[frame].map(f32::to_bits), [half, at], "frame {}", frame);
        }
    }

    /// A soloed track is the only one heard.
    #[test]
    fn a_solo_silences_the_other_tracks(a in note(), b in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let b = NoteSpec { track: 1, ..b };
        let frames = 4 * rate.get() as usize;
        let soloed = with(project(&[a, b]), Command::SetTrackSolo { track: 1, solo: true });
        prop_assert_eq!(mono(&soloed, rate, frames), mono(&project(&[b]), rate, frames));
    }

    /// A muted track is silent, whatever its volume.
    #[test]
    fn a_muted_track_is_silent(a in note(), volume in 0u64..=2000, rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let frames = 4 * rate.get() as usize;
        let loud = with(project(&[a]), Command::SetTrackVolume { track: 0, volume });
        let muted = with(loud, Command::SetTrackMute { track: 0, muted: true });
        prop_assert!(render(&muted, rate, frames).iter().flatten().all(|sample| *sample == 0.0));
    }
}

/// A4 completes 440 cycles in a second and A5 880: the triangle wave falls
/// through zero once a cycle.
#[test]
fn a_note_plays_at_its_pitch() {
    let rate = NonZeroU32::new(48_000).unwrap();
    for (pitch, cycles) in [(69, 440), (81, 880), (57, 220)] {
        let note = NoteSpec {
            track: 0,
            start: 0,
            length: SECOND,
            pitch,
            velocity: 127,
        };
        let out = mono(&project(&[note]), rate, 48_000);
        let falls = out
            .windows(2)
            .filter(|pair| pair[0] > 0.0 && pair[1] <= 0.0)
            .count();
        assert_eq!(falls, cycles, "pitch {pitch}");
    }
}

/// The render of a fixed project, hashed bit for bit. CI runs this on each
/// platform it builds, so a render that depends on the platform fails here.
/// A deliberate change to the render changes the hash, and the new value is
/// written here with the reason in the commit.
#[test]
fn a_render_is_the_same_everywhere() {
    let notes = [
        (0, 0, 960, 60, 100),
        (0, 480, 960, 64, 90),
        (1, 960, 1920, 67, 127),
        (1, 3000, 240, 127, 1),
        (0, 4000, 1000, 0, 64),
    ]
    .map(|(track, start, length, pitch, velocity)| NoteSpec {
        track,
        start,
        length,
        pitch,
        velocity,
    });
    let project = project(&notes);
    let rate = NonZeroU32::new(48_000).unwrap();
    let hash = fnv1a(&mono(&project, rate, 192_000));
    assert_eq!(hash, 0x615c_6c63_2423_a304, "render hash {hash:#018x}");
    // Track 0 left of the center and track 1 hard right, at other volumes.
    let mut panned = project;
    for command in [
        Command::SetTrackPan { track: 0, pan: 40 },
        Command::SetTrackVolume {
            track: 0,
            volume: 700,
        },
        Command::SetTrackPan { track: 1, pan: 200 },
        Command::SetTrackVolume {
            track: 1,
            volume: 1300,
        },
    ] {
        panned = with(panned, command);
    }
    let samples: Vec<f32> = render(&panned, rate, 192_000)
        .into_iter()
        .flatten()
        .collect();
    let hash = fnv1a(&samples);
    assert_eq!(
        hash, 0x62cf_1d09_65ae_8555,
        "panned render hash {hash:#018x}"
    );
}

/// FNV-1a over the bits of every sample.
fn fnv1a(samples: &[f32]) -> u64 {
    samples
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, sample| {
            sample
                .to_bits()
                .to_le_bytes()
                .iter()
                .fold(hash, |hash, &byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
                })
        })
}

/// Rendering twice gives the same samples.
#[test]
fn a_render_is_the_same_every_time() {
    let note = NoteSpec {
        track: 0,
        start: 100,
        length: 1000,
        pitch: 60,
        velocity: 100,
    };
    let rate = NonZeroU32::new(44_100).unwrap();
    let first = mono(&project(&[note]), rate, 88_200);
    let second = mono(&project(&[note]), rate, 88_200);
    assert!(
        first
            .iter()
            .zip(&second)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
}

/// A loop's frames follow the tempo, as the notes do (ADR 22).
#[test]
fn a_loop_follows_the_tempo() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let fast = project(&[]);
    // From the second beat to the third: at 120 BPM from half a second to a
    // second, at 60 BPM from one second to two.
    assert_eq!(
        Score::new(&fast, rate)
            .looped(SECOND / 2..SECOND)
            .looped_frames(),
        Some(24_000..48_000)
    );
    let slow = with(
        fast,
        Command::SetTempo {
            tick: 0,
            us_per_quarter: 1_000_000,
        },
    );
    assert_eq!(
        Score::new(&slow, rate)
            .looped(SECOND / 2..SECOND)
            .looped_frames(),
        Some(48_000..96_000)
    );
}

/// The frames from `start` of `score`.
fn play(score: &Score, start: u64, frames: usize) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; frames];
    score.render(start, &mut out);
    out
}

/// The frames that are not silent in either channel.
fn sounding(frames: &[Frame]) -> Vec<bool> {
    frames
        .iter()
        .map(|frame| frame.iter().any(|&sample| sample != 0.0))
        .collect()
}

/// A click of the metronome lasts 20 ms.
const CLICK: usize = 960;

/// Checks that `frames`, rendered from frame 0, sound only in the 20 ms from
/// each of `beats`, and from their first frame.
fn assert_clicks(frames: &[Frame], beats: &[usize]) {
    let sounding = sounding(frames);
    for (frame, &sounds) in sounding.iter().enumerate() {
        if !beats
            .iter()
            .any(|&beat| (beat..beat + CLICK).contains(&frame))
        {
            assert!(!sounds, "frame {frame} is silent");
        }
    }
    for &beat in beats {
        assert!(sounding[beat], "a click starts on frame {beat}");
    }
}

/// The metronome clicks on every beat and is silent between (ADR 23).
#[test]
fn the_metronome_clicks_on_each_beat() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let score = Score::new(&project(&[]), rate).with_metronome();
    // At 120 BPM a beat is 24 000 frames.
    assert_clicks(
        &play(&score, 0, 100_000),
        &[0, 24_000, 48_000, 72_000, 96_000],
    );
}

/// The clicks follow the tempo, as the notes do.
#[test]
fn the_metronome_follows_the_tempo() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let project = with(
        project(&[]),
        Command::SetTempo {
            tick: SECOND,
            us_per_quarter: 1_000_000,
        },
    );
    let score = Score::new(&project, rate).with_metronome();
    // 60 BPM from the third beat: a beat is then 48 000 frames.
    assert_clicks(
        &play(&score, 0, 150_000),
        &[0, 24_000, 48_000, 96_000, 144_000],
    );
}

/// The first beat of a bar clicks differently from the other beats, and the
/// same on every bar.
#[test]
fn the_metronome_marks_the_first_beat_of_a_bar() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let score = Score::new(&project(&[]), rate).with_metronome();
    let click = |beat: u64| fnv1a(&play(&score, beat * 24_000, CLICK).concat());
    assert_eq!(click(0), click(4));
    assert_eq!(click(1), click(2));
    assert_ne!(click(0), click(1));
}

/// The metronome adds its clicks to the notes, and rendering in blocks gives
/// the same frames as rendering at once, also when a block starts within a
/// click.
#[test]
fn the_metronome_adds_to_the_notes_in_any_block() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let notes = project(&[NoteSpec {
        track: 0,
        start: 0,
        length: SECOND,
        pitch: 60,
        velocity: 100,
    }]);
    let score = Score::new(&notes, rate).with_metronome();
    let clicks = play(&Score::new(&project(&[]), rate).with_metronome(), 0, 60_000);
    let sum: Vec<Frame> = render(&notes, rate, 60_000)
        .iter()
        .zip(&clicks)
        .map(|(note, click)| [note[0] + click[0], note[1] + click[1]])
        .collect();
    assert_eq!(
        fnv1a(&play(&score, 0, 60_000).concat()),
        fnv1a(&sum.concat())
    );
    for block in [1, 7, 441, 5000] {
        let start = 24_500;
        let mut frames = Vec::new();
        while frames.len() < 30_000 {
            frames.extend(play(&score, (start + frames.len()) as u64, block));
        }
        frames.truncate(30_000);
        assert_eq!(
            fnv1a(&frames.concat()),
            fnv1a(&sum[start..start + 30_000].concat()),
            "blocks of {block}"
        );
    }
}

/// The last beat that clicks is the last tick a clip may reach.
#[test]
fn the_metronome_ends_at_the_last_tick() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let project = project(&[]);
    let score = Score::new(&project, rate).with_metronome();
    let last = project.tempo_map().sample_of(48_000, MAX_TICK / PPQ * PPQ) as u64;
    let frames = play(&score, last - 24_000, 60_000);
    let sounding = sounding(&frames);
    assert!(sounding[0] && sounding[24_000], "the last two beats click");
    assert!(
        !sounding[48_000..].contains(&true),
        "no beat after the last"
    );
}

/// A beat cuts the click of the beat before it, so clicks never add up, even
/// at the fastest tempo, where beats are closer than a frame (ADR 23).
#[test]
fn the_metronome_clicks_one_beat_at_a_time() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let project = with(
        project(&[]),
        Command::SetTempo {
            tick: 0,
            us_per_quarter: 1,
        },
    );
    let score = Score::new(&project, rate).with_metronome();
    let frames = play(&score, 0, 4096);
    let single = play(
        &Score::new(&self::project(&[]), rate).with_metronome(),
        0,
        CLICK,
    );
    let loudest = |frames: &[Frame]| {
        frames
            .iter()
            .flatten()
            .fold(0.0_f32, |loudest, sample| loudest.max(sample.abs()))
    };
    assert!(
        loudest(&frames) <= loudest(&single),
        "no louder than one click"
    );
}

/// The first beat of a bar clicks an A6 and the other beats an A5: in 20 ms
/// the triangle wave falls through zero about 35 and 17.6 times.
#[test]
fn the_metronome_clicks_at_its_pitches() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let score = Score::new(&project(&[]), rate).with_metronome();
    let falls = |beat: u64| {
        play(&score, beat * 24_000, CLICK)
            .windows(2)
            .filter(|pair| pair[0][0] > 0.0 && pair[1][0] <= 0.0)
            .count()
    };
    assert_eq!((falls(0), falls(1)), (35, 18));
}

/// The last second of an hour-long note above the rate, hashed bit for bit.
/// Such a note advances more than a whole cycle each frame, and only the
/// fraction may enter its phase, or the phase loses bits as the note goes on.
#[test]
fn a_long_note_above_the_rate_is_the_same_everywhere() {
    const HOUR: u64 = 3600 * SECOND;
    let mut history = History::default();
    for command in [
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 0,
            length: HOUR,
        },
        Command::AddNote {
            id: 0,
            start: 0,
            length: HOUR,
            pitch: 127,
            velocity: 100,
        },
    ] {
        history.edit(Edit::Apply(command)).unwrap();
    }
    let rate = NonZeroU32::new(8000).unwrap();
    let last = play(&Score::new(history.present(), rate), 3599 * 8000, 8000);
    let hash = fnv1a(&last.concat());
    assert_eq!(hash, 0x99f8_febd_fd0a_58f5, "render hash {hash:#018x}");
}
