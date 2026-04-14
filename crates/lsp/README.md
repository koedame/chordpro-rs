<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-lsp

Language Server Protocol (LSP) server for
[ChordPro](https://www.chordpro.org/) files. Provides diagnostics,
completions, hover information, and formatting for `.cho`, `.chordpro`,
and `.chopro` files in any editor that supports LSP.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Installation

```bash
cargo install chordsketch-lsp
```

## Features

| Feature | Description |
|---------|-------------|
| Diagnostics | Parse error reporting with line/column positions |
| Completions | Directive names, chord names, metadata keys (triggered on `{` and `[`) |
| Hover | Chord diagrams and directive documentation |
| Formatting | Full document formatting |

## Editor Setup

See [docs/editors.md](../../docs/editors.md) for detailed setup instructions
for VS Code, JetBrains IDEs, Zed, Neovim, and Helix.

### Generic LSP configuration

The server communicates over stdio:

```json
{
  "command": "chordsketch-lsp",
  "args": ["--stdio"],
  "filetypes": ["chordpro"],
  "root_markers": [".git"]
}
```

## Links

- [ChordSketch repository](https://github.com/koedame/chordsketch)
- [Playground](https://chordsketch.koeda.me)
- [Editor integration guide](../../docs/editors.md)
- [Issue tracker](https://github.com/koedame/chordsketch/issues)

## License

MIT
