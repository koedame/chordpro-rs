{
  description = "ChordSketch — a Rust implementation of the ChordPro file format";

  # Only nixpkgs is needed as an input.  The Rust toolchain bundled with
  # nixpkgs unstable supports Rust 1.85+ and the 2024 edition used by this
  # workspace, so no rust-overlay or similar overlay is required.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      # Platforms for which packages and dev shells are produced.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      # Evaluate `f pkgs` for each system in `systems`.
      forEachSystem = f:
        nixpkgs.lib.genAttrs systems
          (system: f (import nixpkgs { inherit system; }));
    in {
      packages = forEachSystem (pkgs: rec {
        # Build the `chordsketch` CLI from the Cargo workspace.
        #
        # `cargoLock.lockFile` is used for reproducible dependency vendoring
        # without requiring a pre-computed cargoHash.  All crates in this
        # workspace are pure Rust with no native C library dependencies, so no
        # additional `buildInputs` are needed.
        chordsketch = pkgs.rustPlatform.buildRustPackage {
          pname = "chordsketch";
          version = "0.2.0";

          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          # Build only the CLI crate from the workspace; this avoids pulling in
          # the WASM, FFI, and NAPI crates whose build requirements differ.
          cargoBuildFlags = [ "--package" "chordsketch" ];
          cargoTestFlags = [ "--package" "chordsketch" ];

          meta = with pkgs.lib; {
            description = "ChordPro file format renderer and CLI (text, HTML, PDF)";
            homepage = "https://github.com/koedame/chordsketch";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "chordsketch";
            platforms = platforms.all;
          };
        };

        default = chordsketch;
      });

      # Development shell: `rustup` manages the active Rust toolchain (the
      # workspace requires Rust ≥ 1.85; run `rustup show` inside the shell),
      # and `wasm-pack` is required to build the `@chordsketch/wasm` package.
      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustup
            wasm-pack
          ];
        };
      });
    };
}
