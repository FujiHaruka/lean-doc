#!/usr/bin/env bash
# What happens when a prebuilt `extract` meets the WRONG toolchain?
# (docs/plans/distribution.md, check X5)
#
# ============================================================================
# WHY THIS DECIDES WHETHER A PREBUILT EXTRACTOR CAN BE SHIPPED
# ============================================================================
#   Shipping one binary per toolchain means users will eventually run one
#   against a toolchain it was not built for — a package bumps `lean-toolchain`
#   and a cache, a pinned action, or a stale download hands over yesterday's
#   binary. Two outcomes are acceptable and one is not:
#
#     it refuses to run          -> fine, the user sees an error and rebuilds
#     it runs and is correct     -> fine (and surprising)
#     it runs and writes an IR   -> NOT fine. Silent wrong output is worse than
#     that is subtly wrong          no distribution at all, because the docs
#                                   look built and nobody checks.
#
#   So this script's success condition is "it did NOT quietly succeed", and it
#   is red when the extractor comes back 0 while reading a foreign environment.
#
# ============================================================================
# WHY NO `lake build`, AND WHY THE MODULE IS `Init`
# ============================================================================
#   The module list names `Init`, which lives in the toolchain itself, so the
#   package needs no build output at all — only its `lean-toolchain` and enough
#   of a lakefile for `lake env` to resolve. That keeps this to an elan download
#   and seconds of work, and it puts the version skew exactly where it matters:
#   in the oleans `importModules` reads.
#
# usage:
#   extractor-mismatch.sh --extractor <bin> --package <dir> [--built-for <tc>]
#                         [--expect fail|any] [--json <path>]
#
#   --package     a Lean package whose lean-toolchain is NOT the one the
#                 extractor was built against (checked when --built-for is given)
set -euo pipefail

LAKE="${LAKE:-lake}"
EXTRACTOR=""
PACKAGE=""
BUILT_FOR=""
EXPECT="fail"
JSON=""

while [ $# -gt 0 ]; do
  case "$1" in
    --extractor) EXTRACTOR="$2"; shift 2 ;;
    --package) PACKAGE="$2"; shift 2 ;;
    --built-for) BUILT_FOR="$2"; shift 2 ;;
    --expect) EXPECT="$2"; shift 2 ;;
    --json) JSON="$2"; shift 2 ;;
    -h|--help) sed -n '/^# usage:/,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//;$d'; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

[ -x "$EXTRACTOR" ] || { echo "--extractor must be an executable" >&2; exit 2; }
[ -d "$PACKAGE" ] || { echo "--package must be a directory" >&2; exit 2; }

# **Absolute, before anything runs.** The extractor is launched from inside
# --package, so a relative path resolves against the wrong directory: the first
# run of this script reported "REFUSED — the mismatch is visible as a failure"
# for a binary that had never started. A non-zero exit is only evidence about
# the toolchain if the process actually ran.
EXTRACTOR="$(cd "$(dirname "$EXTRACTOR")" && pwd)/$(basename "$EXTRACTOR")"
PACKAGE="$(cd "$PACKAGE" && pwd)"
[ -f "$PACKAGE/lean-toolchain" ] || { echo "no lean-toolchain in $PACKAGE" >&2; exit 2; }

HAVE="$(tr -d '\n' < "$PACKAGE/lean-toolchain")"
if [ -n "$BUILT_FOR" ] && [ "$BUILT_FOR" = "$HAVE" ]; then
  echo "the package pins the toolchain the extractor was built for ($HAVE)." >&2
  echo "this experiment needs a MISMATCH to mean anything." >&2
  exit 2
fi

echo "extractor   $EXTRACTOR"
[ -n "$BUILT_FOR" ] && echo "built for   $BUILT_FOR"
echo "package     $PACKAGE  (pins $HAVE)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "Init" > "$WORK/modules.txt"

set +e
( cd "$PACKAGE" && "$LAKE" env "$EXTRACTOR" "$WORK/modules.txt" "$WORK/out.jsonl" ) \
  > "$WORK/stdout.txt" 2> "$WORK/stderr.txt"
CODE=$?
set -e

BYTES=0
[ -f "$WORK/out.jsonl" ] && BYTES="$(wc -c < "$WORK/out.jsonl" | tr -d ' ')"

echo
echo "=== what happened"
echo "exit code   $CODE"
echo "wrote       $BYTES bytes of IR"
echo "--- stderr (first 5 lines)"
head -5 "$WORK/stderr.txt" || true

if [ -n "$JSON" ]; then
  printf '{"builtFor":"%s","ran_against":"%s","exitCode":%s,"irBytes":%s,"stderrFirstLine":"%s"}\n' \
    "$BUILT_FOR" "$HAVE" "$CODE" "$BYTES" \
    "$(head -1 "$WORK/stderr.txt" 2>/dev/null | tr -d '"\\' | cut -c1-200)" > "$JSON"
  echo "json        $JSON"
fi

echo
# **"It exited non-zero" is not the finding.** A process that never started
# exits non-zero too, and reporting that as "the mismatch was refused" would be
# a green light bought with a broken invocation — the same shape as CLAUDE.md's
# `eprintln!("skipping") + return`. So the not-launched case is separated out,
# and it is an error, not a pass.
if grep -q "could not execute external process" "$WORK/stderr.txt" 2>/dev/null; then
  echo "DID NOT RUN: lake could not launch the extractor at all." >&2
  echo "That is a broken invocation, not evidence about toolchains." >&2
  exit 2
fi

if [ "$CODE" -ne 0 ]; then
  echo "REFUSED: the mismatch is visible to the user as a failure, not as bad output."
  exit 0
fi

if [ "$EXPECT" = "any" ]; then
  echo "RAN CLEAN against a foreign toolchain (--expect any, so not a failure here)."
  echo "Whether the IR is correct is a separate question this script does not answer."
  exit 0
fi

echo "QUIETLY SUCCEEDED against a foreign toolchain — and wrote $BYTES bytes." >&2
echo "This is the outcome that forbids shipping a prebuilt extractor without a" >&2
echo "version check of its own: nothing here would tell a user their docs were" >&2
echo "built by the wrong binary." >&2
exit 1
