# Editor Integration

ChordSketch provides language support for ChordPro files in multiple editors.

## VS Code / Cursor / Windsurf / VSCodium

Install the **ChordSketch** extension from the
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=koedame.chordsketch)
or [Open VSX Registry](https://open-vsx.org/).

The extension provides syntax highlighting, live preview, chord transposition,
and LSP integration out of the box.

## Zed

Install the **ChordPro** extension from the Zed extensions panel (command
palette → "zed: extensions" → search for "ChordPro").

The extension provides syntax highlighting via a tree-sitter grammar and LSP
integration via `chordsketch-lsp`.

### LSP setup

The Zed extension requires `chordsketch-lsp` to be installed and available in
your `PATH`:

```bash
cargo install chordsketch-lsp
```

### Manual configuration (alternative)

If the extension is not yet available in the Zed extension registry, you can
install it as a dev extension:

1. Clone the repository:
   ```bash
   git clone https://github.com/koedame/chordsketch.git
   ```
2. In Zed, open the command palette and run **"zed: install dev extension"**
3. Select the `packages/zed-extension` directory

## Language Server (any editor)

`chordsketch-lsp` implements the Language Server Protocol and can be used with
any editor that supports LSP. Install the server:

```bash
cargo install chordsketch-lsp
```

The server communicates over stdio and supports:

| Feature | Description |
|---------|-------------|
| Diagnostics | Parse error reporting |
| Completions | Directive names, chord names, metadata keys (triggered on `{` and `[`) |
| Hover | Chord diagrams and directive documentation |
| Formatting | Full document formatting |

### Generic LSP configuration

Point your editor's LSP client at the `chordsketch-lsp` binary with `--stdio`:

```json
{
  "command": "chordsketch-lsp",
  "args": ["--stdio"],
  "filetypes": ["chordpro"],
  "root_markers": [".git"]
}
```

Associate these file extensions with the ChordPro file type: `.cho`,
`.chordpro`, `.chopro`.
