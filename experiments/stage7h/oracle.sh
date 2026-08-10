#!/usr/bin/env bash
# Stage 7h — the oracle for the cached build, at the unit level.
#
# The contract `stage5/global.ts` states in its header is that an incrementally
# updated artifact equals a from-scratch one. Here that is checked directly:
# `stage5/global.ts build` is run over each IR state as the from-scratch side,
# and `stage7h/global.ts build --state` is run over the *same* states in sequence
# carrying its cache forward. Every one of the six artifacts must be byte-equal,
# every time.
#
# The end-to-end check (a real move, a real `lake build`, the whole site compared
# against a full re-extraction) is in `run.sh`; this one exists because it can
# reach the cases that scenario does not contain — a module deleted, a module
# added, a module whose bytes changed — and because when it fails it says which.
#
# The IR mutations are synthetic and are *not* claimed to be what the extractor
# would emit. They only have to be a valid IR: `global.ts` reads `module`,
# `imports`, `tactics`, `moduleDocs` and `declarations`, and `contentHash` out of
# the index. A mutation changes the bytes and the hash together, which is the
# invariant the cache is keyed on.
#
# usage: oracle.sh <work-dir>   (expects <work-dir>/ir to be a schema-4 IR tree)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
DIFF=/usr/bin/diff   # `diff` is aliased to a colordiff that is not installed here.
W="${1:?work dir}"
O="$W/oracle"
rm -rf "$O"
mkdir -p "$O"
FAIL=0

step () { # step <name> <ir-dir>
  local name="$1" ir="$2"
  rm -rf "$O/from-scratch-$name" "$O/cached-$name"
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$ir" \
    --out "$O/from-scratch-$name" > "$O/from-scratch-$name.log"
  deno run --allow-read --allow-write "$HERE/global.ts" build --ir "$ir" \
    --out "$O/cached-$name" --state "$O/state" --timings "$O/cached-$name.json" \
    > "$O/cached-$name.log"
  local hits misses
  hits=$(python3 -c "import json;print(json.load(open('$O/cached-$name.json'))['cacheHits'])")
  misses=$(python3 -c "import json;print(json.load(open('$O/cached-$name.json'))['cacheMisses'])")
  if "$DIFF" -r "$O/from-scratch-$name" "$O/cached-$name" > "$O/diff-$name.txt" 2>&1; then
    printf '  %-14s cache %3s hit / %3s miss   6 artifacts byte-identical\n' "$name" "$hits" "$misses"
  else
    printf '  %-14s cache %3s hit / %3s miss   DIFFERS:\n' "$name" "$hits" "$misses"
    head -10 "$O/diff-$name.txt" | sed 's/^/      /'
    FAIL=1
  fi
}

echo "### stage7h oracle — cached build vs from-scratch build, over a sequence of IR states"

# 1. the unchanged tree, with no state at all: the cache must not be needed for
#    correctness, only for speed.
step base "$W/ir"
# 2. the same tree again: every module is a hit, and the bytes must not move.
step rerun "$W/ir"

# 3. one module's bytes change (a declaration renamed, the hash moved with it).
cp -R "$W/ir" "$O/ir-mod"
python3 - "$O/ir-mod" <<'PY'
import json, os, sys
ir = sys.argv[1]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
e = sorted(idx["modules"], key=lambda x: -x["declarations"])[0]
p = os.path.join(ir, e["file"])
m = json.load(open(p, encoding="utf-8"))
m["declarations"][0]["name"] += "_renamed"
m["declarations"][0]["doc"] = "mentions `Nat.find` and `" + m["declarations"][-1]["name"] + "`"
json.dump(m, open(p, "w", encoding="utf-8"))
e["contentHash"] = "0000" + e["contentHash"][4:]
json.dump(idx, open(os.path.join(ir, "index.json"), "w", encoding="utf-8"))
print("  modified %s (%d declarations)" % (e["module"], e["declarations"]))
PY
step modified "$O/ir-mod"

# 4. two modules disappear. The index is the authority on what exists, so the
#    cache entries have to go with them — including out of `importedBy`, which is
#    the field a stale entry would show up in first.
cp -R "$O/ir-mod" "$O/ir-del"
python3 - "$O/ir-del" <<'PY'
import json, os, sys
ir = sys.argv[1]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
own = {e["module"] for e in idx["modules"]}
# Delete leaves only: a module nobody imports, so the tree stays consistent.
imported = set()
for e in idx["modules"]:
    m = json.load(open(os.path.join(ir, e["file"]), encoding="utf-8"))
    imported.update(i for i in m["imports"] if i in own)
leaves = [e for e in idx["modules"] if e["module"] not in imported]
gone = leaves[:2]
for e in gone:
    os.remove(os.path.join(ir, e["file"]))
idx["modules"] = [e for e in idx["modules"] if e not in gone]
idx["moduleCount"] = len(idx["modules"])
json.dump(idx, open(os.path.join(ir, "index.json"), "w", encoding="utf-8"))
print("  removed %s" % ", ".join(e["module"] for e in gone))
PY
step removed "$O/ir-del"

# 5. a module appears. Its declarations are new names, so it moves the name map,
#    the instance index and the navbar at once.
cp -R "$O/ir-del" "$O/ir-add"
python3 - "$O/ir-add" <<'PY'
import json, os, sys
ir = sys.argv[1]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
src = sorted(idx["modules"], key=lambda x: -x["declarations"])[1]
m = json.load(open(os.path.join(ir, src["file"]), encoding="utf-8"))
new = m["module"] + "Added"
m["module"] = new
for d in m["declarations"]:
    d["name"] = d["name"] + "_added"
m["imports"] = m["imports"] + [src["module"]]
body = json.dumps(m)
f = "modules/%s.json" % new
open(os.path.join(ir, f), "w", encoding="utf-8").write(body)
idx["modules"].append({"bytes": len(body.encode()), "contentHash": "addedaddedadded1",
                       "declarations": len(m["declarations"]), "file": f, "module": new})
idx["moduleCount"] = len(idx["modules"])
json.dump(idx, open(os.path.join(ir, "index.json"), "w", encoding="utf-8"))
print("  added %s (%d declarations)" % (new, len(m["declarations"])))
PY
step added "$O/ir-add"

# 6. back to the original tree with a state that has seen all of the above. A
#    cache that only ever grows would pass every step so far and fail this one.
step restored "$W/ir"

# 7. a state written by a different derivation must be discarded, not trusted.
python3 - "$O/state/global-state.json" <<'PY'
import json, sys
p = sys.argv[1]
st = json.load(open(p, encoding="utf-8"))
st["derivation"] = "some older rule"
json.dump(st, open(p, "w", encoding="utf-8"))
PY
step stale-state "$W/ir"
MISSES=$(python3 -c "import json;print(json.load(open('$O/cached-stale-state.json'))['cacheMisses'])")
if [ "$MISSES" -eq 0 ]; then
  echo "  !! a state with a foreign derivation id was used as a cache"
  FAIL=1
else
  echo "  stale-state   the foreign state was dropped whole ($MISSES misses)"
fi

echo
if [ "$FAIL" = 0 ]; then
  echo "oracle: PASS"
else
  echo "oracle: FAIL"
  exit 1
fi
