//! The properties ADR 15 asks of the render: silence where no note sounds, an
//! onset at the scheduled sample, and the same output on every run and
//! platform; the track volume and mute of ADR 20; and the stereo pan of
//! ADR 21.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

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

    /// At master volume 500 every track plays at exactly half the level
    /// (ADR 24).
    #[test]
    fn the_master_volume_scales_every_track(a in note(), b in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let b = NoteSpec { track: 1, ..b };
        let frames = 4 * rate.get() as usize;
        let full = project(&[a, b]);
        let half = with(full.clone(), Command::SetMasterVolume { volume: 500 });
        let (full, half) = (mono(&full, rate, frames), mono(&half, rate, frames));
        for frame in 0..frames {
            prop_assert_eq!(half[frame].to_bits(), (full[frame] * 0.5).to_bits(), "frame {}", frame);
        }
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
        let (left, right, left_quarter, right_quarter) = (pan(0), pan(200), pan(50), pan(150));
        for frame in 0..frames {
            let at = centered[frame].to_bits();
            prop_assert_eq!(left[frame].map(f32::to_bits), [at, 0], "frame {}", frame);
            prop_assert_eq!(right[frame].map(f32::to_bits), [0, at], "frame {}", frame);
            let half = (centered[frame] * 0.5).to_bits();
            prop_assert_eq!(left_quarter[frame].map(f32::to_bits), [at, half], "frame {}", frame);
            prop_assert_eq!(right_quarter[frame].map(f32::to_bits), [half, at], "frame {}", frame);
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

/// The master volume scales the notes, not the metronome's clicks (ADR 24).
#[test]
fn the_master_volume_leaves_the_metronome_alone() {
    let rate = NonZeroU32::new(48_000).unwrap();
    let clicks = |project: &Project| {
        fnv1a(&play(&Score::new(project, rate).with_metronome(), 0, 50_000).concat())
    };
    let silent = with(project(&[]), Command::SetMasterVolume { volume: 0 });
    assert_eq!(clicks(&silent), clicks(&project(&[])));
}

/// The frames from the note-on of each note up to its note-off, together with
/// the track that owns the note.
fn spans_with_tracks(project: &Project, rate: NonZeroU32) -> Vec<(u64, usize, usize)> {
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
            } => spans.push((track, on.remove(&(track, pitch)).unwrap(), sample as usize)),
        }
    }
    spans
}

#[derive(Clone, Debug)]
struct RichNote {
    start: u64,
    length: u64,
    pitch: u64,
    velocity: u64,
}

#[derive(Clone, Debug)]
struct RichClip {
    gap: u64,
    length: u64,
    notes: Vec<RichNote>,
}

#[derive(Clone, Debug)]
struct RichTrack {
    volume: u64,
    pan: u64,
    clips: Vec<RichClip>,
}

#[derive(Clone, Debug)]
struct TempoSpec {
    tick: u64,
    us_per_quarter: u64,
}

#[derive(Clone, Debug)]
struct RichProjectSpec {
    tempos: Vec<TempoSpec>,
    tracks: Vec<RichTrack>,
    master_volume: u64,
}

fn volume_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        1 => Just(0u64),
        3 => 1u64..1000,
        1 => Just(1000u64),
        3 => 1001u64..=2000,
    ]
}

fn pan_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![
        1 => Just(0u64),
        3 => 1u64..100,
        1 => Just(100u64),
        3 => 101u64..200,
        1 => Just(200u64),
    ]
}

fn tempo_spec() -> impl Strategy<Value = TempoSpec> {
    (
        prop_oneof![1 => Just(0u64), 3 => 1u64..3 * PPQ],
        200_000u64..=1_000_000,
    )
        .prop_map(|(tick, us_per_quarter)| TempoSpec {
            tick,
            us_per_quarter,
        })
}

fn rich_note() -> impl Strategy<Value = RichNote> {
    (0u64..PPQ, 1u64..=PPQ, 0u64..128, 1u64..128).prop_map(|(start, length, pitch, velocity)| {
        RichNote {
            start,
            length,
            pitch,
            velocity,
        }
    })
}

