---
number: 26
title: Color each track by its place and lay out its header as a strip of controls
status: proposed
date: 2026-11-19
---

# Color each track by its place and lay out its header as a strip of controls

## Context and Problem Statement

The window grew a control at a time: a track header of 112 pixels holds a
name, a pan label, two buttons and a volume label, and every clip is drawn in
one blue whatever its track. A redesign of the window, made as a design
mock-up, puts the controls of a track in a wider header, gives each track a
color and fades the clips of a muted track. Where does a track's color come
from, and how are the header's controls drawn and hit?

## Decision Drivers

* A track's color must not change the project or its file (ADR 19), unless a
  user can choose it.
* Each control keeps one rectangle that the drawing and the hit test share, so
  that what is drawn is what is clicked.
* A volume should read the same in a track's header as on the meter (ADR 25).

## Considered Options

* A color per track from a fixed palette, by the track's place
* A color stored in the project, set by a command
* One color for every clip, as before

## Decision Outcome

Chosen option: "A color per track from a fixed palette, by the track's place",
because it needs no change to the model or the file, and a command to choose a
color can still be added later without changing how a track is drawn.

Track n takes the n-th of six colors, starting over after the sixth. A track
header is 240 pixels wide and a lane 72 pixels high. The header holds a strip
of the track's color at its left edge, its name, its mute and solo buttons on
an upper row, and its pan and volume on a lower row; the rows share the lane's
spare height evenly. The pan is a bar that grows from its center toward the
side the track is panned to, over its label. The volume is a fader whose
position is on the meter's scale, from -60 dB to +6 dB, beside its level in
decibels; dragging and double clicks work as before (ADR 20, ADR 21).

A clip is drawn in its track's color, with a strip naming the bar it starts in
and its notes drawn small below the strip; the clips of a muted track are
fainter, and a selected clip is outlined. The lanes have a faint line on each
beat and a stronger one on each bar.

The top bar, the ruler and the piano roll of the same design follow in later
changes.

### Consequences

* Good, because the project, the model and the file do not change.
* Good, because a muted track can be told apart from the others at a glance.
* Bad, because a track's color moves with its place, so it would change if
  tracks could be reordered or removed.
* Bad, because the wider header leaves the timeline 128 pixels less room.
* Neutral, because choosing a track's color is left to a later decision.

### Confirmation

Tests of the layout check where each control of a header lies, that the rows
share the spare height evenly, and what each point of the header hits. A
screenshot of the window on Xvfb is compared with the design by eye.

## Pros and Cons of the Options

### A color per track from a fixed palette, by the track's place

* Good, because nothing is stored.
* Bad, because a user cannot choose a color.

### A color stored in the project, set by a command

* Good, because a user could choose a color and it would be saved.
* Bad, because it changes the model, the core and the file format for a
  matter of display.

### One color for every clip, as before

* Good, because nothing changes.
* Bad, because the clips of different tracks look alike.
