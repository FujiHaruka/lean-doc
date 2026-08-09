#!/usr/bin/env bash
# The stage-5 extraction block: full package re-extraction versus re-extracting
# a single module.
#
# NOT interleaved, and the reason is a measurement, not a preference. The first
# attempt did round-robin (the rule the earlier legs used) and every series came
# out cold: the full run's import closure is 6,021 modules / 2.6 GB of olean
# plus 2.8 GB of anonymous memory, which evicts what the one-module runs just
# paged in. Six rounds later the one-module runs were still at 72,000 major
# faults. Interleaving is the right tool when the variants have comparable
# working sets (stage 4's tagged/untagged pair); here they differ by 3x and it
# only guarantees that nothing is ever warm.
#
# So each series runs consecutively to its own warm steady state, and the block
# ends with a re-take of the small series to measure how much the full run moved
# the level. That drift is reported rather than assumed away.
#
# usage: block-extract.sh [runs] [series...]
#   runs    repetitions per series (default 6; run 1 of each series is dropped)
#   series  which of hub / leaf / mid / full / hub-after / leaf-after to run.
#           Default: all of them, in that order. Split into separate invocations
#           when a single call would run longer than the caller can wait; the
#           order above is the one the numbers were taken in.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"
WORK="${WORK_DIR:?set WORK_DIR}"
RUNS="${1:-6}"
shift || true
WANTED=("$@")
[ ${#WANTED[@]} -gt 0 ] || WANTED=(hub leaf mid full hub-after leaf-after)
want () { for w in "${WANTED[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

RAW="$WORK/block-extract"; mkdir -p "$RAW" "$RESULTS_DIR"
LIST_DIR="$WORK/lists"; mkdir -p "$LIST_DIR"
cp "$RESULTS_DIR/it-modules.txt" "$LIST_DIR/full.txt"
# The three one-module series, chosen at the ends of the dependency graph
# (README "Choosing the module to change"):
echo "InformationTheory.Meta.EntryPoint" > "$LIST_DIR/hub.txt"
echo "InformationTheory.Shannon.Kolmogorov.OmegaNoncomputable" > "$LIST_DIR/leaf.txt"
echo "InformationTheory.Shannon.ChannelCoding.Basic" > "$LIST_DIR/mid.txt"

series () { # series <name> <list> <runs>
  local s="$1" list="$2" n="$3"
  rm -f "$RESULTS_DIR/stage5-extract-$s.jsonl"
  for i in $(seq 1 "$n"); do
    local tl="$RAW/time-$s-$i.txt" tj="$RAW/timings-$s-$i.json"
    printf '{}\n' > "$tj"
    python3 "$LD/experiments/stage4c/merge-timing.py" --name "stage5-extract-$s" --run "$i" \
      --time-l "$tl" --timings "$tj" --exec -- \
      "$HERE/extract-once.sh" --modules "$list" \
      --ir-dir "$WORK/ir-$s" --timings "$tj" --events "$RAW/events-$s-$i.jsonl" \
      >> "$RESULTS_DIR/stage5-extract-$s.jsonl"
    echo "  $s run $i done"
  done
}

# Small working sets first, the full package last: the cheap series get their
# warm steady state before the expensive one perturbs the machine.
if want hub;  then series hub  "$LIST_DIR/hub.txt"  "$RUNS"; fi
if want leaf; then series leaf "$LIST_DIR/leaf.txt" "$RUNS"; fi
if want mid;  then series mid  "$LIST_DIR/mid.txt"  "$RUNS"; fi
if want full; then series full "$LIST_DIR/full.txt" "$RUNS"; fi
# Drift check: the same one-module series again, after the full run.
if want hub-after;  then series hub-after  "$LIST_DIR/hub.txt"  3; fi
if want leaf-after; then series leaf-after "$LIST_DIR/leaf.txt" 3; fi

KEYS=importModules,analyze,writeIR,total,indexLookup,tactics,moduleDocs,envStats:loadedModules
for s in "${WANTED[@]}"; do
  [ -s "$RESULTS_DIR/stage5-extract-$s.jsonl" ] || continue
  list="$LIST_DIR/${s%-after}.txt"
  {
    echo "# stage5-extract-$s"
    echo
    echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
    echo "lean-toolchain    $(cat /Users/haruka/dev/lean-projects/lean-toolchain)"
    echo "modules           $(grep -c . "$list") from $list"
    echo "binary            $LD/experiments/stage4b/build/extract (unchanged from stage 4b)"
    echo "args              --equations --refs --write-ir --tagged-code"
    echo "order             consecutive series, small working sets first (see script header)"
    echo
    python3 "$HERE/summarize.py" "$RESULTS_DIR/stage5-extract-$s.jsonl" --keys "$KEYS"
  } > "$RESULTS_DIR/stage5-extract-$s.txt"
  echo "-> $RESULTS_DIR/stage5-extract-$s.txt"
done
