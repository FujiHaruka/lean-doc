#!/usr/bin/env bash
# The stage-5 change-detection block: how long it takes to answer "which of the
# 432 modules changed?" without starting Lean.
#
# Series:
#   sha256-c1     read + SHA-256 the 432 oleans, one at a time
#   sha256-c8     the same, 8 reads in flight
#   lake          read the `.olean.hash` Lake already wrote (16 B per module)
#   sha256-cold   sha256-c1 with the oleans evicted from the page cache first
#   lake-cold     `lake` with the same eviction
#
# The warm series are interleaved with each other; the cold series run
# afterwards, on their own. They cannot be interleaved with the warm ones: the
# eviction is what makes them cold, and it would make the next warm run cold
# too. Mixing them would produce five cold series and no warm ones.
#
# Eviction is benchmarks/tools/olean-evict (msync(MS_INVALIDATE), no sudo) over
# exactly the 432 target oleans, so it is surgical: deno, its transpile cache
# and the ledger file stay warm, and the only thing that has to be paged back in
# is what is being hashed.
#
# usage: block-ledger.sh [rounds]      (default 8; run 1 of each series dropped)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"
WORK="${WORK_DIR:?set WORK_DIR}"
IR="${IR_DIR:?set IR_DIR}"
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
ROUNDS="${1:-8}"
MODULES="$RESULTS_DIR/it-modules.txt"

RAW="$WORK/block-ledger"; mkdir -p "$RAW"

# The exact file set the cold series evicts: the 432 target modules' oleans.
OLEANS="$WORK/target-oleans.txt"
: > "$OLEANS"
while read -r m; do
  [ -n "$m" ] || continue
  for s in .olean .olean.server .olean.private; do
    p="$TARGET/.lake/build/lib/lean/$(echo "$m" | tr '.' '/')$s"
    [ -f "$p" ] && echo "$p" >> "$OLEANS"
  done
done < "$MODULES"
OLEAN_FILES=$(grep -c . "$OLEANS")
OLEAN_BYTES=$(xargs stat -f '%z' < "$OLEANS" | awk '{s+=$1} END {printf "%d", s}')
echo "eviction set: $OLEAN_FILES files, $OLEAN_BYTES B"

build () { # build <algorithm> <out>
  deno run --allow-read --allow-write --allow-env "$HERE/ledger.ts" build \
    --modules "$MODULES" --target "$TARGET" --ir "$IR" \
    --algorithm "$1" --out "$2" > /dev/null
}
build sha256 "$WORK/ledger-sha256.json"
build lake "$WORK/ledger-lake.json"

params () { # params <series> -> "<ledger> <algorithm> <concurrency> <evict?>"
  case "$1" in
    sha256-c1)   echo "$WORK/ledger-sha256.json sha256 1 0" ;;
    sha256-c8)   echo "$WORK/ledger-sha256.json sha256 8 0" ;;
    lake)        echo "$WORK/ledger-lake.json lake 1 0" ;;
    sha256-cold) echo "$WORK/ledger-sha256.json sha256 1 1" ;;
    lake-cold)   echo "$WORK/ledger-lake.json lake 1 1" ;;
    *) echo "unknown series: $1" >&2; exit 2 ;;
  esac
}

one_run () { # one_run <series> <index>
  local s="$1" i="$2"
  read -r L A C EV <<< "$(params "$s")"
  local tl="$RAW/time-$s-$i.txt" tj="$RAW/timings-$s-$i.json"
  printf '{}\n' > "$tj"
  if [ "$EV" = 1 ]; then
    "$LD/benchmarks/tools/olean-evict" < "$OLEANS" > "$RAW/evict-$s-$i.txt" 2>&1
  fi
  python3 "$LD/experiments/stage4c/merge-timing.py" --name "stage5-ledger-$s" --run "$i" \
    --time-l "$tl" --timings "$tj" --exec -- \
    "$(command -v deno)" run --allow-read --allow-write --allow-env "$HERE/ledger.ts" check \
    --ledger "$L" --algorithm "$A" --concurrency "$C" --ir "$IR" --timings "$tj" \
    >> "$RESULTS_DIR/stage5-ledger-$s.jsonl"
}

WARM=(sha256-c1 sha256-c8 lake)
COLD=(sha256-cold lake-cold)
for s in "${WARM[@]}" "${COLD[@]}"; do rm -f "$RESULTS_DIR/stage5-ledger-$s.jsonl"; done

for i in $(seq 1 "$ROUNDS"); do
  for s in "${WARM[@]}"; do one_run "$s" "$i"; done
  echo "  warm round $i done"
done
for s in "${COLD[@]}"; do
  for i in $(seq 1 "$ROUNDS"); do one_run "$s" "$i"; done
  echo "  cold series $s done"
done

cache_note () {
  case "$1" in
    *cold) echo "cold: olean-evict over the $OLEAN_FILES oleans before every run" ;;
    *)     echo "warm (interleaved with ${WARM[*]})" ;;
  esac
}

KEYS=readLedgerSeconds,keySeconds,hashSeconds,compareSeconds,totalSeconds,changed
for s in "${WARM[@]}" "${COLD[@]}"; do
  {
    echo "# stage5-ledger-$s"
    echo
    echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
    echo "deno              $(deno --version | head -1)"
    echo "hashed            432 modules / $OLEAN_FILES olean files / $OLEAN_BYTES B"
    echo "algorithm         $(params "$s" | cut -d' ' -f2), concurrency $(params "$s" | cut -d' ' -f3)"
    echo "cache             $(cache_note "$s")"
    echo "runs              $ROUNDS (run 1 dropped)"
    echo "Lean              never started"
    echo
    python3 "$HERE/summarize.py" "$RESULTS_DIR/stage5-ledger-$s.jsonl" --keys "$KEYS"
  } > "$RESULTS_DIR/stage5-ledger-$s.txt"
  echo "-> $RESULTS_DIR/stage5-ledger-$s.txt"
done
