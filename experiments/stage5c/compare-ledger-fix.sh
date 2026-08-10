#!/usr/bin/env bash
# Did closing the three ledger holes move the numbers?
#
# The rule this script exists to obey: a before/after comparison is only valid
# inside one session at one warm state. So it measures BEFORE, then AFTER, then
# BEFORE again -- the second BEFORE is the drift check. If the two BEFORE blocks
# disagree by more than the BEFORE/AFTER gap, the comparison says nothing.
#
# BEFORE is the parent commit of the fix, checked out into the working tree for
# the duration of its block and restored afterwards.
#
# usage: compare-fix.sh <work-dir> <fix-commit> [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
W=${1:?work dir}
FIX=${2:?commit that contains the fix}
RUNS=${3:-6}

block () { # block <label> <results-dir>
  local label="$1" res="$2"
  mkdir -p "$res"
  cp "$LD/benchmarks/results/it-modules.txt" "$res/"
  echo "=== block $label ==="
  RESULTS_DIR="$res" WORK_DIR="$W/work-$label" IR_DIR="$W/base-ir" \
    "$LD/experiments/stage5/block-incremental.sh" "$RUNS" leaf-self mid-referrers hub-importers \
    >"$W/$label.log" 2>&1 || { echo "block $label failed; see $W/$label.log" >&2; return 1; }
}

restore () { git -C "$LD" checkout "$FIX" -- experiments/stage5/; }
trap restore EXIT

git -C "$LD" checkout "$FIX~1" -- experiments/stage5/
block before1 "$W/res-before1"
restore
block after "$W/res-after"
git -C "$LD" checkout "$FIX~1" -- experiments/stage5/
block before2 "$W/res-before2"
restore
echo "done"
