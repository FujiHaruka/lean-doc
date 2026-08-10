#!/usr/bin/env bash
# Stage 7h — where the cache stops paying.
#
# The A/B measures one point: 6 of 433 modules changed (1.4%), which is what a
# one-module edit plus L3-1's second round produces. That is the case the whole
# incremental pipeline is built for, but it is not the only case a driver meets —
# a `lake update`, a `git pull`, or a rename that touches a namespace can miss on
# a large fraction of the package, and then the cache is doing the full read
# *plus* bookkeeping.
#
# So: invalidate N of the 433 cache entries and time `stage7h/global.ts build`
# against `stage5/global.ts build` on the same IR. The crossing point, if there
# is one, is a measurement rather than an argument.
#
# Invalidation is done by corrupting the stored `contentHash` of N entries, which
# is exactly what a real change does to the cache — the entry is there and the
# hash no longer matches, so the module is re-read.
#
# usage: probe-scale.sh <work-dir> [reps]
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W="${1:?work dir}"
N="${2:-5}"
IR="$W/ir"
P="$W/scale"
rm -rf "$P"; mkdir -p "$P"
: > "$P/scale.jsonl"

now () { python3 -c 'import time; print(repr(time.time()))'; }

# A full, valid cache to cut down from.
deno run --allow-read --allow-write "$HERE/global.ts" build --ir "$IR" \
  --out "$P/pages" --state "$P/state-full" > "$P/seed.log"

time_one () { # time_one <tag> <misses> <cmd...>
  local tag="$1" misses="$2"; shift 2
  local a b
  a=$(now)
  "$@" > "$P/$tag.log" 2>&1
  b=$(now)
  python3 - "$P/scale.jsonl" "$tag" "$misses" "$a" "$b" "$P/t.json" <<'PY'
import json, os, sys
out, tag, misses, a, b, tj = sys.argv[1:]
rec = {"tag": tag, "misses": int(misses), "wallSeconds": float(b) - float(a)}
if os.path.exists(tj):
    inner = json.load(open(tj, encoding="utf-8"))
    for k in ("readSeconds", "stateLoadSeconds", "writeSeconds", "stateSaveSeconds",
              "totalSeconds", "cacheHits", "cacheMisses"):
        if k in inner:
            rec[k] = inner[k]
open(out, "a", encoding="utf-8").write(json.dumps(rec) + "\n")
PY
}

echo "### stage7h scale probe — $N reps per point"
for i in $(seq 1 "$N"); do
  # The control, re-measured in the same loop so it moves with the machine.
  rm -rf "$P/pages-old"
  time_one old 433 deno run --allow-read --allow-write "$S5/global.ts" build \
    --ir "$IR" --out "$P/pages-old" --timings "$P/t.json"
  for m in 0 6 25 100 433; do
    rm -rf "$P/state-$m"
    cp -R "$P/state-full" "$P/state-$m"
    python3 - "$P/state-$m/global-state.json" "$m" <<'PY'
import json, sys
p, m = sys.argv[1], int(sys.argv[2])
st = json.load(open(p, encoding="utf-8"))
mods = list(st["modules"])
# Every k-th entry, so the invalidated set is spread over the package rather
# than clustered in one namespace.
if m > 0:
    step = max(1, len(mods) // m)
    for name in mods[::step][:m]:
        st["modules"][name]["contentHash"] = "invalidated0000"
json.dump(st, open(p, "w", encoding="utf-8"))
PY
    rm -rf "$P/pages-$m"
    time_one "new-$m" "$m" deno run --allow-read --allow-write "$HERE/global.ts" build \
      --ir "$IR" --out "$P/pages-$m" --state "$P/state-$m" --timings "$P/t.json"
  done
  echo "  rep $i done"
done
echo "-> $P/scale.jsonl"
