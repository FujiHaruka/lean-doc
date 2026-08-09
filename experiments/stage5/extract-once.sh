#!/usr/bin/env bash
# One extractor process over an arbitrary module list, with its phase timers
# flattened into a single JSON object so that `time-step.sh` can merge them with
# `/usr/bin/time -l`.
#
# The extractor itself is `experiments/stage4b/build/extract`, unchanged: stage 5
# measures the *incremental path*, not a new extractor. Re-extracting one module
# is the same binary with a one-line module list.
#
# usage:
#   extract-once.sh --modules <list> --ir-dir <dir> --timings <path> [--events <path>]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../benchmarks/tools/env.sh
source "$HERE/../../benchmarks/tools/env.sh"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
BIN="$HERE/../stage4b/build/extract"

MODULES=""; IRDIR=""; TIMINGS=""; EVENTS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --modules) MODULES="$2"; shift 2 ;;
    --ir-dir) IRDIR="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --events) EVENTS="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODULES" ] && [ -n "$IRDIR" ] && [ -n "$TIMINGS" ] || {
  echo "usage: extract-once.sh --modules <list> --ir-dir <dir> --timings <path>" >&2; exit 2; }
case "$IRDIR" in
  /Users/haruka/dev/lean-projects*) echo "refusing to write into the measurement target" >&2; exit 2 ;;
esac

EVENTS="${EVENTS:-${TIMINGS%.json}-events.jsonl}"
rm -f "$EVENTS"
mkdir -p "$IRDIR"

cd "$TARGET_REPO"
"$LAKE" env "$BIN" "$MODULES" "$EVENTS" \
  --equations --refs --write-ir --tagged-code --ir-dir "$IRDIR" > /dev/null

python3 - "$EVENTS" "$TIMINGS" "$MODULES" <<'PY'
import json, sys
events, out, modules = sys.argv[1], sys.argv[2], sys.argv[3]
rec = {}
for line in open(events, encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    e = json.loads(line)
    p = e["phase"].replace("stage4b.", "")
    rec[p] = e["us"] / 1e6
    for k, v in e.items():
        if k not in ("phase", "pid", "us"):
            rec[p + ":" + k] = v
rec["targetModules"] = sum(
    1 for l in open(modules, encoding="utf-8")
    if l.strip() and not l.startswith("#")
)
json.dump(rec, open(out, "w", encoding="utf-8"))
PY
