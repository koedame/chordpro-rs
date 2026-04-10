# Documentation Maintenance

## Policy

Project documentation (`CLAUDE.md`, `.claude/rules/`) is updated via **dedicated PRs**,
not inside feature branches (as per the Shared Files Policy in `parallel-work.md`).

Updates are **event-driven**, not periodic. After a PR merges to `main`, evaluate
whether any of the triggers below apply and, if so, open a dedicated documentation PR
promptly.

## Update Triggers

| Event | What to update |
|---|---|
| New crate added to workspace | `CLAUDE.md` Architecture table |
| New rule or convention agreed upon | `.claude/rules/` — add new file |
| Phase started or completed | `CLAUDE.md` Phase Roadmap |
| Build commands or CI pipeline changed | `CLAUDE.md` Build Commands |
| New workflow or process introduced | `.claude/rules/` — add new file |
| Existing rule no longer applies | Remove or update the relevant `.claude/rules/` file |
| Dependency policy changed | `CLAUDE.md` Dependency Policy section |
| Public function signature or behavior changed | Doc comment on the changed item — verify examples, parameter descriptions, and return-value semantics still match the new implementation |

## Phase Completion Review

When a phase's tracking issue is closed, perform a full review of all documentation
files to ensure they accurately reflect the current state of the project. This is the
one scheduled (non-event-driven) documentation checkpoint.

## Principles

- Keep documentation minimal and accurate — remove stale content rather than
  accumulating it.
- Documentation should describe the current state and rules, not history. Use git
  history for that.
- If a trigger applies but the change is trivial (e.g., a typo fix), it may be batched
  with the next documentation PR rather than requiring its own.
- `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` is enforced in
  CI. Broken intra-doc links, invalid doc-test examples, and rustdoc warnings all fail
  the build. Fix them in the same PR that changes the code.
