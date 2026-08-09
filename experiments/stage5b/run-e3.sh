#!/usr/bin/env bash
# leg 8 / judgement point 3, E3 (proposition S3): does re-extracting one module
# give the same IR as a full extraction, under *every* supported extractor
# configuration?
#
# The configuration that is not the default is `--open <ns>`, which the stage-4b
# extractor has for the scoped-notation probe. This script runs, in order:
#
#   1. a full extraction with `--open`                    (Lean, ~15-45 s)
#   2. the byte diff against the `--open`-off base tree, split by cause
#   3. the two single-module re-extractions the rule in `open-diff.py` picks
#      (Lean, ~4-20 s each) plus one `--open`-off control
#   4. the byte comparison that decides S3
#   5. the page-level effect (two full renders + a byte diff)
#
# Nothing under /Users/haruka/dev/lean-projects is written: `lake env` only
# borrows the environment, and every output path is under --work.
#
# usage:
#   run-e3.sh --ir <schema-2 base ir, --open OFF> --work <scratch> [--out <results>]
#
# Existing IR trees under --work are reused, so a re-run costs no Lean time.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RENDER="$LD/experiments/stage4c/render.ts"
MODLIST="$LD/benchmarks/results/it-modules.txt"
TARGET=/Users/haruka/dev/lean-projects
# The two namespaces the package's scoped notation lives in. Not module names:
# `notation-reach.py` prints where they come from.
OPENNS="InformationTheory.Shannon,InformationTheory.Asymptotic"
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
[ -n "$IR" ] && [ -n "$WORK" ] || { echo "usage: run-e3.sh --ir <dir> --work <dir>" >&2; exit 2; }
OUT="${OUT:-$WORK}"; mkdir -p "$WORK/e3" "$OUT"
W="$WORK/e3"

timed () { # timed <label> <ir-dir> <module list> [open-namespaces]
  local label="$1" irdir="$2" list="$3" ns="${4:-}"
  if [ -d "$irdir" ]; then echo "  ($label: reusing $irdir)"; return; fi
  local args=(--modules "$list" --ir-dir "$irdir" --timings "$W/$label.json"
              --events "$W/$label-events.jsonl")
  if [ -n "$ns" ]; then args+=(--open "$ns"); fi
  echo "  $label:"
  /usr/bin/time -l "$HERE/extract-open.sh" "${args[@]}" 2>&1 \
    | grep -E 'real|maximum resident set size|page faults' | sed 's/^/    /'
}

echo "########## E3 step 1 — full extraction with --open ##########"
cp "$MODLIST" "$W/full.txt"
timed full-open "$WORK/ir-open-full" "$W/full.txt" "$OPENNS"

echo "########## E3 step 2 — the reach of the printing channel, on the IR graphs ##########"
python3 "$HERE/notation-reach.py" --ir "$IR" --sources "$TARGET" --json "$W/notation-reach.json" \
  | tee "$W/notation-reach.txt" | head -40

echo "########## E3 step 3 — which modules changed, and why ##########"
python3 "$HERE/open-diff.py" classify --base "$IR" --open "$WORK/ir-open-full" \
  --reach "$W/notation-reach.json" --out-dir "$W" | tee "$W/classify.txt"

A=$(python3 -c "import json;print(json.load(open('$W/picks.json'))['notation'])")
B=$(python3 -c "import json;print(json.load(open('$W/picks.json'))['shorten'])")
echo "$A" > "$W/one-A.txt"; echo "$B" > "$W/one-B.txt"

echo "########## E3 step 4 — single-module re-extraction ##########"
timed one-A-open   "$WORK/ir-one-A-open"   "$W/one-A.txt" "$OPENNS"
timed one-B-open   "$WORK/ir-one-B-open"   "$W/one-B.txt" "$OPENNS"
timed one-A-noopen "$WORK/ir-one-A-noopen" "$W/one-A.txt"

python3 "$HERE/open-diff.py" bytecmp \
  --pair "A --open: single vs full --open=$WORK/ir-one-A-open/modules/$A.json:$WORK/ir-open-full/modules/$A.json" \
  --pair "A --open: single vs base (--open off)=$WORK/ir-one-A-open/modules/$A.json:$IR/modules/$A.json" \
  --pair "B --open: single vs full --open=$WORK/ir-one-B-open/modules/$B.json:$WORK/ir-open-full/modules/$B.json" \
  --pair "B --open: single vs base (--open off)=$WORK/ir-one-B-open/modules/$B.json:$IR/modules/$B.json" \
  --pair "A --open off: single vs base=$WORK/ir-one-A-noopen/modules/$A.json:$IR/modules/$A.json" \
  | tee "$W/bytecmp.txt"

echo "########## E3 step 5 — the page-level effect ##########"
for v in base open-full; do
  case $v in base) D="$IR" ;; open-full) D="$WORK/ir-open-full" ;; esac
  if [ -d "$W/pages-$v" ]; then continue; fi
  deno run --allow-read --allow-write "$RENDER" --ir "$D" --pages "$W/pages-$v" \
    --source-url "$URL" > "$W/render-$v.log"
done
python3 "$HERE/compare-pages.py" --mode impact --a "$W/pages-base" --b "$W/pages-open-full" \
  --truth-out "$W/pages-changed.txt" \
  --sets irchanged="$W/changed-modules.txt" notation="$W/cls-notation.txt" \
  | tee "$W/pages.txt"

echo "picked A (notation class) = $A"
echo "picked B (shorten class)  = $B"
