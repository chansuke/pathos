# Agent instructions

pathos is a DAW built with [gpui](https://gpui.rs). Build and run it inside
`nix develop`; see `README.md` for each platform.

## Design decisions

Read the accepted ADRs in [docs/adr/README.md](docs/adr/README.md) before
changing the design, and follow them. Record a new significant decision with
`adrs new "<title>"`, then regenerate the index with
`adrs generate toc > docs/adr/README.md`.

## Specification

The Lean model in `spec/` is the specification of `crates/pathos-core` (ADR 3).
The project changes only by applying a `Command` (ADR 2).

- To change the core's behavior, change the model first, then the Rust core.
  Changes to definitions and theorem statements in `spec/` need human review.
- When the differential tests fail, the Rust core is wrong unless the model was
  meant to change. Do not weaken the model, the generators or the comparison to
  make a failure go away.
- State a property even if its proof is `sorry` (ADR 4). When a failing trace
  sits on a boundary of a rule, add it to
  `core_agrees_with_model_on_boundaries`.
- When REAPER disagrees (ADR 33), the cause is pathos, the converter or the
  JSFX instrument; record a deliberate difference in an ADR.

## Before you call a change done

Run `scripts/verify`, then read `target/verify/summary.md`: it says what to
fix for each failed step and lists the items of
[docs/verification.md](docs/verification.md) that the change triggers. Check
each of them. `scripts/verify quick` runs the fast part, as the pre-commit
hook does; `docs/verification.md` lists the other modes, such as `mutants`,
`kani`, `tla` and `reaper`.

The checks hold the rules: `tests/architecture.rs` for the direction of
dependencies, `[workspace.lints]` and `clippy.toml` for the banned APIs,
`scripts/check-escape-hatches.sh` for added lines, the Audit and the Spec
workflow for the proofs, and the pre-push hook for the comparison with REAPER.
When one fails, its message names the fix. Change the design to pass it; do
not silence the check, skip a hook, or write a line that a run did not print.

## When a bug is fixed or a review finds something

First ask whether a lint, a type, a test, the architecture check, or a
`docs/verification.md` item can stop it from coming back, and add that.
Write a rule here only when none of them can express it.
