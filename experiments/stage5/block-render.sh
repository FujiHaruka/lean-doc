#!/usr/bin/env bash
# The stage-5 rendering block: re-rendering the whole site versus re-rendering
# only the pages a one-module change can reach.
#
# The generator is `experiments/stage4c/render.ts`, unchanged — it already takes
# a repeatable `--only <Module>`. The page *sets* come from `impact.ts`, which
# derives them from the IR's import and reference graphs.
#
# Interleaved, same reasoning as block-extract.sh.
#
# usage: block-render.sh [rounds]      (default 6; run 1 of each series dropped)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"
WORK="${WORK_DIR:?set WORK_DIR}"
IR="${IR_DIR:?set IR_DIR}"
ROUNDS="${1:-6}"
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

RAW="$WORK/block-render"; mkdir -p "$RAW" "$RESULTS_DIR"
SETS="$WORK/sets"; mkdir -p "$SETS"

LEAF=InformationTheory.Shannon.Kolmogorov.OmegaNoncomputable
HUB=InformationTheory.Meta.EntryPoint
MID=InformationTheory.Shannon.ChannelCoding.Basic

impact () { # impact <name> <module> <mode>
  deno run --allow-read --allow-write "$HERE/impact.ts" --ir "$IR" \
    --changed "$2" --mode "$3" --print-set "$SETS/$1.txt" --json "$SETS/$1.json" > /dev/null
}
impact leaf-self "$LEAF" self
impact leaf-importers "$LEAF" importers
impact mid-referrers "$MID" referrers
impact mid-importers "$MID" importers
impact hub-importers "$HUB" importers
for f in "$SETS"/*.txt; do echo "  $(basename "$f" .txt): $(grep -c . "$f") pages"; done

# `--only` is repeatable; build the argument vector from the set file.
only_args () { # only_args <set file>
  local out=()
  while read -r m; do [ -n "$m" ] && out+=(--only "$m"); done < "$1"
  printf '%s\n' "${out[@]}"
}

SERIES=(full leaf-self leaf-importers mid-referrers mid-importers hub-importers)
for s in "${SERIES[@]}"; do rm -f "$RESULTS_DIR/stage5-render-$s.jsonl"; done

for i in $(seq 1 "$ROUNDS"); do
  for s in "${SERIES[@]}"; do
    tl="$RAW/time-$s-$i.txt"; tj="$RAW/timings-$s-$i.json"
    printf '{}\n' > "$tj"
    ONLY=()
    if [ "$s" != "full" ]; then
      while read -r m; do [ -n "$m" ] && ONLY+=(--only "$m"); done < "$SETS/$s.txt"
    fi
    python3 "$LD/experiments/stage4c/merge-timing.py" --name "stage5-render-$s" --run "$i" \
      --time-l "$tl" --timings "$tj" --exec -- \
      "$(command -v deno)" run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
      --ir "$IR" --pages "$WORK/pages-$s" --source-url "$SOURCE_URL" \
      --timings "$tj" ${ONLY[@]+"${ONLY[@]}"} \
      >> "$RESULTS_DIR/stage5-render-$s.jsonl"
  done
  echo "  round $i done"
done

KEYS=seconds.readIr,seconds.indexBuild,seconds.renderHeaders,seconds.renderPage,seconds.write,seconds.total
for s in "${SERIES[@]}"; do
  {
    echo "# stage5-render-$s"
    echo
    echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
    echo "deno              $(deno --version | head -1)"
    echo "ir                $IR"
    echo "pages             $([ "$s" = full ] && echo 432 || grep -c . "$SETS/$s.txt") of 432"
    echo "interleave        round-robin with ${SERIES[*]}, $ROUNDS rounds"
    echo
    python3 "$HERE/summarize.py" "$RESULTS_DIR/stage5-render-$s.jsonl" --keys "$KEYS"
  } > "$RESULTS_DIR/stage5-render-$s.txt"
  echo "-> $RESULTS_DIR/stage5-render-$s.txt"
done
