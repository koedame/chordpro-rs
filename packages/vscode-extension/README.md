<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo-256.png" alt="ChordSketch" width="80" height="80">
</p>

<!--
  PNG (not SVG) because the VS Code Marketplace's vsce packaging step
  rejects SVG images in README.md with "SVGs are restricted in
  README.md". `assets/logo-256.png` is the canonical high-DPI raster
  per .claude/rules/package-documentation.md §Logo.
-->

# ChordSketch for VS Code

[![GitHub Release](https://img.shields.io/github/v/release/koedame/chordsketch)](https://github.com/koedame/chordsketch/releases/latest)

ChordPro language support for [Visual Studio Code](https://code.visualstudio.com):
syntax highlighting, diagnostics, completions, a live HTML/text preview
panel, and one-keystroke transpose/export commands. The
[ChordPro](https://www.chordpro.org/) format is a plain-text notation that
interleaves chords with lyrics; this extension makes editing and
previewing `.cho` / `.chordpro` / `.chopro` files pleasant inside VS Code.

The extension is built on the [ChordSketch](https://github.com/koedame/chordsketch)
toolchain — a Rust reimplementation of the ChordPro reference
implementation — and bundles a prebuilt
[Language Server (`chordsketch-lsp`)](https://github.com/koedame/chordsketch/tree/main/crates/lsp)
binary on every supported platform. **No Rust toolchain, Node.js runtime,
or extra install step is required** on those platforms.

## Installation

Install from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=koedame.chordsketch)
or [Open VSX Registry](https://open-vsx.org/extension/koedame/chordsketch):

```bash
code --install-extension koedame.chordsketch
```

Or from the Extensions sidebar: search for `ChordSketch` and click
Install.

### Platform support (prebuilt LSP bundled)

The Marketplace and Open VSX serve one universal VSIX plus seven
platform-specific VSIXes so the LSP binary matches your machine
automatically.

| Platform | Architecture | Bundled LSP |
|---|---|---|
| Linux (glibc) | x86_64 | ✅ |
| Linux (glibc) | arm64 | ✅ |
| macOS | x86_64 (Intel) | ✅ |
| macOS | arm64 (Apple Silicon) | ✅ |
| Windows | x86_64 | ✅ |
| Linux (musl / Alpine) | x86_64 | ✅ |
| Linux (musl / Alpine) | arm64 | ✅ |

On any platform not in the table, VS Code installs the universal VSIX
and the LSP resolver falls back to (in order) the path set by the
`chordsketch.lsp.path` setting, then `chordsketch-lsp` on `PATH`. In
that case install the CLI separately:

```bash
cargo install chordsketch-lsp
```

## Quick start

1. Install the extension (above).
2. Open or create a file with a ChordPro extension (`.cho`,
   `.chordpro`, or `.chopro`). A minimal example:

   ```chordpro
   {title: Amazing Grace}
   {subtitle: Traditional}

   [G]Amazing [G7]grace, how [C]sweet the [G]sound
   That saved a [G]wretch like [D]me
   ```

3. Click the preview icon in the editor title bar (or run
   **ChordSketch: Open Preview to the Side** from the Command Palette
   with `Ctrl+Shift+P` / `Cmd+Shift+P`) to see the rendered song next
   to the source.
4. Run **ChordSketch: Transpose Up / Down** to shift every chord by
   one semitone.

## Commands

Commands are available in the Command Palette when the active editor
has language `chordpro`.

| Command | Title | Notes |
|---|---|---|
| `chordsketch.openPreview` | ChordSketch: Open Preview | |
| `chordsketch.openPreviewToSide` | ChordSketch: Open Preview to the Side | |
| `chordsketch.transposeUp` | ChordSketch: Transpose Up | Shifts every chord by +1 semitone |
| `chordsketch.transposeDown` | ChordSketch: Transpose Down | Shifts every chord by −1 semitone |
| `chordsketch.convertTo` | ChordSketch: Export As… | Exports the current song to HTML, text, or PDF |

## Configuration

Settings live under **ChordSketch** in Settings UI (or edit
`settings.json` directly).

| Setting | Type | Default | Purpose |
|---|---|---|---|
| `chordsketch.lsp.enabled` | `boolean` | `true` | Enable the Language Server (diagnostics, completions, hover, formatting). |
| `chordsketch.lsp.path` | `string` | `""` | Absolute path to a `chordsketch-lsp` binary. Empty uses (1) the binary on `PATH` if present, then (2) the bundled binary for your platform. |
| `chordsketch.preview.defaultMode` | `"html"` \| `"text"` | `"html"` | Default rendering mode when a new preview panel opens. Changing this affects the next preview only; existing panels keep their persisted mode. |

## Features

- **Syntax highlighting** for ChordPro directives, chord symbols,
  comments, and tab/ABC blocks via a TextMate grammar.
- **Live preview** that re-renders on each edit, with HTML and plain
  text modes (see `chordsketch.preview.defaultMode`).
- **Transpose** commands that shift every chord by ±1 semitone.
- **Export** the current song as HTML, plain text, or PDF via
  `chordsketch.convertTo`.
- **LSP-backed diagnostics, completions, hover, and formatting** when
  `chordsketch.lsp.enabled` is on (default).

## Links

- Main repository: <https://github.com/koedame/chordsketch>
- Playground (browser-hosted renderer): <https://chordsketch.koeda.me>
- Issue tracker: <https://github.com/koedame/chordsketch/issues>
- ChordPro format specification: <https://www.chordpro.org/chordpro/>

## License

MIT. See [LICENSE](LICENSE).
