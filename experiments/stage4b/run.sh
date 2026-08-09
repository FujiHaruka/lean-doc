#!/usr/bin/env bash
# Runs the stage-4b extractor over the fixed measurement target and records both
# the timing JSONL and the conditions the numbers were taken under.
#
# usage: run.sh [name] [-- extra args for the binary]
#          name defaults to `stage4b-extract`
#
# Extra args are passed through to the extractor, e.g.
#   run.sh stage4b-noir -- --equations --refs
#   run.sh stage4b-ir  -- --equations --refs --write-ir --ir-dir "$IR_DIR"
#   run.sh stage4b-tag -- --equations --refs --write-ir --tagged-code --ir-dir "$IR_DIR"
#
# The IR is written OUTSIDE the measurement target on purpose; never point
# --ir-dir inside $TARGET_REPO.
#
# MODULES=<list> and RESULTS_DIR=<dir> override the target module list and where
# the output lands; use them for smoke runs so nothing is written next to the
# committed measurements.
#
# Writes:
#   benchmarks/results/<name>.jsonl          raw timing records (committed)
#   benchmarks/results/<name>-summary.txt    conditions + stdout + peak RSS
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../benchmarks/tools/env.sh
source "$HERE/../../benchmarks/tools/env.sh"

LAKE="${LAKE:-$HOME/.elan/bin/lake}"
NAME="${1:-stage4b-extract}"
shift || true
if [ "${1:-}" = "--" ]; then shift; fi
EXTRA=("$@")

MODULES="${MODULES:-$RESULTS_DIR/it-modules.txt}"
BIN="$HERE/build/extract"
OUT="$RESULTS_DIR/$NAME.jsonl"
SUMMARY="$RESULTS_DIR/$NAME-summary.txt"

[ -x "$BIN" ] || { echo "not built: run $HERE/build.sh first" >&2; exit 1; }
[ -f "$MODULES" ] || { echo "module list not found: $MODULES" >&2; exit 1; }
rm -f "$OUT"

{
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$TARGET_REPO/lean-toolchain")"
  echo "LEAN_NUM_THREADS  ${LEAN_NUM_THREADS:-unset (Lean default)}"
  echo "target            $TARGET_REPO"
  echo "modules           $(grep -c . "$MODULES") from $MODULES"
  echo "binary            $BIN"
  echo "args              ${EXTRA[*]:-(none)}"
  echo
} | tee "$SUMMARY"

cd "$TARGET_REPO"
# `/usr/bin/time -l` reports peak RSS ("maximum resident set size", bytes).
"$LAKE" env /usr/bin/time -l "$BIN" "$MODULES" "$OUT" ${EXTRA[@]+"${EXTRA[@]}"} 2>&1 | tee -a "$SUMMARY"

echo
echo "timings -> $OUT"
echo "summary -> $SUMMARY"
