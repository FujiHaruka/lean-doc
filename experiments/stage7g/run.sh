#!/usr/bin/env bash
# Stage 7g — Y0..Y3: is the resident server's share the same with `--jobs 4`,
# and does the wiring still produce byte-identical bytes?
#
# Three phases, run separately because their preconditions differ:
#
#   y0    the gate. Four ways of extracting the same small module set —
#         serve/j1, serve/j4, one-shot/j1, one-shot/j4 — plus two independent
#         requests to the same server. If their IR is not byte-identical, the
#         combination of residency and parallelism is unusable and every later
#         number is meaningless. Runs against the real target (read-only), which
#         is cheap and needs no clone.
#
#   main  the two-round incremental case (Y1/Y2) and the full re-extraction case
#         (Y3), both against an APFS clone with a real move and a real
#         `lake build`. REFERENCE — extract everything from scratch and render
#         everything — is the oracle; "no stale pages reported" comes from the
#         code under test and proves nothing (stages 5e, 5f and 6a were each
#         saved by this).
#
# Variants are interleaved *within a round* and round 1 is discarded, because the
# page cache state is what moves these numbers and it moves slowly. A number from
# a variant run alone is not comparable with one from a variant run in a batch.
#
# ONE SERVER AT A TIME. The extractor's peak RSS is 3.3 GB on a 16 GB machine and
# this workload is memory-bound; two residents plus a one-shot would page, and the
# paging would land in the timings.
#
# usage:
#   run.sh y0   <work-dir>
#   run.sh main <work-dir> <clone-dir> [module-to-move] [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
RESULTS="$LD/benchmarks/results"
RENDER_TS="$LD/experiments/stage7d/render.ts"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"
DIFF=/usr/bin/diff   # `diff` is aliased to a colordiff that is not installed here;
                     # its exit 127 reads as "differences found" and has already
                     # cost this project one wrong conclusion.

PHASE=${1:?y0|main}
W=${2:?work dir}
mkdir -p "$W"

conditions () { # conditions <target-description>
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "extractor         $LD/experiments/stage7d/build/extract (stage 7d, IR schema 4, --jobs)"
  echo "renderer          experiments/stage7d/render.ts"
  echo "target            $1"
}