fn rich_clip() -> impl Strategy<Value = RichClip> {
    (
        prop_oneof![1 => Just(0u64), 3 => 1u64..PPQ / 3],
        120u64..=2 * PPQ / 3,
        prop::collection::vec(rich_note(), 1..=4),
    )
        .prop_map(|(gap, length, notes)| RichClip { gap, length, notes })
}

fn rich_track() -> impl Strategy<Value = RichTrack> {
    (
        volume_strategy(),
        pan_strategy(),
        prop::collection::vec(rich_clip(), 1..=3),
    )
        .prop_map(|(volume, pan, clips)| RichTrack { volume, pan, clips })
}

fn rich_project_spec() -> impl Strategy<Value = RichProjectSpec> {
    (
        prop::collection::vec(tempo_spec(), 1..=4),
        prop::collection::vec(rich_track(), 1..=3),
        volume_strategy(),
    )
        .prop_map(|(tempos, tracks, master_volume)| RichProjectSpec {
            tempos,
            tracks,
            master_volume,
        })
}

fn add_track_clips(history: &mut History, track: u64, clips: &[RichClip], next_clip_id: &mut u64) {
    let mut cursor = 0u64;
    for clip in clips {
        let start = cursor + clip.gap;
        let clip_id = *next_clip_id;
        *next_clip_id += 1;
        history
            .edit(Edit::Apply(Command::AddClip {
                track,
                start,
                length: clip.length,
            }))
            .unwrap();
        for note in &clip.notes {
            let note_start = note.start % clip.length;
            let max_len = clip.length - note_start;
            let note_length = note.length.min(max_len).max(1);
            let _ = history.edit(Edit::Apply(Command::AddNote {
                id: clip_id,
                start: note_start,
                length: note_length,
                pitch: note.pitch,
                velocity: note.velocity,
            }));
        }
        cursor = start + clip.length;
    }
}

fn build_rich_project(spec: &RichProjectSpec) -> Project {
    let mut history = History::default();
    for tempo in &spec.tempos {
        history
            .edit(Edit::Apply(Command::SetTempo {
                tick: tempo.tick,
                us_per_quarter: tempo.us_per_quarter,
            }))
            .unwrap();
    }
    history
        .edit(Edit::Apply(Command::SetMasterVolume {
            volume: spec.master_volume,
        }))
        .unwrap();
    let mut next_clip_id = 0u64;
    for (index, track) in spec.tracks.iter().enumerate() {
        let track_id = index as u64;
        history.edit(Edit::Apply(Command::AddTrack)).unwrap();
        history
            .edit(Edit::Apply(Command::SetTrackVolume {
                track: track_id,
                volume: track.volume,
            }))
            .unwrap();
        history
            .edit(Edit::Apply(Command::SetTrackPan {
                track: track_id,
                pan: track.pan,
            }))
            .unwrap();
        add_track_clips(&mut history, track_id, &track.clips, &mut next_clip_id);
    }
    history.present().clone()
}

/// Builds a centered, unity-volume single-track project containing only track
/// `track_index` of `spec`, sharing the same tempo map.
fn build_single_track_unity(spec: &RichProjectSpec, track_index: usize) -> Project {
    let mut history = History::default();
    for tempo in &spec.tempos {
        history
            .edit(Edit::Apply(Command::SetTempo {
                tick: tempo.tick,
                us_per_quarter: tempo.us_per_quarter,
            }))
            .unwrap();
    }
    history.edit(Edit::Apply(Command::AddTrack)).unwrap();
    let mut next_clip_id = 0u64;
    add_track_clips(
        &mut history,
        0,
        &spec.tracks[track_index].clips,
        &mut next_clip_id,
    );
    history.present().clone()
}

/// Expected frame length covering all scheduled spans plus a silent tail.
fn render_len(spans: &[(usize, usize)], rate: NonZeroU32) -> usize {
    spans.iter().map(|&(_, off)| off).max().unwrap_or(0) + (rate.get() as usize / 4).max(16)
}

/// The 5 ms linear attack/release envelope at `frame` for a note on `on..off`.
fn expected_envelope(frame: usize, on: usize, off: usize, rate: NonZeroU32) -> f32 {
    let ramp = (rate.get() / 200).max(1) as f32;
    ((frame - on + 1) as f32 / ramp)
        .min((off - frame) as f32 / ramp)
        .min(1.0)
}

