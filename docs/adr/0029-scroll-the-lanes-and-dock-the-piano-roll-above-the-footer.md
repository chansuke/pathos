---
number: 29
title: Scroll the lanes and dock the piano roll above the footer
status: proposed
date: 2026-11-19
---

# Scroll the lanes and dock the piano roll above the footer

## Context and Problem Statement

The lanes start under the ruler and the piano roll starts under the last lane
(ADR 28). A project with five or more tracks pushes the roll into the footer
and past the bottom of a new window, and nothing in the window scrolls, so its
lowest pitches, and with enough tracks its last lanes, cannot be reached. How
does the window show a project with more tracks than fit?

## Decision Drivers

* Every lane and every pitch of the roll must be reachable in a window of any
  height.
* The lane being edited and the roll of its clip should be visible together.
* The rectangles that the window draws and the hit test reads stay shared
  (ADR 26), so a scrolled lane is hit where it is drawn.
* The scroll position is a view of the project, not part of it.

## Considered Options

* Scroll the lanes, and dock the roll above the footer
* Scroll the lanes and the roll together
* Make the roll a separate window or tab

## Decision Outcome

Chosen option: "Scroll the lanes, and dock the roll above the footer", because
the roll keeps its place and its full height however many tracks there are,
and the lanes alone move.

When a clip is selected, its roll and the roll's ruler sit at the bottom of the
window, right above the footer. The lanes fill the space between the ruler at
the top and the roll, or the footer when no clip is selected. A vertical scroll
of the wheel or trackpad over the lanes or their headers moves them by its
pixels, from the first lane at the top down to the last lane at the bottom of
that space; when every lane fits, they do not move. Lanes, clips, the loop and
the playhead are cut at the edges of that space. A track that is added scrolls
the lanes to their end, so the new track is in view. When the space grows,
because no clip is selected any more or the window is made taller, the lanes
scroll back only as far as they must to end at the last lane.

A point in the lanes is hit at its place in the lanes as scrolled; a point
outside them hits nothing in them. The scroll is held by the editor, not the
project, and is not saved.

### Consequences

* Good, because any number of tracks can be reached, and the roll's pitches
  are always all visible in a window tall enough for them.
* Good, because the roll no longer moves when a track is added.
* Bad, because a window of 800 pixels shows about four lanes while a clip is
  selected, fewer than it showed with one or two tracks before.
* Bad, because a roll does not scroll, so a window shorter than the roll, its
  ruler, the top bar and the footer cuts its lowest pitches.
* Neutral, because scrolling the timeline sideways, and dragging a clip to a
  lane out of view, are left to later decisions.

### Confirmation

Tests of the layout check where the roll sits for a window height, that a
scrolled lane, clip and note are hit where they are drawn, that a point
outside the lanes hits none of them, and how far the lanes may scroll. Tests
of the editor check that a wheel scrolls within those bounds, that adding a
track brings it into view, and that the scroll follows the space as it
grows.

## Pros and Cons of the Options

### Scroll the lanes, and dock the roll above the footer

* Good, because the lanes and the roll are visible together however many
  tracks there are.
* Bad, because the roll's place depends on the window's height, so the layout
  needs it.

### Scroll the lanes and the roll together

* Good, because the layout only shifts everything below the ruler.
* Bad, because with many tracks the roll is out of view while its lane is in
  view, and the reverse.

### Make the roll a separate window or tab

* Good, because the roll could grow to the whole window.
* Bad, because a note and the lanes it plays against are no longer seen
  together, and the window gets a second view to manage.
