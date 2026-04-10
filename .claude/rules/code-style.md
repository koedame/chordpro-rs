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

## Error Handling

- Do not silently swallow errors with `unwrap_or_default()`, `unwrap_or("")`,
  or `.ok()` when the error indicates a bug or invalid input. Errors that callers
  need to know about must propagate. If a silent fallback is intentional, add a
  comment explaining why it is safe and what conditions trigger it.

## Resource Limits

- Every loop or recursion that processes untrusted input must have a bounded
  iteration count. Define a named `MAX_*` constant for the bound — do not use
  a bare integer literal.
- Allocation sizes derived from untrusted input must be validated against a
  `MAX_*` constant before allocating.

## Unicode Safety

- Never index a `&str` by raw byte offset unless the offset is proven to be a
  character boundary (e.g., obtained from `char_indices()`). Prefer `chars()`,
  `char_indices()`, or the `unicode-width` crate for text measurement.
- Never cast `u8` to `char` via `byte as char` outside ASCII-only contexts.
  Use `char::from(b)` only when `b.is_ascii()` is guaranteed; otherwise
  decode with `str::chars()`.

## Test Quality

- Every `#[test]` function must contain at least one meaningful assertion
  (`assert!`, `assert_eq!`, `assert_ne!`, or a call that panics on failure).
  Tests that call a function without asserting on its return value prove nothing.
- `let _ = result;` in a test is prohibited — it discards the value and makes
  the test a no-op with respect to correctness.
- Assert on the **actual output** of the function under test. Asserting a
  hardcoded string that is never computed by the function under test is meaningless.
- When adding a test to cover a bug fix, verify the test fails when the fix is
  reverted before committing.
