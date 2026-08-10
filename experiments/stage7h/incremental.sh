#!/usr/bin/env bash
# One end-to-end incremental generation: a changed build tree in, an updated IR
# and updated HTML pages out.
#
# Stage 7h's copy of `stage7g/incremental.sh`. **One** difference, and nothing
# else — 7g's copy is untouched because docs quote its numbers:
#
#   `--global old|new` switches step 6 between `stage5/global.ts` (two processes,
#   a full read of all 433 module IRs in each) and `stage7h/global.ts` (one
#   process, a cache keyed on `contentHash`). `old` reproduces 7g exactly; it is
#   the control, and it exists so that the two sides can be measured in the same
#   session against the same clone rather than against 7g's stored numbers.
#
# Extraction (`stage7g/extract-once.sh`) and residency (`stage7g/serve-ctl.sh`)
# are 7g's, called in place: the A/B is about step 6, so everything else has to
# be literally the same program on both sides. The intermediate steps
# (`ledger.ts`, `ownership.ts`, `merge-ir.ts`, `prune-pages.ts`, `impact.ts`) are
# stage 5's, likewise called in place.
#
# `--count-reads <jsonl>` wraps every `deno` step in `count-reads.ts`, which
# counts module-file reads. It makes the timings meaningless and is never used on
# a timed run; it answers a different question — how many times one incremental
# run reads the whole IR — which is what says how large L3-3's share of the
# redundancy actually is.
#
# The stages are the ones `approach.md` §5.5 splits the problem into, timed
# separately inside one wall clock so the total is a measurement rather than a
# sum of medians taken at different times:
#
#   1 detect     hash ledger over the oleans -> the changed / added / removed
#                module sets (L1 + L2), plus whether the render key moved
#   2 extract    the extractor over exactly that set (Lean runs here)
#   3 ownership  L3-1: whose IR now names a module that no longer defines what
#                it points at? Those modules go into another extraction round.
#   4 merge      fold the partial IR back in, and drop removed modules; the IR
#                content hash decides which pages are stale
#   5 prune      delete the pages of removed modules
#   6 global     rebuild the whole-package artifacts and diff the global name ->
#                module map; names that moved in or out of it decide which
#                *other* pages went stale (L3-2)
#   7 render     render.ts over the page set
#
# WHY 2-3-4 IS A LOOP AND NOT A LINE
#   L3-1 cannot run before the extraction it depends on: knowing that a name
#   moved requires the fresh IR of the module it moved out of. So the shape is
#   "extract, then ask who that invalidated, then extract those too". Stage 5c
#   established that this cannot be replaced by widening the *first* set — the
#   referring module's olean does not change when a declaration moves, so no
#   ledger-side rule can reach it. Whether the loop terminates in two rounds is a
#   measurement, not an assumption; `--max-rounds` bounds it and the round count
#   is reported.
#
# RESIDENCY (stage 6a, rewired here in stage 7g)
#   `--serve-dir <d>` sends the extraction of rounds >= `--serve-from` (default 1,
#   i.e. all of them) to a resident extractor the *caller* started and owns.
#
#   `--serve auto` is stage 7g's addition and the shape stage 6a's measurement
#   picked: **one server per pipeline run**, started at the head of the run,
#   serving every round, stopped on the way out — through a `trap`, so a failure
#   does not leave a 3 GB process behind. Its startup is inside the run's wall
#   clock on purpose: that is what "one server per pipeline run" costs, and
#   hiding it would make the comparison against `fresh` dishonest. `--modules` is
#   required with it, because that list is the environment the server imports.
#
#   **Correctness comes from the server's olean generation, never from the round
#   number.** Stage 6a measured both: a server imported *before* the edit returns
#   the pre-edit owner for every name that moved — the exact stale pair L3-1 exists
#   to repair (`ownership.ts:8-20`, `Extract.lean:1352`) — and the site it produces
#   is wrong. With such a server no round is safe, including round 2, which is what
#   stage 5h had assumed was the safe one. With a server started *after* `lake
#   build`, every round is safe. `--serve auto` can only build the safe one: it
#   starts the server inside a run that begins after the build.
#
# PARALLELISM
#   `--jobs N` goes to the one-shot extractor *and* to the server `--serve auto`
#   starts, so that a resident run and a fresh run are compared at the same N.
#   The server has no per-request job count (`Extract.lean:2722` reuses the
#   start-time `cfg`), so with an externally started `--serve-dir` the caller is
#   responsible for having started it with the same N; this script cannot check it
#   beyond reading the server's `info.txt`.
#
# THE ONE THING THAT MAY BE FAKED, AND EXACTLY HOW MUCH
#   Against the measurement target no `.lean` file is edited and no `lake build`
#   is run, because that target must not be modified. "M changed" is injected by
#   invalidating M's ledger entry (`ledger.ts touch`). Against a *clone* of the
#   target (stages 5e/6a/7g) nothing is faked: the source is really edited and
#   `lake build` really runs.
#
# usage:
#   incremental.sh --module M --ir <live ir> --pages <live pages> --ledger <file>
#                  --work <dir> --mode self|referrers|importers|all
#                  [--source-url <u>] [--modules <list>] [--l3-1 on|off]
#                  [--max-rounds N] [--timings <p>] [--jobs N]
#                  [--serve auto] [--serve-dir <d>] [--serve-from N]
#                  [--global old|new] [--state <dir>] [--count-reads <jsonl>]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
S7G="$LD/experiments/stage7g"
RENDER_TS="$LD/experiments/stage7d/render.ts"
# The default is the revision every stage-5 number was taken at. It is an option
# because a *new* revision is the ordinary case, not an exotic one, and stage 5d
# measures what one costs.
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

