---
number: 24
title: Give the project a master volume that scales the notes, not the metronome
status: proposed
date: 2026-11-18
---

# Give the project a master volume that scales the notes, not the metronome

## Context and Problem Statement

Each track has a volume (ADR 20), but making the whole song louder or quieter
means moving every track's volume. Other DAWs have a master fader. Where does
the master volume live, what does it scale, and does the model specify it?

## Decision Drivers

* The project changes only through commands (ADR 2), so a change of the master
  volume can be undone and is saved with the project (ADR 19).
* A render must stay identical, bit for bit, on every platform (ADR 15), and a
  project made before this decision must render as it did.
* The metronome is an aid to the player, not part of the song (ADR 23).

## Considered Options

* A master volume in the project, applied to each note's level
* A master volume in the project, applied to the sum of the tracks
* A playback volume held by the window

## Decision Outcome

Chosen option: "A master volume in the project, applied to each note's level",
because it is saved and undone like any edit, it changes the WAV file as it
changes playback, and the render needs no further pass over its frames.

The project has a master volume, a whole number from 0 to 2000 that is its gain
in thousandths, as a track's volume is (ADR 20). A new project has master
volume 1000. The command `set-master-volume volume` sets it; a volume above
2000 is rejected with `invalid-volume`, and setting the volume it already has
is accepted and changes nothing. The model holds the master volume and checks
it against the Rust core, but it does not change the schedule.

The render multiplies each note's level in each channel by the master volume
divided by 1000 in `f32`, after the track's gains (ADR 21). At volume 1000 that
factor is exactly 1, so every render and the stored render hash stay as they
were. The metronome's clicks are not scaled, so the click stays audible while
the song is turned down.

The window shows the master volume in decibels at the left of the status row,
above the track headers, and a vertical drag on it changes it as a drag on a
track's volume does: it is heard while it moves and applied as one command when
the button is released (ADR 20). A double click sets it to 0 dB.

### Consequences

* Good, because files saved before this decision load unchanged, with master
  volume 1000.
* Good, because a master volume is undone, saved and rendered like the rest of
  the project.
* Bad, because the master volume and a track's volume multiply in `f32`, so a
  note at track volume 300 and master volume 700 need not equal, bit for bit,
  the note at track volume 210 and master volume 1000.
* Bad, because the render has no limiter, so a master volume above 1000 can
  push the sum of the tracks past full scale, as track volumes already can.
* Neutral, because a master pan, a meter and a limiter are left to later
  decisions.

### Confirmation

The differential tests generate the new command and compare the replies, which
now show the master volume, with the model. A render test checks that a
project at master volume 500 renders at half the samples of the same project at
1000, that the clicks of the metronome do not change, and the stored render
hash stays the same.

## Pros and Cons of the Options

### A master volume in the project, applied to each note's level

* Good, because the render loop is unchanged.
* Bad, because the factor is applied once per note rather than once per frame,
  so its rounding differs from scaling the sum.

### A master volume in the project, applied to the sum of the tracks

* Good, because it is what a master fader does in an analog mixer.
* Bad, because each block takes another pass over its frames, and the clicks
  must be added after it.

### A playback volume held by the window

* Good, because nothing changes in the model or the core.
* Bad, because it is not saved and does not change the WAV file, so it is a
  monitor volume, not a part of the song.
