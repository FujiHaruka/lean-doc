#!/usr/bin/env bash
# Stage 6c — N0..N6: how general is L3-1?
#
# One scenario per kind of moved declaration (instance / @[simp] lemma / abbrev),
# each run on a freshly reset clone, each judged by byte equality of the whole page
# tree with a from-scratch post-move build. `--l3-1 off` runs alongside so that
# "L3-1 was necessary" is measured rather than assumed — for the instance case the
# prediction is that it is *not*.
#
# WHY WHOLE MODULES ARE MOVED, NOT SINGLE DECLARATIONS
#   Cutting one declaration out of a Lean file needs its namespace and `open`
#   context reconstructed, which is source surgery this experiment would then be
#   testing instead of L3-1. So the census picks a module A whose *referenced*
#   declarations are of the kind under test, and the whole module moves. The
#   attribution is then still to the kind, because nothing else of A's is pointed
#   at from outside.
#
# WHY THE BASE IS REBUILT FOR EVERY SCENARIO
#   `setup-clone.sh reset` rebuilds the reverted modules, and a rebuilt olean is
#   only byte-identical to the previous one if the build is deterministic. Rather
#   than assume that, each scenario re-extracts its own base and checks that the
#   ledger reports a fixed point before the move. Costs ~30 s per scenario and
#   removes a whole class of confounds (stage 5e lost a run to exactly this).
#
# usage: run.sh <work-dir> <clone-dir> [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
S5E="$LD/experiments/stage5e"
W=${1:?work dir}
CLONE=${2:?clone dir}
RESULTS="$LD/benchmarks/results"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
render () { deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }
modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
}

# --------------------------------------------------------------- reset and base
reset_clone () {
  echo "### resetting the clone"
  "$S5E/setup-clone.sh" reset "$CLONE" > "$W/reset.log" 2>&1 || {
    echo "reset failed" >&2; tail -20 "$W/reset.log" >&2; exit 1; }
}

build_base () { # build_base <tag>
  local tag="$1"
  rm -rf "$W/base-ir-$tag" "$W/base-pages-$tag"
  modlist "$W/modules-before-$tag.txt"
  "$S5/extract-once.sh" --modules "$W/modules-before-$tag.txt" \
    --ir-dir "$W/base-ir-$tag" --timings "$W/base-extract-$tag.json" \
    > "$W/base-extract-$tag.log"
  render "$W/base-ir-$tag" "$W/base-pages-$tag"
  deno_ "$S5/ledger.ts" build --modules "$W/modules-before-$tag.txt" --target "$CLONE" \
    --ir "$W/base-ir-$tag" --source-url "$URL" --algorithm lake \
    --out "$W/base-ledger-$tag.json" > /dev/null
  deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger-$tag.json" --ir "$W/base-ir-$tag" \
    --source-url "$URL" --modules "$W/modules-before-$tag.txt" --changed-out /dev/null \
    --removed-out /dev/null --render-all-out /dev/null > "$W/fixpoint-$tag.txt"
  grep -q ": 0 changed, 0 added, 0 removed" "$W/fixpoint-$tag.txt" || {
    echo "base $tag is not a fixed point:" >&2; cat "$W/fixpoint-$tag.txt" >&2; exit 3; }
  echo "  base $tag: fixed point confirmed"
}

# ------------------------------------------------------------------- N0 census
reset_clone
build_base census

echo "### N0 — the census: one module to move per kind, or none"
python3 - "$W/base-ir-census" "$CLONE" "$W/census.json" "$OUT/n0.txt" <<'PY'
import json, os, re, subprocess, sys, collections
ir, clone, out_json, out_txt = sys.argv[1:]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
mods = {}
for e in idx["modules"]:
    m = json.load(open(os.path.join(ir, e["file"]), encoding="utf-8"))
    mods[m["module"]] = m

# owner -> {name: kind/modifiers} and the reverse reference index
kind_of, mods_of_name = {}, {}
for name, m in mods.items():
    for d in m["declarations"]:
        kind_of[(name, d["name"])] = (d["kind"], tuple(d.get("modifiers", [])))
        mods_of_name[d["name"]] = name

# who refers to what, from printed signatures only
refs_in = collections.defaultdict(set)   # owner module -> {(referring module, name)}
for name, m in mods.items():
    for d in m["declarations"]:
        for owner, n in d["refs"]:
            if owner != name and owner in mods:
                refs_in[owner].add((name, n))

