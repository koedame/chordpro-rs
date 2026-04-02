# Pull Request Workflow

1. All changes enter `main` via pull request — no direct pushes.
2. **Implement + test** — open a PR with code and tests.
3. **CI must pass** (cargo fmt --check, cargo clippy -- -D warnings, cargo test).
4. **`/review` + `/security-review`** — run both reviews (may run in parallel).
5. **Classify findings** by severity (see [Severity Definitions](#severity-definitions)).
6. **Blocking findings** (High, Medium) — fix in the same PR, then return to step 3.
   The subsequent review is a **delta review**: it covers only the fix commits, not the
   entire PR from scratch.
7. **Non-blocking findings** (Low, Nit) — create a GitHub Issue for each finding. These
   do **not** block the PR from merging.
8. **CI must pass on the final commit** — after all review-driven fixes, CI must be
   green again on the HEAD commit.
9. **Merge** — when CI is green on the latest commit, all blocking findings are resolved,
   and non-blocking findings are tracked as issues.

Branch protection enforces that status checks pass on the HEAD commit before merging.
No merge is possible unless CI is green on the latest commit. All PRs are
**squash-merged** (merge commits and rebase merging are disabled).

### Severity Definitions

| Severity | Blocks PR/Phase | Definition |
|----------|-----------------|------------|
| High | Yes | Security vulnerabilities, data corruption, crashes |
| Medium | Yes | Spec violations, logic bugs, incorrect output |
| Low | No | Defense-in-depth gaps, minor inconsistencies, portability |
| Nit | No | Style, naming, test coverage suggestions |

### Delta Review

When a review produces blocking findings and fixes are applied, the subsequent review
must only examine the new commits (the fix diff), not re-review the entire PR. This
ensures convergence: fix diffs are small and produce fewer findings, trending toward
zero.

Previously-reviewed code that was not flagged is considered accepted. If a reviewer
later discovers a blocking issue in previously-accepted code, it should be filed as a
separate issue — not raised in the delta review.

### PR Formatting

- PR titles should be concise and written in imperative mood (e.g., "Add chord
  transposition support").
- PR descriptions must include What, Why, Test results, and Review summary sections.
