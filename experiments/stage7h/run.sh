#!/usr/bin/env bash
# Stage 7h — does incrementalising the whole-package artifacts (L3-3) pay, and
# does the site survive it byte for byte?
#
# The control is `stage5/global.ts` as stage 7g wired it: two `deno` processes,
# each reading all 433 module IRs. The treatment is `stage7h/global.ts`: one
# process, the same derivation, its per-module facts read from a
# `contentHash`-keyed cache instead of from the IR. Everything else in the
# pipeline — extractor, server, ledger, ownership, merge, prune, impact, renderer
# — is the same program on both sides, called in place.
#
# BOTH SIDES IN ONE SESSION. 7g's `globalSeconds` is not the control: the same
# configuration has been measured moving 0.34 s between sessions, so a stored
# number and a fresh one are not comparable. The four variants here are
# interleaved *within a round* and round 1 is discarded, for the same reason 7g
# does it — page-cache state is what moves these numbers and it moves slowly.
#
# THE ORACLE IS NOT "IT SAID IT WAS FINE".
#   * REFERENCE: extract everything from the moved state, render every page, and
#     build the artifacts with **stage 5's** from-scratch `global.ts`. Every
#     variant's assembled site must equal that, file for file, byte for byte.
#   * The six whole-package artifacts are checked again on their own, because a
#     tree comparison that is dominated by 433 module pages can hide the one file
#     this stage is about.
#   * The `--print-set` step 6 hands to step 7 must be identical between the two
#     implementations. It is the input to the render set, so an equal site with an
#     unequal set would mean the site was equal by luck.
#   * `oracle.sh` covers what this scenario cannot contain (a deletion, an
#     addition, a stale cache) at the unit level.
#
# ONE SERVER AT A TIME (7g): the extractor's peak RSS is 3.3 GB on a 16 GB
# machine and this workload is memory-bound.
#
# The phases are separate because their preconditions differ and because
# measurement and aggregation must not be the same command: the A/B is minutes
# and the summary is a second, so a fix to the summary must never cost a
# re-measurement (7g's `report`, same reason).
#
#   setup   base extraction, the move, `lake build`, REFERENCE. Needs the clone
#           in its pre-move state and leaves it moved.
#   ab      the four variants. Re-runnable on an already-set-up work directory.
#   counts  one instrumented run per side. Timings from it are discarded.
#   report  aggregation only.
#
# usage:
#   run.sh main|setup|ab|counts|report <work-dir> <clone-dir> [module-to-move] [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
S7G="$LD/experiments/stage7g"
RESULTS="$LD/benchmarks/results"
RENDER_TS="$LD/experiments/stage7d/render.ts"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"
DIFF=/usr/bin/diff   # `diff` is aliased to a colordiff that is not installed here;
                     # its exit 127 reads as "differences found" and has already
                     # cost this project one wrong conclusion.

PHASE=${1:?main|setup|ab|counts|report}
case "$PHASE" in
  main|setup|ab|counts|report) ;;
  *) echo "usage: run.sh main|setup|ab|counts|report <work-dir> <clone-dir> ..." >&2; exit 2 ;;
esac
W=${2:?work dir}
CLONE=${3:?clone dir}
A=${4:-InformationTheory.Shannon.Huffman.Length}
RUNS=${5:-7}
X="${A}Core"
mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
# Rendering a tree means the module pages *and* the whole-package artifacts. The
# artifacts come from stage 5's `global.ts`: this is the from-scratch side of the
# oracle, so it must be the program the oracle is stated against.
render () { deno run --allow-read --allow-write "$RENDER_TS" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }

modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

conditions () {
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "extractor         $LD/experiments/stage7d/build/extract (stage 7d, IR schema 4, --jobs)"
  echo "renderer          experiments/stage7d/render.ts"
  echo "deno              $(deno --version | head -1)"
  echo "target            $1"
}

