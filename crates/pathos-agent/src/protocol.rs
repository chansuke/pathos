//! The requests an agent sends on the socket and their responses, one JSON
//! value per line (ADR 35).

use pathos_core::{Document, PPQ, Tick};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    /// The open project.
    Project,
    /// The project file of the open project (ADR 19), from which the
    /// project can be rebuilt exactly, for instance to be measured (ADR 36).
    File,
    /// Propose `commands`, each a line of a project file (ADR 19). A finding
    /// of a review has an anchor, and may have no commands (ADR 36).
    Propose {
        summary: String,
        commands: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Anchor>,
    },
    /// Whether the proposal `id` is still pending.
    Proposal { id: u64 },
}

/// Where in the project a finding is: a track, or the master when there is
/// none, from tick `start` up to `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<u64>,
    pub start: Tick,
    pub end: Tick,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    Project(ProjectState),
    File { text: String },
    Proposed { id: u64 },
    Proposal { id: u64, status: Status },
    Error { message: String },
}

impl Response {
    pub fn error(message: impl ToString) -> Response {
        Response::Error {
            message: message.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The window shows it and the user has not decided.
    Pending,
    Applied,
    Dismissed,
    /// The project changed before the user applied it.
    Stale,
}

/// The project as an agent reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectState {
    /// Ticks per quarter note.
    pub ppq: u64,
    /// In thousandths of a gain of 1.
    pub master_volume: u16,
    pub tracks: Vec<TrackState>,
    pub tempo: Vec<TempoState>,
    pub clips: Vec<ClipState>,
    /// The id that the next added clip, or the right part of the next split,
    /// receives.
    pub next_clip_id: u64,
    /// How many edits built the project. It grows with every edit, so a
    /// different number means that the project changed.
    pub edits: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackState {
    pub index: u64,
    pub volume: u16,
    pub muted: bool,
    pub solo: bool,
    pub pan: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoState {
    pub tick: u64,
    pub us_per_quarter: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipState {
    pub id: u64,
    pub track: u64,
    pub start: u64,
    pub length: u64,
    /// Relative to the start of the clip.
    pub notes: Vec<NoteState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteState {
    pub start: u64,
    pub length: u64,
    pub pitch: u8,
    pub velocity: u8,
}

impl ProjectState {
    pub fn of(document: &Document) -> ProjectState {
        let history = document.history();
        let project = history.present();
        ProjectState {
            ppq: PPQ,
            master_volume: project.master_volume(),
            tracks: (0..project.track_count())
                .filter_map(|index| {
                    let track = project.track(index)?;
                    Some(TrackState {
                        index,
                        volume: track.volume,
                        muted: track.muted,
                        solo: track.solo,
                        pan: track.pan,
                    })
                })
                .collect(),
            tempo: project
                .tempo()
                .iter()
                .map(|change| TempoState {
                    tick: change.tick,
                    us_per_quarter: change.us_per_quarter,
                })
                .collect(),
            clips: project
                .clips()
                .iter()
                .map(|clip| ClipState {
                    id: clip.id,
                    track: clip.track,
                    start: clip.start,
                    length: clip.length,
                    notes: project
                        .notes(clip.id)
                        .map(|note| NoteState {
                            start: note.start,
                            length: note.length,
                            pitch: note.pitch,
                            velocity: note.velocity,
                        })
                        .collect(),
                })
                .collect(),
            next_clip_id: history.next_clip_id(),
            edits: document.edits().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathos_core::{Command, Edit};

    #[test]
    fn requests_and_responses_are_tagged_json() {
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"request":"project"}"#).unwrap(),
            Request::Project
        );
        assert_eq!(
            serde_json::from_str::<Request>(
                r#"{"request":"propose","summary":"a track","commands":["add-track"]}"#
            )
            .unwrap(),
            Request::Propose {
                summary: "a track".into(),
                commands: vec!["add-track".into()],
                anchor: None,
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(
                r#"{"request":"propose","summary":"","commands":[],"anchor":{"start":0,"end":960}}"#
            )
            .unwrap(),
            Request::Propose {
                summary: String::new(),
                commands: vec![],
                anchor: Some(Anchor {
                    track: None,
                    start: 0,
                    end: 960
                }),
            }
        );
        assert_eq!(
            serde_json::to_string(&Response::Proposal {
                id: 2,
                status: Status::Stale
            })
            .unwrap(),
            r#"{"response":"proposal","id":2,"status":"stale"}"#
        );
    }

    #[test]
    fn the_state_shows_clips_with_their_notes_and_the_next_id() {
        let mut document = Document::default();
        for line in [
            "add-track",
            "add-clip 0 960 3840",
            "add-note 0 480 240 60 100",
        ] {
            document.edit(line.parse().unwrap()).unwrap();
        }
        document
            .edit(Edit::Apply(Command::SetTrackMute {
                track: 0,
                muted: true,
            }))
            .unwrap();
        let state = ProjectState::of(&document);
        assert_eq!(
            state.tracks,
            [TrackState {
                index: 0,
                volume: 1000,
                muted: true,
                solo: false,
                pan: 100
            }]
        );
        assert_eq!(
            state.clips,
            [ClipState {
                id: 0,
                track: 0,
                start: 960,
                length: 3840,
                notes: vec![NoteState {
                    start: 480,
                    length: 240,
                    pitch: 60,
                    velocity: 100
                }]
            }]
        );
        assert_eq!(state.tempo.len(), 1);
        assert_eq!((state.next_clip_id, state.edits, state.ppq), (1, 4, 960));
    }
}
