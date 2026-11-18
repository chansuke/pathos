# pathos

A DAW (digital audio workstation) built with [gpui](https://gpui.rs). Early
development.

## Usage

| Key | Action |
| --- | --- |
| `T` | Add a track |
| Double click on a lane | Add a one-bar clip |
| `S` | Split the selected clip at the pointer |
| `Delete` | Delete the selected clip |
| `Space` / `Home` | Play or stop / return to the start |
| `L` | Loop the selected clip, or every clip |
| `K` | Toggle the metronome |
| `Ctrl-Z` / `Ctrl-Shift-Z` | Undo / redo |
| `Ctrl-S` / `Ctrl-Shift-S` / `Ctrl-O` | Save / save as / open |

Use `Cmd` instead of `Ctrl` on macOS. Drag clips to move them, click in the
piano roll to add or remove notes, and drag a track's volume and pan to change
them. `cargo run -- song.pathos` opens or creates `song.pathos`. See
[docs/usage.md](docs/usage.md) for every control.

## Build

### Linux (Nix)

```sh
nix develop
cargo run
```

On NixOS, set `hardware.graphics.enable = true;`. On other distributions, use
[nixGL](https://github.com/nix-community/nixGL).

### macOS (Apple Silicon)

```sh
nix develop
cargo run --features gpui/runtime_shaders
```

### Windows

Install [rustup](https://rustup.rs) (MSVC) and the Visual Studio Build Tools
with "Desktop development with C++", then run `cargo run`.

See [docs/development.md](docs/development.md) for platform notes, audio
examples and pinned dependencies.

## Specification

The core in `crates/pathos-core` is tested against a Lean 4 model in `spec/`:

```sh
nix develop
lake -d spec build
cargo test -p pathos-core
```

Design decisions are in [docs/adr](docs/adr/README.md).

## License

[MIT](LICENSE)
