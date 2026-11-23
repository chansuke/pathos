---
number: 36
title: Review through the connected agent with measurements of the render and anchored findings
status: accepted
date: 2026-11-23
---

# Review through the connected agent with measurements of the render and anchored findings

## Context and Problem Statement

ADR 6 decided that a review takes the project state and measurements of a
deterministic offline render, per track and on the master, and returns
findings anchored to a track and a time range, each optionally with commands.
ADR 5 keeps generative models out of pathos, so the findings come from an AI
outside it, and ADR 35 already lets an agent read the open project and propose
commands through `pathos-mcp`. Which AI reviews, where are the measurements
computed, and how does a finding reach the user?

## Decision Drivers

* pathos holds no key or account for an AI service.
* Measuring must not stall the window.
* A finding is shown and decided like a proposal, through the same checks.
* No audio leaves the machine without consent (ADR 6).

## Considered Options

* The connected agent reviews, with measurements from `pathos-mcp`
* The window calls an AI service itself
* The window measures and the agent only reads the result

## Decision Outcome

Chosen option: "The connected agent reviews, with measurements from
`pathos-mcp`", because it needs no key in pathos and reuses the proposals of
ADR 35.

The crate `pathos-measure` measures a render at 48 kHz, per track and on the
master: the integrated loudness of ITU-R BS.1770 in LUFS, the true peak in
dBTP from a render oversampled four times, the level of each octave band from
31.5 Hz to 16 kHz, and the phase correlation of the two channels. It also
gives the loudness, true peak and correlation of each bar, so that a finding
can point at bars. A track is measured soloed and unmuted, as it would sound
alone at its volume and pan; the master is measured as it is heard.

The socket gains a request, `file`, for the project file of the open project
(ADR 19). `pathos-mcp` loads it, renders and measures it in its own process,
and answers the tool `get_measurements`, so the window does no more work than
reading the file. The tool `report_finding` sends a proposal with an anchor, a
track or the master and a range of ticks, and with no commands or some. The
window shows a finding like a proposal, with its anchor, and marks the range
on the lanes; a finding without commands can only be dismissed.

No tool sends audio, so the review sends only the project state and the
measurements to the agent's model, which ADR 6 allows without consent, and
the tool's description tells the agent that its findings are limited by that.
A tool that gives the agent audio needs the consent of ADR 6 first.

### Consequences

* Good, because measurement is a pure function of the project file, so its
  tests are exact and it runs without a window.
* Good, because findings and proposals share one path through `step`.
* Bad, because a review needs an agent connected through MCP; there is no
  review in pathos alone.
* Bad, because the agent's findings are only as good as the measurements and
  the agent, and there is no evaluation set yet (ADR 6).
* Neutral, because a song is measured up to ten minutes; a longer one is
  refused rather than rendered.

### Confirmation

The tests of `pathos-measure` check each measurement against signals whose
values are known, and the K-weighting filter against the coefficients that
BS.1770 gives at 48 kHz. The tests of `pathos-agent` check the new request,
the tools and the checks of an anchor.

## Pros and Cons of the Options

### The connected agent reviews, with measurements from `pathos-mcp`

* Good, because it needs no key and no new window.
* Bad, because the user has to connect an agent.

### The window calls an AI service itself

* Good, because review works without an agent, as ADR 6 calls "available by
  default".
* Bad, because pathos must keep a key, pick a service and build a consent
  dialog before the first finding.

### The window measures and the agent only reads the result

* Good, because the measurements would be at hand for the window too.
* Bad, because rendering and measuring a song on the window's thread stalls
  it.

## More Information

ITU-R BS.1770-5, algorithms to measure audio programme loudness and true-peak
audio level: <https://www.itu.int/rec/R-REC-BS.1770>.
