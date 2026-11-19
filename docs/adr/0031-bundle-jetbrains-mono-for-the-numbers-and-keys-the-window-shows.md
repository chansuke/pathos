---
number: 31
title: Bundle JetBrains Mono for the numbers and keys the window shows
status: proposed
date: 2026-11-20
---

# Bundle JetBrains Mono for the numbers and keys the window shows

## Context and Problem Statement

The window draws all its text in the system's interface font, which gpui
takes from the platform: the system font on macOS, and IBM Plex Sans on Linux,
or whatever font the system falls back to when that is not installed. In most
of these fonts the digits differ in width, so the position, time and volumes
shift sideways as they change during playback or a drag. The design mock-up
that ADR 26 and ADR 27 follow draws the numbers and the keys in JetBrains
Mono. How does the window get a monospaced font that is there on every
platform?

## Decision Drivers

* A number that changes should not move the digits around it.
* The window should look the same on macOS, Linux and Windows, whatever fonts
  a user has installed.
* The font's license must allow it to ship inside the program.

## Considered Options

* Bundle JetBrains Mono and register it when the program starts
* Name a monospaced font each platform installs, such as Menlo or DejaVu Sans
  Mono
* Turn on tabular figures in the interface font

## Decision Outcome

Chosen option: "Bundle JetBrains Mono and register it when the program
starts", because it is the only option that shows the same digits on every
system, and it is the font of the design.

The program embeds the regular and medium weights of JetBrains Mono 2.304
from `assets/fonts`, about 540 KB, and adds them to gpui's text system before
it opens the window. Its license, the SIL Open Font License 1.1, sits beside
them in `assets/fonts/OFL.txt` and allows the font to be bundled with any
software as long as the license goes with it.

The window draws in JetBrains Mono the text that is a number or a key, as the
design does:

* the position, the time and the tempo in the top bar, the position in the
  medium weight;
* the master volume, and the volume and pan of each track;
* the scale of the meter, the bar numbers of the ruler and the beat numbers of
  the piano roll's ruler;
* the names of the C keys on the piano roll's keyboard;
* the keys on the buttons and in the footer.

Names, captions and messages stay in the interface font.

### Consequences

* Good, because numbers no longer shift as they change, and look the same on
  every platform.
* Good, because the window matches the design in its numbers and keys.
* Bad, because the program grows by about 540 KB, and the repository holds two
  font files that change only when the font is updated by hand.
* Neutral, because the interface font is still the platform's; bundling a
  font for the names too is left to a later decision.

### Confirmation

The program fails to start with an error naming the font if gpui rejects the
bundled files. A screenshot of the window under Xvfb shows the numbers and
keys in JetBrains Mono, and the readout keeps its width while playback runs.

## Pros and Cons of the Options

### Bundle JetBrains Mono and register it when the program starts

* Good, because it does not depend on what a user has installed.
* Bad, because the font files are carried in the repository and the program.

### Name a monospaced font each platform installs, such as Menlo or DejaVu Sans Mono

* Good, because nothing is added to the program.
* Bad, because many Linux systems lack any one given font, and the fallback
  is then proportional again.
* Bad, because each platform shows a different font.

### Turn on tabular figures in the interface font

* Good, because the numbers stay in the font of the rest of the window.
* Bad, because only some interface fonts have tabular figures, and the
  fallback fonts on Linux may not.
* Bad, because the keys and the scale stay proportional, unlike the design.
