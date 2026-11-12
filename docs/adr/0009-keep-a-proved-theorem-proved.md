---
number: 9
title: Keep a proved theorem proved
status: accepted
date: 2026-11-12
links:
- target: 4
  kind: amends
---

# Keep a proved theorem proved

## Context and Problem Statement

ADR 4 lets a theorem be merged with `sorry` and tracks the number of unfinished
proofs as a metric. CI rejects a pull request that removes a theorem, and warns
when one is restated, but `theorems.txt` lists only names and statements. A
pull request that replaces a finished proof with `sorry` therefore leaves the
list unchanged and passes; only the count in the job summary goes up. A proof,
once finished, should not be lost without anyone deciding to lose it.

## Decision Drivers

* A finished proof is assurance that has been paid for; losing it must be
  visible and must fail CI.
* Unfinished theorems must remain mergeable, as ADR 4 decided.
* The check must work from the committed list, without building the model of
  the base commit.

## Considered Options

* Mark each theorem proved or unfinished in `theorems.txt`, and reject a
  theorem that goes from proved to unfinished
* Require every theorem to be proved before merging
* Keep reporting the count only

## Decision Outcome

Chosen option: "Mark each theorem proved or unfinished", because it stops proofs
from being lost while keeping ADR 4's rule that unfinished theorems may merge.

The audit writes each theorem as `name [proved] : statement` or
`name [unfinished] : statement`. A theorem is unfinished when it depends on
`sorryAx`, directly or through another declaration. On a pull request, CI
compares the list with the one at the base commit and fails when a theorem
that was `[proved]` is now `[unfinished]`. Removing a theorem still fails, and
restating one still warns.

This amends ADR 4: proofs are still not required to merge, but a proof that has
been merged is.

### Consequences

* Good, because a lost proof fails CI and is named in the error.
* Good, because finishing a proof shows in the diff of `theorems.txt`.
* Bad, because a change to a definition that breaks a proof must also repair
  the proof, or restate the theorem for review.

### Confirmation

The Spec workflow's step "Check that no theorem is removed or loses its proof"
fails on such a pull request.

## Pros and Cons of the Options

### Mark each theorem proved or unfinished in `theorems.txt`

* Good, because the base status is read from the committed list.
* Good, because the status is computed from the axioms a theorem uses, not by
  searching the source for `sorry`.
* Neutral, because the first list in the new format is compared with an old one
  that has no status, so the check starts on the next change.

### Require every theorem to be proved before merging

* Good, because every stated property holds for the model.
* Bad, because it reverses ADR 4, and `step_preserves_wellFormed` would block
  every pull request until it is proved.

### Keep reporting the count only

* Good, because nothing changes.
* Bad, because a lost proof is hidden when another proof is finished in the
  same change.
