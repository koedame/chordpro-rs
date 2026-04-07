# Branch Strategy

- **All work requires a GitHub Issue first** — no branch without an issue.
- Branch naming: `issue-{number}-{short-kebab-case}` (e.g., `issue-5-chord-parser`).
- One branch per issue, one issue per branch — strict 1:1 mapping.
- All branches are created from latest `main`.
- After merge, delete the remote branch (already configured via repo settings).

## Bot Branch Exceptions

- Dependabot branches (`dependabot/...`) are managed by GitHub's Dependabot service.
- These do NOT follow the `issue-{N}-{slug}` naming convention — this is expected.
- Claude Code automatically reviews these via `.github/workflows/claude-dependabot.yml` and approves the PR if everything passes. **Merging is always a human action** — see [Pull Request Workflow](pr-workflow.md).
