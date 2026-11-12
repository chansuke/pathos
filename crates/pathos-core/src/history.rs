use crate::project::{Command, EditError, Project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Apply(Command),
    Undo,
    Redo,
}

/// A project with the projects before and after it for undo and redo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    /// Earlier projects, most recent last.
    past: Vec<Project>,
    present: Project,
    /// Undone projects, next to redo last.
    future: Vec<Project>,
}

impl History {
    pub fn present(&self) -> &Project {
        &self.present
    }

    pub fn undo_depth(&self) -> usize {
        self.past.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.future.len()
    }

    /// Apply an edit. A rejected edit changes nothing; an accepted command
    /// clears redo.
    pub fn edit(&mut self, edit: Edit) -> Result<(), EditError> {
        match edit {
            Edit::Apply(command) => {
                let next = self.present.apply(command)?;
                self.past.push(std::mem::replace(&mut self.present, next));
                self.future.clear();
            }
            Edit::Undo => {
                let previous = self.past.pop().ok_or(EditError::NothingToUndo)?;
                self.future
                    .push(std::mem::replace(&mut self.present, previous));
            }
            Edit::Redo => {
                let next = self.future.pop().ok_or(EditError::NothingToRedo)?;
                self.past.push(std::mem::replace(&mut self.present, next));
            }
        }
        Ok(())
    }
}
