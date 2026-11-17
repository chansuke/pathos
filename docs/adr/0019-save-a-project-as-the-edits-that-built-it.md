---
number: 19
title: Save a project as the edits that built it
status: accepted
date: 2026-11-17
---

# Save a project as the edits that built it

## Context and Problem Statement

pathos cannot save a project, so all work is lost when the window closes. A
saved project must restore more than the project value: the id the next clip
receives lives in the history (ADR 10), and the user expects undo to work after
reopening. The only way to obtain a project is to apply commands (ADR 2). What
does a project file store, and how is it loaded?

## Decision Drivers

* Loading must restore the project, the clip id counter and the undo history
  exactly.
* A project changes only through commands (ADR 2); loading must not build a
  project in some other way that the model does not specify.
* The Lean model is the specification (ADR 3), so the meaning of a file should
  be defined by the model rather than by a second format.
* A file from an older version of pathos should still load.

## Considered Options

* The accepted edits, one per line in the model's trace format
* The project and the counter as a snapshot, for example in JSON
* Commands that rebuild the present project, without the undo history

## Decision Outcome

Chosen option: "The accepted edits, one per line in the model's trace format",
because replaying them restores the project, the counter and the history
exactly, and both the Rust core and the model read the file as it is.

A project file is text in UTF-8. Each line is one accepted edit, a command,
`undo` or `redo`, written as `spec/Pathos/Trace.lean` parses it, for example
`add-clip 0 0 3840`. A rejected edit is not written. Loading applies the lines
in order to an empty history; a line that does not parse or that is rejected
makes the whole load fail with its line number, and nothing is opened. Saving
writes a temporary file and renames it over the old one, so a failed save never
leaves half a file.

The file has no version header. New commands add new words, so an older file
stays valid; a change to the meaning of an existing command would need a new
decision.

### Consequences

* Good, because loading uses the same `History::edit` as editing, so the
  model's theorems about edits hold for a loaded project.
* Good, because the model's executable can check a saved file: fed the file, it
  must reply `ok` to every line and reach the same state as the core.
* Good, because the file is readable and diffs well, and an agent (ADR 5) can
  write one.
* Bad, because the file grows with every edit, including undone ones, and
  loading replays them all. Compacting a file, for example by dropping undone
  branches, is left for later.
* Neutral, because the project and the undo history are restored, but the
  selection and the playback position are not.

### Confirmation

A property test of the core saves the history of a generated trace, loads it
and expects an equal history. A differential test sends a saved file to the
model and expects `ok` on every line and the same final state as the core. The
app is checked by saving, reopening and undoing in the window.

## Pros and Cons of the Options

### The accepted edits, one per line in the model's trace format

* Good, because the counter and the undo history come back without being
  stored separately.
* Good, because the format already exists and is specified by the model.
* Bad, because the file size and the load time grow with the number of edits.

### The project and the counter as a snapshot, for example in JSON

* Good, because the file size follows the size of the project.
* Bad, because loading builds a project without commands, which the model does
  not specify and ADR 2 rules out; the model would need a load function and its
  own proofs.
* Bad, because the undo history is lost unless every past project is stored.

### Commands that rebuild the present project, without the undo history

* Good, because the file size follows the size of the project.
* Bad, because the rebuilt clips get new ids unless deleted ids are replayed
  with placeholder clips, and the undo history is lost.
