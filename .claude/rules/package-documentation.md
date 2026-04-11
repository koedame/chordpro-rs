# Package Documentation Standard

## Logo

The canonical logo lives at `assets/logo.svg` (180×180 px, red `#BD1642` background,
white mark). Use these assets:

| File | Size | Use |
|------|------|-----|
| `assets/logo.svg` | 180×180 | All READMEs, web contexts |
| `assets/logo-128.png` | 128×128 | VS Code Marketplace icon, app icons |
| `assets/logo-256.png` | 256×256 | High-DPI contexts, social previews |

**In every published package README**, embed the logo at the very top using the
absolute raw GitHub URL so it renders on all registries (npm, PyPI, RubyGems, etc.):

```markdown
<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>
```

**VS Code extension**: set `"icon": "icon.png"` in `package.json`; the file at
`packages/vscode-extension/icon.png` is generated from `assets/logo-128.png` and
checked in. Regenerate it when the logo changes:

```bash
convert -background none assets/logo.svg -resize 128x128 packages/vscode-extension/icon.png
```

Defines the minimum quality bar and discoverability requirements for every
publicly published ChordSketch package. Apply this rule when adding a new
package or updating an existing one, and run `/doc-quality-check` to verify
compliance before opening a release PR.

## Quality Levels

| Level | Name | Requirements |
|-------|------|-------------|
| **L1** | Minimal | README exists. Description, license, and repository link are set in the package manifest. |
| **L2** | Standard | L1 + installation command, complete runnable quick-start example, API summary table, links to project/playground/issues. |
| **L3** | Polished | L2 + full typed API reference, options documentation, error/exception type documentation, platform compatibility table, note on prebuilt binaries where applicable. |

**Every published package must reach L2 before release.**
Primary consumer-facing packages (`@chordsketch/wasm`, `@chordsketch/node`, CLI
crate) must reach **L3**.

## Version Placeholder Rule

Installation examples in READMEs MUST NOT hardcode a specific release version.
Hardcoded versions silently become stale and direct users to outdated releases.

**Required pattern:** pair a self-updating badge with a `VERSION` placeholder in
every code snippet that includes a version number.

| Registry | Badge | Placeholder |
|----------|-------|-------------|
| Maven Central | `[![Maven Central](https://img.shields.io/maven-central/v/me.koeda/chordsketch)](https://central.sonatype.com/artifact/me.koeda/chordsketch)` | `VERSION` in all three Gradle/Maven snippets |
| Swift Package Index | `[![GitHub Release](https://img.shields.io/github/v/release/koedame/chordsketch)](https://github.com/koedame/chordsketch/releases/latest)` | Keep the numeric version in `.from:` but add inline comment `// replace with the latest release tag` and a version-agnostic note above the snippet (do not quote a specific version number in the prose) |
| crates.io | `[![crates.io](https://img.shields.io/crates/v/CRATE-NAME)](https://crates.io/crates/CRATE-NAME)` | `VERSION` |
| npm (if version is shown) | `[![npm](https://img.shields.io/npm/v/@chordsketch/wasm)](https://www.npmjs.com/package/@chordsketch/wasm)` | `VERSION` |
| PyPI (if version is shown) | `[![PyPI](https://img.shields.io/pypi/v/chordsketch)](https://pypi.org/project/chordsketch/)` | `VERSION` |

For registries where the install command does not include a version (e.g.
`gem install chordsketch`, `pip install chordsketch`) no badge or placeholder
is needed in the code block — the registry always serves the latest by default.

When a badge is present, place it at the top of the `## Installation` section,
immediately before the note about replacing `VERSION`.

## Required README Sections (L2 template, in order)

1. **Package name heading** — exact name as installed from the registry
2. **What it is** — one paragraph: what ChordPro is, what this binding does,
   and which registry it lives in
3. **Installation** — exact registry install command as the **first** fenced
   code block in the file (no prose before it except the above description);
   if the command includes a version number, apply the Version Placeholder Rule
4. **Quick start** — complete runnable example; do not omit imports or
   `require` statements; use a non-trivial ChordPro snippet (title + chord line)
5. **API** — table of all public functions/methods with signature and return type
6. **Options / Configuration** — document the `RenderOptions` equivalent where
   applicable (transpose, config preset, RRJSON)
7. **Links** — links to: main repo, playground (`https://chordsketch.koeda.me`),
   `docs.rs` or equivalent API docs, issue tracker
8. **License** — one-line MIT statement

## Per-Registry Discoverability Requirements

### npm (`@chordsketch/wasm`, `@chordsketch/node`)

**Metadata (`package.json`):**
- `description`: one sentence, must include "ChordPro"
- `keywords`: must include `chordpro`, `music`, `chord`, `parser`, `lyrics`
- Runtime-specific keywords:
  - wasm package: add `wasm`, `webassembly`
  - node package: add `napi-rs`, `native`, `n-api`, `node-addon`
- `repository.url`, `homepage`: set and pointing to the GitHub repo
- `license`: `"MIT"`