# ------------------------------------------------------------------- setup ---
setup () {
  probe="$CLONE/.lake/build/lib/lean/${A//.//}.olean"
  if ! strings "$probe" 2>/dev/null | grep -q "$CLONE"; then
    echo "the clone's oleans were not built at the clone's path — run stage5e/rebuild-own.sh first" >&2
    exit 2
  fi
  if [ -f "$CLONE/${X//.//}.lean" ]; then
    echo "the clone already has the move applied — run stage5e/setup-clone.sh reset first" >&2
    exit 2
  fi

  echo "### BASE — module list, extract, render, ledger, cache (pre-move)"
  modlist "$W/modules-before.txt"
  if [ ! -f "$W/base-ir/index.json" ]; then
    "$S7G/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
      --jobs 4 --timings "$W/base-extract.json" > "$W/base-extract.log"
    render "$W/base-ir" "$W/base-pages"
    deno_ "$S5/ledger.ts" build --modules "$W/modules-before.txt" --target "$CLONE" \
      --ir "$W/base-ir" --source-url "$URL" --algorithm lake --out "$W/base-ledger.json" > /dev/null
  fi
  # The cache the `new` variants start from. In production it is written by the
  # run that produced the site; here it is seeded from the same IR, into a
  # throwaway page tree that is then checked against the real one — which makes
  # the seeding itself an oracle check rather than a setup step nobody looked at.
  rm -rf "$W/base-state" "$W/state-seed-pages"
  deno run --allow-read --allow-write "$HERE/global.ts" build --ir "$W/base-ir" \
    --out "$W/state-seed-pages" --state "$W/base-state" \
    --timings "$W/base-state.json" > "$W/base-state.log"
  for f in declarations/declaration-data.bmp declarations/name-map.json navbar.html \
           tactics.html references.html references.bib; do
    if "$DIFF" "$W/base-pages/$f" "$W/state-seed-pages/$f" > /dev/null 2>&1; then
      echo "  seed == stage5 build: $f"
    else
      echo "  seed != stage5 build: $f — STOP" >&2; exit 4
    fi
  done
  rm -rf "$W/state-seed-pages"

  echo "### baseline fixed-point check"
  deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
    --source-url "$URL" --modules "$W/modules-before.txt" --changed-out /dev/null \
    --render-all-out /dev/null | tee "$OUT/base.txt"
  grep -q ": 0 changed, 0 added, 0 removed" "$OUT/base.txt" || {
    echo "baseline is not a fixed point; the clone drifted" >&2; exit 3; }

  echo "### applying the move and rebuilding"
  "$LD/experiments/stage5e/setup-clone.sh" move "$CLONE" "$A" minimal > "$W/move.log" 2>&1 || {
    echo "move failed; see $W/move.log" >&2; tail -20 "$W/move.log" >&2; exit 1; }
  grep -E "^A is now" "$W/move.log" || true
  modlist "$W/modules-after.txt"

  echo "### REFERENCE — extract everything from the moved state, render everything"
  if [ ! -f "$W/ref-ir/index.json" ]; then
    rm -rf "$W/ref-ir"
    "$S7G/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir" \
      --jobs 4 --timings "$W/ref-extract.json" > "$W/ref-extract.log"
  fi
  rm -rf "$W/ref-pages"; render "$W/ref-ir" "$W/ref-pages"
  echo "  REFERENCE pages: $(find "$W/ref-pages" -type f | wc -l | tr -d ' ')"
}

