#!/usr/bin/env bash
# Build a Lean package **and its documentation** in one job.
#
# Milestone **M6**. This is the body of the CI job; `.github/workflow-templates/
# lean-doc-docs.yml` is a wrapper that checks things out and calls this file.
#
# ============================================================================
# WHY THE COMMANDS ARE HERE AND NOT IN THE YAML
# ============================================================================
#   GitHub Actions cannot be run from where this was written (no runner, no
#   network), so a workflow that carried the commands inline would be a file
#   nobody had ever executed. Put the commands in a script and the workflow
#   shrinks to checkout + caches + one `run:` line: the interesting half can
#   then be run on a laptop, against a real package, and the untested remainder
#   is reduced to the actions themselves. What has NOT been executed is stated
#   in README.md and in the workflow's own header — it is not implied to work.
#
# ============================================================================
# WHY `lake build` AND THE DOCS ARE IN THE SAME JOB  【実測 → approach.md §3, §8】
# ============================================================================
#   The extractor's floor is loading the Lean environment, and that cost is I/O,
#   not Lean: on a Linux runner the same import took **2.61 s when `lake build`
#   had run in the same job and 20-89 s when it had not** — 8-34x, decided by
#   whether the oleans were still in the page cache. A separate "docs" job
#   starts on a fresh runner with a cold cache and pays the high side even
#   though `actions/cache` restored the same bytes.
#
#   So the placement is the point of this script: `lake build` first, then
#   `lean-doc build`, in one job, one runner, one page cache. Splitting them is
#   the single change that can make documentation generation an order of
#   magnitude slower without anything looking broken.
#
# ============================================================================
# HOW MATHLIB'S OLEANS ARE OBTAINED  — `lake exe cache get`, and why it is a flag
# ============================================================================
#   A Mathlib-dependent package does not compile Mathlib; it downloads the
#   prebuilt oleans with `lake exe cache get`. That needs the network, so it is
#   behind `--cache-get` rather than being unconditional:
#
#     * in CI it is what you want, and the workflow passes it;
#     * on a developer machine (and in this repository's own measurements) the
#       dependencies are already there, and an unconditional network call would
#       make a local run of "the CI command" not the same command;
#     * `~/.cache/mathlib` is what `actions/cache` should key on
#       `lake-manifest.json` — the download is the slow part, not the unpack.
#
#   A run without `--cache-get` says so in its log, with the reason. A silent
#   skip would be the failure mode where CI passes because the last run's
#   leftovers were still on disk.
#
# ============================================================================
# WHEN THE EXTRACTOR IS BUILT
# ============================================================================
#   `extractor/build.sh` compiles Extract.lean against **the package's own
#   toolchain** (`lake env` borrows it — lean-doc has no toolchain, no lakefile
#   and no Mathlib of its own, CLAUDE.md). It therefore cannot be shipped as a
#   binary and cannot be built before the package's toolchain exists. It also
#   does not change between commits of the package, so in CI it belongs in a
#   cache keyed on `lean-toolchain` + the hash of `Extract.lean` — see the
#   workflow. Here: built if `--extractor-bin` is missing, skipped if present,
#   and either way the phase is timed and reported.
#
#   Cost, measured: **14.90 s wall / 10.07 s user, peak RSS 1.52 GB** on an
#   Apple M1 with a warm page cache 【実測 2026-08-15】. It is cached because it
#   does not change between commits, not because it is enormous — on a cold
#   runner the environment import inside it is the 2.61-89 s spread above, and
#   that is the part that is not measured here.
#
#   Same measurement, one thing worth knowing: built against a *different*
#   package (the same toolchain and the same `.lake/packages`), the binary came
#   out **byte for byte identical** (SHA-256 47f95072...). That is evidence for
#   the cache key, not proof of it: the two packages' dependency sets were
#   copies of each other.
#
# ============================================================================
# WHAT THIS NEVER DOES
# ============================================================================
#   It never writes inside `--root` beyond what `lake build` itself writes:
#   `--out` is refused by `lean-doc build` if it is under `--root`, and this
#   script refuses it earlier so that the run stops before it has done anything.
#   The documentation tree is `<out>/site`; a caller who wants it inside the
#   repository copies it there.
#
# usage:
#   ci-build.sh --root <lean package> --out <dir> [options] [-- <build args>...]
#
#   --root <dir>            the Lean package to build and document (required)
#   --out <dir>             where the documentation state goes (required).
#                           <out>/site is the site; the rest is cache.
#   --cache-get             run `lake exe cache get` in --root first (network)
#   --no-lake-build         skip `lake build` — only for a caller that has
#                           already built the package **in the same job**
#   --jobs <n>              extractor threads (default 4)
#   --lake <path>           the lake executable (default: $LAKE, else `lake`)
#   --extractor-bin <path>  the Lean extractor (default: <repo>/extractor/build/
#                           extract, built by extractor/build.sh if missing)
#   --lean-doc-bin <path>   the lean-doc binary (default: <repo>/target/release/
#                           lean-doc, built with cargo if missing)
#   --timings <file>        phase timings as one JSON object
#                           (default: <out>/ci-timings.json)
#   -- <args>...            passed through to `lean-doc build` (e.g. --lib,
#                           --source-url, --full)
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

