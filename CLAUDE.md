# CLAUDE.md — Project Context for Claude Code

## Project Overview

**ChordSketch** is a Rust rewrite of the Perl [ChordPro](https://www.chordpro.org/)
reference implementation. The goal is full compatibility with the ChordPro file format
and rendering pipeline, implemented as a set of focused Rust library crates with a CLI
front-end.

All code, comments, documentation, commit messages, and PR descriptions must be in
**English**.

## Build Commands

```bash
cargo build          # Build all crates
cargo test           # Run all tests
cargo test -- --ignored  # Run tests requiring external tools
cargo clippy         # Lint (CI uses -D warnings)
cargo fmt --check    # Check formatting (CI enforced)
cargo fmt            # Auto-format code
```

## Architecture

This is a Cargo workspace with the following crates:

| Crate | Path | Kind | Dependencies |
|---|---|---|---|
| `chordsketch-core` | `crates/core` | lib | *none* (zero external deps) |
| `chordsketch-render-text` | `crates/render-text` | lib | `chordsketch-core` |
| `chordsketch-render-html` | `crates/render-html` | lib | `chordsketch-core` |
| `chordsketch-render-pdf` | `crates/render-pdf` | lib | `chordsketch-core` |
| `chordsketch` (CLI) | `crates/cli` | bin | `chordsketch-core`, `chordsketch-render-text`, `chordsketch-render-html`, `chordsketch-render-pdf` |
| `chordsketch-lsp` | `crates/lsp` | bin | `chordsketch-core`, `tower-lsp`, `tokio` |
| `chordsketch-wasm` | `crates/wasm` | cdylib | `chordsketch-core`, all renderers, `wasm-bindgen`, `serde` |
| `chordsketch-ffi` | `crates/ffi` | cdylib/staticlib/lib | `chordsketch-core`, all renderers, `uniffi`, `thiserror` |
| `chordsketch-napi` | `crates/napi` | cdylib | `chordsketch-core`, all renderers, `napi`, `napi-derive` |

Additionally, these non-Rust packages exist:

| Package | Path | Description |
|---|---|---|
| `@chordsketch/wasm` | `packages/npm` | npm package, **dual build** (browser ESM + Node.js CJS) with TypeScript types |
| `@chordsketch/node` | `crates/napi` | Native Node.js addon via napi-rs, multi-package prebuilt layout (main resolver + 5 platform packages). See `docs/releasing.md` §napi distribution. |
| Playground | `packages/playground` | Vite-based browser playground (Vanilla TS) |
| Python (`chordsketch`) | `crates/ffi` | Python package via UniFFI + maturin |
| Swift (`ChordSketch`) | `packages/swift` | Swift package with XCFramework |
| Kotlin (`chordsketch`) | `packages/kotlin` | Kotlin/JVM package via JNI |
| Ruby (`chordsketch`) | `packages/ruby` | Ruby gem via UniFFI |
| `@chordsketch/syntaxes` | `syntaxes/` | TextMate grammar and language configuration for ChordPro files (private, not published) |

### Dependency Policy

- `chordsketch-core` must have **zero external dependencies**. All parsing and AST logic
  is implemented from scratch.
- Renderer crates may depend only on `chordsketch-core` and, when justified, minimal
  external crates.
- The CLI crate may use external crates for argument parsing, I/O, etc.

### License Policy

- **SDK layer** (all current crates): MIT
- **Application layer** (future Forum, Playground, Desktop apps): AGPL-3.0-only

## Project Tracking

- **GitHub Project**: https://github.com/orgs/koedame/projects/1/views/1
- **Issues**: https://github.com/koedame/chordsketch/issues

## Merge Policy

PRs are automatically reviewed; **merging is always a human action**.

1. **PR created** — CI runs (fmt, clippy, test, plus workflow-specific smoke jobs)
2. **Auto-review** — Claude reviews with severity classification on CI success
3. **Blocking findings** (High/Medium) — Claude pushes fix commits, delta review follows
4. **Non-blocking findings** (Low/Nit) — issues created, merge not blocked
5. **Ready for human merge** — when there are no blocking findings, Claude posts a
   "Ready for human merge" comment. A human inspects the **full check rollup** (not
   just the required checks listed in branch protection) and performs the squash merge.

All PRs are **squash-merged**. Branch protection requires CI to pass on HEAD. Bots do
NOT run `gh pr merge` — see `.claude/rules/pr-workflow.md` for the rationale.

## Parallel Development with tmux

This project is designed for multiple Claude Code instances working simultaneously
via tmux.

**Key principle**: Each instance works in an isolated git worktree. No shared mutable
state.

| Resource | Isolation Method |
|---|---|
| Git branch | One branch per worktree, named `issue-{N}-{slug}` |
| Build artifacts | Each worktree has its own `target/` directory |
| Network ports | `3000 + issue_number` |
| Working directory | `../chordsketch-wt/issue-{N}-{slug}/` |

**Before starting work**: Always create a fresh worktree from latest `origin/main`.
**After PR merge**: Remove the worktree and local branch.

## Ticket-Driven Development

- No code changes without a corresponding GitHub Issue.
- Branch names must reference the issue number.
- PR descriptions must include `Closes #N`.
- Use `gh issue create` for new work, `gh issue list` to find existing work.

## Compatibility Strategy

- The ChordPro file format specification (https://www.chordpro.org/chordpro/) is the
  primary reference.
- The Perl reference implementation is the source of truth for ambiguous or
  underdocumented behavior.
- Parser behavior is validated via **golden tests**: input `.cho` files paired with
  expected output snapshots.
- Compatibility with the Perl reference implementation is verified by comparing output
  on a shared test corpus.
