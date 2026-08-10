#!/usr/bin/env bash
# Stage 6a — W0..W5: assembling the resident extractor into `incremental.sh`.
#
# Four ways of running the same two-round L3-1 case, all through the real
# `incremental.sh`:
#
#   fresh    a process per round                    (the current pipeline)
#   pre      round 2 -> server P (pre-edit env)     (what the 5.06 s assumed)
#   post     round 2 -> server Q (post-build env)
#   postall  every round -> server Q
#
# and one oracle: REFERENCE, the post-move state reached by extracting and
# rendering everything from scratch. Byte equality with REFERENCE is the only
# check a wrong answer cannot satisfy — the pipeline's own "0 stale" report comes
# from the code under test (stages 5e and 5f were both saved by this).
#
# THE TWO SERVERS ARE NEVER ALIVE AT THE SAME TIME. 3.3 GB peak RSS each on a
# 16 GB machine, and the workload is memory-bound; paging would land in the
# timings. Phase 1 runs `fresh` + `pre` under P, phase 2 runs `fresh` + `post` +
# `postall` under Q, and the two `fresh` medians are the check that the phases are
# comparable.
#
# PREREQUISITE: `stage5e/rebuild-own.sh` must have run on the clone, so every
# olean in it carries the clone's path. Without it the baseline ledger describes
# oleans built at the original path and every rebuilt module reports as changed
# for path reasons alone (stage 5e learned this the hard way).
#
# usage: run.sh <work-dir> <clone-dir> [module-to-move] [runs]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
CLONE=${2:?clone dir}
A=${3:-InformationTheory.Shannon.Huffman.Length}
RUNS=${4:-6}
X="${A}Core"
RESULTS="$LD/benchmarks/results"
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

mkdir -p "$W"
OUT="$W/out"; mkdir -p "$OUT"
export TARGET_REPO="$CLONE"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }
render () { deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$1" --pages "$2" --source-url "$URL" > /dev/null
  deno run --allow-read --allow-write "$S5/global.ts" build --ir "$1" --out "$2" > /dev/null; }

# A glob over the sources, never a scan of `.lake/build`: Lake does not delete
# orphaned oleans, so the build tree is not a list of what exists.
modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
  echo "  $(grep -c . "$1") modules -> $1"
}

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
  "$S5/extract-once.sh" --modules "$W/modules-before.txt" --ir-dir "$W/base-ir" \
    --timings "$W/base-extract.json" > "$W/base-extract.log"
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
for k, v in hits.most_common():
    print("    %-70s %3d  e.g. %s" % (k, v, sorted(names[k])[0]))
if not hits:
    sys.exit("no module names anything of %s in a printed signature; pick another" % a)
PY

echo "### baseline fixed-point check"
deno_ "$S5/ledger.ts" check --ledger "$W/base-ledger.json" --ir "$W/base-ir" \
  --source-url "$URL" --modules "$W/modules-before.txt" --changed-out /dev/null \
  --render-all-out /dev/null | tee "$OUT/w-base.txt"
grep -q ": 0 changed, 0 added, 0 removed" "$OUT/w-base.txt" || {
  echo "baseline is not a fixed point; the clone drifted" >&2; exit 3; }

# ------------------------------------------------------- server P, then the move
# P must exist BEFORE the edit: that is the only way to get an environment whose
# snapshot of the oleans predates it, which is what the extrapolation assumed.
echo "### starting server P (pre-edit environment)"
"$HERE/serve-ctl.sh" start "$W/P" "$W/modules-before.txt" pre-edit

echo "### applying the move and rebuilding (P is alive across this)"
"$S5/../stage5e/setup-clone.sh" move "$CLONE" "$A" minimal > "$W/move.log" 2>&1 || {
  echo "move failed; see $W/move.log" >&2; tail -20 "$W/move.log" >&2; exit 1; }
grep -E "^A is now" "$W/move.log" || true
modlist "$W/modules-after.txt"

