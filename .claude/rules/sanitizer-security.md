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

## Blocklist and Allowlist Completeness

When maintaining a URI scheme denylist, tag blocklist, or attribute allowlist,
the list MUST be verified against the relevant specification for completeness.
A partial list gives a false sense of security.

### URI scheme denylists

Every URI scheme denylist MUST block at minimum:
`javascript:`, `vbscript:`, `data:`, `file:`, `blob:`

Whenever an entry is added to any URI scheme list:
1. Cross-check `has_dangerous_uri_scheme` and `is_safe_image_src` (and any future
   sibling functions) to ensure they are consistent.
2. Verify the list against the [OWASP XSS Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
   for completeness. (The WHATWG Fetch spec does not define a URI scheme denylist
   for HTML sanitization purposes.)

### SVG tag blocklists

Blocklisted SVG tags MUST include all tags that can load external resources:
`script`, `foreignObject`, `use` (with external `href`), `feImage`, `image`, `iframe`, `embed`, `object`

Whenever a tag is added to `DANGEROUS_TAGS`, check whether there is an equivalent
filter primitive or presentation element that can load the same resource class.

### Attribute allowlists / URI attribute lists

Direct URI-bearing SVG/HTML attributes MUST include at minimum:
`href`, `xlink:href`, `src`, `action`, `formaction`, `poster`, `background`, `ping`

SVG animation value attributes (`to`, `values`, `from`, `by`) are NOT URI-bearing
in the conventional sense — they carry animation values. However, they MUST still be
sanitized when the animated `attributeName` is a URI attribute (e.g.
`<animate attributeName="href" to="javascript:alert(1)"/>`). The implementation uses
a defense-in-depth approach: animation elements (`animate`, `set`, etc.) are stripped
entirely via `DANGEROUS_TAGS`, and `to`/`values`/`from`/`by` are additionally included
in `is_uri_attr` as a secondary defense on any surviving element. Both layers are
intentional and must be preserved.

### Testing completeness

When a new entry is added to any blocklist or allowlist:
- Add a test that exercises the new entry with a malicious value.
- Add a comment citing the relevant spec section or CVE that motivated the entry.

## Why

53 sanitizer bypass issues and 18 security asymmetry issues were filed.
The most common pattern was sanitizing only one field of a multi-field
record, leaving the other fields exploitable.

A 2026-04-12 audit found two blocklist completeness gaps: `file:` URI not blocked
in `has_dangerous_uri_scheme` despite being blocked in `is_safe_image_src` (#1538),
and `feImage` absent from `DANGEROUS_TAGS` despite being a known resource-loading
SVG primitive (#1545). Both were caused by partial lists copied from one context
without auditing the full specification.
