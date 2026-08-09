#!/usr/bin/env bash
# Full documentation build of the target repo: its own modules plus the whole
# Mathlib closure. Resumable — Lake skips modules whose marker file is up to
# date, and the timing log is appended to rather than truncated.
#
# Usage: run-full.sh [threads] [timing-log-basename]
#   threads  value for LEAN_NUM_THREADS (0 = leave unset). Lake has no -j flag.
set -u

. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

THREADS=${1:-0}
NAME=${2:-full-build}
TARGET_FACET="${TARGET_FACET:-InformationTheory:docs}"

cd "$TARGET_REPO"
export DOCGEN_TIMING="$RESULTS_DIR/$NAME.jsonl"
[ "$THREADS" != "0" ] && export LEAN_NUM_THREADS=$THREADS

SUMMARY="$RESULTS_DIR/$NAME-summary.txt"
{ echo "--- run started $(date +%s) threads=$THREADS facet=$TARGET_FACET ---"; df -k . | tail -1; } >> "$SUMMARY"
start=$(date +%s)
/usr/bin/time -l lake build "$TARGET_FACET" >> "$RESULTS_DIR/$NAME.log" 2>&1
rc=$?
end=$(date +%s)
{ echo "rc=$rc threads=$THREADS wall_seconds=$((end-start))"; df -k . | tail -1
  du -sk .lake/build/api-docs.db .lake/build/doc .lake/build/doc-data 2>/dev/null; } >> "$SUMMARY"
