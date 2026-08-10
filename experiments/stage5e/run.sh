#!/usr/bin/env bash
# Stage 5e — D1..D5: does L3-1 make a real move come out right?
#
# Three page trees are built and compared:
#
#   BASE       the pre-move state: 432 modules extracted, all pages rendered.
#   INC-NOL3   the post-move state reached incrementally with L2 only.
#   INC-L3     the post-move state reached incrementally with L2 + L3-1.
#   REFERENCE  the post-move state reached by extracting *everything* again.
#
# REFERENCE is the oracle. "No stale pages reported" proves nothing — the report
# comes from the same code being tested. Byte equality with a from-scratch build
# is the only check that a mistake cannot satisfy.
#
# PREREQUISITE: `rebuild-own.sh` must have run on the clone first, so that every
# one of the package's oleans carries the clone's path. Without it the baseline
# ledger describes oleans built at the *original* path and every rebuilt module
# reports as changed for path reasons alone — see rebuild-own.sh's header for
# how that nearly produced a wrong answer here.
#
# usage: run.sh <work-dir> <clone-dir>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
CLONE=${2:?clone dir}
RESULTS="$LD/benchmarks/results"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

A=InformationTheory.Shannon.GaussianPDFVarianceDerivative
X=InformationTheory.Shannon.GaussianPDFVarianceDerivativeCore
C=InformationTheory.Shannon.FisherDeBruijnGaussian
ROOT=InformationTheory

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
render () { # render <ir> <pages>
  deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
    --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
}

