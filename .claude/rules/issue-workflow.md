# Issue-Driven Workflow

## Issue Requirements

- Every piece of work starts with a GitHub Issue.
- Issue title: imperative mood, concise (e.g., "Implement chord directive parser").
- Issue body must include:
  - **Goal**: What should be achieved
  - **Acceptance Criteria**: Checkboxes for done-ness
  - **Phase**: Which roadmap phase this belongs to

## Issue Labels

| Category | Labels |
|---|---|
| Phase | `phase:1` through `phase:6` (roadmap phases) |
| Type | `type:feature`, `type:bug`, `type:docs`, `type:refactor`, `type:ci`, `type:tracking` |
| Size | `size:small` (< 1 hour), `size:medium` (1-4 hours), `size:large` (4+ hours) |
| Priority | `priority:high`, `priority:medium`, `priority:low` |
| Status | `blocked` (waiting on another issue) |

## Workflow Lifecycle

1. Create Issue with labels.
2. Create worktree + branch from latest `main`.
3. Implement (commits reference issue: `Part of #N` or `Closes #N`).
4. Open PR (title references issue, body has `Closes #N`).
5. CI -> `/review` -> `/security-review` -> merge.
6. Cleanup worktree.

## Tracking Issues & Sub-Issues

- Each **Phase** has a single **tracking issue** labeled `type:tracking` and `phase:N`.
- Tracking issue title format: `Phase N: <description>` (e.g., `Phase 2: Core Parser`).
- Tracking issue body contains a high-level overview; GitHub displays sub-issue relationships automatically in the issue UI.
- Individual tasks are created as **GitHub sub-issues** of the tracking issue.
- Sub-issues follow the same rules as regular issues (imperative title, English only, Goal, Acceptance Criteria, labels).
- Sub-issues inherit the `phase:N` label from their parent tracking issue.
- A phase tracking issue may only be closed after the **Phase Completion Gate**
  (see below).

### Phase Completion Gate

Before closing a phase tracking issue, perform the following review against
`main`:

1. **`/review`** — full code review of the phase's changes on `main`.
2. **`/security-review`** — security review of the phase's changes on `main`.
3. If either review finds issues, create new sub-issues under the phase tracking
   issue for each finding. These sub-issues follow normal workflow (implement,
   PR, CI, review, merge).
4. Repeat steps 1–3 until a review pass produces **no new findings**.
5. Only when both `/review` and `/security-review` pass with no new sub-issues
   may the phase tracking issue be closed.

### Creating Sub-Issue Relationships

```bash
# Get node ID of an issue
gh issue view NUMBER --json id -q .id -R koedame/chordpro-rs

# Link child to parent
gh api graphql -f query='
  mutation {
    addSubIssue(input: {issueId: "PARENT_NODE_ID", subIssueId: "CHILD_NODE_ID"}) {
      subIssue { id number }
    }
  }'
```
