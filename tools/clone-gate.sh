#!/usr/bin/env bash
# M3-d4 — the real gate: two **real** edits inside a clone of the measurement
# target, each followed by a real `lake build`, each run through both
# implementations of the incremental pipeline.
#
# **The sibling of tools/incremental-reference.sh**: same `--impl ts|rust`, same
# per-scenario record shape (`<s>-counts.json` / `<s>-work/` / `<s>-status.txt` /
# `<s>-complained.txt` / `<s>-pages.txt` / `<s>-work-present.txt` / `<s>-ir/` /
# `<s>-global/`), so tools/incremental-compare.sh reads this tree unchanged.
#
# usage: tools/clone-gate.sh <phase> [--impl ts|rust] [--clone DIR] [--out DIR]
#                            [--lidx FILE] [--shared DIR] [--jobs N]
#                            [--move-module <Module>]
#   phases: setup | move | delete | reset | all
#
# ============================================================================
# WHY THIS EXISTS — the path M3-d3 never walked 【実測 2026-08-15】
# ============================================================================
#   M3-d3 injected change with `ledger touch`, which invalidates a ledger entry
#   and **does not touch the olean**. Re-extracting a module whose olean did not
#   move reproduces its IR byte for byte, so across all seven scenarios
#   `irChanged` was 0 wherever the change was injected, **`staleFound` (L3-1) was
#   0 in 7 of 7** and **`globalStale` (L3-2) was 0 in 7 of 7** — and no run ever
#   reached a second round. The two derivations that make this pipeline
#   non-trivial were only ever compared **on the empty answer**.
#
#   Here the sources are really edited and `lake build` really runs, so the
#   oleans really move. **The point of the exercise is that `staleFound`,
#   `globalStale` and `rounds >= 2` come out non-empty** — if they do not, the
#   module choice or the protocol is wrong and the run must be stopped rather
#   than "passed".
#
# ============================================================================
# THE TWO SCENARIOS
# ============================================================================
#   move    `--move-module`'s body moves into a new module `…Core`; the original
#           becomes a one-line shim (`experiments/stage5e/setup-clone.sh move …
#           minimal`). Site denominator **439** = 432 pages + 6 whole-package
#           artifacts + the new module's page.
#
#           THE MODULE IS A PARAMETER AND CHOOSING IT WRONG COSTS THE RUN ONE OF
#           THE TWO DERIVATIONS. `setup-clone.sh:18-25` already says it must have
#           **referrers**, or the move is invisible in the IR's `refs`. That is
#           necessary and **not sufficient**: L3-1 and L3-2 key on different
#           things, and a module can satisfy one and not the other.
#
#             L3-1 (`staleFound`) reads the IR's `refs` — who *names* a
#                  declaration that moved.
#             L3-2 (`globalStale`) reads [`ModuleFacts::tokens`], which is built
#                  from **declaration docstrings' code spans and markdown link
#                  targets only** (`facts.rs:94-108`) — who *documents* a name
#                  that moved.
#
#           Measured on this package 【実測 2026-08-15】: only **7 of the 432
#           modules** have any of their declarations named inside another module's
#           docstring at all, so the second condition is the scarce one.
#           `InformationTheory.Shannon.Huffman.Length` (declarations 27,
#           referrersDirect 4) is *not* one of them — a whole move of it was run
#           and produced **rounds 2, staleFound 1, globalStale 0**, gate 1 439/439
#           identical. Real, and half the experiment.
#           The default below is one of the 7 and has the widest reference set of
#           them 【実測 census + `lean-doc global --before`】: declarations 25,
#           directImports 2, importedByDirect 6, importersTransitive 37,
#           **referrersDirect 33**, and its names are documented by
#           `…BroadcastChannel.Marton.Basic`, so the map delta reports 25 names
#           moved and 1 module affected.
#
#   delete  `InformationTheory.Shannon.ArithmeticCoding`'s source file is deleted
#           and the single `import` line naming it is removed from
#           `InformationTheory.lean`. Chosen because it is imported by **exactly
#           one** file 【実測: `rg -l '^import InformationTheory\.Shannon\.ArithmeticCoding$'`
#           hits `InformationTheory.lean` and nothing else; census: declarations 6,
#           importedByDirect 1, importersTransitive 1, referrersDirect 0】, so the
#           build stays green after the deletion.
#           **Lake does not delete the orphaned olean** and this script leaves it
#           where it is: the module list is a glob over the *sources*, so the
#           module correctly reads as gone, and the orphan is the field test of
#           「孤児 olean を拾わない」 (plan §5, M3-d).
#           Site denominator **437** = 431 pages + 6 whole-package artifacts.
#
# ============================================================================
# THE TWO GATES
# ============================================================================
#   GATE 1 (the real one, `--impl rust`, recorded in `<s>-sitecheck.txt`)
#     INCREMENTAL — the base {ir, pages, ledger, state} copied, then
#     `lean-doc incremental` over the **post-edit** module list — is compared with
#     `/usr/bin/diff -r` against FROM-SCRATCH: the same post-edit module list
#     extracted from zero and put through `lean-doc site`. Bytes, whole tree, plus
#     the same comparison one layer down on the IR.
#     This is a *within-implementation* question and it is the one that decides
#     whether the pipeline is correct.
#
#   GATE 2 (`tools/incremental-compare.sh` over the two `--out` trees)
#     The same two scenarios run under `--impl ts`, and the two records compared.
#     **Page bytes are not compared across implementations** 【決定 4, 実測】: the
#     prototype's step 7 (`incremental.sh:371-373`) renders without
#     `--link-index`, which moves 150 of the 432 pages' bytes, and the prototype
#     is frozen. What is compared is the set of pages, the IR, the six
#     whole-package artifacts and **the answers the run computed**.
#
# ============================================================================
# THE PROTOCOL — what has to be true before an edit, and after the reset
# ============================================================================
#   The clone is a baseline only if its oleans were built **at the clone's own
#   path** (`experiments/stage5e/rebuild-own.sh`). Without that the moved module's
#   referrers would rebuild for the wrong reason and **the gate would pass for a
#   reason nobody meant** (stage 5e (e)). `require_baseline` checks it with
#   `strings`, checks `git status` is clean, checks Lake reports every target up
#   to date, and — once a base ledger exists — checks the ledger's own fixed
#   point: 0 changed, 0 added, 0 removed against the base IR. That last one is
#   what says a `reset` really put the oleans back.
#
#   NOTHING OUTSIDE $OUT, $OUT.work, $SHARED AND THE CLONE'S SOURCES IS WRITTEN.
#   The measurement target is never opened for writing by anything here; the
#   clone's `.lake/build/doc` (doc-gen4's reference tree) is never touched; no
#   `git commit` is ever run.
#
# ============================================================================
# WHAT IS SHARED BETWEEN THE TWO IMPLEMENTATIONS, AND WHAT IS NOT
# ============================================================================
#   SHARED (impl-neutral, built once, in $SHARED)
#     the module lists (before / after each edit) and every IR tree the
#     **extractor** produces — base IR and the two from-scratch IRs. The
#     extractor is the same program on both sides, so building these twice would
#     only add a way for the two sides to be handed different inputs.
#     One module list is built here and handed to both sides 【M3-d2 実測】: the
#     extractor keeps the order it is given, and that order makes the bytes of the
#     ledger's `modules` array and of the merged `index.json`.
#
#   PER IMPL (in $OUT.work/fixtures)
#     `base-site/`, `base-state/`, `base-ledger.json`. `extractKey.extractor` and
#     `STATE_DERIVATION` are **designed** to differ between implementations (plan
#     §6): feeding one side's seed to the other reports every module changed and
#     measures the key mismatch instead of the pipeline.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INCREMENTAL_SH="$REPO/experiments/stage7h/incremental.sh"
LEDGER_TS="$REPO/experiments/stage5/ledger.ts"
GLOBAL_TS="$REPO/experiments/stage7h/global.ts"
RENDER_TS="$REPO/experiments/stage7d/render.ts"
EXTRACTOR="$REPO/experiments/stage7g/extract-once.sh"
SETUP_CLONE="$REPO/experiments/stage5e/setup-clone.sh"
RUST_BIN="$REPO/target/release/lean-doc"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
# `diff` is aliased to a colordiff that is not installed here; its exit 127 reads
# as "differences found" and has already cost this project one wrong conclusion.
DIFF=/usr/bin/diff

