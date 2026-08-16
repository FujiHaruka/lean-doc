#!/usr/bin/env bash
# Run the `detect` stage over the measurement target and record everything it
# writes: the ledgers, a touched ledger, and the check outputs of twelve
# scenarios.
#
# **Rust only.** Until 2026-08-16 `--impl ts` ran the same twelve scenarios
# against the frozen prototype (`experiments/stage5/ledger.ts`) and
# tools/ledger-compare.sh diffed the two trees. `experiments/` was removed, so
# **that comparison can no longer be made from this tree** — the prototype exists
# only at tag `experiments-frozen`. This script stays the single definition of the
# twelve questions; the comparator now diffs two recordings of it.
#
# Two implementations could never share a ledger: `extractKey.extractor` and
# `renderKey.renderer` are deliberately different strings (plan §6 — a shared
# string would mean "a different implementation with the same key"). The
# comparator's substitution for that is kept, so a tree recorded before the
# removal is still readable.
#
# The target is only ever read: `touch` is what injects "module M changed".
#
# usage: tools/ledger-reference.sh [--out DIR] [--target REPO] [--ir DIR]

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_BIN="$REPO/target/release/lean-doc"

OUT=
TARGET=/Users/haruka/dev/lean-projects
IR=/private/tmp/lean-doc-relay/w7h/base-ir
MODULES="$REPO/benchmarks/results/it-modules.txt"

# The package's own 432 modules have one `.olean` each; the three-file form of
# the module system only appears in the dependencies. Eight of Mathlib's, hashed
# with the dependency package as the target, are how the three-olean case gets
# measured on real files instead of on a synthetic tree.
MATHLIB_MODULES="Mathlib.Init
Mathlib.Logic.Basic
Mathlib.Logic.Denumerable
Mathlib.Order.Basic
Mathlib.Data.Set.Defs
Mathlib.Algebra.Group.Defs
Mathlib.Tactic.Ring.Basic
Mathlib.Topology.Basic"

# The rev is configuration, not a fact about the target: it only has to be a
# 40-hex string, and the second one only has to differ from the first.
URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec
URL2=https://github.com/FujiHaruka/information-theory/blob/0000000000000000000000000000000000000000

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --ir) IR="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

OUT="${OUT:-/private/tmp/lean-doc-relay/m3/rust}"

MATHLIB_TARGET="$TARGET/.lake/packages/mathlib"

for p in "$TARGET" "$IR" "$MODULES" "$MATHLIB_TARGET"; do
  [ -e "$p" ] || { echo "missing: $p" >&2; exit 1; }
done

[ -x "$RUST_BIN" ] || {
  echo "missing: $RUST_BIN — run: cargo build --release -p lean-doc" >&2; exit 1;
}
ledger () { "$RUST_BIN" ledger "$@"; }

rm -rf "$OUT"
mkdir -p "$OUT"

# --- the module lists the drift scenario needs -------------------------------
# Deterministic slices of the committed list, written by both sides so that the
# comparator can check they really are the same lists.
sed -n '3,432p' "$MODULES" > "$OUT/list-minus-ab.txt"
A=$(sed -n '1p' "$MODULES"); B=$(sed -n '2p' "$MODULES"); C=$(sed -n '3p' "$MODULES")
{ sed -n '1,2p' "$MODULES"; sed -n '4,432p' "$MODULES"; echo "InformationTheory.NotAModule.Ghost"; } \
  > "$OUT/list-minus-c-plus-ghost.txt"

# --- the ledgers -------------------------------------------------------------
build () { # build <name> <module list> [extra args...]
  local name="$1" list="$2"; shift 2
  ledger build --modules "$list" --out "$OUT/ledger-$name.json" \
    --timings "$OUT/ledger-$name.json.timings.json" "$@"
}
build sha256   "$MODULES"               --target "$TARGET" --ir "$IR" --source-url "$URL" --algorithm sha256
build lake     "$MODULES"               --target "$TARGET" --ir "$IR" --source-url "$URL" --algorithm lake
build minus-ab "$OUT/list-minus-ab.txt" --target "$TARGET" --ir "$IR" --source-url "$URL" --algorithm sha256
# Eight reads in flight. The ledger's bytes must not depend on the scheduling,
# so this file is compared with ledger-sha256.json as well as across the two
# recordings.
build conc8    "$MODULES"               --target "$TARGET" --ir "$IR" --source-url "$URL" \
                                        --algorithm sha256 --concurrency 8
