#!/usr/bin/env bash
# One arm of the placement A/B: build the documentation N times and leave behind
# what makes the numbers readable.
#
# The caller decides the arm by what it has already done to the machine — this
# script is identical on both sides:
#
#   arm `same`   is run in a job that has just finished `lake build`.
#   arm `split`  is run on a fresh runner that restored the same oleans as bytes.
#
# **Only run 1 is a placement measurement.** It is the one that pays whatever
# the page cache does not already hold. Runs 2..N are that runner's warm floor,
# and their real job is to normalise the arm against its own machine: the same
# `ubuntu-latest` label covers instances whose pure-CPU speed differs by 2.19x
# 【実測 → docs/verification-log.md「CI 軸」】, so a raw same-vs-split ratio is
# confounded by which machines the two arms landed on. run1/runN inside one arm
# is not.
#
# Each run gets a fresh `--out`, so every run is a full build and none of them
# takes the incremental path.
#
# THE THIRD ARM, AND WHY A NULL RESULT NEEDS IT
#   arm `cold` is the same runner as `same` with the page cache dropped before
#   every build. It is not a placement — it is the **positive control**. If the
#   A/B comes back "no difference", that reading is worth nothing unless the
#   instrument is known to be able to show one; `cold` is the condition where a
#   difference must appear, and if it does not, the measurement is broken rather
#   than the claim being refuted. Dropping the cache is therefore allowed to
#   fail the run: a `cold` arm that was not cold is a control that lies.
#
# environment:
#   ARM            same | split | cold                    (required)
#   PAIR           the matrix pair this arm belongs to    (default 1)
#   REPS           builds in this arm                     (default 2)
#   LIB            the package's lean_lib                 (default InformationTheory)
#   JOBS           extractor threads                      (default: nproc)
#   DROP           1 = drop the page cache before each build (default 0)
#   APPEND         1 = add to an existing inventory rather than start one
#   LEAN_DOC_DIR   this repository                        (default ./lean-doc)
#   TARGET_DIR     the Lean package to document           (default ./target)
#   RESULTS        where the results go                   (default ./results)
#   WORK           where the builds go                    (default $RUNNER_TEMP)
set -euo pipefail

ARM="${ARM:?ARM must be same, split or cold}"
PAIR="${PAIR:-1}"
REPS="${REPS:-2}"
LIB="${LIB:-InformationTheory}"
DROP="${DROP:-0}"
APPEND="${APPEND:-0}"
LEAN_DOC_DIR="${LEAN_DOC_DIR:-lean-doc}"
TARGET_DIR="${TARGET_DIR:-target}"
RESULTS="${RESULTS:-results}"
WORK="${WORK:-${RUNNER_TEMP:-/tmp}}"
JOBS="${JOBS:-$( (nproc 2> /dev/null || sysctl -n hw.ncpu) 2> /dev/null || echo 2)}"

case "$ARM" in
  same | split | cold) ;;
  *) echo "ARM must be same, split or cold, not '$ARM'" >&2; exit 2 ;;
esac
[ -d "$LEAN_DOC_DIR" ] || { echo "no lean-doc at $LEAN_DOC_DIR" >&2; exit 2; }
[ -d "$TARGET_DIR" ] || { echo "no package at $TARGET_DIR" >&2; exit 2; }

LEAN_DOC_DIR="$(cd "$LEAN_DOC_DIR" && pwd)"
TARGET_DIR="$(cd "$TARGET_DIR" && pwd)"
mkdir -p "$RESULTS" "$WORK"
RESULTS="$(cd "$RESULTS" && pwd)"

# GNU `time -v` gives major faults and peak RSS; BSD's spells it `-l`. A run
# without either still measures — the phase timings come from ci-build.sh — so
# this degrades rather than refuses.
TIME_CMD=""
if /usr/bin/time -v true > /dev/null 2>&1; then TIME_CMD="/usr/bin/time -v"
elif /usr/bin/time -l true > /dev/null 2>&1; then TIME_CMD="/usr/bin/time -l"
fi

# ---------------------------------------------------------------- the inventory
#
# Written BEFORE the first build, so that a run that dies leaves a declared line
# with nothing behind it and `check-placement.sh` says so. An inventory written
# afterwards would only ever list what happened, which is the failure mode this
# repository has already paid for twice (CLAUDE.md「ゲートは走った本数を数える」).
[ "$APPEND" = 1 ] || : > "$RESULTS/runs.txt"
for i in $(seq 1 "$REPS"); do
  echo "$ARM p${PAIR}r${i}" >> "$RESULTS/runs.txt"
done
echo "### declared $REPS run(s) for arm $ARM, pair $PAIR, jobs $JOBS, drop=$DROP"
cat "$RESULTS/runs.txt"

for i in $(seq 1 "$REPS"); do
  id="${ARM}-p${PAIR}r${i}"
  out="$WORK/docs-$id"
  rm -rf "$out"

  echo
  echo "########## $id ##########"

  # No `|| true`: a control arm that silently stayed warm would report "no
  # difference" from a condition that was never established.
  if [ "$DROP" = 1 ]; then
    sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'
    grep -E '^(MemFree|MemAvailable|Cached)' /proc/meminfo > "$RESULTS/meminfo-$id.txt"
    echo "dropped the page cache before $id"
  fi
  # shellcheck disable=SC2086
  $TIME_CMD "$LEAN_DOC_DIR/tools/ci-build.sh" \
    --root "$TARGET_DIR" \
    --out "$out" \
    --no-lake-build \
    --jobs "$JOBS" \
    --timings "$RESULTS/timings-$id.json" \
    -- --lib "$LIB" \
    2>&1 | tee "$RESULTS/time-$id.txt" | tail -20

  # The site's bytes, path-independent: `--out` differs per run, so the digest
  # is taken over paths relative to the site root.
  (cd "$out/site" && find . -type f | sort | xargs shasum -a 256 | shasum -a 256) \
    > "$RESULTS/site-$id.sha256"
  echo "site digest $(cut -d' ' -f1 < "$RESULTS/site-$id.sha256")"

  # The extractor's own phase log is where `importModules` lives — the phase the
  # placement claim is actually about. The end-to-end number contains CPU work
  # that placement cannot move.
  cp "$out/lean-doc-timings.json" "$RESULTS/leandoc-$id.json" 2> /dev/null || true

  # Freed before the next run: a full documentation tree per rep does not fit on
  # a runner alongside Mathlib.
  rm -rf "$out"
done

echo
echo "### $ARM pair $PAIR done"
ls -1 "$RESULTS"
