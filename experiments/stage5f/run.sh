#!/usr/bin/env bash
# Stage 5f — F1..F5: the deletion path.
#
# Delete a leaf module inside the clone, run the incremental pipeline, and
# compare the resulting page tree against a from-scratch build of the
# post-deletion state. As in stage 5e the oracle is byte equality, not a report
# produced by the code under test.
#
# PREREQUISITE: `../stage5e/rebuild-own.sh` must have run on the clone, and the
# clone must be reset to its pristine sources.
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
LAKE="${LAKE:-$HOME/.elan/bin/lake}"

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
# Rendering a tree means the module pages *and* the whole-package artifacts:
# they are part of the site, so leaving them out of the comparison would leave
# the thing stage 5g implements untested.
render () { deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }
modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

# ------------------------------------------------------------ BASE
if [ ! -f "$W/base-ir/index.json" ]; then
  echo "### BASE — module list, extract, render, ledger"
  modlist "$W/modules-before.txt"
  "$S5/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
    --timings "$W/base-extract.json" > "$W/base-extract.log"
  render "$W/base-ir" "$W/base-pages"
  deno_ "$S5/ledger.ts" build --modules "$W/modules-before.txt" --target "$CLONE" \
    --ir "$W/base-ir" --source-url "$URL" --algorithm lake --out "$W/base-ledger.json" > /dev/null
fi

echo "### baseline fixed-point check"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-before.txt" --changed-out /dev/null \
  --render-all-out /dev/null | tee "$OUT/f0.txt"
grep -q ": 0 changed, 0 added, 0 removed" "$OUT/f0.txt" || {
  echo "baseline is not a fixed point; reset the clone and rebuild-own it" >&2; exit 3; }

# ------------------------------------------------------------ pick a leaf
# Both conditions are read off the IR, not assumed: nothing but the root may
# import it (or the source stops compiling for reasons unrelated to the test),
# and nothing may name its declarations in a printed signature (same reason).
deno_ "$S5/impact.ts" --ir "$W/base-ir" --census "$W/census.tsv" > /dev/null
DEL=$(python3 - "$W/census.tsv" <<'PY'
import csv, sys
rows = list(csv.DictReader(open(sys.argv[1]), delimiter="\t"))
for r in rows:
    for k in ("declarations", "importedByDirect", "referrersDirect"):
        r[k] = int(r[k])
cand = [r for r in rows
        if r["importedByDirect"] <= 1 and r["referrersDirect"] == 0
        and r["declarations"] >= 3]
if not cand:
    sys.exit("no deletable leaf in this package")
cand.sort(key=lambda r: -r["declarations"])
print(cand[0]["module"])
PY
)
echo "### deleting the leaf: $DEL"
DEL_REL="$(echo "$DEL" | tr '.' '/').lean"
grep -c . <<< "$DEL" > /dev/null

# ------------------------------------------------------------ apply
python3 - "$CLONE" "$DEL_REL" "$DEL" <<'PY'
import os, re, sys
clone, rel, mod = sys.argv[1:]
os.remove(os.path.join(clone, rel))
# The root imports every module of the package, so it has to lose the line too.
root = os.path.join(clone, "InformationTheory.lean")
src = open(root, encoding="utf-8").read().split("\n")
kept = [l for l in src if l.strip() != "import " + mod]
if len(kept) == len(src):
    sys.exit("the root does not import %s; the leaf choice is wrong" % mod)
open(root, "w", encoding="utf-8").write("\n".join(kept))
print("  removed %s and its import from the root" % rel)
PY
echo "### rebuilding"
(cd "$CLONE" && "$LAKE" build 2>&1 | tail -3)
modlist "$W/modules-after.txt"

# ------------------------------------------------------------ F1
echo "### F1 — what the ledger reports"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-after.txt" --changed-out "$W/f1-changed.txt" \
  --removed-out "$W/f1-removed.txt" --render-all-out /dev/null | tee "$OUT/f1.txt"
{
  grep -qx "$DEL" "$W/f1-removed.txt" && echo "F1 removed contains  $DEL  yes" \
    || echo "F1 removed contains  $DEL  NO (F1 refuted)"
  grep -qx "InformationTheory" "$W/f1-changed.txt" && echo "F1 changed contains  root  yes" \
    || echo "F1 changed contains  root  NO (F1 refuted)"
} | tee -a "$OUT/f1.txt"

# ------------------------------------------------------------ REFERENCE
echo "### REFERENCE — extract and render everything from the post-deletion state"
rm -rf "$W/ref-ir" "$W/ref-pages"
"$S5/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir" \
  --timings "$W/ref-extract.json" > "$W/ref-extract.log"
render "$W/ref-ir" "$W/ref-pages"