# The module list is a glob over the sources, never a scan of `.lake/build`:
# Lake does not delete orphaned oleans (659 ghosts out of 1,090 on this target,
# stage 5c), so the build tree is not a list of what exists.
modlist () { # modlist <out>
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

# ------------------------------------------------------------ BASE (pre-move)
# The guard, not a comment: a baseline taken over oleans built elsewhere is the
# failure mode this experiment already walked into once.
if ! grep -q "$CLONE" <(strings "$CLONE/.lake/build/lib/lean/InformationTheory/Shannon/FisherDeBruijnGaussian.olean" 2>/dev/null); then
  echo "the clone's oleans were not built at the clone's path — run rebuild-own.sh first" >&2
  exit 2
fi

if [ ! -f "$W/base-ir/index.json" ]; then
  echo "### BASE — module list, extract, render, ledger (pre-move)"
  modlist "$W/modules-before.txt"
  "$S5/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
    --timings "$W/base-extract.json" > "$W/base-extract.log"
  render "$W/base-ir" "$W/base-pages"
  deno_ "$S5/ledger.ts" build --modules "$W/modules-before.txt" --target "$CLONE" \
    --ir "$W/base-ir" --source-url "$URL" --algorithm lake --out "$W/ledger.json" > /dev/null
fi

# ------------------------------------------------------------ the move
echo "### applying the move and rebuilding"
"$HERE/setup-clone.sh" move "$CLONE" > "$W/move.log" 2>&1 || {
  echo "move failed; see $W/move.log" >&2; tail -20 "$W/move.log" >&2; exit 1; }
tail -3 "$W/move.log"
modlist "$W/modules-after.txt"

# ------------------------------------------------------------ D1: what L2 sees
echo "### D1 — the changed set L2 reports"
deno_ "$S5/ledger.ts" check --ledger "$W/ledger.json" --ir "$W/base-ir" --source-url "$URL" \
  --modules "$W/modules-after.txt" --changed-out "$W/changed.txt" \
  --removed-out "$W/removed.txt" --render-all-out "$W/render-all.txt" | tee "$OUT/d1.txt"
{
  echo "D1 changed set ($(grep -c . "$W/changed.txt") modules):"
  sed 's/^/     /' "$W/changed.txt"
  for m in "$A" "$X" "$ROOT"; do
    grep -qx "$m" "$W/changed.txt" && echo "D1 contains   $m  yes" || echo "D1 contains   $m  NO"
  done
  grep -qx "$C" "$W/changed.txt" && echo "D1 contains C $C  YES (D1 refuted)" \
    || echo "D1 contains C $C  no (as predicted: L2 cannot see the move)"
} | tee -a "$OUT/d1.txt"

# ------------------------------------------------------------ REFERENCE
echo "### REFERENCE — extract everything from the moved state, render everything"
rm -rf "$W/ref-ir" "$W/ref-pages"
"$S5/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir" \
  --timings "$W/ref-extract.json" > "$W/ref-extract.log"
render "$W/ref-ir" "$W/ref-pages"

# ------------------------------------------------------------ round 1
# Shared by both variants: re-extract exactly what L2 asked for. The IR is kept
# unmerged so that L3-1 can diff the old ownership against the new.
echo "### round 1 — re-extract L2's changed set"
rm -rf "$W/inc1"
"$S5/extract-once.sh" --modules "$W/changed.txt" --ir-dir "$W/inc1" \
  --timings "$W/inc1-extract.json" > "$W/inc1-extract.log"

# ------------------------------------------------------------ D2: L2 only
echo "### D2 — L2 only: merge round 1 and render everything"
rm -rf "$W/nol3-ir" "$W/nol3-pages"
cp -R "$W/base-ir" "$W/nol3-ir"
deno_ "$S5/merge-ir.ts" --base "$W/nol3-ir" --inc "$W/inc1" --out "$W/nol3-ir" \
  --changed-out "$W/nol3-irchanged.txt" > /dev/null
render "$W/nol3-ir" "$W/nol3-pages"

# ------------------------------------------------------------ D3/D4/D5: + L3-1
echo "### D3 — L3-1: ownership diff on round 1, then a second round"
rm -rf "$W/l3-ir" "$W/l3-pages"
cp -R "$W/base-ir" "$W/l3-ir"
deno_ "$S5/ownership.ts" --base "$W/l3-ir" --inc "$W/inc1" --exclude "$W/changed.txt" \
  --print-set "$W/stale1.txt" --json "$OUT/ownership-round1.json" | tee "$OUT/d3.txt"
deno_ "$S5/merge-ir.ts" --base "$W/l3-ir" --inc "$W/inc1" --out "$W/l3-ir" \
  --changed-out "$W/l3-irchanged-1.txt" > /dev/null

ROUND2=$(grep -c . "$W/stale1.txt" || true)
if [ "$ROUND2" -gt 0 ]; then
  rm -rf "$W/inc2"
  "$S5/extract-once.sh" --modules "$W/stale1.txt" --ir-dir "$W/inc2" \
    --timings "$W/inc2-extract.json" > "$W/inc2-extract.log"
  # D4: a third round is needed only if this one moved ownership again.
  deno_ "$S5/ownership.ts" --base "$W/l3-ir" --inc "$W/inc2" --exclude "$W/stale1.txt" \
    --print-set "$W/stale2.txt" --json "$OUT/ownership-round2.json" | tee -a "$OUT/d3.txt"
  deno_ "$S5/merge-ir.ts" --base "$W/l3-ir" --inc "$W/inc2" --out "$W/l3-ir" \
    --changed-out "$W/l3-irchanged-2.txt" > /dev/null
else
  : > "$W/stale2.txt"
fi
render "$W/l3-ir" "$W/l3-pages"

# ------------------------------------------------------------ compare
echo "### comparing the three trees against REFERENCE"
python3 - "$W" "$C" "$OUT/compare.txt" <<'PY'
import json, os, subprocess, sys
W, C, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

ref = tree(os.path.join(W, "ref-pages"))
lines = ["REFERENCE pages: %d" % len(ref)]
c_page = C.replace(".", "/") + ".html"
for name, d in (("INC-NOL3", "nol3-pages"), ("INC-L3", "l3-pages")):
    got = tree(os.path.join(W, d))
    only_ref = sorted(set(ref) - set(got))
    only_got = sorted(set(got) - set(ref))
    diff = sorted(k for k in set(ref) & set(got) if ref[k] != got[k])
    lines.append("")
    lines.append("%s: %d pages, missing %d, extra %d, differing %d"
                 % (name, len(got), len(only_ref), len(only_got), len(diff)))
    if diff:
        lines.append("  differing pages: " + ", ".join(diff[:10])
                     + (" ..." if len(diff) > 10 else ""))
    lines.append("  C's page (%s) differs: %s" % (c_page, "YES" if c_page in diff else "no"))
    if name == "INC-NOL3" and c_page in diff:
        a = ref[c_page].decode("utf-8", "replace")
        b = got[c_page].decode("utf-8", "replace")
        import difflib
        d1 = [l for l in difflib.unified_diff(b.split("\n"), a.split("\n"),
                                              "incremental", "reference", n=0, lineterm="")]
        lines.append("  first 6 diff lines (incremental -> reference):")
        lines += ["    " + l for l in d1[2:8]]

for r, path in ((1, "ownership-round1.json"), (2, "ownership-round2.json")):
    p = os.path.join(W, "out", path)
    if os.path.exists(p):
        o = json.load(open(p, encoding="utf-8"))
        lines.append("")
        lines.append("L3-1 round %d: lost %d / gained %d names, scanned %d base modules, "
                     "stale %d (byLostOwner %d, byMovedElsewhere %d), %.4f s"
                     % (r, o["lostNames"], o["gainedNames"], o["scannedBaseModules"],
                        o["stale"], o["staleByLostOwner"], o["staleByMovedElsewhere"],
                        o["totalSeconds"]))
        for w in o["witnesses"][:5]:
            lines.append("    %s  %s  (ref %s :: %s)" % (w["rule"], w["module"],
                                                         w["ref"][0], w["ref"][1]))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# ------------------------------------------------------------ report
{
  echo "# stage5e — L3-1 (name ownership) against a real move"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "target            APFS clone of /Users/haruka/dev/lean-projects (original untouched)"
  echo "move              $A -> $X, C untouched"
  echo
  echo "## D1 — what L2 reports"
  cat "$OUT/d1.txt"
  echo
  echo "## D2/D3/D4 — the three trees against a from-scratch reference"
  cat "$OUT/compare.txt"
} > "$RESULTS/stage5e-ownership.txt"
echo "-> $RESULTS/stage5e-ownership.txt"
