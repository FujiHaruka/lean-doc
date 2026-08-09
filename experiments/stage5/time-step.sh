#!/usr/bin/env bash
# Times one step of the incremental pipeline, N times, one process per run.
#
# Generic on purpose: stage 5 measures three different programs (the hash
# ledger, the Lean extractor, the HTML renderer) and the honest thing is to time
# them all the same way -- `/usr/bin/time -l` for CPU / peak RSS / page faults,
# a monotonic wall clock from outside, and whatever the program itself writes to
# its `--timings` file merged into the same record.
#
# usage:
#   time-step.sh <name> [--runs N] [--warmup] [--keys k1,k2] [--title T]
#                [--evict <file-list>] -- CMD...
#
#   The token {TIMINGS} anywhere in CMD is replaced by the per-run timings path.
#   --evict <list>  before every run, drop the page-cache pages of the files
#                   named in <list> (benchmarks/tools/olean-evict, no sudo).
#                   This is how the cold side is produced; without it the series
#                   is warm.
#   --warmup        one discarded run before run 1.
#
# Writes benchmarks/results/<name>.jsonl (one record per run) and <name>.txt.
# Never writes inside the measurement target.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"
WORK="${WORK_DIR:?set WORK_DIR to a scratch directory}"

NAME="${1:?usage: time-step.sh <name> [--runs N] [--warmup] -- CMD...}"
shift

RUNS=6
WARMUP=0
KEYS=""
TITLE=""
EVICT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --runs) RUNS="$2"; shift 2 ;;
    --warmup) WARMUP=1; shift ;;
    --keys) KEYS="$2"; shift 2 ;;
    --title) TITLE="$2"; shift 2 ;;
    --evict) EVICT="$2"; shift 2 ;;
    --) shift; break ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ $# -gt 0 ] || { echo "no command given (missing --)" >&2; exit 2; }
CMD=("$@")

case "$WORK" in
  /Users/haruka/dev/lean-projects*) echo "refusing to work inside the measurement target" >&2; exit 2 ;;
esac
mkdir -p "$WORK" "$RESULTS_DIR"
RAW="$WORK/raw-$NAME"; mkdir -p "$RAW"
OUT="$RESULTS_DIR/$NAME.jsonl"
SUMMARY="$RESULTS_DIR/$NAME.txt"
rm -f "$OUT"

{
  echo "# $NAME"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "command           ${CMD[*]}"
  echo "cache             $([ -n "$EVICT" ] && echo "cold (olean-evict over $EVICT before every run)" || echo "warm (warm-up run: $([ "$WARMUP" = 1 ] && echo yes || echo no))")"
  echo "runs              $RUNS$([ "$RUNS" -gt 1 ] && echo " (run 1 dropped; median over runs 2..$RUNS)")"
  echo
} | tee "$SUMMARY"

run_once () {
  local i="$1"
  local tl="$RAW/time-$i.txt" tj="$RAW/timings-$i.json"
  printf '{}\n' > "$tj"
  if [ -n "$EVICT" ]; then
    "$LD/benchmarks/tools/olean-evict" < "$EVICT" > "$RAW/evict-$i.txt" 2>&1
  fi
  local expanded=()
  for a in "${CMD[@]}"; do expanded+=("${a//\{TIMINGS\}/$tj}"); done
  python3 "$LD/experiments/stage4c/merge-timing.py" --name "$NAME" --run "$i" \
    --time-l "$tl" --timings "$tj" --exec -- "${expanded[@]}" >> "$OUT"
}

if [ "$WARMUP" = 1 ]; then
  echo "warm-up run (discarded)"
  run_once 0
  : > "$OUT"
fi
for i in $(seq 1 "$RUNS"); do
  run_once "$i"
  echo "  run $i done"
done

python3 "$HERE/summarize.py" "$OUT" ${KEYS:+--keys "$KEYS"} ${TITLE:+--title "$TITLE"} \
  | tee -a "$SUMMARY"

echo
echo "jsonl   -> $OUT"
echo "summary -> $SUMMARY"
