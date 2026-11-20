# Development

## Platform notes

- On NixOS, enable Vulkan with `hardware.graphics.enable = true;`. On other
  Linux distributions, run the Nix-built binary through
  [nixGL](https://github.com/nix-community/nixGL), or build outside
  `nix develop`.
- On Linux, the file prompts need an XDG desktop portal.
- On macOS with Xcode installed, a plain `cargo run` outside `nix develop` also
  works.
- On Windows, if a release build cannot find `fxc.exe`, set `GPUI_FXC_PATH` to
  its path in the Windows SDK.

## Audio without the window

Play a short phrase on the default output device:

```sh
cargo run -p pathos-playback --example play
```

Render a short phrase of the built-in instrument to a WAV file:

```sh
cargo run -p pathos-render --example render -- phrase.wav
```

On Linux, the dev shell points ALSA at the PipeWire and PulseAudio plugins from
nixpkgs, since the host's own plugins do not load into Nix's alsa-lib.

## Comparison with REAPER

On Linux, the dev shell `.#reaper` adds REAPER and a virtual display, and the
ignored tests of `pathos-reaper` render each scenario in
`crates/pathos-reaper/scenarios` and generated projects with pathos and with
REAPER:

```sh
nix develop .#reaper -c cargo test -p pathos-reaper -- --ignored
```

REAPER is not free software. Its evaluation lasts 60 days, after which a
license is needed.

## Pinned dependencies

`Cargo.lock` pins `libc` to 0.2.189 because gpui 0.2.2 depends on an `xattr`
release that does not build with newer `libc` on Linux. Avoid a bare
`cargo update`.