# W0 — did P survive having its oleans rewritten underneath it?
echo "### W0 — P after the rebuild"
{
  if kill -0 "$(cat "$W/P/server.pid")" 2>/dev/null; then
    echo "W0 P is still running after lake build: yes"
    echo "$A" > "$W/probe.txt"
    if "$HERE/serve-ctl.sh" request "$W/P" "$W/probe.txt" "$W/p-probe.jsonl" "$W/p-probe-ir" \
         > "$OUT/w0-probe.txt" 2>&1; then
      echo "W0 P still answers a request: yes ($(cat "$OUT/w0-probe.txt"))"
    else
      echo "W0 P still answers a request: NO"
      cat "$OUT/w0-probe.txt"
    fi
  else
    echo "W0 P is still running after lake build: NO — it died during the build"
    tail -5 "$W/P/serve.err" 2>/dev/null || true
  fi
} | tee "$OUT/w0.txt"

# ------------------------------------------------------------------- REFERENCE
echo "### REFERENCE — extract everything from the moved state, render everything"
rm -rf "$W/ref-ir" "$W/ref-pages"
"$S5/extract-once.sh" --modules "$W/modules-after.txt" --ir-dir "$W/ref-ir" \
  --timings "$W/ref-extract.json" > "$W/ref-extract.log"
render "$W/ref-ir" "$W/ref-pages"

# ------------------------------------------------------------------- variants
: > "$W/runs.jsonl"
variant () { # variant <name> <run> [extra incremental.sh args...]
  local v="$1" run="$2"; shift 2
  local d="$W/$v"
  rm -rf "$d"; mkdir -p "$d"
  cp -R "$W/base-ir" "$d/ir"
  cp -R "$W/base-pages" "$d/pages"
  cp "$W/base-ledger.json" "$d/ledger.json"
  if "$S5/incremental.sh" --module "$A" --ir "$d/ir" --pages "$d/pages" \
       --ledger "$d/ledger.json" --modules "$W/modules-after.txt" \
       --source-url "$URL" --work "$d/work" --mode self --l3-1 on \
       --timings "$d/timings.json" "$@" > "$OUT/$v-run$run.txt" 2>&1; then
    python3 - "$d/timings.json" "$v" "$run" >> "$W/runs.jsonl" <<'PY'
import json, sys
p, v, run = sys.argv[1:]
r = json.load(open(p, encoding="utf-8"))
r["variant"], r["run"] = v, int(run)
print(json.dumps(r))
PY
    python3 -c "
import json
r = json.load(open('$d/timings.json'))
print('  %-8s run %s: rounds %d, changed %d, stale %d, irChanged %d, pages %d, total %.3f s'
      % ('$v', '$run', r['rounds'], r['changed'], r['staleFound'], r['irChanged'],
         r['pagesRendered'], r['totalSeconds']))"
  else
    echo "  $v run $run FAILED:"; tail -12 "$OUT/$v-run$run.txt"
  fi
  # The last run's tree is the one compared against REFERENCE. Copied only on that
  # run: every run overwrites the same variant directory, so copying each time
  # would move ~40 MB per page tree for nothing.
  if [ "$run" = "$RUNS" ]; then
    rm -rf "$W/keep-$v"; cp -R "$d/pages" "$W/keep-$v"
    rm -rf "$W/keep-ir2-$v"
    [ -d "$d/work/inc-ir-2" ] && cp -R "$d/work/inc-ir-2" "$W/keep-ir2-$v" || true
  fi
}

echo "### phase 1 — P alive: fresh and pre, interleaved, $RUNS runs (run 1 discarded)"
for r in $(seq 1 "$RUNS"); do
  variant fresh "$r"
  variant pre   "$r" --serve-dir "$W/P" --serve-from 2
done
echo "### stopping server P"
"$HERE/serve-ctl.sh" stop "$W/P" | tee -a "$OUT/w0.txt"

echo "### starting server Q (post-build environment)"
"$HERE/serve-ctl.sh" start "$W/Q" "$W/modules-after.txt" post-build

echo "### phase 2 — Q alive: fresh2, post, postall, interleaved, $RUNS runs"
for r in $(seq 1 "$RUNS"); do
  variant fresh2  "$r"
  variant post    "$r" --serve-dir "$W/Q" --serve-from 2
  variant postall "$r" --serve-dir "$W/Q" --serve-from 1
