//! The editing state of the window and the edit each gesture makes. Every
//! change to the project goes through [`Document::edit`] (ADR 2), so the tests
//! here check which edit a gesture produces (ADR 7).

use std::borrow::Cow;
use std::ops::Range;

use pathos_agent::Proposals;
use pathos_core::{
    CENTER_PAN, Command, Document, Edit, EditError, History, MAX_PAN, MAX_VOLUME, PPQ, Project,
    Tick, UNITY_VOLUME,
};

use crate::layout::{self, BEAT_WIDTH, Fader, Hit, View, WINDOW_HEIGHT, WINDOW_WIDTH};

/// A new clip lasts one bar of 4/4.
const CLIP_LENGTH: u64 = 4 * PPQ;
/// A new note lasts a sixteenth and starts on one.
const NOTE_LENGTH: u64 = PPQ / 4;
const NOTE_VELOCITY: u64 = 100;
/// The change of a track's volume per pixel of a vertical drag on it.
const DECIBELS_PER_PIXEL: f32 = 0.25;
/// A volume dragged down to this level or below is silent.
const SILENT_DECIBELS: f32 = -60.0;

/// A gesture, in window coordinates, as the view receives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Down { x: f32, y: f32, clicks: usize },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32 },
    Key(Key),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    AddTrack,
    DeleteClip,
    /// Split the selected clip at the beat under the pointer.
    SplitClip,
    Undo,
    Redo,
}

/// What the pointer drags from where the button went down.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Clip {
        id: u64,
        x: f32,
    },
    /// A volume or a track's pan. It is heard while it moves and applied
    /// when the button is released, so that the whole gesture is one step of
    /// undo (ADR 20, ADR 21, ADR 24).
    Volume {
        fader: Fader,
        y: f32,
        from: f32,
        volume: u16,
    },
    Pan {
        track: u64,
        x: f32,
        from: u8,
        pan: u8,
    },
}

impl Drag {
    /// The drag once the pointer is at (`x`, `y`).
    fn to(self, x: f32, y: f32) -> Drag {
        match self {
            Drag::Volume {
                fader,
                y: from_y,
                from,
                ..
            } => Drag::Volume {
                fader,
                y: from_y,
                from,
                volume: volume_at(from + (from_y - y) * DECIBELS_PER_PIXEL),
            },
            Drag::Pan {
                track,
                x: from_x,
                from,
                ..
            } => Drag::Pan {
                track,
                x: from_x,
                from,
                // A pixel is a step of pan, so 100 pixels move a track from
                // the center to a side.
                pan: pan_at(f32::from(from) + x - from_x),
            },
            clip @ Drag::Clip { .. } => clip,
        }
    }

