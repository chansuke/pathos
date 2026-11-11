# pathos

A DAW (digital audio workstation) built with [gpui](https://gpui.rs).

Early development: the app currently opens an empty window.

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

## Note

`Cargo.lock` pins `libc` to 0.2.189 because gpui 0.2.2 depends on an `xattr`
release that does not build with newer `libc` on Linux. Avoid a bare
`cargo update`.
