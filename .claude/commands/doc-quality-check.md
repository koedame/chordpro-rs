# Package Documentation Quality Check

Audit all publicly published ChordSketch packages against the standard defined
in `.claude/rules/package-documentation.md`. The argument, if provided, is a
specific package name to check; if omitted, check all packages: `$ARGUMENTS`

## Packages to Check

| Package | Registry | README path | Manifest |
|---------|----------|-------------|----------|
| `@chordsketch/wasm` | npm | `packages/npm/README.md` | `packages/npm/package.json` |
| `@chordsketch/node` | npm | `crates/napi/README.md` | `crates/napi/package.json` |
| `chordsketch` | PyPI | `crates/ffi/README.md` | `crates/ffi/pyproject.toml` |
| `chordsketch` | RubyGems | `packages/ruby/README.md` | `packages/ruby/chordsketch.gemspec` |
| `ChordSketch` | Swift Package Index | `packages/swift/README.md` | `packages/swift/Package.swift` |
| `me.koeda:chordsketch` | Maven Central | `packages/kotlin/README.md` | `packages/kotlin/lib/build.gradle.kts` |
| `chordsketch` (CLI) | crates.io | `crates/cli/README.md` | `crates/cli/Cargo.toml` |
| `chordsketch-core` | crates.io | `crates/core/README.md` | `crates/core/Cargo.toml` |
| `chordsketch-render-text` | crates.io | `crates/render-text/README.md` | `crates/render-text/Cargo.toml` |
| `chordsketch-render-html` | crates.io | `crates/render-html/README.md` | `crates/render-html/Cargo.toml` |
| `chordsketch-render-pdf` | crates.io | `crates/render-pdf/README.md` | `crates/render-pdf/Cargo.toml` |
| `chordsketch-lsp` | crates.io | (not yet published) | `crates/lsp/Cargo.toml` |
| `chordsketch` | VS Code Marketplace | (README in `packages/vscode-extension/`) | `packages/vscode-extension/package.json` |

## Steps

1. **For each package** (or only the specified one if `$ARGUMENTS` is set):

   a. **README check**: Does the file exist? Read it and verify it contains
      all 8 required sections from the standard:
      - Package name heading
      - "What it is" paragraph
      - Installation command (first code block)
      - Quick start example
      - API table
      - Options / Configuration
      - Links section (repo, playground, docs, issues)
      - License line

   b. **Metadata check**: Read the manifest file and verify:
      - `description` is non-empty and contains "ChordPro"
      - `keywords` / `classifiers` include required terms (per-registry list
        in the rule)
      - `readme` field points to a file (not inline text) where applicable
      - `repository` / `homepage` / `url` set and correct
      - `license` set

   c. **Version consistency check**: For any package that depends on another
      workspace package, verify the pinned version satisfies the current release.

   d. **Assign level**: Based on findings, assign L1 / L2 / L3:
      - L1: README exists, basic manifest fields set
      - L2: all 8 README sections present, metadata complete
      - L3: L2 + typed API reference, platform table, error docs, prebuilt note

2. **Output a compliance matrix** in this format:

   ```
   Package                   | Registry        | Level | Gaps
   --------------------------|-----------------|-------|-----
   @chordsketch/wasm         | npm             | L3    | —
   @chordsketch/node         | npm             | L2    | missing typed API signatures
   chordsketch               | PyPI            | L2    | —
   ...
   ```

3. **List all gaps** grouped by severity:

   **Blocking (must fix before release):**
   - `<package>`: <gap description> — fix: <specific file and field>

   **Non-blocking (should track as issue):**
   - `<package>`: <gap description>

4. **Version consistency findings**: List any package dependency version mismatches.

5. **Summary verdict**:
   - If all published packages are L2+: "All packages meet the minimum standard."
   - If any published package is below L2: "BLOCKED: fix the following before release: ..."

## Notes

- The VS Code extension `README.md` lives at `packages/vscode-extension/README.md`
  if it exists; the Marketplace also shows the repo README for extensions.
- `chordsketch-lsp` is not yet published to crates.io; skip the README requirement
  but still check the manifest metadata.
- Do not flag L3 gaps on packages whose target level is L2 (Ruby, Python, Swift,
  Kotlin) — only flag if they fall below L2.
- The playground package (`packages/playground`) is private (`"private": true`);
  skip it entirely.
