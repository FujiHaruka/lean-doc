#!/usr/bin/env bash
# Stage 5d, C6: does splitting L1 into two keys cost anything in `detect`?
#
# The two versions are **interleaved**, not run in blocks. Stage 5c used
# before/after/before because a block takes minutes and drift had to be bounded;
# here one `check` is ~0.1 s, so alternating them removes the cache-state
# confound outright instead of bounding it. (That confound is real: in run.sh
# the two variants' detect times differ by 40% purely because one of them
# rebuilt its ledger — and therefore warmed the 432 `.olean.hash` files —
# immediately beforehand.)
#
# BEFORE is the parent of the split commit, extracted to scratch rather than
# checked out, so nothing in the working tree moves while this runs.
#
# usage: compare-detect.sh <work-dir> <split-commit> [rounds]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
W=${1:?work dir}
SPLIT=${2:?the commit that split the key}
ROUNDS=${3:-20}
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
RESULTS="$LD/benchmarks/results"
MODLIST="$RESULTS/it-modules.txt"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

mkdir -p "$W"
IR="${IR_DIR:?set IR_DIR to a schema-2 IR tree}"

git -C "$LD" show "$SPLIT~1:experiments/stage5/ledger.ts" > "$W/ledger-before.ts"
cp "$LD/experiments/stage5/ledger.ts" "$W/ledger-after.ts"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }

# One ledger per version: the schemas differ (1 vs 2), which is the point.
deno_ "$W/ledger-before.ts" build --modules "$MODLIST" --target "$TARGET" --ir "$IR" \
  --algorithm lake --out "$W/ledger-before.json" > /dev/null
deno_ "$W/ledger-after.ts" build --modules "$MODLIST" --target "$TARGET" --ir "$IR" \
  --algorithm lake --source-url "$URL" --out "$W/ledger-after.json" > /dev/null

JSONL="$RESULTS/stage5d-detect-cost.jsonl"
: > "$JSONL"
for i in $(seq 1 "$ROUNDS"); do
  deno_ "$W/ledger-before.ts" check --ledger "$W/ledger-before.json" --ir "$IR" \
    --modules "$MODLIST" --timings "$W/t-before.json" > /dev/null
  python3 -c "
import json,sys
d=json.load(open('$W/t-before.json')); d['variant']='before'; d['run']=$i
print(json.dumps(d))" >> "$JSONL"

  deno_ "$W/ledger-after.ts" check --ledger "$W/ledger-after.json" --ir "$IR" \
    --modules "$MODLIST" --source-url "$URL" --timings "$W/t-after.json" > /dev/null
  python3 -c "
import json,sys
d=json.load(open('$W/t-after.json')); d['variant']='after'; d['run']=$i
print(json.dumps(d))" >> "$JSONL"
done

python3 - "$JSONL" "$RESULTS/stage5d-detect-cost.txt" "$ROUNDS" "$TARGET" <<'PY'
import json, statistics, subprocess, sys, datetime
src, out, rounds, target = sys.argv[1:]
rows = [json.loads(l) for l in open(src, encoding="utf-8") if l.strip()]
# The before version called this `envKeySeconds`; it is the same phase (build the
# global key) and has to line up in the table.
for r in rows:
    if "envKeySeconds" in r:
        r.setdefault("keySeconds", r["envKeySeconds"])
keys = ["readLedgerSeconds", "keySeconds", "hashSeconds", "compareSeconds", "totalSeconds"]
lines = ["# stage5d — C6: the cost of splitting L1 in two", "",
         "date              " + datetime.datetime.now(datetime.timezone.utc)
         .strftime("%Y-%m-%dT%H:%M:%SZ"),
         "host              " + subprocess.run(["uname", "-srm"], capture_output=True,
                                               text=True).stdout.strip(),
         "lean-toolchain    " + open(target + "/lean-toolchain").read().strip(),
         f"rounds            {rounds} per variant, interleaved (round 1 of each dropped)",
         "before            the single envKey, at the split commit's parent",
         "after             extractKey + renderKey", ""]
lines.append("| key | before | after | delta |")
lines.append("|---|---:|---:|---:|")
med = {}
for k in keys:
    cell = {}
    for v in ("before", "after"):
        xs = [r[k] for r in rows if r["variant"] == v and r["run"] > 1 and k in r]
        cell[v] = statistics.median(xs) if xs else float("nan")
    med[k] = cell
    lines.append("| %s | %.4f | %.4f | %+.4f |" % (k, cell["before"], cell["after"],
                                                   cell["after"] - cell["before"]))
# The drift bound: how far the *same* variant moves between its own runs.
lines.append("")
for v in ("before", "after"):
    xs = sorted(r["totalSeconds"] for r in rows if r["variant"] == v and r["run"] > 1)
    lines.append("%-7s totalSeconds spread over its own runs: %.4f .. %.4f (%.4f)"
                 % (v, xs[0], xs[-1], xs[-1] - xs[0]))
delta = med["totalSeconds"]["after"] - med["totalSeconds"]["before"]
spread = max(
    max(r["totalSeconds"] for r in rows if r["variant"] == v and r["run"] > 1)
    - min(r["totalSeconds"] for r in rows if r["variant"] == v and r["run"] > 1)
    for v in ("before", "after"))
lines.append("")
lines.append("C6 delta %+.4f s vs within-variant spread %.4f s -> %s"
             % (delta, spread, "within drift" if abs(delta) <= spread else "ABOVE drift"))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY
echo "-> $RESULTS/stage5d-detect-cost.txt"
