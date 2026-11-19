---
number: 30
title: Hold each channel's peak on the meter for two seconds
status: proposed
date: 2026-11-19
---

# Hold each channel's peak on the meter for two seconds

## Context and Problem Statement

A bar of the meter jumps to each peak and falls 24 dB a second (ADR 25), so a
short peak is gone from it within a fraction of a second, before a user has
read how loud it was. ADR 25 left a peak hold to a later decision, and the
design mock-up that ADR 26 and ADR 27 follow draws a thin mark at each
channel's recent peak. How long does the mark stay, and how does it leave?

## Decision Drivers

* A peak should stay readable for long enough to look up at the meter after
  hearing it.
* The mark should follow the music, not pile up the loudest peak of the whole
  playback.
* The meter is a view of what the audio thread reports; the hold needs nothing
  more from it (ADR 25).

## Considered Options

* Hold for two seconds, then fall like the bar
* Hold until the next louder peak, or until playback starts again
* Hold for two seconds, then drop at once

## Decision Outcome

Chosen option: "Hold for two seconds, then fall like the bar", because the mark
stays long enough to read and then moves smoothly back toward the bar instead
of jumping.

Each channel holds the loudest level it has shown. A peak at or above the held
level replaces it and starts its two seconds again. After two seconds without
such a peak, the held level falls 24 dB a second, like the bar, and the hold
is empty once it falls below -60 dB. Since the bar falls at the same rate from
no louder a peak and starts falling no later, the mark is never below the end
of the bar.

The window draws the mark as a line two pixels wide at the held level, in the
color of the zone it lies in: green up to -6 dB, yellow up to full scale and
red past it. The window keeps drawing until the bars and the holds are empty.

### Consequences

* Good, because a short peak, and above all one past full scale, stays visible
  for two seconds.
* Good, because the hold is computed in the window from the peaks it already
  takes, so the audio thread does not change.
* Bad, because a peak past full scale is shown for two seconds only; a user
  who looks away misses it. A clip indicator that stays until clicked is left
  to a later decision.
* Neutral, because the window draws for up to two seconds longer after
  playback stops.

### Confirmation

Tests of the meter check that a hold rises to each peak, stays for two
seconds, falls 24 dB a second after them and empties below -60 dB, and that
it is never below the bar.

## Pros and Cons of the Options

### Hold for two seconds, then fall like the bar

* Good, because the mark shows recent peaks and leaves smoothly.

### Hold until the next louder peak, or until playback starts again

* Good, because the loudest peak of a whole playback is never missed.
* Bad, because one early peak hides how loud the rest of the music is.

### Hold for two seconds, then drop at once

* Good, because it is the simplest rule.
* Bad, because the mark jumps down to the bar, which draws the eye for no
  reason.