PHASE="${1-}"
case "$PHASE" in
  setup|move|delete|reset|all) shift ;;
  *) echo "usage: $0 setup|move|delete|reset|all [--impl ts|rust] [--clone DIR]" >&2
     exit 2 ;;
esac

IMPL=rust
CLONE=/private/tmp/lean-doc-relay/clone
ROOT=/private/tmp/lean-doc-relay/m3d4
OUT=
SHARED=
LIDX=/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx
LIB=InformationTheory
JOBS=4
# See the heading: the default is a module that exercises **both** L3-1 and L3-2.
MOVE_MODULE=InformationTheory.Shannon.BroadcastChannel.Basic

while [ $# -gt 0 ]; do
  case "$1" in
    --impl) IMPL="$2"; shift 2 ;;
    --move-module) MOVE_MODULE="$2"; shift 2 ;;
    --clone) CLONE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --shared) SHARED="$2"; shift 2 ;;
    --lidx) LIDX="$2"; shift 2 ;;
    --lib) LIB="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$IMPL" in
  ts) OUT="${OUT:-$ROOT/ref}" ;;
  rust) OUT="${OUT:-$ROOT/rust}" ;;
  *) echo "--impl wants ts or rust, not $IMPL" >&2; exit 2 ;;
esac
SHARED="${SHARED:-$ROOT/shared}"
WORKROOT="$OUT.work"
FIX="$WORKROOT/fixtures"

