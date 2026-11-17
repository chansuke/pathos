//! Project files (ADR 19): the accepted edits of a history, one per line in the
//! model's trace format (`spec/Pathos/Trace.lean`).

use std::fmt;
use std::str::FromStr;

use crate::history::{Edit, History};
use crate::project::{Command, EditError};

impl fmt::Display for Edit {
    /// The edit as the model's trace format writes it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Edit::Undo => f.write_str("undo"),
            Edit::Redo => f.write_str("redo"),
            Edit::Apply(command) => match command {
                Command::AddTrack => f.write_str("add-track"),
                Command::AddClip {
                    track,
                    start,
                    length,
                } => write!(f, "add-clip {track} {start} {length}"),
                Command::MoveClip { id, track, start } => {
                    write!(f, "move-clip {id} {track} {start}")
                }
                Command::SplitClip { id, tick } => write!(f, "split-clip {id} {tick}"),
                Command::DeleteClip { id } => write!(f, "delete-clip {id}"),
                Command::SetTempo {
                    tick,
                    us_per_quarter,
                } => write!(f, "set-tempo {tick} {us_per_quarter}"),
                Command::RemoveTempo { tick } => write!(f, "remove-tempo {tick}"),
                Command::AddNote {
                    id,
                    start,
                    length,
                    pitch,
                    velocity,
                } => write!(f, "add-note {id} {start} {length} {pitch} {velocity}"),
                Command::RemoveNote { id, start, pitch } => {
                    write!(f, "remove-note {id} {start} {pitch}")
                }
            },
        }
    }
}

/// A line that is not an edit in the trace format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnrecognizedEdit;

impl fmt::Display for UnrecognizedEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not an edit")
    }
}

impl std::error::Error for UnrecognizedEdit {}

impl FromStr for Edit {
    type Err = UnrecognizedEdit;

