# pathos

A DAW (digital audio workstation) built with [gpui](https://gpui.rs).

Early development. The window shows a timeline of tracks and MIDI clips and a
piano roll for the selected clip:

- `T` adds a track, and a double click on a lane adds a one-bar clip.
- Dragging a clip moves it by whole beats, also to another track.
- `S` splits the selected clip at the beat under the pointer, and `Delete`
  deletes it.
- A click in the piano roll adds a sixteenth note, and a click on a note
  removes it.
- `Ctrl-Z` and `Ctrl-Shift-Z` (`Cmd` on macOS) undo and redo.

- `Space` plays and stops, and `Home` returns to the start. Playback is the
  offline render of ADR 15, sample for sample.
- `Ctrl-S` saves and `Ctrl-Shift-S` saves under a new name; `Ctrl-O` opens a
  file. `cargo run -- song.pathos` opens `song.pathos`, or creates it on the
  first save. A project file lists the edits that built the project, one per
  line, so undo still works after reopening it (ADR 19). On Linux the file
  prompts need an XDG desktop portal. Closing the window with unsaved changes
  asks whether to save them first.

To listen without the window, play a short phrase on the default output device:

```sh
cargo run -p pathos-playback --example play
```

On Linux, the dev shell points ALSA at the PipeWire and PulseAudio plugins from
nixpkgs, since the host's own plugins do not load into Nix's alsa-lib.

## Build

### Linux (Nix)

```sh
nix develop
cargo run
```

On NixOS, enable Vulkan with `hardware.graphics.enable = true;`. On other
distributions, run the Nix-built binary through
[nixGL](https://github.com/nix-community/nixGL), or build outside `nix develop`.

### macOS (Apple Silicon)

```sh
nix develop
cargo run --features gpui/runtime_shaders
```

With Xcode installed, a plain `cargo run` outside `nix develop` also works.

### Windows

Install [rustup](https://rustup.rs) (MSVC toolchain) and the Visual Studio Build
Tools with the "Desktop development with C++" workload, then:

```powershell
cargo run
```

If a release build cannot find `fxc.exe`, set `GPUI_FXC_PATH` to its path in the
Windows SDK.

## Specification

The project core is specified by an executable Lean 4 model in `spec/`. The Rust
core in `crates/pathos-core` is tested against it:

```sh
nix develop
lake -d spec build
cargo test -p pathos-core
```

To listen to the built-in instrument, render a short phrase to a WAV file:

```sh
cargo run -p pathos-render --example render -- phrase.wav
```

Design decisions are recorded in [docs/adr](docs/adr/README.md).

## Note

`Cargo.lock` pins `libc` to 0.2.189 because gpui 0.2.2 depends on an `xattr`
release that does not build with newer `libc` on Linux. Avoid a bare
`cargo update`.

## License

pathos is licensed under the [MIT License](LICENSE).