fn common_rate() -> impl Strategy<Value = NonZeroU32> {
    prop_oneof![
        Just(8_000),
        Just(22_050),
        Just(44_100),
        Just(48_000),
        Just(96_000),
        1u32..8_000,
    ]
    .prop_map(|rate| NonZeroU32::new(rate).unwrap())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// In a project with tempo changes, multiple clips (including tick > 0),
    /// arbitrary track volumes/pans and master volume, no frame outside every
    /// scheduled note span sounds.
    #[test]
    fn rich_project_is_silent_where_no_note_sounds(
        spec in rich_project_spec(),
        rate in rate(),
    ) {
        let project = build_rich_project(&spec);
        let spans = spans(&project, rate);
        let frames = render_len(&spans, rate);
        let out = render(&project, rate, frames);
        for (frame, &[left, right]) in out.iter().enumerate() {
            if !spans.iter().any(|&(on, off)| on <= frame && frame < off) {
                prop_assert_eq!(left.to_bits(), 0.0f32.to_bits(), "left frame {}", frame);
                prop_assert_eq!(right.to_bits(), 0.0f32.to_bits(), "right frame {}", frame);
            }
        }
    }

    /// The first non-silent frame of a rich project matches the earliest
    /// scheduled note-on of a track with non-zero volume (when master volume is
    /// non-zero).
    #[test]
    fn rich_project_starts_at_its_scheduled_sample(
        spec in rich_project_spec(),
        rate in rate(),
    ) {
        let project = build_rich_project(&spec);
        let tracked = spans_with_tracks(&project, rate);
        let frames = tracked.iter().map(|&(_, _, off)| off).max().unwrap_or(0)
            + (rate.get() as usize / 4).max(16);
        let out = render(&project, rate, frames);
        let first = out.iter().position(|&[left, right]| left != 0.0 || right != 0.0);
        let expected = if project.master_volume() == 0 {
            None
        } else {
            tracked
                .iter()
                .filter(|&&(track, _, _)| project.track(track).is_some_and(|t| t.volume > 0))
                .map(|&(_, on, _)| on)
                .min()
        };
        prop_assert_eq!(first, expected);
    }

    /// Blocks rendered one after another join bit-for-bit into the whole render
    /// of a rich project with tempo changes, multiple clips, arbitrary gains,
    /// optional metronome and loop range.
    #[test]
    fn rich_project_blocks_join_into_the_render(
        spec in rich_project_spec(),
        rate in rate(),
        sizes in prop::collection::vec(1usize..4096, 1..40),
        metronome in any::<bool>(),
        loop_start in 0u64..2 * PPQ,
        loop_len in 0u64..PPQ,
    ) {
        let project = build_rich_project(&spec);
        let spans = spans(&project, rate);
        let frames = render_len(&spans, rate);
        let mut score = Score::new(&project, rate);
        if metronome {
            score = score.with_metronome();
        }
        let loop_end = loop_start + loop_len;
        let tempo = project.tempo_map();
        let expected_start = tempo.sample_of(rate.get().into(), loop_start) as u64;
        let expected_end = tempo.sample_of(rate.get().into(), loop_end) as u64;
        let expected_loop = (expected_start < expected_end).then_some(expected_start..expected_end);
        score = score.looped(loop_start..loop_end);
        prop_assert_eq!(score.looped_frames(), expected_loop);

        let whole = play(&score, 0, frames);
        let mut joined = Vec::with_capacity(frames);
        let mut block = Vec::new();
        for size in sizes.iter().cycle() {
            if joined.len() >= frames {
                break;
            }
            block.clear();
            block.resize((*size).min(frames - joined.len()), [1.0; 2]);
            score.render(joined.len() as u64, &mut block);
            joined.extend_from_slice(&block);
        }
        let bits = |frames: &[Frame]| {
            frames
                .iter()
                .flatten()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        };
        prop_assert_eq!(bits(&joined), bits(&whole));
    }

    /// Each channel of a rich project equals the sum of each track's centered
    /// unity render scaled by `volume / 1000 * pan_factor * master / 1000`.
    #[test]
    fn rich_project_level_matches_gains(
        spec in rich_project_spec(),
        rate in rate(),
    ) {
        let project = build_rich_project(&spec);
        let spans = spans(&project, rate);
        let frames = render_len(&spans, rate);
        let actual = render(&project, rate, frames);
        let master = f64::from(project.master_volume()) / 1000.0;

        let mut expected_left = vec![0.0_f64; frames];
        let mut expected_right = vec![0.0_f64; frames];
        for track_index in 0..spec.tracks.len() {
            let track = project.track(track_index as u64).unwrap();
            let vol = f64::from(track.volume) / 1000.0;
            let left_pan = ((200.0 - f64::from(track.pan)) / 100.0).min(1.0);
            let right_pan = (f64::from(track.pan) / 100.0).min(1.0);
            let gain_left = vol * left_pan * master;
            let gain_right = vol * right_pan * master;
            let unity = mono(&build_single_track_unity(&spec, track_index), rate, frames);
            for frame in 0..frames {
                let sample = f64::from(unity[frame]);
                expected_left[frame] += sample * gain_left;
                expected_right[frame] += sample * gain_right;
            }
        }

        let tolerance = 1e-5 * (spans.len() as f64).max(1.0);
        for frame in 0..frames {
            if project.master_volume() == 0 {
                prop_assert_eq!(actual[frame].map(f32::to_bits), [0, 0], "frame {}", frame);
            } else {
                let err_l = (f64::from(actual[frame][0]) - expected_left[frame]).abs();
                let err_r = (f64::from(actual[frame][1]) - expected_right[frame]).abs();
                prop_assert!(
                    err_l <= tolerance,
                    "left frame {}: actual {}, expected {}, err {}",
                    frame,
                    actual[frame][0],
                    expected_left[frame],
                    err_l
                );
                prop_assert!(
                    err_r <= tolerance,
                    "right frame {}: actual {}, expected {}, err {}",
                    frame,
                    actual[frame][1],
                    expected_right[frame],
                    err_r
                );
            }
        }
    }

    /// For a single note with tempo changes, a non-zero clip start, and
    /// arbitrary volume, pan (including left-side values < 100) and master
    /// volume, left and right match the centered unity render scaled by the
    /// expected gains within tight relative floating-point tolerance.
    #[test]
    fn single_note_level_matches_gains(
        tempos in prop::collection::vec(tempo_spec(), 1..=3),
        clip_start in 1u64..PPQ,
        note in rich_note(),
        volume in volume_strategy(),
        pan in pan_strategy(),
        master_volume in volume_strategy(),
        rate in rate(),
    ) {
        let clip_length = PPQ;
        let spec = RichProjectSpec {
            tempos,
            tracks: vec![RichTrack {
                volume,
                pan,
                clips: vec![RichClip {
                    gap: clip_start,
                    length: clip_length,
                    notes: vec![note],
                }],
            }],
            master_volume,
        };
        let project = build_rich_project(&spec);
        let spans = spans(&project, rate);
        let frames = render_len(&spans, rate);
        let actual = render(&project, rate, frames);
        let unity = mono(&build_single_track_unity(&spec, 0), rate, frames);

        let vol = volume as f32 / 1000.0;
        let left_pan = ((200.0 - pan as f32) / 100.0).min(1.0);
        let right_pan = (pan as f32 / 100.0).min(1.0);
        let master = master_volume as f32 / 1000.0;
        let left_gain = vol * left_pan * master;
        let right_gain = vol * right_pan * master;

        for frame in 0..frames {
            for (ch, gain) in [(0, left_gain), (1, right_gain)] {
                let expected = unity[frame] * gain;
                let got = actual[frame][ch];
                if expected == 0.0 {
                    prop_assert_eq!(got.to_bits(), 0.0f32.to_bits(), "ch {} frame {}", ch, frame);
                } else {
                    let rel = (got - expected).abs() / expected.abs();
                    prop_assert!(
                        rel <= 8.0 * f32::EPSILON,
                        "ch {} frame {}: got {}, expected {}, rel {}",
                        ch,
                        frame,
                        got,
                        expected,
                        rel
                    );
                }
            }
        }
    }

    /// At common and low sample rates, a note's magnitude never exceeds
    /// `level * envelope(frame)`, starts at `level * envelope(on)` on its
    /// onset frame, follows a monotonic attack/release envelope, and never
    /// reaches full level when shorter than two ramps minus one frame.
    #[test]
    fn envelope_bounds_samples_and_is_monotonic(
        clip_start in 0u64..PPQ / 2,
        length in prop_oneof![1u64..30, 30u64..PPQ],
        pitch in 0u64..128,
        velocity in 1u64..128,
        rate in common_rate(),
    ) {
        let mut history = History::default();
        history.edit(Edit::Apply(Command::AddTrack)).unwrap();
        history
            .edit(Edit::Apply(Command::AddClip {
                track: 0,
                start: clip_start,
                length,
            }))
            .unwrap();
        history
            .edit(Edit::Apply(Command::AddNote {
                id: 0,
                start: 0,
                length,
                pitch,
                velocity,
            }))
            .unwrap();
        let project = history.present().clone();
        let spans = spans(&project, rate);
        let frames = render_len(&spans, rate);
        let out = mono(&project, rate, frames);
        let level = velocity as f32 / 127.0;
        let ramp = (rate.get() / 200).max(1) as usize;

        if let Some(&(on, off)) = spans.first() {
            let len = off - on;
            let mid = on + len / 2;

            // At frame == on, phase is 0 so triangle == +1 and sample == level * envelope(on).
            let onset_env = expected_envelope(on, on, off, rate);
            prop_assert_eq!(
                out[on].to_bits(),
                (level * onset_env).to_bits(),
                "onset frame {}",
                on
            );

            // Envelope is monotonic non-decreasing over attack and non-increasing over release.
            for frame in on..mid {
                prop_assert!(
                    expected_envelope(frame, on, off, rate)
                        <= expected_envelope(frame + 1, on, off, rate),
                    "attack monotonicity at frame {}",
                    frame
                );
            }
            for frame in mid..off - 1 {
                prop_assert!(
                    expected_envelope(frame, on, off, rate)
                        >= expected_envelope(frame + 1, on, off, rate),
                    "release monotonicity at frame {}",
                    frame
                );
            }

            for (frame, &sample) in out.iter().enumerate() {
                if on <= frame && frame < off {
                    let bound = level * expected_envelope(frame, on, off, rate);
                    prop_assert!(
                        sample.abs() <= bound,
                        "frame {}: |{}| > {}",
                        frame,
                        sample,
                        bound
                    );
                    if len < 2 * ramp - 1 {
                        prop_assert!(
                            sample.abs() < level,
                            "short note frame {} reached full level: |{}| >= {}",
                            frame,
                            sample,
                            level
                        );
                    }
                } else {
                    prop_assert_eq!(sample.to_bits(), 0.0f32.to_bits(), "frame {}", frame);
                }
            }
        }
    }
}

