# Sanitizer Security

## Design Principles

- Sanitizers use **allowlists**, not denylists. An allowlist enumerates every
  safe construct explicitly; a denylist attempts to enumerate dangerous ones and
  inevitably misses novel bypasses.
- Operate on a **parsed representation** (DOM, token stream) wherever feasible —
  not raw text with line-by-line or byte-by-byte pattern matching. Line-split
  attacks and multi-byte sequences exploit parsers that do not track tag
  boundaries.
- All element and attribute comparisons must be **case-insensitive**: use
  `eq_ignore_ascii_case`, `to_ascii_lowercase`, or equivalent. Mixed-case
  variants (e.g., `ScRiPt`) must not bypass checks.
- Never cast `u8` to `char` (e.g., `bytes[i] as char`) outside ASCII-only
  contexts. This pattern silently corrupts multi-byte UTF-8 sequences and can
  allow sanitizer bypasses via crafted byte patterns. Use `char::from(b)` only
  when `b.is_ascii()` is proven; otherwise iterate with `str::chars()`.
- URI scheme detection must strip control characters and whitespace before
  comparing the scheme name. Fixed byte-length caps on scheme detection windows
  are not sufficient.

## Coverage Requirements

- Sanitization must be applied **consistently across every code path** that emits
  the same data type. If HTML content is sanitized in the HTML renderer, the same
  sanitizer must cover equivalent content in the PDF and text renderers.
- When adding a new output path (renderer, API binding, FFI function, WASM export)
  that handles user-supplied or external-tool content, audit all existing
  sanitizers to confirm the new path is covered.
- When fixing a sanitizer bypass: (1) add a regression test for the bypass,
  (2) audit sibling sanitizers (e.g., if you fixed the SVG sanitizer, check the
  CSS and URI sanitizers) for the same gap.
