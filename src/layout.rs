//! Where the timeline and the piano roll put each track, clip and note, and
//! what lies under a point of the window. Coordinates are logical pixels from
//! the top left of the window.

use std::num::NonZeroU32;

use pathos_core::{Clip, MAX_TICK, Note, PPQ, Project, Tick};

/// The height of the status line above the timeline.
pub const STATUS_HEIGHT: f32 = 28.0;
/// The width of the track names left of the timeline and of the pitch names
/// left of the piano roll.
pub const HEADER_WIDTH: f32 = 72.0;
pub const LANE_HEIGHT: f32 = 48.0;
pub const BEAT_WIDTH: f32 = 32.0;
/// The gap between the last lane and the piano roll.
pub const ROLL_GAP: f32 = 16.0;
pub const ROW_HEIGHT: f32 = 8.0;
pub const ROLL_BEAT_WIDTH: f32 = 96.0;
/// The piano roll shows C2 to C6.
pub const LOWEST_PITCH: u8 = 36;
pub const HIGHEST_PITCH: u8 = 84;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn contains(&self, x: f32, y: f32) -> bool {
        self.x <= x && x < self.x + self.width && self.y <= y && y < self.y + self.height
    }
}

/// What lies under a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A lane of the timeline where no clip lies.
    Lane {
        track: u64,
        tick: Tick,
    },
    Clip {
        id: u64,
        tick: Tick,
    },
    /// A cell of the piano roll of the clip `id` where no note lies; `tick` is
    /// counted from the start of the clip.
    Roll {
        id: u64,
        tick: Tick,
        pitch: u8,
    },
    Note {
        id: u64,
        start: Tick,
        pitch: u8,
    },
    /// The mute button in the header of a track.
    Mute {
        track: u64,
    },
    /// The volume in the header of a track.
    Volume {
        track: u64,
    },
    Nothing,
}

pub fn lane_top(track: u64) -> f32 {
    STATUS_HEIGHT + track as f32 * LANE_HEIGHT
}

/// The mute button of a track, below its name.
pub fn mute_rect(track: u64) -> Rect {
    Rect {
        x: 6.0,
        y: lane_top(track) + 26.0,
        width: 16.0,
        height: 16.0,
    }
}

/// The volume of a track, right of its mute button.
pub fn volume_rect(track: u64) -> Rect {
    Rect {
        x: 26.0,
        y: lane_top(track) + 26.0,
        width: HEADER_WIDTH - 28.0,
        height: 16.0,
    }
}

pub fn clip_rect(clip: &Clip) -> Rect {
    Rect {
        x: HEADER_WIDTH + ticks_to_width(clip.start, BEAT_WIDTH),
        y: lane_top(clip.track),
        width: ticks_to_width(clip.length, BEAT_WIDTH),
        height: LANE_HEIGHT,
    }
}

pub fn roll_top(project: &Project) -> f32 {
    lane_top(project.track_count()) + ROLL_GAP
}

pub fn row_top(project: &Project, pitch: u8) -> f32 {
    roll_top(project) + f32::from(HIGHEST_PITCH - pitch) * ROW_HEIGHT
}

pub fn roll_height() -> f32 {
    f32::from(HIGHEST_PITCH - LOWEST_PITCH + 1) * ROW_HEIGHT
}

/// The rectangle of a note in the piano roll of its clip, or `None` when its
/// pitch lies outside the roll.
pub fn note_rect(project: &Project, note: &Note) -> Option<Rect> {
    (LOWEST_PITCH..=HIGHEST_PITCH)
        .contains(&note.pitch)
        .then(|| Rect {
            x: HEADER_WIDTH + ticks_to_width(note.start, ROLL_BEAT_WIDTH),
            y: row_top(project, note.pitch),
            width: ticks_to_width(note.length, ROLL_BEAT_WIDTH),
            height: ROW_HEIGHT,
        })
}

/// The width of the piano roll of `clip`.
pub fn roll_width(clip: &Clip) -> f32 {
    ticks_to_width(clip.length, ROLL_BEAT_WIDTH)
}

/// Where the playhead is drawn when frame `frame` at `sample_rate` plays: on
/// the last tick that starts at or before it.
pub fn playhead_x(project: &Project, sample_rate: NonZeroU32, frame: u64) -> f32 {
    let at = |tick: Tick| project.tick_to_micros(tick) * u128::from(sample_rate.get()) / 1_000_000;
    let (mut low, mut high) = (0, MAX_TICK);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if at(middle) <= u128::from(frame) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    HEADER_WIDTH + ticks_to_width(low, BEAT_WIDTH)
}

