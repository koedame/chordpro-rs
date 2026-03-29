# Parallel Work Rules

## Git Worktree Isolation

- Each Claude Code instance MUST work in a separate git worktree.
- Worktree location: `../chordpro-rs-wt/issue-{N}-{slug}/`
- Never modify files outside your own worktree.
- Each worktree has its own `target/` directory — no build lock contention.

## Setup for a New Task

```bash
# From the main repo checkout
git fetch origin
git worktree add ../chordpro-rs-wt/issue-{N}-{slug} -b issue-{N}-{slug} origin/main
cd ../chordpro-rs-wt/issue-{N}-{slug}
```

## Cleanup after PR Merge

```bash
git worktree remove ../chordpro-rs-wt/issue-{N}-{slug}
git branch -d issue-{N}-{slug}
```

## Port Allocation (for future dev servers)

- If a task needs a network port: use `3000 + issue_number`
  (e.g., issue #5 -> port 3005).
- This prevents port conflicts between parallel instances.

## Shared Files Policy

- `CLAUDE.md`, `.claude/rules/`, `.github/` — coordinate via separate PRs, never
  modify in a feature branch unless that is the PR's purpose.
- `Cargo.toml` workspace changes (adding crates) — one PR at a time, rebase other
  branches after merge.

## Rebase Protocol

When another PR merges to `main` and your branch has conflicts:

```bash
git fetch origin
git rebase origin/main
# resolve conflicts if any
git push --force-with-lease
```