# --------------------------------------------------------------- one variant --
# `variant <name> <run> <global-impl> [extra incremental.sh args...]`
variant () {
  local v="$1" run="$2" impl="$3"; shift 3
  local d="$W/$v"
  rm -rf "$d"; mkdir -p "$d"
  cp -R "$W/base-ir" "$d/ir"
  cp -R "$W/base-pages" "$d/pages"
  cp "$W/base-ledger.json" "$d/ledger.json"
  local STATE_ARG=()
  if [ "$impl" = new ]; then
    cp -R "$W/base-state" "$d/state"
    STATE_ARG=(--state "$d/state")
  fi
  if /usr/bin/time -l "$HERE/incremental.sh" --module "$A" --ir "$d/ir" --pages "$d/pages" \
       --ledger "$d/ledger.json" --modules "$W/modules-after.txt" \
       --source-url "$URL" --work "$d/work" --mode self --l3-1 on --jobs 4 \
       --global "$impl" ${STATE_ARG[@]+"${STATE_ARG[@]}"} \
       --timings "$d/timings.json" "$@" > "$OUT/$v-run$run.txt" 2> "$OUT/$v-run$run.time"; then
    python3 - "$d/timings.json" "$v" "$run" "$OUT/$v-run$run.time" >> "$W/runs.jsonl" <<'PY'
import json, re, sys
p, v, run, timefile = sys.argv[1:]
r = json.load(open(p, encoding="utf-8"))
r["variant"], r["run"] = v, int(run)
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
print('  %-9s run %s: rounds %d, irChanged %d, globalStale %d, pages %d, global %.4f s, total %.3f s'
      % ('$v', '$run', r['rounds'], r['irChanged'], r['globalStale'],
         r['pagesRendered'], r['globalSeconds'], r['totalSeconds']))"
  else
    echo "  $v run $run FAILED:"; tail -12 "$OUT/$v-run$run.txt"
  fi
  # The last run's tree is the one compared against REFERENCE, and its work
  # directory holds the `--print-set` the two implementations must agree on.
  if [ "$run" = "$RUNS" ]; then
    rm -rf "$W/keep-$v"; cp -R "$d/pages" "$W/keep-$v"
    cp "$d/work/global-set.txt" "$W/keep-$v-global-set.txt"
    [ -f "$d/work/global-delta.json" ] && cp "$d/work/global-delta.json" "$W/keep-$v-delta.json"
  fi
}

# -------------------------------------------------------------------- main ---
if [ "$PHASE" = main ] || [ "$PHASE" = setup ]; then
  setup
fi
if [ "$PHASE" = main ] || [ "$PHASE" = ab ]; then
  : > "$W/runs.jsonl"
  echo "### the A/B — 4 variants interleaved, $RUNS runs each (run 1 discarded)"
  # The order flips every round. A fixed order aliases the treatment with the
  # variant's *position*, and on this workload position is worth more than the
  # treatment: a run that starts right after a 3 GB resident extractor exited
  # pays for the page cache that exited with it. Measured on the first attempt
  # at this A/B — `old-fresh`, always first, took 12.27 s with 29,779 page faults
  # while `new-fresh`, always second, took 8.93 s with 2,013, a 3.3 s gap that
  # step 6 (0.27 s) cannot explain and did not cause. Flipping by round parity
  # gives each side each position equally often.
  for r in $(seq 1 "$RUNS"); do
    if [ $((r % 2)) -eq 1 ]; then
      variant old-fresh "$r" old
      variant new-fresh "$r" new
      variant old-res   "$r" old --serve auto
      variant new-res   "$r" new --serve auto
    else
      variant new-fresh "$r" new
      variant old-fresh "$r" old
      variant new-res   "$r" new --serve auto
      variant old-res   "$r" old --serve auto
    fi
  done
fi

# ------------------------------------------------------------------ counts ---
# How many times does one incremental run read all 433 module IRs? Answered by
# running the real scripts under a wrapper that counts `Deno.readTextFile`,
# never by reading the code. Timings from these runs are discarded — the counting
# is inside the measured region and would corrupt them.
if [ "$PHASE" = counts ]; then
  : > "$W/counts.jsonl"
  for impl in old new; do
    d="$W/count-$impl"
    rm -rf "$d"; mkdir -p "$d"
    cp -R "$W/base-ir" "$d/ir"; cp -R "$W/base-pages" "$d/pages"
    cp "$W/base-ledger.json" "$d/ledger.json"
    STATE_ARG=()
    if [ "$impl" = new ]; then cp -R "$W/base-state" "$d/state"; STATE_ARG=(--state "$d/state"); fi
    "$HERE/incremental.sh" --module "$A" --ir "$d/ir" --pages "$d/pages" \
      --ledger "$d/ledger.json" --modules "$W/modules-after.txt" \
      --source-url "$URL" --work "$d/work" --mode self --l3-1 on --jobs 4 \
      --global "$impl" ${STATE_ARG[@]+"${STATE_ARG[@]}"} \
      --count-reads "$W/counts-$impl.jsonl" \
      --timings "$d/timings.json" > "$OUT/counts-$impl.txt" 2>&1
    python3 - "$W/counts-$impl.jsonl" "$impl" "$W/counts.jsonl" <<'PY'
