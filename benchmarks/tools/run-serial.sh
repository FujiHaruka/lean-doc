#!/bin/bash
# Method A: one doc-gen4 process per module, the way Lake drives `single` today.
set -u
cd /Users/haruka/dev/lean-projects
export LEAN_PATH=$(cat /tmp/leanpath.txt)
BENCH=$PWD/docs/doc-gen-bench/raw
export DOCGEN_TIMING=$BENCH/serial.jsonl
rm -f "$DOCGEN_TIMING" .lake/build/bench-a.db*
DG=.lake/packages/doc-gen4/.lake/build/bin/doc-gen4
start=$(date +%s)
n=0
while read -r m; do
  "$DG" single --build .lake/build "$m" bench-a.db "file:///tmp/x" > /dev/null 2>&1
  n=$((n+1))
done < "$BENCH/it-modules.txt"
end=$(date +%s)
echo "modules=$n wall_seconds=$((end-start))" > "$BENCH/serial-summary.txt"
