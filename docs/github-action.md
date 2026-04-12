# ChordSketch GitHub Action

The ChordSketch composite action installs the CLI binary and renders one
ChordPro (`.cho`) file per step. It is a zero-setup way to produce HTML,
PDF, or plain-text songbooks from a Git repository as part of any GitHub
Actions workflow.

## Usage

```yaml
- uses: koedame/chordsketch/packages/github-action@action-v1
  with:
    input: songs/amazing-grace.cho
    output: dist/amazing-grace.html
    format: html
```

### Inputs

| Name        | Required | Default  | Description |
|-------------|----------|----------|-------------|
| `input`     | yes      | —        | Path to the `.cho` source file (relative to the repository root or absolute) |
| `output`    | yes      | —        | Path for the rendered output file (parent directories are created automatically) |
| `format`    | no       | `text`   | Output format: `text`, `html`, or `pdf` |
| `transpose` | no       | `0`      | Semitones to transpose (positive = up, negative = down) |
| `version`   | no       | `latest` | ChordSketch release tag to install, e.g. `v0.2.0`. `latest` resolves the current GitHub Release at runtime. |

### Outputs

| Name          | Description |
|---------------|-------------|
| `output-path` | Absolute path to the rendered output file (useful for subsequent upload steps) |

## Platform support

Pre-built binaries are downloaded for the runner OS:

| Runner OS  | Architecture | Binary target |
|------------|-------------|---------------|
| Linux      | x86\_64     | `x86_64-unknown-linux-musl` |
| Linux      | arm64       | `aarch64-unknown-linux-musl` |
| macOS      | x86\_64     | `x86_64-apple-darwin` |
| macOS      | arm64       | `aarch64-apple-darwin` |
| Windows    | x86\_64     | `x86_64-pc-windows-msvc` |

No Rust toolchain is required on the runner.

## Examples

### Render to HTML and upload as artifact

```yaml
jobs:
  render:
    runs-on: ubuntu-latest
    steps:
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

### Transpose before rendering

```yaml
      - uses: koedame/chordsketch/packages/github-action@action-v1
        with:
          input: songs/song.cho
          output: dist/song-capo2.html
          format: html
          transpose: '-2'
```

### Pin to a specific version

```yaml
      - uses: koedame/chordsketch/packages/github-action@action-v1
        with:
          input: songs/song.cho
          output: dist/song.txt
          version: v0.2.0
```

## How it works

1. Resolves the requested version tag (or queries the GitHub API for `latest`).
2. Downloads the pre-built binary tarball/zip for the current runner platform
   from the GitHub Release.
3. Adds the binary directory to `$PATH`.
4. Runs `chordsketch -f <format> -o <output> <input>`, creating output
   directories as needed.
5. Sets `outputs.output-path` to the absolute path of the generated file.

## Tagging convention

The action is versioned independently from the main ChordSketch CLI releases.
Tags follow the `action-vX[.Y.Z]` convention (e.g. `action-v1`, `action-v1.0.1`)
and are created alongside regular ChordSketch release tags.

## Links

- [ChordSketch repository](https://github.com/koedame/chordsketch)
- [ChordPro format reference](https://www.chordpro.org/chordpro/)
- [ChordSketch Playground](https://chordsketch.koeda.me)
- [Issue tracker](https://github.com/koedame/chordsketch/issues)