MODULE=""; IR=""; PAGES=""; LEDGER=""; WORK=""; MODE=self; TIMINGS=""; MODULES=""
L31=on; MAXROUNDS=5; SERVEDIR=""; SERVEFROM=1; SERVE=""; JOBS="${JOBS:-1}"
GLOBAL=old; STATE=""; COUNTREADS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --global) GLOBAL="$2"; shift 2 ;;
    --state) STATE="$2"; shift 2 ;;
    --count-reads) COUNTREADS="$2"; shift 2 ;;
    --serve) SERVE="$2"; shift 2 ;;
    --serve-dir) SERVEDIR="$2"; shift 2 ;;
    --serve-from) SERVEFROM="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --source-url) SOURCE_URL="$2"; shift 2 ;;
    --module) MODULE="$2"; shift 2 ;;
    --ir) IR="$2"; shift 2 ;;
    --pages) PAGES="$2"; shift 2 ;;
    --ledger) LEDGER="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --mode) MODE="$2"; shift 2 ;;
    --modules) MODULES="$2"; shift 2 ;;
    --l3-1) L31="$2"; shift 2 ;;
    --max-rounds) MAXROUNDS="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$IR" ] && [ -n "$PAGES" ] && [ -n "$LEDGER" ] && [ -n "$WORK" ] || {
  echo "usage: incremental.sh --ir <dir> --pages <dir> --ledger <f> --work <dir>" >&2
  exit 2; }
if [ -n "$SERVE" ] && [ "$SERVE" != auto ]; then
  echo "--serve takes only 'auto' (use --serve-dir for a server you own)" >&2; exit 2
fi
if [ -n "$SERVE" ]; then
  [ -z "$SERVEDIR" ] || { echo "--serve auto and --serve-dir are exclusive" >&2; exit 2; }
  [ -n "$MODULES" ] || { echo "--serve auto needs --modules (the environment to import)" >&2; exit 2; }
fi
case "$GLOBAL" in
  old) [ -z "$STATE" ] || { echo "--state belongs to --global new" >&2; exit 2; } ;;
  new) [ -n "$STATE" ] || { echo "--global new needs --state <dir> (the cache lives outside the site)" >&2; exit 2; } ;;
  *) echo "--global takes old|new" >&2; exit 2 ;;
esac
mkdir -p "$WORK"

