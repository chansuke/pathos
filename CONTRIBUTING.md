# Contributing

Thank you for your interest in pathos. It is in early development, so open an
issue to discuss a change before sending a large pull request. Small fixes such
as typos can go straight to a pull request.

## Development

Work inside `nix develop`; see [README.md](README.md) for each platform. Before
opening a pull request, run:

```sh
scripts/verify
```

It checks the formatting, clippy, `cargo deny`, the Lean model and its audit,
and the tests, and writes `target/verify/summary.md` with what to fix.
[docs/verification.md](docs/verification.md) lists its other modes and what
to check by hand for each kind of change.

## How pathos changes

- Read the accepted [ADRs](docs/adr/README.md) before changing the design, and
  record a new significant decision as an ADR.
- The Lean model in `spec/` is the specification of `crates/pathos-core`. To
  change the core's behavior, change the model first, then the Rust core.
- When the differential tests fail, fix the Rust core unless the model was
  meant to change. Do not weaken the model, the generators or the comparison.

[AGENTS.md](AGENTS.md) describes these rules in more detail.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
