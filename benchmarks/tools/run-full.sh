#!/bin/bash
# Full documentation build: InformationTheory + its whole Mathlib closure.
# Resumable — Lake skips modules whose marker file is up to date; the timing log is appended to.
# Usage: run-full.sh [threads] [timing-log-basename]
set -u
cd /Users/haruka/dev/lean-projects
THREADS=${1:-0}
NAME=${2:-full-build}
BENCH=$PWD/docs/doc-gen-bench/raw
export DOCGEN_TIMING=$BENCH/$NAME.jsonl
[ "$THREADS" != "0" ] && export LEAN_NUM_THREADS=$THREADS
{ echo "--- run started $(date +%s) threads=$THREADS log=$NAME ---"; df -k . | tail -1; } >> "$BENCH/full-build-summary.txt"
start=$(date +%s)
/usr/bin/time -l lake build InformationTheory:docs >> "$BENCH/$NAME.log" 2>&1
rc=$?
end=$(date +%s)
{ echo "rc=$rc threads=$THREADS wall_seconds=$((end-start))"; df -k . | tail -1
  du -sk .lake/build/api-docs.db .lake/build/doc .lake/build/doc-data 2>/dev/null; } >> "$BENCH/full-build-summary.txt"
