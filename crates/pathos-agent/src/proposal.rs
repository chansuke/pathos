//! The proposals agents make and the user applies or dismisses (ADR 35).

use std::collections::{HashMap, VecDeque};
use std::fmt;

use pathos_core::{Command, Document, Edit, EditError, History, MAX_TICK};

use crate::difference::difference;
use crate::protocol::{Anchor, ProjectState, Request, Response, Status};

/// The most commands one proposal may hold.
pub const MAX_COMMANDS: usize = 1000;

/// Commands that an agent proposes, checked against the project as it was
/// when they were proposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub id: u64,
    pub summary: String,
    pub commands: Vec<Command>,
    /// What the commands change, as [`difference`] tells it.
    pub changes: Vec<String>,
    /// Where a finding of a review is (ADR 36); a proposal has none.
    pub anchor: Option<Anchor>,
    /// The number of edits of the document it was made against.
    base: usize,
}

/// Why a proposal was refused. Commands are counted from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalError {
    Empty,
    /// The anchor of a finding names no track or no range.
    UnknownTrack(u64),
    EmptyRange,
    RangeBeyondLastTick,
    TooLong,
    /// Not a command of the trace format. Undo and redo are not commands.
    Unrecognized {
        index: usize,
        line: String,
    },
    Rejected {
        index: usize,
        line: String,
        error: EditError,
    },
}

impl fmt::Display for ProposalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProposalError::Empty => f.write_str("a proposal needs at least one command"),
            ProposalError::UnknownTrack(track) => {
                write!(f, "the anchor names track {track}, which does not exist")
            }
            ProposalError::EmptyRange => f.write_str("the anchor must end after it starts"),
            ProposalError::RangeBeyondLastTick => {
                write!(f, "the anchor must end by tick {MAX_TICK}")
            }
            ProposalError::TooLong => {
                write!(f, "a proposal may hold at most {MAX_COMMANDS} commands")
            }
            ProposalError::Unrecognized { index, line } => {
                write!(f, "command {index} ({line:?}) is not a command")
            }
            ProposalError::Rejected { index, line, error } => {
                write!(f, "command {index} ({line:?}) is rejected: {error}")
            }
        }
    }
}

impl std::error::Error for ProposalError {}

/// The pending proposals, oldest first, and what became of the others.
#[derive(Debug, Default)]
pub struct Proposals {
    next_id: u64,
    pending: VecDeque<Proposal>,
    decided: HashMap<u64, Status>,
}

impl Proposals {
    /// Answers an agent's request about `document`.
    pub fn handle(&mut self, document: &Document, request: Request) -> Response {
        self.refresh(document);
        match request {
            Request::Project => Response::Project(ProjectState::of(document)),
            Request::File => Response::File {
                text: document.save(),
            },
            Request::Propose {
                summary,
                commands,
                anchor,
            } => match self.submit(document, summary, &commands, anchor) {
                Ok(id) => Response::Proposed { id },
                Err(error) => Response::error(error),
            },
            Request::Proposal { id } => match self.status(id) {
                Some(status) => Response::Proposal { id, status },
                None => Response::error(format!("no proposal {id}")),
            },
        }
    }

    /// Checks `lines` against `document` and keeps them as a proposal if
    /// every command is accepted in turn.
    pub fn propose(
        &mut self,
        document: &Document,
        summary: String,
        lines: &[String],
    ) -> Result<u64, ProposalError> {
        self.submit(document, summary, lines, None)
    }

    /// Checks a finding of a review against `document`: its anchor, and its
    /// commands as for a proposal, of which it may have none (ADR 36).
    pub fn report(
        &mut self,
        document: &Document,
        summary: String,
        lines: &[String],
        anchor: Anchor,
    ) -> Result<u64, ProposalError> {
        self.submit(document, summary, lines, Some(anchor))
    }

    fn submit(
        &mut self,
        document: &Document,
        summary: String,
        lines: &[String],
        anchor: Option<Anchor>,
    ) -> Result<u64, ProposalError> {
        if let Some(anchor) = anchor {
            let project = document.history().present();
            if let Some(track) = anchor.track.filter(|&track| track >= project.track_count()) {
                return Err(ProposalError::UnknownTrack(track));
            }
            if anchor.end <= anchor.start {
                return Err(ProposalError::EmptyRange);
            }
            if anchor.end > MAX_TICK {
                return Err(ProposalError::RangeBeyondLastTick);
            }
        } else if lines.is_empty() {
            return Err(ProposalError::Empty);
        }
        if lines.len() > MAX_COMMANDS {
            return Err(ProposalError::TooLong);
        }
        let mut history: History = document.history().clone();
        let mut commands = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let unrecognized = || ProposalError::Unrecognized {
                index: index + 1,
                line: line.clone(),
            };
            let Ok(Edit::Apply(command)) = line.trim_ascii().parse::<Edit>() else {
                return Err(unrecognized());
            };
            history
                .edit(Edit::Apply(command))
                .map_err(|error| ProposalError::Rejected {
                    index: index + 1,
                    line: line.clone(),
                    error,
                })?;
            commands.push(command);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.pending.push_back(Proposal {
            id,
            summary,
            commands,
            changes: difference(document.history().present(), history.present()),
            anchor,
            base: document.edits().len(),
        });
        Ok(id)
    }

