#!/usr/bin/env bash
# M5-b — the gate of plan §1's **gate B**: `lean-doc build`, one command, on a
# Mathlib-dependent package that has never seen doc-gen4.
#
# **M7-c moved dependency links, and the reference side here predates it.**
# `build` always resolves a package root and so writes version-pinned GitHub blob
# URLs into every link into a dependency; the `lean-doc site` reference below is
# invoked **without `--root`** and still writes relative ones. A site diff is
# therefore **expected**, and is not a failure of the port. Gate A is suspended,
# not redefined — see `docs/implementation-plan.md` §1.
#
# usage: tools/target2-gate.sh <phase> [--target2 DIR] [--out DIR] [--jobs N]
#   phases: gate1 | gate2 | gate3 | gate4 | boundary | reset | all
#
# ============================================================================
# WHY THIS TARGET AND NOT THE MEASUREMENT ONE
# ============================================================================
#   The measurement target carries a 736 MB doc-gen4 output tree, so the
#   dependency map can always be handed to the renderer from outside — which is
#   what every gate up to M4-d did (`--link-index …/w7c/linkindex/…`). It
#   therefore cannot answer the question M5 exists for: **can the product make
#   the map itself?** Here there is no doc-gen4 tree, no `.bmp`, no published
#   site at the right Mathlib revision, and `build` is run **without
#   `--link-index`**. Whatever it cannot derive, it does not get.
#
#   `tools/make-target2.sh` builds the package (13 modules, two libraries, six
#   boundary values). It is a generator rather than a checked-in tree because
#   /private/tmp is emptied; see its heading.
#
# ============================================================================
# THE GATES
# ============================================================================
#   1  ONE COMMAND, NO MAP    `lean-doc build --root <target2> --out <dir>
#                             --extractor-bin <bin>` and nothing else. It has to
#                             find both libraries in the lakefile, glob the
#                             modules, derive --source-url from git, write its
#                             own dependency map and produce a site — which is
#                             then compared byte for byte with the tree
#                             `lean-doc site` writes from the same IR.
#
#   2  DETERMINISM            the same command into a second, empty --out. Two
#                             sites, byte for byte.
#
#   3  NOTHING CHANGED        `build` again over gate 1's tree: 0 to re-extract,
#                             nothing rendered, and not one byte of the site
#                             moves.
#
#   4  A REAL CHANGE          `Alpha.Basic`'s body moves into a new module
#                             `Alpha.BasicCore` and `lake build` really runs;
#                             then `build`, and the result is compared with a
#                             **from-scratch** build of the edited sources. The
#                             move is the M3-d4 shape: full names do not change,
#                             only which module defines them, so it is the edit
#                             that makes both re-render derivations fire — L3-1
#                             through the IR's refs (`Beta.Referrer` names
#                             `Alpha.Basic.alphaConst` in a signature) and L3-2
#                             through the whole-package map delta (`Beta.Basic`
#                             and `Beta.TokenSep` name it in a docstring code
#                             span).
#
#   boundary                  what the six boundary values did to the bytes.
#
# ============================================================================
# WHAT IS NEVER TOUCHED
# ============================================================================
#   /Users/haruka/dev/lean-projects — the measurement target — is read by
#   `make-target2.sh` (an APFS clone of `.lake/packages`) and by nothing here.
#   Every path this script writes is under $OUT or is target 2's own sources.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_BIN="$REPO/target/release/lean-doc"
EXTRACT_BIN="${EXTRACT_BIN:-$REPO/extractor/build/extract}"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
# `diff` is aliased to a colordiff that is not installed here; its exit 127
# reads as "differences found" and has already cost this project one wrong
# conclusion.
DIFF=/usr/bin/diff
MEASUREMENT_TARGET=/Users/haruka/dev/lean-projects

PHASE="${1-}"
case "$PHASE" in
  gate1|gate2|gate3|gate4|boundary|reset|all) shift ;;
  *) echo "usage: $0 gate1|gate2|gate3|gate4|boundary|reset|all [--target2 DIR]" >&2; exit 2 ;;
esac

TARGET2=/private/tmp/lean-doc-relay/m5b/target2
OUT=/private/tmp/lean-doc-relay/m5b/out
JOBS=4

