# Sanitizer and Security Rules

## Sanitizer Bypass Chains

Any string that will be embedded in structured output (ChordPro directives,
HTML, PDF annotations, JSON) MUST be sanitized before use. Apply sanitizers
at the **output boundary**, not at the parse boundary.

### Rules

- **ChordPro output**: Strip or escape `{` and `}` from all directive names
  and values. Use `sanitize_directive_token` (see `heuristic.rs`) or an
  equivalent at every call site that produces ChordPro text.
- **HTML output**: Escape `<`, `>`, `&`, `"`, and `'` in any user-supplied
  string. Never concatenate raw strings into HTML.
- **No partial sanitization**: If a sanitizer is applied to a value, it must
  also be applied to the corresponding name field and vice versa. Asymmetric
  sanitization is a common source of bypass vulnerabilities.
- **Test sanitization with adversarial inputs**: Every sanitizer must have
  at least one test containing the exact character being stripped or escaped
  in both the name and value positions.

## Security Asymmetry

When a security property (e.g. sanitization, access control, rate limiting)
is applied to one code path, audit all parallel code paths for the same
property. Asymmetric treatment is a structural vulnerability.

Example: if directive *values* are sanitized, directive *names* must be
sanitized too — they appear in the same output context.

## Why

53 sanitizer bypass issues and 18 security asymmetry issues were filed.
The most common pattern was sanitizing only one field of a multi-field
record, leaving the other fields exploitable.
