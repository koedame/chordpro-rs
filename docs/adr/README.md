# Architecture Decision Records

This directory contains the architecture decision records (ADRs) for
ChordSketch. An ADR captures a single, significant architectural or
operational decision together with the context that produced it, the
alternatives considered, and the consequences accepted.

ADRs exist to preserve **why** a decision was made, not just **what** was
decided. The codebase, git history, and issue tracker already record the
**what**; an ADR exists when the **why** would otherwise be lost.

## When to write an ADR

Write an ADR when any of the following are true:

- A decision intentionally **declines** to do work that an open issue
  proposes (e.g. an upstream-blocked migration), and that decision should
  outlive the issue's closure.
- A decision rules out an alternative whose case is non-obvious enough that
  a future contributor might re-propose it without the historical context.
- A decision establishes a project-wide convention that the rules in
  `.claude/rules/` do not already cover.

Routine code changes do **not** need an ADR. The bar is "would a reasonable
future contributor reach the wrong conclusion if this rationale were
missing?"

## File naming

ADR files are named `NNNN-kebab-case-title.md`, where `NNNN` is a
zero-padded sequence number assigned in creation order. Numbers are
**never reused**, even if an ADR is later superseded.

## Template

Each ADR follows this structure:

```markdown
# NNNN. Short title in sentence case

- **Status**: Accepted | Superseded by ADR-NNNN | Deprecated
- **Date**: YYYY-MM-DD

## Context

What problem prompted this decision? What constraints, prior art, or
upstream limits define the space of possible answers?

## Decision

The chosen course of action, stated unambiguously.

## Rationale

Why this option, given the context. Cite evidence (URLs, commit hashes,
issue numbers) so the reasoning can be re-verified later.

## Consequences

The trade-offs accepted by the decision — both positive and negative.
Mitigations for the negatives, if any.

## Alternatives considered

Other options that were on the table and why they were rejected.

## References

Issues, PRs, external documentation, and any "watch signals" that should
prompt revisiting the decision.
```

Once the ADR is committed, its **Status** is locked. If the decision later
changes, write a new ADR that supersedes the old one and update the old
ADR's Status line to `Superseded by ADR-NNNN`.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-kotlin-maven-central-publishing-credentials.md) | Kotlin Maven Central publishing credentials | Accepted (2026-04-11) |