while [ $# -gt 0 ]; do
  case "$1" in
    --target2) TARGET2="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$TARGET2" in
  "$MEASUREMENT_TARGET"|"$MEASUREMENT_TARGET"/*)
    echo "target 2 may not be inside the measurement target" >&2; exit 2 ;;
esac
case "$OUT" in
  "$MEASUREMENT_TARGET"|"$MEASUREMENT_TARGET"/*|"$TARGET2"|"$TARGET2"/*)
    echo "the output may not be inside either package" >&2; exit 2 ;;
esac
[ -d "$TARGET2" ] || { echo "missing target 2: $TARGET2 — run tools/make-target2.sh" >&2; exit 1; }
[ -x "$EXTRACT_BIN" ] || { echo "missing extractor: $EXTRACT_BIN" >&2; exit 1; }
[ -x "$RUST_BIN" ] || {
  echo "missing: $RUST_BIN — run: cargo build --release -p lean-doc" >&2; exit 1; }

WORK="$OUT/work"
mkdir -p "$OUT" "$WORK"

# The move, in the shape `stage5e/setup-clone.sh` established: A's body goes to
# X = A ++ "Core" inside the same `namespace`, and A becomes a one-line shim.
A_MOD=Alpha.Basic
X_MOD=Alpha.BasicCore
A_REL=Alpha/Basic.lean
X_REL=Alpha/BasicCore.lean

nlines () { grep -c . "$1" 2>/dev/null || true; }
files_in () { find "$1" -type f | wc -l | tr -d ' '; }

# ---------------------------------------------------------------- the protocol

target2_state () {
  if [ -f "$TARGET2/$X_REL" ]; then echo moved
  elif [ -z "$(git -C "$TARGET2" status --porcelain)" ]; then echo baseline
  else echo unknown; fi
}

require_up_to_date () { # require_up_to_date <tag>
  (cd "$TARGET2" && "$LAKE" build --no-build 2>&1) > "$WORK/$1-nobuild.txt" || {
    echo "lake build --no-build failed; see $WORK/$1-nobuild.txt" >&2; exit 3; }
  grep -q "All targets up-to-date" "$WORK/$1-nobuild.txt" || {
    echo "lake does not consider target 2 up to date:" >&2
    tail -3 "$WORK/$1-nobuild.txt" >&2; exit 3; }
  echo "  target2: $(tail -1 "$WORK/$1-nobuild.txt")"
}

# **The command line the gate is about.** No `--lib` (the lakefile has two
# [[lean_lib]] blocks and the command has to find both), no `--source-url` (git
# HEAD and the origin remote), and **no `--link-index`**: the map is
# <out>/link-index.lidx and this run writes it.
build () { # build <out dir> <log> [extra args…]
  local out="$1" log="$2"; shift 2
  "$RUST_BIN" build --root "$TARGET2" --out "$out" \
    --extractor-bin "$EXTRACT_BIN" --jobs "$JOBS" \
    --timings "$out.timings.json" "$@" > "$log" 2>&1
}

manifest () { # manifest <site> <out file>
  ( cd "$1" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 ) > "$2"
}

compare () { # compare <name> <a> <b> <expected files>
  local name="$1" a="$2" b="$3" expect="$4" status=0
  "$DIFF" -r -q "$a" "$b" > "$OUT/$name.diff" 2>&1 || status=$?
  {
    printf 'comparison          %s\n' "$name"
    printf 'left                %s (%s file(s))\n' "$a" "$(files_in "$a")"
    printf 'right               %s (%s file(s))\n' "$b" "$(files_in "$b")"
    printf 'expected files      %s\n' "$expect"
    if [ "$(files_in "$a")" != "$expect" ] || [ "$(files_in "$b")" != "$expect" ]; then
      printf 'DENOMINATOR         WRONG\n'
    else
      printf 'denominator         ok\n'
    fi
    printf 'diff                %s\n' \
      "$([ "$status" = 0 ] && echo identical || echo "$(nlines "$OUT/$name.diff") line(s)")"
    sed 's/^/  /' "$OUT/$name.diff"
    if [ "$status" != 0 ]; then
      printf 'first differing byte\n'
      sed -n 's/^Files \(.*\) and \(.*\) differ$/\1|\2/p' "$OUT/$name.diff" \
        | while IFS='|' read -r x y; do printf '  %s\n' "$(cmp "$x" "$y" 2>&1 || true)"; done
    fi
    printf 'RESULT              %s\n' \
      "$([ "$status" = 0 ] && [ "$(files_in "$a")" = "$expect" ] \
         && [ "$(files_in "$b")" = "$expect" ] && echo PASS || echo FAIL)"
  } > "$OUT/$name.txt"
  cat "$OUT/$name.txt"
}

counts () { # counts <log>
  grep -E '^(lib|modules|source|plan|detect|round |extract|serve|prune|impact|render|global|ledger|build) ' "$1" || true
}

# ------------------------------------------------------------------- the gates

phase_gate1 () {
  echo "### gate 1 — one command, and no dependency map handed in"
  require_up_to_date gate1
  rm -rf "$OUT/base" "$OUT/base.timings.json" "$OUT/ref-site" "$OUT/ref-state"
  build "$OUT/base" "$OUT/base.log" || { tail -20 "$OUT/base.log" >&2; exit 3; }
  counts "$OUT/base.log"
  manifest "$OUT/base/site" "$OUT/base.sha256"
  local pages
  pages="$(files_in "$OUT/base/site")"
  echo "  site files          $pages"
  echo "  link index          $(wc -c < "$OUT/base/link-index.lidx" | tr -d ' ') B (derived by this run)"

  # The reference: `lean-doc site` over the IR this run wrote, with the map this
  # run derived. It is the *other* code path to the same tree — full generation
  # through `site` rather than through `build` — so a difference means `build`
  # is doing something to the site that `site` does not.
  local url
  url="$(sed -n 's/^source  //p' "$OUT/base.log" | head -1)"
  echo "  reference site from $OUT/base/ir (source url $url)"
  "$RUST_BIN" site --ir "$OUT/base/ir" --out "$OUT/ref-site" --source-url "$url" \
    --link-index "$OUT/base/link-index.lidx" --state "$OUT/ref-state" \
    > "$OUT/ref-site.log" 2>&1
  compare gate1-site "$OUT/base/site" "$OUT/ref-site" "$pages"
}

phase_gate2 () {
  echo "### gate 2 — determinism: the same command into a second --out"
  [ -d "$OUT/base/site" ] || { echo "run gate1 first" >&2; exit 3; }
  rm -rf "$OUT/twin" "$OUT/twin.timings.json"
  build "$OUT/twin" "$OUT/twin.log" || { tail -20 "$OUT/twin.log" >&2; exit 3; }
  counts "$OUT/twin.log"
  compare gate2-site "$OUT/base/site" "$OUT/twin/site" "$(files_in "$OUT/base/site")"
  compare gate2-ir "$OUT/base/ir" "$OUT/twin/ir" "$(files_in "$OUT/base/ir")"
  if "$DIFF" -q "$OUT/base/link-index.lidx" "$OUT/twin/link-index.lidx" > /dev/null; then
    echo "  link index          identical ($(wc -c < "$OUT/base/link-index.lidx" | tr -d ' ') B)"
  else
    echo "  link index          DIFFERS"
  fi
}

phase_gate3 () {
  echo "### gate 3 — nothing changed"
  [ -d "$OUT/base/site" ] || { echo "run gate1 first" >&2; exit 3; }
  build "$OUT/base" "$OUT/rerun.log" || { tail -20 "$OUT/rerun.log" >&2; exit 3; }
  counts "$OUT/rerun.log"
  manifest "$OUT/base/site" "$OUT/rerun.sha256"
  if "$DIFF" -q "$OUT/base.sha256" "$OUT/rerun.sha256" > /dev/null; then
    echo "  GATE 3              PASS — $(nlines "$OUT/rerun.sha256") files, not one byte moved"
  else
    echo "  GATE 3              FAIL"
    "$DIFF" "$OUT/base.sha256" "$OUT/rerun.sha256" | head -20
  fi
  grep -q 'plan    incremental' "$OUT/rerun.log" || {
    echo "  the second run did not take the incremental path" >&2; exit 3; }
  grep -q '0 to re-extract' "$OUT/rerun.log" || {
    echo "  the second run re-extracted something" >&2; exit 3; }
}

apply_move () {
  local body
  body="$(sed '1,/^$/d' "$TARGET2/$A_REL")"
  {
    printf 'import Mathlib.Data.Nat.Basic\n'
    printf '%s\n' "$body"
  } > "$TARGET2/$X_REL"
  printf 'import %s\n' "$X_MOD" > "$TARGET2/$A_REL"
}

phase_gate4 () {
  echo "### gate 4 — a real change: $A_MOD -> $X_MOD, and a real lake build"
  local state; state="$(target2_state)"
  case "$state" in
    baseline)
      echo "  applying the move"
      apply_move
      (cd "$TARGET2" && "$LAKE" build) > "$WORK/move-build.log" 2>&1 || {
        echo "lake build failed after the move; see $WORK/move-build.log" >&2
        tail -20 "$WORK/move-build.log" >&2; exit 1; }
      tail -2 "$WORK/move-build.log" ;;
    moved) echo "  target 2 already carries the move — reusing it" ;;
    *) echo "target 2 is '$state'; gate 4 needs 'baseline' or 'moved'" >&2; exit 3 ;;
  esac
  require_up_to_date gate4

  build "$OUT/base" "$OUT/moved.log" || { tail -20 "$OUT/moved.log" >&2; exit 3; }
  counts "$OUT/moved.log"
  manifest "$OUT/base/site" "$OUT/moved.sha256"

  # The write-back: the run after the run has to report 0 changed.
  build "$OUT/base" "$OUT/moved-again.log" || { tail -20 "$OUT/moved-again.log" >&2; exit 3; }
  counts "$OUT/moved-again.log"
  manifest "$OUT/base/site" "$OUT/moved-again.sha256"
  if grep -q '0 to re-extract' "$OUT/moved-again.log" \
     && "$DIFF" -q "$OUT/moved.sha256" "$OUT/moved-again.sha256" > /dev/null; then
    echo "  write-back          PASS — 0 changed, and the site did not move"
  else
    echo "  write-back          FAIL"
    grep -E '^detect' "$OUT/moved-again.log" || true
    "$DIFF" "$OUT/moved.sha256" "$OUT/moved-again.sha256" | head -20
  fi

  # Incremental == from scratch, over the edited sources.
  rm -rf "$OUT/scratch" "$OUT/scratch.timings.json"
  build "$OUT/scratch" "$OUT/scratch.log" || { tail -20 "$OUT/scratch.log" >&2; exit 3; }
  counts "$OUT/scratch.log"
  compare gate4-site "$OUT/base/site" "$OUT/scratch/site" "$(files_in "$OUT/scratch/site")"
  compare gate4-ir "$OUT/base/ir" "$OUT/scratch/ir" "$(files_in "$OUT/scratch/ir")"
  if "$DIFF" -q "$OUT/base/ledger.json" "$OUT/scratch/ledger.json" > /dev/null; then
    echo "  ledger              identical"
  else
    echo "  ledger              DIFFERS"
    "$DIFF" <(python3 -m json.tool "$OUT/base/ledger.json") \
            <(python3 -m json.tool "$OUT/scratch/ledger.json") | head -30
  fi
  if "$DIFF" -q "$OUT/base/link-index.lidx" "$OUT/scratch/link-index.lidx" > /dev/null; then
    echo "  link index          identical"
  else
    echo "  link index          DIFFERS (expected: the move changes which module owns the names)"
    "$DIFF" "$OUT/base/link-index.lidx" "$OUT/scratch/link-index.lidx" | head -10
  fi
}

# What each boundary value did, read off the produced site rather than argued.
phase_boundary () {
  echo "### boundary values — what came out"
  local site="$OUT/base/site" ir="$OUT/base/ir"
  [ -d "$site" ] || { echo "run gate1 first" >&2; exit 3; }
  deno run --allow-read "$REPO/tools/target2-boundary.ts" \
    --site "$site" --ir "$ir" --lidx "$OUT/base/link-index.lidx" \
    --state "$OUT/base/state/global-state.json" | tee "$OUT/boundary.txt"
}

phase_reset () {
  echo "### reset"
  git -C "$TARGET2" checkout -- Alpha Beta Alpha.lean Beta.lean 2>/dev/null || true
  rm -f "$TARGET2/$X_REL"
  (cd "$TARGET2" && "$LAKE" build) > "$WORK/reset.log" 2>&1 || {
    echo "reset build failed; see $WORK/reset.log" >&2; tail -20 "$WORK/reset.log" >&2; exit 1; }
  tail -2 "$WORK/reset.log"
  echo "  state: $(target2_state), git status: $(git -C "$TARGET2" status --porcelain | wc -l | tr -d ' ') line(s)"
  echo "  resident extractors left: $(pgrep -f 'extractor/build/extract' | wc -l | tr -d ' ')"
}

conditions () {
  {
    printf 'date              %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'phase             %s\n' "$PHASE"
    printf 'host              %s / %s / %s GB\n' \
      "$(uname -srm)" \
      "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?')" \
      "$(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024))"
    printf 'target2           %s (HEAD %s, state %s)\n' \
      "$TARGET2" "$(git -C "$TARGET2" rev-parse HEAD)" "$(target2_state)"
    printf 'lean-toolchain    %s\n' "$(tr -d '\n' < "$TARGET2/lean-toolchain" 2>/dev/null || echo '?')"
    printf 'mathlib rev       %s\n' \
      "$(python3 -c 'import json,sys;print(next(p["rev"] for p in json.load(open(sys.argv[1]))["packages"] if p["name"]=="mathlib"))' "$TARGET2/lake-manifest.json" 2>/dev/null || echo '?')"
    printf 'extractor         %s\n' "$EXTRACT_BIN"
    printf 'jobs              %s\n' "$JOBS"
    printf 'rustc             %s\n' "$(rustc --version 2>/dev/null || echo '?')"
  } > "$OUT/conditions-$PHASE.txt"
  cat "$OUT/conditions-$PHASE.txt"
}

case "$PHASE" in
  gate1) phase_gate1 ;;
  gate2) phase_gate2 ;;
  gate3) phase_gate3 ;;
  gate4) phase_gate4 ;;
  boundary) phase_boundary ;;
  reset) phase_reset ;;
  all)   phase_gate1; phase_gate2; phase_gate3; phase_boundary; phase_gate4 ;;
esac
conditions
