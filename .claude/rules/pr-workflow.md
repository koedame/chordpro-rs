# Pull Request Workflow

1. All changes enter `main` via pull request — no direct pushes.
2. **Implement + test** — open a PR with code and tests.
3. **CI must pass** (cargo fmt --check, cargo clippy -- -D warnings, cargo test).
4. **`/review`** — request code review. Fix any issues raised, then return to step 3.
5. **`/security-review`** — request security review. Fix any issues raised, then return
   to step 3.
6. **CI must pass on the final commit** — after all review-driven fixes, CI must be
   green again on the HEAD commit.
7. **Merge** — only when CI is green on the latest commit and both reviews approve.

Branch protection enforces that status checks pass on the HEAD commit before merging.
No merge is possible unless CI is green on the latest commit. All PRs are
**squash-merged** (merge commits and rebase merging are disabled).

### PR Formatting

- PR titles should be concise and written in imperative mood (e.g., "Add chord
  transposition support").
- PR descriptions must include What, Why, Test results, and Review summary sections.
