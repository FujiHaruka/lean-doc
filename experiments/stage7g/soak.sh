#!/usr/bin/env bash
# Stage 7g — Y4: 600 requests against one resident extractor.
#
# `approach.md` §8 says "6 requests say nothing about 600". This closes that, and
# it asks three separate questions, because "it did not crash" is not the
# interesting failure mode:
#
#   (a) does the IR stay right? The same module set is requested at #1, #300 and
#       #600 into three separate directories, compared with /usr/bin/diff -r. A
#       server that slowly forgets something would still answer `ok`.
#   (b) does RSS grow? Sampled every 25 requests from `ps`. The environment is
#       shared and immutable, but each request builds and drops a `Core.State`
#       per declaration, and `--jobs 4` does that on four threads.
#   (c) does the answer get slower? The first 100 and the last 100 requests are
#       compared as two medians, on the server's own nanosecond timer *and* on
#       the driver's wall clock.
#
# WHY THE REQUEST LOOP IS NOT `serve-ctl.sh request`
#   That verb polls the reply file every 0.1 s, which is fine for a pipeline
#   whose requests are seconds long and useless here, where a request is 30-90 ms
#   — the quantisation would be the measurement. The loop below speaks the same
#   protocol with a 5 ms poll. Start and stop still go through `serve-ctl.sh`, so
#   there is one implementation of the part that has traps in it.
#
# The server runs with `--jobs 4` because that is the configuration the wiring
# would ship (stage 7d: the knee is around 4 threads on this machine).
#
# usage: soak.sh <work-dir> [requests] [target-repo]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
RESULTS="$LD/benchmarks/results"
DIFF=/usr/bin/diff

W=${1:?work dir}
N=${2:-600}
TARGET=${3:-/Users/haruka/dev/lean-projects}
JOBS=${JOBS:-4}
SAMPLE=${SAMPLE:-25}
export TARGET_REPO="$TARGET"

mkdir -p "$W"
ALL="$W/modules-all.txt"
(cd "$TARGET" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
  | sed 's/\.lean$//; s#/#.#g' > "$ALL"
# The canary: a fixed set, asked for at the start, the middle and the end.
awk 'NR % 97 == 1' "$ALL" > "$W/canary.txt"
echo "### soak: $N requests, jobs $JOBS, $(grep -c . "$ALL") modules imported, canary $(grep -c . "$W/canary.txt")"

"$HERE/serve-ctl.sh" start "$W/S" "$ALL" soak "$JOBS" | sed 's/^/  /'
PID=$(cat "$W/S/server.pid")
trap '"$HERE/serve-ctl.sh" stop "$W/S" >/dev/null 2>&1 || true' EXIT INT TERM

python3 - "$W" "$N" "$ALL" "$PID" "$SAMPLE" <<'PY'
import json, os, re, subprocess, sys, time

W, n, allp, pid, sample = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], int(sys.argv[5])
S = os.path.join(W, "S")
fifo = os.path.join(S, "req.fifo")
outp = os.path.join(S, "serve.out")
mods = [l.strip() for l in open(allp, encoding="utf-8") if l.strip()]
N = len(mods)
reqdir = os.path.join(W, "req"); os.makedirs(reqdir, exist_ok=True)
scratch = os.path.join(W, "scratch-ir"); os.makedirs(scratch, exist_ok=True)
canary = os.path.join(W, "canary.txt")
canary_at = {1: "ir-1", 300: "ir-300", n: "ir-%d" % n}

def ok_lines():
    try:
        with open(outp, encoding="utf-8", errors="replace") as f:
            return [l for l in f if l.startswith("ok ")]
    except FileNotFoundError:
        return []

def rss_kb():
    try:
        return int(subprocess.run(["ps", "-o", "rss=", "-p", pid],
                                  capture_output=True, text=True).stdout.strip())
    except Exception:
        return -1

