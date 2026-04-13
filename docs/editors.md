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

## Neovim

ChordPro support in Neovim requires manual configuration until the
tree-sitter grammar is published to nvim-treesitter and `chordsketch-lsp`
is added to nvim-lspconfig.

### Prerequisites

```bash
cargo install chordsketch-lsp
```

### File type recognition

Add to your `init.lua`:

```lua
vim.filetype.add({
  extension = {
    cho = "chordpro",
    chordpro = "chordpro",
    chopro = "chordpro",
  },
})
```

### Tree-sitter highlighting

If you use [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter),
register the ChordPro parser:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.chordpro = {
  install_info = {
    url = "https://github.com/koedame/chordsketch",
    files = { "src/parser.c", "src/scanner.c" },
    location = "packages/tree-sitter-chordpro",
    branch = "main",
  },
  filetype = "chordpro",
}
```

Then install the parser:

```vim
:TSInstall chordpro
```

Copy the highlight queries to your Neovim runtime:

```bash
mkdir -p ~/.config/nvim/queries/chordpro
cp packages/tree-sitter-chordpro/queries/highlights.scm \
   ~/.config/nvim/queries/chordpro/highlights.scm
```

### LSP setup

Since `chordsketch-lsp` is not yet in nvim-lspconfig, configure it
manually:

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "chordpro",
  callback = function()
    vim.lsp.start({
      name = "chordsketch-lsp",
      cmd = { "chordsketch-lsp", "--stdio" },
      root_dir = vim.fs.dirname(
        vim.fs.find({ ".git" }, { upward = true })[1]
      ),
    })
  end,
})
```

## Helix

ChordPro support in Helix requires manual configuration until the
grammar is submitted upstream to helix-editor/helix.

### Prerequisites

```bash
cargo install chordsketch-lsp
```

### Configuration

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "chordpro"
scope = "source.chordpro"
file-types = ["cho", "chordpro", "chopro"]
comment-token = "#"
language-servers = ["chordsketch-lsp"]

[language-server.chordsketch-lsp]
command = "chordsketch-lsp"
args = ["--stdio"]

[[grammar]]
name = "chordpro"
source = { git = "https://github.com/koedame/chordsketch", rev = "main", subpath = "packages/tree-sitter-chordpro" }
```

> **Tip:** For reproducible builds, replace `rev = "main"` with a specific
> commit hash (e.g. `rev = "404b0a9"`). Using `"main"` always fetches the
> latest grammar on `hx --grammar fetch`.

### Building the grammar

Fetch and build the tree-sitter grammar:

```bash
hx --grammar fetch
hx --grammar build
```

Copy the highlight queries to the Helix runtime:

```bash
mkdir -p ~/.config/helix/runtime/queries/chordpro
cp packages/tree-sitter-chordpro/queries/highlights.scm \
   ~/.config/helix/runtime/queries/chordpro/highlights.scm
```

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
