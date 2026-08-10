#!/usr/bin/env bash
# Stage 5d — C1..C5 and C7: does splitting L1 do what it claims, and what does
# it save?
#
# Both cost variants run in one session at one warm state, interleaved, because
# a before/after taken at different warm states says nothing (CLAUDE.md).
#
#   split    the real thing: `--source-url` revision changes.
#            Expected: re-extract 0, render 432, Lean never started.
#   onekey   the counterfactual: the same revision change under an *unsplit*
#            key. Reproduced by faking the extract key and forcing `--mode all`,
#            which is exactly the work one key would have ordered.
#            Expected: re-extract 432, render 432.
#
# usage: run.sh <work-dir> [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
RUNS=${2:-5}
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
RESULTS="$LD/benchmarks/results"
MODLIST="$RESULTS/it-modules.txt"

REV_A=573793b243fb1343636088eb62d1789ab2b14cec
REV_B=0000111122223333444455556666777788889999
URL_A="https://github.com/FujiHaruka/information-theory/blob/$REV_A"
URL_B="https://github.com/FujiHaruka/information-theory/blob/$REV_B"

mkdir -p "$W" "$RESULTS"
OUT="$W/out"; mkdir -p "$OUT"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
now () { python3 -c 'import time; print(repr(time.time()))'; }

# ---------------------------------------------------------------- base IR
if [ ! -f "$W/base-ir/index.json" ]; then
  echo "### extracting the base IR (432 modules)"
  "$S5/extract-once.sh" --modules "$MODLIST" --ir-dir "$W/base-ir" \
    --timings "$W/base-extract.json" > "$W/base-extract.log"
fi
python3 -c "
import json,sys
d=json.load(open('$W/base-ir/index.json'))
print('base IR: schema', d['schemaVersion'], d['generator'], len(d['modules']), 'modules')
assert len(d['modules'])==432, d
"

# ---------------------------------------------------------------- setup
# One live tree per variant: IR + pages rendered at REV_A + a ledger recorded at
# REV_A. The variants must not share a tree, or run N of one would see run N-1
# of the other.
setup () { # setup <variant>
  # `local a=$1 b=$a` does not work: bash expands every word of a `local`
  # statement before assigning any of them, so `$a` is still unset there.
  local v="$1"
  local live="$W/live-$v"
  rm -rf "$live"; mkdir -p "$live"
  cp -R "$W/base-ir" "$live/ir"
  deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
    --ir "$live/ir" --pages "$live/pages" --source-url "$URL_A" > "$live/render-a.log"
  deno_ "$S5/ledger.ts" build --modules "$MODLIST" --target "$TARGET" \
    --ir "$live/ir" --source-url "$URL_A" --algorithm lake \
    --out "$live/ledger.json" > "$live/ledger-build.log"
}

# The onekey counterfactual: an extract-key change of the same *kind* a single
# key would have produced for a new revision. Faked in our own ledger; the
# target's lean-toolchain is untouched.
fake_extract_key () { # fake_extract_key <ledger.json>
  python3 - "$1" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p, encoding="utf-8"))
d["extractKey"]["leanToolchain"] += "+onekey-counterfactual"
json.dump(d, open(p, "w", encoding="utf-8"))
PY
}

# ---------------------------------------------------------------- C1..C4
echo "### C1..C4 — the split variant, inspected once before timing"
setup split
S="$W/probe"; rm -rf "$S"; mkdir -p "$S"
cp -R "$W/live-split/ir" "$S/ir-before"

deno_ "$S5/ledger.ts" check --ledger "$W/live-split/ledger.json" --ir "$W/live-split/ir" \
  --source-url "$URL_B" --modules "$MODLIST" \
  --changed-out "$S/changed.txt" --removed-out "$S/removed.txt" \
  --render-all-out "$S/render-all.txt" --timings "$S/check.json" | tee "$OUT/c1-check.txt"
C1_REEXTRACT=$(grep -c . "$S/changed.txt" || true)
C1_RENDERALL=$(grep -c . "$S/render-all.txt" || true)
echo "C1: reExtract=$C1_REEXTRACT renderAll lines=$C1_RENDERALL" | tee -a "$OUT/c1-check.txt"
cat "$S/render-all.txt" | sed 's/^/     /' | tee -a "$OUT/c1-check.txt"

echo "### one incremental run at the new revision (this is C2/C3/C4)"
"$S5/incremental.sh" --module "$(head -1 "$MODLIST")" --source-url "$URL_B" \
  --ir "$W/live-split/ir" --pages "$W/live-split/pages" \
  --ledger "$W/live-split/ledger.json" --modules "$MODLIST" \
  --work "$S/run" --mode self --timings "$S/inc.json" \
  > "$OUT/c2-incremental.txt" 2> "$OUT/c2-incremental.err" || true
cat "$OUT/c2-incremental.txt"

