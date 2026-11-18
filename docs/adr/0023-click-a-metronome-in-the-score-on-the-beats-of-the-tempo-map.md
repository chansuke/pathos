---
number: 23
title: Click a metronome in the score, on the beats of the tempo map
status: proposed
date: 2026-11-18
---

# Click a metronome in the score, on the beats of the tempo map

## Context and Problem Statement

A user who plays along with a song, or checks its timing, wants to hear the
beat. Playback has notes only (ADR 18). Where does the metronome sound, how
are its beats placed when the tempo changes, and is it part of the project?

## Decision Drivers

* The audio thread never waits, allocates or frees, and does a bounded amount
  of work per block (ADR 18).
* A click must fall on the frame where a note on that beat starts, at any
  tempo.
* Playback runs past the last clip, so the beats do not end with the song.
* Turning the metronome on is not an edit of the song.

## Considered Options

* Clicks rendered by the score, each beat found from the tempo changes
* Clicks as notes on a hidden track of the project
* A list of click frames built with each score

## Decision Outcome

Chosen option: "Clicks rendered by the score, each beat found from the tempo
changes", because it needs no memory per beat, follows every tempo edit with
the snapshot (ADR 22 does the same for the loop), and keeps the notes' bits as
they are.

The window holds whether the metronome is on, and `K` turns it on and off. A
score built for playback with the metronome on keeps the project's tempo
changes and clicks on every beat: on each tick that is a multiple of a quarter
note, up to the last tick a clip may reach. A beat's frame is computed as the
schedule computes the frame of a tick (ADR 15). A click is a triangle wave of
20 ms at half the level of a note at full velocity, an A6 on the first beat of
each bar of four beats, as the window draws bars, and an A5 on the other
beats. A beat cuts the click of the beat before it, so clicks never add up. The
clicks are added to the notes after them, so a frame's notes have the same
bits with the metronome on or off.

To find the beat at a frame, the score bisects the beats by their frames, which
are in order. Each block turns once for every beat that starts in it, plus
one, and never more often than it has frames, however fast the tempo.

The metronome is not part of the project: it is not undone, not saved, not in
a rendered file, and the model does not specify it.

### Consequences

* Good, because the clicks stay on the beat through tempo changes and loops
  without another list or atomic.
* Good, because a render of the notes, its stored hash and the WAV file do
  not change.
* Bad, because every bar has four beats until a project has a meter.
* Bad, because each beat in a block takes a bisection of about thirty steps,
  each through the tempo changes, which costs more than a list of frames
  would.
* Neutral, because a count-in, the click's level and sound, and saving the
  metronome are left to later decisions.

### Confirmation

Render tests check that the clicks start on the beats at 120 BPM and after a
tempo change, that the first beat of a bar differs from the others and is the
same on every bar, that the clicks add to the notes in blocks of any size, that
at the fastest tempo no frame is louder than one click, and that no beat
clicks after the last tick. The allocation test plays a score with the
metronome on.

## Pros and Cons of the Options

### Clicks rendered by the score, each beat found from the tempo changes

* Good, because it needs memory only for the tempo changes.
* Bad, because finding a beat takes a bisection.

### Clicks as notes on a hidden track of the project

* Good, because the render needs no new sound.
* Bad, because turning the metronome on would be an edit, and the clicks would
  end with the notes added for them.

### A list of click frames built with each score

* Good, because finding a beat is a lookup.
* Bad, because playback runs on without end, so the list would end somewhere,
  or grow with the length of playback.
