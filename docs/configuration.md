# Configuration Guide

chordpro-rs uses a layered configuration system that merges settings from
multiple sources. Later sources override earlier ones.

## Configuration Hierarchy

Settings are loaded and deep-merged in this order (highest precedence last):

| Priority | Source | Location |
|----------|--------|----------|
| 1 (lowest) | Built-in defaults | Hardcoded in the binary |
| 2 | System config | `/etc/chordpro.json` |
| 3 | User config | `~/.config/chordpro/chordpro.json` (or `$XDG_CONFIG_HOME/chordpro/chordpro.json`) |
| 4 | Project config | `chordpro.json` in the song file's directory |
| 5 | CLI `--config` files | Specified via `-c` / `--config` (may be repeated) |
| 6 | CLI `--define` values | Specified via `-D` / `--define` |
| 7 (highest) | Song-level overrides | `{+config.KEY: VALUE}` directives inside the song file |

For map (object) values, keys are recursively deep-merged. Array values are
replaced entirely, not concatenated.

## RRJSON Format

Configuration files use RRJSON (Really Relaxed JSON), a superset of JSON that
allows:

- **Comments**: `//` line comments and `/* */` block comments
- **Trailing commas**: in arrays and objects
- **Unquoted keys**: `settings: {}` instead of `"settings": {}`
- **Single-quoted strings**: `'value'` in addition to `"value"`
- **Dot-separated keys**: `pdf.chorus.indent = 20` as a shorthand for nested
  objects
- **Optional outer braces**: the top-level `{ }` may be omitted

Example:

```json
// My custom config
{
  settings: {
    columns: 2,
    transpose: 0,
  },
  pdf: {
    papersize: "a4",
    chorus: {
      indent: 20,
    },
  },
}
```

Or using the flat dot-key shorthand:

```
pdf.chorus.indent = 20
settings.columns = 2
```

## CLI Flags

### `--config` / `-c`

Load one or more configuration files. May be specified multiple times; files are
merged in order.

```bash
chordpro -c custom.json song.cho
chordpro -c base.json -c overrides.json song.cho
```

You can also pass a preset name instead of a file path:

```bash
chordpro -c guitar song.cho
chordpro -c ukulele song.cho
```

Available presets: `guitar`, `ukulele`.

### `--define` / `-D`

Set a configuration value at runtime. Takes the highest precedence among CLI
options. Format: `key=value`.

```bash
chordpro -D settings.columns=2 song.cho
chordpro -D pdf.chorus.indent=30 song.cho
```

### `--no-default-configs`

Skip loading system, user, and project config files. Only built-in defaults are
used as the base. `--config` and `--define` still apply on top.

```bash
chordpro --no-default-configs -c myconfig.json song.cho
```

### `--instrument`

Set the active instrument for selector filtering. Equivalent to
`--define instrument.type=<INSTRUMENT>`.

```bash
chordpro --instrument piano song.cho
```

## Configuration Sections

### `settings`

General rendering settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `settings.columns` | integer | `1` | Number of columns for layout |
| `settings.suppress_empty_chords` | boolean | `true` | Hide empty chord lines |
| `settings.lyrics_only` | boolean | `false` | Suppress all chords |
| `settings.transpose` | integer | `0` | Transpose all chords by N semitones |

### `pdf`

PDF renderer settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `pdf.papersize` | string | `"a4"` | Paper size |
| `pdf.theme` | object | | Theme settings (colors, etc.) |
| `pdf.fonts` | object | | Font configuration |
| `pdf.spacing` | object | | Line and section spacing |
| `pdf.chorus` | object | | Chorus indentation and styling |
| `pdf.margins` | object | | Page margins |
| `pdf.columns.gap` | number | `20` | Gap between columns in points |

### `html`

HTML renderer settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `html.styles.body` | string | | CSS for the body element |
| `html.styles.chord` | string | | CSS for chord elements |
| `html.styles.comment` | string | | CSS for comment elements |

### `chords`

Chord display settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `chords.show` | string | `"all"` | Which chords to display |
| `chords.capo.show` | boolean | `true` | Show capo indicator in chord names |

### `metadata`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `metadata.separator` | string | `"; "` | Separator for multi-value metadata |

### `instrument`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `instrument.type` | string | `"guitar"` | Active instrument |
| `instrument.description` | string | | Instrument description |

### `delegates`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `delegates.abc2svg` | boolean/null | `null` | ABC delegate (null = auto-detect) |
| `delegates.lilypond` | boolean/null | `null` | Lilypond delegate (null = auto-detect) |

## Song-Level Configuration Overrides

Songs can override configuration values using the `{+config.KEY: VALUE}`
directive syntax:

```
{title: My Song}
{+config.settings.columns: 2}
{+config.pdf.chorus.indent: 30}
```

Allowed key prefixes: `settings.`, `pdf.`, `html.`, `chords.`, `metadata.`,
`instrument.`, `diagrams.`. The exact key `tuning` is also allowed.

Keys outside the allowlist are silently ignored (a warning is emitted). A
maximum of 1000 overrides per song is enforced.

## Selector Filtering

Directives can target specific instruments or users with a selector suffix:

```
{textfont-piano: Courier}
{textfont-guitar: Helvetica}
{textsize-ukulele: 10}
```

The selector is the portion after the last hyphen in the directive name. When
the `--instrument` flag is set, only directives matching that instrument (or
directives with no selector) are applied.
