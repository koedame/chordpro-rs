# Comparison Scripts

## compare-with-perl.sh

Automated comparison tool that runs both chordpro-rs and the Perl ChordPro
reference implementation on the same `.cho` input files and diffs the output.

### Prerequisites

- **Perl ChordPro**: Install via `cpanm App::Music::ChordPro`
- **chordpro-rs**: Built with `cargo build --release`

### Usage

```bash
# Compare text output (default)
./scripts/compare-with-perl.sh

# Compare HTML output
./scripts/compare-with-perl.sh html

# Use a custom corpus directory
./scripts/compare-with-perl.sh text path/to/corpus
```

### Output

- **PASS**: Both implementations produce identical output
- **DIFF**: Output differs — diff saved to `/tmp/chordpro-comparison/diff/`
- **SKIP**: One implementation failed to process the file

### Reviewing Diffs

```bash
# List all diff files
ls /tmp/chordpro-comparison/diff/

# View a specific diff
cat /tmp/chordpro-comparison/diff/basic_04-chords-and-lyrics.diff
```
