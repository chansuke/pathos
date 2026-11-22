---
number: 35
title: Connect agents to the running app through a local socket and an MCP server, as proposals the user accepts
status: accepted
date: 2026-11-21
---

# Connect agents to the running app through a local socket and an MCP server, as proposals the user accepts

## Context and Problem Statement

ADR 5 lets an agent read the project and propose or apply commands, so that the
user can discuss a project with an agent while working on it. pathos has no way
in from outside: the project lives in the window's document, and agents such
as coding assistants talk to tools through the Model Context Protocol (MCP)
over standard input and output. How does an agent reach the project the user
has open, and what may it change without the user?

## Decision Drivers

* The agent works on the project the user has open, not on a copy of its file.
* The user stays in control of what changes in the project (ADR 5).
* Nothing outside the user's account may read or change the project.
* An agent's commands are validated by the core, like any edit (ADR 2).

## Considered Options

* The app listens on a local socket, and an MCP server relays to it
* An MCP server reads and writes the project file
* The app is the MCP server, started by the agent

## Decision Outcome

Chosen option: "The app listens on a local socket, and an MCP server relays to
it", because it reaches the open project and keeps the protocol of agents out
of the app.

The window listens on a Unix socket, `pathos/agent.sock` in
`$XDG_RUNTIME_DIR`, or in the temporary directory when that is not set, or at
`$PATHOS_SOCKET`. The directory is created readable only by the user. A second
window does not take the socket from a running one. Each line on the socket is
a JSON request and gets a JSON response on one line. There are three requests:

* `project`: the open project, its tracks, master volume, tempo map, clips
  with their notes, the id the next clip receives and the number of edits;
* `propose`: a summary and a list of commands in the trace format of project
  files (ADR 19), which become a proposal; and
* `proposal`: whether a proposal is pending, accepted, dismissed or stale.

An agent only proposes. The window checks a proposal by applying its commands
to a copy of the history, and refuses it, naming the first rejected command,
when one is rejected. It shows each proposal it keeps, the oldest first, with its summary
and the difference it makes to the project, and the user applies it with `Enter` or
dismisses it with `Escape`. A proposal is made against the project as it was,
so it becomes stale when the project changes before the user applies it, and
the agent proposes again. Applying a proposal applies its commands as edits, so
undo takes them back one by one.

`pathos-mcp`, in the crate `pathos-agent`, is an MCP server over standard input
and output with the tools `get_project`, `propose_commands` and
`get_proposal`, each a request on the socket.

### Consequences

* Good, because the agent sees the project as the user sees it, including
  unsaved edits.
* Good, because the user sees what a proposal changes before it changes
  anything, and every change goes through `step`.
* Good, because the app knows nothing of MCP, so another protocol needs only
  another relay.
* Bad, because a proposal of several commands takes several steps of undo.
* Bad, because a proposal becomes stale after any edit, even one that does not
  touch it.
* Bad, because only one window at a time can be reached.
* Neutral, because the socket is Unix only, like the platforms the flake
  supports.

### Confirmation

The tests of `pathos-agent` check the requests on a real socket, the
validation and staleness of proposals, the difference shown, and the MCP
messages. The relay and the window call only `Document::edit` and read-only
queries of the core.

## Pros and Cons of the Options

### The app listens on a local socket, and an MCP server relays to it

* Good, because it reaches the open project.
* Bad, because the agent needs the app running.

### An MCP server reads and writes the project file

* Good, because it needs no window and is the smallest to build.
* Bad, because the file and the open window disagree until one reloads, and an
  edit can be lost.

### The app is the MCP server, started by the agent

* Good, because there is no relay.
* Bad, because the agent, not the user, starts the window, and the project the
  user already has open cannot be reached.

## More Information

Model Context Protocol, transports: <https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>.