# ------------------------------------------------------------ the incremental run
echo "### incremental run"
rm -rf "$W/inc"; mkdir -p "$W/inc"
cp -R "$W/base-ir" "$W/inc/ir"
cp -R "$W/base-pages" "$W/inc/pages"
cp "$W/base-ledger.json" "$W/inc/ledger.json"
"$S5/incremental.sh" --module "$DEL" --ir "$W/inc/ir" --pages "$W/inc/pages" \
  --ledger "$W/inc/ledger.json" --modules "$W/modules-after.txt" --source-url "$URL" \
  --work "$W/inc/work" --mode self --l3-1 on --timings "$W/inc/timings.json" \
  > "$OUT/inc.txt" 2>&1 || { echo "  failed:"; tail -15 "$OUT/inc.txt"; exit 1; }
tail -1 "$OUT/inc.txt" | python3 -c "
import json, sys
r = json.loads(sys.stdin.read())
print('  rounds %d, changed %d, removed %d, staleFound %d, pages %d, prune %.3f s, total %.3f s'
      % (r['rounds'], r['changed'], r['removed'], r['staleFound'], r['pagesRendered'],
         r['pruneSeconds'], r['totalSeconds']))"

# ------------------------------------------------------------ F2..F5
echo "### F2..F5"
python3 - "$W" "$DEL" "$OUT/compare.txt" <<'PY'
import json, os, sys
W, DEL, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

lines = []

# F2
o = json.load(open(os.path.join(W, "inc", "work", "ownership-1.json"), encoding="utf-8"))
lines.append("F2 ownership: removedModules %d, lost %d name(s), stale %d "
             "(byLostOwner %d, byMovedElsewhere %d), %.4f s"
             % (o["removedModules"], o["lostNames"], o["stale"],
                o["staleByLostOwner"], o["staleByMovedElsewhere"], o["totalSeconds"]))
for w in o["witnesses"][:5]:
    lines.append("     %s  %s  (ref %s :: %s)" % (w["rule"], w["module"], w["ref"][0], w["ref"][1]))

# F3
inc_idx = json.load(open(os.path.join(W, "inc", "ir", "index.json"), encoding="utf-8"))
ref_idx = json.load(open(os.path.join(W, "ref-ir", "index.json"), encoding="utf-8"))
inc_mods = {e["module"] for e in inc_idx["modules"]}
ref_mods = {e["module"] for e in ref_idx["modules"]}
lines.append("F3 IR index: incremental %d modules, reference %d, DEL still indexed: %s"
             % (len(inc_mods), len(ref_mods), DEL in inc_mods))
lines.append("F3 index module sets equal: %s" % (inc_mods == ref_mods))
mfile = os.path.join(W, "inc", "ir", "modules", DEL + ".json")
lines.append("F3 DEL's module file still on disk: %s" % os.path.exists(mfile))

def depmap(ir):
    idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
    m = {}
    for d in idx.get("dependencyMaps", []):
        f = json.load(open(os.path.join(ir, d["file"]), encoding="utf-8"))
        m.update(f["declarations"])
    return m
di, dr = depmap(os.path.join(W, "inc", "ir")), depmap(os.path.join(W, "ref-ir"))
lines.append("F3 dependency map entries: incremental %d, reference %d, equal: %s"
             % (len(di), len(dr), di == dr))

# F4/F5
ref = tree(os.path.join(W, "ref-pages"))
got = tree(os.path.join(W, "inc", "pages"))
del_page = DEL.replace(".", "/") + ".html"
only_ref = sorted(set(ref) - set(got))
only_got = sorted(set(got) - set(ref))
diff = sorted(k for k in set(ref) & set(got) if ref[k] != got[k])
lines.append("F4 DEL's page present after the run: %s" % (del_page in got))
lines.append("F5 incremental %d pages, reference %d; missing %d, extra %d, differing %d"
             % (len(got), len(ref), len(only_ref), len(only_got), len(diff)))
for label, xs in (("missing", only_ref), ("extra", only_got), ("differing", diff)):
    if xs:
        lines.append("   %s: %s%s" % (label, ", ".join(xs[:10]), " ..." if len(xs) > 10 else ""))

# F4, the directory half: a from-scratch render never creates an empty directory,
# so the trees are only equal if pruning removed the ones it emptied.
def dirs(p):
    return {os.path.relpath(r, p) for r, _d, _f in os.walk(p)}
di_, dr_ = dirs(os.path.join(W, "inc", "pages")), dirs(os.path.join(W, "ref-pages"))
lines.append("F4 directory sets equal: %s%s"
             % (di_ == dr_, "" if di_ == dr_ else "  extra dirs: %s" % sorted(di_ - dr_)[:5]))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage5f — the deletion path"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "target            APFS clone of /Users/haruka/dev/lean-projects, own modules rebuilt in place"
  echo "deleted           $DEL (leaf: imported only by the root, no referrers)"
  echo
  echo "## F0 — the baseline is a fixed point"
  cat "$OUT/f0.txt"
  echo
  echo "## F1 — what the ledger reports"
  cat "$OUT/f1.txt"
  echo
  echo "## F2..F5"
  cat "$OUT/compare.txt"
  echo
  echo "## pipeline timings"
  python3 -m json.tool "$W/inc/timings.json" 2>/dev/null | head -24
} > "$RESULTS/stage5f-deletion.txt"
echo "-> $RESULTS/stage5f-deletion.txt"