import json, sys
src, impl, out = sys.argv[1:]
rows = [json.loads(l) for l in open(src, encoding="utf-8")]
for r in rows:
    r["impl"] = impl
    open(out, "a", encoding="utf-8").write(json.dumps(r) + "\n")
tot = sum(r["moduleFileReads"] for r in rows)
print("  %-4s %d module-file reads across %d steps" % (impl, tot, len(rows)))
PY
    # The counted run also has to be right: a wrapper that changed behaviour
    # would make the counts describe a program nobody runs.
    if "$DIFF" -r "$d/pages" "$W/ref-pages" > "$OUT/counts-$impl-vs-ref.txt" 2>&1; then
      echo "  $impl: the counted run's site is byte-identical to REFERENCE"
    else
      echo "  $impl: the counted run's site DIFFERS from REFERENCE"
      head -5 "$OUT/counts-$impl-vs-ref.txt"
    fi
  done
fi

# ------------------------------------------------------------------ report ---
if [ "$PHASE" = setup ]; then
  echo "setup done. next: run.sh ab $W $CLONE $A <runs>"
  exit 0
fi
python3 - "$W" "$A" "$X" "$OUT/verdict.txt" <<'PY'
import json, os, statistics, sys, difflib

W, A, X, out = sys.argv[1:]
GLOBALS = ["declarations/declaration-data.bmp", "declarations/name-map.json",
           "navbar.html", "tactics.html", "references.html", "references.bib"]
VARIANTS = ["old-fresh", "new-fresh", "old-res", "new-res"]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

lines = []
ref = tree(os.path.join(W, "ref-pages"))
lines.append("REFERENCE pages: %d files" % len(ref))

lines += ["", "## O1 — each assembled site against REFERENCE (byte equality)"]
for v in VARIANTS:
    p = os.path.join(W, "keep-" + v)
    if not os.path.isdir(p):
        lines.append("%-9s no tree kept" % v)
        continue
    got = tree(p)
    missing = sorted(set(ref) - set(got))
    extra = sorted(set(got) - set(ref))
    diff = sorted(k for k in set(ref) & set(got) if ref[k] != got[k])
    lines.append("%-9s %d files, missing %d, extra %d, differing %d  -> %s"
                 % (v, len(got), len(missing), len(extra), len(diff),
                    "byte-identical to REFERENCE" if not (missing or extra or diff)
                    else "WRONG"))
    for k in diff[:5]:
        lines.append("  %s: %d B vs REFERENCE %d B" % (k, len(got[k]), len(ref[k])))
        a = ref[k].decode("utf-8", "replace").split("\n")
        b = got[k].decode("utf-8", "replace").split("\n")
        lines += ["    " + l[:200]
                  for l in list(difflib.unified_diff(b, a, "variant", "reference",
                                                     n=0, lineterm=""))[2:8]]
    if missing:
        lines.append("  missing: " + ", ".join(missing[:5]))
    if extra:
        lines.append("  extra:   " + ", ".join(extra[:5]))

lines += ["", "## O2 — the six whole-package artifacts on their own",
          "(a 439-file tree comparison is dominated by module pages; this stage is",
          " about these six, so they are named)", ""]
for v in VARIANTS:
    p = os.path.join(W, "keep-" + v)
    if not os.path.isdir(p):
        continue
    bad = []
    for f in GLOBALS:
        a = os.path.join(p, f)
        b = os.path.join(W, "ref-pages", f)
        if not os.path.exists(a) or open(a, "rb").read() != open(b, "rb").read():
            bad.append(f)
    lines.append("%-9s %s" % (v, "all 6 byte-identical to REFERENCE" if not bad
                              else "DIFFER: " + ", ".join(bad)))

