---
number: 10
title: Never reuse a clip id, even after undo
status: accepted
date: 2026-11-12
---

# Never reuse a clip id, even after undo

## Context and Problem Statement

The project holds the id the next created clip receives, and undo restores the
whole project, counter included. This trace therefore gives two different clips
the same id:

```text
add-track
add-clip 0 0 240     # id 0
undo
add-clip 0 480 240   # id 0 again
```

The model and the Rust core agree, so the differential tests pass, yet the
model's own comment says ids are never reused. An AI review proposes commands
that name clips by id (ADR 6) and may be applied after the user has undone and
edited; a proposal for the first clip would then edit the second. Should an id
name one clip for the lifetime of the history?

## Decision Drivers

* A command that names a clip must never act on a different clip than the one
  it was written for.
* Undo and redo must still restore the project exactly, as the history
  theorems state.
* The model stays the specification; the Rust core follows it (ADR 3).

## Considered Options

* Move the id counter from the project to the history, outside undo
* Define ids as unique only within the current branch of the history
* Restore the counter on undo, but raise it to the highest id ever issued

## Decision Outcome

Chosen option: "Move the id counter from the project to the history", because
it makes an id name one clip for the lifetime of the history with the smallest
change, and leaves undo and redo exact.

The history holds `nextClipId`. Applying a command passes it to `step`, which
returns the project and the counter after the command; a rejected command
changes neither. Undo and redo restore the project and keep the counter. The
invariant that every clip id is below the counter becomes a property of the
project together with the counter.

Detecting a proposal written against an older state (an expected revision) is
left to the agent API that ADR 5 describes; this decision only guarantees that
an id is never given to a second clip.

### Consequences

* Good, because a stale command naming a clip either acts on that clip or fails
  with `unknown-clip`; it never edits another clip.
* Good, because clips stay ordered by id: a new clip always has the highest.
* Bad, because `step` takes and returns the counter, so its signature and the
  statement of `step_preserves_wellFormed` change.
* Neutral, because saving and loading a project must later store the counter
  with it; nothing is saved yet.

### Confirmation

The theorems `step_nextClipId_le` and `edit_nextClipId_le` state that the
counter never decreases, and `step_new_clip_ids` that a clip created by a
command receives an id the counter had not yet reached. The trace above is a
boundary case of the differential tests, and a Rust unit test expects id 1 for
the second clip without consulting the model.

## Pros and Cons of the Options

### Move the id counter from the project to the history, outside undo

* Good, because the counter only ever increases.
* Bad, because the project alone no longer says which id comes next.

### Define ids as unique only within the current branch of the history

* Good, because nothing in the model or the core changes, only a comment.
* Bad, because a stale proposal can edit the wrong clip without any error.

### Restore the counter on undo, but raise it to the highest id ever issued

* Good, because the project keeps its counter.
* Bad, because undo would no longer restore the project exactly, and the
  history would still have to remember the highest id outside the projects.
