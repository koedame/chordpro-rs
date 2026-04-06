# Contributing to ChordSketch

Thank you for your interest in contributing to ChordSketch! This document
explains how to get started.

## Language

All contributions must be in **English**. This includes code, comments,
documentation, commit messages, PR titles, PR descriptions, issue titles, and
issue bodies.

## Filing Issues

- Check [existing issues](https://github.com/koedame/chordsketch/issues)
  before opening a new one.
- Use a clear, imperative title (e.g., "Fix chord transposition for sharps").
- Issue bodies should include the following fields:

  - **Goal** — What should be achieved.
  - **Acceptance Criteria** — Checkboxes for done-ness.
  - **Phase** — Which roadmap phase this belongs to (if known).

- For bugs, include steps to reproduce. For features, describe the desired
  behavior.
- Labels: `phase:N`, `type:feature`/`type:bug`/`type:docs`/`type:refactor`,
  `size:small`/`size:medium`/`size:large`, `priority:high`/`priority:medium`/`priority:low`.

## Development Setup

Requires **Rust 1.85** or later.

```bash
git clone https://github.com/koedame/chordsketch.git
cd chordsketch
cargo build
cargo test
```

## Playground Development

The web playground requires building the WASM crate first:

```bash
# Install wasm-pack
cargo install wasm-pack

# Build WASM and copy artifacts
wasm-pack build crates/wasm --target web
cp crates/wasm/pkg/chordsketch_wasm* packages/npm/

# Start the playground dev server
cd packages/playground
npm install
npm run dev
```

## Submitting Pull Requests

1. **Open an issue first.** Every change needs a corresponding GitHub issue.
2. **Create a branch** named `issue-{number}-{short-description}`
   (e.g., `issue-42-fix-transpose`).
3. **Make your changes.** Keep the scope focused — one issue per PR.
4. **Run checks before pushing:**

   ```bash
   cargo fmt        # Auto-format
   cargo clippy     # Lint (must pass with zero warnings)
   cargo test       # All tests must pass
   ```

5. **Open a PR** with `Closes #N` in the description. PRs are squash-merged.

CI runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`
on every PR. A Claude-powered auto-review classifies findings by severity and
may push fix commits directly.

## Code Style

- Follow standard Rust conventions (`rustfmt` defaults, Clippy lints).
- All public items must have doc comments.
- Prefer `Result` over panicking.
- Tests go in `#[cfg(test)] mod tests` or in a `tests/` directory for
  integration tests.

## Dependency Policy

- **`chordsketch-core`** must have **zero external dependencies**. No
  exceptions.
- Renderer crates may use minimal external crates when justified.
- New dependencies must be explained in the PR description.

## Golden Tests

Parser behavior is validated via golden tests: input `.cho` files paired with
expected output snapshots in `crates/core/tests/fixtures/`. New directives or
syntax changes must include golden test coverage.

## Project Board

Track progress and find available work on the
[GitHub Project board](https://github.com/orgs/koedame/projects/1/views/1).

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
