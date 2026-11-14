# Agent instructions

pathos is a DAW built with [gpui](https://gpui.rs). Build and run it inside
`nix develop`; see `README.md` for each platform.

## Design decisions

Consult `docs/adr/` before changing the design, and follow the accepted ADRs.
Record a new significant decision as an ADR with `adrs new "<title>"`, then
regenerate the index with `adrs generate toc > docs/adr/README.md`. Check the
records with `adrs doctor`.

## Specification

The Lean model in `spec/` is the specification of `crates/pathos-core` (ADR 3).
The project changes only by applying a `Command` (ADR 2).

- To change the core's behavior, change the model first, then the Rust core.
  Changes to definitions and theorem statements in `spec/` need human review.
- When the differential tests fail, the Rust core is wrong unless the model was
  meant to change. Do not weaken the model, the generators or the comparison to
  make a failure go away.
- Proofs are not required to merge (ADR 4). State a property even if its proof
  is `sorry`, never delete a theorem because its proof is hard, and do not add
  axioms. Never replace a finished proof with `sorry` (ADR 9).
- Keep a failing trace as a regression: commit
  `crates/pathos-core/tests/differential.proptest-regressions`, and add the
  trace to `core_agrees_with_model_on_boundaries` when it sits on a boundary of
  a rule.

Commands:

- `lake -d spec build`: build the model and its executable.
- `cargo test -p pathos-core`: check the Rust core against the model. Build the
  model first. `PROPTEST_CASES` sets the number of generated traces.
- `grep -rw sorry spec --include='*.lean' | wc -l`: count unfinished proofs.
- `cd spec && lake env lean --run Audit.lean`: regenerate `theorems.txt`, which
  marks each theorem proved or unfinished and checks its axioms, and
  `definitions.txt`, which gives each definition a fingerprint of its body.
  Commit both; CI fails when either is out of date, when a theorem is removed
  or when a proved theorem becomes unfinished, and warns on each definition
  that changes.
- `cargo test -p pathos-core --test differential generators_reach_every_rule -- --nocapture`:
  show how often the generators reach each rule. If a rule falls below the
  minimum, make the generators reach it more often; do not lower the minimum.
