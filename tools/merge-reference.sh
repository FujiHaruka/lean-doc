#!/usr/bin/env bash
# Run the `ownership` and `merge` stages over the measurement target's IR and
# record everything they write.
#
# **The scenarios are defined once and run by either implementation** (--impl),
# as tools/ledger-reference.sh does and for the same reason: what has to match is
# the answer to a *question*, and a question asked slightly differently on the
# two sides would compare two things nobody meant to compare.
#
# **One script for both stages, on purpose.** A round of the incremental pipeline
# is `extract -> ownership -> merge` over one edit (plan §6, constraint 1), so the
# two stages share every scenario. Splitting them would mean writing each edit
# down twice, which is the failure this file exists to avoid.
#
# The edits are injected into the IR, never into the measurement target: the
# fixtures below are partial extraction trees built out of the base IR's own
# module files, which is what the extractor would have written for the same
# module list. The `contentHash` of an edited module is **fabricated** (FNV-1a of
# the file, marked in the fixture manifest) — only Lean can compute the real one,
# and every consumer of it only ever compares it for equality.
#
# usage: tools/merge-reference.sh [--impl ts|rust] [--out DIR] [--base-ir DIR]

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWNERSHIP_TS="$REPO/experiments/stage5/ownership.ts"
MERGE_TS="$REPO/experiments/stage5/merge-ir.ts"
RUST_BIN="$REPO/target/release/lean-doc"

IMPL=ts
OUT=
BASE_IR=/private/tmp/lean-doc-relay/w7h/base-ir

while [ $# -gt 0 ]; do
  case "$1" in
    --impl) IMPL="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --base-ir) BASE_IR="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$IMPL" in
  ts) OUT="${OUT:-/private/tmp/lean-doc-relay/m3b/ref}" ;;
  rust) OUT="${OUT:-/private/tmp/lean-doc-relay/m3b/rust}" ;;
  *) echo "--impl wants ts or rust, not $IMPL" >&2; exit 2 ;;
esac

[ -d "$BASE_IR" ] || { echo "missing: $BASE_IR" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }

if [ "$IMPL" = ts ]; then
  command -v deno >/dev/null || { echo "deno is required (node is broken here)" >&2; exit 1; }
  for f in "$OWNERSHIP_TS" "$MERGE_TS"; do
    [ -f "$f" ] || { echo "missing: $f" >&2; exit 1; }
  done
  ownership () { deno run --allow-read --allow-write "$OWNERSHIP_TS" "$@"; }
  merge ()     { deno run --allow-read --allow-write "$MERGE_TS" "$@"; }
else
  [ -x "$RUST_BIN" ] || {
    echo "missing: $RUST_BIN — run: cargo build --release -p lean-doc" >&2; exit 1;
  }
  ownership () { "$RUST_BIN" ownership "$@"; }
  merge ()     { "$RUST_BIN" merge "$@"; }
fi

rm -rf "$OUT"
mkdir -p "$OUT/fixtures"

# --- the edits -------------------------------------------------------------
# One module that 49 others reference through a `(module, name)` pair, one leaf
# nobody references which is also the only user of the `Lean` dependency slice,
# and the names of the modules the moves invent.
OWNER=InformationTheory.Shannon.Bridge
MOVED_NAME=InformationTheory.Shannon.entropy
NEW_HOME=InformationTheory.Shannon.BridgeMoved
ADDED=InformationTheory.Shannon.BridgeAddendum
LEAF=InformationTheory.Meta.EntryPoint

python3 - "$BASE_IR" "$OUT/fixtures" "$OWNER" "$MOVED_NAME" "$NEW_HOME" "$ADDED" "$LEAF" <<'PY'
"""Build the partial-extraction trees the scenarios feed to the two stages.

Deterministic: the same base IR always produces the same fixtures, byte for
byte, so both implementations really are answering the same question. The
comparator checks that by diffing these files too.
"""
import json, os, sys

base, out, owner, moved_name, new_home, added, leaf = sys.argv[1:8]

def read(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)

def compact(value):
    # Lean's `Json.compress` with `Json.mkObj`'s sorted keys: what the extractor
    # writes, so the fixtures are shaped like a real partial run.
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False, sort_keys=True)

def fnv1a64(data: bytes) -> str:
    h = 0xcbf29ce484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"

index = read(os.path.join(base, "index.json"))
by_module = {e["module"]: e for e in index["modules"]}

def module_json(name):
    return read(os.path.join(base, by_module[name]["file"]))