log = open(os.path.join(W, "soak.jsonl"), "w", encoding="utf-8")
fifo_fd = open(fifo, "w")
errors = []
t_all = time.time()
for i in range(1, n + 1):
    if i in canary_at:
        mfile = canary
        irdir = os.path.join(W, canary_at[i])
        if os.path.isdir(irdir):
            import shutil; shutil.rmtree(irdir)
        os.makedirs(irdir)
        kind = "canary"
    else:
        # 1..3 modules, walked around the package so that the working set is not
        # one corner of it.
        k = 1 + (i % 3)
        start = (i * 7) % N
        chosen = [mods[(start + j) % N] for j in range(k)]
        mfile = os.path.join(reqdir, "req.txt")
        open(mfile, "w", encoding="utf-8").write("\n".join(chosen) + "\n")
        irdir = scratch
        kind = "cycle"
    before = len(ok_lines())
    evt = os.path.join(reqdir, "events.jsonl")
    a = time.time()
    fifo_fd.write("%s %s %s\n" % (mfile, evt, irdir))
    fifo_fd.flush()
    deadline = a + 300
    reply = None
    while time.time() < deadline:
        ls = ok_lines()
        if len(ls) > before:
            reply = ls[before].strip()
            break
        time.sleep(0.005)
    b = time.time()
    if reply is None:
        errors.append((i, "no reply within 300 s"))
        break
    m = re.match(r"ok (\d+) (\d+)", reply)
    code, ns = (int(m.group(1)), int(m.group(2))) if m else (-1, -1)
    if code != 0:
        errors.append((i, reply))
    rec = {"i": i, "kind": kind, "code": code, "serverSeconds": ns / 1e9,
           "wallSeconds": b - a,
           "modules": sum(1 for l in open(mfile, encoding="utf-8") if l.strip())}
    if i == 1 or i % sample == 0 or i == n:
        rec["rssBytes"] = rss_kb() * 1024
    log.write(json.dumps(rec) + "\n")
    log.flush()
    if i % 100 == 0:
        print("  %d/%d requests, %.1f s elapsed, rss %s"
              % (i, n, time.time() - t_all, rec.get("rssBytes", "-")), flush=True)
fifo_fd.close()
log.close()
if errors:
    print("  ERRORS: " + json.dumps(errors[:10]))
    open(os.path.join(W, "errors.txt"), "w", encoding="utf-8").write(
        "\n".join("%d %s" % e for e in errors) + "\n")
PY

"$HERE/serve-ctl.sh" stop "$W/S" | sed 's/^/  /'
trap - EXIT INT TERM