    pub fn status(&self, id: u64) -> Option<Status> {
        if self.pending.iter().any(|proposal| proposal.id == id) {
            Some(Status::Pending)
        } else {
            self.decided.get(&id).copied()
        }
    }

    /// The proposal the window shows, the oldest pending one, after the
    /// proposals made against an earlier project have become stale.
    pub fn shown(&mut self, document: &Document) -> Option<&Proposal> {
        self.refresh(document);
        self.pending.front()
    }

    /// How many proposals are pending.
    pub fn pending(&mut self, document: &Document) -> usize {
        self.refresh(document);
        self.pending.len()
    }

    /// Applies the shown proposal to `document`. A finding without commands
    /// has nothing to apply and stays until it is dismissed.
    pub fn apply(&mut self, document: &mut Document) -> Result<(), EditError> {
        self.refresh(document);
        if self
            .pending
            .front()
            .is_none_or(|proposal| proposal.commands.is_empty())
        {
            return Ok(());
        }
        let Some(proposal) = self.pending.pop_front() else {
            return Ok(());
        };
        // The proposal was checked against this very history, so each
        // command is accepted; stop at the first that is not all the same.
        let applied = proposal
            .commands
            .iter()
            .try_for_each(|&command| document.edit(Edit::Apply(command)));
        self.decided.insert(proposal.id, Status::Applied);
        applied
    }

    pub fn dismiss(&mut self, document: &Document) {
        self.refresh(document);
        if let Some(proposal) = self.pending.pop_front() {
            self.decided.insert(proposal.id, Status::Dismissed);
        }
    }

