# Code Style Rules

- Follow standard Rust conventions (rustfmt defaults, Clippy lints).
- All public items must have doc comments (`///` or `//!`).
- No unnecessary external dependencies. Justify any new dependency in the PR description.
- `chordsketch-core` must have zero external dependencies — no exceptions.
- Prefer returning `Result` over panicking. Reserve `unwrap()` / `expect()` for cases
  where failure is provably impossible, and add a comment explaining why.
- Use `#[must_use]` on functions whose return values should not be silently discarded.
- Tests go in `#[cfg(test)] mod tests` inside the same file, or in a `tests/` directory
  for integration tests.
- Keep modules small and focused. Prefer many small files over few large ones.

## Test Quality

- Every test must make at least one meaningful assertion that would fail if the
  tested behaviour regressed. Empty tests or tests that only assert `true` are
  not acceptable.
- Tests must cover both the happy path and the error / edge-case paths.
- When adding a test for a bug fix, the test must fail without the fix applied.

## Silent Fallback

- Never use a default or fallback value where a missing value indicates a bug
  or a contract violation. Return `None`, `Err`, or `Option` to force callers
  to make an explicit decision.
- Document any intentional fallback with a comment explaining why the default
  is safe in context.

## Resource Limits

- Never accept unbounded input sizes without a documented limit. If a parser or
  converter can receive arbitrarily large input, either enforce a limit or document
  the memory/time complexity and the maximum safe input size.
- Prefer streaming / incremental processing over loading entire inputs into memory
  when processing files.

## Unicode Safety

- Do not use byte indices to slice strings that may contain multi-byte characters.
  Use `.chars()`, `char_indices()`, or crate helpers that respect char boundaries.
- When byte offsets are unavoidable (e.g., interoperating with ASCII chord lines),
  document why the bytes are known to be ASCII and add a golden test with a
  multi-byte lyric to catch regressions.
