---
number: 27
title: Put the transport, a readout and the master in a top bar, and the keys in a footer
status: proposed
date: 2026-11-19
---

# Put the transport, a readout and the master in a top bar, and the keys in a footer

## Context and Problem Statement

A status row of 28 pixels holds the master volume, the meter, a line of every
key and the latest message (ADR 24, ADR 25). Playback, the loop and the
metronome work only by key, and nothing shows where the playhead is but a line
on the lanes. The design mock-up that ADR 26 follows puts these in a top bar, a
ruler and a footer. What does each show, and how do its controls take a click?

## Decision Drivers

* Each control keeps one rectangle that the drawing and the hit test share
  (ADR 26).
* The readout must agree with the playhead line and with the schedule's
  rounding of ticks to frames (ADR 15).
* Nothing new goes into the project or the model.

## Considered Options

* A top bar with buttons, a readout and the master, a ruler, and a footer
* The status row as it is, with more in it

## Decision Outcome

Chosen option: "A top bar with buttons, a readout and the master, a ruler, and
a footer", because each control then has room for a label and a click, and the
keys no longer crowd out the messages.

The top bar, 52 pixels high, holds from the left the name of the file with a
dot while it has unsaved changes, a button that returns to the start and one
that plays and stops, a readout, and buttons that turn the loop and the
metronome on and off, each showing its key. The master volume and the meter
sit at its right edge, so their place depends on the window's width, which the
editor is told on each frame. A click on a button acts as its key does.

The readout shows the playhead's tick as bar, beat and sixteenth counted from
1, with four beats a bar as the lanes draw them; the time of the playhead's
frame in minutes, seconds and milliseconds, rounded down; and the tempo in
force at that tick in beats per minute, rounded to a tenth. The tick is the
last one that starts at or before the frame, as for the playhead line.

The meter shows its zones: green up to -6 dB, yellow up to full scale and red
past it, with a line at full scale and a scale of decibels below. This replaces
ADR 25's bar that turns red as a whole.

The ruler, 32 pixels high between the top bar and the lanes, numbers each bar,
ticks each beat, marks the loop with a bracket and the playhead with a
triangle. The footer, 30 pixels high, lists the keys, or shows the latest
message in their place until the next accepted edit.

### Consequences

* Good, because playback, the loop and the metronome can be used with the
  mouse alone.
* Good, because a message is no longer cut off by the keys.
* Bad, because the lanes start 56 pixels lower.
* Bad, because the readout assumes four beats a bar until a project has a
  meter.
* Neutral, because clicking the ruler to move the playhead, and the piano roll
  of the design, are left to later changes.

### Confirmation

Tests of the readout check bars, beats and sixteenths, times and tempo
rounding. Tests of the layout check the order and centering of the top bar's
controls, that the master volume and the meter follow the window's right
edge, and that the lanes start under the ruler. A test of the editor drags the
master volume where a window of a given width puts it. Screenshots on Xvfb
check the buttons by clicking them.

## Pros and Cons of the Options

### A top bar with buttons, a readout and the master, a ruler, and a footer

* Good, because each part of the window has one job.
* Bad, because the editor must know the window's width.

### The status row as it is, with more in it

* Good, because the lanes keep their place.
* Bad, because the row is already too narrow for its keys and messages.
