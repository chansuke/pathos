---
number: 22
title: Loop playback over a range held by the window, not the project
status: accepted
date: 2026-11-18
---

# Loop playback over a range held by the window, not the project

## Context and Problem Statement

Playback runs from the playhead to the end (ADR 18). While working on a few
bars, a user wants them to repeat. Where does the loop range live, and how does
the audio thread jump back without a gap or a frame too many?

## Decision Drivers

* The audio thread never waits, allocates or frees (ADR 18).
* The jump must be exact to the frame, so a loop of one bar stays in time.
* Changing a loop is not an edit of the song: it should not fill the undo
  history.
* The loop must follow tempo changes, which move the frame of every tick.

## Considered Options

* A range in ticks held by the window and carried in the score snapshot
* A range in frames set through the transport's atomics
* A range in the project, set by commands

## Decision Outcome

Chosen option: "A range in ticks held by the window and carried in the score
snapshot", because the snapshot already reaches the audio thread whole and is
rebuilt after every edit, so the range and the tempo it depends on can never
disagree.

The window holds an optional loop range in ticks. `L` sets it to the selected
clip, or to all clips when none is selected, and turns it off when it is on.
The window marks the range on the timeline. Each score snapshot built for the
audio thread carries the range in frames, computed with the same rounding as
the note schedule (ADR 15). When the playhead reaches the end of the range, the
engine continues from its start within the same block, so the loop is exact to
the frame. Playback that starts before the range runs into it and then repeats
it; playback that starts after it runs on. A note that sounds across the end of
the range is cut there, and one that started before its start sounds from the
middle, as the render of those frames is.

The loop is not part of the project: it is not undone, not saved, and the
model does not specify it.

### Consequences

* Good, because the engine reads the loop from the snapshot it already holds,
  without another atomic or lock.
* Good, because a loop never takes a step of undo.
* Bad, because a loop is lost when the window closes, and a file opened later
  plays without it.
* Bad, because a note cut at the end of the range may click.
* Neutral, because dragging the loop's edges and saving the loop in the
  project are left to later decisions.

### Confirmation

Engine tests check that a looped playback is the offline render of the range
repeated, bit for bit, for blocks of several sizes, that it starts before the
range and runs into it, and that playback after the range runs on. A test of the
window checks which range `L` loops.

## Pros and Cons of the Options

### A range in ticks held by the window and carried in the score snapshot

* Good, because the range is converted to frames with the tempo of the same
  snapshot.
* Bad, because changing the loop builds a new snapshot.

### A range in frames set through the transport's atomics

* Good, because changing the loop builds nothing.
* Bad, because the start and the end are two values that the engine may read
  half updated, and the window must convert them again after a tempo edit.

### A range in the project, set by commands

* Good, because the loop is saved with the project.
* Bad, because every change of the loop is a step of undo and a change of the
  model, which the loop does not need.
