#!/usr/bin/env bash
# Stage 5e — D1..D5: does L3-1 make a real move come out right?
#
# Three page trees are built and compared:
#
#   INC-NOL3   the post-move state reached incrementally with L2 only
#   INC-L3     the post-move state reached incrementally with L2 + L3-1
#   REFERENCE  the post-move state reached by extracting *everything* again
#
# REFERENCE is the oracle. "No stale pages reported" proves nothing — the report
# comes from the same code being tested. Byte equality with a from-scratch build
# is the only check that a mistake cannot satisfy.
#
# Both incremental variants run through the real `incremental.sh`, not a
# hand-rolled sequence, so what is being tested is the pipeline and not a
# transcription of it. `--mode self` is used deliberately: the render set is then
# the honest minimal one, so the comparison catches a wrong *render* set as well
# as a wrong *extraction* set.
#
# THE MODULE TO MOVE IS CHOSEN, NOT ASSUMED. It has to have referrers — modules
# that name something of its in a *printed signature*, which is what the IR's
# `refs` records. The script picks the best candidate out of the base IR and
# refuses to run if it has none, because a move nobody refers to cannot show
# anything about ownership.
#
# PREREQUISITE: `rebuild-own.sh` must have run on the clone first, so that every
# one of the package's oleans carries the clone's path. Without it the baseline
# ledger describes oleans built at the *original* path and every rebuilt module
# reports as changed for path reasons alone.
#
# usage: run.sh <work-dir> <clone-dir> [module-to-move]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
CLONE=${2:?clone dir}
A=${3:-InformationTheory.Shannon.Huffman.Length}
X="${A}Core"
RESULTS="$LD/benchmarks/results"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
render () { deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null; }

# The module list is a glob over the sources, never a scan of `.lake/build`:
# Lake does not delete orphaned oleans (706 of them under this package's tree in
# the clone), so the build tree is not a list of what exists.
modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

probe="$CLONE/.lake/build/lib/lean/InformationTheory/Shannon/Huffman/Length.olean"
if ! strings "$probe" 2>/dev/null | grep -q "$CLONE"; then
  echo "the clone's oleans were not built at the clone's path — run rebuild-own.sh first" >&2
  exit 2
fi

# ------------------------------------------------------------ BASE (pre-move)
if [ ! -f "$W/base-ir/index.json" ]; then
  echo "### BASE — module list, extract, render, ledger (pre-move)"
  modlist "$W/modules-before.txt"
  "$S5/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
    --timings "$W/base-extract.json" > "$W/base-extract.log"
  render "$W/base-ir" "$W/base-pages"
  deno_ "$S5/ledger.ts" build --modules "$W/modules-before.txt" --target "$CLONE" \
    --ir "$W/base-ir" --source-url "$URL" --algorithm lake --out "$W/base-ledger.json" > /dev/null
fi

# Who refers to A in a printed signature? Read out of the IR, not assumed.
echo "### the referrers of $A, from the base IR"
python3 - "$W/base-ir" "$A" "$W/referrers.txt" <<'PY'
import json, os, sys, collections
ir, a, out = sys.argv[1:]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
hits, names = collections.Counter(), collections.defaultdict(set)
for e in idx["modules"]:
    m = json.load(open(os.path.join(ir, e["file"]), encoding="utf-8"))
    if m["module"] == a:
        continue
    for d in m["declarations"]:
        for owner, name in d["refs"]:
            if owner == a:
                hits[m["module"]] += 1
                names[m["module"]].add(name)
open(out, "w", encoding="utf-8").write("\n".join(sorted(hits)) + ("\n" if hits else ""))
print("  %d referring module(s), %d reference(s)" % (len(hits), sum(hits.values())))
for k, v in hits.most_common():
    print("    %-70s %3d  e.g. %s" % (k, v, sorted(names[k])[0]))
if not hits:
    sys.exit("no module names anything of %s in a printed signature; pick another" % a)
PY

# The baseline must be a fixed point: with nothing changed, the ledger must say
# nothing changed. If it does not, the comparison below is measuring drift.
echo "### baseline fixed-point check"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-before.txt" --changed-out /dev/null \
  --render-all-out /dev/null | tee "$OUT/d0.txt"
grep -q ": 0 changed, 0 added, 0 removed" "$OUT/d0.txt" || {
  echo "baseline is not a fixed point; the clone drifted" >&2; exit 3; }

# ------------------------------------------------------------ the move
echo "### applying the move and rebuilding"
"$HERE/setup-clone.sh" move "$CLONE" "$A" minimal > "$W/move.log" 2>&1 || {
  echo "move failed; see $W/move.log" >&2; tail -20 "$W/move.log" >&2; exit 1; }
grep -E "^A is now" "$W/move.log" || true
modlist "$W/modules-after.txt"

# ------------------------------------------------------------ D1: what L2 sees
echo "### D1 — the changed set L2 reports"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-after.txt" --changed-out "$W/d1-changed.txt" \
  --removed-out /dev/null --render-all-out /dev/null | tee "$OUT/d1.txt"
{
  for m in "$A" "$X"; do
    grep -qx "$m" "$W/d1-changed.txt" && echo "D1 contains  $m  yes" \
      || echo "D1 contains  $m  NO"
  done
  n=0; miss=0
  while read -r r; do
    [ -n "$r" ] || continue
    n=$((n + 1))
    grep -qx "$r" "$W/d1-changed.txt" || miss=$((miss + 1))
  done < "$W/referrers.txt"
  echo "D1 referrers in the changed set: $((n - miss))/$n" \
    "$([ "$miss" = "$n" ] && echo '(as predicted: L2 cannot see the move)' \
       || echo '(D1 refuted for at least one referrer)')"
} | tee -a "$OUT/d1.txt"

