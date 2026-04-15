# Reference nixpkgs derivation for chordsketch.
#
# This file is a template for submitting a PR to NixOS/nixpkgs.
# Place it at pkgs/by-name/ch/chordsketch/package.nix in a nixpkgs
# checkout and submit a PR.
#
# Verified: builds and passes tests on x86_64-linux with nixpkgs unstable.
#
# When bumping to a new release, update `version` and recompute `hash`
# and `cargoHash` by setting both to "" and building once — nix will
# report the correct SRI hashes in the error messages.

{
  lib,
  rustPlatform,
  fetchFromGitHub,
}:

rustPlatform.buildRustPackage rec {
  pname = "chordsketch";
  version = "0.2.0";

  src = fetchFromGitHub {
    owner = "koedame";
    repo = "chordsketch";
    tag = "v${version}";
    hash = "sha256-8i5rDUVmE5MKqXtXY8bgumEHr8jVS6bk9XClZATwC6E=";
  };

  cargoHash = "sha256-52vnW3E7Fdl2aLOKh+w13Tmf0PaD6FdXqf+YyCV6Yec=";

  cargoBuildFlags = [ "--package" "chordsketch" ];
  cargoTestFlags = [ "--package" "chordsketch" ];

  meta = {
    description = "ChordPro file format renderer and CLI (text, HTML, PDF)";
    homepage = "https://github.com/koedame/chordsketch";
    changelog = "https://github.com/koedame/chordsketch/blob/main/CHANGELOG.md";
    license = lib.licenses.mit;
    maintainers = [ ]; # TODO: add your nixpkgs maintainer handle
    mainProgram = "chordsketch";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  };
}
