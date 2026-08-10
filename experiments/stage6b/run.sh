#!/usr/bin/env bash
# Stage 6b — V0..V6: keeping the revision out of the page bytes.
#
# THE CHANGE TURNED OUT TO NEED NO RENDERER CHANGE AT ALL.
#   `--source-url` is a plain prefix string that render.ts concatenates with an
#   IR-derived path (`render.ts:1250`). So "put a placeholder in the bytes" is
#   `--source-url '{{SOURCE_URL}}'` and nothing else. That also takes the revision
#   out of `renderKey` for free, because the key stores the string it was given
#   (`ledger.ts:203-208`) and the string is now constant across commits.
#
#   The output-contract change is real even though the diff is not: the pages stop
#   being self-contained. What is measured here is what that buys.
#
# TWO TREES, MEASURED IN THE SAME SESSION
#   A  rendered with a real revision — today's contract
#   B  rendered with the token       — the proposed contract
#
#   For A, "a new commit" means running the pipeline with a *different* revision,
#   which is what a commit does today. For B a commit changes no pipeline input at
#   all, which is the whole claim, so the same run is the test.
#
# usage: run.sh <work-dir> <clone-dir> [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
CLONE=${2:?clone dir}
RUNS=${3:-6}
RESULTS="$LD/benchmarks/results"

REV1=573793b243fb1343636088eb62d1789ab2b14cec
REV2=a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4
BASE=https://github.com/FujiHaruka/information-theory/blob
URL1="$BASE/$REV1"
URL2="$BASE/$REV2"
TOKEN='{{SOURCE_URL}}'

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
render () { # render <ir> <pages> <source-url>
  deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
    --ir "$1" --pages "$2" --source-url "$3" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }

modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

# ------------------------------------------------------------------- the IR
# Reused from stage 6a rather than re-extracted: this stage asks nothing about
# extraction, and 27 s of Lean would only add variance. If it is not there, extract.
echo "### module list and IR"
modlist "$W/modules.txt"
if [ ! -f "$W/ir/index.json" ]; then
  if [ -f "$W/../w6a/ref-ir/index.json" ]; then
    echo "  reusing stage 6a's post-move IR"
    cp -R "$W/../w6a/ref-ir" "$W/ir"
  else
    "$S5/extract-once.sh" --modules "$W/modules.txt" --ir-dir "$W/ir" \
      --timings "$W/extract.json" > "$W/extract.log"
  fi
fi

# --------------------------------------------------------------- the two bases
echo "### base A (revision in the bytes) and base B (token in the bytes)"
for v in A B; do
  [ "$v" = A ] && u="$URL1" || u="$TOKEN"
  if [ ! -d "$W/base-$v" ]; then
    render "$W/ir" "$W/base-$v" "$u"
    deno_ "$S5/ledger.ts" build --modules "$W/modules.txt" --target "$CLONE" \
      --ir "$W/ir" --source-url "$u" --algorithm lake \
      --out "$W/base-ledger-$v.json" > /dev/null
  fi
done

echo "### both baselines must be fixed points"
for v in A B; do
  [ "$v" = A ] && u="$URL1" || u="$TOKEN"
  deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger-$v.json" --ir "$W/ir" \
    --source-url "$u" --modules "$W/modules.txt" --changed-out /dev/null \
    --removed-out /dev/null --render-all-out /dev/null | sed "s/^/  $v /" \
    | tee -a "$OUT/fixpoint.txt"
done
grep -c ": 0 changed, 0 added, 0 removed" "$OUT/fixpoint.txt" > /dev/null || {
  echo "a baseline is not a fixed point" >&2; exit 3; }

# ------------------------------------------------------------------- V2 / V5
echo "### V2/V5 — what is in the bytes, and how many of them"
python3 - "$W/base-A" "$W/base-B" "$REV1" "$TOKEN" "$OUT/bytes.txt" <<'PY'
import os, sys
a, b, rev, token, out = sys.argv[1:]
def scan(p, needle):
    total = bytes_ = files = hits = 0
    per = []
    for root, _d, fs in os.walk(p):
        for f in fs:
            data = open(os.path.join(root, f), "rb").read()
            bytes_ += len(data)
            files += 1
            n = data.count(needle.encode())
            if n:
                hits += 1
                per.append(n)
            total += n
    return total, hits, files, bytes_, per