# ---------------------------------------------------------------------- y0 ---
if [ "$PHASE" = y0 ]; then
  TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
  export TARGET_REPO="$TARGET"
  ALL="$W/modules-all.txt"
  SMALL="$W/modules-small.txt"
  (cd "$TARGET" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$ALL"
  # A spread rather than the first 25: the candidate list is in module order, so
  # a prefix would sample one corner of the package.
  awk 'NR % 17 == 1' "$ALL" > "$SMALL"
  echo "### y0 — $(grep -c . "$SMALL") of $(grep -c . "$ALL") modules"

  one () { # one <tag> <jobs>
    rm -rf "$W/ir-$1"
    "$HERE/extract-once.sh" --modules "$SMALL" --ir-dir "$W/ir-$1" --jobs "$2" \
      --timings "$W/t-$1.json" --events "$W/e-$1.jsonl" > "$W/log-$1.txt" 2>&1
    echo "  one-shot  $1 (jobs $2) done"
  }
  srv () { # srv <tag> <jobs> <n-requests>
    "$HERE/serve-ctl.sh" start "$W/S-$1" "$ALL" "y0-$1" "$2" > "$W/start-$1.txt"
    grep -E '^(jobs|startSeconds|ready)' "$W/start-$1.txt" | sed 's/^/  /'
    for i in $(seq 1 "$3"); do
      rm -rf "$W/ir-$1-r$i"
      "$HERE/extract-once.sh" --modules "$SMALL" --ir-dir "$W/ir-$1-r$i" --jobs "$2" \
        --serve-dir "$W/S-$1" --timings "$W/t-$1-r$i.json" \
        --events "$W/e-$1-r$i.jsonl" > "$W/log-$1-r$i.txt" 2>&1
      echo "  resident  $1 request $i: $(cat "$W/log-$1-r$i.txt")"
    done
    "$HERE/serve-ctl.sh" stop "$W/S-$1" | sed 's/^/  /'
  }

  one one-j1 1
  one one-j4 4
  srv srv-j1 1 1
  srv srv-j4 4 2

  {
    echo "# stage7g y0 — the gate: does residency x parallelism change a byte?"
    echo
    conditions "$TARGET (read-only; the IR is written outside it)"
    echo "modules           $(grep -c . "$SMALL") of $(grep -c . "$ALL") (every 17th, so the sample is spread)"
    echo "server env        all $(grep -c . "$ALL") modules (the superset a real server imports)"
    echo "warm              same session, oleans in page cache"
    echo
    echo "## IR trees, pairwise (/usr/bin/diff -r)"
    for pair in "ir-one-j1 ir-one-j4" "ir-one-j1 ir-srv-j1-r1" "ir-one-j1 ir-srv-j4-r1" \
                "ir-srv-j1-r1 ir-srv-j4-r1" "ir-srv-j4-r1 ir-srv-j4-r2"; do
      set -- $pair
      if "$DIFF" -r "$W/$1" "$W/$2" > "$W/diff-$1-$2.txt" 2>&1; then
        echo "  $1 == $2   byte-identical"
      else
        echo "  $1 != $2   DIFFERS:"
        head -20 "$W/diff-$1-$2.txt" | sed 's/^/      /'
      fi
    done
    echo
    echo "## counts the extractor reports (they must agree too)"
    python3 - "$W" <<'PY'
import json, os, sys
W = sys.argv[1]
keys = ["analyze:considered", "analyze:produced", "analyze:blacklisted",
        "analyze:equations", "analyze:refOccurrences", "writeIR:spans"]
tags = ["one-j1", "one-j4", "srv-j1-r1", "srv-j4-r1", "srv-j4-r2"]
rows = {}
for t in tags:
    p = os.path.join(W, "t-%s.json" % t)
    if not os.path.exists(p):
        p = os.path.join(W, "t-%s.json" % t.replace("-r", "-r"))
    if not os.path.exists(p):
        continue
    rec = json.load(open(p, encoding="utf-8"))
    rows[t] = rec
allk = sorted({k for r in rows.values() for k in r if ":" in k and not k.endswith("Us")})
show = [k for k in keys if any(k in r for r in rows.values())] or allk
hdr = "  %-24s" % "counter" + "".join("%16s" % t for t in rows)
print(hdr)
bad = 0
for k in show:
    vals = [str(rows[t].get(k, "-")) for t in rows]
    same = len(set(vals)) == 1
    bad += 0 if same else 1
    print("  %-24s" % k + "".join("%16s" % v for v in vals) + ("" if same else "   <-- DIFFERS"))
print()
print("  counters that disagree: %d" % bad)
PY
    echo
    echo "## server startup, by job count (a check that --jobs does not touch importModules)"
    for t in srv-j1 srv-j4; do
      printf '  %-8s %s\n' "$t" "$(grep '^startSeconds' "$W/start-$t.txt" | awk '{printf "%.3f s", $2}')"
    done
  } | tee "$RESULTS/stage7g-y0-gate.txt"
  echo "-> $RESULTS/stage7g-y0-gate.txt"
  exit 0
fi

# -------------------------------------------------------------------- main ---
# `report` re-runs only the aggregation over an existing work directory. The
# measurement is 25 minutes and the aggregation is a second; separating them
# means a fix to the summary never costs a re-measurement, and never tempts
# anyone into hand-editing a generated file.
case "$PHASE" in
  main|report) ;;
  *) echo "usage: run.sh y0|main|report <work-dir> ..." >&2; exit 2 ;;
esac
CLONE=${3:?clone dir}
A=${4:-InformationTheory.Shannon.Huffman.Length}
RUNS=${5:-7}
X="${A}Core"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
# Rendering a tree means the module pages *and* the whole-package artifacts.
render () { deno run --allow-read --allow-write "$RENDER_TS" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }

# A glob over the sources, never a scan of `.lake/build`: Lake does not delete
# orphaned oleans, so the build tree is not a list of what exists.
modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

FULLRUNS=${FULLRUNS:-5}
if [ "$PHASE" = main ]; then
probe="$CLONE/.lake/build/lib/lean/${A//.//}.olean"
if ! strings "$probe" 2>/dev/null | grep -q "$CLONE"; then
  echo "the clone's oleans were not built at the clone's path — run stage5e/rebuild-own.sh first" >&2
  exit 2
fi
if [ -f "$CLONE/${X//.//}.lean" ]; then
  echo "the clone already has the move applied — run stage5e/setup-clone.sh reset first" >&2
  exit 2
