#!/usr/bin/env bash
# Measure `lake build` wall clock / CPU / page faults in the target clone.
#
# Usage: measure-lake-build.sh <clone-dir> <label> <runs> <out-dir>
#
# Each run appends a /usr/bin/time -l block and the lake summary line so that
# "up-to-date vs rebuilt" is visible next to every timing.
set -euo pipefail

CLONE=${1:?clone dir}
LABEL=${2:?label}
RUNS=${3:-5}
OUT=${4:?out dir}

mkdir -p "$OUT"
LOG="$OUT/lake-build-$LABEL.log"
: >"$LOG"

cd "$CLONE"
for i in $(seq 1 "$RUNS"); do
  echo "=== run $i ===" >>"$LOG"
  /usr/bin/time -l lake build >"$OUT/.stdout.$$" 2>>"$LOG" || echo "EXIT=$?" >>"$LOG"
  # keep only the job summary, not the thousands of linter warnings
  grep -E '^(All targets up-to-date|Build completed|error)' "$OUT/.stdout.$$" | tail -3 >>"$LOG" || true
  tail -1 "$OUT/.stdout.$$" >>"$LOG"
  echo >>"$LOG"
done
rm -f "$OUT/.stdout.$$"
echo "wrote $LOG"
