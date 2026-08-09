#!/usr/bin/env bash
# Baseline identity battery for experiments/stage4b: with `--tagged-code` off this
# binary must still be stage 4, and with it on it must only add.
#
# Re-run this whenever Extract.lean changes. It is what keeps stage 4b's numbers
# comparable with stages 2, 3 and 4 (see README, "Baseline identity").
#
#   1. --dump / --dump-modules / --dump-refs == stage 4, on a 10-module smoke
#      list, both with --refs and without
#   2. the IR (432 modules) == a stage-4 IR, modulo the `generator` string
#   3. --tagged-code is purely additive: the three dumps do not move
#   4. the tagged IR is deterministic
#
# Everything is written under WORK; nothing lands in benchmarks/results and
# nothing is written inside the measurement target.
#
# usage: baseline-identity.sh <work-dir> [<reference stage-4 IR dir>]
#   With no reference IR, stage 4 is re-run to produce one.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
WORK="${1:?usage: baseline-identity.sh <work-dir> [<reference stage-4 IR dir>]}"
REF_IR="${2:-}"

export RESULTS_DIR="$WORK/results"
mkdir -p "$RESULTS_DIR"

SMOKE="$WORK/smoke-modules.txt"
if [ ! -f "$SMOKE" ]; then
  # Stage 4's list, unchanged: 10 modules of the fixed target.
  cat > "$SMOKE" <<'EOF'
InformationTheory.Asymptotic
InformationTheory.Fano
InformationTheory.Probability.Mixture
InformationTheory.Probability.SingletonMass
InformationTheory.Probability.TwoSidedExtension
InformationTheory.Meta.EntryPoint
InformationTheory.Polymatroid.Basic
InformationTheory.Shannon.StrongTypicality
InformationTheory.Shannon.SufficientStatistic
InformationTheory.Shannon.NormalizedSinc
EOF
fi

smoke () {   # smoke <stage-dir> <name> [extra args...]
  local dir="$1" name="$2"; shift 2
  MODULES="$SMOKE" "$LD/experiments/$dir/run.sh" "$name" -- \
    --dump "$RESULTS_DIR/$name-dump.jsonl" \
    --dump-modules "$RESULTS_DIR/$name-mods.jsonl" \
    --dump-refs "$RESULTS_DIR/$name-refs.jsonl" \
    "$@" > "$RESULTS_DIR/$name.log" 2>&1
  echo "  ran $name"
}

full () {    # full <stage-dir> <name> <ir-dir> [extra args...]
  local dir="$1" name="$2" irdir="$3"; shift 3
  rm -rf "$irdir"
  MODULES="$LD/benchmarks/results/it-modules.txt" "$LD/experiments/$dir/run.sh" "$name" -- \
    --equations --refs --write-ir --ir-dir "$irdir" "$@" \
    > "$RESULTS_DIR/$name.log" 2>&1
  echo "  ran $name"
}

echo "== A. smoke: stage 4 vs stage 4b with --tagged-code off =="
smoke stage4  a1-s4  --equations --refs
smoke stage4b a1-s4b --equations --refs
smoke stage4  a2-s4  --equations
smoke stage4b a2-s4b --equations

echo "== B. smoke: stage 4b, --tagged-code on vs off =="
smoke stage4b b1-s4b --equations --refs --tagged-code
smoke stage4b b2-s4b --equations --tagged-code

echo "== C/D. full runs =="
if [ -z "$REF_IR" ]; then
  REF_IR="$WORK/ir-stage4"
  full stage4 c-ref "$REF_IR"
fi
full stage4b c-off  "$WORK/ir-off"
full stage4b d1-tag "$WORK/ir-tagged"  --tagged-code
full stage4b d2-tag "$WORK/ir-tagged2" --tagged-code

echo
echo "=== RESULTS ==="
rc=0
ck () { if cmp -s "$1" "$2"; then echo "PASS  $3"; else echo "FAIL  $3"; rc=1; fi; }

for k in dump mods refs; do
  ck "$RESULTS_DIR/a1-s4-$k.jsonl"  "$RESULTS_DIR/a1-s4b-$k.jsonl" "1. --refs on : $k  stage4 == stage4b(off)"
  ck "$RESULTS_DIR/a2-s4-$k.jsonl"  "$RESULTS_DIR/a2-s4b-$k.jsonl" "1. --refs off: $k  stage4 == stage4b(off)"
  ck "$RESULTS_DIR/a1-s4b-$k.jsonl" "$RESULTS_DIR/b1-s4b-$k.jsonl" "3. --refs on : $k  tagged == untagged"
  ck "$RESULTS_DIR/a2-s4b-$k.jsonl" "$RESULTS_DIR/b2-s4b-$k.jsonl" "3. --refs off: $k  tagged == untagged"
done

echo
diff -r "$REF_IR/modules" "$WORK/ir-off/modules" > "$RESULTS_DIR/c-modules.diff" 2>&1 \
  && echo "PASS  2. IR modules/ byte-identical to stage 4" || { echo "FAIL  2. IR modules/ (see c-modules.diff)"; rc=1; }
diff -r "$REF_IR/deps" "$WORK/ir-off/deps" > "$RESULTS_DIR/c-deps.diff" 2>&1 \
  && echo "PASS  2. IR deps/ byte-identical to stage 4" || { echo "FAIL  2. IR deps/ (see c-deps.diff)"; rc=1; }
# The one field that is allowed to differ: this binary names itself.
sed 's|lean-doc/experiments/stage4b|lean-doc/experiments/stage4|' "$WORK/ir-off/index.json" \
  > "$RESULTS_DIR/c-index-renamed.json"
ck "$REF_IR/index.json" "$RESULTS_DIR/c-index-renamed.json" "2. IR index.json (modulo the generator string)"

diff -r "$WORK/ir-tagged" "$WORK/ir-tagged2" > "$RESULTS_DIR/d.diff" 2>&1 \
  && echo "PASS  4. the tagged IR is deterministic" || { echo "FAIL  4. (see d.diff)"; rc=1; }

echo
echo "-- sizes --"
for d in "$REF_IR" "$WORK/ir-off" "$WORK/ir-tagged"; do
  printf '%-24s files=%s bytes=%s\n' "$(basename "$d")" \
    "$(find "$d" -type f | wc -l | tr -d ' ')" \
    "$(find "$d" -type f -exec wc -c {} + | tail -1 | awk '{print $1}')"
done

echo
deno run --allow-read "$HERE/check-spans.ts" --ir "$WORK/ir-tagged" || rc=1
exit $rc