fi

# ------------------------------------------------------------ BASE (pre-move)
echo "### BASE — module list, extract, render, ledger (pre-move)"
modlist "$W/modules-before.txt"
if [ ! -f "$W/base-ir/index.json" ]; then
  "$HERE/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
    --jobs 4 --timings "$W/base-extract.json" > "$W/base-extract.log"
  render "$W/base-ir" "$W/base-pages"
  deno_ "$S5/ledger.ts" build --modules "$W/modules-before.txt" --target "$CLONE" \
    --ir "$W/base-ir" --source-url "$URL" --algorithm lake --out "$W/base-ledger.json" > /dev/null
fi

echo "### the referrers of $A, from the base IR"
python3 - "$W/base-ir" "$A" "$W/referrers.txt" <<'PY'
import json, os, sys, collections
ir, a, out = sys.argv[1:]
idx = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
hits, names = collections.Counter(), collections.defaultdict(set)
for e in idx["modules"]:
    m = json.load(open(os.path.join(ir, e["file"]), encoding="utf-8"))
    if m["module"] == a:
        continue
    for d in m["declarations"]:
        for owner, name in d["refs"]:
            if owner == a:
                hits[m["module"]] += 1
                names[m["module"]].add(name)
open(out, "w", encoding="utf-8").write("\n".join(sorted(hits)) + ("\n" if hits else ""))
print("  %d referring module(s), %d reference(s)" % (len(hits), sum(hits.values())))
if not hits:
    sys.exit("no module names anything of %s in a printed signature; pick another" % a)
PY

echo "### baseline fixed-point check"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-before.txt" --changed-out /dev/null \
  --render-all-out /dev/null | tee "$OUT/base.txt"
grep -q ": 0 changed, 0 added, 0 removed" "$OUT/base.txt" || {
  echo "baseline is not a fixed point; the clone drifted" >&2; exit 3; }

# ------------------------------------------------------------------- the move
echo "### applying the move and rebuilding"
"$LD/experiments/stage5e/setup-clone.sh" move "$CLONE" "$A" minimal > "$W/move.log" 2>&1 || {
  echo "move failed; see $W/move.log" >&2; tail -20 "$W/move.log" >&2; exit 1; }
grep -E "^A is now" "$W/move.log" || true
modlist "$W/modules-after.txt"

# ------------------------------------------------------------------- REFERENCE
# Built twice, at both job counts, because "the parallel extractor writes the
# same bytes" is this stage's first premise and the oracle is the place it has to
# hold. `ref-pages` (from the j4 IR) is the page oracle.
echo "### REFERENCE — extract everything from the moved state, at --jobs 1 and 4"
for j in 1 4; do
  if [ ! -f "$W/ref-ir-j$j/index.json" ]; then
    rm -rf "$W/ref-ir-j$j"
    "$HERE/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir-j$j" \
      --jobs "$j" --timings "$W/ref-extract-j$j.json" > "$W/ref-extract-j$j.log"
  fi
done
if "$DIFF" -r "$W/ref-ir-j1" "$W/ref-ir-j4" > "$OUT/ref-ir-j1-vs-j4.txt" 2>&1; then
  echo "  REFERENCE IR: --jobs 1 == --jobs 4, byte-identical"
else
  echo "  REFERENCE IR: --jobs 1 != --jobs 4 — STOP" >&2
  head -20 "$OUT/ref-ir-j1-vs-j4.txt" >&2; exit 4
fi
rm -rf "$W/ref-pages"; render "$W/ref-ir-j4" "$W/ref-pages"

