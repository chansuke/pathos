{
  description = "pathos: a gpui desktop app";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Libraries gpui links against or loads with dlopen at runtime
      # (Vulkan via blade, Wayland/X11 windowing, xkbcommon keymaps, fonts).
      runtimeLibs =
        pkgs: with pkgs; [
          fontconfig
          freetype
          libxkbcommon
          vulkan-loader
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
          libxcb
        ];
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "pathos";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            makeWrapper
            pkg-config
          ];
          buildInputs = runtimeLibs pkgs;

          postFixup = ''
            wrapProgram $out/bin/pathos \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)}
          '';
        };
      });

      devShells = forAllSystems (
        pkgs:
        let
          cargoTarget = pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
            packages = with pkgs; [
              clippy
              mold
              rust-analyzer
              rustfmt
            ];
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);

            # Override a host-level ~/.cargo/config.toml linker so binaries link
            # against Nix glibc, matching the libraries in LD_LIBRARY_PATH.
            # RUSTFLAGS replaces target.*.rustflags, whereas the per-target env
            # var would be merged with them.
            "CARGO_TARGET_${cargoTarget}_LINKER" = "cc";
            RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
