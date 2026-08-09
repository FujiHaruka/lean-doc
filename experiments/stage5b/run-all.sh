#!/usr/bin/env bash
# leg 8 / judgement point 3: E1, E2, E4, E5 — the four experiments that need no
# Lean. One run, all raw output under --work, the numbered logs named after the
# experiment. E3 (the `--open` extraction) is not here: it starts Lean.
#
# NOTHING under /Users/haruka/dev/lean-projects is written. The olean tree E4
# needs to mutate is a tree of symlinks (`fake-target.py`).
#
# usage:
#   run-all.sh --ir <base ir> --work <scratch dir> [--out <results dir>]
#              [--target /Users/haruka/dev/lean-projects]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RENDER="$LD/experiments/stage4c/render.ts"
LEDGER="$LD/experiments/stage5/ledger.ts"
IMPACT="$LD/experiments/stage5/impact.ts"
MODLIST="$LD/benchmarks/results/it-modules.txt"

# The revision `incremental.sh` hardcodes, and a 40-hex stand-in for "the same
# tree, committed under a different revision".
REV_A="573793b243fb1343636088eb62d1789ab2b14cec"
REV_B="deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
URL_A="https://github.com/FujiHaruka/information-theory/blob/$REV_A"
URL_B="https://github.com/FujiHaruka/information-theory/blob/$REV_B"

# E2's injection. `log` is the top row of the candidate census (§A of
# autolink-analysis.py): the identifier-shaped docstring token that currently
# fails to autolink on the most pages. Picking the maximum is a rule, not a
# choice — see README §3.
INJECT_NAME=log
LEAF=InformationTheory.Shannon.Kolmogorov.OmegaNoncomputable   # 1 transitive importer
HUB=InformationTheory.Meta.EntryPoint                          # 414 transitive importers
DELETED=InformationTheory.Fano

IR=""; WORK=""; OUT=""; TARGET=/Users/haruka/dev/lean-projects
while [ $# -gt 0 ]; do
  case "$1" in
    --ir) IR="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$IR" ] && [ -n "$WORK" ] || { echo "usage: run-all.sh --ir <dir> --work <dir>" >&2; exit 2; }
mkdir -p "$WORK"
OUT="${OUT:-$WORK}"
mkdir -p "$OUT"

deno_render () { deno run --allow-read --allow-write "$@"; }

echo "########## E1 — S1: does the ledger see every input that changes a page? ##########"
for v in a b; do
  case $v in a) U=$URL_A ;; b) U=$URL_B ;; esac
  rm -rf "$WORK/pages-rev-$v"
  deno_render "$RENDER" --ir "$IR" --pages "$WORK/pages-rev-$v" --source-url "$U" \
    > "$WORK/e1-render-$v.log"
done
python3 "$HERE/compare-pages.py" --mode count --a "$WORK/pages-rev-a" --b "$WORK/pages-rev-b" \
  --needle "$REV_A" --needle-b "$REV_B" > "$OUT/stage5b-e1-pages.txt"
head -6 "$OUT/stage5b-e1-pages.txt"

deno run --allow-read --allow-write --allow-env "$LEDGER" build --modules "$MODLIST" \
  --target "$TARGET" --out "$WORK/e1-ledger.json" --ir "$IR" --algorithm lake --concurrency 8 \
  | tee "$OUT/stage5b-e1-ledger.txt"
deno run --allow-read --allow-write --allow-env "$LEDGER" check --ledger "$WORK/e1-ledger.json" \
  --ir "$IR" --changed-out "$WORK/e1-changed.txt" --timings "$OUT/stage5b-e1-check.json" \
  --concurrency 8 | tee -a "$OUT/stage5b-e1-ledger.txt"