ra, ha, fa, ba, pa = scan(a, rev)
ta, _, _, _, _ = scan(a, token)
rb, hb, fb, bb, pb = scan(b, rev)
tb, htb, _, _, ptb = scan(b, token)
lines = [
  "A (revision in the bytes): %d files, %d bytes" % (fa, ba),
  "  occurrences of the 40-hex revision: %d across %d files (per file min %d max %d)"
  % (ra, ha, min(pa) if pa else 0, max(pa) if pa else 0),
  "  occurrences of the token: %d" % ta,
  "B (token in the bytes): %d files, %d bytes" % (fb, bb),
  "  occurrences of the 40-hex revision: %d  <- V2" % rb,
  "  occurrences of the token: %d across %d files (per file min %d max %d)"
  % (tb, htb, min(ptb) if ptb else 0, max(ptb) if ptb else 0),
  "V5 size delta B - A: %d bytes (%+.2f%%)" % (bb - ba, 100.0 * (bb - ba) / ba),
]
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# ----------------------------------------------------------------------- V4
# The inverse substitution must reproduce A-at-REV2 exactly. Rendering A at REV2
# is how the comparison target is produced, so this also exercises the ordinary
# path at a second revision.
echo "### V4 — substituting the token back must reproduce the rendered tree"
rm -rf "$W/A-rev2" "$W/B-injected"
render "$W/ir" "$W/A-rev2" "$URL2"
cp -R "$W/base-B" "$W/B-injected"
INJ_START=$(python3 -c 'import time; print(repr(time.time()))')
python3 - "$W/B-injected" "$TOKEN" "$URL2" <<'PY'
import os, sys
root, token, url = sys.argv[1:]
tb, ub = token.encode(), url.encode()
n = 0
for r, _d, fs in os.walk(root):
    for f in fs:
        p = os.path.join(r, f)
        data = open(p, "rb").read()
        if tb in data:
            open(p, "wb").write(data.replace(tb, ub))
            n += 1
print("  substituted in %d files" % n)
PY
INJ_END=$(python3 -c 'import time; print(repr(time.time()))')
INJ_S=$(python3 -c "print(repr($INJ_END - $INJ_START))")
echo "  V6 deploy-time substitution over the tree: $INJ_S s"

python3 - "$W/A-rev2" "$W/B-injected" "$INJ_S" "$OUT/v4.txt" <<'PY'
import os, sys
a, b, inj, out = sys.argv[1:]
def tree(p):
    d = {}
    for r, _dd, fs in os.walk(p):
        for f in fs:
            full = os.path.join(r, f)
            d[os.path.relpath(full, p)] = open(full, "rb").read()
    return d
ta, tb = tree(a), tree(b)
missing = sorted(set(ta) - set(tb)); extra = sorted(set(tb) - set(ta))
diff = sorted(k for k in set(ta) & set(tb) if ta[k] != tb[k])
lines = ["V4 A-at-REV2 vs B-with-token-substituted: %d files, missing %d, extra %d, differing %d -> %s"
         % (len(ta), len(missing), len(extra), len(diff),
            "byte-identical" if not (missing or extra or diff) else "DIFFERS")]
for k in diff[:5]:
    lines.append("   differs: %s (%d vs %d bytes)" % (k, len(ta[k]), len(tb[k])))
lines.append("V6 substitution cost: %.4f s for %d files" % (float(inj), len(tb)))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# ------------------------------------------------------------- V0 / V1 / V3
# A: run the pipeline with a *new* revision — a real commit, today's contract.
# B: run it with the same token — a real commit under the proposed contract,
#    because the revision is no longer a pipeline input.
# V0 is measured on the mechanism, because the bug it names has just been fixed in
# `incremental.sh` and cannot be observed there any more. What the pre-fix script
# did was hand `render.ts` no `--only` at all, so the measurement is: what does
# render.ts write when given no `--only`?
echo "### V0 — what 'no --only' means to the renderer"
rm -rf "$W/v0-probe"
deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$W/ir" --pages "$W/v0-probe" --source-url "$TOKEN" > "$W/v0.log"
V0N=$(find "$W/v0-probe" -name '*.html' | wc -l | tr -d ' ')
{
  echo "V0 render.ts with no --only wrote $V0N HTML pages"
  echo "V0 -> an empty render set therefore meant 'render everything'."
  echo "     Pre-fix incremental.sh built --only from the render set and passed none"
  echo "     when it was empty (see the commit that added the guard). Fixed: the"
  echo "     renderer is now skipped instead."
} | tee "$OUT/v0.txt"