/// What lies under `(x, y)`, with the piano roll showing the clip `selected`.
pub fn hit(project: &Project, selected: Option<u64>, x: f32, y: f32) -> Hit {
    if x < HEADER_WIDTH {
        return match track_at(project, y) {
            Some(track) if mute_rect(track).contains(x, y) => Hit::Mute { track },
            Some(track) if volume_rect(track).contains(x, y) => Hit::Volume { track },
            _ => Hit::Nothing,
        };
    }
    if let Some(track) = track_at(project, y) {
        let tick = width_to_ticks(x - HEADER_WIDTH, BEAT_WIDTH);
        return match project
            .clips()
            .iter()
            .find(|clip| clip.track == track && clip.start <= tick && tick < clip.stop())
        {
            Some(clip) => Hit::Clip { id: clip.id, tick },
            None => Hit::Lane { track, tick },
        };
    }
    let Some(clip) = selected.and_then(|id| project.clips().iter().find(|clip| clip.id == id))
    else {
        return Hit::Nothing;
    };
    let row = (y - roll_top(project)) / ROW_HEIGHT;
    let tick = width_to_ticks(x - HEADER_WIDTH, ROLL_BEAT_WIDTH);
    if row < 0.0 || row >= f32::from(HIGHEST_PITCH - LOWEST_PITCH + 1) || tick >= clip.length {
        return Hit::Nothing;
    }
    let pitch = HIGHEST_PITCH - row as u8;
    match project
        .notes(clip.id)
        .find(|note| note.pitch == pitch && note.start <= tick && tick < note.stop())
    {
        Some(note) => Hit::Note {
            id: clip.id,
            start: note.start,
            pitch,
        },
        None => Hit::Roll {
            id: clip.id,
            tick,
            pitch,
        },
    }
}

/// The track whose lane spans `y`.
pub fn track_at(project: &Project, y: f32) -> Option<u64> {
    let lane = (y - STATUS_HEIGHT) / LANE_HEIGHT;
    (lane >= 0.0 && lane < project.track_count() as f32).then_some(lane as u64)
}

fn ticks_to_width(ticks: Tick, beat_width: f32) -> f32 {
    ticks as f32 / PPQ as f32 * beat_width
}

fn width_to_ticks(width: f32, beat_width: f32) -> Tick {
    (width / beat_width * PPQ as f32) as Tick
}

#[cfg(test)]
mod tests {
    use pathos_core::{Command, Edit, History};

    use super::*;

    /// Two tracks: on track 0 a bar at beat 0 with notes at the edges of the
    /// piano roll and one above it, and on track 1 a bar at beat 6.
    fn project() -> Project {
        let mut history = History::default();
        let commands = [
            Command::AddTrack,
            Command::AddTrack,
            Command::AddClip {
                track: 0,
                start: 0,
                length: 4 * PPQ,
            },
            Command::AddClip {
                track: 1,
                start: 6 * PPQ,
                length: 4 * PPQ,
            },
        ];
        let notes = [
            (0, PPQ, LOWEST_PITCH),
            (3 * PPQ, PPQ, HIGHEST_PITCH),
            (PPQ, PPQ, 60),
            (0, PPQ, 90),
        ];
        for command in commands
            .into_iter()
            .chain(notes.map(|(start, length, pitch)| Command::AddNote {
                id: 0,
                start,
                length,
                pitch: pitch.into(),
                velocity: 100,
            }))
        {
            history.edit(Edit::Apply(command)).unwrap();
        }
        history.present().clone()
    }

    fn center(rect: Rect) -> (f32, f32) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn the_middle_of_each_clip_hits_it() {
        let project = project();
        for clip in project.clips() {
            let (x, y) = center(clip_rect(clip));
            assert!(
                matches!(hit(&project, None, x, y), Hit::Clip { id, .. } if id == clip.id),
                "clip {}",
                clip.id
            );
        }
    }

    #[test]
    fn the_header_of_each_track_has_a_mute_button_and_a_volume() {
        let project = project();
        for track in 0..project.track_count() {
            let (x, y) = center(mute_rect(track));
            assert_eq!(hit(&project, None, x, y), Hit::Mute { track });
            let (x, y) = center(volume_rect(track));
            assert_eq!(hit(&project, None, x, y), Hit::Volume { track });
            // The volume reaches almost to the timeline; its first pixel is
            // inside and the ones past its ends are not.
            let rect = volume_rect(track);
            let volume = Hit::Volume { track };
            assert_eq!(hit(&project, None, rect.x, rect.y), volume);
            assert_eq!(hit(&project, None, HEADER_WIDTH - 3.0, y), volume);
            assert_eq!(hit(&project, None, rect.x + rect.width, y), Hit::Nothing);
            assert_eq!(hit(&project, None, x, rect.y + rect.height), Hit::Nothing);
            assert_eq!(hit(&project, None, x, rect.y - 1.0), Hit::Nothing);
            assert_eq!(
                hit(&project, None, 2.0, y),
                Hit::Nothing,
                "left of the button"
            );
        }
        let below = lane_top(project.track_count()) + 30.0;
        assert_eq!(hit(&project, None, 30.0, below), Hit::Nothing);
    }

