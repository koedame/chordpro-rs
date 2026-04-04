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
cargo test -- --ignored  # Run tests requiring external tools
cargo clippy         # Lint (CI uses -D warnings)
cargo fmt --check    # Check formatting (CI enforced)
cargo fmt            # Auto-format code
```

## Architecture

This is a Cargo workspace with five crates:

| Crate | Path | Kind | Dependencies |
|---|---|---|---|
| `chordpro-core` | `crates/core` | lib | *none* (zero external deps) |
| `chordpro-render-text` | `crates/render-text` | lib | `chordpro-core` |
| `chordpro-render-html` | `crates/render-html` | lib | `chordpro-core` |
| `chordpro-render-pdf` | `crates/render-pdf` | lib | `chordpro-core` |
| `chordpro-rs` (CLI) | `crates/cli` | bin | `chordpro-core`, `chordpro-render-text`, `chordpro-render-html`, `chordpro-render-pdf` |

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

1. **Phase 1** — Workspace setup, CI, project scaffolding ✅
2. **Phase 2** — Core parser (ChordPro format lexer + AST) ✅
3. **Phase 3** — Plain text renderer ✅
4. **Phase 4** — CLI tool with file I/O ✅
5. **Phase 5** — Extended directives, metadata, transposition ✅
6. **Phase 6** — Additional renderers (HTML, PDF) ✅
7. **Phase 7** — Missing metadata, `{meta}`, `{transpose}` directives ✅
8. **Phase 8** — Additional section environments (grid, custom, chorus recall) ✅
9. **Phase 9** — Inline markup parsing and rendering ✅
10. **Phase 10** — Font, size, and color directives (legacy formatting) ✅
11. **Phase 11** — Page control and multi-page PDF ✅
12. **Phase 12** — Image directive ✅
13. **Phase 13** — Configuration file system (chordpro.json, RRJSON) ✅
14. **Phase 14** — Chord diagram rendering and extended `{define}` ✅
15. **Phase 15** — Delegate environments (ABC, Lilypond, SVG, textblock) ✅
16. **Phase 16** — Conditional directive selectors ✅
17. **Phase 17** — Multi-song files and `{new_song}` directive ✅
18. **Phase 18** — Perl reference implementation compatibility testing ✅
19. **Phase 19** — Production readiness and publishing

## Merge Policy

PRs are automatically reviewed and merged:

1. **PR created** — CI runs (fmt, clippy, test)
2. **Auto-review** — Claude reviews with severity classification on CI success
3. **Blocking findings** (High/Medium) — Claude pushes fix commits, delta review follows
4. **Non-blocking findings** (Low/Nit) — issues created, merge not blocked
5. **Auto-merge** — enabled when no blocking findings remain

All PRs are **squash-merged**. Branch protection requires CI to pass on HEAD.
See `.claude/rules/pr-workflow.md` for full details.

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
| Working directory | `../chordpro-rs-wt/issue-{N}-{slug}/` |

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