# Every `deno run` of the pipeline goes through this, so that `--count-reads` can
# wrap all of them at once and a timed run has exactly the old command line.
deno_ () { # deno_ <tag> <script> [args...]
  local tag="$1"; shift
  if [ -n "$COUNTREADS" ]; then
    deno run --allow-read --allow-write --allow-env "$HERE/count-reads.ts" \
      --tag "$tag" --out "$COUNTREADS" -- "$@"
  else
    deno run --allow-read --allow-write --allow-env "$@"
  fi
}
CHANGED="$WORK/changed.txt"
IRCHANGED="$WORK/ir-changed.txt"
RENDERSET="$WORK/render-set.txt"

# `time.monotonic()` is per-process on this platform (Python 3.9 / macOS returns
# time since interpreter start), so it cannot be sampled from separate
# processes. `time.time()` is the wall clock and is comparable across them; the
# stages here are 0.05 s and up, well above its resolution and above the ~0.03 s
# it costs to sample. The outer `/usr/bin/time -l` wall clock is the authority
# on the total; these stamps only split it.
now () { python3 -c 'import time; print(repr(time.time()))'; }
T0=$(now)

# --- the server this run owns, if any -----------------------------------------
# Started *after* T0 so that its cost is inside the total, and stopped from a
# trap so that neither a failure nor a `set -e` exit leaks a resident extractor.
SERVE_START_S=0
OWNSERVER=""
cleanup () {
  if [ -n "$OWNSERVER" ]; then
    "$S7G/serve-ctl.sh" stop "$OWNSERVER" >> "$WORK/serve.log" 2>&1 || true
    OWNSERVER=""
  fi
}
trap cleanup EXIT INT TERM
if [ -n "$SERVE" ]; then
  OWNSERVER="$WORK/server"
  A=$(now)
  JOBS="$JOBS" "$S7G/serve-ctl.sh" start "$OWNSERVER" "$MODULES" per-run "$JOBS" \
    > "$WORK/serve.log" 2>&1
  SERVE_START_S=$(python3 -c "print(repr($(now) - $A))")
  SERVEDIR="$OWNSERVER"
  SERVEFROM=1
fi

# The global name -> module map as it stands *before* this run. Snapshotted
# rather than recomputed, because step 6 overwrites it in place.
NAMEMAP="$PAGES/declarations/name-map.json"
MAPBEFORE="$WORK/name-map-before.json"
rm -f "$MAPBEFORE"
[ -f "$NAMEMAP" ] && cp "$NAMEMAP" "$MAPBEFORE"

# 1 -- detect ---------------------------------------------------------------
# `--ir` is not optional here: without it the ledger cannot see the IR schema or
# the generator id, so a schema bump would leave every page stale with the
# ledger reporting "0 changed" (stage 5b, S1). `--modules` re-reads the current
# module list so that additions and deletions are visible at all.
#
# `--source-url` is not optional either, for the mirror-image reason: it is the
# renderer's input, it appears in the page bytes, and it carries the git
# revision, so it changes on every commit. It goes into the *render* key, not
# the extract key, so a new revision re-renders every page without starting Lean
# once (stage 5d: 18.39 s -> 1.34 s).
REMOVEDF="$WORK/removed.txt"
RENDERALLF="$WORK/render-all.txt"
deno_ detect "$S5/ledger.ts" check \
  --ledger "$LEDGER" --ir "$IR" --source-url "$SOURCE_URL" \
  --changed-out "$CHANGED" --removed-out "$REMOVEDF" --render-all-out "$RENDERALLF" \
  ${MODULES:+--modules "$MODULES"} > "$WORK/detect.log"
T1=$(now)
NCHANGED=$(grep -c . "$CHANGED" || true)
NREMOVED=$(grep -c . "$REMOVEDF" 2>/dev/null || true)
NRENDERALL=$(grep -c . "$RENDERALLF" 2>/dev/null || true)

