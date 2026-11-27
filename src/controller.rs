//! Non-rendering window state and key routing: ties the editor, file state,
//! transport toggles and agent proposals together without depending on gpui.

use std::num::NonZeroU32;
use std::ops::Range;
use std::path::PathBuf;

use pathos_agent::{Proposals, Request, Response};
use pathos_core::{Document, Tick};
use pathos_render::Score;

use crate::editor::{self, Editor, Input, Key};
use crate::file::{self, Saved};

/// The outcome of the last save or open, shown until the next accepted edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Saved(PathBuf),
    Failed(String),
}

/// What a keystroke in the window does, in priority order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ApplyProposal,
    DismissProposal,
    Save { choose: bool },
    Open,
    PlayOrStop,
    Rewind,
    ToggleLoop,
    ToggleMetronome,
    Editor(Key),
}

/// Maps a keystroke to its window action.
///
/// Priority order:
/// 1. Proposal actions (`Enter` / `Escape` when a proposal is shown and no
///    secondary modifier is held).
/// 2. File actions (`Ctrl/Cmd+S`, `Ctrl/Cmd+Shift+S`, `Ctrl/Cmd+O`).
/// 3. Transport actions (`Space`, `Home`).
/// 4. Loop and metronome toggles (`L`, `K` without secondary modifier).
/// 5. Editor keys (`Ctrl/Cmd+Z`, `Ctrl/Cmd+Shift+Z`, `Ctrl/Cmd+Y`, `T`, `S`,
///    `Delete`, `Backspace`).
pub fn route_key(key: &str, secondary: bool, shift: bool, has_proposal: bool) -> Option<Action> {
    if !secondary && has_proposal {
        match key {
            "enter" => return Some(Action::ApplyProposal),
            "escape" => return Some(Action::DismissProposal),
            _ => {}
        }
    }
    if secondary {
        match key {
            "s" => return Some(Action::Save { choose: shift }),
            "o" => return Some(Action::Open),
            _ => {}
        }
    }
    match key {
        "space" => return Some(Action::PlayOrStop),
        "home" => return Some(Action::Rewind),
        _ => {}
    }
    if !secondary {
        match key {
            "l" => return Some(Action::ToggleLoop),
            "k" => return Some(Action::ToggleMetronome),
            _ => {}
        }
    }
    editor_key(key, secondary, shift).map(Action::Editor)
}

fn editor_key(key: &str, secondary: bool, shift: bool) -> Option<Key> {
    match key {
        "z" if secondary && shift => Some(Key::Redo),
        "z" if secondary => Some(Key::Undo),
        "y" if secondary => Some(Key::Redo),
        _ if secondary => None,
        "t" => Some(Key::AddTrack),
        "s" => Some(Key::SplitClip),
        "delete" | "backspace" => Some(Key::DeleteClip),
        _ => None,
    }
}

/// The window's document, file, loop, metronome and agent proposal state.
#[derive(Debug, Default)]
pub struct Controller {
    pub editor: Editor,
    pub saved: Saved,
    pub notice: Option<Notice>,
    /// The ticks that playback repeats, if any (ADR 22).
    pub looping: Option<Range<Tick>>,
    /// Whether playback clicks on every beat (ADR 23).
    pub metronome: bool,
    /// What agents proposed through the socket (ADR 35).
    pub proposals: Proposals,
}

impl Controller {
    /// Routes a keystroke using whether a proposal is currently shown.
    pub fn route_key(&mut self, key: &str, secondary: bool, shift: bool) -> Option<Action> {
        let has_proposal = self.proposals.shown(self.editor.document()).is_some();
        route_key(key, secondary, shift, has_proposal)
    }

    /// Handles an editor gesture and returns whether the published score
    /// should be updated.
    pub fn handle(&mut self, input: Input) -> bool {
        let heard = self.editor.pending();
        if self.editor.handle(input) {
            self.notice = None;
            true
        } else {
            // A dragged volume or pan changed, or was let go without an edit.
            self.editor.pending() != heard
        }
    }

    /// Starts editing `document`, stored in `path`, from the beginning:
    /// clears loop range and notice, and invalidates pending agent proposals.
    pub fn replace(&mut self, document: Document, path: PathBuf) {
        self.saved = Saved::new(Some(path), &document);
        self.editor = Editor::new(document);
        self.looping = None;
        self.notice = None;
        self.proposals.invalidate();
    }