/// Builds a single-note project with a clip starting at `clip_start` ticks.
fn single_note_project(clip_start: u64, length: u64, pitch: u64, velocity: u64) -> Project {
    let mut history = History::default();
    history.edit(Edit::Apply(Command::AddTrack)).unwrap();
    history
        .edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: clip_start,
            length,
        }))
        .unwrap();
    history
        .edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: 0,
            length,
            pitch,
            velocity,
        }))
        .unwrap();
    history.present().clone()
}

/// At rate 880 Hz, A4 (pitch 69 = 440 Hz) has step 0.5 so the triangle wave is
/// exactly ±1 on every frame, velocity 127 has level 1, and the 5 ms ramp is
/// exactly 4 frames: `|sample|` equals the exact linear envelope in quarters.
#[test]
fn long_note_envelope_ramps_linearly_over_5_ms() {
    let rate = NonZeroU32::new(880).unwrap();
    // Clip starts at half a beat (0.25 s = 220 frames) and lasts 0.1 s (88 frames).
    let project = single_note_project(480, SECOND / 10, 69, 127);
    let [(on, off)] = spans(&project, rate).try_into().unwrap();
    assert_eq!((on, off), (220, 308));

    let out = mono(&project, rate, off + 16);
    assert!(out[..on].iter().all(|&s| s.to_bits() == 0));
    assert!(out[off..].iter().all(|&s| s.to_bits() == 0));

    let abs: Vec<f32> = out[on..off].iter().map(|s| s.abs()).collect();
    assert_eq!(
        abs[..4].iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
        [0.25f32, 0.5, 0.75, 1.0].map(f32::to_bits)
    );
    assert!(
        abs[4..abs.len() - 4]
            .iter()
            .all(|&s| s.to_bits() == 1.0f32.to_bits())
    );
    assert_eq!(
        abs[abs.len() - 4..]
            .iter()
            .map(|s| s.to_bits())
            .collect::<Vec<_>>(),
        [1.0f32, 0.75, 0.5, 0.25].map(f32::to_bits)
    );

    // Also verify the actual attack/sustain/release shape at 48 kHz (ramp = 240 frames)
    // using a high note (pitch 105, ~3520 Hz) whose peaks rise across 40-frame windows
    // during the 5 ms attack and fall during the 5 ms release.
    let hi_rate = NonZeroU32::new(48_000).unwrap();
    let hi_project = single_note_project(0, SECOND / 10, 105, 127);
    let [(hi_on, hi_off)] = spans(&hi_project, hi_rate).try_into().unwrap();
    assert_eq!((hi_on, hi_off), (0, 4_800));
    let hi_out = mono(&hi_project, hi_rate, hi_off + 64);
    let window_peak = |range: std::ops::Range<usize>| {
        hi_out[range]
            .iter()
            .fold(0.0_f32, |peak, &s| peak.max(s.abs()))
    };
    let attack_peaks: Vec<f32> = (0..6).map(|w| window_peak(w * 40..(w + 1) * 40)).collect();
    for pair in attack_peaks.windows(2) {
        assert!(
            pair[0] < pair[1],
            "attack peaks should rise: {attack_peaks:?}"
        );
    }
    let sustain_peak = window_peak(240..hi_off - 240);
    assert!(
        sustain_peak > 0.99 && sustain_peak <= 1.0,
        "sustain peak {sustain_peak}"
    );
    let release_start = hi_off - 240;
    let release_peaks: Vec<f32> = (0..6)
        .map(|w| window_peak(release_start + w * 40..release_start + (w + 1) * 40))
        .collect();
    for pair in release_peaks.windows(2) {
        assert!(
            pair[0] > pair[1],
            "release peaks should fall: {release_peaks:?}"
        );
    }
}

