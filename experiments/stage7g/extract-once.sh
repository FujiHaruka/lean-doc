#!/usr/bin/env bash
# One extractor process over an arbitrary module list, with its phase timers
# flattened into a single JSON object so that `time-step.sh` can merge them with
# `/usr/bin/time -l`.
#
# Stage 7g's copy of `stage5/extract-once.sh`. Two differences:
#
#   * `BIN` is the **stage-7d** extractor (IR schema 4). Stage 5's copy still
#     points at stage 4b's (schema 3) and is left untouched — docs quote its
#     numbers.
#   * `--jobs N` is passed through to the one-shot path. The resident path has
#     **no** per-request job count: the server was started with one and uses it
#     for every request, so the caller starts the server with the same N (see
#     `serve-ctl.sh`). `--jobs` is accepted here even with `--serve-dir` so that
#     the two call sites read the same; it is recorded, not sent.
#
# `--serve-dir` swaps the one process for a request to a resident one started by
# `stage7g/serve-ctl.sh`. It is here rather than in `incremental.sh` on purpose:
# the events -> timings conversion below then applies to *both* paths unchanged,
# so the two are numerically comparable, which is the whole point of the
# comparison. With the flag absent nothing about this script changes.
#
# usage:
#   extract-once.sh --modules <list> --ir-dir <dir> --timings <path> [--events <path>]
#                   [--jobs N] [--serve-dir <server dir>]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../benchmarks/tools/env.sh
source "$HERE/../../benchmarks/tools/env.sh"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
BIN="${EXTRACT_BIN:-$HERE/../stage7d/build/extract}"

MODULES=""; IRDIR=""; TIMINGS=""; EVENTS=""; SERVEDIR=""; JOBS="${JOBS:-1}"
while [ $# -gt 0 ]; do
  case "$1" in
    --modules) MODULES="$2"; shift 2 ;;
    --ir-dir) IRDIR="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --events) EVENTS="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --serve-dir) SERVEDIR="$2"; shift 2 ;;
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

if [ -n "$SERVEDIR" ]; then
  # The resident path. `--serve`'s reply carries its own nanosecond timer, but the
  # authority here is the same wall clock the one-shot path is measured with, so
  # the reply is only echoed.
  "$HERE/serve-ctl.sh" request "$SERVEDIR" "$MODULES" "$EVENTS" "$IRDIR"
else
  cd "$TARGET_REPO"
  "$LAKE" env "$BIN" "$MODULES" "$EVENTS" \
    --equations --refs --write-ir --tagged-code --jobs "$JOBS" --ir-dir "$IRDIR" > /dev/null
fi

python3 - "$EVENTS" "$TIMINGS" "$MODULES" "$JOBS" <<'PY'
import json, sys
events, out, modules, jobs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
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
rec["jobsRequested"] = int(jobs)
json.dump(rec, open(out, "w", encoding="utf-8"))
PY
