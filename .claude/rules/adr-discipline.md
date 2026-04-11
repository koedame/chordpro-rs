# ADR Discipline

Significant decisions in this project — especially ones that intentionally
**decline** to do work, lock in a non-obvious tradeoff, or establish a
project-wide convention — MUST be recorded as Architecture Decision
Records (ADRs) under `docs/adr/`.

ADRs exist to preserve the **why** of a decision, so the rationale
survives after the issue tracker, chat history, or commit message moves
on. The bar for writing one is: *would a reasonable future contributor
(or AI assistant session) reach the wrong conclusion if this rationale
were missing?* If yes, write the ADR.

## When to write an ADR (proactively, without being asked)

Open an ADR PR whenever any of the following occur, **before** taking
the action that locks in the decision:

- **Closing an issue without implementing it.** If the work is being
  declined because it is upstream-blocked, no longer wanted, superseded,
  or rejected on its merits, the rationale belongs in an ADR. The
  issue's closing comment then links to the ADR. (Canonical example:
  ADR 0001, which captured why #1124 was closed without migrating
  Kotlin Maven Central to OIDC.)
- **Choosing one technical approach over another with non-obvious
  tradeoffs.** If a future contributor might reasonably re-propose the
  rejected option, the rejection needs to be durable.
- **Establishing a project-wide convention not yet captured in
  `.claude/rules/`.** Files in `.claude/rules/` are operational rules;
  ADRs are the *decisions* that produced those rules. When a rule is
  itself novel or contested, write the ADR first and link it from the
  rule.
- **Declining a known bug fix or improvement** because the cost
  outweighs the benefit, or because the fix lives upstream and is
  blocked there.
- **Locking in a security trade-off** (e.g. accepting a long-lived
  credential because the alternative is unavailable, or allowing a
  particular permission scope).
- **Reversing or superseding a previous ADR.** Write a new ADR
  explaining the reversal; do not silently edit the old one.

## When an ADR is NOT needed

- Routine code changes that any maintainer would make the same way.
- Bug fixes whose root cause is fully captured in the commit message.
- Decisions already covered by an existing rule in `.claude/rules/` or
  by an existing accepted ADR.
- Minor stylistic, naming, or formatting choices.
- Reversible experiments that can be rolled back without consequence.

## Process

1. **Recognize the trigger.** When you encounter one of the situations
   above, pause and propose writing an ADR before executing the related
   action. Do not wait for the user to ask.
2. **Find the next sequence number** by listing existing ADRs in
   `docs/adr/` and incrementing the highest. Sequence numbers are
   never reused, even when an ADR is later superseded.
3. **Write the ADR** following the template in `docs/adr/README.md`
   (Status, Date, Context, Decision, Rationale, Consequences,
   Alternatives considered, References).
4. **Open a dedicated PR** for the ADR (per the shared-files policy in
   `parallel-work.md` and `doc-maintenance.md`). Update the ADR index
   table in `docs/adr/README.md` in the same PR.
5. **Link the ADR** from any issue, PR, or rule the decision touches.
   When closing an issue that the ADR governs, the closing comment must
   include the ADR link.
6. **Lock the Status** once the ADR PR merges. To change the decision
   later, write a new ADR that supersedes the old one and update the
   old ADR's Status line to `Superseded by ADR-NNNN`. Do not rewrite
   accepted ADRs in place.

## Proactive recognition (for AI assistants)

When the user instructs you to **close an issue without implementing
it**, **decline a proposal**, **choose between approaches with
significant tradeoffs**, or **establish a new project-wide convention**,
first check whether an ADR is warranted using the triggers above. If
yes, propose writing the ADR (and the next sequence number) BEFORE
executing the close/decline/convention action. The user should not have
to ask "should we record this as an ADR?" — the rule fires
automatically.

If you are uncertain whether an ADR is warranted, err on the side of
proposing one. A short ADR is cheap to write; a lost rationale is
expensive to reconstruct.

## Why

Issues, comments, and chat history decay. Rationale that lives only in
those places becomes invisible to future contributors and to future AI
assistant sessions, who can then re-propose work that was already
considered and declined — burning time on the same upstream check, the
same alternatives analysis, and the same conclusion.

ADR 0001 (Kotlin Maven Central credentials) is the canonical case: the
OIDC migration cannot work today, the reasoning involved checking four
upstream sources, and the issue would have been closed with that
rationale buried in a comment if the maintainer had not asked for an
ADR. This rule exists so that ask is no longer necessary.