{
  echo "# stage7g Y4 — a soak test: $N requests against one resident extractor"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "extractor         experiments/stage7d/build/extract, --jobs $JOBS --serve"
  echo "target            $TARGET (read-only; the IR is written outside it)"
  echo "environment       $(grep -c . "$ALL") modules imported once"
  echo "requests          $N; 1-3 modules each, walked around the package;"
  echo "                  #1, #300 and #$N ask for the same $(grep -c . "$W/canary.txt")-module canary"
  echo "warm              same session, oleans in page cache"
  echo "rss               ps -o rss=, every $SAMPLE requests"
  echo
  echo "## (a) is the IR still right at the end?"
  for pair in "ir-1 ir-300" "ir-1 ir-$N"; do
    set -- $pair
    if [ -d "$W/$1" ] && [ -d "$W/$2" ]; then
      if "$DIFF" -r "$W/$1" "$W/$2" > "$W/diff-$1-$2.txt" 2>&1; then
        echo "  request $1 == $2   byte-identical"
      else
        echo "  request $1 != $2   DIFFERS:"
        head -20 "$W/diff-$1-$2.txt" | sed 's/^/      /'
      fi
    else
      echo "  $1 / $2 missing — the soak did not reach them"
    fi
  done
  echo
  python3 - "$W" "$N" <<'PY'
import json, os, statistics, sys
W, n = sys.argv[1], int(sys.argv[2])
rows = [json.loads(l) for l in open(os.path.join(W, "soak.jsonl"), encoding="utf-8")]
print("## (b) RSS over the soak")
rss = [(r["i"], r["rssBytes"]) for r in rows if "rssBytes" in r]
if rss:
    print("  samples %d, first %.2f GB (#%d), last %.2f GB (#%d), max %.2f GB, min %.2f GB"
          % (len(rss), rss[0][1] / 1024 ** 3, rss[0][0], rss[-1][1] / 1024 ** 3, rss[-1][0],
             max(v for _, v in rss) / 1024 ** 3, min(v for _, v in rss) / 1024 ** 3))
    print("  growth first -> last: %+.1f MB (%+.2f%%)"
          % ((rss[-1][1] - rss[0][1]) / 1024 ** 2,
             100.0 * (rss[-1][1] - rss[0][1]) / rss[0][1]))
    print("  trace (every sample): " + " ".join("%.2f" % (v / 1024 ** 3) for _, v in rss))
print()
print("## (c) does it get slower?")
cyc = [r for r in rows if r["kind"] == "cycle"]
def block(rs, label):
    if not rs:
        return
    for key in ("serverSeconds", "wallSeconds"):
        xs = [r[key] for r in rs]
        print("  %-18s %-14s n=%3d median %.4f s  [%.4f-%.4f]"
              % (label, key, len(xs), statistics.median(xs), min(xs), max(xs)))
first = [r for r in cyc if r["i"] <= 100]
last = [r for r in cyc if r["i"] > n - 100]
block(first, "first 100")
block(last, "last 100")
for key in ("serverSeconds", "wallSeconds"):
    if first and last:
        a = statistics.median([r[key] for r in first])
        b = statistics.median([r[key] for r in last])
        print("  %-14s first100 %.4f -> last100 %.4f = %+.4f s (%+.1f%%), denominator = first 100"
              % (key, a, b, b - a, 100.0 * (b - a) / a))
print()
print("  per-module normalised (server timer): "
      + ", ".join("%d module(s) median %.4f s (n=%d)"
                  % (k, statistics.median([r["serverSeconds"] for r in cyc if r["modules"] == k]),
                     sum(1 for r in cyc if r["modules"] == k))
                  for k in sorted({r["modules"] for r in cyc})))
print()
print("  CAVEAT: first-100 vs last-100 is confounded. The request walk is")
print("  start=(i*7) mod M, k=1+(i mod 3), so the two blocks ask about *different*")
print("  modules and a module's declaration count is what sets its time. The two")
print("  clean controls are below.")
print()
# Control 1: the walk repeats with period M (gcd(7, M) = 1) x 3 = M when 3 | M,
# so request i and request i+M ask for exactly the same modules.
import collections
bykey = collections.defaultdict(list)
for r in cyc:
    bykey[(r["i"] % 432, r["modules"])].append(r)
pairs = [(v[0], v[-1]) for v in bykey.values() if len(v) >= 2 and v[-1]["i"] - v[0]["i"] >= 400]
print("  control 1 — the same request, asked early and again %d+ requests later" % 400)
if pairs:
    for key in ("serverSeconds", "wallSeconds"):
        d = [b[key] - a[key] for a, b in pairs]
        rel = [100.0 * (b[key] - a[key]) / a[key] for a, b in pairs]
        print("    %-14s n=%3d  median delta %+.4f s (%+.1f%%)  [%+.4f - %+.4f]"
              % (key, len(d), statistics.median(d), statistics.median(rel), min(d), max(d)))
    print("    later slower in %d of %d pairs (server timer)"
          % (sum(1 for a, b in pairs if b["serverSeconds"] > a["serverSeconds"]), len(pairs)))
else:
    print("    not enough requests to pair")
print()
can = [r for r in rows if r["kind"] == "canary"]
print("  control 2 — the canary, an identical %d-module request at #1 / #300 / #%d"
      % (can[0]["modules"] if can else 0, n))
print("    " + ", ".join("#%d %.4f s" % (r["i"], r["serverSeconds"]) for r in can))
bad = [r for r in rows if r["code"] != 0]
print()
print("## errors")
print("  non-zero replies: %d" % len(bad))
print("  requests completed: %d of %d" % (len(rows), n))
PY
  if [ -s "$W/errors.txt" ]; then
    echo
    echo "  errors.txt:"; sed 's/^/    /' "$W/errors.txt" | head -20
    echo "  serve.err tail:"; tail -20 "$W/S/serve.err" | sed 's/^/    /'
  fi
} | tee "$RESULTS/stage7g-soak.txt"
cp "$W/soak.jsonl" "$RESULTS/stage7g-soak.jsonl"
echo "-> $RESULTS/stage7g-soak.txt"
