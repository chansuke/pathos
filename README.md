# pathos

A desktop app built with [gpui](https://gpui.rs).

## Building

`Cargo.lock` pins `libc` to 0.2.189 because gpui 0.2.2 depends on
`xattr` 0.2.3, which does not compile against later `libc` on Linux. Avoid a
bare `cargo update` until gpui moves off that `xattr` release.

### NixOS / Linux (Nix)

```sh
nix build           # ./result/bin/pathos
nix develop         # shell with the Rust toolchain and gpui's libraries
cargo run
```

On NixOS, Vulkan drivers come from the system, so enable them:

```nix
hardware.graphics.enable = true;
```

On other Linux distributions, the Nix-built binary cannot load the host's GPU
drivers. Run it through [nixGL](https://github.com/nix-community/nixGL), or
build with the host toolchain outside `nix develop`.

### macOS (Apple Silicon)

```sh
nix build
nix develop
cargo run --features gpui/runtime_shaders
```

The Nix build enables `gpui/runtime_shaders` because the Metal shader compiler
ships only with Xcode. With Xcode installed, a plain `cargo run` outside
`nix develop` compiles the shaders ahead of time instead.

Intel Macs are not covered by the flake: nixpkgs unstable dropped
`x86_64-darwin`.
