//! The properties ADR 15 asks of the render: silence where no note sounds, an
//! onset at the scheduled sample, and the same output on every run and
//! platform; and the track volume and mute of ADR 20.

use std::num::NonZeroU32;

use pathos_core::{Command, Edit, Event, History, Project};
use pathos_render::{Score, render};
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
        let out = render(&project, rate, frames);
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
        let out = render(&project, rate, 4 * rate.get() as usize);
        let first = out.iter().position(|&sample| sample != 0.0);
        let on = spans(&project, rate).first().map(|&(on, _)| on);
        prop_assert_eq!(first, on);
    }

    /// A note never exceeds its level, its velocity over 127, even when its
    /// frequency is above the sample rate.
    #[test]
    fn a_note_stays_within_its_level(note in note(), rate in rate()) {
        let out = render(&project(&[note]), rate, 4 * rate.get() as usize);
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
            block.resize((*size).min(frames - joined.len()), 1.0);
            score.render(joined.len() as u64, &mut block);
            joined.extend_from_slice(&block);
        }
        let bits = |samples: &[f32]| samples.iter().map(|sample| sample.to_bits()).collect::<Vec<_>>();
        prop_assert_eq!(bits(&joined), bits(&whole));
    }

    /// Notes on different tracks add up: the render of both is the sum of the
    /// render of each.
    #[test]
    fn notes_add_up(a in note(), b in note(), rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let b = NoteSpec { track: 1, ..b };
        let frames = 4 * rate.get() as usize;
        let both = render(&project(&[a, b]), rate, frames);
        let a = render(&project(&[a]), rate, frames);
        let b = render(&project(&[b]), rate, frames);
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
        let (full, half) = (render(&full, rate, frames), render(&half, rate, frames));
        for frame in 0..frames {
            prop_assert_eq!(half[frame].to_bits(), (full[frame] * 0.5).to_bits(), "frame {}", frame);
        }
        let both = project(&[a, b]);
        let other = with(both.clone(), Command::SetTrackVolume { track: 0, volume: 0 });
        prop_assert_eq!(render(&other, rate, frames), render(&project(&[b]), rate, frames));
    }

    /// A muted track is silent, whatever its volume.
    #[test]
    fn a_muted_track_is_silent(a in note(), volume in 0u64..=2000, rate in rate()) {
        let a = NoteSpec { track: 0, ..a };
        let frames = 4 * rate.get() as usize;
        let loud = with(project(&[a]), Command::SetTrackVolume { track: 0, volume });
        let muted = with(loud, Command::SetTrackMute { track: 0, muted: true });
        prop_assert!(render(&muted, rate, frames).iter().all(|sample| *sample == 0.0));
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
        let out = render(&project(&[note]), rate, 48_000);
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
    let out = render(&project(&notes), NonZeroU32::new(48_000).unwrap(), 192_000);
    // FNV-1a over the bits of every sample.
    let hash = out.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, sample| {
        sample
            .to_bits()
            .to_le_bytes()
            .iter()
            .fold(hash, |hash, &byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
            })
    });
    assert_eq!(hash, 0x615c_6c63_2423_a304, "render hash {hash:#018x}");
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
    let first = render(&project(&[note]), rate, 88_200);
    let second = render(&project(&[note]), rate, 88_200);
    assert!(
        first
            .iter()
            .zip(&second)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
}