/// Notes shorter than 10 ms (shorter than 2 ramps) have a symmetric,
/// triangle-shaped envelope whose peak stays below 1 when shorter than
/// `2 * ramp - 1` frames.
#[test]
fn short_note_envelope_peaks_below_one_and_is_symmetric() {
    let rate = NonZeroU32::new(880).unwrap();
    let cases: &[(u64, &[f32])] = &[
        (2, &[0.25]),
        (4, &[0.25, 0.25]),
        (7, &[0.25, 0.5, 0.25]),
        (9, &[0.25, 0.5, 0.5, 0.25]),
        (11, &[0.25, 0.5, 0.75, 0.5, 0.25]),
        (13, &[0.25, 0.5, 0.75, 0.75, 0.5, 0.25]),
        (15, &[0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25]),
    ];
    for &(ticks, expected) in cases {
        let project = single_note_project(480, ticks, 69, 127);
        let [(on, off)] = spans(&project, rate).try_into().unwrap();
        assert_eq!(off - on, expected.len(), "ticks {ticks}");
        let out = mono(&project, rate, off + 16);
        assert!(out[..on].iter().all(|&s| s.to_bits() == 0));
        assert!(out[off..].iter().all(|&s| s.to_bits() == 0));
        let got: Vec<u32> = out[on..off].iter().map(|s| s.abs().to_bits()).collect();
        let want: Vec<u32> = expected.iter().map(|s| s.to_bits()).collect();
        assert_eq!(got, want, "ticks {ticks} ({} frames)", expected.len());
        // Symmetric around the center.
        let reversed: Vec<u32> = got.iter().rev().copied().collect();
        assert_eq!(got, reversed, "envelope symmetry for ticks {ticks}");
        // Peak strictly below 1 for notes shorter than 2 * ramp - 1 = 7 frames.
        if expected.len() < 7 {
            let peak = out[on..off].iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
            assert!(peak < 1.0, "peak {peak} for {} frames", expected.len());
        }
    }
}
