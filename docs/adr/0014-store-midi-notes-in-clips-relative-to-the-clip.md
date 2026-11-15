---
number: 14
title: Store MIDI notes in clips, relative to the clip
status: accepted
date: 2026-11-15
---

# Store MIDI notes in clips, relative to the clip

## Context and Problem Statement

A clip has a track, a start and a length, but nothing to play. pathos starts
with MIDI clips, so a clip needs notes before anything can be rendered (ADR 15)
or drawn. Where do notes live, how does a command name one, and what happens to
them when a clip is moved, split or deleted?

## Decision Drivers

* Moving a clip must move its notes without touching each of them.
* A command must name a note unambiguously, so that an agent's proposal (ADR 5,
  ADR 6) either edits the note it was written for or fails.
* The model stays small: no state that a rule does not need.
* A note must be playable as MIDI, where two overlapping notes of the same pitch
  on one channel cannot be told apart.

## Considered Options

* Notes inside the clip, relative to its start, named by start and pitch
* Notes inside the clip with their own ids
* Notes on the track in absolute ticks, with clips as windows onto them

## Decision Outcome

Chosen option: "Notes inside the clip, relative to its start, named by start and
pitch", because moving a clip then changes one value, and a note is named by
what a user sees without a second id counter.

A note has a start, relative to the start of its clip, a length, a pitch and a
velocity. Its length is above zero, it ends at or before the end of the clip,
its pitch is from 0 to 127 and its velocity from 1 to 127. Two notes of the same
pitch in one clip do not overlap; notes of different pitches may. A clip keeps
its notes ordered by start, then pitch, so the order does not depend on the
order of edits.

Two commands edit notes:

* `addNote clip start length pitch velocity` adds a note to a clip;
* `removeNote clip start pitch` removes the note of that pitch starting there.

Changing a note is a removal and an addition. A command to move or resize a
note can be added when the UI needs one.

A new clip has no notes. Moving a clip keeps its notes as they are, so they
move with it. Deleting a clip deletes its notes, and undo restores them with
the clip. Splitting a clip gives the notes that start before the split to the
left clip and the others to the right clip, with their starts reduced by the
length of the left clip. A note that starts before the split and ends after it
is shortened to end at the split; the right clip does not receive its rest,
since a note has one onset.

### Consequences

* Good, because `moveClip` needs no change, and notes cannot leave their clip.
* Good, because a stale command names a note by its clip id, which is never
  reused (ADR 10), and by a start and pitch, so it edits that note or fails.
* Good, because the clip and its notes map directly to a MIDI file or a
  DAWproject note clip.
* Bad, because a command written before a note was removed and added again
  still finds the new note at the same start and pitch.
* Bad, because a split shortens notes that cross it, and undo is the only way
  back.
* Neutral, because a clip cannot be resized yet, so no rule is needed for notes
  that a shorter clip would cut.

### Confirmation

The model in `spec/` states that every command keeps the notes of a
well-formed project well-formed, as part of `step_preserves_wellFormed`, and the
differential tests generate note commands with the generators reaching each new
rule.

## Pros and Cons of the Options

### Notes inside the clip, relative to its start, named by start and pitch

* Good, because the name of a note is what the piano roll shows.
* Bad, because the rule against overlapping notes of one pitch has to be checked
  on every addition.

### Notes inside the clip with their own ids

* Good, because a note keeps its name when it is moved or resized.
* Bad, because the history needs a second counter that undo keeps, as for clips
  (ADR 10), and the user never sees the id.

### Notes on the track in absolute ticks, with clips as windows onto them

* Good, because splitting a clip moves no note.
* Bad, because moving a clip has to move every note under it, and a note that
  no clip covers is state with no meaning.
