#!/usr/bin/env bash
# End to end, on a machine that has never seen the measurement target.
#
# WHAT THIS COVERS THAT `cargo test` CANNOT
#   Every test under `crates/lean-doc/tests/` fakes the extractor with a
#   `/bin/sh` script that copies a baked IR tree — deliberately, because needing
#   a Lean toolchain would mean those tests were never run. The cost is that the
#   **contract between the extractor and the Rust side is not checked by
#   anything**: change what `Extract.lean` writes and every one of them stays
#   green. This script is the one place where a real Lean environment produces a
#   real IR and the real pipeline turns it into a real site.
#
# WHY THE FIXTURE IS TINY AND MATHLIB-FREE
#   `e2e/micro` depends on Lean core and nothing else, so `lake build` takes
#   about a second and this runs on a free CI runner. The measurement target
#   pulls in all of Mathlib and can never be what a push is judged by.
#
#   It also holds, on purpose, the declaration shapes the target does not
#   contain — `class`, `inductive`, `class inductive`, a non-`mk` constructor,
#   an inherited field, an implicit binder on a field, an astral identifier
#   (U+1D49C, the U1/U2 traps), scoped notation. Nine of the renderer's 41
#   branches never fire over the real package (crates/lean-doc-render/tests/
#   page_parts.rs), and the first run of this script found one of them silently
#   rendering nothing: an inductive's constructors were missing from their page
#   while the search index still linked to them.
#
# THE THREE GATES
#   1 ONE COMMAND    `lean-doc build` over the fixture writes a site, and
#                    `tools/site-gate.sh` finds it internally consistent:
#                    0 dead links, 0 external resources, and the search index
#                    and the pages agreeing in both directions.
#   2 IDEMPOTENCE    the same command again, nothing changed: 0 modules
#                    re-extracted, 0 pages rendered, and not one byte of the
#                    site different. The incremental path and the full path have
#                    to agree about a world that did not move.
#   3 DETERMINISM    a *second* full build into a different directory is byte
#                    identical to the first. Anything that leaks a hash order, a
#                    timestamp or a path into the output breaks here — and this
#                    is the invariant that replaces an external oracle, because
#                    it needs nobody's opinion about what the bytes should be.
#
# usage: e2e-micro.sh [--out DIR] [--extractor BIN] [--keep]
#   --out        where to build (default: a temporary directory)
#   --extractor  a prebuilt extractor binary (default: build one into
#                e2e/micro/.lake/e2e-extract, which is gitignored)
#   --keep       do not delete a temporary --out on success
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
FIXTURE="$ROOT/e2e/micro"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
LEAN_DOC="${LEAN_DOC:-$ROOT/target/debug/lean-doc}"

OUT=""
EXTRACTOR=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --extractor) EXTRACTOR="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    -h|--help) sed -n '1,50p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

command -v "$LAKE" >/dev/null 2>&1 || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
[ -x "$LEAN_DOC" ] || { echo "no lean-doc at $LEAN_DOC — cargo build --bin lean-doc" >&2; exit 2; }

if [ -z "$OUT" ]; then
  OUT="$(mktemp -d)"
  TEMPORARY=1
else
  mkdir -p "$OUT"
  TEMPORARY=0
fi

say() { printf '\n=== %s\n' "$1"; }

say "1/7 build the fixture package (Lean core only)"
(cd "$FIXTURE" && "$LAKE" build)

say "2/7 build the extractor inside the fixture's environment"
# The extractor is `import Lean` and nothing else, which is what lets it be
# built against a package that has no Mathlib. `-rdynamic` is load-bearing:
# `importModules (loadExts := true)` resolves symbols in the running executable
# through the Lean interpreter (extractor/build.sh says the same thing).
if [ -z "$EXTRACTOR" ]; then
  EXTRACTOR="$FIXTURE/.lake/e2e-extract/extract"
  if [ ! -x "$EXTRACTOR" ]; then
    mkdir -p "$FIXTURE/.lake/e2e-extract"
    (cd "$FIXTURE" && "$LAKE" env lean --root="$ROOT/extractor" \
      -o "$FIXTURE/.lake/e2e-extract/Extract.olean" \
      -c "$FIXTURE/.lake/e2e-extract/Extract.c" \
      "$ROOT/extractor/Extract.lean")
    (cd "$FIXTURE" && "$LAKE" env leanc -rdynamic \
      -o "$EXTRACTOR" "$FIXTURE/.lake/e2e-extract/Extract.c")
  else
    echo "reusing $EXTRACTOR"
  fi
