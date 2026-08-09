#!/usr/bin/env bash
# The stage-5 end-to-end block: detect + extract + merge + render, timed as one
# process tree per run.
#
# Setup (outside every timed run, because it is the *state a repository is
# already in* when an edit happens, not part of the edit):
#   * a live IR, a copy of the full extraction's tree
#   * a live page tree, the 432 pages rendered from it
#   * a hash ledger over the 432 oleans, with the chosen module invalidated
#
# The live IR is updated **in place** by every run. That is safe here precisely
# because the injected change is a fake: the re-extracted module is byte-
# identical, so the merge is idempotent and run N sees the same state as run 1.
#
# usage: block-incremental.sh [runs] [variant...]
#   variants: leaf-self mid-referrers hub-importers full-rebuild
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"
WORK="${WORK_DIR:?set WORK_DIR}"
BASE_IR="${IR_DIR:?set IR_DIR to the IR tree of the full extraction}"
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
RUNS="${1:-6}"
shift || true
WANTED=("$@")
[ ${#WANTED[@]} -gt 0 ] || WANTED=(leaf-self mid-referrers hub-importers)

LEAF=InformationTheory.Shannon.Kolmogorov.OmegaNoncomputable
MID=InformationTheory.Shannon.ChannelCoding.Basic
HUB=InformationTheory.Meta.EntryPoint
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

RAW="$WORK/block-incremental"; mkdir -p "$RAW"

setup () { # setup <variant> <module>
  local v="$1" m="$2"
  local ir="$WORK/live-$v/ir" pages="$WORK/live-$v/pages"
  rm -rf "$WORK/live-$v"; mkdir -p "$WORK/live-$v"
  cp -R "$BASE_IR" "$ir"
  deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
    --ir "$ir" --pages "$pages" --source-url "$SOURCE_URL" > /dev/null
  deno run --allow-read --allow-write --allow-env "$HERE/ledger.ts" build \
    --modules "$RESULTS_DIR/it-modules.txt" --target "$TARGET" --ir "$ir" \
    --algorithm lake --out "$WORK/live-$v/ledger.json" > /dev/null
  deno run --allow-read --allow-write --allow-env "$HERE/ledger.ts" touch \
    --ledger "$WORK/live-$v/ledger.json" --module "$m" > /dev/null
}

variant () { # variant <name> <module> <mode> <runs>
  local v="$1" m="$2" mode="$3" n="$4"
  setup "$v" "$m"
  rm -f "$RESULTS_DIR/stage5-incremental-$v.jsonl"
  for i in $(seq 1 "$n"); do
    local tl="$RAW/time-$v-$i.txt" tj="$RAW/timings-$v-$i.json"
    printf '{}\n' > "$tj"
    python3 "$LD/experiments/stage4c/merge-timing.py" --name "stage5-incremental-$v" --run "$i" \
      --time-l "$tl" --timings "$tj" --exec -- \
      "$HERE/incremental.sh" --module "$m" --ir "$WORK/live-$v/ir" \
      --pages "$WORK/live-$v/pages" --ledger "$WORK/live-$v/ledger.json" \
      --work "$RAW/run-$v" --mode "$mode" --timings "$tj" \
      >> "$RESULTS_DIR/stage5-incremental-$v.jsonl"
    echo "  $v run $i done"
  done
}

want () { for w in "${WANTED[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }
if want leaf-self;     then variant leaf-self     "$LEAF" self      "$RUNS"; fi
if want mid-referrers; then variant mid-referrers "$MID"  referrers "$RUNS"; fi
if want hub-importers; then variant hub-importers "$HUB"  importers "$RUNS"; fi

KEYS=detectSeconds,extractSeconds,mergeSeconds,impactSeconds,renderSeconds,totalSeconds,changed,irChanged,pagesRendered
for v in "${WANTED[@]}"; do
  [ -s "$RESULTS_DIR/stage5-incremental-$v.jsonl" ] || continue
  {
    echo "# stage5-incremental-$v"
    echo
    echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
    echo "lean-toolchain    $(cat "$TARGET/lean-toolchain")"
    echo "pipeline          detect (ledger, lake hashes) -> extract (stage4b) -> merge -> render (stage4c)"
    echo "injected          the changed module's ledger entry, nothing else. No .lean edit, no lake build."
    echo "state             live IR + 432 rendered pages already on disk; the IR is updated in place"
    echo "runs              $RUNS (run 1 dropped)"
    echo
    python3 "$HERE/summarize.py" "$RESULTS_DIR/stage5-incremental-$v.jsonl" --keys "$KEYS"
  } > "$RESULTS_DIR/stage5-incremental-$v.txt"
  echo "-> $RESULTS_DIR/stage5-incremental-$v.txt"
done
