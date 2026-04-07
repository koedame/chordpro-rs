# Development Setup

## Rust Toolchain

Minimum supported Rust version: **1.85**

```bash
cargo build          # Build all crates
cargo test           # Run all tests
cargo clippy         # Lint (CI uses -D warnings)
cargo fmt --check    # Check formatting (CI enforced)
```

## WASM and npm Package Development

Required only when working on `crates/wasm` or the `@chordsketch/wasm` npm
package (`packages/npm`). Not needed for core/CLI/renderer development.

```bash
# wasm-pack: builds the WASM package via wasm-bindgen.
# CI pins this exact version via .github/workflows/npm-publish.yml
# (WASM_PACK_VERSION: "0.14.0"); install the same version locally to
# avoid build-output drift between local dev and CI:
cargo install wasm-pack --version 0.14.0
```

Node.js ≥ **22** is required to run the npm package smoke tests. This is
what `.github/workflows/readme-smoke.yml` uses on the `npm-wasm` job.

```bash
# Build both browser and Node targets locally:
cd packages/npm
npm run build              # produces packages/npm/{web,node}/* (gitignored)

# Pack the package for local install testing:
npm pack --dry-run
```

See `docs/releasing.md` "npm publish via CI" for the publish-side quirks.

## External Tools (Optional)

These tools are **not required** for normal development or CI. They are only
needed for delegate environment rendering (ABC, Lilypond) and compatibility
testing against the Perl reference implementation.

### abc2svg

Converts ABC music notation to SVG. Used by `{start_of_abc}` rendering.

| Platform | Install |
|----------|---------|
| Ubuntu/Debian | `sudo apt install nodejs npm && npm install -g abc2svg` |
| macOS | `brew install node && npm install -g abc2svg` |
| Windows | Install [Node.js](https://nodejs.org/), then `npm install -g abc2svg` |

Verify: `abc2svg --version`

### Lilypond

Converts Lilypond notation to SVG/PNG. Used by `{start_of_ly}` rendering.

| Platform | Install |
|----------|---------|
| Ubuntu/Debian | `sudo apt install lilypond` |
| macOS | `brew install lilypond` |
| Windows | Download from [lilypond.org](https://lilypond.org/download.html) |

Verify: `lilypond --version`

### Perl ChordPro (reference implementation)

Used for compatibility testing — comparing our output against the reference.

| Platform | Install |
|----------|---------|
| Ubuntu/Debian | `sudo apt install cpanminus && cpanm App::Music::ChordPro` |
| macOS | `brew install cpanminus && cpanm App::Music::ChordPro` |
| Windows | Install [Strawberry Perl](https://strawberryperl.com/), then `cpanm App::Music::ChordPro` |

Verify: `chordpro --version`

## Running Extended Tests

Tests that require external tools are marked with `#[ignore]` and skipped
by default during `cargo test`.

```bash
# Run only external-tool tests
cargo test --workspace -- --ignored

# Run the Perl compatibility comparison
./scripts/compare-with-perl.sh

# Run both
cargo test --workspace -- --ignored && ./scripts/compare-with-perl.sh
```

## Tool Detection

The `chordsketch_core::external_tool` module provides runtime detection:

```rust
use chordsketch_core::external_tool;

if external_tool::has_abc2svg() {
    // abc2svg is available, proceed with conversion
}
```

Renderers use these checks to gracefully fall back when tools are missing.