def write_tree(name, modules, hashes=None):
    """One partial extraction: index.json + modules/, and the own-package
    dependency slice a one-module run wrongly produces (merge has to ignore it)."""
    root = os.path.join(out, name)
    os.makedirs(os.path.join(root, "modules"), exist_ok=True)
    os.makedirs(os.path.join(root, "deps"), exist_ok=True)
    entries = []
    for module, body in modules:
        text = compact(body)
        raw = text.encode("utf-8")
        rel = f"modules/{module}.json"
        with open(os.path.join(root, rel), "wb") as f:
            f.write(raw)
        entries.append({
            "module": module,
            "file": rel,
            "bytes": len(raw),
            "declarations": len(body["declarations"]),
            # Fabricated: only Lean computes the real one, and everything
            # downstream only compares it for equality.
            "contentHash": (hashes or {}).get(module) or fnv1a64(raw),
        })
    # The misfiled own-package slice: with a one-module target list the
    # extractor calls the package's other modules dependencies.
    slice_body = {"schemaVersion": index["schemaVersion"], "package": "InformationTheory",
                  "declarations": {moved_name: owner}}
    slice_text = compact(slice_body)
    with open(os.path.join(root, "deps/InformationTheory.json"), "w", encoding="utf-8") as f:
        f.write(slice_text)
    partial = {
        "schemaVersion": index["schemaVersion"],
        "generator": index["generator"],
        "leanVersion": index["leanVersion"],
        "hashAlgorithm": index["hashAlgorithm"],
        "moduleCount": len(entries),
        "declarationCount": sum(e["declarations"] for e in entries),
        "modules": entries,
        "dependencyMaps": [{
            "package": "InformationTheory",
            "file": "deps/InformationTheory.json",
            "entries": 1,
            "bytes": len(slice_text.encode("utf-8")),
        }],
    }
    with open(os.path.join(root, "index.json"), "w", encoding="utf-8") as f:
        f.write(compact(partial))

# 1. rerun: the same module, the same bytes, the same hash. Nothing changed.
write_tree("inc-rerun", [(owner, module_json(owner))],
           hashes={owner: by_module[owner]["contentHash"]})

# 2. modified: the same declaration names, different bytes. The IR hash moves,
#    nothing moved owner.
edited = module_json(owner)
edited["declarations"][0]["doc"] = "Injected edit: the bytes move, the names do not.\n"
write_tree("inc-modified", [(edited["module"], edited)])

# 3. moved: one declaration leaves `owner` for a module the base IR has never
#    seen. This is the case no olean hash can see (stage 5c).
shrunk = module_json(owner)
carried = [d for d in shrunk["declarations"] if d["name"] == moved_name]
assert len(carried) == 1, f"{moved_name} is not in {owner}"
shrunk["declarations"] = [d for d in shrunk["declarations"] if d["name"] != moved_name]
new_module = {"schemaVersion": shrunk["schemaVersion"], "module": new_home,
              "imports": shrunk["imports"], "moduleDocs": [], "tactics": [],
              "declarations": [dict(carried[0], index=0)]}
write_tree("inc-moved", [(owner, shrunk), (new_home, new_module)])

# 3b. the same move with only the *new* home re-extracted — the build order the
#     second rule exists for. Nothing was lost (the old owner was not looked at),
#     so a referrer is only caught by "that name now lives somewhere else".
write_tree("inc-gained", [(new_home, new_module)])

# 4. added: a module with no ownership history at all, carrying a reference to a
#    dependency name the package does not otherwise mention — so the dependency
#    slice grows by exactly one entry.
grown = {"schemaVersion": index["schemaVersion"], "module": added,
         "imports": ["Mathlib.Order.Basic"], "moduleDocs": [], "tactics": [],
         "declarations": [{
             "name": f"{added}.witness", "kind": "theorem", "modifiers": [],
             "binders": [], "implicits": [], "binderCode": [],
             "type": "True", "typeCode": [], "line": 1, "col": 0,
             "endLine": 1, "endCol": 20, "index": 0, "members": [],
             "doc": None, "equations": [], "equationCode": [],
             "refs": [["Mathlib.Order.Basic", "Mathlib.LeanDocFixtureOnly"],
                      ["Init.Core", "trivial"]],
         }]}
write_tree("inc-added", [(added, grown)])

# 5. restored: the leaf module comes back exactly as it was, after having been
#    deleted in the round before.
write_tree("inc-restored", [(leaf, module_json(leaf))],
           hashes={leaf: by_module[leaf]["contentHash"]})

# --- the lists the two stages take
with open(os.path.join(out, "removed-leaf.txt"), "w", encoding="utf-8") as f:
    f.write(leaf + "\n")
# Three of the modules that reference the moved name, so that `--exclude` has
# something to exclude that is not already fresh.
excluded = []
for entry in index["modules"]:
    if entry["module"] in (owner, new_home):
        continue
    body = read(os.path.join(base, entry["file"]))
    if any([owner, moved_name] in [list(r) for r in d["refs"]] for d in body["declarations"]):
        excluded.append(entry["module"])
    if len(excluded) == 3:
        break
assert len(excluded) == 3, "the base IR does not reference the moved name three times"
with open(os.path.join(out, "exclude-three.txt"), "w", encoding="utf-8") as f:
    f.write("# modules an earlier round already took\n" + "\n".join(excluded) + "\n")
with open(os.path.join(out, "manifest.json"), "w", encoding="utf-8") as f:
    json.dump({"owner": owner, "movedName": moved_name, "newHome": new_home,
               "added": added, "leaf": leaf, "excluded": excluded,
               "contentHash": "fabricated (FNV-1a of the module file) except where copied"},
              f, indent=2, sort_keys=True)
    f.write("\n")