    /// Opens `path` from disk, replacing the current document on success or
    /// recording a failure notice on error. Returns whether the document was
    /// replaced.
    pub fn open_path(&mut self, path: PathBuf) -> bool {
        match file::open(&path) {
            Ok(document) => {
                self.replace(document, path);
                true
            }
            Err(error) => {
                self.notice = Some(Notice::Failed(format!(
                    "cannot open {}: {error}",
                    path.display()
                )));
                false
            }
        }
    }

    /// Opens a file passed on the command line, or binds the new empty
    /// document to that path if it does not exist yet. Returns whether an
    /// existing document was loaded.
    pub fn open_cli_path(&mut self, path: PathBuf) -> bool {
        if path.exists() {
            self.open_path(path)
        } else {
            self.saved = Saved::new(Some(path), self.editor.document());
            false
        }
    }

    /// Checks whether opening another file is allowed; if there are unsaved
    /// changes, records a failure notice and returns `false`.
    pub fn can_open(&mut self) -> bool {
        if self.saved.modified(self.editor.document()) {
            self.notice = Some(Notice::Failed(
                "there are unsaved changes; save them first".into(),
            ));
            false
        } else {
            true
        }
    }

    /// Saves the open document to `path`, updating `notice` with the outcome.
    pub fn save_to(&mut self, path: PathBuf) -> bool {
        let saved = self.saved.save(path.clone(), self.editor.document());
        self.notice = Some(match &saved {
            Ok(()) => Notice::Saved(path),
            Err(error) => Notice::Failed(format!("cannot save {}: {error}", path.display())),
        });
        saved.is_ok()
    }

    /// Applies the shown proposal to the document (ADR 35) and returns whether
    /// the project changed.
    pub fn apply_proposal(&mut self) -> bool {
        if self.editor.apply_proposal(&mut self.proposals) {
            self.notice = None;
            true
        } else {
            false
        }
    }

    pub fn dismiss_proposal(&mut self) {
        self.proposals.dismiss(self.editor.document());
    }

    /// Answers an agent request against the current document.
    pub fn handle_request(&mut self, request: Request) -> Response {
        self.proposals.handle(self.editor.document(), request)
    }

    /// Loops the selected clip, or every clip, and stops looping when a loop
    /// is on (ADR 22).
    pub fn toggle_loop(&mut self) {
        self.looping = match self.looping {
            Some(_) => None,
            None => editor::loop_range(self.editor.history().present(), self.editor.selected()),
        };
    }

    pub fn toggle_metronome(&mut self) {
        self.metronome = !self.metronome;
    }

    /// Builds the audio score for the current project, loop and metronome
    /// settings.
    pub fn score(&self, sample_rate: NonZeroU32) -> Score {
        let heard = self.editor.heard();
        let mut score = Score::new(&heard, sample_rate);
        if let Some(ticks) = &self.looping {
            score = score.looped(ticks.clone());
        }
        if self.metronome {
            score = score.with_metronome();
        }
        score
    }

    /// Whether the document has unsaved changes that require confirmation
    /// before closing the window.
    pub fn needs_close_confirmation(&self) -> bool {
        self.saved.modified(self.editor.document())
    }

    /// The file name shown in the top bar and close prompt.
    pub fn document_name(&self) -> String {
        self.saved
            .path()
            .and_then(|path| path.file_name())
            .map_or("untitled".into(), |name| {
                name.to_string_lossy().into_owned()
            })
    }