done
echo "### stopping server Q"
"$HERE/serve-ctl.sh" stop "$W/Q"

# ------------------------------------------------------------------- W1..W5
python3 - "$W" "$A" "$X" "$OUT/verdict.txt" <<'PY'
import difflib, json, os, statistics, sys
W, A, X, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

referrers = [l.strip() for l in open(os.path.join(W, "referrers.txt"),
                                    encoding="utf-8") if l.strip()]
ref_pages = tree(os.path.join(W, "ref-pages"))
lines = ["REFERENCE pages: %d" % len(ref_pages),
         "referrers of A:  %d  (%s)" % (len(referrers), ", ".join(referrers))]

# ---- W1: the round-2 IR itself, before any rendering blurs it
lines += ["", "## W1 — the round-2 IR each path produced"]
ref_ir = os.path.join(W, "ref-ir", "modules")
def ir2(v):
    d = os.path.join(W, "keep-ir2-" + v, "modules")
    if not os.path.isdir(d):
        return None
    return {f: open(os.path.join(d, f), "rb").read() for f in os.listdir(d)}
truth = {}
for v in ("fresh", "pre", "post"):
    got = ir2(v)
    if got is None:
        lines.append("W1 %-8s no round 2 happened" % v)
        continue
    verdicts = []
    for f, b in sorted(got.items()):
        want_p = os.path.join(ref_ir, f)
        want = open(want_p, "rb").read() if os.path.exists(want_p) else None
        same = want is not None and want == b
        verdicts.append((f, same, len(b), len(want) if want else 0))
        truth.setdefault(f, want)
    lines.append("W1 %-8s round-2 modules: %s"
                 % (v, ", ".join("%s %s" % (f.replace(".json", ""),
                                            "byte-equal to REFERENCE" if s else "DIFFERS")
                                 for f, s, _, _ in verdicts)))
    for f, s, gl, wl in verdicts:
        if s:
            continue
        lines.append("     %s: %d B vs REFERENCE %d B" % (f, gl, wl))
        g = json.loads(got[f].decode("utf-8"))
        r = json.loads(truth[f].decode("utf-8")) if truth[f] else {"declarations": []}
        gr = {d["name"]: {tuple(x) for x in d["refs"]} for d in g["declarations"]}
        rr = {d["name"]: {tuple(x) for x in d["refs"]} for d in r["declarations"]}
        shown = 0
        for n in sorted(set(gr) & set(rr)):
            if gr[n] == rr[n] or shown >= 3:
                continue
            only_got = sorted(gr[n] - rr[n])
            only_ref = sorted(rr[n] - gr[n])
            lines.append("       %s" % n)
            for o, nm in only_got[:3]:
                lines.append("         this path says the owner is  %s :: %s" % (o, nm))
            for o, nm in only_ref[:3]:
                lines.append("         REFERENCE says the owner is  %s :: %s" % (o, nm))
            shown += 1

# ---- W2/W3: the assembled site
lines += ["", "## W2/W3 — each assembled site against REFERENCE"]
for v in ("fresh", "pre", "post", "postall", "fresh2"):
    p = os.path.join(W, "keep-" + v)
    if not os.path.isdir(p):
        continue
    got = tree(p)
    missing = sorted(set(ref_pages) - set(got))
    extra = sorted(set(got) - set(ref_pages))
    diff = sorted(k for k in set(ref_pages) & set(got) if ref_pages[k] != got[k])
    lines.append("%-8s %d pages, missing %d, extra %d, differing %d  -> %s"
                 % (v, len(got), len(missing), len(extra), len(diff),
                    "byte-identical to REFERENCE" if not (missing or extra or diff)
                    else "WRONG"))
    if missing:
        lines.append("  missing:   " + ", ".join(missing[:5]))
    if extra:
        lines.append("  extra:     " + ", ".join(extra[:5]))
    if diff:
        lines.append("  differing: " + ", ".join(diff[:10]) + (" ..." if len(diff) > 10 else ""))
        bad = [r for r in referrers if r.replace(".", "/") + ".html" in diff]
        lines.append("  of those, referrer pages: %d/%d %s"
                     % (len(bad), len(referrers), bad[:3]))
        k = diff[0]
        a = ref_pages[k].decode("utf-8", "replace").split("\n")
        b = got[k].decode("utf-8", "replace").split("\n")
        d1 = list(difflib.unified_diff(b, a, "this variant", "reference", n=0, lineterm=""))
        lines.append("  first difference in %s, variant -> reference:" % k)
        lines += ["    " + l[:200] for l in d1[2:8]]

