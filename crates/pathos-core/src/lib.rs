//! The project core of pathos: a project value, the commands that change it, an
//! undo history, the schedule of its notes and the project file.
//!
//! The Lean model in `spec/` is the specification of this crate. The
//! differential tests check that both give the same result for every command.

mod file;
mod history;
mod project;
mod schedule;

pub use file::{Document, LoadError, UnrecognizedEdit};
pub use history::{Edit, History};
pub use project::{
    CENTER_PAN, Clip, Command, EditError, MAX_PAN, MAX_PITCH, MAX_TICK, MAX_US_PER_QUARTER,
    MAX_VELOCITY, MAX_VOLUME, Note, PPQ, Project, TempoChange, Tick, Track, UNITY_VOLUME,
};
pub use schedule::{Event, sample_of};
