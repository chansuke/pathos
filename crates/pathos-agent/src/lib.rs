//! How agents read the project open in pathos and propose commands to it
//! (ADR 5, ADR 35).
//!
//! The window listens on a Unix socket ([`Listener`]) and answers each
//! [`Request`] with [`Proposals::handle`]. `pathos-mcp` relays the tool calls
//! of an MCP client to that socket ([`mcp::serve`]). An agent only proposes:
//! the user applies or dismisses each [`Proposal`] in the window.

mod difference;
pub mod mcp;
mod proposal;
mod protocol;
mod socket;

pub use difference::difference;
pub use proposal::{MAX_COMMANDS, Proposal, ProposalError, Proposals};
pub use protocol::{
    ClipState, NoteState, ProjectState, Request, Response, Status, TempoState, TrackState,
};
pub use socket::{BindError, Call, Listener, MAX_REQUEST, request, socket_path};
