# Renderer Parity

ChordSketch has three renderers — text (`crates/render-text`), HTML
(`crates/render-html`), and PDF (`crates/render-pdf`) — plus bindings (WASM, FFI,
napi). All three renderers consume the same AST and must produce semantically
equivalent output for the same input unless a divergence is explicitly documented.

## Rules

- **Bug fixes**: when fixing a bug in one renderer, check the other two for the
  same bug before marking the issue closed. File separate issues for confirmed
  bugs in sibling renderers.
- **New directives / AST nodes**: implement support in all three renderers in the
  same PR. Do not merge partial support where one renderer silently ignores the
  new node.
- **Shared constants** (e.g., `MAX_CHORUS_RECALLS`, `MAX_COLUMNS`, limit values):
  define them once in `chordsketch-core` and import them. Do not copy the value
  across crates.
- **Intentional divergence** (e.g., PDF has page limits, text renderer has no
  image support): document the divergence with an inline comment citing the
  rationale. Undocumented divergence is treated as a bug.
- **Regression tests**: when adding a regression test for one renderer, add an
  equivalent test for the other two — or include a comment explaining why the
  other renderers are not affected.
- **Bindings**: parameter validation ranges (e.g., transpose semitone limits)
  must be identical across WASM, FFI, and napi. A mismatch is a Medium-severity
  finding.
