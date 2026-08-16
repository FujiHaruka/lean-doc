#!/usr/bin/env bash
# M4-d — the gate of M4: **one command**, run against a clone of the measurement
# target, with a real edit and a real `lake build` in the middle.
#
# **M7-c moved dependency links, and the reference side here predates it.**
# `build` always resolves a package root and so writes version-pinned GitHub blob
# URLs into every link into a dependency; the `lean-doc site` reference below is
# invoked **without `--root`** and still writes relative ones. A site diff is
# therefore **expected**, and is not a failure of the port. Gate A is suspended,
# not redefined — see `docs/implementation-plan.md` §1.
#
# usage: tools/build-gate.sh <phase> [--clone DIR] [--out DIR] [--lidx FILE]
#                            [--jobs N] [--move-module <Module>]
#   phases: gate1 | gate2 | gate3 | gate4 | reset | all
#
# ============================================================================
# THE FIVE GATES
# ============================================================================
#   1  ONE COMMAND       `lean-doc build` over a clean clone produces a site,
#                        and that site is byte-identical to the one
#                        `lean-doc site` writes from the **independently
#                        extracted** base IR of M3-d4 (the frozen prototype's
#                        `extract-once.sh` + stage 7d binary). Denominator 438
#                        = 432 module pages + 6 whole-package artifacts.
#                        The IR trees are compared too (436 files), which is
#                        the stronger half: `build` derived its own module list,
#                        its own source URL and ran its own resident extractor,
#                        and the tree still has to come out the same bytes.
#
#   2  THE SECOND RUN    the same command again, with nothing changed: 0 to
#                        re-extract, 0 pages rendered, and **not one byte of the
#                        site moves**. This is what the ledger write-back buys;
#                        without it the second run re-extracts the first run's
#                        changed set (plan §7, M3-d2's debt 1).
#
#   3  THE WRITE-BACK    a **real** move inside the clone
#                        (`setup-clone.sh move … minimal` + `lake build`), then
#                        `build`, then `build` again. The second one must report
#                        **0 changed**. Without a write-back it would re-extract
#                        the same modules for ever, and the difference between
#                        "the ledger was written" and "the ledger was not" is
#                        exactly this run.
#
#   4  INCREMENTAL == FULL  the tree gate 3 left behind, against a `build` run
#                        from zero over the same edited sources, in a different
#                        --out. Byte for byte, denominator 439 (the move adds a
#                        module). This is M3-d4's gate 1 restated through
#                        `build`: both sides now derive the module list, the
#                        source URL and the layout themselves, so a difference
#                        in any of those three shows up here.
#
#   5  THE RESET         `setup-clone.sh reset`, then: `git status` empty,
#                        `lake build --no-build` = All targets up-to-date.
#
# ============================================================================
# WHAT IS NEVER TOUCHED
# ============================================================================
#   The measurement target /Users/haruka/dev/lean-projects is not opened for
#   writing by anything here — every path this script writes is under $OUT or is
#   the clone's own sources. The clone's `.lake/build/doc` (doc-gen4's reference
#   tree) is never touched and no `git commit` is ever run.
#
#   The clone is a baseline only if its oleans were built **at the clone's own
#   path** (`tools/rebuild-own.sh`): without that, a moved
#   declaration's referrers rebuild for the wrong reason and the gate passes for
#   a reason nobody meant (stage 5e (e)). `require_baseline` checks it with
#   `strings`, and checks that Lake considers the tree up to date.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_BIN="$REPO/target/release/lean-doc"
EXTRACT_BIN="${EXTRACT_BIN:-$REPO/extractor/build/extract}"
SETUP_CLONE="$REPO/tools/setup-clone.sh"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
# `diff` is aliased to a colordiff that is not installed here; its exit 127
# reads as "differences found" and has already cost this project one wrong
# conclusion.
DIFF=/usr/bin/diff

PHASE="${1-}"
case "$PHASE" in
  gate1|gate2|gate3|gate4|reset|all) shift ;;
  *) echo "usage: $0 gate1|gate2|gate3|gate4|reset|all [--clone DIR]" >&2; exit 2 ;;
esac

