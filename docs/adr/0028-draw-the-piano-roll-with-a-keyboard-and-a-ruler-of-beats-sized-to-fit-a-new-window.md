---
number: 28
title: Draw the piano roll with a keyboard and a ruler of beats, sized to fit a new window
status: proposed
date: 2026-11-19
---

# Draw the piano roll with a keyboard and a ruler of beats, sized to fit a new window

## Context and Problem Statement

The piano roll names each C left of its rows, draws its notes in one yellow
and has no beat numbers. The design mock-up that ADR 26 and ADR 27 follow draws
a keyboard, numbers the beats and colors the notes like their track. Since the
top bar and ruler moved the lanes down (ADR 27), a roll under four tracks
would also reach into the footer of a window at its opening size. How is the
roll drawn, and how does it fit?

## Decision Drivers

* The roll's rows are where clicks add and remove notes, so their rectangles
  stay shared by the drawing and the hit test (ADR 26).
* A new window, 1200 by 800 pixels, with four tracks should show the whole
  roll above the footer.
* The roll's range of pitches, C2 to C6, should not shrink.

## Considered Options

* One ruler row above the roll, and rows of 7 pixels
* A header and a ruler above the roll, and rows of 8 pixels
* A larger window at opening

## Decision Outcome

Chosen option: "One ruler row above the roll, and rows of 7 pixels", because it
keeps every pitch and fits four tracks in a new window, at the cost of a pixel
of each row.

A row of 24 pixels between the last lane and the roll names the clip, as its
track and the bar it starts in beside the track's color, and numbers each beat
of the clip as bar and beat. Left of the rows, at the right of the header
column, a keyboard of 72 pixels draws white keys with a line below each, darker
below a C and an F, black keys over three fifths of its width, and the name of
each C. Each row is 7 pixels high, darker for a black key, with a line below
each C. A line marks each sixteenth and a stronger one each beat. A note is
drawn in its track's color (ADR 26). A test checks that four tracks and the
roll end above the footer of a window at its opening size.

### Consequences

* Good, because the roll reads like a keyboard and its notes like their track.
* Good, because a new window with four tracks shows the whole roll.
* Bad, because a row of 7 pixels is a smaller target for a click.
* Bad, because a fifth track still pushes the roll into the footer; the window
  does not scroll yet.
* Neutral, because scrolling and zooming the roll are left to later decisions.

### Confirmation

Tests of the layout check where the roll starts and ends and that it fits a
new window with four tracks. A test of the readout checks the beat numbers. A
screenshot on Xvfb is compared with the design by eye.

## Pros and Cons of the Options

### One ruler row above the roll, and rows of 7 pixels

* Good, because every pitch stays and the roll fits.
* Bad, because rows are smaller.

### A header and a ruler above the roll, and rows of 8 pixels

* Good, because it matches the design row for row.
* Bad, because the roll would end 56 pixels lower, inside the footer.

### A larger window at opening

* Good, because nothing shrinks.
* Bad, because many laptop screens are no taller than 800 pixels.
