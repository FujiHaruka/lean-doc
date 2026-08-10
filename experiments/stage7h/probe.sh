#!/usr/bin/env bash
# Stage 7h, step 1 — the ceiling: splitting `globalSeconds` into what
# incrementalisation can remove and what it cannot.
#
# `globalSeconds` in stage 7g's record is the wall clock around *two* `deno run`
# invocations (`incremental.sh:286-294`). The two scripts' own timers cover only
# what happens after the runtime is up, so the difference between the two is
# process startup, twice — and one process is still needed after any amount of
# incrementalisation, so that half of the gap is not a ceiling, it is a floor.
#
# What this measures, each N times, median reported:
#
#   empty        an empty script. The floor of `deno run`.
#   usage        `global.ts` with no command: startup + parse + exit. The floor
#                for *this* script, i.e. including its own module load.
#   build        `global.ts build`, wall clock vs its internal `totalSeconds`.
#   delta-hit    `global.ts delta` with names that moved -> the full-IR scan runs.
#   delta-miss   `global.ts delta` with identical maps -> the scan is skipped.
#                The difference between the two is the scan, measured rather
#                than inferred.
#   readall      a script that only reads and parses all 433 module files. The
#                unit cost every step below pays.
#
# usage: probe.sh <work-dir> [reps]
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W="${1:?work dir}"
N="${2:-9}"
IR="$W/ir"
P="$W/probe"
mkdir -p "$P"
: > "$P/probe.jsonl"

# Recorded at measurement time, not at report time: the report can be re-run days
# later, and a header that says when *it* ran is a header that lies.
{
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "deno              $(deno --version | head -1)"
  echo "IR                $IR ($(python3 -c "import json;print(json.load(open('$IR/index.json'))['moduleCount'])") modules, $(du -sh "$IR" | awk '{print $1}'))"
  echo "reps              $N, interleaved"
  echo "warm              same session; the IR was written and re-read repeatedly"
  echo "                  before this ran, so it is in page cache. No cold number here."
} > "$P/conditions.txt"

now () { python3 -c 'import time; print(repr(time.time()))'; }

cat > "$P/empty.ts" <<'EOF'
// nothing: the floor of `deno run`.
EOF

cat > "$P/readall.ts" <<'EOF'
// Read + JSON.parse every module file of an IR tree, and nothing else.
const ir = Deno.args[0];
const t0 = performance.now();
const idx = JSON.parse(await Deno.readTextFile(`${ir}/index.json`));
let decls = 0;
for (const e of idx.modules) {
  const m = JSON.parse(await Deno.readTextFile(`${ir}/${e.file}`));
  decls += m.declarations.length;
}
const t1 = performance.now();
await Deno.writeTextFile(
  Deno.args[1],
  JSON.stringify({ command: "readall", modules: idx.modules.length, declarations: decls,
                   readSeconds: (t1 - t0) / 1000, totalSeconds: (t1 - t0) / 1000 }) + "\n",
);
EOF

# The two name maps `delta` is given. `after` is what `build` just wrote; `before`
# is the same map with one module's names attributed to the module they came
# from, which is the shape a move produces (stage 5e/7g's scenario).
deno run --allow-read --allow-write "$S5/global.ts" build --ir "$IR" --out "$P/pages" \
  --timings "$P/build-once.json" > "$P/build-once.log"
cp "$P/pages/declarations/name-map.json" "$P/map-after.json"
cp "$P/pages/declarations/name-map.json" "$P/map-same.json"
python3 - "$P/map-after.json" "$P/map-before.json" <<'PY'
import json, sys, collections
src, dst = sys.argv[1:]
m = json.load(open(src, encoding="utf-8"))
# Pick the module with the most names among the smaller half, and move its names
# to its parent module name: "a declaration moved between two modules".
by = collections.Counter(m.values())
own = [k for k, v in by.items() if k.startswith("InformationTheory")]
tgt = sorted(own, key=lambda k: (-by[k], k))[3]
parent = tgt.rsplit(".", 1)[0] or tgt
n = 0
for k, v in list(m.items()):
    if v == tgt:
        m[k] = parent + ".Elsewhere"
        n += 1
json.dump(m, open(dst, "w", encoding="utf-8"))
print("  before-map: %d name(s) of %s attributed elsewhere" % (n, tgt))
PY

run_one () { # run_one <tag> <timings-json-or-empty> <cmd...>
  local tag="$1" tj="$2"; shift 2
  local a b rc=0
  a=$(now)
  "$@" > "$P/$tag.log" 2>&1 || rc=$?
  b=$(now)
  python3 - "$P/probe.jsonl" "$tag" "$a" "$b" "$tj" "$rc" <<'PY'
import json, os, sys
out, tag, a, b, tj, rc = sys.argv[1:]
rec = {"tag": tag, "wallSeconds": float(b) - float(a), "exit": int(rc)}
if tj and os.path.exists(tj):
    try:
        rec["inner"] = json.load(open(tj, encoding="utf-8"))
    except Exception:
        pass
open(out, "a", encoding="utf-8").write(json.dumps(rec) + "\n")
PY
}

echo "### stage7h probe — $N reps, interleaved (page-cache state moves slowly)"
for i in $(seq 1 "$N"); do
  run_one empty "" deno run --allow-read --allow-write "$P/empty.ts"
  run_one usage "" deno run --allow-read --allow-write "$S5/global.ts"
  run_one readall "$P/readall.json" deno run --allow-read --allow-write "$P/readall.ts" "$IR" "$P/readall.json"
  rm -rf "$P/pages-b"
  run_one build "$P/build.json" deno run --allow-read --allow-write "$S5/global.ts" \
    build --ir "$IR" --out "$P/pages-b" --timings "$P/build.json"
  run_one delta-hit "$P/delta-hit.json" deno run --allow-read --allow-write "$S5/global.ts" \
    delta --before "$P/map-before.json" --after "$P/map-after.json" --ir "$IR" \
    --print-set "$P/delta-hit-set.txt" --json "$P/delta-hit.json"
  run_one delta-miss "$P/delta-miss.json" deno run --allow-read --allow-write "$S5/global.ts" \
    delta --before "$P/map-same.json" --after "$P/map-after.json" --ir "$IR" \
    --print-set "$P/delta-miss-set.txt" --json "$P/delta-miss.json"
  echo "  rep $i done"
done
echo "-> $P/probe.jsonl"
