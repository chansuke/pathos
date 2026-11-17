//! The editing state of the window and the edit each gesture makes. Every
//! change to the project goes through [`History::edit`] (ADR 2), so the tests
//! here check which edit a gesture produces (ADR 7).

use pathos_core::{Command, Edit, EditError, History, PPQ, Tick};

use crate::layout::{self, BEAT_WIDTH, Hit};

/// A new clip lasts one bar of 4/4.
const CLIP_LENGTH: u64 = 4 * PPQ;
/// A new note lasts a sixteenth and starts on one.
const NOTE_LENGTH: u64 = PPQ / 4;
const NOTE_VELOCITY: u64 = 100;

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

/// A clip being dragged from where the button went down.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    id: u64,
    x: f32,
}

#[derive(Debug, Default)]
pub struct Editor {
    history: History,
    selected: Option<u64>,
    drag: Option<Drag>,
    pointer: (f32, f32),
    /// Why the last edit was rejected, until the next edit is accepted.
    rejected: Option<EditError>,
}

impl Editor {
    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected.filter(|id| {
            self.history
                .present()
                .clips()
                .iter()
                .any(|clip| clip.id == *id)
        })
    }

    pub fn rejected(&self) -> Option<EditError> {
        self.rejected
    }

    /// Handles `input` and returns whether the project changed.
    pub fn handle(&mut self, input: Input) -> bool {
        let Some(edit) = self.edit_for(input) else {
            return false;
        };
        if let Edit::Apply(Command::AddClip { .. }) = edit {
            self.selected = Some(self.history.next_clip_id());
        }
        self.rejected = self.history.edit(edit).err();
        self.rejected.is_none()
    }

    /// The edit `input` makes, if any, after updating the selection, the drag
    /// and the pointer.
    fn edit_for(&mut self, input: Input) -> Option<Edit> {
        let project = self.history.present();
        match input {
            Input::Down { x, y, clicks } => {
                self.pointer = (x, y);
                match layout::hit(project, self.selected(), x, y) {
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
                        self.drag = Some(Drag { id, x });
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
                None
            }
            Input::Up { x, y } => {
                self.pointer = (x, y);
                let drag = self.drag.take()?;
                let clip = project.clips().iter().find(|clip| clip.id == drag.id)?;
                let beats = ((x - drag.x) / BEAT_WIDTH).round() as i64;
                let start = clip.start.saturating_add_signed(beats * PPQ as i64);
                let track = layout::track_at(project, y).unwrap_or(clip.track);
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
                match layout::hit(project, Some(id), x, y) {
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
        HEADER_WIDTH, LANE_HEIGHT, ROLL_BEAT_WIDTH, ROW_HEIGHT, lane_top, row_top,
    };

    /// The point `beats` into the lane of `track`, halfway down.
    fn in_lane(track: u64, beats: f32) -> (f32, f32) {
        (
            HEADER_WIDTH + beats * BEAT_WIDTH,
            lane_top(track) + LANE_HEIGHT / 2.0,
        )
    }

    fn in_roll(editor: &Editor, beats: f32, pitch: u8) -> (f32, f32) {
        (
            HEADER_WIDTH + beats * ROLL_BEAT_WIDTH,
            row_top(editor.history().present(), pitch) + ROW_HEIGHT / 2.0,
        )
    }

    fn click(editor: &mut Editor, (x, y): (f32, f32), clicks: usize) {
        editor.handle(Input::Down { x, y, clicks });
        editor.handle(Input::Up { x, y });
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
        let cell = in_roll(&editor, 1.3, 60);
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
