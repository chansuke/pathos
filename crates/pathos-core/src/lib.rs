//! The project core of pathos: a project value, the commands that change it and
//! an undo history.
//!
//! The Lean model in `spec/` is the specification of this crate. The
//! differential tests check that both give the same result for every command.

mod history;
mod project;

pub use history::{Edit, History};
pub use project::{
    Clip, Command, EditError, MAX_TICK, MAX_US_PER_QUARTER, PPQ, Project, TempoChange, Tick,
};