lines += ["", "## O3 — the render set step 6 hands to step 7",
          "(equal sites with unequal sets would mean the site was equal by luck)", ""]
sets = {}
for v in VARIANTS:
    p = os.path.join(W, "keep-%s-global-set.txt" % v)
    if os.path.exists(p):
        sets[v] = open(p, encoding="utf-8").read()
for a, b in (("old-fresh", "new-fresh"), ("old-res", "new-res")):
    if a in sets and b in sets:
        n = len([x for x in sets[a].split("\n") if x])
        lines.append("%-9s vs %-9s : %s (%d module(s) in the set)"
                     % (a, b, "identical" if sets[a] == sets[b] else "DIFFER", n))
for v in VARIANTS:
    p = os.path.join(W, "keep-%s-delta.json" % v)
    if os.path.exists(p):
        d = json.load(open(p, encoding="utf-8"))
        lines.append("%-9s delta: %d name(s) moved, %d page(s) affected"
                     % (v, d["changedNames"], d["affected"]))

runs = []
rp = os.path.join(W, "runs.jsonl")
if os.path.exists(rp):
    runs = [json.loads(l) for l in open(rp, encoding="utf-8")]

def stat(v, key, sub=None):
    xs = []
    for r in runs:
        if r["variant"] != v or r["run"] <= 1:
            continue
        o = r.get(sub) if sub else r
        if isinstance(o, dict) and key in o and isinstance(o[key], (int, float)):
            xs.append(o[key])
    if not xs:
        return None
    return statistics.median(xs), min(xs), max(xs), len(xs)

if runs:
    lines += ["", "## T1 — step 6 (`globalSeconds`), median of runs 2..N", "",
              "%-9s %10s %19s %8s %10s %10s" %
              ("variant", "global", "[min-max]", "n", "total", "share")]
    for v in VARIANTS:
        s = stat(v, "globalSeconds")
        t = stat(v, "totalSeconds")
        if not s:
            continue
        lines.append("%-9s %10.4f %19s %8d %10.3f %9.2f%%"
                     % (v, s[0], "[%.4f-%.4f]" % (s[1], s[2]), s[3], t[0],
                        100.0 * s[0] / t[0]))
    lines.append("")
    for a, b in (("old-fresh", "new-fresh"), ("old-res", "new-res")):
        sa, sb = stat(a, "globalSeconds"), stat(b, "globalSeconds")
        ta, tb = stat(a, "totalSeconds"), stat(b, "totalSeconds")
        if not (sa and sb):
            continue
        lines.append("%-9s -> %-9s  global %.4f -> %.4f = %+.4f s"
                     % (a, b, sa[0], sb[0], sb[0] - sa[0]))
        lines.append("%22s  total  %.3f -> %.3f = %+.3f s (%+.2f%%), denominator = %s"
                     % ("", ta[0], tb[0], tb[0] - ta[0],
                        100.0 * (tb[0] - ta[0]) / ta[0], a))
        lines.append("%22s  the step-6 saving alone is %.2f%% of %s's total"
                     % ("", 100.0 * (sa[0] - sb[0]) / ta[0], a))
        # The totals move for reasons that have nothing to do with step 6, and
        # saying so is not a hedge: the Lean-side phases move by more than the
        # whole treatment, so the difference of the totals is not a measurement
        # of the treatment and must not be quoted as one.
        noisy = 0.0
        for key in ("extractSeconds", "serveStartSeconds"):
            xa, xb = stat(a, key), stat(b, key)
            if xa and xb:
                noisy += abs(xb[0] - xa[0])
        lines.append("%22s  NOT ATTRIBUTABLE: `extract` + `serveStart` alone differ by"
                     % "")
        lines.append("%22s  %.3f s between these two variants, which is %.1fx the"
                     % ("", noisy, noisy / max(1e-9, sa[0] - sb[0])))
        lines.append("%22s  effect. Quote the step-6 line, not the total line."
                     % "")

    lines += ["", "### inside step 6 (median, s)", "",
              "%-26s%12s%12s%12s%12s" % ("", *VARIANTS)]
    for key in ("readSeconds", "stateLoadSeconds", "writeSeconds", "deltaSeconds",
                "stateSaveSeconds", "totalSeconds", "cacheHits", "cacheMisses",
                "stateBytes"):
        row = []
        for v in VARIANTS:
            s = stat(v, key, sub="global")
            row.append("%12.4f" % s[0] if s else "%12s" % "-")
        lines.append("%-26s%s" % ("global." + key, "".join(row)))
    lines += ["",
              "`global.totalSeconds` is the script's own timer; `globalSeconds` is the",
              "wall clock around the process(es). The gap is `deno` startup — twice on",
              "the old side, once on the new one."]

    lines += ["", "### the rest of the pipeline (median, s) — it must not have moved", "",
              "%-26s%12s%12s%12s%12s" % ("phase", *VARIANTS)]
    for key in ("serveStartSeconds", "detectSeconds", "extractSeconds",
                "ownershipSeconds", "mergeSeconds", "pruneSeconds", "globalSeconds",
                "impactSeconds", "renderSeconds", "serveStopSeconds", "totalSeconds"):
        row = []
        for v in VARIANTS:
            s = stat(v, key)
            row.append("%12.4f" % s[0] if s else "%12s" % "-")
        lines.append("%-26s%s" % (key, "".join(row)))

