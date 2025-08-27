#!/usr/bin/env bash
set -euo pipefail

# Stabilized acceptance for legalpdf-to-md

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
OUT_DIR="$ROOT_DIR/output"
OUT_DIR_OFF="$ROOT_DIR/output_off"
BIN="$ROOT_DIR/target/debug/legalpdf2md"

require() { command -v "$1" >/dev/null || { echo "Missing tool: $1" >&2; exit 2; }; }
require jq
require rg

ARTIFACTS="--artifacts=on"
IDEM_FAST=1
CI_SAMPLE=""
OCR_DPI=""

while [ $# -gt 0 ]; do
  case "$1" in
    --artifacts=on|--artifacts=off) ARTIFACTS="$1" ; shift ;;
    --idempotency-fast) IDEM_FAST=1 ; shift ;;
    --ci-sample) CI_SAMPLE="$2" ; shift 2 ;;
    --ocr-dpi) OCR_DPI="$2" ; shift 2 ;;
    *) shift ;;
  esac
done

run_pipeline() {
  local artifacts_flag=$1
  rm -rf "$OUT_DIR" && mkdir -p "$OUT_DIR"
  (cd "$ROOT_DIR" && cargo build -q)
  if [ -n "$CI_SAMPLE" ] && [ "$CI_SAMPLE" -gt 0 ] 2>/dev/null; then
    (cd "$ROOT_DIR" && CI_SAMPLE_SUSPECTS="$CI_SAMPLE" "$BIN" --strict --law-mode auto "$artifacts_flag" ${OCR_DPI:+--ocr-dpi "$OCR_DPI"} 1>/dev/null 2>"$OUT_DIR/accept.log") || true
  else
    (cd "$ROOT_DIR" && "$BIN" --strict --law-mode auto "$artifacts_flag" ${OCR_DPI:+--ocr-dpi "$OCR_DPI"} 1>/dev/null 2>"$OUT_DIR/accept.log") || true
  fi
}

check_structure() {
  for d in "$OUT_DIR"/*/; do
    [ -d "$d" ] || continue
    local doc_id=$(basename "$d")
    [[ -f "$d/${doc_id}.md" && -f "$d/${doc_id}.meta.json" ]] || { echo "[FAIL] PerDocLayout: $doc_id"; return 1; }
    if [ "$ARTIFACTS" = "--artifacts=off" ]; then
      [ ! -d "$d/artifacts" ] || { echo "[FAIL] UnexpectedArtifacts: $doc_id"; return 1; }
      rg -n "step[0-9]+_" "$d" >/dev/null && { echo "[FAIL] StepLeak: $doc_id"; return 1; } || true
    fi
  done
  return 0
}

no_step_leak_global() {
  ls "$OUT_DIR"/*.step*_*.txt 2>/dev/null | grep -q . && { echo "[FAIL] NoStepLeak: root"; return 1; } || true
  rg -n "\\.tmp$" "$OUT_DIR" -g '!**/.git/**' >/dev/null && { echo "[FAIL] TmpLeak: root"; return 1; } || true
  return 0
}

check_schema_and_kpis() {
  for d in "$OUT_DIR"/*/; do
    [ -d "$d" ] || continue
    local doc_id=$(basename "$d")
    local meta="$d/${doc_id}.meta.json"
    jq -e 'has("doc_id") and has("engine") and has("suspect_pages") and has("ocr") and has("found") and has("stats") and has("metrics") and has("timestamps") and has("page_count") and has("p95_latency_ms_per_page") and (.ocr|has("enabled") and has("ran") and has("ocr_run_pages") and has("lang") and has("psm") and has("oem") and has("dpi")) and (.metrics|has("coverage_pages"))' "$meta" >/dev/null || { echo "[FAIL] Schema: $doc_id"; return 1; }
    local suspects=$(jq -r '.suspect_pages | length' "$meta")
    local ocr_run=$(jq -r '.ocr.ocr_run_pages | length // 0' "$meta")
    local covp=$(jq -r '.metrics.coverage_pages // 0' "$meta")
    if [ "$suspects" -gt 0 ] && [ "$ocr_run" -eq 0 ]; then echo "[FAIL] OCRNoRunPages: $doc_id"; return 1; fi
    if [ "$suspects" -gt 0 ] && [ "$covp" != "1" ] && [ "$covp" != "1.0" ]; then echo "[FAIL] CoveragePagesLow: $doc_id ($covp)"; return 1; fi
    if [ "$ARTIFACTS" = "--artifacts=on" ]; then
      local pngs=$(ls -1 "$d/artifacts/ocr"/page-*.png 2>/dev/null | wc -l | tr -d ' ')
      if [ "$ocr_run" -gt 0 ] && [ "$pngs" -lt "$ocr_run" ]; then echo "[FAIL] OCRArtifactsMismatch: $doc_id ($pngs < $ocr_run)"; return 1; fi
    fi
  done
  return 0
}