fi

say "3/7 GATE 1 — one command"
rm -rf "$OUT/first"
"$LEAN_DOC" build --root "$FIXTURE" --lib Micro --out "$OUT/first" \
  --extractor-bin "$EXTRACTOR" | tee "$OUT/first.log"
[ -f "$OUT/first/site/index.html" ] || { echo "no site was written" >&2; exit 1; }

"$HERE/site-gate.sh" "$OUT/first/site"

# Snapshot *before* the second run touches the same directory. Comparing the
# tree with a copy taken afterwards compares it with itself, which passes
# whatever happens — the first version of this script did exactly that and its
# GATE 2 was checking nothing at all.
rm -rf "$OUT/first-snapshot"
cp -R "$OUT/first/site" "$OUT/first-snapshot"

say "4/7 GATE 2 — the second run changes nothing"
"$LEAN_DOC" build --root "$FIXTURE" --lib Micro --out "$OUT/first" \
  --extractor-bin "$EXTRACTOR" | tee "$OUT/second.log"

# The site is compared before the counters, so that a diff is reported as a diff
# rather than as whatever the log happened to say.
if ! diff -r "$OUT/first-snapshot" "$OUT/first/site"; then
  echo "the second run changed the site" >&2
  exit 1
fi
if ! grep -qE 'incremental|0 module\(s\)|nothing to' "$OUT/second.log"; then
  echo "note: the second run's log does not name an incremental path:" >&2
  sed -n '1,20p' "$OUT/second.log" >&2
fi
if grep -qE '^render +modules [1-9]' "$OUT/second.log"; then
  echo "the second run rendered pages over an unchanged world" >&2
  grep -E '^render' "$OUT/second.log" >&2
  exit 1
fi

say "5/7 GATE 3 — a second full build is byte identical"
rm -rf "$OUT/again"
"$LEAN_DOC" build --root "$FIXTURE" --lib Micro --out "$OUT/again" \
  --extractor-bin "$EXTRACTOR" >"$OUT/again.log"
if ! diff -r "$OUT/first/site" "$OUT/again/site"; then
  echo "two full builds of the same world disagree — determinism is broken" >&2
  exit 1
fi
# The IR too: the site could agree while the tree it came from does not.
if ! diff -r "$OUT/first/ir" "$OUT/again/ir"; then
  echo "two extractions of the same world disagree" >&2
  exit 1
fi

say "6/7 GATE 4 — --jobs does not change the output"
# The extractor splits declarations across threads inside one environment
# (approach.md §5.1). That the IR comes out identical was measured once at stage
# 7d; that the *site* does has never been checked, and a parallel step that
# reorders its output is exactly the kind of thing that shows up as a diff on one
# machine and not another.
rm -rf "$OUT/jobs4"
"$LEAN_DOC" build --root "$FIXTURE" --lib Micro --out "$OUT/jobs4" \
  --extractor-bin "$EXTRACTOR" --jobs 4 >"$OUT/jobs4.log"
if ! diff -r "$OUT/first/ir" "$OUT/jobs4/ir"; then
  echo "--jobs 4 extracted a different IR than --jobs 1" >&2
  exit 1
fi
if ! diff -r "$OUT/first/site" "$OUT/jobs4/site"; then
  echo "--jobs 4 rendered a different site than --jobs 1" >&2
  exit 1
fi

say "7/7 summary"
printf 'site files : %s\n' "$(find "$OUT/first/site" -type f | wc -l | tr -d ' ')"
printf 'ir files   : %s\n' "$(find "$OUT/first/ir" -type f | wc -l | tr -d ' ')"
printf 'out        : %s\n' "$OUT"

if [ "$TEMPORARY" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
  rm -rf "$OUT"
fi
echo
echo "E2E MICRO: ok"