# --------------------------------------------------- Y1/Y2: the two-round case
: > "$W/runs.jsonl"
variant () { # variant <name> <run> <jobs> [extra incremental.sh args...]
  local v="$1" run="$2" jobs="$3"; shift 3
  local d="$W/$v"
  rm -rf "$d"; mkdir -p "$d"
  cp -R "$W/base-ir" "$d/ir"
  cp -R "$W/base-pages" "$d/pages"
  cp "$W/base-ledger.json" "$d/ledger.json"
  if /usr/bin/time -l "$HERE/incremental.sh" --module "$A" --ir "$d/ir" --pages "$d/pages" \
       --ledger "$d/ledger.json" --modules "$W/modules-after.txt" \
       --source-url "$URL" --work "$d/work" --mode self --l3-1 on --jobs "$jobs" \
       --timings "$d/timings.json" "$@" > "$OUT/$v-run$run.txt" 2> "$OUT/$v-run$run.time"; then
    python3 - "$d/timings.json" "$v" "$run" "$jobs" "$OUT/$v-run$run.time" >> "$W/runs.jsonl" <<'PY'
import json, re, sys
p, v, run, jobs, timefile = sys.argv[1:]
r = json.load(open(p, encoding="utf-8"))
r["variant"], r["run"], r["jobsArg"] = v, int(run), int(jobs)
txt = open(timefile, encoding="utf-8", errors="replace").read()
m = re.search(r"^\s*([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys", txt, re.M)
if m:
    r["timeReal"], r["timeUser"], r["timeSys"] = (float(m.group(i)) for i in (1, 2, 3))
for key, label in (("peakRSS", "maximum resident set size"),
                   ("pageReclaims", "page reclaims"),
                   ("pageFaults", "page faults")):
    mm = re.search(r"^\s*(\d+)\s+%s" % re.escape(label), txt, re.M)
    if mm:
        r[key] = int(mm.group(1))
print(json.dumps(r))
PY
    python3 -c "
import json
r = json.load(open('$d/timings.json'))
print('  %-9s run %s: rounds %d, changed %d, stale %d, irChanged %d, pages %d, total %.3f s (serve start %.3f)'
      % ('$v', '$run', r['rounds'], r['changed'], r['staleFound'], r['irChanged'],
         r['pagesRendered'], r['totalSeconds'], r['serveStartSeconds']))"
  else
    echo "  $v run $run FAILED:"; tail -12 "$OUT/$v-run$run.txt"
  fi
  # The last run's tree is the one compared against REFERENCE. Copied only on
  # that run: every run overwrites the same variant directory, so copying each
  # time would move ~40 MB per page tree for nothing.
  if [ "$run" = "$RUNS" ]; then
    rm -rf "$W/keep-$v"; cp -R "$d/pages" "$W/keep-$v"
  fi
}

echo "### Y1/Y2 — the two-round case, 4 variants interleaved, $RUNS runs (run 1 discarded)"
for r in $(seq 1 "$RUNS"); do
  variant fresh-j1 "$r" 1
  variant fresh-j4 "$r" 4
  variant res-j1   "$r" 1 --serve auto
  variant res-j4   "$r" 4 --serve auto
done

# ------------------------------------------- Y3: re-extracting all 432 modules
# The other end of the same question: with `--jobs 4` the resident server removes
# `importModules`, which is *not* parallelised, so its relative share should go
# up rather than down. Measured on the extraction alone (no ledger, no render):
# that is the quantity the server changes.
echo "### Y3 — full re-extraction, fresh vs resident, at both job counts"
: > "$W/full.jsonl"
full_fresh () { # full_fresh <jobs> <round>
  local j="$1" r="$2"
  rm -rf "$W/full-ir"
  local a b
  a=$(python3 -c 'import time; print(repr(time.time()))')
  "$HERE/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/full-ir" \
    --jobs "$j" --timings "$W/full-t.json" > "$W/full.log" 2>&1
  b=$(python3 -c 'import time; print(repr(time.time()))')
  python3 - "$W/full-t.json" "$W/full.jsonl" "fresh-j$j" "$r" "$a" "$b" <<'PY'
import json, sys
t, out, v, r, a, b = sys.argv[1:]
rec = json.load(open(t, encoding="utf-8"))
rec = {k: v2 for k, v2 in rec.items() if not k.endswith("Us")}
rec["variant"], rec["round"], rec["wallSeconds"] = v, int(r), float(b) - float(a)
open(out, "a", encoding="utf-8").write(json.dumps(rec) + "\n")
print("  %-12s round %s: %.3f s wall" % (v, r, rec["wallSeconds"]))
PY
  if ! "$DIFF" -r "$W/full-ir" "$W/ref-ir-j$j" > "$W/full-diff-fresh-j$j.txt" 2>&1; then
    echo "  !! fresh-j$j IR differs from REFERENCE" ; head -5 "$W/full-diff-fresh-j$j.txt"
  fi
}
full_res () { # full_res <jobs> <round> <serverdir>
  local j="$1" r="$2" sd="$3"
  rm -rf "$W/full-ir"
  local a b
  a=$(python3 -c 'import time; print(repr(time.time()))')
  "$HERE/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/full-ir" \
    --jobs "$j" --serve-dir "$sd" --timings "$W/full-t.json" > "$W/full.log" 2>&1
  b=$(python3 -c 'import time; print(repr(time.time()))')
  python3 - "$W/full-t.json" "$W/full.jsonl" "res-j$j" "$r" "$a" "$b" <<'PY'
import json, sys
t, out, v, r, a, b = sys.argv[1:]
rec = json.load(open(t, encoding="utf-8"))
rec = {k: v2 for k, v2 in rec.items() if not k.endswith("Us")}
rec["variant"], rec["round"], rec["wallSeconds"] = v, int(r), float(b) - float(a)
open(out, "a", encoding="utf-8").write(json.dumps(rec) + "\n")
print("  %-12s round %s: %.3f s wall (no startup)" % (v, r, rec["wallSeconds"]))
PY
  if ! "$DIFF" -r "$W/full-ir" "$W/ref-ir-j$j" > "$W/full-diff-res-j$j.txt" 2>&1; then
    echo "  !! res-j$j IR differs from REFERENCE" ; head -5 "$W/full-diff-res-j$j.txt"
  fi
}

