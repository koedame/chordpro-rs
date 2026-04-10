#!/usr/bin/env bash
# check-action-pins.sh — Verify all GitHub Actions references are SHA-pinned.
#
# Exits with status 1 and prints offending lines if any `uses:` reference
# points to a mutable tag (@v*, @main, @master) instead of a full 40-char
# commit SHA.
set -euo pipefail

WORKFLOWS_DIR="${1:-.github}"
FOUND=0

while IFS= read -r -d '' file; do
  while IFS= read -r line; do
    # Match "uses: owner/repo@REF" where REF is not a 40-char hex SHA
    if echo "$line" | grep -qE '^\s*uses:\s+[^@]+@[^#[:space:]]+'; then
      ref=$(echo "$line" | sed -E 's/.*@([^# ]+).*/\1/')
      # A valid SHA is exactly 40 lowercase hex characters
      if ! echo "$ref" | grep -qE '^[0-9a-f]{40}$'; then
        echo "NOT SHA-PINNED: $file"
        echo "  $line"
        FOUND=1
      fi
    fi
  done < "$file"
done < <(find "$WORKFLOWS_DIR" -name '*.yml' -print0)

if [ "$FOUND" -eq 1 ]; then
  echo ""
  echo "All 'uses:' action references must be pinned to a full 40-character commit SHA."
  echo "Replace tag references (e.g. @v3) with the corresponding SHA and add a comment:"
  echo "  uses: actions/checkout@<40-char-sha> # v6"
  exit 1
fi

echo "All action references are SHA-pinned."