    #[test]
    fn the_middle_of_each_note_in_the_roll_hits_it() {
        let project = project();
        let shown: Vec<_> = project
            .notes(0)
            .filter_map(|note| Some((note, note_rect(&project, note)?)))
            .collect();
        assert_eq!(shown.len(), 3, "the note above the roll is not shown");
        for (note, rect) in shown {
            let (x, y) = center(rect);
            assert_eq!(
                hit(&project, Some(0), x, y),
                Hit::Note {
                    id: 0,
                    start: note.start,
                    pitch: note.pitch
                }
            );
        }
    }

    #[test]
    fn a_clip_spans_from_its_start_up_to_its_stop() {
        let project = project();
        let y = lane_top(0) + LANE_HEIGHT / 2.0;
        assert_eq!(
            hit(&project, None, HEADER_WIDTH, y),
            Hit::Clip { id: 0, tick: 0 }
        );
        assert_eq!(
            hit(&project, None, HEADER_WIDTH + 4.0 * BEAT_WIDTH, y),
            Hit::Lane {
                track: 0,
                tick: 4 * PPQ
            }
        );
        assert_eq!(hit(&project, None, HEADER_WIDTH - 1.0, y), Hit::Nothing);
    }

    #[test]
    fn the_lanes_end_after_the_last_track() {
        let project = project();
        let x = HEADER_WIDTH + 1.0;
        assert_eq!(
            hit(&project, None, x, lane_top(1)),
            Hit::Lane { track: 1, tick: 30 }
        );
        assert_eq!(hit(&project, None, x, lane_top(2)), Hit::Nothing);
        assert_eq!(hit(&project, None, x, STATUS_HEIGHT - 1.0), Hit::Nothing);
    }

    #[test]
    fn the_roll_lies_below_the_lanes_and_spans_the_clip_and_its_pitches() {
        let project = project();
        let clip = project.clips()[0];
        // The roll starts a gap below the last lane.
        let (left, top) = (HEADER_WIDTH, lane_top(2) + ROLL_GAP);
        let (right, bottom) = (left + roll_width(&clip), top + roll_height());
        let inside = |x, y| !matches!(hit(&project, Some(0), x, y), Hit::Nothing);
        assert!(inside(left, top));
        assert!(inside(right - 1.0, bottom - 1.0));
        assert!(!inside(left, top - 1.0));
        assert!(!inside(left, bottom));
        assert!(!inside(right, top));
        assert_eq!(row_top(&project, HIGHEST_PITCH), top);
        assert_eq!(row_top(&project, LOWEST_PITCH) + ROW_HEIGHT, bottom);
    }

    #[test]
    fn a_note_spans_from_its_start_up_to_its_stop() {
        let project = project();
        let y = row_top(&project, 60) + ROW_HEIGHT / 2.0;
        let at_beat = |beats: f32| HEADER_WIDTH + beats * ROLL_BEAT_WIDTH;
        assert_eq!(
            hit(&project, Some(0), at_beat(1.0), y),
            Hit::Note {
                id: 0,
                start: PPQ,
                pitch: 60
            }
        );
        assert_eq!(
            hit(&project, Some(0), at_beat(2.0), y),
            Hit::Roll {
                id: 0,
                tick: 2 * PPQ,
                pitch: 60
            }
        );
    }

    #[test]
    fn the_playhead_follows_the_tempo() {
        let mut history = History::default();
        let rate = NonZeroU32::new(48_000).unwrap();
        // 120 BPM: a beat is 24 000 frames.
        let x = |history: &History, frame| playhead_x(history.present(), rate, frame);
        assert_eq!(x(&history, 0), HEADER_WIDTH);
        assert_eq!(x(&history, 24_000), HEADER_WIDTH + BEAT_WIDTH);
        assert!(x(&history, 23_999) < HEADER_WIDTH + BEAT_WIDTH);
        // 60 BPM from beat 1: beat 2 is 48 000 frames after beat 1.
        history
            .edit(Edit::Apply(Command::SetTempo {
                tick: PPQ,
                us_per_quarter: 1_000_000,
            }))
            .unwrap();
        assert_eq!(x(&history, 72_000), HEADER_WIDTH + 2.0 * BEAT_WIDTH);
    }

    #[test]
    fn without_a_selected_clip_there_is_no_roll() {
        let project = project();
        assert_eq!(
            hit(&project, None, HEADER_WIDTH, roll_top(&project)),
            Hit::Nothing
        );
    }
}