    /// Parses an edit as the model does: words separated by spaces, numbers
    /// in decimal digits only.
    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut words = line.split(' ').filter(|word| !word.is_empty());
        let name = words.next().ok_or(UnrecognizedEdit)?;
        let numbers = words
            .map(|word| {
                if word.bytes().all(|byte| byte.is_ascii_digit()) {
                    word.parse::<u64>().map_err(|_| UnrecognizedEdit)
                } else {
                    Err(UnrecognizedEdit)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let command = match (name, numbers.as_slice()) {
            ("undo", []) => return Ok(Edit::Undo),
            ("redo", []) => return Ok(Edit::Redo),
            ("add-track", []) => Command::AddTrack,
            ("add-clip", &[track, start, length]) => Command::AddClip {
                track,
                start,
                length,
            },
            ("move-clip", &[id, track, start]) => Command::MoveClip { id, track, start },
            ("split-clip", &[id, tick]) => Command::SplitClip { id, tick },
            ("delete-clip", &[id]) => Command::DeleteClip { id },
            ("set-tempo", &[tick, us_per_quarter]) => Command::SetTempo {
                tick,
                us_per_quarter,
            },
            ("remove-tempo", &[tick]) => Command::RemoveTempo { tick },
            ("add-note", &[id, start, length, pitch, velocity]) => Command::AddNote {
                id,
                start,
                length,
                pitch,
                velocity,
            },
            ("remove-note", &[id, start, pitch]) => Command::RemoveNote { id, start, pitch },
            _ => return Err(UnrecognizedEdit),
        };
        Ok(Edit::Apply(command))
    }
}

/// A history with the accepted edits that built it, which is what a project
/// file stores. Every edit goes through [`Document::edit`], so replaying the
/// edits always gives the history back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    history: History,
    edits: Vec<Edit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The line, counted from 1, is not an edit.
    Unrecognized { line: usize },
    /// The edit on the line, counted from 1, was rejected.
    Rejected { line: usize, error: EditError },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Unrecognized { line } => write!(f, "line {line}: not an edit"),
            LoadError::Rejected { line, error } => write!(f, "line {line}: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl Document {
    pub fn history(&self) -> &History {
        &self.history
    }

    /// The accepted edits, oldest first. They only ever grow, so their number
    /// tells whether a document changed since it was saved.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Applies `edit` to the history and records it if it is accepted.
    pub fn edit(&mut self, edit: Edit) -> Result<(), EditError> {
        self.history.edit(edit)?;
        self.edits.push(edit);
        Ok(())
    }

    /// The project file: each accepted edit on a line of its own.
    pub fn save(&self) -> String {
        self.edits.iter().map(|edit| format!("{edit}\n")).collect()
    }

    /// Replays a project file on an empty history. Fails on the first line
    /// that is not an accepted edit.
    pub fn load(file: &str) -> Result<Self, LoadError> {
        let mut document = Document::default();
        for (index, line) in file.lines().enumerate() {
            let line_number = index + 1;
            let edit: Edit = line
                .trim_ascii()
                .parse()
                .map_err(|_| LoadError::Unrecognized { line: line_number })?;
            document.edit(edit).map_err(|error| LoadError::Rejected {
                line: line_number,
                error,
            })?;
        }
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_an_empty_document() {
        assert_eq!(Document::load(""), Ok(Document::default()));
        assert_eq!(Document::default().save(), "");
    }

    #[test]
    fn a_saved_document_loads_with_its_counter_and_undo_history() {
        let mut document = Document::default();
        for edit in [
            Edit::Apply(Command::AddTrack),
            Edit::Apply(Command::AddClip {
                track: 0,
                start: 0,
                length: 240,
            }),
            Edit::Undo,
        ] {
            document.edit(edit).unwrap();
        }
        let file = document.save();
        assert_eq!(file, "add-track\nadd-clip 0 0 240\nundo\n");
        let mut loaded = Document::load(&file).unwrap();
        assert_eq!(loaded, document);
        loaded.edit(Edit::Redo).unwrap();
        assert_eq!(loaded.history().present().clips()[0].id, 0);
    }

    #[test]
    fn a_rejected_edit_is_not_recorded() {
        let mut document = Document::default();
        assert_eq!(document.edit(Edit::Undo), Err(EditError::NothingToUndo));
        document.edit(Edit::Apply(Command::AddTrack)).unwrap();
        assert_eq!(document.edit(Edit::Redo), Err(EditError::NothingToRedo));
        assert_eq!(document.edits(), [Edit::Apply(Command::AddTrack)]);
        assert_eq!(document.save(), "add-track\n");
    }

    #[test]
    fn loading_fails_on_the_first_bad_line() {
        assert_eq!(
            Document::load("add-track\nadd-track 1\n"),
            Err(LoadError::Unrecognized { line: 2 })
        );
        assert_eq!(
            Document::load("add-track\n\nadd-track\n"),
            Err(LoadError::Unrecognized { line: 2 })
        );
        assert_eq!(
            Document::load("add-track\ndelete-clip 0\n"),
            Err(LoadError::Rejected {
                line: 2,
                error: EditError::UnknownClip
            })
        );
    }

    #[test]
    fn a_load_error_names_its_line() {
        assert_eq!(
            LoadError::Unrecognized { line: 2 }.to_string(),
            "line 2: not an edit"
        );
        assert_eq!(
            LoadError::Rejected {
                line: 3,
                error: EditError::UnknownClip
            }
            .to_string(),
            "line 3: no such clip"
        );
        assert_eq!(UnrecognizedEdit.to_string(), "not an edit");
    }

    #[test]
    fn lines_parse_as_the_model_parses_them() {
        assert_eq!(
            "  add-clip 0  480 240\r".trim_ascii().parse(),
            Ok(Edit::Apply(Command::AddClip {
                track: 0,
                start: 480,
                length: 240
            }))
        );
        for line in [
            "",
            "add-clip 0 +1 2",
            "add-clip 0 -1 2",
            "add-clip 0 1",
            "add-clip\t0 1 2",
            "Add-track",
            "undo 1",
            "add-clip 0 0 18446744073709551616",
        ] {
            assert_eq!(line.parse::<Edit>(), Err(UnrecognizedEdit), "{line:?}");
        }
    }
}
