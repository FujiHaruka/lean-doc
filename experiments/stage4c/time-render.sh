#!/usr/bin/env bash
# Times `render.ts --pages` -- the whole consumer side of judgement point 2:
# schema-2 IR in, 432 module HTML pages on disk out, Lean never started.
#
# One process per run, wrapped in `/usr/bin/time -l`, so every run records
# wall / user / sys / peak RSS / page faults. The in-process phase timers come
# out of `render.ts --timings` and are merged into the same JSONL record, so a
# reader can check the phases against the wall clock of the very same run.
#
# usage:
#   time-render.sh <name> [options]
#     --runs N        repetitions (default 6). Run 1 is dropped as cold-ish,
#                     matching stage 4 increment 2's rule; the reported number
#                     is the median of runs 2..N.
#     --warmup        do one discarded run before run 1 (the warm series)
#     --cold          no warm-up, and each run gets a fresh output directory
#                     (the first run of a session)
#     --limit N       pass --limit N to render.ts (linearity check)
#     --empty         time an empty Deno script instead: the start-up floor
#     --no-flatten-probe   pass it through (moves rope flattening to `write`)
#     --ir DIR        schema-2 IR (default $IR_DIR)
#     --work DIR      scratch root for the generated pages (default $WORK_DIR)
#
# Writes:
#   benchmarks/results/<name>.jsonl   one record per run (committed)
#   benchmarks/results/<name>.txt     conditions + per-run table + medians
#
# The output pages go under --work, NEVER inside the measurement target
# /Users/haruka/dev/lean-projects. Nothing here writes to that repository.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$LD/benchmarks/results}"

NAME="${1:?usage: time-render.sh <name> [--runs N] [--warmup|--cold] [--limit N] [--empty]}"
shift

RUNS=6
WARMUP=0
COLD=0
LIMIT=""
EMPTY=0
EXTRA=()
IR="${IR_DIR:-}"
WORK="${WORK_DIR:-}"
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

while [ $# -gt 0 ]; do
  case "$1" in
    --runs) RUNS="$2"; shift 2 ;;
    --warmup) WARMUP=1; shift ;;
    --cold) COLD=1; shift ;;
    --limit) LIMIT="$2"; shift 2 ;;
    --empty) EMPTY=1; shift ;;
    --ir) IR="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --no-flatten-probe) EXTRA+=("--no-flatten-probe"); shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[ -n "$IR" ] || { echo "set IR_DIR or pass --ir" >&2; exit 2; }
[ -n "$WORK" ] || { echo "set WORK_DIR or pass --work" >&2; exit 2; }
case "$WORK" in
  /Users/haruka/dev/lean-projects*) echo "refusing to write into the measurement target" >&2; exit 2 ;;
esac
mkdir -p "$WORK" "$RESULTS_DIR"

OUT="$RESULTS_DIR/$NAME.jsonl"
SUMMARY="$RESULTS_DIR/$NAME.txt"
RAW="$WORK/raw-$NAME"
mkdir -p "$RAW"
rm -f "$OUT"

# The start-up floor. Same permission flags as render.ts so the two are
# comparable, and it reports the same `boot` quantity render.ts reports (time
# from process start to the first line of user code) -- measured before the one
# small write it does, so the write is not in it.
EMPTY_JS="$WORK/empty.ts"
cat > "$EMPTY_JS" <<'EOF'
const boot = performance.now();
const out = Deno.args[0];
if (out) {
  await Deno.writeTextFile(out, JSON.stringify({ bootSeconds: boot / 1000 }) + "\n");
}
EOF

# --------------------------------------------------------------- conditions
{
  echo "# $NAME"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "deno              $(deno --version | head -1) / $(deno --version | sed -n 2p)"
  echo "measured          $([ "$EMPTY" = 1 ] && echo 'empty script (Deno start-up floor)' || echo "render.ts --pages (one HTML page per IR module; ${LIMIT:-432} modules)")"
  echo "ir                $IR"
  echo "pages out         $WORK/pages-$NAME"
  echo "parallelism       1 (one process, no workers, no --jobs)"
  echo "cache             $([ "$COLD" = 1 ] && echo 'session first run, no warm-up (cold side)' || echo "warm-up run before run 1: $([ "$WARMUP" = 1 ] && echo yes || echo no)")"
  echo "limit             ${LIMIT:-(none: all modules)}"
  echo "extra             ${EXTRA[*]:-(none)}"
  echo "runs              $RUNS (run 1 dropped; median over runs 2..$RUNS)"
  echo
} | tee "$SUMMARY"

# --------------------------------------------------------------- one run
run_once () {   # run_once <index> <pages-dir>
  local i="$1" pages="$2"
  local tl="$RAW/time-$i.txt" tj="$RAW/timings-$i.json" st="$RAW/stats-$i.txt"
  if [ "$EMPTY" = 1 ]; then
    printf '{}\n' > "$tj"
    python3 "$HERE/merge-timing.py" --name "$NAME" --run "$i" --time-l "$tl" --timings "$tj" \
      --exec -- "$(command -v deno)" run --allow-read --allow-write "$EMPTY_JS" "$tj" >> "$OUT"
  else
    python3 "$HERE/merge-timing.py" --name "$NAME" --run "$i" --time-l "$tl" --timings "$tj" \
      --exec -- "$(command -v deno)" run --allow-read --allow-write "$HERE/render.ts" \
      --ir "$IR" --pages "$pages" --source-url "$SOURCE_URL" \
      --timings "$tj" --stats "$st" \
      ${LIMIT:+--limit "$LIMIT"} ${EXTRA[@]+"${EXTRA[@]}"} >> "$OUT"
  fi
}

PAGES_SHARED="$WORK/pages-$NAME"
if [ "$COLD" = 1 ]; then
  : # each run gets its own directory, created below
else
  mkdir -p "$PAGES_SHARED"
  if [ "$WARMUP" = 1 ]; then
    echo "warm-up run (discarded, not in the JSONL)"
    run_once 0 "$PAGES_SHARED"
    : > "$OUT"   # the warm-up must not appear in the record
  fi
fi

for i in $(seq 1 "$RUNS"); do
  if [ "$COLD" = 1 ]; then
    run_once "$i" "$WORK/pages-$NAME-$i"
  else
    run_once "$i" "$PAGES_SHARED"
  fi
  echo "  run $i done"
done

# --------------------------------------------------------------- summary
python3 "$HERE/merge-timing.py" --summarize "$OUT" | tee -a "$SUMMARY"

echo
echo "jsonl   -> $OUT"
echo "summary -> $SUMMARY"
echo "raw     -> $RAW"
