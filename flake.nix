{
  description = "pathos: a gpui desktop app";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    # REAPER 7.83 crashes when it renders a project; 7.86 does not (ADR 33).
    nixpkgs-reaper.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-reaper,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Libraries gpui and cpal link against or load with dlopen at runtime on
      # Linux (Vulkan via blade, Wayland/X11 windowing, xkbcommon keymaps, fonts,
      # ALSA).
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
          # cpal plays through ALSA (ADR 18).
          alsa-lib
        ];

      # The ALSA plugins that a host's configuration may route the default
      # device through, such as PipeWire's or PulseAudio's. Nix's alsa-lib
      # does not look for them where the host keeps its own.
      alsaPlugins =
        pkgs:
        pkgs.symlinkJoin {
          name = "pathos-alsa-plugins";
          paths = [
            "${pkgs.pipewire}/lib/alsa-lib"
            "${pkgs.alsa-plugins}/lib/alsa-lib"
          ];
        };
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
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath (linuxRuntimeLibs pkgs)} \
                --set-default ALSA_PLUGIN_DIR ${alsaPlugins pkgs}
            '';
          };
        }
      );

      devShells = forAllSystems (
        pkgs:
        let
          inherit (pkgs) lib stdenv;
          cargoTarget = stdenv.hostPlatform.rust.cargoEnvVarTarget;
          # REAPER is unfree, and only it is allowed here.
          reaper =
            (import nixpkgs-reaper {
              inherit (stdenv.hostPlatform) system;
              config.allowUnfreePredicate = package: lib.getName package == "reaper";
            }).reaper;
          default = pkgs.mkShell (
            {
              inputsFrom = [ self.packages.${stdenv.hostPlatform.system}.default ];
              packages =
                with pkgs;
                [
                  adrs
                  cargo-deny
                  cargo-mutants
                  lean4
                  # TLC checks the TLA+ specifications in tla/ (ADR 17).
                  tlaplus
                  clippy
                  rust-analyzer
                  rustfmt
                ]
                ++ lib.optionals stdenv.hostPlatform.isLinux [ mold ];
            }
            // lib.optionalAttrs stdenv.hostPlatform.isLinux {
              LD_LIBRARY_PATH = lib.makeLibraryPath (linuxRuntimeLibs pkgs);
              ALSA_PLUGIN_DIR = alsaPlugins pkgs;

              # Override a host-level ~/.cargo/config.toml linker so binaries link
              # against Nix glibc, matching the libraries in LD_LIBRARY_PATH.
              # RUSTFLAGS replaces target.*.rustflags, whereas the per-target env
              # var would be merged with them.
              "CARGO_TARGET_${cargoTarget}_LINKER" = "cc";
              RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
            }
          );
        in
        {
          inherit default;
        }
        # The comparison with REAPER (ADR 33) renders on a virtual display.
        // lib.optionalAttrs stdenv.hostPlatform.isLinux {
          reaper = default.overrideAttrs (old: {
            nativeBuildInputs = old.nativeBuildInputs ++ [
              reaper
              pkgs.xvfb-run
            ];
            PATHOS_REAPER = "xvfb-run -a reaper";
          });
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
