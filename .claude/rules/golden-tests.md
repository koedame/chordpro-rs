# Golden Test Rule

- Every parser behavior must have golden test coverage.
- A golden test consists of an input `.cho` file and an expected output snapshot file.
- Golden test fixtures live in `crates/core/tests/fixtures/`.
- When parser behavior changes, update the corresponding golden snapshot and explain the
  change in the PR description.
- New ChordPro directives or syntax support must include at least one golden test before
  merging.

## Stop-Word Collision Tests

When adding or modifying the chord parser or heuristic importer, add at least one
golden test (or unit test) for each of the following stop-word collision patterns:

- A word that starts with a valid root note letter but is NOT a chord (e.g., `Am` in
  `Amazing`, `Em` in `Empty`, `G` in `Get`).
- A section label whose first letter is a valid chord root (e.g., `Chorus`, `Bridge`,
  `Dm` as a label).

These collisions are the most common source of chord-detection false positives and
regressions. A test that explicitly asserts rejection prevents future parsers from
silently accepting them.
