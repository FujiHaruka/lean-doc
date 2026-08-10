#!/usr/bin/env bash
# Stage 5h — H1..H5: one resident process serving many extractions.
#
# The server is driven through a FIFO so that requests can be sent one at a time
# and each reply read before the next is sent: a pipe into a background process
# would let the shell run ahead of the server and make the per-request timings
# meaningless.
#
# usage: run.sh <work-dir> [requests]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
BIN="$LD/experiments/stage4b/build/extract"
W=${1:?work dir}
REQS=${2:-6}
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
RESULTS="$LD/benchmarks/results"
MODLIST="$RESULTS/it-modules.txt"

# Three modules of different sizes, so a per-request cost is not read off one
# point. Picked from the same census stages 5e/5f use.
M1=InformationTheory.Shannon.Huffman.StrongForm
M2=InformationTheory.Shannon.Huffman.Length
M3=InformationTheory.Asymptotic

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
rm -rf "$W/resident" "$W/oneshot" "$W/req"
mkdir -p "$W/resident" "$W/oneshot" "$W/req"

for m in "$M1" "$M2" "$M3"; do echo "$m" > "$W/req/$m.txt"; done

# ---------------------------------------------------------------- one-shot
echo "### one-shot baselines"
for m in "$M1" "$M2" "$M3"; do
  "$S5/extract-once.sh" --modules "$W/req/$m.txt" --ir-dir "$W/oneshot/$m" \
    --timings "$W/oneshot/$m.json" --events "$W/oneshot/$m.events" > "$W/oneshot/$m.log"
  python3 -c "
import json
d = json.load(open('$W/oneshot/$m.json'))
print('  %-58s import %.3f s  total %.3f s' % ('$m', d.get('importModules', 0), d.get('total', 0)))"
done

# ---------------------------------------------------------------- resident
echo "### resident: import the whole package once, then $REQS requests"
FIFO="$W/req.fifo"
rm -f "$FIFO"; mkfifo "$FIFO"
cd "$TARGET"
"$LAKE" env "$BIN" "$MODLIST" "$W/serve-events.jsonl" \
  --equations --refs --write-ir --tagged-code --serve \
  < "$FIFO" > "$W/serve.out" 2> "$W/serve.err" &
SERVER=$!
exec 9> "$FIFO"

# `run` prints its own human-readable report to stdout, so the protocol lines
# have to be counted by prefix rather than by line number.
wait_for () { # wait_for <pattern> <n>
  local pat="$1" n="$2" i=0
  until [ "$(grep -c "$pat" "$W/serve.out" 2>/dev/null)" -ge "$n" ] 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -gt 1200 ] && { echo "server did not answer" >&2; tail -20 "$W/serve.err" >&2; exit 1; }
    sleep 0.5
  done
}
wait_for '^ready ' 1
READY=$(grep '^ready ' "$W/serve.out" | head -1)
echo "  $READY"

# The request order deliberately repeats and interleaves: H4 is about whether the
# Nth answer equals the first, which a straight sequence of distinct modules
# cannot show.
ORDER=("$M1" "$M2" "$M1" "$M3" "$M1" "$M2")
n=0
for m in "${ORDER[@]:0:$REQS}"; do
  n=$((n + 1))
  echo "$W/req/$m.txt $W/resident/req-$n.jsonl $W/resident/ir-$n" >&9
  wait_for '^ok ' "$n"
  echo "  request $n ($m): $(grep '^ok ' "$W/serve.out" | sed -n "${n}p")"
done
exec 9>&-
wait "$SERVER" 2>/dev/null || true
cd "$LD"

# ---------------------------------------------------------------- H1..H5
python3 - "$W" "$M1" "$M2" "$M3" "$REQS" "$OUT/compare.txt" <<'PY'
import json, os, sys
W, M1, M2, M3, reqs, out = sys.argv[1:]
reqs = int(reqs)
order = [M1, M2, M1, M3, M1, M2][:reqs]

def events(path):
    rec = {}
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        e = json.loads(line)
        p = e["phase"].replace("stage4b.", "")
        rec.setdefault(p, []).append(e)
    return rec

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

lines = []
ready = [l for l in open(os.path.join(W, "serve.out"), encoding="utf-8")
         if l.startswith("ready ")][0].split()
lines.append("ready: env load %.3f s, %s modules loaded from a %s-module import list"
             % (int(ready[1]) / 1e9, ready[2], ready[3]))

# The server has no sink of its own: `run` creates one per request, so the
# events for request N are in that request's own file.
imports, sps = [], []
for i in range(1, reqs + 1):
    ev = events(os.path.join(W, "resident", "req-%d.jsonl" % i))
    imports += ev.get("importModules", [])
    sps += ev.get("initSearchPath", [])
lines.append("")
lines.append("H1 per-request importModules (s): "
             + ", ".join("%.4f" % (e["us"] / 1e6) for e in imports))
lines.append("H1 resident flag on each: "
             + ", ".join(str(e.get("resident")) for e in imports))
lines.append("H1 per-request initSearchPath (s): "
             + ", ".join("%.4f" % (e["us"] / 1e6) for e in sps))

replies = [l.split() for l in open(os.path.join(W, "serve.out"), encoding="utf-8")
           if l.startswith("ok ")]
per = [int(r[2]) / 1e9 for r in replies]
lines.append("")
lines.append("H3 per-request wall (s): " + ", ".join("%.3f" % x for x in per))
oneshot = {}
for m in (M1, M2, M3):
    d = json.load(open(os.path.join(W, "oneshot", m + ".json"), encoding="utf-8"))
    oneshot[m] = d
lines.append("H3 one-shot totals (s): "
             + ", ".join("%s %.3f" % (m.split(".")[-1], oneshot[m].get("total", 0))
                         for m in (M1, M2, M3)))
lines.append("H3 one-shot importModules (s): "
             + ", ".join("%s %.3f" % (m.split(".")[-1], oneshot[m].get("importModules", 0))
                         for m in (M1, M2, M3)))

lines.append("")
first_of = {}
for i, m in enumerate(order, start=1):
    got = tree(os.path.join(W, "resident", "ir-%d" % i))
    want = tree(os.path.join(W, "oneshot", m))
    # index.json carries the generator's own view of the target list; the module
    # files are the IR proper and are what a page is rendered from.
    mod_got = {k: v for k, v in got.items() if k.startswith("modules/")}
    mod_want = {k: v for k, v in want.items() if k.startswith("modules/")}
    same = mod_got == mod_want
    lines.append("H2 request %d (%s): module IR equals the one-shot's: %s"
                 % (i, m.split(".")[-1], same))
    if not same:
        for k in sorted(set(mod_got) | set(mod_want)):
            if mod_got.get(k) != mod_want.get(k):
                lines.append("     differs: %s (%s vs %s bytes)"
                             % (k, len(mod_got.get(k, b"")), len(mod_want.get(k, b""))))
    if m in first_of:
        lines.append("H4 request %d (%s): equals request %d's answer: %s"
                     % (i, m.split(".")[-1], first_of[m][0],
                        mod_got == first_of[m][1]))
    else:
        first_of[m] = (i, mod_got)
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage5h — stage 6: one process, many extractions"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$TARGET/lean-toolchain")"
  echo "target            $TARGET (read-only; the extractor never writes into it)"
  echo "requests          ${ORDER[*]:0:$REQS}"
  echo
  cat "$OUT/compare.txt"
} > "$RESULTS/stage5h-resident.txt"
echo "-> $RESULTS/stage5h-resident.txt"