# Phase A: no server alive. fresh j1 and j4, interleaved.
for r in $(seq 1 "$FULLRUNS"); do
  full_fresh 1 "$r"
  full_fresh 4 "$r"
done
# Phase B: one server at a time, so the memory picture matches phase A's.
for j in 1 4; do
  "$HERE/serve-ctl.sh" start "$W/F-j$j" "$W/modules-after.txt" "full-j$j" "$j" > "$W/F-j$j-start.txt"
  grep -E '^(jobs|startSeconds)' "$W/F-j$j-start.txt" | sed 's/^/  /'
  # The server's own RSS, sampled outside every timed region. `/usr/bin/time -l`
  # cannot see it: the server is detached from the shell that started it, so it
  # is not in anyone's `RUSAGE_CHILDREN`.
  echo "rssAfterStart $(ps -o rss= -p "$(cat "$W/F-j$j/server.pid")" | tr -d ' ')" \
    >> "$W/F-j$j-start.txt"
  for r in $(seq 1 "$FULLRUNS"); do full_res "$j" "$r" "$W/F-j$j"; done
  echo "rssAfterRuns $(ps -o rss= -p "$(cat "$W/F-j$j/server.pid")" | tr -d ' ')" \
    >> "$W/F-j$j-start.txt"
  "$HERE/serve-ctl.sh" stop "$W/F-j$j" | sed 's/^/  /'
done
rm -rf "$W/full-ir"
fi   # PHASE = main

# ------------------------------------------------------------------- verdict
python3 - "$W" "$A" "$X" "$OUT/verdict.txt" <<'PY'
import json, os, statistics, sys, difflib
W, A, X, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

lines = []
ref_pages = tree(os.path.join(W, "ref-pages"))
lines.append("REFERENCE pages: %d" % len(ref_pages))

lines += ["", "## Y1 — each assembled site against REFERENCE (byte equality)"]
for v in ("fresh-j1", "fresh-j4", "res-j1", "res-j4"):
    p = os.path.join(W, "keep-" + v)
    if not os.path.isdir(p):
        lines.append("%-9s no tree kept" % v)
        continue
    got = tree(p)
    missing = sorted(set(ref_pages) - set(got))
    extra = sorted(set(got) - set(ref_pages))
    diff = sorted(k for k in set(ref_pages) & set(got) if ref_pages[k] != got[k])
    lines.append("%-9s %d pages, missing %d, extra %d, differing %d  -> %s"
                 % (v, len(got), len(missing), len(extra), len(diff),
                    "byte-identical to REFERENCE" if not (missing or extra or diff)
                    else "WRONG"))
    for k in diff[:5]:
        lines.append("  %s: %d B vs REFERENCE %d B" % (k, len(got[k]), len(ref_pages[k])))
        a = ref_pages[k].decode("utf-8", "replace").split("\n")
        b = got[k].decode("utf-8", "replace").split("\n")
        d1 = list(difflib.unified_diff(b, a, "this variant", "reference", n=0, lineterm=""))
        lines += ["    " + l[:200] for l in d1[2:8]]
    if missing:
        lines.append("  missing: " + ", ".join(missing[:5]))
    if extra:
        lines.append("  extra:   " + ", ".join(extra[:5]))