python3 - "$S/inc.json" "$S/run" "$S/ir-before" "$W/live-split/ir" "$OUT/c2-c4.txt" <<'PY'
import json, os, sys, subprocess
inc, work, ir_before, ir_after, out = sys.argv[1:]
rec = json.load(open(inc, encoding="utf-8"))
lines = []
lines.append("C2 extractSeconds  %.4f  (Lean started: %s)"
             % (rec["extractSeconds"], "yes" if rec["extractSeconds"] > 0.5 else "no"))
ev = os.path.join(work, "extract-events.jsonl")
lines.append("C2 extractor events file exists: %s" % os.path.exists(ev))
lines.append("C2 mode actually used: %s" % rec["mode"])
lines.append("C3 pagesRendered   %d" % rec["pagesRendered"])
d = subprocess.run(["diff", "-r", "-q", ir_before, ir_after], capture_output=True, text=True)
lines.append("C4 IR before vs after: %s" % ("byte-identical" if not d.stdout.strip() else d.stdout.strip()))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

echo "### C3 — do the page bytes really carry the new revision?"
python3 - "$W/live-split/pages" "$REV_A" "$REV_B" "$OUT/c3-pages.txt" <<'PY'
import os, sys
pages, rev_a, rev_b, out = sys.argv[1:]
n = with_a = with_b = 0
for root, _dirs, files in os.walk(pages):
    for f in files:
        if not f.endswith(".html"):
            continue
        n += 1
        s = open(os.path.join(root, f), encoding="utf-8").read()
        if rev_a in s:
            with_a += 1
        if rev_b in s:
            with_b += 1
lines = ["C3 html pages            %d" % n,
         "C3 still carrying REV_A  %d" % with_a,
         "C3 now carrying REV_B    %d" % with_b]
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# ---------------------------------------------------------------- C5
echo "### C5 — an extract-key change re-extracts all and forces no render"
cp "$W/live-split/ledger.json" "$S/ledger-extractfake.json"
fake_extract_key "$S/ledger-extractfake.json"
deno_ "$S5/ledger.ts" check --ledger "$S/ledger-extractfake.json" --ir "$W/live-split/ir" \
  --source-url "$URL_A" --modules "$MODLIST" \
  --changed-out "$S/changed-x.txt" --render-all-out "$S/render-all-x.txt" | tee "$OUT/c5-check.txt"
{
  echo "C5 reExtract      $(grep -c . "$S/changed-x.txt" || true)"
  echo "C5 renderAll lines $(grep -c . "$S/render-all-x.txt" || true)"
} | tee -a "$OUT/c5-check.txt"

# ---------------------------------------------------------------- C7
echo "### C7 — split vs single key, interleaved, $RUNS runs each"
setup onekey
JSONL="$RESULTS/stage5d-keysplit.jsonl"
: > "$JSONL"

run_one () { # run_one <variant> <i>
  local v="$1" i="$2"
  local live="$W/live-$v" wd="$W/run-$v"
  local tl="$W/time-$v-$i.txt" tj="$W/timings-$v-$i.json"
  local url mode
  printf '{}\n' > "$tj"
  if [ "$v" = onekey ]; then
    # Rebuild the ledger each run so the fake extract-key change is present
    # again (the run does not write the ledger back, but the setup is cheap and
    # this keeps run N identical to run 1).
    deno_ "$S5/ledger.ts" build --modules "$MODLIST" --target "$TARGET" \
      --ir "$live/ir" --source-url "$URL_A" --algorithm lake \
      --out "$live/ledger.json" > /dev/null
    fake_extract_key "$live/ledger.json"
    url="$URL_A"; mode=all
  else
    url="$URL_B"; mode=self
  fi
  python3 "$LD/experiments/stage4c/merge-timing.py" --name "stage5d-$v" --run "$i" \
    --time-l "$tl" --timings "$tj" --exec -- \
    "$S5/incremental.sh" --module "$(head -1 "$MODLIST")" --source-url "$url" \
    --ir "$live/ir" --pages "$live/pages" --ledger "$live/ledger.json" \
    --modules "$MODLIST" --work "$wd" --mode "$mode" --timings "$tj" \
    >> "$JSONL"
  echo "  $v run $i done"
}

for i in $(seq 1 "$RUNS"); do
  run_one split "$i"
  run_one onekey "$i"
done

KEYS=detectSeconds,extractSeconds,mergeSeconds,impactSeconds,renderSeconds,totalSeconds,changed,irChanged,pagesRendered
{
  echo "# stage5d — L1 split into extractKey / renderKey"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$TARGET/lean-toolchain")"
  echo "runs              $RUNS per variant, interleaved (run 1 dropped)"
  echo "split             --source-url revision changed: render key differs, extract key does not"
  echo "onekey            counterfactual: the same change under one key (extract key faked + --mode all)"
  echo
  for v in split onekey; do
    echo "## $v"
    python3 "$S5/summarize.py" <(grep "\"stage5d-$v\"" "$JSONL") --keys "$KEYS" || true
    echo
  done
  echo "## criteria"
  cat "$OUT/c1-check.txt" "$OUT/c2-c4.txt" "$OUT/c3-pages.txt" "$OUT/c5-check.txt"
} > "$RESULTS/stage5d-keysplit.txt"
echo "-> $RESULTS/stage5d-keysplit.txt"
