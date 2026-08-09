#!/usr/bin/env bash
# Method A: one doc-gen4 process per module, the way Lake drives `single` today.
# This is the baseline that the batch method (23x) is measured against.
#
# Usage: run-serial.sh [timing-log-basename]
set -u

. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

NAME=${1:-serial}
MODULES="$RESULTS_DIR/it-modules.txt"
[ -f "$MODULES" ] || { echo "module list not found: $MODULES" >&2; exit 1; }

cd "$TARGET_REPO"
export LEAN_PATH=$(lake env printenv LEAN_PATH)
export DOCGEN_TIMING="$RESULTS_DIR/$NAME.jsonl"
rm -f "$DOCGEN_TIMING" .lake/build/bench-a.db*

start=$(date +%s)
n=0
while read -r m; do
  "$DOCGEN_BIN" single --build .lake/build "$m" bench-a.db "file:///tmp/x" > /dev/null 2>&1
  n=$((n+1))
done < "$MODULES"
end=$(date +%s)
echo "modules=$n wall_seconds=$((end-start))" > "$RESULTS_DIR/$NAME-summary.txt"