runs = [json.loads(l) for l in open(os.path.join(W, "runs.jsonl"), encoding="utf-8")]
def stat(v, key):
    xs = [r[key] for r in runs if r["variant"] == v and r["run"] > 1 and key in r]
    if not xs:
        return None
    return statistics.median(xs), min(xs), max(xs), len(xs)

lines += ["", "## Y2 — the two-round case (median of runs 2..N, [min-max])", "",
          "%-9s %9s %19s %9s %9s %9s %7s %9s" %
          ("variant", "total", "[min-max]", "extract", "srvStart", "render", "rounds", "peakRSS")]
for v in ("fresh-j1", "fresh-j4", "res-j1", "res-j4"):
    s = stat(v, "totalSeconds")
    if not s:
        continue
    ex = stat(v, "extractSeconds"); ss = stat(v, "serveStartSeconds")
    rn = stat(v, "renderSeconds"); ro = stat(v, "rounds"); rss = stat(v, "peakRSS")
    lines.append("%-9s %9.3f %19s %9.3f %9.3f %9.3f %7.1f %9s"
                 % (v, s[0], "[%.3f-%.3f]" % (s[1], s[2]), ex[0], ss[0], rn[0], ro[0],
                    ("%.2f GB" % (rss[0] / 1024 ** 3)) if rss else "-"))
lines.append("")
for a, b in (("fresh-j1", "fresh-j4"), ("fresh-j1", "res-j1"), ("fresh-j4", "res-j4"),
             ("res-j1", "res-j4")):
    sa, sb = stat(a, "totalSeconds"), stat(b, "totalSeconds")
    if sa and sb:
        lines.append("%-9s %.3f -> %-9s %.3f : %+.3f s (%+.1f%%), denominator = %s"
                     % (a, sa[0], b, sb[0], sb[0] - sa[0],
                        100.0 * (sb[0] - sa[0]) / sa[0], a))
lines.append("")
lines.append("server startup inside the run (median): " + ", ".join(
    "%s %.3f s" % (v, stat(v, "serveStartSeconds")[0])
    for v in ("res-j1", "res-j4") if stat(v, "serveStartSeconds")))

# Why the totals move the way they do, at the phase level. Without this the
# table is four numbers with no mechanism, and a mechanism is what makes a
# result transferable to another workload.
lines += ["", "### where the two-round pipeline's time goes (median, s)", "",
          "%-30s%10s%10s%10s%10s" % ("phase", "fresh-j1", "fresh-j4", "res-j1", "res-j4")]
for key in ("serveStartSeconds", "detectSeconds", "extractSeconds", "ownershipSeconds",
            "mergeSeconds", "globalSeconds", "impactSeconds", "renderSeconds",
            "serveStopSeconds", "totalSeconds"):
    row = []
    for v in ("fresh-j1", "fresh-j4", "res-j1", "res-j4"):
        s = stat(v, key)
        row.append("%10.4f" % s[0] if s else "%10s" % "-")
    lines.append("%-30s%s" % (key, "".join(row)))
lines += ["", "### inside `extract`, per round (median, s)", "",
          "%-30s%10s%10s%10s%10s" % ("", "fresh-j1", "fresh-j4", "res-j1", "res-j4")]
def roundstat(v, idx, key):
    xs = []
    for r in runs:
        if r["variant"] != v or r["run"] <= 1:
            continue
        e = r.get("extract")
        if isinstance(e, list) and len(e) > idx and key in e[idx]:
            xs.append(e[idx][key])
    return statistics.median(xs) if xs else None
for idx in (0, 1):
    n = None
    for v in ("fresh-j1",):
        n = roundstat(v, idx, "targetModules")
    for key in ("importModules", "analyze", "writeIR"):
        row = []
        for v in ("fresh-j1", "fresh-j4", "res-j1", "res-j4"):
            x = roundstat(v, idx, key)
            row.append("%10.4f" % x if x is not None else "%10s" % "-")
        lines.append("%-30s%s" % ("round %d %s (%s mod)" % (idx + 1, key, int(n) if n else "?"),
                                  "".join(row)))
lines += ["",
          "`importModules` 0.0000 on the resident side is the phase not running at all:",
          "the environment was imported once, at server start, and that cost is the",
          "`serveStartSeconds` row above — inside the same total."]