# The envKey half: fake the *recorded* key (the ledger is ours, in scratch) so
# that `check` sees a toolchain / manifest / IR-schema change.
python3 - "$WORK/e1-ledger.json" "$WORK/e1-ledger-envfake.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
d["envKey"]["leanToolchain"] = "leanprover/lean4:v4.99.0-fake"
d["envKey"]["manifestSha256"] = "0" * 64
d["envKey"]["irSchemaVersion"] = "1"
json.dump(d, open(sys.argv[2], "w", encoding="utf-8"))
PY
deno run --allow-read --allow-write --allow-env "$LEDGER" check \
  --ledger "$WORK/e1-ledger-envfake.json" --ir "$IR" --changed-out "$WORK/e1-changed-envfake.txt" \
  --timings "$OUT/stage5b-e1-check-envfake.json" --concurrency 8 | tee -a "$OUT/stage5b-e1-ledger.txt"
echo "changed-out lines after the envKey change: $(grep -c . "$WORK/e1-changed-envfake.txt" || true)" \
  | tee -a "$OUT/stage5b-e1-ledger.txt"

echo "########## E2 — S2: does the impact set contain every page that changes? ##########"
python3 "$HERE/instrument-render.py" --out "$WORK/render-instr.ts"
deno_render "$WORK/render-instr.ts" --ir "$IR" --pages "$WORK/pages-instr" --source-url "$URL_A" \
  --autolink-dump "$WORK/e2-unresolved.jsonl" > "$WORK/e2-instr.log"
echo "instrumented copy vs pristine render:"
diff -r -q "$WORK/pages-rev-a" "$WORK/pages-instr" && echo "  432 pages byte-identical"
grep -E "autolinkAttempts|autolinkResolved" "$WORK/e1-render-a.log" "$WORK/e2-instr.log"

for v in leaf hub; do
  case $v in leaf) M=$LEAF ;; hub) M=$HUB ;; esac
  python3 "$HERE/inject-decl.py" --base "$IR" --out "$WORK/ir-after-$v" --module "$M" \
    --name "$INJECT_NAME"
  rm -rf "$WORK/pages-after-$v"
  deno_render "$RENDER" --ir "$WORK/ir-after-$v" --pages "$WORK/pages-after-$v" \
    --source-url "$URL_A" > "$WORK/e2-render-after-$v.log"
  for mode in self referrers importers; do
    deno run --allow-read --allow-write "$IMPACT" --ir "$IR" --changed "$M" --mode "$mode" \
      --print-set "$WORK/e2-set-$v-$mode.txt" > /dev/null
  done
done
{
  for v in leaf hub; do
    case $v in leaf) M=$LEAF ;; hub) M=$HUB ;; esac
    echo "=== N = $M ($v), injected declaration \`$INJECT_NAME\` ==="
    python3 "$HERE/compare-pages.py" --mode impact --a "$WORK/pages-rev-a" \
      --b "$WORK/pages-after-$v" --truth-out "$WORK/e2-truth-$v.txt" \
      --stale-out-prefix "$WORK/e2-stale-$v-" \
      --sets self="$WORK/e2-set-$v-self.txt" referrers="$WORK/e2-set-$v-referrers.txt" \
             importers="$WORK/e2-set-$v-importers.txt"
    echo
  done
} > "$OUT/stage5b-e2-stale.txt"
cat "$OUT/stage5b-e2-stale.txt"

python3 "$HERE/autolink-analysis.py" --ir "$IR" --dump "$WORK/e2-unresolved.jsonl" \
  --token "$INJECT_NAME" \
  --validate "leaf=$LEAF=$WORK/e2-truth-leaf.txt" "hub=$HUB=$WORK/e2-truth-hub.txt" \
  > "$OUT/stage5b-e2-autolink.txt"
tail -25 "$OUT/stage5b-e2-autolink.txt"

# One stale page in full, for the write-up: a page that is stale even under the
# sound bound `--mode importers` of the hub.
{
  for p in $(head -3 "$WORK/e2-stale-hub-importers.txt"); do
    python3 "$HERE/compare-pages.py" --mode excerpt --a "$WORK/pages-rev-a" \
      --b "$WORK/pages-after-hub" --page "$p" --hunks 2
    echo
  done
} > "$OUT/stage5b-e2-excerpt.txt"

