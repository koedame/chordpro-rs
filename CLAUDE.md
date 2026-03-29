# CLAUDE.md — Project Context for Claude Code

## Project Overview

**chordpro-rs** is a Rust rewrite of the Perl [ChordPro](https://www.chordpro.org/)
reference implementation. The goal is full compatibility with the ChordPro file format
and rendering pipeline, implemented as a set of focused Rust library crates with a CLI
front-end.

All code, comments, documentation, commit messages, and PR descriptions must be in
**English**.

## Build Commands

```bash
cargo build          # Build all crates
cargo test           # Run all tests
cargo clippy         # Lint (CI uses -D warnings)
cargo fmt --check    # Check formatting (CI enforced)
cargo fmt            # Auto-format code
```

## Architecture

This is a Cargo workspace with three crates:

| Crate | Path | Kind | Dependencies |
|---|---|---|---|
| `chordpro-core` | `crates/core` | lib | *none* (zero external deps) |
| `chordpro-render-text` | `crates/render-text` | lib | `chordpro-core` |
| `chordpro` (CLI) | `crates/cli` | bin | `chordpro-core`, `chordpro-render-text` |

### Dependency Policy

- `chordpro-core` must have **zero external dependencies**. All parsing and AST logic
  is implemented from scratch.
- Renderer crates may depend only on `chordpro-core` and, when justified, minimal
  external crates.
- The CLI crate may use external crates for argument parsing, I/O, etc.

## Project Tracking

- **GitHub Project**: https://github.com/orgs/koedame/projects/1/views/1
- **Issues**: https://github.com/koedame/chordpro-rs/issues

## Phase Roadmap

High-level phases:

1. **Phase 1** — Workspace setup, CI, project scaffolding
2. **Phase 2** — Core parser (ChordPro format lexer + AST)
3. **Phase 3** — Plain text renderer
4. **Phase 4** — CLI tool with file I/O
5. **Phase 5** — Extended directives, metadata, transposition
6. **Phase 6** — Additional renderers (HTML, PDF)

## Merge Policy

Pull requests follow this workflow before merging to `main`:

1. **Implement + test** — author opens PR with code and tests
2. **CI passes** — fmt, clippy, test must all be green
3. **`/review`** — code review; fix any issues raised (go back to step 2)
4. **`/security-review`** — security review; fix any issues raised (go back to step 2)
5. **CI passes on the final commit** — after all fixes, CI must be green again
6. **Merge** — only when CI is green on the latest commit and both reviews approve

Branch protection requires status checks to pass on the HEAD commit before merging.
No merge is possible unless CI is green on the latest commit, regardless of how many
fix iterations occurred. All PRs are **squash-merged** (merge commits and rebase
merging are disabled).

## Compatibility Strategy

- The ChordPro file format specification (https://www.chordpro.org/chordpro/) is the
  primary reference.
- The Perl reference implementation is the source of truth for ambiguous or
  underdocumented behavior.
- Parser behavior is validated via **golden tests**: input `.cho` files paired with
  expected output snapshots.
- Compatibility with the Perl reference implementation is verified by comparing output
  on a shared test corpus.
