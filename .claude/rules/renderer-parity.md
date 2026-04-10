# Renderer Parity

## Rule

Every ChordPro directive or AST node that is rendered by one renderer MUST
be handled by **all** renderers (text, HTML, PDF). A missing case in one
renderer is a correctness bug.

## Required practices

- When adding a new directive or AST node, add rendering support to all
  three renderers in the same PR (or in explicitly tracked follow-up issues
  that are linked and prioritised).
- When fixing a rendering bug in one renderer, check all other renderers for
  the same bug and fix them in the same PR.
- Golden tests for each renderer must cover the same set of directives. If
  a fixture exercises a directive in the text renderer, an equivalent fixture
  must exist (or be explicitly tracked) for HTML and PDF.

## Audit pattern

Before closing a PR that touches any renderer:

1. Search for all `match` arms on `Line`, `Directive`, or equivalent AST
   enums in the changed renderer.
2. Verify every arm exists in the other renderers.
3. If an arm is missing, either add it or file a sub-issue.

## Why

45 renderer parity issues were filed — the most common was a new directive
handled in the text renderer but silently ignored or panicking in HTML/PDF.
