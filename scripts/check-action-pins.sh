#!/usr/bin/env bash
# check-action-pins.sh — Verify all GitHub Actions references are SHA-pinned.
#
# Exits with status 1 and prints offending lines if any `uses:` reference
# points to a mutable tag (@v*, @main, @master) instead of a pinned ref.
#
# Two pin formats are accepted:
#   1. Git commit SHA  — exactly 40 lowercase hex characters
#      uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd
#   2. Docker image digest — sha256: followed by 64 lowercase hex characters
#      uses: docker://alpine@sha256:1234567890abcdef...
#
# Any other ref (e.g. @v3, @main, @stable) is treated as unpinned.
set -euo pipefail

WORKFLOWS_DIR="${1:-.github}"
FOUND=0

while IFS= read -r -d '' file; do
  while IFS= read -r line; do
    # Match "uses: owner/repo@REF" or "uses: docker://image@REF"
    if printf '%s\n' "$line" | grep -qE '^\s*uses:\s+[^@]+@[^#[:space:]]+'; then
      ref=$(printf '%s\n' "$line" | sed -E 's/.*@([^# ]+).*/\1/')
      # Accept a 40-char git commit SHA
      if printf '%s\n' "$ref" | grep -qE '^[0-9a-f]{40}$'; then
        continue
      fi
      # Accept a Docker image digest: sha256:<64-char hex>
      if printf '%s\n' "$ref" | grep -qE '^sha256:[0-9a-f]{64}$'; then
        continue
      fi
      echo "NOT SHA-PINNED: $file"
      echo "  $line"
      FOUND=1
    fi
  done < "$file"
done < <(find "$WORKFLOWS_DIR" \( -name '*.yml' -o -name '*.yaml' \) -print0)

if [ "$FOUND" -eq 1 ]; then
  echo ""
  echo "All 'uses:' action references must be pinned to an immutable ref:"
  echo "  - Git commit SHA (40 hex chars): uses: actions/checkout@<sha> # v6"
  echo "  - Docker digest:                 uses: docker://alpine@sha256:<64 hex chars>"
  exit 1
fi

echo "All action references are SHA-pinned."
