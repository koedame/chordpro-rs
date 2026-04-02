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

1. **Initial review** — run `/review` and `/security-review` on the full phase
   diff (may run in parallel).
2. **Classify findings** by severity (see
   [Severity Definitions](pr-workflow.md#severity-definitions)).
3. **Blocking findings** (High, Medium) — create sub-issues, implement fixes
   via normal PR workflow, and merge to `main`.
4. **Non-blocking findings** (Low, Nit) — create issues for future work. These
   do **not** block phase closure.
5. **Delta review** — run `/review` and `/security-review` on only the fix
   commits merged since the initial review. Do **not** re-review the entire
   phase. Only new blocking findings in the fix code require further fixes.
6. **Repeat steps 3–5** until a delta review produces no new blocking findings.
7. **Close** the phase tracking issue when all blocking findings are resolved
   and all non-blocking findings are tracked as issues.

### Review Finding Accountability

All review findings must be:

1. **Individually documented** in the review comment on the tracking issue or PR,
   with a clear description and severity classification.
2. **Resolved or tracked** before the phase or PR is closed:
   - **Blocking** (High, Medium) — fixed in the current cycle.
   - **Non-blocking** (Low, Nit) — GitHub Issue created for future work.
3. **Never silently dropped.** Aggregate counts like "14 Low findings" without
   enumeration are not acceptable. Every finding must be enumerable and
   traceable.

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