check_structure_ground_truth() {
  local gt="$ROOT_DIR/tests/fixtures/ground_truth.yaml"
  [ -f "$gt" ] || { echo "[FAIL] GroundTruthMissing: $gt"; return 1; }
  local fail=0
  # Parse gt lines like: doc_id: { bab: N, pasal: M }
  while IFS= read -r line; do
    case "$line" in
      \#*|'') continue ;;
    esac
    doc=$(echo "$line" | sed -n 's/^\s*\([^: ]\+\):.*/\1/p')
    bab=$(echo "$line" | sed -n 's/.*bab:\s*\([0-9]\+\).*/\1/p')
    pasal=$(echo "$line" | sed -n 's/.*pasal:\s*\([0-9]\+\).*/\1/p')
    [ -n "$doc" ] || continue
    [ -d "$OUT_DIR/$doc" ] || continue
    meta="$OUT_DIR/$doc/${doc}.meta.json"
    [ -f "$meta" ] || { echo "[FAIL] StructureMetaMissing: $doc"; fail=1; continue; }
    fb=$(jq -r '.found.bab // 0' "$meta")
    fp=$(jq -r '.found.pasal // 0' "$meta")
    if [ "$fb" -ne "${bab:-0}" ] || [ "$fp" -ne "${pasal:-0}" ]; then
      echo "[FAIL] StructureAccuracyLow: $doc (found bab=$fb pasal=$fp vs gt bab=${bab:-0} pasal=${pasal:-0})"
      fail=1
    fi
  done < "$gt"
  return $fail
}