: > "$W/runs.jsonl"
variant () { # variant <A|B> <run>
  local v="$1" run="$2" u
  [ "$v" = A ] && u="$URL2" || u="$TOKEN"
  local d="$W/run-$v"
  rm -rf "$d"; mkdir -p "$d"
  cp -R "$W/ir" "$d/ir"
  cp -R "$W/base-$v" "$d/pages"
  cp "$W/base-ledger-$v.json" "$d/ledger.json"
  if "$S5/incremental.sh" --ir "$d/ir" --pages "$d/pages" --ledger "$d/ledger.json" \
       --modules "$W/modules.txt" --source-url "$u" --work "$d/work" --mode self \
       --timings "$d/timings.json" > "$OUT/$v-run$run.txt" 2>&1; then
    python3 - "$d/timings.json" "$v" "$run" >> "$W/runs.jsonl" <<'PY'
import json, sys
p, v, run = sys.argv[1:]
r = json.load(open(p, encoding="utf-8"))
r["variant"], r["run"] = v, int(run)
print(json.dumps(r))
PY
    python3 -c "
import json
r = json.load(open('$d/timings.json'))
print('  %s run %s: changed %d, renderAll-forced mode %s, pages %d, render %.4f s, total %.4f s'
      % ('$v', '$run', r['changed'], r['mode'], r['pagesRendered'],
         r['renderSeconds'], r['totalSeconds']))"
  else
    echo "  $v run $run FAILED:"; tail -12 "$OUT/$v-run$run.txt"
  fi
}
echo "### V0/V1/V3 — the pipeline on a commit, both contracts, interleaved"
for r in $(seq 1 "$RUNS"); do
  variant A "$r"
  variant B "$r"
done

python3 - "$W" "$OUT/verdict.txt" <<'PY'
import json, os, statistics, sys
W, out = sys.argv[1:]
runs = [json.loads(l) for l in open(os.path.join(W, "runs.jsonl"), encoding="utf-8")]
def sel(v, k):
    return [r[k] for r in runs if r["variant"] == v and r["run"] > 1]
def med(v, k):
    xs = sel(v, k)
    return statistics.median(xs) if xs else float("nan")
lines = ["%-3s %8s %8s %10s %22s %10s" %
         ("var", "changed", "pages", "total", "[min-max]", "render")]
for v in ("A", "B"):
    xs = sel(v, "totalSeconds")
    if not xs:
        continue
    lines.append("%-3s %8.0f %8.0f %10.4f %22s %10.4f"
                 % (v, med(v, "changed"), med(v, "pagesRendered"), med(v, "totalSeconds"),
                    "[%.4f-%.4f]" % (min(xs), max(xs)), med(v, "renderSeconds")))
lines.append("")
lines.append("modes seen: A %s ; B %s"
             % (sorted({r["mode"] for r in runs if r["variant"] == "A"}),
                sorted({r["mode"] for r in runs if r["variant"] == "B"})))
lines.append("V1 -> B renders %.0f pages (A renders %.0f)"
             % (med("B", "pagesRendered"), med("A", "pagesRendered")))
lines.append("V3 -> total %.4f s -> %.4f s (%+.4f s, %+.1f%%); render stage %.4f -> %.4f s"
             % (med("A", "totalSeconds"), med("B", "totalSeconds"),
                med("B", "totalSeconds") - med("A", "totalSeconds"),
                100.0 * (med("B", "totalSeconds") - med("A", "totalSeconds")) / med("A", "totalSeconds"),
                med("A", "renderSeconds"), med("B", "renderSeconds")))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage6b — keeping the revision out of the page bytes"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "target            APFS clone of /Users/haruka/dev/lean-projects (post-move state, 433 modules)"
  echo "contract A        --source-url $BASE/<40 hex>   (today)"
  echo "contract B        --source-url $TOKEN            (proposed)"
  echo "runs              $RUNS per variant, interleaved, run 1 discarded"
  echo "note              the IR is reused across both; no Lean runs in the measured loop"
  echo
  echo "## V2/V5 — the bytes"
  cat "$OUT/bytes.txt"
  echo
  echo "## V4/V6 — the inverse substitution"
  cat "$OUT/v4.txt"
  echo
  echo "## V0 — an empty render set meant 'render everything'"
  cat "$OUT/v0.txt"
  echo
  echo "## V1/V3 — the pipeline on a commit"
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage6b-revless.txt"
cp "$W/runs.jsonl" "$RESULTS/stage6b-revless.jsonl"
echo "-> $RESULTS/stage6b-revless.txt"
