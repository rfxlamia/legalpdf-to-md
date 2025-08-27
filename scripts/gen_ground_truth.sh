#!/usr/bin/env bash
set -euo pipefail

# Generate structural ground truth from current output/*.md
# Counts headings anchored at line starts: '## BAB ' and '## Pasal N'

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
OUT_DIR="$ROOT_DIR/output"
FIXT_DIR="$ROOT_DIR/tests/fixtures"
GT="$FIXT_DIR/ground_truth.yaml"

mkdir -p "$FIXT_DIR"

echo "# Ground truth frozen from current outputs" > "$GT"
echo "# doc_id: { bab: <int>, pasal: <int> }" >> "$GT"

shopt -s nullglob
for d in "$OUT_DIR"/*/; do
  [ -d "$d" ] || continue
  doc_id=$(basename "$d")
  md="$d/${doc_id}.md"
  if [ ! -f "$md" ]; then
    echo "# skip: $doc_id has no md" >> "$GT"
    continue
  fi
  bab=$(rg -n '^##\s+BAB\s+[IVXLCDM]+' -c "$md" || true)
  pasal=$(rg -n '^##\s+Pasal\s+\d+' -c "$md" || true)
  echo "$doc_id: { bab: ${bab:-0}, pasal: ${pasal:-0} }" >> "$GT"
done

echo "Wrote $GT"