ROOT=""
OUT=""
JOBS=4
LAKE="${LAKE:-lake}"
CACHE_GET=0
LAKE_BUILD=1
EXTRACTOR_BIN="${EXTRACT_BIN:-$REPO/extractor/build/extract}"
LEAN_DOC_BIN="${LEAN_DOC_BIN:-$REPO/target/release/lean-doc}"
TIMINGS=""
BUILD_ARGS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --cache-get) CACHE_GET=1; shift ;;
    --no-lake-build) LAKE_BUILD=0; shift ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --lake) LAKE="$2"; shift 2 ;;
    --extractor-bin) EXTRACTOR_BIN="$2"; shift 2 ;;
    --lean-doc-bin) LEAN_DOC_BIN="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --) shift; BUILD_ARGS=("$@"); break ;;
    -h|--help) sed -n '/^# usage:/,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//;$d'; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

[ -n "$ROOT" ] || { echo "--root is required" >&2; exit 2; }
[ -n "$OUT" ] || { echo "--out is required" >&2; exit 2; }
[ -d "$ROOT" ] || { echo "no such package: $ROOT" >&2; exit 1; }

ROOT="$(cd "$ROOT" && pwd)"

# Stated against the paths rather than left to the later stage: `lean-doc build`
# refuses this too, but by then the run has already spent `lake build`.
#
# **Before `mkdir`, and twice.** Creating the directory is itself a write into
# the package (M4-b paid for exactly this: a guard that ran after the directory
# existed), so the lexical form is checked while --out is still just a string;
# and again after it is resolved, because a symlink can land inside --root.
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac
refuse_out_inside_root () {
  case "$1" in
    "$ROOT"|"$ROOT"/*) echo "--out may not be inside --root ($ROOT)" >&2; exit 2 ;;
  esac
}
refuse_out_inside_root "$OUT"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
refuse_out_inside_root "$OUT"
[ -n "$TIMINGS" ] || TIMINGS="$OUT/ci-timings.json"

# ------------------------------------------------------------------ timing
#
# `date` on BSD has no sub-second format, so the clock is bash 5's
# $EPOCHREALTIME with a perl fallback. Every phase is timed, including the ones
# that are skipped (a note says why), so that a reader can see which step a slow
# job spent its minutes in. The phases do not quite add up to the total: the
# version banner above is outside all of them.
now () {
  if [ -n "${EPOCHREALTIME:-}" ]; then printf '%s' "${EPOCHREALTIME/,/.}"
  else perl -MTime::HiRes -e 'printf "%.6f", Time::HiRes::time()'; fi
}
elapsed () { awk -v a="$1" -v b="$2" 'BEGIN{printf "%.3f", b-a}'; }

PHASE_NAME=()
PHASE_SECS=()
PHASE_NOTE=()
record () { PHASE_NAME+=("$1"); PHASE_SECS+=("$2"); PHASE_NOTE+=("$3"); }

step () { echo; echo "=== $* ==="; }

T0="$(now)"

# ------------------------------------------------------------------ versions
step "environment"
echo "package     $ROOT"
echo "output      $OUT"
echo "lean-doc    $REPO"
if [ -f "$ROOT/lean-toolchain" ]; then
  echo "toolchain   $(tr -d '\n' < "$ROOT/lean-toolchain")"
fi
# Asked from inside the package: `lake` is an elan shim that picks the
# toolchain from the nearest `lean-toolchain`, and lean-doc has none of its own
# (CLAUDE.md), so the same command run from this repository answers "not found".
echo "lake        $( (cd "$ROOT" && "$LAKE" --version 2>&1 | head -1) || echo 'not found')"
echo "uname       $(uname -srm)"
if command -v git > /dev/null && git -C "$ROOT" rev-parse HEAD > /dev/null 2>&1; then
  echo "HEAD        $(git -C "$ROOT" rev-parse HEAD)"
fi

# ------------------------------------------------------------------ 1 cache get
step "1/5  lake exe cache get"
t="$(now)"
if [ "$CACHE_GET" = 1 ]; then
  (cd "$ROOT" && "$LAKE" exe cache get)
  record cache-get "$(elapsed "$t" "$(now)")" "ran"
else
  echo "skipped: --cache-get was not given, so the dependencies' oleans are"
  echo "         whatever is already in $ROOT/.lake/packages."
  record cache-get "$(elapsed "$t" "$(now)")" "skipped (no --cache-get)"
fi

# ------------------------------------------------------------------ 2 lake build
#
# The placement this whole script exists for. It is also the step that decides
# whether the extractor's imports are a 2.61 s read or a 20-89 s one.
step "2/5  lake build"
t="$(now)"
if [ "$LAKE_BUILD" = 1 ]; then
  (cd "$ROOT" && "$LAKE" build)
  record lake-build "$(elapsed "$t" "$(now)")" "ran"
else
  echo "skipped: --no-lake-build. This is only correct if the package was built"
  echo "         earlier in THIS job; from another job the page cache is cold."
  record lake-build "$(elapsed "$t" "$(now)")" "skipped (--no-lake-build)"
fi

# ------------------------------------------------------------------ 3 extractor
step "3/5  the extractor (Lean)"
t="$(now)"
if [ -x "$EXTRACTOR_BIN" ]; then
  echo "cached: $EXTRACTOR_BIN"
  record extractor "$(elapsed "$t" "$(now)")" "cached"
elif [ "$EXTRACTOR_BIN" = "$REPO/extractor/build/extract" ]; then
  echo "building with extractor/build.sh (borrowing $ROOT's toolchain)"
  TARGET_REPO="$ROOT" LAKE="$LAKE" "$REPO/extractor/build.sh"
  record extractor "$(elapsed "$t" "$(now)")" "built"
else
  echo "no extractor at $EXTRACTOR_BIN, and it is not the path extractor/build.sh" >&2
  echo "writes — build it there, or drop --extractor-bin." >&2
  exit 1
fi

# ------------------------------------------------------------------ 4 lean-doc
step "4/5  the lean-doc binary (Rust)"
t="$(now)"
if [ -x "$LEAN_DOC_BIN" ]; then
  echo "cached: $LEAN_DOC_BIN"
  record cargo "$(elapsed "$t" "$(now)")" "cached"
elif [ "$LEAN_DOC_BIN" = "$REPO/target/release/lean-doc" ]; then
  (cd "$REPO" && cargo build --release -p lean-doc)
  record cargo "$(elapsed "$t" "$(now)")" "built"
else
  echo "no lean-doc binary at $LEAN_DOC_BIN" >&2
  exit 1
fi

# ------------------------------------------------------------------ 5 the docs
#
# One command. --lib comes from the lakefile, the module list from the source
# glob, --source-url from git, the dependency map from the environment the
# extractor imports anyway, and the choice between full generation and the
# incremental path from what is already under --out.
step "5/5  lean-doc build"
t="$(now)"
"$LEAN_DOC_BIN" build \
  --root "$ROOT" \
  --out "$OUT" \
  --extractor-bin "$EXTRACTOR_BIN" \
  --lake "$LAKE" \
  --jobs "$JOBS" \
  --timings "$OUT/lean-doc-timings.json" \
  "${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}"
record docs "$(elapsed "$t" "$(now)")" "ran"

TOTAL="$(elapsed "$T0" "$(now)")"

# ------------------------------------------------------------------ the report
step "summary"
printf '%-12s %10s  %s\n' phase seconds note
for i in "${!PHASE_NAME[@]}"; do
  printf '%-12s %10s  %s\n' "${PHASE_NAME[$i]}" "${PHASE_SECS[$i]}" "${PHASE_NOTE[$i]}"
done
printf '%-12s %10s\n' total "$TOTAL"
echo
echo "site        $OUT/site  ($(find "$OUT/site" -type f | wc -l | tr -d ' ') files)"

{
  printf '{"root":"%s","out":"%s","totalSeconds":%s,"phases":{' "$ROOT" "$OUT" "$TOTAL"
  for i in "${!PHASE_NAME[@]}"; do
    [ "$i" = 0 ] || printf ','
    printf '"%s":{"seconds":%s,"note":"%s"}' \
      "${PHASE_NAME[$i]}" "${PHASE_SECS[$i]}" "${PHASE_NOTE[$i]}"
  done
  printf '},"siteFiles":%s}\n' "$(find "$OUT/site" -type f | wc -l | tr -d ' ')"
} > "$TIMINGS"
echo "timings     $TIMINGS"
