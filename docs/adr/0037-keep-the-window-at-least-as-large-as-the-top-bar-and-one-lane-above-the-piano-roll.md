---
number: 37
title: Keep the window at least as large as the top bar and one lane above the piano roll
status: proposed
date: 2026-11-27
---

# Keep the window at least as large as the top bar and one lane above the piano roll

## Context and Problem Statement

The top bar places the transport, the readout and the toggles at fixed
places from the left, and the master volume and the meter from the right
(ADR 24, ADR 27). The piano roll has a fixed height above the footer
(ADR 28), and the lanes scroll in the space left above it (ADR 29). In a
window of 640 by 420 pixels, the master volume covers the readout, the
piano roll covers the top bar and the ruler, and the footer is cut off.
What should the window do when it is made that small?

## Decision Drivers

* No control should cover another or leave the window, since a click on it
  would reach the wrong one.
* The layout is computed in `src/layout.rs`, where tests can check it; the
  rule should live there too.
* The design follows a mock-up for a window of 1200 by 800 pixels.

## Considered Options

* Set a minimum size for the window
* Shrink the piano roll and fold the top bar in a small window
* Leave it as it is

## Decision Outcome

Chosen option: "Set a minimum size for the window", because it keeps every
rule of the layout as it is and needs one number for each side.

`layout::min_window_size` computes the size from the layout. The width
leaves the top bar's padding between the metronome button and the master
volume, which is 1171 pixels. The height holds the top bar, the ruler, one
lane, the ruler of the piano roll, the piano roll and the footer, which is
553 pixels. The window passes it to gpui as its minimum size, and the
window manager keeps the window from getting smaller.

### Consequences

* Good, because no part of the window covers another at any size it can take.
* Good, because the minimum follows the layout when a rect in it changes.
* Bad, because the window cannot be narrower than 1171 pixels, which is
  close to the size it opens at, and does not fit half of a small screen.
* Neutral, because a top bar that folds in a narrow window, and a piano roll
  that scrolls or shrinks, are left to later decisions.

### Confirmation

A test in `src/layout.rs` checks that at the minimum width the master volume
starts exactly the top bar's padding after the metronome button, that one
pixel narrower it would come closer, that at the minimum height one lane
shows above the piano roll, and that the window opens at least that large.
Looking at the window (docs/verification.md) checks that it cannot be made
smaller.

## Pros and Cons of the Options

### Set a minimum size for the window

* Good, because it is one option of the window and a function of the layout.
* Bad, because it limits how the window can be placed on a small screen.

### Shrink the piano roll and fold the top bar in a small window

* Good, because the window fits any screen.
* Bad, because rows of the roll smaller than 7 pixels are hard to click
  (ADR 28), and the top bar needs a second layout.

### Leave it as it is

* Good, because nothing changes.
* Bad, because controls cover each other and clicks reach the wrong one.