# 2/3/4 -- extract, ownership, merge, in rounds ------------------------------
EXTRACT_S=0; OWN_S=0; MERGE_S=0; ROUNDS=0; NSTALE_TOTAL=0
: > "$IRCHANGED"
SEEN="$WORK/seen.txt"; cp "$CHANGED" "$SEEN"
ROUND_IN="$CHANGED"
add_s () { python3 -c "print(repr($1 + $2))"; }

while [ "$(grep -c . "$ROUND_IN" || true)" -gt 0 ] || \
      { [ "$ROUNDS" -eq 0 ] && [ "${NREMOVED:-0}" -gt 0 ]; }; do
  ROUNDS=$((ROUNDS + 1))
  INCIR="$WORK/inc-ir-$ROUNDS"
  rm -rf "$INCIR"
  NIN=$(grep -c . "$ROUND_IN" || true)

  if [ "$NIN" -gt 0 ]; then
    SERVE_ARG=()
    if [ -n "$SERVEDIR" ] && [ "$ROUNDS" -ge "$SERVEFROM" ]; then
      SERVE_ARG=(--serve-dir "$SERVEDIR")
      echo "  round $ROUNDS: served by the resident extractor at $SERVEDIR" >&2
    fi
    A=$(now)
    "$S7G/extract-once.sh" --modules "$ROUND_IN" --ir-dir "$INCIR" \
      --jobs "$JOBS" \
      --timings "$WORK/extract-timings-$ROUNDS.json" \
      --events "$WORK/extract-events-$ROUNDS.jsonl" \
      ${SERVE_ARG[@]+"${SERVE_ARG[@]}"}
    EXTRACT_S=$(add_s "$EXTRACT_S" "$(python3 -c "print($(now) - $A)")")
  fi

  # Deletions are handled in the first round only: after it the modules are gone
  # from the IR, and asking again would be asking about nothing.
  INC_ARG=(); [ "$NIN" -gt 0 ] && INC_ARG=(--inc "$INCIR")
  DEL_ARG=(); OWN_DEL_ARG=()
  if [ "$ROUNDS" -eq 1 ] && [ "${NREMOVED:-0}" -gt 0 ]; then
    DEL_ARG=(--remove "$REMOVEDF")
    OWN_DEL_ARG=(--removed "$REMOVEDF")
  fi

  # 3 -- ownership (L3-1). Must run *before* the merge: it needs the IR's
  # previous idea of who owns each name, which the merge is about to overwrite.
  STALE="$WORK/stale-$ROUNDS.txt"
  : > "$STALE"
  if [ "$L31" = on ]; then
    A=$(now)
    deno_ "ownership-r$ROUNDS" "$S5/ownership.ts" --base "$IR" \
      ${INC_ARG[@]+"${INC_ARG[@]}"} ${OWN_DEL_ARG[@]+"${OWN_DEL_ARG[@]}"} \
      --exclude "$SEEN" --print-set "$STALE" --json "$WORK/ownership-$ROUNDS.json" \
      > "$WORK/ownership-$ROUNDS.log"
    OWN_S=$(add_s "$OWN_S" "$(python3 -c "print($(now) - $A)")")
  fi

  # 4 -- merge. The removals are folded into the first round's merge, so the IR
  # is never left in a state where a deleted module is still indexed.
  A=$(now)
  deno_ "merge-r$ROUNDS" "$S5/merge-ir.ts" --base "$IR" \
    ${INC_ARG[@]+"${INC_ARG[@]}"} --out "$IR" \
    --changed-out "$WORK/ir-changed-$ROUNDS.txt" ${DEL_ARG[@]+"${DEL_ARG[@]}"} \
    --timings "$WORK/merge-timings-$ROUNDS.json" > "$WORK/merge-$ROUNDS.log"
  MERGE_S=$(add_s "$MERGE_S" "$(python3 -c "print($(now) - $A)")")
  cat "$WORK/ir-changed-$ROUNDS.txt" >> "$IRCHANGED"

  NSTALE=$(grep -c . "$STALE" || true)
  NSTALE_TOTAL=$((NSTALE_TOTAL + NSTALE))
  cat "$STALE" >> "$SEEN"
  ROUND_IN="$STALE"
  if [ "$ROUNDS" -ge "$MAXROUNDS" ] && [ "$NSTALE" -gt 0 ]; then
    echo "incremental.sh: still $NSTALE stale module(s) after $ROUNDS rounds" >&2
    exit 5
  fi
