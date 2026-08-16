#!/usr/bin/env bash
# Measures how many olean bytes an extractor run pulls into the page cache.
#
# Takes a residency snapshot (mincore, see olean-residency.c) before any run, then
# alternates "run the extractor" / "take a snapshot". Snapshot 1 minus snapshot 0 is
# the cold run's page-in volume; later snapshots show what the warm runs add.
#
# With EVICT=1 the olean pages are dropped first (olean-evict.c), so the cold state is
# produced on demand and *verified* by R0 rather than assumed from "first run of the
# session". Without it, only the very first extractor run after a long idle is cold.
#
# THE WORKLOAD IS NO LONGER BUILT IN
#   Until 2026-08-16 the E-steps ran `experiments/stage1/run.sh <name>` — the stage-1
#   extractor over the fixed target. That is what produced every committed
#   `ci-residency-*` number. `experiments/` was removed; stage 1 now only exists at tag
#   `experiments-frozen`, and **HEAD has no equivalent**, so those numbers cannot be
#   reproduced from this tree. `RUN_CMD` therefore has no default: whoever measures has
#   to name the workload, and a run driven by any other workload is a NEW baseline, not
#   a continuation of the recorded one.
#
# usage: RUN_CMD='<cmd>' measure-residency.sh <paths.txt> <snapdir> [runs]
#          runs defaults to 6. RUN_CMD is invoked once per E-step as `$RUN_CMD <name>`,
#          where <name> is "$NAME_PREFIX$i" (the recorded runs used NAME_PREFIX to name
#          their JSONL under benchmarks/results/).
#        EVICT=1 ...                                            evict before R0
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$HERE/env.sh"

PATHS="${1:?usage: measure-residency.sh <paths.txt> <snapdir> [runs]}"
SNAPDIR="${2:?usage: measure-residency.sh <paths.txt> <snapdir> [runs]}"
RUNS="${3:-6}"
RESIDENCY="$HERE/olean-residency"
NAME_PREFIX="${NAME_PREFIX:-ci-residency-e}"
RUN_CMD="${RUN_CMD:-}"

[ -n "$RUN_CMD" ] || { echo "RUN_CMD is unset: name the extractor run to measure." >&2
  echo "The recorded ci-residency-* numbers used experiments/stage1/run.sh, which" >&2
  echo "only exists at tag experiments-frozen; HEAD has no equivalent." >&2; exit 2; }
[ -x "$RESIDENCY" ] || { echo "not built: cc -O2 -o $RESIDENCY $RESIDENCY.c" >&2; exit 1; }
[ -f "$PATHS" ] || { echo "path list not found: $PATHS" >&2; exit 1; }
mkdir -p "$SNAPDIR"

snapshot() {
  local n="$1"
  date -u +%Y-%m-%dT%H:%M:%SZ > "$SNAPDIR/R$n.meta"
  vm_stat > "$SNAPDIR/R$n.vmstat"
  "$RESIDENCY" < "$PATHS" > "$SNAPDIR/R$n.jsonl" 2> "$SNAPDIR/R$n.stderr"
  echo "R$n: $(wc -l < "$SNAPDIR/R$n.jsonl") records"
}

if [ "${EVICT:-0}" = "1" ]; then
  echo "== evict =="
  "$HERE/olean-evict" < "$PATHS"
fi

echo "== R0 (baseline, before any extractor run) =="
snapshot 0

# NB: `seq 1 0` counts *down* on BSD seq and would run an extra extractor pass.
for ((i = 1; i <= RUNS; i++)); do
  echo "== E$i =="
  # shellcheck disable=SC2086  # RUN_CMD is a command line, deliberately split
  $RUN_CMD "$NAME_PREFIX$i"
  echo "== R$i =="
  snapshot "$i"
done

echo "snapshots -> $SNAPDIR"
