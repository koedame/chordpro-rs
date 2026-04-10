# Defensive Input Handling

## Input Validation

- Every `pub fn` validates its arguments at the boundary — never rely on callers
  to pass valid data.
- Structs with invariants (e.g., numeric fields that must stay in range) use
  **private fields** and a `Result`-returning constructor that enforces them.
- Numeric inputs: reject `NaN`, `Infinity`, negative values where invalid, and
  values outside the documented range. Check both the lower and upper bound.
- String inputs: check length before processing; guard against empty strings when
  the function requires non-empty input; reject or strip control characters where
  the caller expects printable text.
- Named limit constants (`MAX_*`, `LIMIT_*`) with a doc comment explaining the
  bound must be defined for every hard upper bound derived from an invariant.

## File and Path Safety

- **Open-then-fstat**, never stat-then-open. Obtain an `fs::File` first, then
  call `file.metadata()` on the open handle. The reverse order creates a TOCTOU
  window between the check and the use.
- On Unix, use `O_NOFOLLOW` (or `OpenOptions::custom_flags`) for paths that
  must not be symlinks.
- Never construct a temp-file path from a PID or timestamp. Use the
  [`tempfile`](https://docs.rs/tempfile) crate; its RAII types (`NamedTempFile`,
  `TempDir`) delete the file or directory automatically — including on panic.
- In tests, create temporary directories via `tempfile::tempdir()` so they are
  cleaned up even when an assertion fails. Manual `remove_dir_all` calls are
  skipped on test failure.

## Resource Bounds

- Every loop or recursion that processes untrusted input must have a bounded
  iteration count. Define a named `MAX_*` constant for the bound.
- Allocation sizes derived from untrusted input must be validated against the
  relevant `MAX_*` constant before allocating. Prefer `Vec::try_reserve` or an
  explicit size check over unbounded `Vec::with_capacity(untrusted_len)`.