done
T2=$(now)
NIRCHANGED=$(grep -c . "$IRCHANGED" || true)

# 5 -- prune ----------------------------------------------------------------
# The third of the deletion path. The renderer only ever writes, so without this
# a deleted module's page survives every later run and is indistinguishable from
# a live one.
if [ "${NREMOVED:-0}" -gt 0 ]; then
  deno_ prune "$S5/prune-pages.ts" --pages "$PAGES" \
    --remove "$REMOVEDF" --json "$WORK/prune.json" > "$WORK/prune.log"
fi
T3=$(now)

# 6 -- global ---------------------------------------------------------------
# The whole-package artifacts, and the diff of the global name -> module map. The
# diff is the reason this runs *before* the render: it names every declaration
# whose links can have changed anywhere on the site.
#
#   old  stage 5's two commands. Each reads all 433 module IRs — `build` to
#        derive the artifacts, `delta` to find which pages mention a name that
#        moved — and each pays `deno` startup.
#   new  stage 7h's one command. Same derivation over the same 433 modules, but
#        the facts it derives from come from a `contentHash`-keyed cache outside
#        the site (`--state`), so only the modules whose IR bytes moved are read.
#
# The artifacts are byte-identical between the two by construction and by
# `oracle.sh`; the `--print-set` they hand to step 7 is checked equal there too.
GLOBALSET="$WORK/global-set.txt"
: > "$GLOBALSET"
if [ "$GLOBAL" = new ]; then
  BEFORE_ARG=()
  [ -f "$MAPBEFORE" ] && BEFORE_ARG=(--before "$MAPBEFORE" --print-set "$GLOBALSET" \
    --delta-json "$WORK/global-delta.json")
  deno_ global "$HERE/global.ts" build --ir "$IR" --out "$PAGES" --state "$STATE" \
    ${BEFORE_ARG[@]+"${BEFORE_ARG[@]}"} \
    --timings "$WORK/global-timings.json" > "$WORK/global.log"
else
  deno_ global "$S5/global.ts" build --ir "$IR" \
    --out "$PAGES" --timings "$WORK/global-timings.json" > "$WORK/global.log"
  if [ -f "$MAPBEFORE" ]; then
    deno_ global-delta "$S5/global.ts" delta --before "$MAPBEFORE" \
      --after "$NAMEMAP" --ir "$IR" --print-set "$GLOBALSET" \
      --json "$WORK/global-delta.json" > "$WORK/global-delta.log"
  fi
fi
T4=$(now)
NGLOBAL=$(grep -c . "$GLOBALSET" || true)

# 7 -- render ---------------------------------------------------------------
# `--mode` decides the page set (see the header). A changed render key overrides
# it with `all`: that is the one page set not derived from the changed module
# set, which is the point of splitting the key — nothing was re-extracted, yet
# every page is stale.
MODE_EFF="$MODE"
if [ "${NRENDERALL:-0}" -gt 0 ]; then
  MODE_EFF=all
  sed 's/^/  render-all /' "$RENDERALLF" >&2
fi
deno_ impact "$S5/impact.ts" --ir "$IR" \
  --changed-file "$SEEN" --mode "$MODE_EFF" --print-set "$WORK/impact-set.txt" \
  > "$WORK/impact.log"
# The render set is the union of the two derivations, which is the whole point
# of deriving them separately: one comes from the changed modules, the other
# from the global map and reaches modules the first cannot see.
sort -u "$WORK/impact-set.txt" "$GLOBALSET" | grep . > "$RENDERSET" || : > "$RENDERSET"
T5=$(now)
ONLY=()
while read -r m; do [ -n "$m" ] && ONLY+=(--only "$m"); done < "$RENDERSET"
# An empty render set must skip the renderer, not run it bare. `render.ts` treats
# "no --only" as "every module", so an empty set would have re-rendered all 432
# pages — the exact opposite of what it means.
if [ "${#ONLY[@]}" -eq 0 ]; then
  echo '{"skipped":"empty render set"}' > "$WORK/render-timings.json"
  echo "render: nothing to render" > "$WORK/render.log"