    /// The command that a drag of a volume or a track's pan applies.
    fn command(&self) -> Option<Command> {
        match *self {
            Drag::Volume { fader, volume, .. } => Some(set_volume(fader, volume)),
            Drag::Pan { track, pan, .. } => Some(Command::SetTrackPan {
                track,
                pan: pan.into(),
            }),
            Drag::Clip { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct Editor {
    document: Document,
    selected: Option<u64>,
    drag: Option<Drag>,
    pointer: (f32, f32),
    /// The size of the window, which places the master volume and the piano
    /// roll.
    width: f32,
    height: f32,
    /// How far the lanes were last scrolled; [`Editor::view`] keeps it within
    /// the lanes the window shows (ADR 29).
    scroll: f32,
    /// Why the last edit was rejected, until the next edit is accepted.
    rejected: Option<EditError>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            document: Document::default(),
            selected: None,
            drag: None,
            pointer: (0.0, 0.0),
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            scroll: 0.0,
            rejected: None,
        }
    }
}

impl Editor {
    pub fn new(document: Document) -> Self {
        Editor {
            document,
            ..Editor::default()
        }
    }

    /// Follows the window to `width` by `height`.
    pub fn resize(&mut self, width: f32, height: f32) {
        (self.width, self.height) = (width, height);
    }

    /// What the window shows: its size, and the lanes scrolled as far as
    /// they were, but no further than their end.
    pub fn view(&self) -> View {
        let view = View {
            width: self.width,
            height: self.height,
            scroll: 0.0,
        };
        let max = layout::max_scroll(self.history().present(), &view, self.selected().is_some());
        View {
            scroll: self.scroll.clamp(0.0, max),
            ..view
        }
    }

    /// Scrolls the lanes down by `by` pixels, or up when it is negative, if
    /// `(x, y)` is over them, and returns whether they moved.
    pub fn scroll(&mut self, x: f32, y: f32, by: f32) -> bool {
        let view = self.view();
        let rolled = self.selected().is_some();
        if !layout::lanes_rect(&view, rolled).contains(x, y) {
            return false;
        }
        let max = layout::max_scroll(self.history().present(), &view, rolled);
        self.scroll = (view.scroll + by).clamp(0.0, max);
        self.scroll != view.scroll
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn history(&self) -> &History {
        self.document.history()
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected.filter(|id| {
            self.history()
                .present()
                .clips()
                .iter()
                .any(|clip| clip.id == *id)
        })
    }

    pub fn rejected(&self) -> Option<EditError> {
        self.rejected
    }

    /// The command that the drag of a track's volume or pan would apply if
    /// the button were released now.
    pub fn pending(&self) -> Option<Command> {
        self.drag.as_ref().and_then(Drag::command)
    }

    /// The project as it should sound: while a volume or pan is dragged, with
    /// that volume or pan, so that the drag is heard before it becomes an edit
    /// (ADR 20, ADR 21).
    pub fn heard(&self) -> Cow<'_, Project> {
        let present = self.history().present();
        let Some(command) = self.pending() else {
            return Cow::Borrowed(present);
        };
        match present.apply(self.history().next_clip_id(), command) {
            Ok((project, _)) => Cow::Owned(project),
            Err(_) => Cow::Borrowed(present),
        }
    }

    /// The volume of `fader` as the window shows it: while it is dragged,
    /// the volume it would get.
    pub fn volume(&self, fader: Fader) -> u16 {
        let project = self.history().present();
        match (self.drag, fader) {
            (
                Some(Drag::Volume {
                    fader: dragged,
                    volume,
                    ..
                }),
                _,
            ) if dragged == fader => volume,
            (_, Fader::Track(track)) => project
                .track(track)
                .map_or(UNITY_VOLUME, |track| track.volume),
            (_, Fader::Master) => project.master_volume(),
        }
    }

    /// The pan of `track` as the window shows it: while it is dragged, the
    /// pan it would get.
    pub fn pan(&self, track: u64) -> u8 {
        match self.drag {
            Some(Drag::Pan {
                track: dragged,
                pan,
                ..
            }) if dragged == track => pan,
            _ => self
                .history()
                .present()
                .track(track)
                .map_or(CENTER_PAN, |track| track.pan),
        }
    }

    /// Applies the proposal of an agent that the window shows (ADR 35), and
    /// returns whether the project changed.
    pub fn apply_proposal(&mut self, proposals: &mut Proposals) -> bool {
        let edits = self.document.edits().len();
        self.rejected = proposals.apply(&mut self.document).err();
        self.document.edits().len() != edits
    }

    /// Handles `input` and returns whether the project changed.
    pub fn handle(&mut self, input: Input) -> bool {
        let Some(edit) = self.edit_for(input) else {
            return false;
        };
        match edit {
            Edit::Apply(Command::AddClip { .. }) => {
                self.selected = Some(self.history().next_clip_id());
            }
            // The lanes end at the new track, so it is in view.
            Edit::Apply(Command::AddTrack) => self.scroll = f32::INFINITY,
            _ => {}
        }
        self.rejected = self.document.edit(edit).err();
        self.rejected.is_none()
    }

    /// The edit `input` makes, if any, after updating the selection, the drag
    /// and the pointer.
    fn edit_for(&mut self, input: Input) -> Option<Edit> {
        let view = self.view();
        let project = self.document.history().present();
        match input {
            Input::Down { x, y, clicks } => {
                self.pointer = (x, y);
                let hit = if layout::master_rect(self.width).contains(x, y) {
                    Hit::Volume {
                        fader: Fader::Master,
                    }
                } else {
                    layout::hit(project, &view, self.selected(), x, y)
                };
                match hit {
                    Hit::Lane { track, tick } if clicks == 2 => {
                        Some(Edit::Apply(Command::AddClip {
                            track,
                            start: round_down(tick, PPQ),
                            length: CLIP_LENGTH,
                        }))
                    }
                    Hit::Lane { .. } => {
                        self.selected = None;
                        None
                    }
                    Hit::Clip { id, .. } => {
                        self.selected = Some(id);
                        self.drag = Some(Drag::Clip { id, x });
                        None
                    }
                    Hit::Mute { track } => {
                        let muted = project.track(track).is_some_and(|track| track.muted);
                        Some(Edit::Apply(Command::SetTrackMute {
                            track,
                            muted: !muted,
                        }))
                    }
                    Hit::Solo { track } => {
                        let solo = project.track(track).is_some_and(|track| track.solo);
                        Some(Edit::Apply(Command::SetTrackSolo { track, solo: !solo }))
                    }
                    Hit::Pan { track } if clicks == 2 => Some(Edit::Apply(Command::SetTrackPan {
                        track,
                        pan: CENTER_PAN.into(),
                    })),
                    Hit::Pan { track } => {
                        let pan = self.pan(track);
                        self.drag = Some(Drag::Pan {
                            track,
                            x,
                            from: pan,
                            pan,
                        });
                        None
                    }
                    Hit::Volume { fader } if clicks == 2 => {
                        Some(Edit::Apply(set_volume(fader, UNITY_VOLUME)))
                    }
                    Hit::Volume { fader } => {
                        let volume = self.volume(fader);
                        self.drag = Some(Drag::Volume {
                            fader,
                            y,
                            from: decibels(volume).unwrap_or(SILENT_DECIBELS),
                            volume,
                        });
                        None
                    }
                    Hit::Roll { id, tick, pitch } => Some(Edit::Apply(Command::AddNote {
                        id,
                        start: round_down(tick, NOTE_LENGTH),
                        length: NOTE_LENGTH,
                        pitch: pitch.into(),
                        velocity: NOTE_VELOCITY,
                    })),
                    Hit::Note { id, start, pitch } => Some(Edit::Apply(Command::RemoveNote {
                        id,
                        start,
                        pitch: pitch.into(),
                    })),
                    Hit::Nothing => None,
                }
            }
            Input::Move { x, y } => {
                self.pointer = (x, y);
                self.drag = self.drag.map(|drag| drag.to(x, y));
                None
            }
            Input::Up { x, y } => {
                self.pointer = (x, y);
                let drag = self.drag.take()?.to(x, y);
                if let Some(command) = drag.command() {
                    // A drag that ends where it started changes nothing.
                    let next_clip_id = self.document.history().next_clip_id();
                    return match project.apply(next_clip_id, command) {
                        Ok((after, _)) if &after != project => Some(Edit::Apply(command)),
                        _ => None,
                    };
                }
                let Drag::Clip { id, x: from_x } = drag else {
                    return None;
                };
                let clip = project.clips().iter().find(|clip| clip.id == id)?;
                let beats = ((x - from_x) / BEAT_WIDTH).round() as i64;
                let start = clip.start.saturating_add_signed(beats * PPQ as i64);
                let track = layout::track_at(project, &view, Some(id), y).unwrap_or(clip.track);
                (start != clip.start || track != clip.track).then_some(Edit::Apply(
                    Command::MoveClip {
                        id: clip.id,
                        track,
                        start,
                    },
                ))
            }
            Input::Key(Key::AddTrack) => Some(Edit::Apply(Command::AddTrack)),
            Input::Key(Key::DeleteClip) => self
                .selected()
                .map(|id| Edit::Apply(Command::DeleteClip { id })),
            Input::Key(Key::SplitClip) => {
                let id = self.selected()?;
                let (x, y) = self.pointer;
                match layout::hit(project, &view, Some(id), x, y) {
                    Hit::Clip { id: under, tick } if under == id => {
                        Some(Edit::Apply(Command::SplitClip {
                            id,
                            tick: round_nearest(tick, PPQ),
                        }))
                    }
                    _ => None,
                }
            }
            Input::Key(Key::Undo) => Some(Edit::Undo),
            Input::Key(Key::Redo) => Some(Edit::Redo),
        }
    }
}

/// The command that sets `fader` to `volume`.
fn set_volume(fader: Fader, volume: u16) -> Command {
    let volume = volume.into();
    match fader {
        Fader::Track(track) => Command::SetTrackVolume { track, volume },
        Fader::Master => Command::SetMasterVolume { volume },
    }
}

/// The level of `volume` in decibels, or `None` when it is silent.
pub fn decibels(volume: u16) -> Option<f32> {
    (volume > 0).then(|| 20.0 * (f32::from(volume) / f32::from(UNITY_VOLUME)).log10())
}

/// The volume nearest to `decibels`, silent at [`SILENT_DECIBELS`] and below.
fn volume_at(decibels: f32) -> u16 {
    if decibels <= SILENT_DECIBELS {
        return 0;
    }
    let volume = f32::from(UNITY_VOLUME) * 10f32.powf(decibels / 20.0);
    volume.round().min(f32::from(MAX_VOLUME)) as u16
}

/// The ticks that `L` loops (ADR 22): the clip `selected`, or every clip when
/// none is selected. A project without clips has nothing to loop.
pub fn loop_range(project: &Project, selected: Option<u64>) -> Option<Range<Tick>> {
    let clips = project.clips().iter();
    let clips: Vec<_> = match selected {
        Some(id) => clips.filter(|clip| clip.id == id).collect(),
        None => clips.collect(),
    };
    let start = clips.iter().map(|clip| clip.start).min()?;
    let stop = clips.iter().map(|clip| clip.stop()).max()?;
    Some(start..stop)
}

/// The pan nearest to `pan`, within the range.
fn pan_at(pan: f32) -> u8 {
    pan.round().clamp(0.0, f32::from(MAX_PAN)) as u8
}

/// A pan as the header of a track shows it: C at the center, else the side
/// and how far toward it, up to 100.
pub fn pan_label(pan: u8) -> String {
    match pan.cmp(&CENTER_PAN) {
        std::cmp::Ordering::Less => format!("L{}", CENTER_PAN - pan),
        std::cmp::Ordering::Equal => "C".into(),
        std::cmp::Ordering::Greater => format!("R{}", pan - CENTER_PAN),
    }
}

/// A volume as the header of a track shows it.
pub fn volume_label(volume: u16) -> String {
    match decibels(volume) {
        Some(decibels) => format!("{decibels:.1} dB"),
        None => "-inf dB".into(),
    }
}

fn round_down(tick: Tick, step: u64) -> Tick {
    tick / step * step
}

fn round_nearest(tick: Tick, step: u64) -> Tick {
    (tick + step / 2) / step * step
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{
        HEADER_WIDTH, LANE_HEIGHT, ROLL_BEAT_WIDTH, ROW_HEIGHT, Rect, TIMELINE_TOP, lane_top,
        lanes_rect, master_rect, max_scroll, mute_rect, pan_rect, row_top, solo_rect, volume_rect,
    };

    /// The point `beats` into the lane of `track`, halfway down.
    fn in_lane(track: u64, beats: f32) -> (f32, f32) {
        (
            HEADER_WIDTH + beats * BEAT_WIDTH,
            lane_top(track) + LANE_HEIGHT / 2.0,
        )
    }

    fn in_roll(beats: f32, pitch: u8) -> (f32, f32) {
        (
            HEADER_WIDTH + beats * ROLL_BEAT_WIDTH,
            row_top(WINDOW_HEIGHT, pitch) + ROW_HEIGHT / 2.0,
        )
    }

    fn center(rect: Rect) -> (f32, f32) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn click(editor: &mut Editor, (x, y): (f32, f32), clicks: usize) {
        editor.handle(Input::Down { x, y, clicks });
        editor.handle(Input::Up { x, y });
    }

    #[test]
    fn a_proposal_is_applied_only_while_the_project_is_as_it_was() {
        let mut editor = Editor::default();
        let mut proposals = Proposals::default();
        let lines = ["add-track".to_string(), "add-clip 0 0 3840".to_string()];
        proposals
            .propose(editor.document(), String::new(), &lines)
            .unwrap();
        assert!(editor.apply_proposal(&mut proposals));
        assert_eq!(editor.document().save(), "add-track\nadd-clip 0 0 3840\n");
        assert_eq!(editor.rejected(), None);

        proposals
            .propose(editor.document(), String::new(), &lines[..1])
            .unwrap();
        editor.handle(Input::Key(Key::AddTrack));
        // The new track made the proposal stale.
        assert!(!editor.apply_proposal(&mut proposals));
        assert_eq!(editor.history().present().track_count(), 2);
    }

    fn drag(editor: &mut Editor, (x, y): (f32, f32), (to_x, to_y): (f32, f32)) {
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x: to_x, y: to_y });
        editor.handle(Input::Up { x: to_x, y: to_y });
    }

