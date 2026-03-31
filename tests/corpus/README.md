# ChordPro Test Corpus

A collection of 55 `.cho` files organized by category, designed to exercise all major
features of the ChordPro file format. Each file is a valid ChordPro file focused on
specific features.

## Directory Structure

### basic/ (10 files)

Fundamental ChordPro constructs: lyrics, chords, sections, comments.

| File | Description |
|------|-------------|
| `01-empty.cho` | Empty file (zero bytes) |
| `02-title-only.cho` | Single `{title}` directive, no content |
| `03-simple-lyrics.cho` | Lyrics text with no chords |
| `04-chords-and-lyrics.cho` | Inline `[chord]lyrics` notation |
| `05-multiple-verses.cho` | Multiple `{start_of_verse}`/`{end_of_verse}` sections |
| `06-chorus.cho` | `{start_of_chorus}`/`{end_of_chorus}` sections |
| `07-chorus-recall.cho` | `{chorus}` directive to recall a previously defined chorus |
| `08-bridge.cho` | `{start_of_bridge}`/`{end_of_bridge}` section |
| `09-tab.cho` | `{start_of_tab}`/`{end_of_tab}` with tablature content |
| `10-comments.cho` | `{comment}`, `{comment_italic}`, `{comment_box}` directives |

### directives/ (15 files)

All directive types: metadata, chord definitions, layout, fonts, selectors.

| File | Description |
|------|-------------|
| `01-metadata.cho` | All standard metadata directives (title, artist, key, tempo, etc.) |
| `02-meta.cho` | `{meta: key value}` generic metadata directives |
| `03-transpose.cho` | `{transpose: N}` directive |
| `04-define.cho` | `{define}` with fret positions for guitar |
| `05-define-keyboard.cho` | `{define}` with `keys` for keyboard instruments |
| `06-define-copy.cho` | `{define}` with `copy` to alias chord names |
| `07-new-page.cho` | `{new_page}` and `{new_physical_page}` directives |
| `08-columns.cho` | `{columns}` and `{column_break}` directives |
| `09-image.cho` | `{image}` directive with various attributes |
| `10-textfont.cho` | `{textfont}`, `{textsize}`, `{textcolour}` directives |
| `11-chordfont.cho` | `{chordfont}`, `{chordsize}`, `{chordcolour}` directives |
| `12-titlefont.cho` | `{titlefont}`, `{titlesize}`, `{titlecolour}` directives |
| `13-grid.cho` | `{start_of_grid}`/`{end_of_grid}` sections |
| `14-custom-section.cho` | Custom section names (intro, solo, outro) |
| `15-selectors.cho` | Instrument and user selectors on directives |

### formatting/ (10 files)

Inline markup tags within lyrics text.

| File | Description |
|------|-------------|
| `01-bold.cho` | `<b>bold</b>` inline markup |
| `02-italic.cho` | `<i>italic</i>` inline markup |
| `03-mixed-markup.cho` | Nested bold and italic tags |
| `04-highlight.cho` | `<highlight>text</highlight>` markup |
| `05-span.cho` | `<span foreground="color">` markup |
| `06-span-attrs.cho` | `<span>` with multiple attributes (font, size, colors) |
| `07-unclosed-tags.cho` | Unclosed markup tags (error recovery) |
| `08-case-insensitive.cho` | Uppercase tag names (`<B>`, `<I>`, `<BOLD>`) |
| `09-markup-with-chords.cho` | Inline markup interleaved with chord brackets |
| `10-comment-markup.cho` | Markup inside comment directives |

### multi-song/ (5 files)

Multiple songs in a single file separated by `{new_song}`.

| File | Description |
|------|-------------|
| `01-two-songs.cho` | Two songs separated by `{new_song}` |
| `02-three-songs.cho` | Three songs in one file |
| `03-songs-with-metadata.cho` | Each song has independent metadata |
| `04-song-with-transpose.cho` | Different `{transpose}` per song |
| `05-empty-song.cho` | `{new_song}` followed by an empty second song |

### edge-cases/ (10 files)

Boundary conditions, unusual input, and stress tests.

| File | Description |
|------|-------------|
| `01-no-newline-at-end.cho` | File without trailing newline |
| `02-windows-line-endings.cho` | CRLF (`\r\n`) line endings |
| `03-unicode-lyrics.cho` | CJK text and accented characters |
| `04-special-chars.cho` | `&`, `<`, `>`, `"`, `'` and other symbols in lyrics |
| `05-long-lines.cho` | Very long lyrics lines (stress test for wrapping) |
| `06-many-chords.cho` | Lines with 10+ chords |
| `07-chord-only-line.cho` | Lines containing only chords, no lyrics text |
| `08-empty-lines.cho` | Many consecutive empty lines |
| `09-unknown-directives.cho` | Unrecognized directive names |
| `10-mixed-everything.cho` | Combines metadata, sections, markup, tabs, grid, and more |

### delegate/ (5 files)

Delegate environments for embedded notation formats.

| File | Description |
|------|-------------|
| `01-abc.cho` | `{start_of_abc}` with ABC music notation |
| `02-lilypond.cho` | `{start_of_ly}` with Lilypond notation |
| `03-svg.cho` | `{start_of_svg}` with inline SVG markup |
| `04-textblock.cho` | `{start_of_textblock}` with free-form text |
| `05-multiple-delegates.cho` | Multiple delegate section types in one file |

## Usage

These files are used by integration tests and golden tests to validate parser and
renderer behavior. Each file targets specific ChordPro features so that regressions
can be traced to individual capabilities.

## Total: 55 files
