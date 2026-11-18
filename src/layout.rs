//! Where the timeline and the piano roll put each track, clip and note, and
//! what lies under a point of the window. Coordinates are logical pixels from
//! the top left of the window.

use std::num::NonZeroU32;
use std::ops::Range;

use pathos_core::{Clip, MAX_TICK, Note, PPQ, Project, Tick};

/// The height of the bar at the top of the window, which holds the transport
/// and the master volume (ADR 27).
pub const TOP_BAR_HEIGHT: f32 = 52.0;
/// The height of the ruler between the top bar and the lanes.
pub const RULER_HEIGHT: f32 = 32.0;
/// Where the lanes start.
pub const TIMELINE_TOP: f32 = TOP_BAR_HEIGHT + RULER_HEIGHT;
/// The height of the bar at the bottom of the window, which holds the keys
/// and the status.
pub const FOOTER_HEIGHT: f32 = 30.0;
/// The width of the meter at the right of the top bar.
pub const METER_WIDTH: f32 = 196.0;
/// The space between the top bar's groups and its edges.
const BAR_PADDING: f32 = 16.0;
/// The width of the track headers left of the timeline and of the pitch names
/// left of the piano roll.
pub const HEADER_WIDTH: f32 = 240.0;
pub const LANE_HEIGHT: f32 = 72.0;
/// The space around the controls of a track header.
const HEADER_PADDING: f32 = 12.0;
/// The height of a row of controls in a track header.
const CONTROL_HEIGHT: f32 = 22.0;
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
    pub fn contains(&self, x: f32, y: f32) -> bool {
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
    /// The solo button in the header of a track.
    Solo {
        track: u64,
    },
    /// The pan in the header of a track.
    Pan {
        track: u64,
    },
    /// The volume in the header of a track, or the master volume.
    Volume {
        fader: Fader,
    },
    Nothing,
}

/// A volume the window can change: a track's, or the master volume (ADR 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fader {
    Track(u64),
    Master,
}

pub fn lane_top(track: u64) -> f32 {
    TIMELINE_TOP + track as f32 * LANE_HEIGHT
}

/// The upper row of controls of a track header, beside its name.
fn upper_row(track: u64) -> f32 {
    lane_top(track) + (LANE_HEIGHT - 2.0 * CONTROL_HEIGHT) / 3.0
}

/// The lower row of controls of a track header.
fn lower_row(track: u64) -> f32 {
    lane_top(track) + LANE_HEIGHT - (LANE_HEIGHT - 2.0 * CONTROL_HEIGHT) / 3.0 - CONTROL_HEIGHT
}

/// The name of a track, in its upper row left of its buttons.
pub fn name_rect(track: u64) -> Rect {
    Rect {
        x: HEADER_PADDING,
        y: upper_row(track),
        width: mute_rect(track).x - 2.0 * HEADER_PADDING,
        height: CONTROL_HEIGHT,
    }
}

/// The mute button of a track, left of its solo button.
pub fn mute_rect(track: u64) -> Rect {
    Rect {
        x: solo_rect(track).x - 30.0,
        ..solo_rect(track)
    }
}

/// The button that returns to the start, first of the transport.
pub fn home_rect() -> Rect {
    Rect {
        x: 232.0,
        y: 10.0,
        width: 44.0,
        height: 32.0,
    }
}

/// The button that plays and stops, right of the one that returns to the
/// start.
pub fn play_rect() -> Rect {
    let home = home_rect();
    Rect {
        x: home.x + home.width + 3.0,
        width: 52.0,
        ..home
    }
}

/// The position, the time and the tempo, right of the transport.
pub fn readout_rect() -> Rect {
    let play = play_rect();
    Rect {
        x: play.x + play.width + 16.0,
        y: 7.0,
        width: 284.0,
        height: 38.0,
    }
}

/// The button that turns the loop on and off, right of the readout.
pub fn loop_button_rect() -> Rect {
    let readout = readout_rect();
    Rect {
        x: readout.x + readout.width + 16.0,
        width: 84.0,
        ..home_rect()
    }
}

/// The button that turns the metronome on and off, right of the loop's.
pub fn metronome_button_rect() -> Rect {
    let toggle = loop_button_rect();
    Rect {
        x: toggle.x + toggle.width + 6.0,
        width: 128.0,
        ..toggle
    }
}

/// The master volume in a window `width` wide, under its name at the right
/// of the top bar, left of the meter (ADR 24).
pub fn master_rect(width: f32) -> Rect {
    Rect {
        x: meter_rect(width, 0).x - 14.0 - 64.0,
        y: 24.0,
        width: 64.0,
        height: 22.0,
    }
}