CLONE=/private/tmp/lean-doc-relay/clone
OUT=/private/tmp/lean-doc-relay/m4d
LIDX=/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx
# M3-d4's independently extracted trees: the base IR is the reference gate 1
# compares against, and it was produced by the **prototype's** extractor.
REF_IR=/private/tmp/lean-doc-relay/m3d4/shared/base-ir
REF_MODULES=/private/tmp/lean-doc-relay/m3d4/shared/modules-base.txt
JOBS=4
# The module M3-d4 chose, and its reasons hold here: it is one of the 7 modules
# of 432 whose declarations are named inside another module's docstring, so the
# move exercises **both** derivations (L3-1 through the IR's refs, L3-2 through
# the whole-package map delta). See plan §7, M3-d4.
MOVE_MODULE=InformationTheory.Shannon.BroadcastChannel.Basic

while [ $# -gt 0 ]; do
  case "$1" in
    --clone) CLONE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --lidx) LIDX="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --move-module) MOVE_MODULE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

TARGET=/Users/haruka/dev/lean-projects
case "$CLONE" in
  "$TARGET"|"$TARGET"/*) echo "the clone may not be inside the measurement target" >&2; exit 2 ;;
esac
case "$OUT" in
  "$TARGET"|"$TARGET"/*|"$CLONE"|"$CLONE"/*)
    echo "the output may not be inside the target or the clone" >&2; exit 2 ;;
esac
[ -d "$CLONE" ] || { echo "missing clone: $CLONE" >&2; exit 1; }
[ -f "$LIDX" ] || { echo "missing link index: $LIDX" >&2; exit 1; }
[ -x "$EXTRACT_BIN" ] || { echo "missing extractor: $EXTRACT_BIN" >&2; exit 1; }
[ -x "$RUST_BIN" ] || {
  echo "missing: $RUST_BIN — run: cargo build --release -p lean-doc" >&2; exit 1; }

WORK="$OUT/work"
mkdir -p "$OUT" "$WORK"

A_MOD="$MOVE_MODULE"
X_MOD="${A_MOD}Core"
X_REL="$(printf '%s' "$X_MOD" | tr '.' '/').lean"
DEL_MOD=InformationTheory.Shannon.ArithmeticCoding
DEL_REL="$(printf '%s' "$DEL_MOD" | tr '.' '/').lean"

# Written down rather than derived, so that a run which produced the wrong
# number of pages fails instead of redefining the question. 6 = the
# whole-package artifacts; the move adds one module and therefore one page.
EXPECT_BASE=438
EXPECT_MOVE=439

nlines () { grep -c . "$1" 2>/dev/null || true; }
files_in () { find "$1" -type f | wc -l | tr -d ' '; }

# ---------------------------------------------------------------- the protocol

clone_state () {
  local dirty x del
  dirty="$(git -C "$CLONE" status --porcelain)"
  x=no; [ -f "$CLONE/$X_REL" ] && x=yes
  del=yes; [ -f "$CLONE/$DEL_REL" ] || del=no
  if [ -z "$dirty" ] && [ "$x" = no ] && [ "$del" = yes ]; then echo baseline
  elif [ "$x" = yes ] && [ "$del" = yes ]; then echo moved
  else echo unknown; fi
}

require_own_oleans () {
  local probe="$CLONE/.lake/build/lib/lean/${A_MOD//.//}.olean" dump="$WORK/olean-strings.txt"
  [ -f "$probe" ] || { echo "no olean to probe: $probe" >&2; exit 2; }
  strings "$probe" 2>/dev/null > "$dump" || true
  grep -q "$CLONE" "$dump" || {
    echo "the clone's oleans were not built at the clone's path — run" >&2
    echo "tools/rebuild-own.sh first (stage 5e (e))" >&2; exit 2; }
  grep -q "$TARGET/" "$dump" && {
    echo "the clone's oleans still name the measurement target's path" >&2; exit 2; }
  true
}

require_up_to_date () { # require_up_to_date <tag>
  (cd "$CLONE" && "$LAKE" build --no-build 2>&1) > "$WORK/$1-nobuild.txt" || {
    echo "lake build --no-build failed; see $WORK/$1-nobuild.txt" >&2; exit 3; }
  grep -q "All targets up-to-date" "$WORK/$1-nobuild.txt" || {
    echo "lake does not consider the clone up to date:" >&2
    tail -3 "$WORK/$1-nobuild.txt" >&2; exit 3; }
  echo "  clone: $(tail -1 "$WORK/$1-nobuild.txt")"
}

require_baseline () { # require_baseline <tag>
  local state; state="$(clone_state)"
  [ "$state" = baseline ] || {
    echo "the clone is '$state', not 'baseline' — run: $0 reset" >&2; exit 3; }
  require_own_oleans
  require_up_to_date "$1"
}

# `lean-doc build`, with **only** the flags a caller cannot derive: where the
# package is, where the output goes, the dependency map (M5 owns its supply) and
# the extractor binary (171 MB, built against the target's toolchain, so it can
# have no default — M4-a). Everything else — the library, the module list, the
# source URL, the choice between full and incremental — is the command's.
build () { # build <out dir> <log> [extra args…]
  local out="$1" log="$2"; shift 2
  "$RUST_BIN" build --root "$CLONE" --out "$out" --link-index "$LIDX" \
    --extractor-bin "$EXTRACT_BIN" --jobs "$JOBS" \
    --timings "$out.timings.json" "$@" > "$log" 2>&1
}

# A manifest of the whole site, so that "nothing moved" is a comparison of
# bytes rather than of file counts.
manifest () { # manifest <site> <out file>
  ( cd "$1" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 ) > "$2"
}

# One `diff -r`, reported with its denominator and with the first differing byte
# of every file that differs — "they differ" is not a finding.
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

# What the run said it did, in the four numbers the gates are stated in.
counts () { # counts <log>
  grep -E '^(lib|modules|source|plan|detect|round |extract|prune|impact|render|global|ledger|build) ' "$1" || true
}

# ------------------------------------------------------------------- the gates

phase_gate1 () {
  echo "### gate 1 — one command over a clean clone"
  require_baseline gate1
  rm -rf "$OUT/base" "$OUT/ref-site" "$OUT/ref-state"
  build "$OUT/base" "$OUT/base.log"
  counts "$OUT/base.log"
  manifest "$OUT/base/site" "$OUT/base.sha256"

  # The reference: `lean-doc site` over the IR M3-d4 extracted with the frozen
  # prototype's extract-once.sh, at the same revision, with the same map.
  local url
  url="$(sed -n 's/^source  //p' "$OUT/base.log" | head -1)"
  echo "  reference site from $REF_IR (source url $url)"
  "$RUST_BIN" site --ir "$REF_IR" --out "$OUT/ref-site" --source-url "$url" \
    --link-index "$LIDX" --state "$OUT/ref-state" > "$OUT/ref-site.log" 2>&1
  compare gate1-site "$OUT/base/site" "$OUT/ref-site" "$EXPECT_BASE"
  compare gate1-ir "$OUT/base/ir" "$REF_IR" "$(files_in "$REF_IR")"
  # The module list is the other half of "it derived the same question": the
  # order makes the ledger's and the merged index.json's bytes (M3-d2b).
  "$DIFF" "$OUT/base/work/modules.txt" "$REF_MODULES" > "$OUT/gate1-modules.diff" \
    && echo "  module list: identical to M3-d4's ($(nlines "$REF_MODULES") modules)" \
    || { echo "  module list DIFFERS from M3-d4's" >&2; exit 3; }
}

phase_gate2 () {
  echo "### gate 2 — the second run"
  require_baseline gate2
  [ -d "$OUT/base/site" ] || { echo "run gate1 first" >&2; exit 3; }
  build "$OUT/base" "$OUT/rerun.log"
  counts "$OUT/rerun.log"
  manifest "$OUT/base/site" "$OUT/rerun.sha256"
  if "$DIFF" -q "$OUT/base.sha256" "$OUT/rerun.sha256" > /dev/null; then
    echo "  GATE 2              PASS — $(nlines "$OUT/rerun.sha256") files, not one byte moved"
  else
    echo "  GATE 2              FAIL"
    "$DIFF" "$OUT/base.sha256" "$OUT/rerun.sha256" | head -20
  fi
  grep -q 'plan    incremental' "$OUT/rerun.log" || {
    echo "  the second run did not take the incremental path" >&2; exit 3; }
  grep -q '0 to re-extract' "$OUT/rerun.log" || {
    echo "  the second run re-extracted something" >&2; exit 3; }
}

phase_gate3 () {
  echo "### gate 3 — a real move, and the ledger write-back"
  local state; state="$(clone_state)"
  case "$state" in
    baseline)
      require_baseline gate3
      echo "  applying the move ($A_MOD -> $X_MOD) and rebuilding"
      "$SETUP_CLONE" move "$CLONE" "$A_MOD" minimal > "$WORK/move-edit.log" 2>&1 || {
        echo "the move failed; see $WORK/move-edit.log" >&2
        tail -20 "$WORK/move-edit.log" >&2; exit 1; }
      grep -E "^A is now|error" "$WORK/move-edit.log" || true ;;
    moved) echo "  the clone already carries the move — reusing it" ;;
    *) echo "the clone is '$state'; gate 3 needs 'baseline' or 'moved'" >&2; exit 3 ;;
  esac
  [ -f "$CLONE/$X_REL" ] || { echo "the move did not create $X_REL" >&2; exit 3; }
  require_up_to_date gate3

  build "$OUT/base" "$OUT/moved.log"
  counts "$OUT/moved.log"
  manifest "$OUT/base/site" "$OUT/moved.sha256"

  # The gate itself: **the run after the run**. A ledger that was not written
  # back re-extracts the same modules, and says so.
  build "$OUT/base" "$OUT/moved-again.log"
  counts "$OUT/moved-again.log"
  manifest "$OUT/base/site" "$OUT/moved-again.sha256"
  if grep -q '0 to re-extract' "$OUT/moved-again.log" \
     && "$DIFF" -q "$OUT/moved.sha256" "$OUT/moved-again.sha256" > /dev/null; then
    echo "  GATE 3              PASS — 0 changed, and the site did not move"
  else
    echo "  GATE 3              FAIL"
    grep -E '^detect' "$OUT/moved-again.log" || true
    "$DIFF" "$OUT/moved.sha256" "$OUT/moved-again.sha256" | head -20
  fi
}

phase_gate4 () {
  echo "### gate 4 — incremental == full, through \`build\` on both sides"
  [ -f "$CLONE/$X_REL" ] || { echo "the clone does not carry the move; run gate3" >&2; exit 3; }
  rm -rf "$OUT/scratch"
  build "$OUT/scratch" "$OUT/scratch.log" --full
  counts "$OUT/scratch.log"
  compare gate4-site "$OUT/base/site" "$OUT/scratch/site" "$EXPECT_MOVE"
  compare gate4-ir "$OUT/base/ir" "$OUT/scratch/ir" "$(files_in "$OUT/scratch/ir")"
  # The ledgers are two independent answers to "which oleans is this IR from",
  # one reached through five incremental runs and one from zero.
  if "$DIFF" -q "$OUT/base/ledger.json" "$OUT/scratch/ledger.json" > /dev/null; then
    echo "  ledger              identical"
  else
    echo "  ledger              DIFFERS"
    "$DIFF" <(python3 -m json.tool "$OUT/base/ledger.json") \
            <(python3 -m json.tool "$OUT/scratch/ledger.json") | head -30
  fi
}

phase_reset () {
  echo "### reset"
  "$SETUP_CLONE" reset "$CLONE" > "$WORK/reset.log" 2>&1 || {
    echo "reset failed; see $WORK/reset.log" >&2; tail -20 "$WORK/reset.log" >&2; exit 1; }
  tail -3 "$WORK/reset.log"
  require_baseline reset
  echo "  reset verified: state=$(clone_state), HEAD=$(git -C "$CLONE" rev-parse --short HEAD)"
  echo "  git status: $(git -C "$CLONE" status --porcelain | wc -l | tr -d ' ') line(s)"
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
    printf 'clone             %s (HEAD %s, state %s)\n' \
      "$CLONE" "$(git -C "$CLONE" rev-parse HEAD)" "$(clone_state)"
    printf 'move module       %s -> %s\n' "$A_MOD" "$X_MOD"
    printf 'lean-toolchain    %s\n' "$(tr -d '\n' < "$CLONE/lean-toolchain" 2>/dev/null || echo '?')"
    printf 'extractor         %s\n' "$EXTRACT_BIN"
    printf 'jobs              %s\n' "$JOBS"
    printf 'link index        %s (%s B)\n' "$LIDX" "$(wc -c < "$LIDX" | tr -d ' ')"
    printf 'reference IR      %s\n' "$REF_IR"
    printf 'rustc             %s\n' "$(rustc --version 2>/dev/null || echo '?')"
  } > "$OUT/conditions-$PHASE.txt"
  cat "$OUT/conditions-$PHASE.txt"
}

case "$PHASE" in
  gate1) phase_gate1 ;;
  gate2) phase_gate2 ;;
  gate3) phase_gate3 ;;
  gate4) phase_gate4 ;;
  reset) phase_reset ;;
  all)   phase_gate1; phase_gate2; phase_gate3; phase_gate4; phase_reset ;;
esac
conditions