# No --ir: an extract key with two keys fewer, so that a later check *with*
# --ir exercises the other direction of the union rule.
build noir     "$MODULES"               --target "$TARGET"            --source-url "$URL" --algorithm sha256

printf '%s\n' "$MATHLIB_MODULES" > "$OUT/list-mathlib.txt"
build mathlib-sha256 "$OUT/list-mathlib.txt" --target "$MATHLIB_TARGET" --ir "$IR" --source-url "$URL" --algorithm sha256
build mathlib-lake   "$OUT/list-mathlib.txt" --target "$MATHLIB_TARGET" --ir "$IR" --source-url "$URL" --algorithm lake

# Two injected changes, applied one after the other to the same file: `touch`
# has to be idempotent in shape, not just work once.
ledger touch --ledger "$OUT/ledger-sha256.json" --module "$A" --out "$OUT/ledger-touched.json"
ledger touch --ledger "$OUT/ledger-touched.json" --module "$B" --out "$OUT/ledger-touched.json"

# --- the twelve questions -----------------------------------------------------
check () { # check <name> <ledger> [extra args...]
  local name="$1" led="$2"; shift 2
  ledger check --ledger "$led" \
    --changed-out "$OUT/$name-changed.txt" \
    --removed-out "$OUT/$name-removed.txt" \
    --render-all-out "$OUT/$name-render-all.txt" \
    --timings "$OUT/$name-timings.json" "$@" > "$OUT/$name-stdout.txt"
}

# 1. nothing changed at all: the answer the pipeline sees most often.
check clean       "$OUT/ledger-sha256.json"  --modules "$MODULES" --ir "$IR" --source-url "$URL"
# 2. two injected changes.
check touched     "$OUT/ledger-touched.json" --modules "$MODULES" --ir "$IR" --source-url "$URL"
# 3. two modules appeared, one left the list, one is in the list with no olean.
check drift       "$OUT/ledger-minus-ab.json" --modules "$OUT/list-minus-c-plus-ghost.txt" \
                                              --ir "$IR" --source-url "$URL"
# 4. the extract key lost two keys (no --ir): everything is re-extracted.
check extractkey  "$OUT/ledger-sha256.json"  --modules "$MODULES" --source-url "$URL"
# 5. the render key changed value: re-render everything, re-extract nothing.
check rendervalue "$OUT/ledger-sha256.json"  --modules "$MODULES" --ir "$IR" --source-url "$URL2"
# 6. the render key lost a key (no --source-url): the union rule, not the
#    intersection one, is what makes this a change.
check renderless  "$OUT/ledger-sha256.json"  --modules "$MODULES" --ir "$IR"
# 7. the lake algorithm reads Lake's hash instead of the olean bytes.
check lake        "$OUT/ledger-lake.json"    --modules "$MODULES" --ir "$IR" --source-url "$URL"
# 8. no --modules: the list comes from the ledger, and nothing can be added.
check fromledger  "$OUT/ledger-sha256.json"                       --ir "$IR" --source-url "$URL"
# 9-10. modules with all three olean files, on both algorithms.
check mathlib     "$OUT/ledger-mathlib-sha256.json" --modules "$OUT/list-mathlib.txt" \
                                              --ir "$IR" --source-url "$URL"
check mathliblake "$OUT/ledger-mathlib-lake.json" --modules "$OUT/list-mathlib.txt" \
                                              --ir "$IR" --source-url "$URL"
# 11. the extract key gained two keys (a ledger built without --ir, checked with
#     one): the union rule from the other side.
check intoir      "$OUT/ledger-noir.json"    --modules "$MODULES" --ir "$IR" --source-url "$URL"
# 12. a --source-url with a trailing slash is the same render key: the strip
#     happens, and nothing is re-rendered.
check slash       "$OUT/ledger-sha256.json"  --modules "$MODULES" --ir "$IR" --source-url "$URL/"

# A manifest makes the tree verifiable later without rerunning anything, and
# makes an accidental edit loud.
( cd "$OUT" && find . -type f | sort | xargs shasum -a 256 ) > "$OUT.sha256"

printf 'dropped for the drift scenario: %s / %s (added), %s (removed)\n' "$A" "$B" "$C"
printf 'files: %s\n' "$(find "$OUT" -type f | wc -l | tr -d ' ')"
printf 'manifest: %s\n' "$OUT.sha256"
