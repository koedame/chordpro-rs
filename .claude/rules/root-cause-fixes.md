# Root Cause Fixes

When encountering a problem — whether a bug, test failure, build error, or unexpected
behavior — always investigate and fix the **root cause**, not the symptom.

## Rules

- **No workarounds without justification.** Do not suppress warnings, skip tests, add
  special-case patches, or apply band-aid fixes. If a workaround is truly necessary
  (e.g., upstream bug outside our control), document the reason and link to a tracking
  issue for the proper fix.
- **Diagnose before fixing.** Read the error, trace the cause, and understand *why* the
  problem occurs before writing any fix. A fix you don't understand is not a fix.
- **Fix at the right layer.** If the bug is in the parser, fix the parser — don't add
  compensating logic in the renderer. If the issue is in the data model, fix the data
  model — don't patch every call site.
- **Avoid `#[allow(...)]` or `// nolint` to silence legitimate warnings.** These hide
  real problems. Fix the code that triggers the warning instead.
- **Tests must validate the root cause.** When adding a regression test, ensure it
  covers the actual root cause, not just the surface-level symptom. The test should fail
  if the root cause is reintroduced.
