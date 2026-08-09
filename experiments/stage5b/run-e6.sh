#!/usr/bin/env bash
# leg 8 / judgement point 3, E6: the *ownership* channel
# (`experiments/stage5/README.md` §4.2).
#
# The IR stores every reference as a `(defining module, name)` pair. Move a
# declaration D from M to M' and the `refs` of every module N that names D point
# at the wrong module — while N's own source, and therefore (assumption, see the
# summary) N's olean, is unchanged.
#
# No Lean is started: the target cannot be edited, so the move is injected into a
# copy of the IR (`move-decl.py`). Two trees come out of one move:
#
#   ir-moved-<v>        D moved, refs left stale  -> what the pipeline publishes
#                       when only M and M' are re-extracted
#   ir-moved-<v>-fixed  D moved, refs rewritten   -> what a full re-extraction
#                       would have produced (the ground truth)
#
# Two destinations M', by a rule fixed in advance: **the module with the fewest
# declarations (>= 1), ties broken by name ascending**, among
#   down  the modules that transitively import M and do not reference D
#   far   the modules that do not transitively import M and do not reference D
#
# usage:
#   run-e6.sh --ir <schema-2 base ir> --work <scratch> [--out <results>]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RENDER="$LD/experiments/stage4c/render.ts"
IMPACT="$LD/experiments/stage5/impact.ts"
REV=573793b243fb1343636088eb62d1789ab2b14cec
URL="https://github.com/FujiHaruka/information-theory/blob/$REV"

IR=""; WORK=""; OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --ir) IR="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$IR" ] && [ -n "$WORK" ] || { echo "usage: run-e6.sh --ir <dir> --work <dir>" >&2; exit 2; }
OUT="${OUT:-$WORK}"; mkdir -p "$WORK/e6" "$OUT"
W="$WORK/e6"

echo "########## E6 step 0 — pick D, M and the two M' ##########"
python3 "$HERE/pick-move.py" --ir "$IR" --json "$W/pick.json" | tee "$W/pick.txt"
D=$(python3 -c "import json;print(json.load(open('$W/pick.json'))['decl'])")
M=$(python3 -c "import json;print(json.load(open('$W/pick.json'))['from'])")
DOWN=$(python3 -c "import json;print(json.load(open('$W/pick.json'))['down'])")
FAR=$(python3 -c "import json;print(json.load(open('$W/pick.json'))['far'])")

echo "########## E6 step 1 — inject the move ##########"
for v in down far; do
  case $v in down) MP="$DOWN" ;; far) MP="$FAR" ;; esac
  python3 "$HERE/move-decl.py" --base "$IR" --out "$WORK/ir-moved-$v" \
    --fixed-out "$WORK/ir-moved-$v-fixed" --decl "$D" --to "$MP" --json "$W/move-$v.json"
done

echo "########## E6 step 2 — render ##########"
render () { # render <ir> <pages>
  if [ -d "$2" ]; then return; fi
  deno run --allow-read --allow-write "$RENDER" --ir "$1" --pages "$2" --source-url "$URL" \
    > "$2.log"
}
render "$IR" "$W/pages-base"
for v in down far; do
  render "$WORK/ir-moved-$v" "$W/pages-moved-$v"
  render "$WORK/ir-moved-$v-fixed" "$W/pages-moved-$v-fixed"
done

echo "########## E6 step 3 — score the impact sets ##########"
{
for v in down far; do
  case $v in down) MP="$DOWN" ;; far) MP="$FAR" ;; esac
  for mode in self referrers importers; do
    deno run --allow-read --allow-write "$IMPACT" --ir "$IR" --changed "$M" --changed "$MP" \
      --mode "$mode" --print-set "$W/set-$v-$mode.txt" > /dev/null
  done
  echo "=== variant $v : M' = $MP ==="
  echo "-- (1) ground truth: base vs a full re-extraction (moved-$v-fixed) --"
  python3 "$HERE/compare-pages.py" --mode impact --a "$W/pages-base" --b "$W/pages-moved-$v-fixed" \
    --truth-out "$W/truth-$v.txt" --stale-out-prefix "$W/stale-$v-" \
    --sets self="$W/set-$v-self.txt" referrers="$W/set-$v-referrers.txt" \
           importers="$W/set-$v-importers.txt"
  echo "-- (2) what the pipeline publishes (moved-$v, refs stale) vs the truth --"
  python3 "$HERE/compare-pages.py" --mode impact --a "$W/pages-moved-$v" \
    --b "$W/pages-moved-$v-fixed" --truth-out "$W/wrong-$v.txt" \
    --sets self="$W/set-$v-self.txt" referrers="$W/set-$v-referrers.txt" \
           importers="$W/set-$v-importers.txt"
  echo "-- (3) base vs moved-$v: what a byte comparison of the published site sees --"
  python3 "$HERE/compare-pages.py" --mode impact --a "$W/pages-base" --b "$W/pages-moved-$v" \
    --truth-out "$W/naive-$v.txt" --sets self="$W/set-$v-self.txt"
  echo
done
} | tee "$W/impact.txt"

echo "########## E6 step 4 — where does a referring page's link point? ##########"
python3 "$HERE/link-target.py" --decl "$D" --from-module "$M" \
  --trees base="$W/pages-base" stale="$W/pages-moved-down" fixed="$W/pages-moved-down-fixed" \
  --referrer-file "$W/stale-down-self.txt" | tee "$W/links.txt"
