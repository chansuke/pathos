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
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Libraries gpui links against or loads with dlopen at runtime on Linux
      # (Vulkan via blade, Wayland/X11 windowing, xkbcommon keymaps, fonts).
      linuxRuntimeLibs =
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
      packages = forAllSystems (
        pkgs:
        let
          inherit (pkgs) lib stdenv;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "pathos";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.pkg-config
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.makeWrapper ]
            # gpui's build script runs bindgen for libdispatch on macOS.
            ++ lib.optionals stdenv.hostPlatform.isDarwin [ pkgs.rustPlatform.bindgenHook ];
            buildInputs = lib.optionals stdenv.hostPlatform.isLinux (linuxRuntimeLibs pkgs);

            # The Metal shader compiler ships only with Xcode, which the Nix
            # sandbox cannot reach, so compile shaders at runtime instead.
            buildFeatures = lib.optionals stdenv.hostPlatform.isDarwin [ "gpui/runtime_shaders" ];

            postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
              wrapProgram $out/bin/pathos \
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath (linuxRuntimeLibs pkgs)}
            '';
          };
        }
      );

      devShells = forAllSystems (
        pkgs:
        let
          inherit (pkgs) lib stdenv;
          cargoTarget = stdenv.hostPlatform.rust.cargoEnvVarTarget;
        in
        {
          default = pkgs.mkShell (
            {
              inputsFrom = [ self.packages.${stdenv.hostPlatform.system}.default ];
              packages =
                with pkgs;
                [
                  adrs
                  cargo-deny
                  lean4
                  clippy
                  rust-analyzer
                  rustfmt
                ]
                ++ lib.optionals stdenv.hostPlatform.isLinux [ mold ];
            }
            // lib.optionalAttrs stdenv.hostPlatform.isLinux {
              LD_LIBRARY_PATH = lib.makeLibraryPath (linuxRuntimeLibs pkgs);

              # Override a host-level ~/.cargo/config.toml linker so binaries link
              # against Nix glibc, matching the libraries in LD_LIBRARY_PATH.
              # RUSTFLAGS replaces target.*.rustflags, whereas the per-target env
              # var would be merged with them.
              "CARGO_TARGET_${cargoTarget}_LINKER" = "cc";
              RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
            }
          );
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