# ---- W4/W5: the clock
lines += ["", "## W4/W5 — the clock"]
runs = [json.loads(l) for l in open(os.path.join(W, "runs.jsonl"), encoding="utf-8")]
def med(v, key):
    xs = [r[key] for r in runs if r["variant"] == v and r["run"] > 1]
    return statistics.median(xs) if xs else float("nan")
def rng(v, key):
    xs = [r[key] for r in runs if r["variant"] == v and r["run"] > 1]
    return (min(xs), max(xs)) if xs else (float("nan"), float("nan"))
startup = {}
for tag, d in (("P", "P"), ("Q", "Q")):
    p = os.path.join(W, d, "info.txt")
    if os.path.exists(p):
        for l in open(p, encoding="utf-8"):
            if l.startswith("startSeconds"):
                startup[tag] = float(l.split()[1])
lines.append("server startup: " + ", ".join("%s %.3f s" % (k, v) for k, v in startup.items()))
lines.append("")
lines.append("%-8s %10s %22s %10s %10s %7s" %
             ("variant", "total", "[min-max]", "extract", "rounds", "pages"))
for v in ("fresh", "pre", "fresh2", "post", "postall"):
    xs = [r for r in runs if r["variant"] == v and r["run"] > 1]
    if not xs:
        continue
    lo, hi = rng(v, "totalSeconds")
    lines.append("%-8s %10.3f %22s %10.3f %10.1f %7.0f"
                 % (v, med(v, "totalSeconds"), "[%.3f-%.3f]" % (lo, hi),
                    med(v, "extractSeconds"), med(v, "rounds"), med(v, "pagesRendered")))
lines.append("")
lines.append("fresh vs fresh2 (phase comparability): %.3f vs %.3f s, delta %.3f s"
             % (med("fresh", "totalSeconds"), med("fresh2", "totalSeconds"),
                med("fresh2", "totalSeconds") - med("fresh", "totalSeconds")))
if "Q" in startup:
    for v in ("post", "postall"):
        if any(r["variant"] == v for r in runs):
            lines.append("%s including Q's startup (a per-run server must pay it): "
                         "%.3f + %.3f = %.3f s vs fresh2 %.3f s -> %s"
                         % (v, med(v, "totalSeconds"), startup["Q"],
                            med(v, "totalSeconds") + startup["Q"], med("fresh2", "totalSeconds"),
                            "a LOSS" if med(v, "totalSeconds") + startup["Q"]
                            > med("fresh2", "totalSeconds") else "a win"))
lines.append("")
lines.append("stage 5h's extrapolation was 7.989 -> 5.06 s for the 'pre' shape.")
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

# ------------------------------------------------------------------- report
{
  echo "# stage6a — wiring the resident extractor into incremental.sh"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
  echo "lean-toolchain    $(cat "$CLONE/lean-toolchain")"
  echo "target            APFS clone of /Users/haruka/dev/lean-projects, own modules rebuilt in place"
  echo "move              $A -> $X (minimal shim)"
  echo "runs              $RUNS per variant, interleaved within a phase, run 1 discarded"
  echo "render set        --mode self"
  echo "servers           P = pre-edit env (before the move), Q = post-build env; never alive together"
  echo
  echo "## W0 — does a resident server survive the build?"
  cat "$OUT/w0.txt"
  echo
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage6a-resident-wiring.txt"
cp "$W/runs.jsonl" "$RESULTS/stage6a-resident-wiring.jsonl"
echo "-> $RESULTS/stage6a-resident-wiring.txt"