/// The meter of `channel`, 0 for the left and 1 for the right, at the right
/// of the top bar (ADR 25). Its scale lies below the right one.
pub fn meter_rect(width: f32, channel: usize) -> Rect {
    Rect {
        x: width - BAR_PADDING - METER_WIDTH,
        y: 12.0 + 9.0 * channel as f32,
        width: METER_WIDTH,
        height: 6.0,
    }
}

/// The solo button of a track, at the right of its upper row.
pub fn solo_rect(track: u64) -> Rect {
    Rect {
        x: HEADER_WIDTH - HEADER_PADDING - 24.0,
        y: upper_row(track),
        width: 24.0,
        height: CONTROL_HEIGHT,
    }
}

/// The volume of a track, right of its pan, reaching to the right of its
/// lower row.
pub fn volume_rect(track: u64) -> Rect {
    let pan = pan_rect(track);
    let x = pan.x + pan.width + 10.0;
    Rect {
        x,
        y: pan.y,
        width: HEADER_WIDTH - HEADER_PADDING - x,
        height: CONTROL_HEIGHT,
    }
}

/// The pan of a track, at the left of its lower row.
pub fn pan_rect(track: u64) -> Rect {
    Rect {
        x: HEADER_PADDING,
        y: lower_row(track),
        width: 56.0,
        height: CONTROL_HEIGHT,
    }
}

