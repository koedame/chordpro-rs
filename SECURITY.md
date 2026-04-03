# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability in chordpro-rs, please report it
responsibly by [opening a GitHub security advisory][advisory]. Do **not** file
a public issue for security-sensitive reports.

[advisory]: https://github.com/koedame/chordpro-rs/security/advisories/new

We aim to acknowledge reports within 72 hours and provide a fix or mitigation
plan within 30 days.

## Threat Model

chordpro-rs processes **untrusted** `.cho` (ChordPro) files and renders them to
text, HTML, and PDF. The primary threat is a malicious or crafted input file
attempting to:

- Execute arbitrary code on the host.
- Read or write files outside the intended scope.
- Produce excessively large output to exhaust memory or disk.
- Inject scripts or markup into rendered HTML/SVG output.

### Trust Boundaries

| Source | Trust Level | Notes |
|--------|-------------|-------|
| CLI flags and environment variables | **Trusted** | Controlled by the invoking user |
| System and user config files | **Trusted** | Located under `/etc/` or `~/.config/` |
| Project-level config files | **Untrusted** | May come from a cloned repository |
| Song `.cho` files | **Untrusted** | Primary attack surface |
| Delegate tool output (abc2svg, Lilypond) | **Untrusted** | External process output is sanitized |

## Security Controls

### Configuration File Loading

- **Hierarchy**: defaults → system → user → project → song-level overrides.
- **Symlink rejection**: Config files are opened with `O_NOFOLLOW` on Unix to
  prevent symlink-based redirection.
- **File size limit**: Config files are capped at 10 MB (`MAX_CONFIG_FILE_SIZE`).
- **Delegate execution restriction**: Only system-level, user-level config, or
  explicit CLI flags can enable delegate execution. Project-level and song-level
  configs cannot enable delegates — any attempt is silently overridden with a
  warning.

### RRJSON Parser

- **Nesting depth limit**: 64 levels (`MAX_NESTING_DEPTH`).
- **Entry count limit**: 10,000 entries per object or array (`MAX_ENTRIES`).
- **Non-finite number rejection**: `Infinity`, `-Infinity`, and `NaN` are
  rejected.

### Delegate Execution (abc2svg, Lilypond)

Delegate environments invoke external tools to convert music notation to SVG.
Multiple defense layers are applied:

1. **Content-level sanitization** — dangerous constructs are stripped from input
   before the external tool is invoked:
   - ABC: `%%beginjs`/`%%endjs` blocks and `%%javascript` directives are
     removed (case-insensitive).
   - Lilypond: lines containing dangerous Scheme functions (`system`, `getenv`,
     `open-input-file`, `open-output-file`, `open-file`, `primitive-load`,
     `primitive-load-path`, `eval-string`, `ly:gulp-file`, `ly:system`) are
     stripped.
2. **Process sandboxing** — Lilypond is invoked with the `-dsafe` flag to
   sandbox its embedded Scheme interpreter.
3. **Safe command construction** — all arguments are passed via the
   `Command::arg()` API, preventing shell metacharacter injection.
4. **Output sanitization** — SVG output from delegates is run through the SVG
   sanitizer before inclusion in rendered output.
5. **Temp file safety** — temporary files use PID + atomic counter for unique
   names, are created with `O_EXCL` (exclusive create) semantics, and
   directories use `create_dir` (not `create_dir_all`) to prevent symlink
   attacks on parent paths.

### SVG Sanitization

All SVG content (from delegates or `{start_of_svg}` sections) is sanitized:

- **Dangerous elements stripped**: `<script>`, `<foreignobject>`, `<iframe>`,
  `<object>`, `<embed>`, `<math>`, `<set>`, `<animate>`, `<animatetransform>`,
  `<animatemotion>`.
- **Event handlers removed**: all `on*` attributes (case-insensitive).
- **URI scheme validation**: `href`, `src`, and `xlink:href` attributes are
  checked; `javascript:`, `vbscript:`, and `data:` schemes are blocked.
  Whitespace obfuscation (tabs, newlines) is stripped before scheme detection.

### HTML Output

- All user-provided text content is escaped via `escape_xml()` before
  insertion into HTML.
- CSS values are sanitized through a whitelist filter
  (`sanitize_css_value()`).
- CSS class names are sanitized via `sanitize_css_class()`.

**Recommendation for consumers**: when serving HTML output in a browser, apply
a restrictive `Content-Security-Policy` header. A reasonable starting point:

```
Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; img-src 'self'
```

### Image Handling (PDF Renderer)

- **Path validation**: rejects absolute paths, Windows drive letters, UNC
  paths, null bytes, and `..` directory traversal — on all platforms.
- **Symlink rejection**: images are opened with `O_NOFOLLOW` on Unix; file
  metadata is checked on the open file descriptor to close TOCTOU gaps.
- **Size limits**:
  - `MAX_IMAGE_FILE_SIZE`: 50 MB per image file.
  - `MAX_DECOMPRESSED_SIZE`: 256 MB for PNG IDAT decompression.
  - `MAX_IMAGE_PIXELS`: 10,000 px per dimension (render clamp — images larger
    than this are scaled down for rendering, not rejected).
  - `MAX_IMAGES`: 1,000 images per document.
  - `MAX_PAGES`: 10,000 pages per document.

### Output Amplification Prevention

- **Chorus recall limit**: `{chorus}` directives are capped at 1,000 recalls
  per song across all renderers (text, HTML, PDF) to prevent output
  amplification from malicious input.

## Dependency Policy

- `chordpro-core` has **zero external dependencies**. All parsing, validation,
  and sanitization logic is implemented from scratch.
- Renderer crates use minimal, well-audited dependencies (`unicode-width`,
  `flate2`).
- The CLI uses `clap` for argument parsing.