# `@[simp]` is not in the IR (the extractor drops attributes on purpose,
# Extract.lean:730), so it is read from the sources.
simp_names = set()
for root, _d, fs in os.walk(clone):
    if ".lake" in root:
        continue
    for f in fs:
        if not f.endswith(".lean"):
            continue
        txt = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
        for mo in re.finditer(r"@\[([^\]]*)\]\s*(?:private\s+|protected\s+|nonrec\s+)*"
                              r"(?:theorem|lemma|def|abbrev)\s+([A-Za-z_][^\s\(\{\[:]*)",
                              txt):
            if "simp" in [a.strip() for a in mo.group(1).split(",")]:
                simp_names.add(mo.group(2))

lines, chosen = [], {}

def report(kind, cands, why):
    lines.append("")
    lines.append("### %s" % kind)
    if not cands:
        lines.append("  NO WITNESS on this target — %s. Reported as untestable." % why)
        return
    for mod, detail in cands[:5]:
        lines.append("  %-70s %s" % (mod, detail))
    chosen[kind] = cands[0][0]
    lines.append("  chosen: %s" % cands[0][0])

# --- instance: A defines an instance. Instances are *not* named in printed
# signatures (typeclass resolution finds them), so the interesting A is one whose
# instances exist at all; refs-referrers are reported but not required.
inst = []
for name, m in mods.items():
    insts = [d["name"] for d in m["declarations"] if d["kind"] == "instance"]
    if not insts:
        continue
    named = [n for (_rm, n) in refs_in.get(name, ()) if n in insts]
    inst.append((name, "%d instance(s), %d of them named in a signature elsewhere, "
                       "%d referring module(s) overall"
                 % (len(insts), len(named), len({rm for rm, _ in refs_in.get(name, ())}))))
# prefer a module with instances AND at least one referring module, so the page
# tree can differ at all; among those prefer the fewest signature-named instances,
# which isolates the instances-index path.
inst.sort(key=lambda t: (-int(t[1].split()[0]), t[0]))
inst = [c for c in inst if "0 referring module(s)" not in c[1]] or inst
report("instance", inst, "no module defines an instance")

# --- simp: A defines a @[simp] declaration that another module names in a signature
simp = []
for name, m in mods.items():
    own = {d["name"] for d in m["declarations"]}
    simp_here = {n for n in own if n.split(".")[-1] in simp_names or n in simp_names}
    if not simp_here:
        continue
    hits = [(rm, n) for (rm, n) in refs_in.get(name, ()) if n in simp_here]
    if hits:
        simp.append((name, "%d @[simp] decl(s), referenced from %d module(s), e.g. %s"
                     % (len(simp_here), len({rm for rm, _ in hits}), hits[0][1])))
simp.sort(key=lambda t: (-int(t[1].split()[0]), t[0]))
report("simp", simp, "no @[simp] declaration is named in another module's signature")

# --- abbrev: A defines an abbrev that another module names in a signature
abb = []
for name, m in mods.items():
    own = {d["name"] for d in m["declarations"] if "abbrev" in d.get("modifiers", [])}
    if not own:
        continue
    hits = [(rm, n) for (rm, n) in refs_in.get(name, ()) if n in own]
    if hits:
        abb.append((name, "%d abbrev(s), referenced from %d module(s), e.g. %s"
                    % (len(own), len({rm for rm, _ in hits}), hits[0][1])))
    else:
        abb.append((name, "%d abbrev(s), referenced from 0 module(s)" % len(own)))
abb = [c for c in abb if "from 0 module(s)" not in c[1]] or []
report("abbrev", abb, "no abbrev is named in another module's signature")