# ------------------------------------------------------------ REFERENCE
echo "### REFERENCE — extract everything from the moved state, render everything"
rm -rf "$W/ref-ir" "$W/ref-pages"
"$S5/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir" \
  --timings "$W/ref-extract.json" > "$W/ref-extract.log"
render "$W/ref-ir" "$W/ref-pages"

# ------------------------------------------------------------ the two variants
variant () { # variant <name> <l3-1 on|off>
  local v="$1" l31="$2"
  echo "### $v — incremental.sh --l3-1 $l31"
  rm -rf "$W/$v"; mkdir -p "$W/$v"
  cp -R "$W/base-ir" "$W/$v/ir"
  cp -R "$W/base-pages" "$W/$v/pages"
  cp "$W/base-ledger.json" "$W/$v/ledger.json"
  "$S5/incremental.sh" --module "$A" --ir "$W/$v/ir" --pages "$W/$v/pages" \
    --ledger "$W/$v/ledger.json" --modules "$W/modules-after.txt" \
    --source-url "$URL" --work "$W/$v/work" --mode self --l3-1 "$l31" \
    --timings "$W/$v/timings.json" > "$OUT/$v.txt" 2>&1 || {
      echo "  $v failed:"; tail -10 "$OUT/$v.txt"; return 1; }
  python3 -c "
import json
r = json.load(open('$W/$v/timings.json'))
print('  rounds %d, changed %d, staleFound %d, irChanged %d, pages %d, total %.3f s'
      % (r['rounds'], r['changed'], r['staleFound'], r['irChanged'],
         r['pagesRendered'], r['totalSeconds']))"
}
variant nol3 off
variant l3 on

# ------------------------------------------------------------ compare
echo "### comparing both trees against REFERENCE"
python3 - "$W" "$A" "$OUT/compare.txt" <<'PY'
import difflib, json, os, sys
W, A, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

referrers = [l.strip() for l in open(os.path.join(W, "referrers.txt"),
                                     encoding="utf-8") if l.strip()]
ref = tree(os.path.join(W, "ref-pages"))
lines = ["REFERENCE pages: %d" % len(ref), "referrers of A: %d" % len(referrers)]
for name, d in (("INC-NOL3", "nol3/pages"), ("INC-L3", "l3/pages")):
    got = tree(os.path.join(W, d))
    only_ref = sorted(set(ref) - set(got))
    only_got = sorted(set(got) - set(ref))
    diff = sorted(k for k in set(ref) & set(got) if ref[k] != got[k])
    lines += ["", "%s: %d pages, missing %d, extra %d, differing %d"
              % (name, len(got), len(only_ref), len(only_got), len(diff))]
    if only_ref:
        lines.append("  missing:   " + ", ".join(only_ref[:5]))
    if only_got:
        lines.append("  extra:     " + ", ".join(only_got[:5]))
    if diff:
        lines.append("  differing: " + ", ".join(diff[:10])
                     + (" ..." if len(diff) > 10 else ""))
    bad = [r for r in referrers if r.replace(".", "/") + ".html" in diff]
    lines.append("  referrer pages wrong: %d/%d" % (len(bad), len(referrers)))
    if bad:
        p = bad[0].replace(".", "/") + ".html"
        a = ref[p].decode("utf-8", "replace").split("\n")
        b = got[p].decode("utf-8", "replace").split("\n")
        d1 = list(difflib.unified_diff(b, a, "incremental", "reference", n=0, lineterm=""))
        lines.append("  the staleness in %s, incremental -> reference:" % bad[0])
        lines += ["    " + l[:200] for l in d1[2:8]]

for v in ("nol3", "l3"):
    for r in (1, 2, 3, 4):
        p = os.path.join(W, v, "work", "ownership-%d.json" % r)
        if not os.path.exists(p):
            continue
        o = json.load(open(p, encoding="utf-8"))
        lines += ["", "%s L3-1 round %d: lost %d / gained %d names, scanned %d base modules, "
                  "stale %d (byLostOwner %d, byMovedElsewhere %d), %.4f s"
                  % (v, r, o["lostNames"], o["gainedNames"], o["scannedBaseModules"],
                     o["stale"], o["staleByLostOwner"], o["staleByMovedElsewhere"],
                     o["totalSeconds"])]
        for w in o["witnesses"][:5]:
            lines.append("    %s  %s  (ref %s :: %s)"
                         % (w["rule"], w["module"], w["ref"][0], w["ref"][1]))
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
  echo "target            APFS clone of /Users/haruka/dev/lean-projects, own modules rebuilt in place"
  echo "move              $A -> $X (minimal shim)"
  echo "referrers         $(grep -c . "$W/referrers.txt") module(s) naming A's declarations in a printed signature"
  echo "render set        --mode self (the honest minimal set; no render key change)"
  echo
  echo "## D0 — the baseline is a fixed point"
  cat "$OUT/d0.txt"
  echo
  echo "## D1 — what L2 reports after the move"
  cat "$OUT/d1.txt"
  echo
  echo "## D2/D3/D4/D5 — both trees against a from-scratch reference"
  cat "$OUT/compare.txt"
  echo
  echo "## pipeline timings"
  for v in nol3 l3; do
    echo "### $v"
    python3 -m json.tool "$W/$v/timings.json" 2>/dev/null | head -24
  done
} > "$RESULTS/stage5e-ownership.txt"
echo "-> $RESULTS/stage5e-ownership.txt"
