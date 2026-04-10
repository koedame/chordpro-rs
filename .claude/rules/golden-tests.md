# Golden Test Rule

- Every parser behavior must have golden test coverage.
- A golden test consists of an input `.cho` file and an expected output snapshot file.
- Golden test fixtures live in `crates/core/tests/fixtures/`.
- When parser behavior changes, update the corresponding golden snapshot and explain the
  change in the PR description.
- New ChordPro directives or syntax support must include at least one golden test before
  merging.
- When a golden test covers a keyword-terminated loop (e.g., a directive that scans
  tokens until a closing keyword), at least one fixture must contain **another keyword
  inside the scanned region**. This catches stop-word collisions where the parser exits
  early on an unrelated keyword. For example, a fixture for
  `{start_of_chorus}...{end_of_chorus}` should include a `{comment: ...}` line inside
  the chorus body.
