<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="100" height="100">
</p>

# ChordSketch

A Rust implementation of the [ChordPro](https://www.chordpro.org/) file format
parser and renderer. 100% ChordPro compatible. Supports parsing ChordPro files
into a structured AST and rendering to plain text, HTML, and PDF.

## Features

- Full [ChordPro](https://www.chordpro.org/chordpro/) format parser with zero
  external dependencies in the core crate
- Three output formats: plain text, HTML, and PDF
- Chord transposition
- Configuration file system (chordsketch.json)
- Inline markup (bold, italic, etc.)
- Chord diagrams and extended `{define}` directives
- Section environments (verse, chorus, tab, grid, custom)
- Delegate environments (ABC, Lilypond, SVG, textblock)
- Conditional directive selectors (instrument, user)
- Multi-song files (`{new_song}`)
- Font, size, and color directives
- Image directive
- Multi-page PDF with page control

## Try it Online

**[ChordSketch Playground](https://koedame.github.io/chordsketch/)** — try
ChordPro rendering directly in your browser, no installation required.

## Editor Integration

ChordSketch provides syntax highlighting and Language Server Protocol (LSP)
support for multiple editors:

- **VS Code / Cursor / Windsurf / VSCodium** — install the [ChordSketch extension](https://marketplace.visualstudio.com/items?itemName=koedame.chordsketch)
- **JetBrains IDEs** (IntelliJ IDEA, PyCharm, WebStorm, etc.) — install the ChordPro plugin
- **Zed** — install the ChordPro extension from the extensions panel
- **Neovim** — manual tree-sitter + LSP configuration
- **Helix** — manual grammar + LSP configuration

See [docs/editors.md](docs/editors.md) for detailed setup instructions.

## Installation

### npm (WASM)

```bash
npm install @chordsketch/wasm
```

See the [@chordsketch/wasm README](packages/npm/README.md) for usage with
JavaScript/TypeScript.

### Homebrew (macOS / Linux)

```bash
brew tap koedame/tap
brew install chordsketch
```

### Scoop (Windows)

```bash
scoop bucket add koedame https://github.com/koedame/scoop-bucket
scoop install chordsketch
```

### winget (Windows)

```bash
winget install koedame.chordsketch
```

### Docker

```bash
docker run --rm ghcr.io/koedame/chordsketch --version
docker run --rm -v "$PWD:/data" ghcr.io/koedame/chordsketch /data/song.cho
```

### From crates.io

```bash
cargo install chordsketch
```

### From source

Requires Rust 1.85 or later.

```bash
git clone https://github.com/koedame/chordsketch.git
cd chordsketch
cargo install --path crates/cli
```

## Usage

```bash
# Render to plain text (default)
chordsketch song.cho

# Render to HTML
chordsketch -f html song.cho -o song.html

# Render to PDF
chordsketch -f pdf song.cho -o song.pdf

# Transpose up 2 semitones
chordsketch --transpose 2 song.cho

# Use a custom config file
chordsketch -c myconfig.json song.cho

# Process multiple files
chordsketch -f pdf song1.cho song2.cho -o songbook.pdf
```

## Library Usage

The core parser and renderers are available as separate library crates:

```rust
use chordsketch_core::parser::parse;
use chordsketch_render_text::render_song;

let input = "{title: Amazing Grace}\n{subtitle: Traditional}\n\n[G]Amazing [G7]grace, how [C]sweet the [G]sound";
let song = parse(input).unwrap();
let text = render_song(&song);
println!("{text}");
```

## Workspace Structure

| Crate | Description |
|---|---|
| [`chordsketch-core`](crates/core) | Parser, AST, and transforms (zero external dependencies) |
| [`chordsketch-render-text`](crates/render-text) | Plain text renderer |
| [`chordsketch-render-html`](crates/render-html) | HTML renderer |
| [`chordsketch-render-pdf`](crates/render-pdf) | PDF renderer |
| [`chordsketch`](crates/cli) | Command-line tool |
| [`chordsketch-lsp`](crates/lsp) | Language Server Protocol server |
| [`chordsketch-wasm`](crates/wasm) | WebAssembly bindings via wasm-bindgen |
| [`chordsketch-convert-musicxml`](crates/convert-musicxml) | MusicXML ↔ ChordPro bidirectional converter |

### Packages

| Package | Path | Description |
|---|---|---|
| [`@chordsketch/wasm`](packages/npm) | `packages/npm` | npm package with TypeScript types |
| [Playground](packages/playground) | `packages/playground` | Browser-based ChordPro editor and renderer |

## GitHub Actions

Use the composite action to render ChordPro files in any GitHub Actions
workflow — no Rust toolchain required:

```yaml
- uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6

- uses: koedame/chordsketch/packages/github-action@action-v1
  id: render
  with:
    input: songs/setlist.cho
    output: dist/setlist.html
    format: html

- uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4
  with:
    name: setlist-html
    path: ${{ steps.render.outputs.output-path }}
```

See [docs/github-action.md](docs/github-action.md) for full input/output
reference and additional examples.

## Links

- [ChordPro file format specification](https://www.chordpro.org/chordpro/)
- [Configuration guide](docs/configuration.md)
- [Versioning and release process](docs/releasing.md)
- [GitHub Action reference](docs/github-action.md)
- [Architecture decision records](docs/adr/README.md)
- [SECURITY.md](SECURITY.md)
- [CHANGELOG.md](CHANGELOG.md)

## License

SDK crates (core, renderers, CLI): [MIT](LICENSE)

Future application layer (Forum, Playground, Desktop): AGPL-3.0-only
