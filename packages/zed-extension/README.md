<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# ChordPro (Zed Extension)

[ChordPro](https://www.chordpro.org/) language support for the
[Zed](https://zed.dev/) editor, powered by
[ChordSketch](https://github.com/koedame/chordsketch).

This extension provides syntax highlighting and LSP integration for `.cho`,
`.chordpro`, and `.chopro` files.

## Installation

### From Zed Extensions (once published)

Open the Zed command palette and search for **"ChordPro"** in the extensions
panel.

### Development install

```bash
git clone https://github.com/koedame/chordsketch.git
cd chordsketch/packages/zed-extension
# In Zed: command palette → "zed: install dev extension" → select this directory
```

## Quick Start

1. Install the extension (see above)
2. Install the language server:
   ```bash
   cargo install chordsketch-lsp
   ```
3. Open any `.cho` file in Zed — syntax highlighting and LSP features activate
   automatically

Example ChordPro file:

```chordpro
{title: Amazing Grace}
{subtitle: Traditional}

{start_of_verse: Verse 1}
[G]Amazing [G7]grace, how [C]sweet the [G]sound
That [G]saved a [Em]wretch like [D]me
{end_of_verse}
```

## Features

| Feature | Source | Description |
|---------|--------|-------------|
| Syntax highlighting | tree-sitter grammar | Comments, directives, chords, lyrics, delegate blocks |
| Diagnostics | `chordsketch-lsp` | Parse error reporting |
| Completions | `chordsketch-lsp` | Directive names, chord names, metadata keys (on `{` and `[`) |
| Hover | `chordsketch-lsp` | Chord diagrams and directive documentation |
| Formatting | `chordsketch-lsp` | Full document formatting |

## Configuration

The extension looks for `chordsketch-lsp` in your `PATH`. If the binary is
installed in a non-standard location, add its directory to your `PATH`
environment variable before launching Zed.

No additional Zed settings are required.

## Links

- [ChordSketch](https://github.com/koedame/chordsketch) — main project
- [Playground](https://chordsketch.koeda.me) — browser-based demo
- [Editor setup guide](https://github.com/koedame/chordsketch/blob/main/docs/editors.md) — all editors
- [Issues](https://github.com/koedame/chordsketch/issues) — bug reports

## License

MIT