cp = os.path.join(W, "counts.jsonl")
rows = []
if os.path.exists(cp):
    rows = [json.loads(l) for l in open(cp, encoding="utf-8")]
    rows = [r for r in rows if "impl" in r]
if rows:
    lines += ["", "## C1 — module-file reads in one incremental run",
              "(counted by wrapping `Deno.readTextFile`, not by reading the code;",
              " 433 is the whole package)", "",
              "%-16s%22s%22s" % ("step", "old", "new")]
    steps = []
    for r in rows:
        if r["tag"] not in steps:
            steps.append(r["tag"])
    for tag in steps:
        cells = []
        for impl in ("old", "new"):
            hit = [r for r in rows if r["tag"] == tag and r["impl"] == impl]
            cells.append("%22s" % ("%d" % hit[0]["moduleFileReads"] if hit else "-"))
        lines.append("%-16s%s" % (tag, "".join(cells)))
    for impl in ("old", "new"):
        tot = sum(r["moduleFileReads"] for r in rows if r["impl"] == impl)
        full = sum(1 for r in rows if r["impl"] == impl and r["moduleFileReads"] >= 400)
        lines.append("%-16s%s" % ("TOTAL " + impl,
                                  "%22s" % ("%d reads, %d full passes" % (tot, full))))

open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage7h — L3-3: the whole-package artifacts, incrementalised"
  echo
  conditions "APFS clone of /Users/haruka/dev/lean-projects at $CLONE, own modules rebuilt in place"
  echo "measured          $( [ -f "$W/runs.jsonl" ] && date -u -r "$W/runs.jsonl" +%Y-%m-%dT%H:%M:%SZ || echo '(no runs.jsonl)') (mtime of runs.jsonl)"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "move              $A -> $X (minimal shim) — stage 6a/7g's move, so the shapes match"
  echo "runs              $RUNS per variant, interleaved, run 1 discarded"
  echo "jobs              4 on every variant (step 6 is single-threaded either way)"
  echo "render set        --mode self"
  echo "warm              same session; the clone was rebuilt before the run and its"
  echo "                  oleans are in page cache. No cold number is reported here."
  echo "control           stage5/global.ts (build + delta, two processes)"
  echo "treatment         stage7h/global.ts (build+delta in one, contentHash-keyed cache)"
  echo
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage7h-main.txt"
[ -f "$W/runs.jsonl" ] && cp "$W/runs.jsonl" "$RESULTS/stage7h-incremental.jsonl"
[ -f "$W/counts.jsonl" ] && cp "$W/counts.jsonl" "$RESULTS/stage7h-reads.jsonl"
echo "-> $RESULTS/stage7h-main.txt"
