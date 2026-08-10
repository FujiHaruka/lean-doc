#!/usr/bin/env bash
# Stage 5g — G1..G4: the whole-package artifacts.
#
# G5 (the global-map delta closing stage 5f's F5) is measured by stage 5f's own
# run, because the only honest test of it is the end-to-end byte comparison
# there. This script measures the artifact generation itself.
#
# usage: run.sh <work-dir> <ir-dir>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
IR=${2:?ir dir}
RUNS=${3:-8}
RESULTS="$LD/benchmarks/results"
mkdir -p "$W"

JSONL="$RESULTS/stage5g-global.jsonl"
: > "$JSONL"
# Two output trees, alternated, so the determinism check is over runs that did
# not overwrite each other.
for i in $(seq 1 "$RUNS"); do
  d="$W/out-$((i % 2))"
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$IR" --out "$d" \
    --timings "$W/t.json" > "$W/build-$i.log"
  python3 -c "
import json
d = json.load(open('$W/t.json')); d['run'] = $i
print(json.dumps(d))" >> "$JSONL"
done

echo "### determinism: two independent generations over the same IR"
DIFF=$(diff -r "$W/out-0" "$W/out-1" && echo same || echo differ)

python3 - "$JSONL" "$W" "$DIFF" "$RESULTS/stage5g-global.txt" "$IR" <<'PY'
import datetime, json, os, statistics, subprocess, sys
src, W, same, out, ir = sys.argv[1:]
rows = [json.loads(l) for l in open(src, encoding="utf-8") if l.strip()]
warm = [r for r in rows if r["run"] > 1]


def med(k):
    return statistics.median(r[k] for r in warm)


r0 = rows[0]
sizes = {}
for root, _d, fs in os.walk(os.path.join(W, "out-0")):
    for f in fs:
        p = os.path.join(root, f)
        sizes[os.path.relpath(p, os.path.join(W, "out-0"))] = os.path.getsize(p)
total = sum(sizes.values())

lines = [
    "# stage5g — the whole-package artifacts",
    "",
    "date              " + datetime.datetime.now(datetime.timezone.utc)
    .strftime("%Y-%m-%dT%H:%M:%SZ"),
    "host              " + subprocess.run(["uname", "-srm"], capture_output=True,
                                          text=True).stdout.strip(),
    "ir                %s (%d modules, %d own declarations, %d dependency names)"
    % (ir, r0["modules"], r0["declarations"], r0["dependencyNames"]),
    "runs              %d (run 1 dropped)" % len(rows),
    "",
    "## G1 — is `tactics.html` a function of this package?",
    "tactic docstrings across all %d modules: **%d**" % (r0["modules"], r0["tacticDocs"]),
    "",
    "## G2 — cost of generating everything from the full IR",
    "| phase | seconds (median of warm runs) |",
    "|---|---:|",
    "| read the whole IR | %.4f |" % med("readSeconds"),
    "| build + write all artifacts | %.4f |" % med("writeSeconds"),
    "| **total** | **%.4f** |" % med("totalSeconds"),
    "",
    "spread over warm runs: %.4f .. %.4f"
    % (min(r["totalSeconds"] for r in warm), max(r["totalSeconds"] for r in warm)),
    "",
    "## sizes",
    "| file | bytes |",
    "|---|---:|",
]
for k in sorted(sizes):
    lines.append("| `%s` | %s |" % (k, format(sizes[k], ",")))
lines.append("| **total** | **%s** |" % format(total, ","))
lines += [
    "",
    "## G3 — determinism",
    "two independent generations over the same IR: **%s**" % same,
    "",
    "## G4 — would an incremental update be worth writing?",
    "The most an incremental path could remove is the full-IR read, %.4f s: it "
    "would still have to read the changed modules, rebuild the derived maps and "
    "write every artifact (%.4f s). Against an incremental run of 4.5-8.0 s "
    "(stages 5e/5f) the whole step is %.1f%%-%.1f%%, and the part that could be "
    "saved is %.1f%%-%.1f%%."
    % (med("readSeconds"), med("writeSeconds"),
       100 * med("totalSeconds") / 8.0, 100 * med("totalSeconds") / 4.5,
       100 * med("readSeconds") / 8.0, 100 * med("readSeconds") / 4.5),
]
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY
echo "-> $RESULTS/stage5g-global.txt"