echo "########## E4 — S4: does a module that disappears leave anything behind? ##########"
{
  python3 "$HERE/fake-target.py" --target "$TARGET" --out "$WORK/fake-target" --modules "$MODLIST"
  deno run --allow-read --allow-write --allow-env "$LEDGER" build --modules "$MODLIST" \
    --target "$WORK/fake-target" --out "$WORK/e4-ledger.json" --ir "$IR" \
    --algorithm lake --concurrency 8
  echo "-- baseline check"
  deno run --allow-read --allow-write --allow-env "$LEDGER" check --ledger "$WORK/e4-ledger.json" \
    --ir "$IR" --concurrency 8
  echo
  echo "-- one module's olean removed"
  python3 "$HERE/fake-target.py" --out "$WORK/fake-target" --drop "$DELETED"
  set +e
  deno run --allow-read --allow-write --allow-env "$LEDGER" check --ledger "$WORK/e4-ledger.json" \
    --ir "$IR" --changed-out "$WORK/e4-changed-deleted.txt" --concurrency 8 2>&1
  echo "exit=$?"
  set -e
  echo
  echo "-- restored, then one brand-new module added on disk"
  python3 "$HERE/fake-target.py" --target "$TARGET" --out "$WORK/fake-target" \
    --add "$DELETED" --copy-of "$DELETED"
  python3 "$HERE/fake-target.py" --target "$TARGET" --out "$WORK/fake-target" \
    --add InformationTheory.NewlyAdded --copy-of "$DELETED"
  echo "oleans on disk: $(find "$WORK/fake-target/.lake/build/lib/lean" -name '*.olean' | wc -l | tr -d ' ')"
  deno run --allow-read --allow-write --allow-env "$LEDGER" check --ledger "$WORK/e4-ledger.json" \
    --ir "$IR" --changed-out "$WORK/e4-changed-added.txt" --concurrency 8
  echo "changed-out lines: $(grep -c . "$WORK/e4-changed-added.txt" || true)"
  echo
  echo "-- delete paths in the pipeline programs"
  rg -n "Deno.remove|rmdir|unlink" "$LD/experiments/stage5"/*.ts "$RENDER" || echo "(none)"
  echo
  echo "-- the page side: drop the module from the IR and re-render into the live tree"
  python3 "$HERE/drop-module.py" --base "$IR" --out "$WORK/ir-minus" --module "$DELETED"
  rm -rf "$WORK/pages-live"; cp -R "$WORK/pages-rev-a" "$WORK/pages-live"
  deno_render "$RENDER" --ir "$WORK/ir-minus" --pages "$WORK/pages-live" --source-url "$URL_A" \
    | grep -E "modulesRead|pagesWritten"
  echo "pages on disk after the re-render: $(find "$WORK/pages-live" -name '*.html' | wc -l | tr -d ' ')"
  P="$WORK/pages-live/$(echo "$DELETED" | tr '.' '/').html"
  if [ -f "$P" ]; then
    echo "the deleted module's page: STILL PRESENT ($(wc -c < "$P" | tr -d ' ') B), byte-identical to before: \
$(cmp -s "$P" "$WORK/pages-rev-a/$(echo "$DELETED" | tr '.' '/').html" && echo yes || echo no)"
  else
    echo "the deleted module's page: gone"
  fi
  echo "live pages still linking to it: $(grep -rl "$(basename "$DELETED").html" "$WORK/pages-live" \
    | grep -v "/$(basename "$DELETED").html" | wc -l | tr -d ' ')"
  python3 "$HERE/compare-pages.py" --mode impact --a "$WORK/pages-rev-a" --b "$WORK/pages-live" \
    | head -1
} > "$OUT/stage5b-e4-deletion.txt" 2>&1
cat "$OUT/stage5b-e4-deletion.txt"

echo "########## E5 — S5: is what the pipeline maintains the whole site? ##########"
python3 "$HERE/site-inventory.py" --doc "$TARGET/.lake/build/doc" --pages "$WORK/pages-rev-a" \
  > "$OUT/stage5b-e5-inventory.txt"
sed -n '/=== 5/,$p' "$OUT/stage5b-e5-inventory.txt"

echo
echo "raw output -> $OUT"
