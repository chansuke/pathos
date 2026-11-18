---
number: 21
title: Solo tracks in the schedule and pan them in a stereo render
status: accepted
date: 2026-11-18
---

# Solo tracks in the schedule and pan them in a stereo render

## Context and Problem Statement

Tracks have a volume and a mute (ADR 20), but a user cannot listen to one track
alone without muting all the others, nor place a track left or right: the
render has one channel (ADR 15). How are solo and pan specified, and what does
pan do to the render, the WAV file and playback?

## Decision Drivers

* The model specifies which notes sound (ADR 15, ADR 20), so solo belongs
  there, next to mute.
* A render must stay identical, bit for bit, on every platform (ADR 15).
* A project made before this decision must sound as it did, and the stored
  render hash should keep its meaning.
* A project saved before this decision must still load (ADR 19).

## Considered Options

* A linear balance with 0 dB at the center
* A constant-power pan law, -3 dB at the center
* Solo only in the window, as a shortcut that mutes the other tracks

## Decision Outcome

Chosen option: "A linear balance with 0 dB at the center", together with solo
in the model's schedule, because a centered track then sounds exactly as it did
in one channel, and the gains need only `f32` multiplication and division.

A track also has a solo and a pan. Two commands set the solo, `solo-track
track` and `unsolo-track track`, and `set-track-pan track pan` sets the pan, a
whole number from 0 (left) through 100 (center) to 200 (right); a pan above
200 is rejected with `invalid-pan`. When any track is soloed, the schedule
leaves out the events of every track that is not; a muted track stays silent
even when soloed. A theorem states that with a soloed track, every event of the
schedule belongs to a soloed track.

The render has two channels. A track's gain on the left is `(200 - pan) / 100`
and on the right `pan / 100`, each at most 1, times its volume (ADR 20). At the
center both gains are exactly 1, so each channel of a project whose tracks are
all centered is the one-channel render as before, bit for bit, and the stored
hash still holds for it. WAV files are written with two channels. Playback
writes the left and right channel to the first two channels of the device, the
mean of both to a device with one channel, and silence to any further channel.

### Consequences

* Good, because "only soloed tracks sound" is checked against the model.
* Good, because files and renders made before this decision stay valid.
* Bad, because a track panned hard left is no louder on the left than at the
  center, so the total level falls as a track moves aside; DAWs that use a
  constant-power law behave differently.
* Bad, because a render now takes twice the memory.
* Neutral, because a pan law setting, a master bus and stereo instruments are
  left to later decisions.

### Confirmation

The differential tests generate the new commands and compare the schedule with
soloed and muted tracks against the model. Render tests check that a centered
project renders each channel as before and that hard left and right put a track
in one channel only. The stored render hash does not change.

## Pros and Cons of the Options

### A linear balance with 0 dB at the center

* Good, because a centered track keeps its level and the stored hash holds.
* Bad, because the total level falls by up to 6 dB as a track is panned aside.

### A constant-power pan law, -3 dB at the center

* Good, because the loudness stays about the same across the pan.
* Bad, because every existing project would play 3 dB quieter, and the gains
  need a square root or a table.

### Solo only in the window, as a shortcut that mutes the other tracks

* Good, because nothing changes in the model or the core.
* Bad, because soloing and unsoloing would overwrite the user's own mutes, and
  each would be several edits.
