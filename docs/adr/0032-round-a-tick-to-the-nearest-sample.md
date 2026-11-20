---
number: 32
title: Round a tick to the nearest sample
status: proposed
date: 2026-11-20
---

# Round a tick to the nearest sample

## Context and Problem Statement

The schedule places a note at the sample of its tick, which it rounds down
from the exact time (ADR 15). A first comparison with REAPER (ADR 7) rendered
the same notes with a copy of the built-in instrument and found every note-on
and note-off whose exact sample has a fraction of a half or more one sample
later in REAPER, and every other one at the same sample. REAPER rounds to the
nearest sample. ADR 7 requires positions to match exactly, and a difference
from REAPER is either a defect or a deliberate difference. Which rounding should
pathos use?

## Decision Drivers

* A note should start as close to its exact time as a sample allows.
* The position layer of the comparison with REAPER should be exact, without a
  rule to excuse a known difference.
* The rounding must stay a single integer division, so that the sample of a
  tick is exact on every platform (ADR 15).

## Considered Options

* Round to the nearest sample, a half up
* Keep rounding down, and record it as a deliberate difference from REAPER

## Decision Outcome

Chosen option: "Round to the nearest sample, a half up", because it halves the
largest error, from just under a sample to half a sample, and matches REAPER.

The sample of a tick is the exact time in samples, rounded to the nearest
integer, with an exact half rounded up. The model computes it as
`(microsTimesPpq * rate + ppq * 500000) / (ppq * 1000000)`, which is still one
division of natural numbers. The render, playback, the loop range (ADR 22) and
the metronome (ADR 23) all take their samples from it.

### Consequences

* Good, because notes lie closer to their exact time.
* Good, because the position layer compares with REAPER exactly.
* Bad, because a note whose exact sample has a fraction of a half or more
  moves one sample later in every render, playback and saved project played
  again.
* Neutral, because a tick still never lies at an earlier sample than an
  earlier tick.

### Confirmation

The model states that the sample of a tick is monotone, and the differential
tests check the Rust schedule against it. The comparison with REAPER (ADR 7)
checks that note-ons and note-offs lie at the same samples.

## Pros and Cons of the Options

### Round to the nearest sample, a half up

* Good, because the error is at most half a sample.
* Good, because it is what REAPER does.
* Bad, because it changes the model and every render with such a note.

### Keep rounding down, and record it as a deliberate difference from REAPER

* Good, because nothing changes.
* Bad, because the position layer needs an exception, and a defect that moves
  a note by one sample would hide behind it.

## More Information

REAPER 7.86 rendered 120 notes on one track at two tempos of 82.07 and 116.05
BPM, at 48 kHz. Of the notes REAPER and pathos both rendered apart from their
neighbours, 109 note-ons and 109 note-offs, those whose exact sample has a
fraction from 0.002 to 0.498 lay at the same sample, and those with a fraction
from 0.500 to 0.999 lay one sample later in REAPER. At 44.1 kHz and 120 BPM,
a note-on at tick 16, whose exact sample is 367.5, lay at sample 368.