    /// The status line for the footer: `(message, is_error)`, or `None` when
    /// nothing overrides the key legend.
    pub fn status(&self, audio_error: Option<&str>) -> Option<(String, bool)> {
        match (self.editor.rejected(), &self.notice, audio_error) {
            (Some(error), _, _) => Some((error.to_string(), true)),
            (None, Some(Notice::Saved(path)), _) => {
                Some((format!("Saved {}", path.display()), false))
            }
            (None, Some(Notice::Failed(error)), _) => Some((error.clone(), true)),
            (None, None, Some(error)) => Some((format!("No audio output: {error}"), true)),
            (None, None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pathos_agent::Status;
    use pathos_core::{Command, Edit, PPQ};

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("pathos-controller-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn document(lines: &[&str]) -> Document {
        let mut doc = Document::default();
        for line in lines {
            doc.edit(line.parse::<Edit>().unwrap()).unwrap();
        }
        doc
    }

    #[test]
    fn route_key_respects_priority_and_modifiers() {
        // Proposal keys only fire when a proposal is shown and secondary is off.
        assert_eq!(route_key("enter", false, false, false), None);
        assert_eq!(route_key("escape", false, false, false), None);
        assert_eq!(
            route_key("enter", false, false, true),
            Some(Action::ApplyProposal)
        );
        assert_eq!(
            route_key("escape", false, false, true),
            Some(Action::DismissProposal)
        );
        assert_eq!(route_key("enter", true, false, true), None);
        assert_eq!(route_key("escape", true, false, true), None);

        // File shortcuts take priority over plain editor keys when secondary is held.
        assert_eq!(
            route_key("s", true, false, true),
            Some(Action::Save { choose: false })
        );
        assert_eq!(
            route_key("s", true, true, true),
            Some(Action::Save { choose: true })
        );
        assert_eq!(route_key("o", true, false, true), Some(Action::Open));
        assert_eq!(route_key("o", false, false, false), None);

        // Transport keys.
        assert_eq!(
            route_key("space", false, false, true),
            Some(Action::PlayOrStop)
        );
        assert_eq!(route_key("home", false, false, false), Some(Action::Rewind));

        // Loop and metronome toggles require no secondary modifier.
        assert_eq!(
            route_key("l", false, false, false),
            Some(Action::ToggleLoop)
        );
        assert_eq!(route_key("l", true, false, false), None);
        assert_eq!(
            route_key("k", false, false, false),
            Some(Action::ToggleMetronome)
        );
        assert_eq!(route_key("k", true, false, false), None);

        // Editor keys.
        assert_eq!(
            route_key("z", true, false, false),
            Some(Action::Editor(Key::Undo))
        );
        assert_eq!(
            route_key("z", true, true, false),
            Some(Action::Editor(Key::Redo))
        );
        assert_eq!(
            route_key("y", true, false, false),
            Some(Action::Editor(Key::Redo))
        );
        assert_eq!(route_key("z", false, false, false), None);
        assert_eq!(route_key("y", false, false, false), None);
        assert_eq!(
            route_key("t", false, false, false),
            Some(Action::Editor(Key::AddTrack))
        );
        assert_eq!(route_key("t", true, false, false), None);
        assert_eq!(
            route_key("s", false, false, false),
            Some(Action::Editor(Key::SplitClip))
        );
        assert_eq!(
            route_key("delete", false, false, false),
            Some(Action::Editor(Key::DeleteClip))
        );
        assert_eq!(
            route_key("backspace", false, false, false),
            Some(Action::Editor(Key::DeleteClip))
        );
        assert_eq!(route_key("delete", true, false, false), None);
        assert_eq!(route_key("x", false, false, false), None);
    }

    #[test]
    fn replace_clears_looping_notice_and_invalidates_pending_proposals() {
        let mut controller = Controller::default();
        controller.replace(
            document(&["add-track", "add-clip 0 0 3840"]),
            PathBuf::from("a.pathos"),
        );
        controller.toggle_loop();
        assert_eq!(controller.looping, Some(0..4 * PPQ));
        controller.notice = Some(Notice::Saved(PathBuf::from("a.pathos")));

        // Submit a proposal against document A (2 edits).
        let Response::Proposed { id } = controller.handle_request(Request::Propose {
            summary: "mute".into(),
            commands: vec!["mute-track 0".into()],
            anchor: None,
        }) else {
            panic!("expected Proposed");
        };
        assert_eq!(
            controller.route_key("enter", false, false),
            Some(Action::ApplyProposal)
        );

        // Replace with a different document B that also has 2 edits.
        let doc_b = document(&["add-track", "set-master-volume 500"]);
        assert_eq!(doc_b.edits().len(), 2);
        controller.replace(doc_b, PathBuf::from("b.pathos"));

        assert_eq!(controller.looping, None);
        assert_eq!(controller.notice, None);
        assert_eq!(controller.document_name(), "b.pathos");
        assert!(!controller.needs_close_confirmation());
        assert_eq!(controller.proposals.status(id), Some(Status::Stale));
        assert_eq!(controller.route_key("enter", false, false), None);
        // Applying does nothing to B.
        assert!(!controller.apply_proposal());
        assert_eq!(
            controller.editor.document().save(),
            "add-track\nset-master-volume 500\n"
        );
    }

    #[test]
    fn replace_with_an_identical_document_still_invalidates_proposals() {
        let mut controller = Controller::default();
        let Response::Proposed { id } = controller.handle_request(Request::Propose {
            summary: "add track".into(),
            commands: vec!["add-track".into()],
            anchor: None,
        }) else {
            panic!("expected Proposed");
        };
        controller.replace(Document::default(), PathBuf::from("fresh.pathos"));
        assert_eq!(controller.proposals.status(id), Some(Status::Stale));
        assert_eq!(
            controller.proposals.pending(controller.editor.document()),
            0
        );
    }

    #[test]
    fn open_save_and_status_precedence_work_end_to_end() {
        let scratch = Scratch::new("open-save");
        let path = scratch.0.join("song.pathos");
        let mut controller = Controller::default();
        assert_eq!(controller.document_name(), "untitled");
        assert!(!controller.needs_close_confirmation());
        assert_eq!(controller.status(None), None);
        assert_eq!(
            controller.status(Some("no device")),
            Some(("No audio output: no device".into(), true))
        );

        // Editing marks the document modified and blocks opening until saved.
        assert!(controller.handle(Input::Key(Key::AddTrack)));
        assert!(controller.needs_close_confirmation());
        assert!(!controller.can_open());
        assert_eq!(
            controller.status(Some("no device")),
            Some(("there are unsaved changes; save them first".into(), true))
        );

        // Saving clears modified state and shows a Saved notice.
        assert!(controller.save_to(path.clone()));
        assert!(!controller.needs_close_confirmation());
        assert!(controller.can_open());
        assert_eq!(
            controller.status(None),
            Some((format!("Saved {}", path.display()), false))
        );

        // A rejected edit overrides the notice in status().
        let _ = controller.handle(Input::Key(Key::Redo));
        assert_eq!(
            controller.status(None),
            Some(("nothing to redo".into(), true))
        );

        // An accepted edit clears both the rejection and the notice.
        assert!(controller.handle(Input::Key(Key::AddTrack)));
        assert_eq!(controller.status(None), None);

        // Opening an existing file replaces the document; opening a missing file sets a Failed notice.
        assert!(controller.open_path(path.clone()));
        assert_eq!(controller.editor.history().present().track_count(), 1);
        assert_eq!(controller.status(None), None);

        let missing = scratch.0.join("missing.pathos");
        assert!(!controller.open_path(missing.clone()));
        assert!(matches!(controller.notice, Some(Notice::Failed(_))));

        // CLI path that does not exist binds Saved without error; existing path loads.
        let mut cli = Controller::default();
        assert!(!cli.open_cli_path(missing.clone()));
        assert_eq!(cli.saved.path(), Some(missing.as_path()));
        assert!(cli.open_cli_path(path));
        assert_eq!(cli.editor.history().present().track_count(), 1);
    }

    #[test]
    fn proposal_apply_dismiss_and_transport_toggles_update_state() {
        let mut controller = Controller {
            notice: Some(Notice::Saved(PathBuf::from("x.pathos"))),
            ..Controller::default()
        };
        let Response::Proposed { id } = controller.handle_request(Request::Propose {
            summary: "setup".into(),
            commands: vec!["add-track".into(), "add-clip 0 0 3840".into()],
            anchor: None,
        }) else {
            panic!("expected Proposed");
        };
        assert!(controller.apply_proposal());
        assert_eq!(controller.notice, None);
        assert_eq!(controller.proposals.status(id), Some(Status::Applied));

        let Response::Proposed { id: second } = controller.handle_request(Request::Propose {
            summary: "mute".into(),
            commands: vec!["mute-track 0".into()],
            anchor: None,
        }) else {
            panic!("expected Proposed");
        };
        controller.dismiss_proposal();
        assert_eq!(controller.proposals.status(second), Some(Status::Dismissed));

        // Loop and metronome toggles and score creation.
        assert_eq!(controller.looping, None);
        controller.toggle_loop();
        assert_eq!(controller.looping, Some(0..4 * PPQ));
        assert!(!controller.metronome);
        controller.toggle_metronome();
        assert!(controller.metronome);

        let rate = NonZeroU32::new(48_000).unwrap();
        let score = controller.score(rate);
        let mut block = [[0.0_f32; 2]; 64];
        // Metronome is on, so the start of bar 1 produces a non-silent click.
        score.render(0, &mut block);
        assert!(block.iter().any(|&[l, r]| l != 0.0 || r != 0.0));

        controller.toggle_loop();
        assert_eq!(controller.looping, None);
        controller.toggle_metronome();
        assert!(!controller.metronome);

        // Verify Editor::apply_proposal with a single command works after toggling.
        assert!(controller.editor.document().edits().iter().any(|e| {
            *e == Edit::Apply(Command::AddClip {
                track: 0,
                start: 0,
                length: 3840,
            })
        }));
    }
}