    fn refresh(&mut self, document: &Document) {
        let edits = document.edits().len();
        while let Some(index) = self.pending.iter().position(|p| p.base != edits) {
            let proposal = self.pending.remove(index).unwrap();
            self.decided.insert(proposal.id, Status::Stale);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    fn document(edits: &[&str]) -> Document {
        let mut document = Document::default();
        for edit in edits {
            document.edit(edit.parse().unwrap()).unwrap();
        }
        document
    }

    #[test]
    fn a_proposal_changes_nothing_until_it_is_applied() {
        let mut document = document(&["add-track"]);
        let mut proposals = Proposals::default();
        let id = proposals
            .propose(
                &document,
                "a clip with a note".into(),
                &lines(&["add-clip 0 0 3840", "add-note 0 0 960 60 100"]),
            )
            .unwrap();
        assert_eq!(document.edits().len(), 1);
        assert_eq!(proposals.status(id), Some(Status::Pending));
        let shown = proposals.shown(&document).unwrap();
        assert_eq!(shown.summary, "a clip with a note");
        assert_eq!(
            shown.changes,
            ["add clip 0 on Track 1, bar 1 beat 1, 4 beats, with 1 note"]
        );

        proposals.apply(&mut document).unwrap();
        assert_eq!(
            document.save(),
            "add-track\nadd-clip 0 0 3840\nadd-note 0 0 960 60 100\n"
        );
        assert_eq!(proposals.status(id), Some(Status::Applied));
        assert_eq!(proposals.shown(&document), None);
        // Undo takes the commands back one by one.
        document.edit(Edit::Undo).unwrap();
        assert_eq!(document.history().present().clips().len(), 1);
    }

    #[test]
    fn a_proposal_is_refused_at_its_first_rejected_command() {
        let document = document(&["add-track"]);
        let mut proposals = Proposals::default();
        let refuse = |proposals: &mut Proposals, commands: &[&str]| {
            proposals
                .propose(&document, String::new(), &lines(commands))
                .unwrap_err()
        };
        assert_eq!(refuse(&mut proposals, &[]), ProposalError::Empty);
        assert_eq!(
            refuse(&mut proposals, &["add-clip 0 0 960", "add-clip 0 480 960"]),
            ProposalError::Rejected {
                index: 2,
                line: "add-clip 0 480 960".into(),
                error: EditError::Overlap
            }
        );
        assert_eq!(
            refuse(&mut proposals, &["add-track", "undo"]),
            ProposalError::Unrecognized {
                index: 2,
                line: "undo".into()
            }
        );
        assert_eq!(
            refuse(&mut proposals, &["add-track"; MAX_COMMANDS + 1]),
            ProposalError::TooLong
        );
        assert_eq!(
            refuse(&mut proposals, &["delete-clip 0"]).to_string(),
            "command 1 (\"delete-clip 0\") is rejected: no such clip"
        );
        assert_eq!(proposals.shown(&document), None);
    }

    #[test]
    fn a_proposal_goes_stale_when_the_project_changes_first() {
        let mut document = document(&[]);
        let mut proposals = Proposals::default();
        let first = proposals
            .propose(&document, "one".into(), &lines(&["add-track"]))
            .unwrap();
        let second = proposals
            .propose(&document, "two".into(), &lines(&["add-track"]))
            .unwrap();
        assert_eq!(proposals.pending(&document), 2);
        proposals.apply(&mut document).unwrap();
        // Applying the first changed the project the second was made against.
        assert_eq!(proposals.status(first), Some(Status::Applied));
        assert_eq!(proposals.pending(&document), 0);
        assert_eq!(proposals.status(second), Some(Status::Stale));

        let third = proposals
            .propose(&document, "three".into(), &lines(&["add-track"]))
            .unwrap();
        document.edit(Edit::Undo).unwrap();
        // An undo changes the project too, and nothing stale is applied.
        proposals.apply(&mut document).unwrap();
        assert_eq!(proposals.status(third), Some(Status::Stale));
        assert_eq!(document.history().present().track_count(), 0);
    }

    #[test]
    fn a_finding_is_anchored_and_may_have_no_commands() {
        let mut document = document(&["add-track", "add-clip 0 0 3840"]);
        let mut proposals = Proposals::default();
        let anchor = |track, start, end| Anchor { track, start, end };
        let id = proposals
            .report(&document, "too loud".into(), &[], anchor(Some(0), 0, 3840))
            .unwrap();
        let shown = proposals.shown(&document).unwrap();
        assert_eq!(shown.anchor, Some(anchor(Some(0), 0, 3840)));
        assert!(shown.changes.is_empty());
        // Nothing to apply: it stays until it is dismissed.
        proposals.apply(&mut document).unwrap();
        assert_eq!(proposals.status(id), Some(Status::Pending));
        assert_eq!(document.edits().len(), 2);
        proposals.dismiss(&document);
        assert_eq!(proposals.status(id), Some(Status::Dismissed));

        let fix = proposals
            .report(
                &document,
                "turn it down".into(),
                &lines(&["set-track-volume 0 500"]),
                anchor(None, 0, 3840),
            )
            .unwrap();
        proposals.apply(&mut document).unwrap();
        assert_eq!(proposals.status(fix), Some(Status::Applied));
        assert_eq!(document.history().present().track(0).unwrap().volume, 500);

        let mut refuse = |anchor| {
            proposals
                .report(&document, String::new(), &[], anchor)
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            refuse(anchor(Some(1), 0, 960)),
            "the anchor names track 1, which does not exist"
        );
        assert_eq!(
            refuse(anchor(None, 960, 960)),
            "the anchor must end after it starts"
        );
        assert_eq!(
            refuse(anchor(None, 0, MAX_TICK + 1)),
            format!("the anchor must end by tick {MAX_TICK}")
        );
    }

    #[test]
    fn the_file_request_gives_the_edits_of_the_project() {
        let document = document(&["add-track", "add-clip 0 0 960", "undo"]);
        assert_eq!(
            Proposals::default().handle(&document, Request::File),
            Response::File {
                text: "add-track\nadd-clip 0 0 960\nundo\n".into()
            }
        );
    }

    #[test]
    fn a_dismissed_proposal_is_not_applied() {
        let mut document = document(&[]);
        let mut proposals = Proposals::default();
        let id = proposals
            .propose(&document, String::new(), &lines(&["add-track"]))
            .unwrap();
        proposals.dismiss(&document);
        proposals.apply(&mut document).unwrap();
        assert_eq!(proposals.status(id), Some(Status::Dismissed));
        assert_eq!(document.edits().len(), 0);
    }

    #[test]
    fn requests_are_answered_from_the_document() {
        let document = document(&["add-track"]);
        let mut proposals = Proposals::default();
        let Response::Project(state) = proposals.handle(&document, Request::Project) else {
            panic!("not a project");
        };
        assert_eq!(state.tracks.len(), 1);
        assert_eq!(
            proposals.handle(
                &document,
                Request::Propose {
                    summary: String::new(),
                    commands: lines(&["add-track"]),
                    anchor: None,
                }
            ),
            Response::Proposed { id: 0 }
        );
        assert_eq!(
            proposals.handle(&document, Request::Proposal { id: 0 }),
            Response::Proposal {
                id: 0,
                status: Status::Pending
            }
        );
        assert_eq!(
            proposals.handle(&document, Request::Proposal { id: 1 }),
            Response::error("no proposal 1")
        );
        assert_eq!(
            proposals.handle(
                &document,
                Request::Propose {
                    summary: String::new(),
                    commands: lines(&["redo"]),
                    anchor: None,
                }
            ),
            Response::error("command 1 (\"redo\") is not a command")
        );
    }
}