# The measurement target. Named only so that every path this script writes can be
# checked against it.
TARGET=/Users/haruka/dev/lean-projects
case "$CLONE" in
  "$TARGET"|"$TARGET"/*) echo "the clone may not be inside the measurement target" >&2; exit 2 ;;
esac
[ -d "$CLONE" ] || { echo "missing clone: $CLONE" >&2; exit 1; }
[ -f "$LIDX" ] || { echo "missing link index: $LIDX" >&2; exit 1; }
[ -x "$EXTRACTOR" ] || { echo "missing extractor: $EXTRACTOR" >&2; exit 1; }
[ -x "$RUST_BIN" ] || {
  echo "missing: $RUST_BIN — run: cargo build --release -p lean-doc" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }

# `extract-once.sh` sources benchmarks/tools/env.sh and `cd`s to $TARGET_REPO.
# Everything Lean-facing in this run has to look at the clone, and the pipeline
# spawns the extractor as a child, so this is exported rather than passed.
export TARGET_REPO="$CLONE"

# The clone's own HEAD, 40 lower-case hex digits, because `lean-doc incremental`
# refuses anything else (plan 決定 1). Unquoted on every use below: a
# `--source-url` that still carries its quotes renders into every page and turns
# the whole site into a difference.
REV="$(git -C "$CLONE" rev-parse HEAD)"
case "$REV" in
  [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
  *) echo "the clone's HEAD is not 40 hex digits: $REV" >&2; exit 2 ;;
esac
URL="https://github.com/FujiHaruka/information-theory/blob/$REV"

# The move's parameters, and the module the deletion removes.
A_MOD="$MOVE_MODULE"
X_MOD="${A_MOD}Core"
A_REL="$(printf '%s' "$A_MOD" | tr '.' '/').lean"
X_REL="$(printf '%s' "$X_MOD" | tr '.' '/').lean"
DEL_MOD="$LIB.Shannon.ArithmeticCoding"
DEL_REL="$(printf '%s' "$DEL_MOD" | tr '.' '/').lean"
ROOT_LEAN="$LIB.lean"

# The denominators, written down rather than derived, so that a run which
# silently produced the wrong number of pages fails instead of redefining the
# question. 6 = the whole-package artifacts.
GLOBAL_FILES=6
EXPECT_MOVE=439
EXPECT_DELETE=437

GLOBAL_ARTIFACTS="declarations/declaration-data.bmp declarations/name-map.json \
navbar.html tactics.html references.html references.bib"

WORK_FILES="changed.txt removed.txt render-all.txt seen.txt ir-changed.txt \
global-set.txt impact-set.txt render-set.txt name-map-before.json \
global-delta.json prune.json"

mkdir -p "$OUT" "$WORKROOT" "$FIX" "$SHARED"

# ------------------------------------------------------- the two implementations

if [ "$IMPL" = ts ]; then
  command -v deno >/dev/null || { echo "deno is required (node is broken here)" >&2; exit 1; }
  for f in "$INCREMENTAL_SH" "$LEDGER_TS" "$GLOBAL_TS" "$RENDER_TS"; do
    [ -f "$f" ] || { echo "missing: $f" >&2; exit 1; }
  done
  deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }

  ledger_build () { # ledger_build <modules> <ir> <out>
    deno_ "$LEDGER_TS" build --modules "$1" --target "$CLONE" --ir "$2" \
      --source-url "$URL" --out "$3"
  }
  ledger_check () { # ledger_check <ledger> <ir> <modules> <changed> <removed> <renderall>
    deno_ "$LEDGER_TS" check --ledger "$1" --ir "$2" --source-url "$URL" \
      --modules "$3" --changed-out "$4" --removed-out "$5" --render-all-out "$6"
  }
  # `render.ts` **does** take --link-index (render.ts:53) even though
  # `incremental.sh` never passes it, so it is passed here: this side's base is
  # then the same site the product's base is, which is what the per-scenario page
  # listings are differences from.
  base_site () { # base_site <ir> <site> <state>
    deno_ "$RENDER_TS" --ir "$1" --pages "$2" --source-url "$URL" --link-index "$LIDX"
    deno_ "$GLOBAL_TS" build --ir "$1" --out "$2" --state "$3"
  }
  pipeline () { # pipeline <ir> <pages> <ledger> <work> <state> <modules> <mode> <timings>
    "$INCREMENTAL_SH" --ir "$1" --pages "$2" --ledger "$3" --work "$4" \
      --global new --state "$5" --modules "$6" --mode "$7" --source-url "$URL" \
      --timings "$8" --l3-1 on --jobs "$JOBS"
  }
else
  ledger_build () {
    "$RUST_BIN" ledger build --modules "$1" --target "$CLONE" --ir "$2" \
      --source-url "$URL" --out "$3"
  }
  ledger_check () {
    "$RUST_BIN" ledger check --ledger "$1" --ir "$2" --source-url "$URL" \
      --modules "$3" --changed-out "$4" --removed-out "$5" --render-all-out "$6"
  }
  base_site () { # one command: `site` writes the pages, the six artifacts and the cache
    "$RUST_BIN" site --ir "$1" --out "$2" --source-url "$URL" \
      --link-index "$LIDX" --state "$3"
  }
  pipeline () {
    "$RUST_BIN" incremental --ir "$1" --pages "$2" --ledger "$3" --work "$4" \
      --state "$5" --modules "$6" --mode "$7" --source-url "$URL" \
      --link-index "$LIDX" --timings "$8" \
      --extractor "$EXTRACTOR" --extractor-arg --jobs --extractor-arg "$JOBS"
  }
fi

# ------------------------------------------------------------------- plumbing

guard_writable () { # guard_writable <path>
  case "$1" in
    "$WORKROOT"/*|"$OUT"/*|"$SHARED"/*) ;;
    *) echo "refusing to write outside $WORKROOT / $OUT / $SHARED: $1" >&2; exit 1 ;;
  esac
}

nlines () { grep -c . "$1" 2>/dev/null || true; }

# One module list, built under LC_ALL=C from the clone's **sources**, checked
# against `lean-doc modules`, and handed to both implementations. The check is
# what documents the trap (M3-d2) instead of merely avoiding it.
modlist () { # modlist <out file>
  guard_writable "$1"
  ( cd "$CLONE" && LC_ALL=C find "$ROOT_LEAN" "$LIB" -name '*.lean' | LC_ALL=C sort ) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  "$RUST_BIN" modules --root "$CLONE" --lib "$LIB" > "$1.from-cli"
  if ! "$DIFF" -q "$1" "$1.from-cli" > /dev/null; then
    echo "the LC_ALL=C source glob and \`lean-doc modules\` disagree — the run would" >&2
    echo "compare two different module orders, which makes two different ledgers and" >&2
    echo "two different index.json files. Refusing." >&2
    "$DIFF" "$1" "$1.from-cli" | head -20 >&2
    exit 4
  fi
  echo "  $(nlines "$1") modules -> $1"
}

# An IR tree is reused only when it is a tree of **exactly** the modules asked
# for, in the order asked for. Keying the cache on the scenario's name alone is
# not enough: `--move-module` changes which modules the post-move list holds
# while the scenario is still called `move`, and a stale tree then becomes the
# from-scratch side of gate 1. Measured, not imagined — it happened on the first
# run with a second module, and gate 1 caught it as 42 differing files.
extract_all () { # extract_all <modules> <ir dir> <log prefix>
  guard_writable "$2"
  if [ -f "$2/index.json" ]; then
    if python3 - "$2/index.json" "$1" <<'PY'
import json, sys
index, listing = sys.argv[1], sys.argv[2]
with open(index, encoding="utf-8") as f:
    have = [m["module"] if isinstance(m, dict) else m for m in json.load(f)["modules"]]
with open(listing, encoding="utf-8") as f:
    want = [line.strip() for line in f if line.strip()]
sys.exit(0 if have == want else 1)
PY
    then
      echo "  reusing $2"; return 0
    fi
    echo "  $2 holds a different module list — re-extracting"
  fi
  rm -rf "$2"
  echo "  extracting $(nlines "$1") modules -> $2"
  "$EXTRACTOR" --modules "$1" --ir-dir "$2" --jobs "$JOBS" \
    --timings "$3.json" > "$3.log"
  python3 -c "
import json,sys
r=json.load(open('$3.json'))
print('  extract: %.2f s, %s modules' % (r.get('total',0.0), r.get('targetModules','?')))"
}

# ------------------------------------------------------------- the clone's state
#
# baseline | moved | deleted | unknown. Every phase states which one it needs.
clone_state () {
  local dirty x del
  dirty="$(git -C "$CLONE" status --porcelain)"
  x=no; [ -f "$CLONE/$X_REL" ] && x=yes
  del=yes; [ -f "$CLONE/$DEL_REL" ] || del=no
  if [ -z "$dirty" ] && [ "$x" = no ] && [ "$del" = yes ]; then echo baseline
  elif [ "$x" = yes ] && [ "$del" = yes ]; then echo moved
  elif [ "$x" = no ] && [ "$del" = no ]; then echo deleted
  else echo unknown; fi
}

# The oleans must have been built at the clone's own path. If they were copied
# from the measurement target, the referrers of a moved declaration rebuild for
# the wrong reason and **the gate passes for a reason nobody meant** (stage 5e).
require_own_oleans () {
  local probe="$CLONE/.lake/build/lib/lean/${A_MOD//.//}.olean"
  local dump="$WORKROOT/olean-strings.txt"
  [ -f "$probe" ] || { echo "no olean to probe: $probe" >&2; exit 2; }
  # Dumped to a file rather than piped: `grep -q` closes the pipe early, and with
  # `pipefail` a SIGPIPE'd `strings` would make a *successful* match look like a
  # failure.
  strings "$probe" 2>/dev/null > "$dump" || true
  grep -q "$CLONE" "$dump" || {
    echo "the clone's oleans were not built at the clone's path — run" >&2
    echo "experiments/stage5e/rebuild-own.sh first (stage 5e (e))" >&2; exit 2; }
  if grep -q "$TARGET/" "$dump"; then
    echo "the clone's oleans still name the measurement target's path" >&2; exit 2
  fi
}

# Lake agrees nothing is left to build, and the ledger agrees the oleans are the
# ones the base IR was extracted from. The second half only runs once a base
# ledger exists — which is exactly when it becomes able to say whether a `reset`
# put the tree back.
require_baseline () { # require_baseline <tag>
  local tag="$1" state
  state="$(clone_state)"
  [ "$state" = baseline ] || {
    echo "the clone is '$state', not 'baseline' — run: $0 reset" >&2; exit 3; }
  require_own_oleans
  (cd "$CLONE" && "$LAKE" build --no-build 2>&1) > "$WORKROOT/$tag-nobuild.txt" || {
    echo "lake build --no-build failed; see $WORKROOT/$tag-nobuild.txt" >&2; exit 3; }
  grep -q "All targets up-to-date" "$WORKROOT/$tag-nobuild.txt" || {
    echo "lake does not consider the clone up to date:" >&2
    tail -3 "$WORKROOT/$tag-nobuild.txt" >&2; exit 3; }
  echo "  clone baseline: $(tail -1 "$WORKROOT/$tag-nobuild.txt")"
  if [ -f "$FIX/base-ledger.json" ] && [ -f "$SHARED/modules-base.txt" ]; then
    ledger_check "$FIX/base-ledger.json" "$SHARED/base-ir" "$SHARED/modules-base.txt" \
      "$WORKROOT/$tag-fp-changed.txt" "$WORKROOT/$tag-fp-removed.txt" \
      "$WORKROOT/$tag-fp-renderall.txt" > "$WORKROOT/$tag-fixed-point.txt"
    local c r a
    c=$(nlines "$WORKROOT/$tag-fp-changed.txt")
    r=$(nlines "$WORKROOT/$tag-fp-removed.txt")
    a=$(nlines "$WORKROOT/$tag-fp-renderall.txt")
    printf '  ledger fixed point: %s changed, %s removed, %s render-all\n' "$c" "$r" "$a"
    [ "$c" = 0 ] && [ "$r" = 0 ] && [ "$a" = 0 ] || {
      echo "the base ledger is not a fixed point on this tree — the clone drifted," >&2
      echo "so nothing measured from here would mean what it says." >&2
      head -10 "$WORKROOT/$tag-fp-changed.txt" >&2; exit 3; }
  fi
}

# --------------------------------------------------------------- the recording
# Identical in shape to tools/incremental-reference.sh, so that
# tools/incremental-compare.sh reads this tree with no change at all.

record () { # record <name> <status>
  printf '%s\n' "$2" > "$OUT/$1-status.txt"
  if [ -s "$OUT/$1-stderr.txt" ]; then
    printf 'yes\n' > "$OUT/$1-complained.txt"
  else
    printf 'no\n' > "$OUT/$1-complained.txt"
  fi
}

counts () { # counts <name> <timings json>
  python3 - "$2" "$OUT/$1-counts.json" <<'PY'
import json, os, sys
src, dst = sys.argv[1], sys.argv[2]
keys = ["changed", "globalStale", "irChanged", "mode",
        "pagesRendered", "removed", "rounds", "staleFound"]
if os.path.isfile(src):
    with open(src, encoding="utf-8") as f:
        record = json.loads(f.read().strip())
    out = {key: record.get(key, "<absent>") for key in keys}
else:
    out = {"timings": "absent"}
with open(dst, "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2, sort_keys=True)
    f.write("\n")
PY
}

copy_work () { # copy_work <name> <work dir>
  local name="$1" work="$2" f
  rm -rf "$OUT/$name-work"
  mkdir -p "$OUT/$name-work"
  : > "$OUT/$name-work-present.txt"
  for f in $WORK_FILES; do
    if [ -f "$work/$f" ]; then
      cp "$work/$f" "$OUT/$name-work/$f"
      printf '%s\n' "$f" >> "$OUT/$name-work-present.txt"
    fi
  done
  LC_ALL=C sort "$OUT/$name-work-present.txt" -o "$OUT/$name-work-present.txt"
}

copy_globals () { # copy_globals <dest dir> <page tree>
  local dest="$1" pages="$2" f
  rm -rf "$dest"
  for f in $GLOBAL_ARTIFACTS; do
    mkdir -p "$dest/$(dirname "$f")"
    if [ -f "$pages/$f" ]; then cp "$pages/$f" "$dest/$f"
    else printf 'absent\n' > "$dest/$f.absent"; fi
  done
}

page_list () { # page_list <name> <page tree>
  local name="$1" pages="$2"
  if [ -d "$pages" ]; then
    ( cd "$pages" && find . -type f | sed 's|^\./||' | LC_ALL=C sort ) > "$OUT/$name-pages.txt"
  else
    : > "$OUT/$name-pages.txt"
  fi
  printf 'files %s\n' "$(nlines "$OUT/$name-pages.txt")" > "$OUT/$name-pages-count.txt"
}

# GATE 1. `--impl rust` only, and skipped by the comparator (there is nothing on
# the TS side to compare it with — decision 4). It asks the only question that
# decides correctness: **did the incremental run leave the tree a from-scratch
# run would have written**, byte for byte, at both layers.
#
# Note this is a *stronger* form than M3-d3's `<s>-sitecheck.txt`, which
# re-rendered the IR the round left behind. Here the from-scratch side is an
# independent extraction of the post-edit sources, so a merge that produced a
# wrong IR cannot cancel out.
gate1 () { # gate1 <name> <live dir> <from-scratch ir> <from-scratch site> <expected files>
  local name="$1" d="$2" fir="$3" fsite="$4" expect="$5"
  local inc_files scratch_files ir_status site_status
  inc_files=$(find "$d/pages" -type f | wc -l | tr -d ' ')
  scratch_files=$(find "$fsite" -type f | wc -l | tr -d ' ')
  ir_status=0;   "$DIFF" -r -q "$d/ir" "$fir"    > "$d/gate1-ir.diff"   2>&1 || ir_status=$?
  site_status=0; "$DIFF" -r -q "$d/pages" "$fsite" > "$d/gate1-site.diff" 2>&1 || site_status=$?
  {
    printf 'scenario            %s\n' "$name"
    printf 'expected files      %s\n' "$expect"
    printf 'incremental files   %s\n' "$inc_files"
    printf 'from-scratch files  %s\n' "$scratch_files"
    if [ "$inc_files" != "$expect" ] || [ "$scratch_files" != "$expect" ]; then
      printf 'DENOMINATOR         WRONG (expected %s)\n' "$expect"
    else
      printf 'denominator         ok\n'
    fi
    printf 'ir diff             %s\n' \
      "$([ "$ir_status" = 0 ] && echo identical || echo "$(nlines "$d/gate1-ir.diff") line(s)")"
    sed 's/^/  /' "$d/gate1-ir.diff"
    printf 'site diff           %s\n' \
      "$([ "$site_status" = 0 ] && echo identical || echo "$(nlines "$d/gate1-site.diff") line(s)")"
    sed 's/^/  /' "$d/gate1-site.diff"
    # A byte position for every file that differs — "they differ" is not a
    # finding, "they differ at byte N" is where the search starts.
    if [ "$site_status" != 0 ] || [ "$ir_status" != 0 ]; then
      printf 'first differing byte\n'
      cat "$d/gate1-ir.diff" "$d/gate1-site.diff" \
        | sed -n 's/^Files \(.*\) and \(.*\) differ$/\1|\2/p' \
        | while IFS='|' read -r x y; do
            printf '  %s\n' "$(cmp "$x" "$y" 2>&1 || true)"
          done
    fi
    printf 'GATE 1              %s\n' \
      "$([ "$ir_status" = 0 ] && [ "$site_status" = 0 ] \
         && [ "$inc_files" = "$expect" ] && [ "$scratch_files" = "$expect" ] \
         && echo PASS || echo FAIL)"
  } > "$OUT/$name-sitecheck.txt"
  cat "$OUT/$name-sitecheck.txt"
}

# ------------------------------------------------------------------ the phases

# setup — the base every scenario starts from. Needs the clone at baseline and
# leaves it there.
phase_setup () {
  echo "### setup ($IMPL)"
  require_baseline setup
  modlist "$SHARED/modules-base.txt"
  extract_all "$SHARED/modules-base.txt" "$SHARED/base-ir" "$SHARED/base-extract"
  cp "$SHARED/modules-base.txt" "$OUT/modules-base.txt"

  echo "  base site + state ($IMPL)"
  rm -rf "$FIX/base-site" "$FIX/base-state"
  mkdir -p "$FIX/base-site" "$FIX/base-state"
  base_site "$SHARED/base-ir" "$FIX/base-site" "$FIX/base-state" \
    > "$WORKROOT/base-site.log" 2>&1
  copy_globals "$OUT/base-global" "$FIX/base-site"
  page_list base "$FIX/base-site"

  echo "  base ledger ($IMPL)"
  ledger_build "$SHARED/modules-base.txt" "$SHARED/base-ir" "$FIX/base-ledger.json" \
    > "$WORKROOT/base-ledger.log" 2>&1

  # Now that the ledger exists, prove it is a fixed point on the tree it was just
  # built from. A base that is not one would report phantom changes in every
  # scenario below.
  require_baseline setup-after
  printf 'ok\n' > "$FIX/base-ready.txt"
  echo "  base ready: $(nlines "$OUT/base-pages.txt") files in the base site"
}

# One scenario, once the clone already carries its edit.
run_scenario () { # run_scenario <name> <modules-after> <from-scratch ir> <expected>
  local name="$1" modules="$2" fir="$3" expect="$4"
  local d="$WORKROOT/$name" status=0
  guard_writable "$d"
  [ -f "$FIX/base-ready.txt" ] || {
    echo "no base for --impl $IMPL — run: $0 setup --impl $IMPL" >&2; exit 3; }

  rm -rf "$d"; mkdir -p "$d/work"
  cp -R "$SHARED/base-ir" "$d/ir"
  cp -R "$FIX/base-site" "$d/pages"
  cp "$FIX/base-ledger.json" "$d/ledger.json"
  cp -R "$FIX/base-state" "$d/state"

  echo "### $name ($IMPL, mode self)"
  pipeline "$d/ir" "$d/pages" "$d/ledger.json" "$d/work" "$d/state" \
    "$modules" self "$d/timings.json" \
    > "$OUT/$name-stdout.txt" 2> "$OUT/$name-stderr.txt" || status=$?
  record "$name" "$status"
  counts "$name" "$d/timings.json"
  copy_work "$name" "$d/work"
  rm -rf "$OUT/$name-ir"; cp -R "$d/ir" "$OUT/$name-ir"
  copy_globals "$OUT/$name-global" "$d/pages"
  page_list "$name" "$d/pages"
  printf '  exit %s, %s file(s) in the tree\n' "$status" "$(nlines "$OUT/$name-pages.txt")"
  python3 -c "
import json
r = json.load(open('$OUT/$name-counts.json'))
print('  rounds %s, changed %s, staleFound %s, globalStale %s, irChanged %s, pagesRendered %s'
      % (r.get('rounds'), r.get('changed'), r.get('staleFound'), r.get('globalStale'),
         r.get('irChanged'), r.get('pagesRendered')))" || true

  if [ "$IMPL" = rust ]; then
    rm -rf "$d/scratch-site" "$d/scratch-state"
    "$RUST_BIN" site --ir "$fir" --out "$d/scratch-site" --source-url "$URL" \
      --link-index "$LIDX" --state "$d/scratch-state" > "$d/scratch-site.log" 2>&1
    gate1 "$name" "$d" "$fir" "$d/scratch-site" "$expect"
  fi
}

# move — A's body into a new module, A a one-line shim, `lake build`.
phase_move () {
  local state; state="$(clone_state)"
  case "$state" in
    baseline)
      require_baseline move
      echo "### applying the move ($A_MOD -> $X_MOD) and rebuilding"
      "$SETUP_CLONE" move "$CLONE" "$A_MOD" minimal > "$WORKROOT/move-edit.log" 2>&1 || {
        echo "the move failed; see $WORKROOT/move-edit.log" >&2
        tail -20 "$WORKROOT/move-edit.log" >&2; exit 1; }
      grep -E "^A is now|error" "$WORKROOT/move-edit.log" || true ;;
    moved) echo "### the clone already carries the move — reusing it" ;;
    *) echo "the clone is '$state'; the move needs 'baseline' or 'moved'" >&2; exit 3 ;;
  esac
  [ -f "$CLONE/$X_REL" ] || { echo "the move did not create $X_REL" >&2; exit 3; }

  modlist "$SHARED/modules-move.txt"
  cp "$SHARED/modules-move.txt" "$OUT/modules-move.txt"
  grep -qx "$X_MOD" "$SHARED/modules-move.txt" || {
    echo "$X_MOD is not in the post-move module list" >&2; exit 3; }
  [ "$(nlines "$SHARED/modules-move.txt")" = "$((EXPECT_MOVE - GLOBAL_FILES))" ] || {
    echo "post-move module count is $(nlines "$SHARED/modules-move.txt"), expected $((EXPECT_MOVE - GLOBAL_FILES))" >&2
    exit 3; }
  extract_all "$SHARED/modules-move.txt" "$SHARED/move-ir" "$SHARED/move-extract"
  run_scenario move "$SHARED/modules-move.txt" "$SHARED/move-ir" "$EXPECT_MOVE"
}

# delete — the source file goes, the one import line naming it goes, `lake build`.
# The orphaned olean is deliberately left in `.lake/build/lib`.
phase_delete () {
  local state; state="$(clone_state)"
  case "$state" in
    baseline)
      require_baseline delete
      echo "### deleting $DEL_MOD and rebuilding"
      rm "$CLONE/$DEL_REL"
      python3 - "$CLONE/$ROOT_LEAN" "$DEL_MOD" <<'PY'
import sys
path, mod = sys.argv[1], sys.argv[2]
line = "import %s" % mod
with open(path, encoding="utf-8") as f:
    lines = f.read().split("\n")
kept = [l for l in lines if l.strip() != line]
dropped = len(lines) - len(kept)
if dropped != 1:
    sys.exit("expected exactly one `%s` line in %s, found %d" % (line, path, dropped))
with open(path, "w", encoding="utf-8") as f:
    f.write("\n".join(kept))
print("  dropped 1 import line from %s" % path)
PY
      (cd "$CLONE" && "$LAKE" build 2>&1 | tail -3) > "$WORKROOT/delete-build.log" 2>&1 || {
        echo "lake build failed after the deletion; see $WORKROOT/delete-build.log" >&2
        cat "$WORKROOT/delete-build.log" >&2; exit 1; }
      cat "$WORKROOT/delete-build.log"
      # The orphan is the point: Lake does not remove it, and the module list is
      # a glob over the sources, so the module still has to read as gone.
      if [ -f "$CLONE/.lake/build/lib/lean/${DEL_MOD//.//}.olean" ]; then
        echo "  orphan olean left in place (deliberate): ${DEL_MOD//.//}.olean"
      else
        echo "  note: lake removed the olean after all" >&2
      fi ;;
    deleted) echo "### the clone already carries the deletion — reusing it" ;;
    *) echo "the clone is '$state'; the deletion needs 'baseline' or 'deleted'" >&2; exit 3 ;;
  esac
  [ ! -f "$CLONE/$DEL_REL" ] || { echo "$DEL_REL is still there" >&2; exit 3; }

  modlist "$SHARED/modules-delete.txt"
  cp "$SHARED/modules-delete.txt" "$OUT/modules-delete.txt"
  if grep -qx "$DEL_MOD" "$SHARED/modules-delete.txt"; then
    echo "$DEL_MOD is still in the post-deletion module list" >&2; exit 3
  fi
  [ "$(nlines "$SHARED/modules-delete.txt")" = "$((EXPECT_DELETE - GLOBAL_FILES))" ] || {
    echo "post-deletion module count is $(nlines "$SHARED/modules-delete.txt"), expected $((EXPECT_DELETE - GLOBAL_FILES))" >&2
    exit 3; }
  extract_all "$SHARED/modules-delete.txt" "$SHARED/delete-ir" "$SHARED/delete-extract"
  run_scenario delete "$SHARED/modules-delete.txt" "$SHARED/delete-ir" "$EXPECT_DELETE"
}

# reset — put the clone back and **prove** it went back. A scenario run on a tree
# that did not return measures nothing.
phase_reset () {
  echo "### reset"
  "$SETUP_CLONE" reset "$CLONE" > "$WORKROOT/reset.log" 2>&1 || {
    echo "reset failed; see $WORKROOT/reset.log" >&2; tail -20 "$WORKROOT/reset.log" >&2; exit 1; }
  tail -3 "$WORKROOT/reset.log"
  require_baseline reset
  echo "  reset verified: state=$(clone_state), HEAD=$(git -C "$CLONE" rev-parse --short HEAD)"
}

conditions () {
  {
    printf 'date              %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'phase             %s\n' "$PHASE"
    printf 'impl              %s\n' "$IMPL"
    printf 'host              %s / %s / %s GB\n' \
      "$(uname -srm)" \
      "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?')" \
      "$(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024))"
    printf 'clone             %s (HEAD %s, state %s)\n' "$CLONE" "$REV" "$(clone_state)"
    printf 'move module       %s -> %s\n' "$A_MOD" "$X_MOD"
    printf 'delete module     %s\n' "$DEL_MOD"
    printf 'lean-toolchain    %s\n' "$(tr -d '\n' < "$CLONE/lean-toolchain" 2>/dev/null || echo '?')"
    printf 'extractor         %s (stage 7d binary, IR schema 4)\n' "$EXTRACTOR"
    printf 'jobs              %s\n' "$JOBS"
    printf 'link index        %s (%s B)\n' "$LIDX" "$(wc -c < "$LIDX" | tr -d ' ')"
    printf 'shared            %s\n' "$SHARED"
    printf 'source url        %s\n' "$URL"
    printf 'deno              %s\n' "$(deno --version 2>/dev/null | head -1 || echo 'not used')"
    printf 'rustc             %s\n' "$(rustc --version 2>/dev/null || echo '?')"
  } > "$OUT/conditions.txt"
}

case "$PHASE" in
  setup)  phase_setup ;;
  move)   phase_move ;;
  delete) phase_delete ;;
  reset)  phase_reset ;;
  all)    phase_setup; phase_move; phase_reset; phase_delete; phase_reset ;;
esac

conditions
( cd "$OUT" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 ) > "$OUT.sha256"

printf 'phase: %s\nimpl: %s\nout: %s\nwork: %s\nfiles: %s\nmanifest: %s\n' \
  "$PHASE" "$IMPL" "$OUT" "$WORKROOT" \
  "$(find "$OUT" -type f | wc -l | tr -d ' ')" "$OUT.sha256"