    /// The history after `commands`, each of which must be accepted.
    fn after(commands: &[Command]) -> History {
        let mut history = History::default();
        for &command in commands {
            history.edit(Edit::Apply(command)).unwrap();
        }
        history
    }

    fn assert_edits(editor: &Editor, commands: &[Command]) {
        assert_eq!(editor.history(), &after(commands));
    }

    /// Two tracks, and a one-bar clip at beat 4 of track 0, selected.
    fn with_clip() -> Editor {
        let mut editor = Editor::default();
        editor.handle(Input::Key(Key::AddTrack));
        editor.handle(Input::Key(Key::AddTrack));
        click(&mut editor, in_lane(0, 4.5), 2);
        editor
    }

    const SETUP: [Command; 3] = [
        Command::AddTrack,
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 4 * PPQ,
            length: CLIP_LENGTH,
        },
    ];

    fn and(command: Command) -> Vec<Command> {
        let mut commands = SETUP.to_vec();
        commands.push(command);
        commands
    }

    #[test]
    fn a_click_on_mute_mutes_the_track_and_another_unmutes_it() {
        let mut editor = with_clip();
        click(&mut editor, center(mute_rect(1)), 1);
        let mute = |muted| Command::SetTrackMute { track: 1, muted };
        assert_edits(&editor, &and(mute(true)));
        click(&mut editor, center(mute_rect(1)), 1);
        let mut commands = and(mute(true));
        commands.push(mute(false));
        assert_edits(&editor, &commands);
    }

    #[test]
    fn a_loop_spans_the_selected_clip_or_every_clip() {
        let mut editor = with_clip();
        assert_eq!(
            loop_range(editor.history().present(), editor.selected()),
            Some(4 * PPQ..8 * PPQ)
        );
        click(&mut editor, in_lane(1, 12.5), 2);
        let both = editor.history().present();
        assert_eq!(loop_range(both, None), Some(4 * PPQ..16 * PPQ));
        assert_eq!(loop_range(both, Some(1)), Some(12 * PPQ..16 * PPQ));
        assert_eq!(loop_range(&Project::default(), None), None);
    }

    #[test]
    fn a_click_on_solo_solos_the_track_and_another_ends_it() {
        let mut editor = with_clip();
        click(&mut editor, center(solo_rect(0)), 1);
        let solo = |solo| Command::SetTrackSolo { track: 0, solo };
        assert_edits(&editor, &and(solo(true)));
        click(&mut editor, center(solo_rect(0)), 1);
        let mut commands = and(solo(true));
        commands.push(solo(false));
        assert_edits(&editor, &commands);
    }

    #[test]
    fn dragging_a_pan_is_heard_and_applies_one_edit_on_release() {
        let mut editor = with_clip();
        let (x, y) = center(pan_rect(1));
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x: x + 20.0, y });
        editor.handle(Input::Move { x: x + 50.0, y });
        let pan = |pan| Command::SetTrackPan { track: 1, pan };
        assert_eq!(editor.pending(), Some(pan(150)));
        assert_eq!(editor.pan(1), 150);
        assert_eq!(editor.pan(0), CENTER_PAN, "the other track");
        assert_eq!(editor.heard().track(1).unwrap().pan, 150);
        assert_edits(&editor, &SETUP);
        editor.handle(Input::Up { x: x + 50.0, y });
        assert_edits(&editor, &and(pan(150)));
        // A pan stops at each side.
        drag(&mut editor, (x, y), (x - 1000.0, y));
        assert_eq!(editor.pan(1), 0);
        drag(&mut editor, (x, y), (x + 1000.0, y));
        assert_eq!(editor.pan(1), MAX_PAN);
        click(&mut editor, (x, y), 2);
        assert_eq!(editor.pan(1), CENTER_PAN);
        assert_eq!(editor.history().undo_depth(), SETUP.len() + 4);
    }

    #[test]
    fn a_pan_dragged_back_to_where_it_was_applies_nothing() {
        let mut editor = with_clip();
        let (x, y) = center(pan_rect(0));
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x: x - 30.0, y });
        assert!(!editor.handle(Input::Up { x, y }));
        assert_edits(&editor, &SETUP);
    }

    #[test]
    fn a_pan_shows_its_side() {
        assert_eq!(pan_label(CENTER_PAN), "C");
        assert_eq!(pan_label(0), "L100");
        assert_eq!(pan_label(60), "L40");
        assert_eq!(pan_label(MAX_PAN), "R100");
        assert_eq!(pan_label(101), "R1");
    }

    #[test]
    fn dragging_a_volume_shows_it_and_applies_one_edit_on_release() {
        let mut editor = with_clip();
        let (x, y) = center(volume_rect(0));
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x, y: y - 12.0 });
        editor.handle(Input::Move { x, y: y - 24.0 });
        // Four pixels a decibel: +6 dB is a gain of 1.995, which is heard
        // before it is an edit.
        assert_eq!(
            editor.pending(),
            Some(Command::SetTrackVolume {
                track: 0,
                volume: 1995
            })
        );
        assert_eq!(editor.volume(Fader::Track(0)), 1995);
        let heard = editor.heard();
        assert_eq!(heard.track(0).unwrap().volume, 1995);
        assert_eq!(heard.track(1).unwrap().volume, UNITY_VOLUME);
        assert_eq!(heard.clips(), editor.history().present().clips());
        assert_eq!(
            editor.volume(Fader::Track(1)),
            UNITY_VOLUME,
            "the other track"
        );
        assert_edits(&editor, &SETUP);
        editor.handle(Input::Up { x, y: y - 24.0 });
        assert_eq!(editor.pending(), None);
        assert!(matches!(editor.heard(), Cow::Borrowed(_)));
        assert_edits(
            &editor,
            &and(Command::SetTrackVolume {
                track: 0,
                volume: 1995,
            }),
        );
        assert_eq!(editor.volume(Fader::Track(0)), 1995);
        // Down from there by 12 dB: -6 dB is a gain of 0.501.
        drag(&mut editor, (x, y), (x, y + 48.0));
        assert_eq!(editor.volume(Fader::Track(0)), 501);
    }

    #[test]
    fn a_volume_stops_at_the_maximum_and_falls_silent_below_60_db() {
        let mut editor = with_clip();
        let (x, y) = center(volume_rect(0));
        drag(&mut editor, (x, y), (x, y - 1000.0));
        assert_eq!(editor.volume(Fader::Track(0)), MAX_VOLUME);
        drag(&mut editor, (x, y), (x, y + 1000.0));
        assert_eq!(editor.volume(Fader::Track(0)), 0);
        // From silence, a drag starts at -60 dB.
        drag(&mut editor, (x, y), (x, y - 4.0));
        assert_eq!(editor.volume(Fader::Track(0)), 1);
    }

    #[test]
    fn a_volume_dragged_back_to_where_it_was_applies_nothing() {
        let mut editor = with_clip();
        let (x, y) = center(volume_rect(0));
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x, y: y - 20.0 });
        assert_ne!(editor.heard().track(0).unwrap().volume, UNITY_VOLUME);
        assert!(!editor.handle(Input::Up { x, y }));
        assert_edits(&editor, &SETUP);
        assert_eq!(&*editor.heard(), editor.history().present());
    }

    #[test]
    fn a_double_click_on_a_volume_resets_it() {
        let mut editor = with_clip();
        let (x, y) = center(volume_rect(0));
        drag(&mut editor, (x, y), (x, y + 8.0));
        click(&mut editor, (x, y), 2);
        assert_eq!(editor.volume(Fader::Track(0)), UNITY_VOLUME);
        assert_eq!(editor.history().undo_depth(), SETUP.len() + 2);
    }

    #[test]
    fn dragging_the_master_volume_is_heard_and_applies_one_edit_on_release() {
        let mut editor = with_clip();
        editor.resize(1200.0, WINDOW_HEIGHT);
        let (x, y) = center(master_rect(1200.0));
        editor.handle(Input::Down { x, y, clicks: 1 });
        editor.handle(Input::Move { x, y: y + 24.0 });
        // Six decibels down: a gain of 0.501.
        assert_eq!(
            editor.pending(),
            Some(Command::SetMasterVolume { volume: 501 })
        );
        assert_eq!(editor.volume(Fader::Master), 501);
        assert_eq!(editor.heard().master_volume(), 501);
        assert_eq!(editor.volume(Fader::Track(0)), UNITY_VOLUME, "a track");
        editor.handle(Input::Up { x, y: y + 24.0 });
        assert_edits(&editor, &and(Command::SetMasterVolume { volume: 501 }));
        click(&mut editor, (x, y), 2);
        assert_eq!(editor.volume(Fader::Master), UNITY_VOLUME);
    }

    #[test]
    fn a_volume_shows_in_decibels() {
        assert_eq!(volume_label(UNITY_VOLUME), "0.0 dB");
        assert_eq!(volume_label(500), "-6.0 dB");
        assert_eq!(volume_label(MAX_VOLUME), "6.0 dB");
        assert_eq!(volume_label(0), "-inf dB");
    }

    #[test]
    fn a_double_click_on_a_lane_adds_a_bar_on_the_beat_and_selects_it() {
        let editor = with_clip();
        assert_edits(&editor, &SETUP);
        assert_eq!(editor.selected(), Some(0));
    }

    #[test]
    fn a_click_on_an_empty_lane_clears_the_selection() {
        let mut editor = with_clip();
        click(&mut editor, in_lane(1, 0.5), 1);
        assert_eq!(editor.selected(), None);
        assert_edits(&editor, &SETUP);
    }

    #[test]
    fn dragging_a_clip_moves_it_by_whole_beats_and_across_tracks() {
        let mut editor = with_clip();
        drag(&mut editor, in_lane(0, 5.0), in_lane(1, 7.2));
        assert_edits(
            &editor,
            &and(Command::MoveClip {
                id: 0,
                track: 1,
                start: 6 * PPQ,
            }),
        );
    }

    #[test]
    fn dragging_a_clip_along_its_lane_moves_it_in_time_only() {
        let mut editor = with_clip();
        drag(&mut editor, in_lane(0, 5.0), in_lane(0, 2.0));
        assert_edits(
            &editor,
            &and(Command::MoveClip {
                id: 0,
                track: 0,
                start: PPQ,
            }),
        );
    }

    fn with_tracks(tracks: u64) -> Editor {
        let mut editor = Editor::default();
        for _ in 0..tracks {
            editor.handle(Input::Key(Key::AddTrack));
        }
        editor
    }

    #[test]
    fn adding_a_track_scrolls_the_lanes_to_it() {
        let editor = with_tracks(10);
        let view = editor.view();
        let lanes = lanes_rect(&view, false);
        assert!(view.scroll > 0.0);
        assert_eq!(
            lane_top(9) + LANE_HEIGHT - view.scroll,
            lanes.y + lanes.height,
            "the last lane ends at the bottom of the lanes"
        );
        assert_eq!(with_tracks(2).view().scroll, 0.0, "two tracks fit");
    }

    #[test]
    fn a_wheel_over_the_lanes_scrolls_them_between_their_ends() {
        let mut editor = with_tracks(10);
        let (x, y) = in_lane(0, 1.0);
        let end = editor.view().scroll;
        assert!(!editor.scroll(x, y, 10.0), "already at the end");
        assert!(editor.scroll(x, y, -30.0));
        assert_eq!(editor.view().scroll, end - 30.0);
        assert!(
            editor.scroll(HEADER_WIDTH - 1.0, y, -10_000.0),
            "over a header"
        );
        assert_eq!(editor.view().scroll, 0.0);
        assert!(!editor.scroll(x, y, -1.0), "already at the start");
        assert!(
            !editor.scroll(x, TIMELINE_TOP - 1.0, 30.0),
            "over the ruler"
        );
        assert!(
            !editor.scroll(x, WINDOW_HEIGHT - 1.0, 30.0),
            "over the footer"
        );
        assert_eq!(editor.view().scroll, 0.0);
    }

    #[test]
    fn the_lanes_scroll_back_when_their_space_grows() {
        let mut editor = with_tracks(10);
        // A bar on the last track, clicked where it is drawn.
        let (x, y) = in_lane(9, 0.5);
        let scroll = editor.view().scroll;
        click(&mut editor, (x, y - scroll), 2);
        assert_eq!(editor.selected(), Some(0));
        assert_eq!(editor.history().present().clips()[0].track, 9);
        // The roll takes space from the lanes; they stay at their end.
        let rolled = max_scroll(editor.history().present(), &editor.view(), true);
        assert_eq!(editor.view().scroll, rolled);
        assert!(editor.scroll(x, TIMELINE_TOP, -100.0));
        // Without the roll, the lanes scroll back only as far as their end.
        click(&mut editor, (x, TIMELINE_TOP), 1);
        assert_eq!(editor.selected(), None);
        let end = max_scroll(editor.history().present(), &editor.view(), false);
        assert!(end < rolled - 100.0);
        assert_eq!(editor.view().scroll, end);
    }

    #[test]
    fn a_click_on_a_clip_selects_it_without_an_edit() {
        let mut editor = with_clip();
        click(&mut editor, in_lane(1, 0.5), 1);
        click(&mut editor, in_lane(0, 6.0), 1);
        assert_eq!(editor.selected(), Some(0));
        assert_edits(&editor, &SETUP);
    }

    #[test]
    fn a_click_in_the_piano_roll_adds_a_sixteenth_and_another_removes_it() {
        let mut editor = with_clip();
        let cell = in_roll(1.3, 60);
        click(&mut editor, cell, 1);
        let mut commands = and(Command::AddNote {
            id: 0,
            start: PPQ + NOTE_LENGTH,
            length: NOTE_LENGTH,
            pitch: 60,
            velocity: NOTE_VELOCITY,
        });
        assert_edits(&editor, &commands);
        click(&mut editor, cell, 1);
        commands.push(Command::RemoveNote {
            id: 0,
            start: PPQ + NOTE_LENGTH,
            pitch: 60,
        });
        assert_edits(&editor, &commands);
    }

    #[test]
    fn the_piano_roll_follows_the_bottom_of_a_resized_window() {
        let mut editor = with_clip();
        editor.resize(1000.0, 600.0);
        let (x, _) = in_roll(1.3, 60);
        click(&mut editor, (x, row_top(600.0, 60) + ROW_HEIGHT / 2.0), 1);
        assert_edits(
            &editor,
            &and(Command::AddNote {
                id: 0,
                start: PPQ + NOTE_LENGTH,
                length: NOTE_LENGTH,
                pitch: 60,
                velocity: NOTE_VELOCITY,
            }),
        );
    }

    #[test]
    fn split_cuts_the_selected_clip_at_the_nearest_beat_under_the_pointer() {
        let mut editor = with_clip();
        let (x, y) = in_lane(0, 5.6);
        editor.handle(Input::Move { x, y });
        editor.handle(Input::Key(Key::SplitClip));
        assert_edits(
            &editor,
            &and(Command::SplitClip {
                id: 0,
                tick: 6 * PPQ,
            }),
        );
    }

    #[test]
    fn split_does_nothing_when_the_pointer_is_off_the_selected_clip() {
        let mut editor = with_clip();
        let (x, y) = in_lane(1, 5.0);
        editor.handle(Input::Move { x, y });
        editor.handle(Input::Key(Key::SplitClip));
        assert_edits(&editor, &SETUP);
    }

    #[test]
    fn split_does_nothing_when_the_pointer_is_on_another_clip() {
        let mut editor = with_clip();
        click(&mut editor, in_lane(1, 0.5), 2);
        click(&mut editor, in_lane(0, 4.5), 1);
        let (x, y) = in_lane(1, 2.0);
        editor.handle(Input::Move { x, y });
        editor.handle(Input::Key(Key::SplitClip));
        assert_eq!(editor.rejected(), None);
        assert_eq!(editor.history().undo_depth(), SETUP.len() + 1);
    }

    #[test]
    fn delete_removes_the_selected_clip_only() {
        let mut editor = with_clip();
        editor.handle(Input::Key(Key::DeleteClip));
        let commands = and(Command::DeleteClip { id: 0 });
        assert_edits(&editor, &commands);
        editor.handle(Input::Key(Key::DeleteClip));
        assert_edits(&editor, &commands);
    }

    #[test]
    fn a_rejected_edit_is_reported_until_one_is_accepted() {
        let mut editor = with_clip();
        click(&mut editor, in_lane(0, 2.0), 2);
        assert_eq!(editor.rejected(), Some(EditError::Overlap));
        assert_edits(&editor, &SETUP);
        editor.handle(Input::Key(Key::AddTrack));
        assert_eq!(editor.rejected(), None);
    }

    #[test]
    fn handling_reports_whether_the_project_changed() {
        let mut editor = with_clip();
        let (x, y) = in_lane(0, 5.0);
        assert!(!editor.handle(Input::Down { x, y, clicks: 1 }));
        assert!(!editor.handle(Input::Up { x, y }));
        let (x, y) = in_lane(0, 2.0);
        assert!(!editor.handle(Input::Down { x, y, clicks: 2 }), "rejected");
        assert!(editor.handle(Input::Key(Key::AddTrack)));
        assert!(editor.handle(Input::Key(Key::Undo)));
        let edits = editor.document().edits();
        assert_eq!(
            edits[edits.len() - 2..],
            [Edit::Apply(Command::AddTrack), Edit::Undo]
        );
    }

    #[test]
    fn undo_and_redo_step_through_the_history() {
        let mut editor = with_clip();
        editor.handle(Input::Key(Key::Undo));
        assert_eq!(editor.history().present(), after(&SETUP[..2]).present());
        editor.handle(Input::Key(Key::Redo));
        assert_eq!(editor.history().present(), after(&SETUP).present());
    }
}
