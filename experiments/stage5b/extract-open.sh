#!/usr/bin/env bash
# E3 (leg 8 / judgement point 3, S3): one extractor process with `--open`.
#
# `experiments/stage5/extract-once.sh` has no way to pass `--open`, and it is the
# script the increment-1 numbers were taken with, so it is left alone. This is a
# copy of its command line with the one flag added; everything else (binary,
# flags, argument order, the timing flattening) is identical, so an `--open` run
# and an `--open`-off run differ by exactly that flag.
#
# The extractor is `experiments/stage4b/build/extract`, unchanged.
#
# usage:
#   extract-open.sh --modules <list> --ir-dir <dir> --timings <path>
#                   [--open <ns>[,<ns>]] [--events <path>]
#
#   --open omitted  =>  no `--open` flag is passed at all (the baseline shape)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../benchmarks/tools/env.sh
source "$HERE/../../benchmarks/tools/env.sh"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
BIN="$HERE/../stage4b/build/extract"

MODULES=""; IRDIR=""; TIMINGS=""; EVENTS=""; OPENNS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --modules) MODULES="$2"; shift 2 ;;
    --ir-dir) IRDIR="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --events) EVENTS="$2"; shift 2 ;;
    --open) OPENNS="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODULES" ] && [ -n "$IRDIR" ] && [ -n "$TIMINGS" ] || {
  echo "usage: extract-open.sh --modules <list> --ir-dir <dir> --timings <path> [--open <ns>]" >&2
  exit 2; }
# The target is read-only for every experiment in this tree.
case "$IRDIR" in
  /Users/haruka/dev/lean-projects*) echo "refusing to write into the measurement target" >&2; exit 2 ;;
esac

EVENTS="${EVENTS:-${TIMINGS%.json}-events.jsonl}"
rm -f "$EVENTS"
mkdir -p "$IRDIR"

OPEN_ARGS=()
if [ -n "$OPENNS" ]; then OPEN_ARGS=(--open "$OPENNS"); fi

cd "$TARGET_REPO"
# `${a[@]+"${a[@]}"}`: bash 3.2 (macOS) treats an empty array as unset under `set -u`.
"$LAKE" env "$BIN" "$MODULES" "$EVENTS" \
  --equations --refs --write-ir --tagged-code --ir-dir "$IRDIR" \
  ${OPEN_ARGS[@]+"${OPEN_ARGS[@]}"} > /dev/null

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
