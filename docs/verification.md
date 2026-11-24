# Verification

Run `scripts/verify` before a pull request. It prints one line per step,
keeps each step's log in `target/verify/logs/`, and writes
`target/verify/summary.md`, which says what to fix for each failed step and
lists the items below that the change triggers.

| Mode | What it runs |
|---|---|
| `scripts/verify quick` | `cargo fmt`, `cargo clippy`, `cargo deny` and the tests of every crate but the core |
| `scripts/verify` | `quick`, then `lake -d spec build`, the audit of `spec/theorems.txt` and `spec/definitions.txt`, `cargo test -p pathos-core`, the check that new proptest seeds are committed, the loom tests and the ADR index |
| `scripts/verify mutants` | `cargo mutants` on the lines of `crates/pathos-core`, `crates/pathos-agent` and `src/editor.rs` changed since `origin/main`, as CI does for a pull request (ADR 13) |
| `scripts/verify tla` | TLC on each specification in `tla/` (ADR 17) |
| `scripts/verify kani [--harness NAME]` | the Kani harnesses of the core, or one of them (ADR 11); run it outside `nix develop` |
| `scripts/verify reaper` | the comparison with REAPER (ADR 33, ADR 34), on Linux |
| `scripts/verify full` | the default, then `tla`, `mutants` and `nix build` |

`--base REV` compares with another revision than `origin/main`.

The items below are what the steps cannot decide on their own. The script
`scripts/verification-checklist.sh [BASE]` prints those whose trigger matches
a change: a path is a shell pattern in which `*` also matches `/`, and an
added-lines pattern is an extended regular expression matched against the
lines the diff adds.

## Adding and removing items

Add an item when a check cannot express the rule and a diff shows when it
applies. Cite the ADR or the commits behind it. Remove an item when a test,
a lint or a CI step now catches the mistake on its own.

## Items

### The model changes first

- Paths: `crates/pathos-core/src/*`
- Check: A change to what a command does starts in `spec/`, and the same pull
  request changes both. If the differential tests fail, the Rust core is
  wrong unless the model was meant to change. Add a failing trace that sits
  on a boundary of a rule to `core_agrees_with_model_on_boundaries`.
- Basis: ADR 2, ADR 3, ADR 4.

### Definitions and theorem statements get a human review

- Paths: `spec/Pathos/*`, `spec/Pathos.lean`
- Check: Name each definition that `spec/definitions.txt` shows as changed
  and each restated theorem in the pull request, so that a reviewer reads
  them. Never weaken a statement to make a proof go through.
- Basis: ADR 9, ADR 12.

### A new command gets a Kani harness and reaches the generators

- Paths: `crates/pathos-core/src/project.rs`
- Check: `harnesses` in `crates/pathos-core/src/project/verification.rs`
  matches every `Command`, but it builds only under Kani, so a new command
  compiles without a harness until Kani builds it. Give it one, run
  `scripts/verify kani --harness <name>`, and check with
  `cargo test -p pathos-core --test differential generators_reach_every_rule -- --nocapture`
  that the generators reach its rule.
- Basis: ADR 11, ADR 16.

### The render agrees with REAPER

- Paths: `crates/pathos-core/*`, `crates/pathos-render/*`, `crates/pathos-reaper/*`
- Check: The pre-push hook runs the comparison (`scripts/verify reaper`)
  before the push; put the line it prints in the pull request. When REAPER disagrees, find
  whether pathos, the converter or the JSFX instrument is wrong, and record
  a deliberate difference in an ADR.
- Basis: ADR 7, ADR 33, ADR 34.

### The render is the same on every platform

- Paths: `crates/pathos-render/src/*`
- Check: The hash test in `crates/pathos-render/tests/render.rs` runs on
  Linux x86_64, Linux arm64 and macOS in CI. A deliberate change to the
  render changes the hash; write the new value with the reason in the commit.
- Basis: ADR 15, ADR 32.

### The audio thread stays within the hand-off

- Paths: `crates/pathos-playback/src/*`, `tla/*`
- Check: The engine does not allocate or free while it fills a block
  (`tests/no_allocation.rs`). A change to `handoff.rs` keeps
  `tla/SnapshotHandoff.tla` in step; run `scripts/verify tla`.
- Basis: ADR 17, ADR 18, ADR 25.

### The window is looked at

- Paths: `src/main.rs`, `src/agent.rs`
- Check: No test draws the window. Run `cargo run` inside `nix develop` and
  look at what the change draws, at the default size and a small one.
- Basis: ADR 26 to ADR 29.