/// The part of the timeline that a loop of `ticks` covers (ADR 22).
pub fn loop_rect(project: &Project, ticks: &Range<Tick>) -> Rect {
    Rect {
        x: HEADER_WIDTH + ticks_to_width(ticks.start, BEAT_WIDTH),
        y: TIMELINE_TOP,
        width: ticks_to_width(ticks.end - ticks.start, BEAT_WIDTH),
        height: project.track_count() as f32 * LANE_HEIGHT,
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

/// Where the playhead is drawn when frame `frame` at `sample_rate` plays.
pub fn playhead_x(project: &Project, sample_rate: NonZeroU32, frame: u64) -> f32 {
    HEADER_WIDTH + ticks_to_width(tick_at(project, sample_rate, frame), BEAT_WIDTH)
}

/// The last tick that starts at or before frame `frame` at `sample_rate`.
pub fn tick_at(project: &Project, sample_rate: NonZeroU32, frame: u64) -> Tick {
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
    low
}

/// What lies under `(x, y)`, with the piano roll showing the clip `selected`.
pub fn hit(project: &Project, selected: Option<u64>, x: f32, y: f32) -> Hit {
    if x < HEADER_WIDTH {
        return match track_at(project, y) {
            Some(track) if mute_rect(track).contains(x, y) => Hit::Mute { track },
            Some(track) if solo_rect(track).contains(x, y) => Hit::Solo { track },
            Some(track) if pan_rect(track).contains(x, y) => Hit::Pan { track },
            Some(track) if volume_rect(track).contains(x, y) => Hit::Volume {
                fader: Fader::Track(track),
            },
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
    let lane = (y - TIMELINE_TOP) / LANE_HEIGHT;
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
            let (x, y) = center(solo_rect(track));
            assert_eq!(hit(&project, None, x, y), Hit::Solo { track });
            let (x, y) = center(pan_rect(track));
            assert_eq!(hit(&project, None, x, y), Hit::Pan { track });
            let (x, y) = center(volume_rect(track));
            let volume = Hit::Volume {
                fader: Fader::Track(track),
            };
            assert_eq!(hit(&project, None, x, y), volume);
            let [name, mute, solo, pan, rect] = [
                name_rect(track),
                mute_rect(track),
                solo_rect(track),
                pan_rect(track),
                volume_rect(track),
            ];
            // The name and the buttons share the upper row, the pan and the
            // volume the lower one, all inside the lane and the header.
            assert_eq!((name.y, mute.y), (solo.y, solo.y));
            assert_eq!(pan.y, rect.y);
            assert!(solo.y >= lane_top(track) && pan.y >= solo.y + solo.height);
            assert!(pan.y + pan.height <= lane_top(track) + LANE_HEIGHT);
            assert!(name.x + name.width < mute.x);
            assert_eq!(
                mute.x + mute.width + 6.0,
                solo.x,
                "the buttons side by side"
            );
            // The rows share the lane's spare height evenly above, between
            // and below them.
            let above = solo.y - lane_top(track);
            let between = pan.y - (solo.y + solo.height);
            let below = lane_top(track) + LANE_HEIGHT - (pan.y + pan.height);
            assert!((between - above).abs() < 1e-3 && (below - above).abs() < 1e-3);
            assert_eq!(solo.x + solo.width, rect.x + rect.width);
            assert!(rect.x + rect.width < HEADER_WIDTH, "short of the timeline");
            assert_eq!(
                hit(&project, None, center(name).0, center(name).1),
                Hit::Nothing
            );
            // The first pixel of a control is inside and the ones past its
            // ends are not.
            assert_eq!(hit(&project, None, rect.x, rect.y), volume);
            assert_eq!(
                hit(&project, None, rect.x - 1.0, y),
                Hit::Nothing,
                "between"
            );
            assert_eq!(hit(&project, None, pan.x + pan.width, y), Hit::Nothing);
            assert_eq!(hit(&project, None, pan.x - 1.0, y), Hit::Nothing);
            assert_eq!(hit(&project, None, rect.x + rect.width, y), Hit::Nothing);
            assert_eq!(hit(&project, None, x, rect.y + rect.height), Hit::Nothing);
            assert_eq!(hit(&project, None, x, rect.y - 1.0), Hit::Nothing);
            assert_eq!(
                hit(&project, None, solo.x + solo.width, solo.y),
                Hit::Nothing
            );
            assert_eq!(hit(&project, None, mute.x - 1.0, mute.y), Hit::Nothing);
        }
        let below = lane_top(project.track_count()) + 30.0;
        assert_eq!(hit(&project, None, 30.0, below), Hit::Nothing);
    }

    #[test]
    fn the_top_bar_holds_the_transport_then_the_toggles_in_a_row() {
        let row = [
            home_rect(),
            play_rect(),
            readout_rect(),
            loop_button_rect(),
            metronome_button_rect(),
        ];
        assert!(row[0].x >= HEADER_WIDTH - 8.0, "after the name of the file");
        for pair in row.windows(2) {
            assert!(pair[0].x + pair[0].width < pair[1].x, "{pair:?}");
        }
        assert_eq!(play_rect().x, home_rect().x + home_rect().width + 3.0);
        assert!(play_rect().width > home_rect().width, "play is the wider");
        // Wide enough for their labels and keys.
        assert!(loop_button_rect().width >= 80.0);
        assert!(metronome_button_rect().width >= 120.0);
        assert_eq!(
            lane_top(0),
            TOP_BAR_HEIGHT + RULER_HEIGHT,
            "the ruler, then the lanes"
        );
        assert_eq!(metronome_button_rect().y, loop_button_rect().y);
        for rect in row {
            assert!(
                rect.y > 0.0 && rect.y + rect.height < TOP_BAR_HEIGHT,
                "{rect:?}"
            );
            let middle = rect.y + rect.height / 2.0;
            assert_eq!(middle, TOP_BAR_HEIGHT / 2.0, "centered: {rect:?}");
        }
    }

    #[test]
    fn the_master_volume_and_the_meter_sit_at_the_right_of_the_top_bar() {
        let width = 1200.0;
        let master = master_rect(width);
        let [left, right] = [meter_rect(width, 0), meter_rect(width, 1)];
        assert_eq!(
            left.x + left.width,
            width - BAR_PADDING,
            "at the right edge"
        );
        assert_eq!(master.x + master.width + 14.0, left.x);
        assert!(metronome_button_rect().x + metronome_button_rect().width < master.x);
        assert_eq!((right.x, right.width), (left.x, left.width));
        assert_eq!(
            right.y,
            left.y + left.height + 3.0,
            "the right under the left"
        );
        assert!(
            right.y + right.height + 12.0 <= TOP_BAR_HEIGHT,
            "room for the scale"
        );
        assert!(master.y + master.height < TOP_BAR_HEIGHT);
        // A wider window moves them right by as much.
        assert_eq!(master_rect(width + 100.0).x, master.x + 100.0);
        assert_eq!(meter_rect(width + 100.0, 1).x, right.x + 100.0);
    }

    #[test]
    fn a_loop_covers_the_lanes_of_its_ticks() {
        let project = project();
        let looped = loop_rect(&project, &(6 * PPQ..10 * PPQ));
        let clip = clip_rect(&project.clips()[1]);
        let lanes = Rect {
            y: lane_top(0),
            height: 2.0 * LANE_HEIGHT,
            ..clip
        };
        assert_eq!(looped, lanes);
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
        assert_eq!(hit(&project, None, x, TIMELINE_TOP - 1.0), Hit::Nothing);
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
