<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# tree-sitter-chordpro

A [Tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for
[ChordPro](https://www.chordpro.org/) music notation files (`.cho`,
`.chordpro`, `.chopro`).

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Supported Syntax

| Element | Example | Node type |
|---------|---------|-----------|
| Comments | `# comment` | `comment` |
| Directives | `{title: Song Name}` | `directive` (with `directive_name`, `directive_value`) |
| Delegate blocks | `{start_of_abc}...{end_of_abc}` | `delegate_block` (with `block_start_directive`, `block_content`, `block_end_directive`) |
| Chords | `[Am]`, `[G/B]` | `chord` (with `chord_name`) |
| Lyrics | `Amazing grace` | `lyrics` |

## Usage

### In a Zed extension

Reference this grammar in your `extension.toml`:

```toml
[grammars.chordpro]
repository = "https://github.com/koedame/chordsketch"
rev = "COMMIT_HASH"
path = "packages/tree-sitter-chordpro"
```

### Development

```bash
# Generate the parser
npx tree-sitter generate

# Run tests
npx tree-sitter test

# Parse a file
npx tree-sitter parse example.cho

# Preview highlighting
npx tree-sitter highlight example.cho
```

## Links

- [ChordSketch](https://github.com/koedame/chordsketch) — main project
- [ChordPro format](https://www.chordpro.org/chordpro/) — file format specification
- [Issues](https://github.com/koedame/chordsketch/issues) — bug reports

## License

MIT
