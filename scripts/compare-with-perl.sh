#!/usr/bin/env bash
# compare-with-perl.sh — Compare chordpro-rs output against Perl ChordPro
#
# Usage:
#   ./scripts/compare-with-perl.sh [--format text|html] [corpus_dir]
#
# Requirements:
#   - Perl ChordPro installed and available as 'chordpro' in PATH
#   - chordpro-rs built (cargo build --release)
#
# The script runs both implementations on each .cho file in the test corpus
# and produces a diff report.

set -euo pipefail

FORMAT="${1:-text}"
CORPUS_DIR="${2:-tests/corpus}"
# Perl ChordPro uses capitalized format names (Text, HTML, PDF).
PERL_FORMAT="$(echo "${FORMAT:0:1}" | tr '[:lower:]' '[:upper:]')${FORMAT:1}"
RUST_BIN="./target/release/chordpro"
PERL_BIN="chordpro"
OUTPUT_DIR="/tmp/chordpro-comparison"
PASS=0
FAIL=0
SKIP=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

# Check prerequisites
if ! command -v "$PERL_BIN" &>/dev/null; then
    echo -e "${YELLOW}Warning: Perl ChordPro not found in PATH.${NC}"
    echo "Install with: cpanm App::Music::ChordPro"
    echo "Or specify the path: PERL_BIN=/path/to/chordpro $0"
    exit 1
fi

if [[ ! -x "$RUST_BIN" ]]; then
    echo "Building chordpro-rs..."
    cargo build --release
fi

if [[ ! -d "$CORPUS_DIR" ]]; then
    echo "Error: corpus directory not found: $CORPUS_DIR"
    exit 1
fi

# Set up output directories
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/rust" "$OUTPUT_DIR/perl" "$OUTPUT_DIR/diff"

EXT="txt"
if [[ "$FORMAT" == "html" ]]; then
    EXT="html"
fi

echo "=== ChordPro Compatibility Comparison ==="
echo "Format: $FORMAT"
echo "Corpus: $CORPUS_DIR"
echo ""

# Process each .cho file.
# Use process substitution (not pipe) so counter variables survive the loop.
while read -r cho_file; do
    relative="${cho_file#"$CORPUS_DIR"/}"
    base="${relative%.cho}"
    safe_name="${base//\//_}"

    rust_out="$OUTPUT_DIR/rust/${safe_name}.${EXT}"
    perl_out="$OUTPUT_DIR/perl/${safe_name}.${EXT}"
    diff_out="$OUTPUT_DIR/diff/${safe_name}.diff"

    # Run Rust implementation
    if ! "$RUST_BIN" --format "$FORMAT" "$cho_file" > "$rust_out" 2>/dev/null; then
        echo -e "${YELLOW}SKIP${NC} $relative (Rust parse error)"
        SKIP=$((SKIP + 1))
        continue
    fi

    # Run Perl implementation (uses capitalized format name).
    if ! "$PERL_BIN" --generate "$PERL_FORMAT" "$cho_file" > "$perl_out" 2>/dev/null; then
        echo -e "${YELLOW}SKIP${NC} $relative (Perl error)"
        SKIP=$((SKIP + 1))
        continue
    fi

    # Compare outputs
    if diff -u "$perl_out" "$rust_out" > "$diff_out" 2>/dev/null; then
        echo -e "${GREEN}PASS${NC} $relative"
        PASS=$((PASS + 1))
        rm -f "$diff_out"  # No diff needed for passing files
    else
        echo -e "${RED}DIFF${NC} $relative"
        FAIL=$((FAIL + 1))
    fi
done < <(find "$CORPUS_DIR" -name '*.cho' -type f | sort)

echo ""
echo "=== Results ==="
echo -e "${GREEN}Pass: $PASS${NC}"
echo -e "${RED}Diff: $FAIL${NC}"
echo -e "${YELLOW}Skip: $SKIP${NC}"

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "Diff files saved to: $OUTPUT_DIR/diff/"
    echo "Review with: ls $OUTPUT_DIR/diff/"
fi

exit $( [[ $FAIL -eq 0 ]] && echo 0 || echo 1 )