full = [json.loads(l) for l in open(os.path.join(W, "full.jsonl"), encoding="utf-8")]
def fstat(v, key):
    xs = [r[key] for r in full if r["variant"] == v and r["round"] > 1 and key in r]
    if not xs:
        return None
    return statistics.median(xs), min(xs), max(xs)
startup, srvrss = {}, {}
for j in (1, 4):
    p = os.path.join(W, "F-j%d" % j, "info.txt")
    if os.path.exists(p):
        for l in open(p, encoding="utf-8"):
            if l.startswith("startSeconds"):
                startup[j] = float(l.split()[1])
    p = os.path.join(W, "F-j%d-start.txt" % j)
    if os.path.exists(p):
        for l in open(p, encoding="utf-8"):
            if l.startswith("rssAfter"):
                srvrss.setdefault(j, {})[l.split()[0]] = int(l.split()[1]) * 1024
lines += ["", "## Y3 — re-extracting all 432 modules (median of rounds 2..N, [min-max])", "",
          "%-9s %9s %19s %10s %10s %10s" %
          ("variant", "wall", "[min-max]", "importMods", "analyze", "writeIR")]
for v in ("fresh-j1", "fresh-j4", "res-j1", "res-j4"):
    s = fstat(v, "wallSeconds")
    if not s:
        continue
    im = fstat(v, "importModules"); an = fstat(v, "analyze"); wr = fstat(v, "writeIR")
    lines.append("%-9s %9.3f %19s %10s %10s %10s"
                 % (v, s[0], "[%.3f-%.3f]" % (s[1], s[2]),
                    "%.3f" % im[0] if im else "-",
                    "%.3f" % an[0] if an else "-",
                    "%.3f" % wr[0] if wr else "-"))
lines.append("")
for j in (1, 4):
    f, r = fstat("fresh-j%d" % j, "wallSeconds"), fstat("res-j%d" % j, "wallSeconds")
    if not (f and r):
        continue
    lines.append("j%d: fresh %.3f -> resident %.3f = %+.3f s (%+.1f%%), denominator = fresh-j%d"
                 % (j, f[0], r[0], r[0] - f[0], 100.0 * (r[0] - f[0]) / f[0], j))
    if j in startup:
        tot = r[0] + startup[j]
        lines.append("    with the server's own startup (%.3f s): %.3f s = %+.1f%% vs fresh-j%d"
                     % (startup[j], tot, 100.0 * (tot - f[0]) / f[0], j))
    if j in srvrss:
        lines.append("    server RSS (ps, outside every timed region): "
                     + ", ".join("%s %.2f GB" % (k, v / 1024 ** 3)
                                 for k, v in sorted(srvrss[j].items())))
lines += ["",
          "NOTE on peakRSS in the Y2 table: it comes from /usr/bin/time -l around",
          "incremental.sh, which sees only the processes it waits for. A resident",
          "server is detached from that accounting, so the res-* peakRSS is the",
          "pipeline's, not the server's. The server's own RSS is the line above."]
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage7g — residency x --jobs 4: the share, and byte equality"
  echo
  conditions "APFS clone of /Users/haruka/dev/lean-projects at $CLONE, own modules rebuilt in place"
  # `date` above is when this text was written, which is not when the numbers
  # were taken if `report` was re-run over an existing work directory.
  echo "measured          $(date -u -r "$W/runs.jsonl" +%Y-%m-%dT%H:%M:%SZ) (mtime of runs.jsonl)"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "move              $A -> $X (minimal shim) — stage 6a's move, so the shapes match"
  echo "runs              $RUNS per variant for Y1/Y2, interleaved, run 1 discarded;"
  echo "                  $FULLRUNS per variant for Y3, round 1 discarded"
  echo "render set        --mode self"
  echo "warm              same session; the clone was rebuilt before the run and its"
  echo "                  oleans are in page cache. No cold number is reported here."
  echo "residency         one server per pipeline run, started after the build, serving"
  echo "                  every round, stopped inside the measured total"
  echo
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage7g-main.txt"
cp "$W/runs.jsonl" "$RESULTS/stage7g-incremental.jsonl"
cp "$W/full.jsonl" "$RESULTS/stage7g-full-extract.jsonl"
echo "-> $RESULTS/stage7g-main.txt"
