# Reference nixpkgs derivation for chordsketch.
#
# This file is a template for submitting a PR to NixOS/nixpkgs.
# Place it at pkgs/by-name/ch/chordsketch/package.nix in a nixpkgs
# checkout and submit a PR.
#
# Before submitting, update:
#   - `version` to match the latest release tag
#   - `hash` to match the source tarball (see instructions below)
#   - `cargoHash` to match the vendored dependencies
#
# To compute the hashes:
#   nix-prefetch-url --unpack \
#     https://github.com/koedame/chordsketch/archive/refs/tags/v${version}.tar.gz
#   # For cargoHash, set it to "" first, run `nix-build`, and copy the
#   # hash from the error message.

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
    hash = ""; # TODO: compute with nix-prefetch-url --unpack
  };

  cargoHash = ""; # TODO: set to "" and build to get the correct hash

  cargoBuildFlags = [ "--package" "chordsketch" ];
  cargoTestFlags = [ "--package" "chordsketch" ];

  meta = {
    description = "ChordPro file format parser and renderer (text, HTML, PDF)";
    homepage = "https://github.com/koedame/chordsketch";
    changelog = "https://github.com/koedame/chordsketch/blob/main/CHANGELOG.md";
    license = lib.licenses.mit;
    maintainers = [ ]; # TODO: add your nixpkgs maintainer handle
    mainProgram = "chordsketch";
  };
}