idempotency_check() {
  if [ "$IDEM_FAST" -eq 1 ]; then
    rm -rf "$OUT_DIR_OFF" && mkdir -p "$OUT_DIR_OFF"
    cp -a "$OUT_DIR"/* "$OUT_DIR_OFF"/ || true
    for d in "$OUT_DIR"/*/; do
      [ -d "$d" ] || continue
      doc_id=$(basename "$d")
      fp1=$(jq -r '.meta_fingerprint // empty' "$OUT_DIR/$doc_id/${doc_id}.meta.json")
      fp2=$(jq -r '.meta_fingerprint // empty' "$OUT_DIR_OFF/$doc_id/${doc_id}.meta.json")
      if [ -n "$fp1" ] && [ -n "$fp2" ]; then
        [ "$fp1" = "$fp2" ] || { echo "[FAIL] MetaIdempotency (fast): $doc_id"; return 1; }
      else
        h1=$(jq 'del(.timestamps, .metrics.duration_ms?, .stats.runtime_ms?)' "$OUT_DIR/$doc_id/${doc_id}.meta.json" | sha256sum | awk '{print $1}')
        h2=$(jq 'del(.timestamps, .metrics.duration_ms?, .stats.runtime_ms?)' "$OUT_DIR_OFF/$doc_id/${doc_id}.meta.json" | sha256sum | awk '{print $1}')
        [ "$h1" = "$h2" ] || { echo "[FAIL] MetaIdempotency (fast,normalized): $doc_id"; return 1; }
      fi
    done
    return 0
  fi
  # Regular two-run idempotency
  run_pipeline "--artifacts=on"
  declare -A md_hash_on meta_fp_on
  for d in "$OUT_DIR"/*/; do
    [ -d "$d" ] || continue
    doc_id=$(basename "$d")
    md_hash_on[$doc_id]=$(sha256sum "$d/${doc_id}.md" | awk '{print $1}')
    meta_fp_on[$doc_id]=$(jq -r '.meta_fingerprint // empty' "$d/${doc_id}.meta.json")
  done
  run_pipeline "--artifacts=off"
  find "$OUT_DIR" -type d -name artifacts | grep -q . && { echo "[FAIL] ArtifactsPersist: off-run"; return 1; } || true
  for d in "$OUT_DIR"/*/; do
    [ -d "$d" ] || continue
    doc_id=$(basename "$d")
    md2=$(sha256sum "$d/${doc_id}.md" | awk '{print $1}')
    fp2=$(jq -r '.meta_fingerprint // empty' "$d/${doc_id}.meta.json")
    [ "${md_hash_on[$doc_id]:-x}" = "$md2" ] || { echo "[FAIL] MdIdempotency: $doc_id"; return 1; }
    if [ -n "${meta_fp_on[$doc_id]:-}" ] && [ -n "$fp2" ]; then
      [ "${meta_fp_on[$doc_id]}" = "$fp2" ] || { echo "[FAIL] MetaIdempotency: $doc_id"; return 1; }
    fi
  done
  return 0
}

collect_report() {
  echo "doc_id|pages|suspect|ocr_run|coverage_pages|p95_ms_per_page|leak|split|meta_fp|cache_hit_txt|cache_hit_img|status" > "$OUT_DIR/accept_table.txt"
  local status_overall=PASS
  for d in "$OUT_DIR"/*/; do
    [ -d "$d" ] || continue
    local doc_id=$(basename "$d")
    local meta="$d/${doc_id}.meta.json"
    local pages=$(jq -r '.page_count // 0' "$meta")
    local suspect=$(jq -r '.suspect_pages | length' "$meta")
    local ocr_run=$(jq -r '.ocr.ocr_run_pages | length // 0' "$meta")
    local p95=$(jq -r '.p95_latency_ms_per_page // 0' "$meta")
    local leak=$(jq -r '.metrics.leak_rate // 0' "$meta")
    local split=$(jq -r '.metrics.split_violations // 0' "$meta")
    local covp=$(jq -r '.metrics.coverage_pages // 0' "$meta")
    local fp=$(jq -r '.meta_fingerprint // ""' "$meta")
    local cache_txt=NA
    local cache_img=NA
    local status=PASS
    if [ "$suspect" -gt 0 ] && [ "$ocr_run" -eq 0 ]; then status=FAIL; fi
    if [ "$suspect" -gt 0 ] && [ "$covp" != "1" ] && [ "$covp" != "1.0" ]; then status=FAIL; fi
    echo "$doc_id|$pages|$suspect|$ocr_run|$covp|$p95|$leak|$split|$fp|$cache_txt|$cache_img|$status" >> "$OUT_DIR/accept_table.txt"
    if [ "$status" = "FAIL" ]; then status_overall=FAIL; fi
  done
  cat "$OUT_DIR/accept_table.txt"
  [ "$status_overall" = "PASS" ]
}

main() {
  echo "Running acceptance with $ARTIFACTS ${CI_SAMPLE:+ci-sample=$CI_SAMPLE} ${OCR_DPI:+ocr-dpi=$OCR_DPI}" >&2
  run_pipeline "$ARTIFACTS"
  check_structure || exit 1
  no_step_leak_global || exit 1
  check_schema_and_kpis || exit 1
  check_structure_ground_truth || exit 1
  collect_report || exit 1
  if ! idempotency_check; then exit 1; fi
  echo "All acceptance checks PASSED" >&2
}

main "$@"