**README:**
- `@chordsketch/node` must mention **"prebuilt binaries — no Rust toolchain
  required"** near the top; include the platform support table (5 platforms)
- Distinguish `@chordsketch/node` vs `@chordsketch/wasm`: native addon for
  Node.js (better performance), WASM for browsers
- PDF rendering: `@chordsketch/node` returns `Buffer`; `@chordsketch/wasm`
  returns `Uint8Array` — document this difference explicitly

### PyPI (`chordsketch`)

**Metadata (`pyproject.toml`):**
- `readme` must be a **file reference** (`readme = "README.md"`), not inline
  text — PyPI renders the file as the package page body
- `keywords`: must include `chordpro`, `music`, `chord`, `parser`, `lyrics`,
  `rust`, `uniffi`
- Required Trove classifiers:
  - `Topic :: Multimedia :: Sound/Audio`
  - `Topic :: Text Processing :: Markup`
  - `Programming Language :: Rust`
  - `Programming Language :: Python :: 3`
  - `Development Status :: 4 - Beta` (update to `5 - Production/Stable` at 1.0)

**README:**
- `pip install chordsketch` as first code block
- Document `ChordSketchError` exception with both variants
- Note that this is a native extension (no pure-Python fallback)

### RubyGems (`chordsketch`)

**Metadata (`chordsketch.gemspec`):**
- `summary`: one-line description (already used in search results)
- `description`: multi-line, indexed by RubyGems search
- `homepage`: GitHub repo URL
- `metadata["changelog_uri"]`, `metadata["source_code_uri"]`,
  `metadata["bug_tracker_uri"]`: all set (already present)

**README:**
- `gem install chordsketch` + Gemfile snippet as the first code block
- Module namespace: `Chordsketch` (capital C, rest lowercase) — document this
  exactly; it's non-obvious
- Method signatures: `parse_and_render_text(input, config_json, transpose)`
  — document argument order explicitly (config before transpose)
- Include the platform support table; state "no Rust required"

### Swift Package Index (`ChordSketch`)

**Metadata (`Package.swift`):**
- `platforms:` must declare `.macOS(.v12)` and `.iOS(.v15)` — already present
- Product name `ChordSketch` must match the import statement in examples

**README (drives SPI full-text search and is the package landing page):**
- SPM `dependencies` block with exact `.package(url:, from:)` syntax
- Import statement: `import ChordSketch`
- Platforms section: explicitly list "macOS 12+, iOS 15+"
- State "Prebuilt XCFramework — no Rust build step required"
- Note that the Swift API uses camelCase (UniFFI convention):
  `parseAndRenderText`, `parseAndRenderHtml`, `parseAndRenderPdf`
- `ChordSketchError` enum: document `.noSongsFound` and `.invalidConfig(reason:)`

### Maven Central (`me.koeda:chordsketch`)

**Metadata (`build.gradle.kts` POM block):**
- `description`: set and meaningful
- `inceptionYear`: must match the actual year the project started
- `url`, `licenses`, `developers`, `scm`: all required by Maven Central — already present
- `coordinates("me.koeda", "chordsketch", ...)` — note the groupId is `me.koeda`
  (reverse-DNS of `koeda.me` domain), NOT `io.github.koedame`

**README:**
- Show both Gradle and Maven XML dependency blocks
- Note JVM requirement: Java 17+ (due to toolchain setting)
- Note JNA is a transitive dependency (automatically pulled)
- Kotlin API uses camelCase (UniFFI): `parseAndRenderText`, etc.
- `ChordSketchException` is the Kotlin exception class

### crates.io (all `chordsketch-*` crates)

**Metadata (every published crate's `Cargo.toml`):**
- `keywords` (max 5): inherit from workspace or set explicitly; must include
  `chordpro` for all crates
- `categories`: at minimum `parser-implementations` for the core crate;
  renderer crates add `text-processing`; CLI crate adds `command-line-utilities`
- `readme = "README.md"`: crates.io displays this file as the crate page body
- `description`: one sentence, non-empty

**README (crate-level, not workspace root):**
- Exists in the crate directory (not just the workspace root)
- Contains a Rust usage example specific to this crate's API

### VS Code Marketplace (`chordsketch`)

**Metadata (`package.json`):**
- `description`: set and meaningful
- `keywords`: must include `chordpro`, `chordpro language`, `music`, `lyrics`
- `categories`: `["Programming Languages", "Formatters"]` at minimum
- Workspace package dependencies (`@chordsketch/wasm`) must track the current
  released version — must not lag more than one minor version behind

## Version Consistency Rule

When a package depends on another workspace package, the pinned version range
must include the current released version. Specifically:
- `packages/vscode-extension/package.json`: `@chordsketch/wasm` pin must
  satisfy the current npm release
- Bump these pins in the same PR as the release that updates the depended-upon
  package version

## Running the Quality Check

Before opening any release PR, run:

```
/doc-quality-check
```

The command will output a compliance matrix and list all gaps that must be
fixed before the release tag is created. Gaps at L1 or missing README files
are **blocking** (must fix before release). L3 gaps on non-primary packages
are non-blocking but should be tracked as issues.
