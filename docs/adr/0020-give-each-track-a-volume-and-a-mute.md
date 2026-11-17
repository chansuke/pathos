---
number: 20
title: Give each track a volume and a mute
status: proposed
date: 2026-11-17
---

# Give each track a volume and a mute

## Context and Problem Statement

Every track plays at the same level (ADR 15 sums tracks with equal weight "until
the project has a mixer"), and no track can be silenced without deleting its
clips. A user balancing a song needs a volume and a mute on each track. Where do
they live, in what unit, and which part of them does the model specify?

## Decision Drivers

* The project changes only through commands (ADR 2), so a volume or a mute
  change is a command and can be undone.
* A render must stay identical, bit for bit, on every platform (ADR 15), so the
  render may use only `f32` addition, subtraction, multiplication and division.
* The model specifies what has one right answer (ADR 15): which notes sound.
* A project saved before this decision must still load (ADR 19).

## Considered Options

* Volume as a linear gain in thousandths, mute in the model's schedule
* Volume in decibels
* Volume and mute both only in the Rust render

## Decision Outcome

Chosen option: "Volume as a linear gain in thousandths, mute in the model's
schedule", because the gain needs no function of the platform's math library,
and the model can then state that a muted track is never heard.

The project has a list of tracks in place of a track count. A track has a
volume, a whole number from 0 to 2000 that is its gain in thousandths: 1000
plays the track as before, 0 is silent and 2000 doubles it, about +6 dB. A new
track has volume 1000 and is not muted. Three commands change a track:
`set-track-volume track volume`, `mute-track track` and `unmute-track track`.
An unknown track is rejected with `unknown-track`, and a volume above 2000 with
`invalid-volume`. Muting a muted track, or setting the volume it already has,
is accepted and changes nothing.

The model's schedule leaves out the events of muted tracks, so a theorem can
state that no event belongs to a muted track. The volume does not change the
schedule. The render multiplies each note of a track by its volume divided by
1000 in `f32`; at volume 1000 that factor is exactly 1, so the stored render
hash does not change.

The window shows each track's volume in decibels, which is how other DAWs show
it. A drag on the volume applies one command when the button is released, so
that one gesture is one step of undo.

### Consequences

* Good, because files saved before this decision load unchanged: their tracks
  get volume 1000 and are not muted.
* Good, because "a muted track is silent" is checked against the model, not
  only by Rust tests.
* Bad, because a gain in thousandths is coarse at low levels: below about
  -40 dB the steps are larger than 1 dB.
* Bad, because the volume is heard only after the drag ends, not while it
  moves.
* Neutral, because pan, solo and a master volume are left to later decisions.

### Confirmation

The differential tests generate the new commands and compare the replies and
the schedule with the model, including tracks that are muted. A theorem of the
model states that the schedule has no event of a muted track. A render test
checks that a track at volume 500 renders at half the samples of the same track
at 1000, and the stored render hash stays the same.

## Pros and Cons of the Options

### Volume as a linear gain in thousandths, mute in the model's schedule

* Good, because the render stays exact to the bit on every platform.
* Bad, because the window converts between gain and decibels.

### Volume in decibels

* Good, because the stored value is what the user sees.
* Bad, because the render would need a power function, whose results may
  differ by platform, or a table of gains that the model would also need.

### Volume and mute both only in the Rust render

* Good, because the model's schedule does not change.
* Bad, because nothing checks "a muted track is silent" against the model.