json.dump(chosen, open(out_json, "w", encoding="utf-8"), indent=2)
lines.insert(0, "N0 kinds with a witness: %s" % (", ".join(sorted(chosen)) or "(none)"))
open(out_txt, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# --------------------------------------------------------------- the scenarios
: > "$W/scenarios.txt"
scenario () { # scenario <kind> <module>
  local kind="$1" A="$2" X="${2}Core"
  echo "### scenario $kind — moving $A"
  reset_clone
  build_base "$kind"
  "$S5E/setup-clone.sh" move "$CLONE" "$A" minimal > "$W/move-$kind.log" 2>&1 || {
    echo "  move failed; see $W/move-$kind.log"; tail -10 "$W/move-$kind.log"; return 1; }
  modlist "$W/modules-after-$kind.txt"
  rm -rf "$W/ref-ir-$kind" "$W/ref-pages-$kind"
  "$S5/extract-once.sh" --modules "$W/modules-after-$kind.txt" \
    --ir-dir "$W/ref-ir-$kind" --timings "$W/ref-extract-$kind.json" \
    > "$W/ref-extract-$kind.log"
  render "$W/ref-ir-$kind" "$W/ref-pages-$kind"
  for l31 in on off; do
    local d="$W/$kind-$l31"
    rm -rf "$d"; mkdir -p "$d"
    cp -R "$W/base-ir-$kind" "$d/ir"
    cp -R "$W/base-pages-$kind" "$d/pages"
    cp "$W/base-ledger-$kind.json" "$d/ledger.json"
    "$S5/incremental.sh" --module "$A" --ir "$d/ir" --pages "$d/pages" \
      --ledger "$d/ledger.json" --modules "$W/modules-after-$kind.txt" \
      --source-url "$URL" --work "$d/work" --mode self --l3-1 "$l31" \
      --timings "$d/timings.json" > "$OUT/$kind-$l31.txt" 2>&1 || {
        echo "  $kind/$l31 failed:"; tail -10 "$OUT/$kind-$l31.txt"; }
  done
  echo "$kind $A" >> "$W/scenarios.txt"
}

for kind in instance simp abbrev; do
  A=$(python3 -c "
import json
c = json.load(open('$W/census.json'))
print(c.get('$kind', ''))")
  if [ -z "$A" ]; then
    echo "### scenario $kind — skipped, no witness on this target"
    continue
  fi
  scenario "$kind" "$A" || true
done

# ------------------------------------------------------------------- N1..N6
python3 - "$W" "$OUT/verdict.txt" <<'PY'
import json, os, sys
W, out = sys.argv[1:]

def tree(p):
    d = {}
    for r, _dd, fs in os.walk(p):
        for f in fs:
            full = os.path.join(r, f)
            d[os.path.relpath(full, p)] = open(full, "rb").read()
    return d

scen = [l.split() for l in open(os.path.join(W, "scenarios.txt"), encoding="utf-8")
        if l.strip()]
lines = []
for kind, A in scen:
    ref = tree(os.path.join(W, "ref-pages-" + kind))
    lines.append("")
    lines.append("## %s — moved %s (REFERENCE: %d files)" % (kind, A, len(ref)))
    verdict = {}
    for l31 in ("on", "off"):
        d = os.path.join(W, "%s-%s" % (kind, l31))
        got = tree(os.path.join(d, "pages"))
        missing = sorted(set(ref) - set(got)); extra = sorted(set(got) - set(ref))
        diff = sorted(k for k in set(ref) & set(got) if ref[k] != got[k])
        ok = not (missing or extra or diff)
        verdict[l31] = ok
        t = json.load(open(os.path.join(d, "timings.json"), encoding="utf-8"))
        lines.append("  --l3-1 %-3s : %d files, missing %d, extra %d, differing %d -> %s"
                     % (l31, len(got), len(missing), len(extra), len(diff),
                        "byte-identical" if ok else "WRONG"))
        lines.append("             rounds %d, changed %d, staleFound %d, globalStale %d, pages %d"
                     % (t["rounds"], t["changed"], t["staleFound"], t["globalStale"],
                        t["pagesRendered"]))
        if diff:
            lines.append("             differing: " + ", ".join(diff[:6])
                         + (" ..." if len(diff) > 6 else ""))
        # N6: the round-2 ownership report
        for r in (2, 3, 4, 5):
            p = os.path.join(d, "work", "ownership-%d.json" % r)
            if not os.path.exists(p):
                continue
            o = json.load(open(p, encoding="utf-8"))
            lines.append("             ownership round %d: lostNames %d, gainedNames %d, "
                         "stale %d %s"
                         % (r, o["lostNames"], o["gainedNames"], o["stale"],
                            "(the structural bound holds)"
                            if o["lostNames"] == 0 and o["gainedNames"] == 0
                            else "<- NON-ZERO: the two-round bound is not structural"))
    lines.append("  N-verdict: L3-1 on is %s; L3-1 off is %s -> L3-1 %s for this kind"
                 % ("correct" if verdict.get("on") else "WRONG",
                    "correct" if verdict.get("off") else "wrong",
                    "is NOT what saves it" if verdict.get("on") and verdict.get("off")
                    else "is necessary" if verdict.get("on") and not verdict.get("off")
                    else "is INSUFFICIENT"))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage6c — how general is L3-1?"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "target            APFS clone of /Users/haruka/dev/lean-projects, reset before each scenario"
  echo "oracle            byte equality of the whole page tree with a from-scratch post-move build"
  echo
  echo "## N0 — the census"
  cat "$OUT/n0.txt"
  echo
  echo "## N1..N6 — one scenario per kind"
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage6c-l31-generality.txt"
echo "-> $RESULTS/stage6c-l31-generality.txt"