PY

FIX="$OUT/fixtures"

# --- one round of the pipeline ----------------------------------------------
# `ownership` before `merge`, always: merge overwrites the base IR's idea of who
# owns each name (plan §6, constraint 1).
round () { # round <name> <ir> <out ir> [--inc DIR] [--removed FILE] [--exclude FILE]
  local name="$1" ir="$2" merged="$3"; shift 3
  # bash 3.2: an empty array is unset under `set -u`, hence the `${a[@]+...}`.
  local inc=() own_del=() mrg_del=() exclude=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --inc) inc=(--inc "$2"); shift 2 ;;
      --removed) own_del=(--removed "$2"); mrg_del=(--remove "$2"); shift 2 ;;
      --exclude) exclude=(--exclude "$2"); shift 2 ;;
      *) echo "round: unknown argument $1" >&2; exit 2 ;;
    esac
  done
  ownership --base "$ir" ${inc[@]+"${inc[@]}"} ${own_del[@]+"${own_del[@]}"} \
    ${exclude[@]+"${exclude[@]}"} \
    --print-set "$OUT/$name-stale.txt" --json "$OUT/$name-ownership.json" \
    > "$OUT/$name-ownership-stdout.txt"
  merge --base "$ir" ${inc[@]+"${inc[@]}"} --out "$merged" \
    ${mrg_del[@]+"${mrg_del[@]}"} \
    --changed-out "$OUT/$name-changed.txt" \
    --timings "$OUT/$name-merge-timings.json" \
    > "$OUT/$name-merge-stdout.txt"
  # A snapshot of what the round computed, taken now: two rounds share one tree
  # (the deletion and the restore), and the second overwrites the first.
  cp "$merged/index.json" "$OUT/$name-index.json"
  cp -R "$merged/deps" "$OUT/$name-deps"
  ( cd "$merged/modules" && find . -type f | wc -l | tr -d ' ' ) > "$OUT/$name-modules.txt"
}

work () { # work <name> -> a fresh in-place copy of the base IR
  local dir="$OUT/$1/ir"
  mkdir -p "$OUT/$1"
  cp -R "$BASE_IR" "$dir"
  printf '%s' "$dir"
}

# 1. a re-extraction that changed nothing: the answer the pipeline sees most.
round rerun    "$(work rerun)"    "$OUT/rerun/ir"    --inc "$FIX/inc-rerun"
# 2. the bytes moved, the names did not.
round modified "$(work modified)" "$OUT/modified/ir" --inc "$FIX/inc-modified"
# 3. the move — the one no olean hash can see — with three referrers excluded.
round moved    "$(work moved)"    "$OUT/moved/ir"    --inc "$FIX/inc-moved" \
                                                     --exclude "$FIX/exclude-three.txt"
# 3b. only the new home re-extracted: the `movedElsewhere` rule, which the
#     `lostOwner` one hides whenever both ends of the move are in the same round.
round gained   "$(work gained)"   "$OUT/gained/ir"   --inc "$FIX/inc-gained"
# 4. the same move written to a *different* tree: the copy branch of merge.
IR_COPY="$(work copyout)"
mkdir -p "$OUT/copyout/merged"
round copyout  "$IR_COPY"         "$OUT/copyout/merged" --inc "$FIX/inc-moved"
# 5. a module with no ownership history, adding one dependency name.
round added    "$(work added)"    "$OUT/added/ir"    --inc "$FIX/inc-added"
# 6. a deletion with nothing re-extracted, which also empties a whole dependency
#    slice: the leaf is the only module in the package that mentions `Lean.*`.
round removed  "$(work removed)"  "$OUT/removed/ir"  --removed "$FIX/removed-leaf.txt"
# 7. …and the deleted module coming back, as a second round on the same tree.
RESTORED="$(work restored)"
round restored-1 "$RESTORED" "$RESTORED" --removed "$FIX/removed-leaf.txt"
round restored-2 "$RESTORED" "$RESTORED" --inc "$FIX/inc-restored"

# --- verify ------------------------------------------------------------------
# Its whole output is the answer, so it is compared byte for byte rather than
# skipped the way a log line is.
verify () { # verify <name> <a> <b>
  local name="$1" a="$2" b="$3" status=0
  merge --verify "$a" --against "$b" > "$OUT/$name-verify.txt" || status=$?
  printf '%s\n' "$status" > "$OUT/$name-verify-status.txt"
}
verify same    "$OUT/rerun/ir"   "$BASE_IR"
verify moved   "$OUT/moved/ir"   "$BASE_IR"
verify deleted "$OUT/removed/ir" "$BASE_IR"

# A manifest makes the tree verifiable later without rerunning anything, and
# makes an accidental edit loud.
( cd "$OUT" && find . -type f | sort | xargs shasum -a 256 ) > "$OUT.sha256"

printf 'impl: %s\n' "$IMPL"
printf 'base IR: %s\n' "$BASE_IR"
printf 'files: %s\n' "$(find "$OUT" -type f | wc -l | tr -d ' ')"
printf 'manifest: %s\n' "$OUT.sha256"