else
  deno_ render "$RENDER_TS" \
    --ir "$IR" --pages "$PAGES" --source-url "$SOURCE_URL" \
    --timings "$WORK/render-timings.json" ${ONLY[@]+"${ONLY[@]}"} > "$WORK/render.log"
fi
T6=$(now)

# The server dies here rather than at process exit, so that its teardown is
# inside the measured total on the same side as its startup. `cleanup` is
# idempotent and the trap still fires.
cleanup
T7=$(now)

NPAGES=$(grep -c . "$RENDERSET" || true)
python3 - "$TIMINGS" "$T0" "$T1" "$T2" "$T3" "$T4" "$T5" "$T6" "$T7" \
  "$EXTRACT_S" "$OWN_S" "$MERGE_S" "$ROUNDS" "$NSTALE_TOTAL" \
  "$NCHANGED" "$NREMOVED" "$NIRCHANGED" "$NGLOBAL" "$NPAGES" "$MODULE" "$MODE_EFF" "$L31" "$WORK" \
  "$SERVEDIR" "$SERVEFROM" "$SERVE_START_S" "$JOBS" "${SERVE:-}" "$GLOBAL" <<'PY'
import json, sys, os, glob
(out, t0, t1, t2, t3, t4, t5, t6, t7, ex, ow, mg, rounds, nstale,
 nch, nrm, nir, nglob, npg, module, mode, l31, work, servedir, servefrom,
 servestart, jobs, serve, globalimpl) = sys.argv[1:]
t = [float(x) for x in (t0, t1, t2, t3, t4, t5, t6, t7)]
rec = {
    "module": module, "mode": mode, "l3_1": l31,
    # Which step 6 ran. Without it the two sides of stage 7h's A/B are
    # indistinguishable in the record, which is the whole result.
    "global_impl": globalimpl,
    # Self-describing on purpose: a resident run and a fresh run are otherwise
    # indistinguishable in the record, and this stage's whole result is the
    # difference between them.
    "serve": serve or None,
    "serveDir": servedir or None,
    "serveFrom": int(servefrom) if servedir else None,
    "jobs": int(jobs),
    # Startup and teardown of a server this run owns. Both are inside
    # totalSeconds; they are broken out so the split is readable, not so it can
    # be subtracted out of the comparison.
    "serveStartSeconds": float(servestart),
    "serveStopSeconds": t[7] - t[6],
    "detectSeconds": t[1] - t[0] - float(servestart),
    "extractSeconds": float(ex),
    "ownershipSeconds": float(ow),
    "mergeSeconds": float(mg),
    "roundsSeconds": t[2] - t[1],
    "pruneSeconds": t[3] - t[2],
    "globalSeconds": t[4] - t[3],
    "impactSeconds": t[5] - t[4],
    "renderSeconds": t[6] - t[5],
    "totalSeconds": t[7] - t[0],
    "rounds": int(rounds), "staleFound": int(nstale),
    "changed": int(nch), "removed": int(nrm or 0),
    "irChanged": int(nir), "globalStale": int(nglob or 0),
    "pagesRendered": int(npg),
}
for name, pattern in (("extract", "extract-timings-*.json"),
                      ("merge", "merge-timings-*.json"),
                      ("global", "global-timings.json"),
                      ("render", "render-timings.json")):
    hits = sorted(glob.glob(os.path.join(work, pattern)))
    if not hits:
        continue
    try:
        loaded = [json.load(open(p, encoding="utf-8")) for p in hits]
        rec[name] = loaded[0] if len(loaded) == 1 else loaded
    except Exception:
        pass
line = json.dumps(rec)
print(line)
if out:
    open(out, "w", encoding="utf-8").write(line + "\n")
PY
