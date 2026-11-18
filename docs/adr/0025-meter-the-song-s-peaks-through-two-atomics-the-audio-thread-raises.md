---
number: 25
title: Meter the song's peaks through two atomics the audio thread raises
status: proposed
date: 2026-11-18
---

# Meter the song's peaks through two atomics the audio thread raises

## Context and Problem Statement

A user who sets the track and master volumes (ADR 20, ADR 24) cannot see how
loud playback is, nor whether the sum of the tracks goes past full scale,
where a WAV file or a device clips it. Only the audio thread sees the samples
as they play. How does their level reach the window, and what does the window
show?

## Decision Drivers

* The audio thread never waits, allocates or frees (ADR 18).
* No peak may be missed, however rarely the window draws: a short peak past
  full scale must show.
* The metronome is not part of the song (ADR 23, ADR 24).
* A new protocol between the threads must not need a new specification
  unless it orders more than one value (ADR 17).

## Considered Options

* The peak of each channel since the window last read it, in an atomic each
* The latest block's level, in an atomic each
* A ring buffer of block levels

## Decision Outcome

Chosen option: "The peak of each channel since the window last read it, in an
atomic each", because each block costs one comparison per sample and one
atomic operation per channel, and the window sees the loudest sample however
many blocks pass between its frames.

The transport holds, for the left and the right channel, the bits of the
loudest absolute sample since the window last took them. After rendering the
notes of a block and before adding the clicks of the metronome, the audio
thread raises each to the block's peak with `fetch_max`; the bits of
non-negative floats are in the order of their values, so the larger bits are
the louder sample. The window takes both with `swap` to zero each time it
draws. Each value is a single atomic word that no other value depends on, so
the protocol needs no TLA+ specification: `fetch_max` and `swap` on one word
cannot lose a peak.

The window shows the meter as two bars, left over right, below the master
volume. A bar jumps up to each peak it takes and falls by 24 dB a second, and
its length runs in decibels from -60 dB, empty, to +6 dB, full. A bar past
full scale is red. The window keeps drawing until both bars have fallen empty.

The meter is not part of the project, and the model does not specify it.

### Consequences

* Good, because a peak past full scale shows even when it lasts one sample.
* Good, because the audio thread does no more than a pass over each block it
  already renders.
* Bad, because the meter shows sample peaks, not loudness: a sustained quiet
  note and a short loud one can show the same bar.
* Bad, because the meter shows what playback renders, so it moves only during
  playback, not for a file being written.
* Neutral, because a meter per track, a peak hold and a loudness meter are left
  to later decisions.

### Confirmation

Engine tests check that the peaks taken are the loudest samples of each
channel of the render since the last take, that a take leaves silence, and
that the clicks of the metronome do not move them. Tests of the window check
how a bar rises and falls and where it lies.

## Pros and Cons of the Options

### The peak of each channel since the window last read it, in an atomic each

* Good, because no peak is lost between two frames of the window.
* Bad, because the window's read also resets, so only one reader may take
  them.

### The latest block's level, in an atomic each

* Good, because reading does not change it.
* Bad, because a peak in a block that the window does not read in time is
  never shown.

### A ring buffer of block levels

* Good, because the window could draw the level over time.
* Bad, because it is a protocol of many values, which ADR 17 would have
  specified and checked.
