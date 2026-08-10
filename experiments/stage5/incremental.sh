#!/usr/bin/env bash
# One end-to-end incremental generation: a changed build tree in, an updated IR
# and updated HTML pages out.
#
# The stages are the ones `approach.md` §5.5 splits the problem into, timed
# separately inside one wall clock so the total is a measurement rather than a
# sum of medians taken at different times:
#
#   1 detect     hash ledger over the oleans -> the changed / added / removed
#                module sets (L1 + L2), plus whether the render key moved
#   2 extract    the stage-4b extractor over exactly that set (Lean runs here)
#   3 ownership  L3-1: whose IR now names a module that no longer defines what
#                it points at? Those modules go into another extraction round.
#   4 merge      fold the partial IR back in, and drop removed modules; the IR
#                content hash decides which pages are stale
#   5 prune      delete the pages of removed modules
#   6 global     rebuild the whole-package artifacts and diff the global name ->
#                module map; names that moved in or out of it decide which
#                *other* pages went stale (L3-2)
#   7 render     stage4c's render.ts over the page set
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
# THE ONE THING THAT MAY BE FAKED, AND EXACTLY HOW MUCH
#   Against the measurement target no `.lean` file is edited and no `lake build`
#   is run, because that target must not be modified. "M changed" is injected by
#   invalidating M's ledger entry (`ledger.ts touch`). Everything else is real
#   work on real inputs. What the numbers therefore exclude is `lake build`
#   itself, which is outside lean-doc but on the critical path of a real edit.
#
#   A consequence worth stating rather than hiding: with an injected change
#   nothing actually moved, so the merge finds the re-extracted IR
#   byte-identical and reports zero stale pages. That is the correct answer and
#   it is checked (`irChanged`), but it would make the render free, so `--mode`
#   forces a page set instead:
#     self       the re-extracted modules
#     referrers  self + the modules whose printed text names something of M's
#     importers  self + everything that transitively imports M (the sound bound)
#     all        every module (also forced automatically when the render key
#                moved — see stage 5d)
#
#   Against a *clone* of the target (stage 5e) nothing is faked: the source is
#   really edited and `lake build` really runs.
#
# usage:
# RESIDENCY (stage 6a)
#   `--serve-dir <d>` sends the extraction of rounds >= `--serve-from` (default 1,
#   i.e. all of them) to a resident extractor instead of starting a process.
#
#   **Correctness comes from the server's olean generation, never from the round
#   number.** Stage 6a measured both: a server imported *before* the edit returns
#   the pre-edit owner for every name that moved — the exact stale pair L3-1 exists
#   to repair (`ownership.ts:8-20`, `Extract.lean:1352`) — and the site it produces
#   is wrong. With such a server no round is safe, including round 2, which is what
#   stage 5h had assumed was the safe one. With a server started *after* `lake
#   build`, every round is safe.
#
#   The default is 1 because that is the configuration the measurement favours:
#   serving only rounds 2+ removes one environment load but adds a whole server
#   startup, which is a net loss (8.788 s vs 7.940 s), while serving every round
#   removes both loads for the same startup and wins (6.168 s vs 7.940 s). The
#   caller is responsible for pointing `--serve-dir` at a post-build server.
#
# usage:
#   incremental.sh --module M --ir <live ir> --pages <live pages> --ledger <file>
#                  --work <dir> --mode self|referrers|importers|all
#                  [--source-url <u>] [--modules <list>] [--l3-1 on|off]
#                  [--max-rounds N] [--timings <p>]
#                  [--serve-dir <d>] [--serve-from N]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
# The default is the revision every stage-5 number was taken at. It is an option
# because a *new* revision is the ordinary case, not an exotic one, and stage 5d
# measures what one costs.
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

MODULE=""; IR=""; PAGES=""; LEDGER=""; WORK=""; MODE=self; TIMINGS=""; MODULES=""
L31=on; MAXROUNDS=5; SERVEDIR=""; SERVEFROM=1
while [ $# -gt 0 ]; do
  case "$1" in
    --serve-dir) SERVEDIR="$2"; shift 2 ;;
    --serve-from) SERVEFROM="$2"; shift 2 ;;
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
mkdir -p "$WORK"
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
deno run --allow-read --allow-write --allow-env "$HERE/ledger.ts" check \
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
    "$HERE/extract-once.sh" --modules "$ROUND_IN" --ir-dir "$INCIR" \
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
    deno run --allow-read --allow-write "$HERE/ownership.ts" --base "$IR" \
      ${INC_ARG[@]+"${INC_ARG[@]}"} ${OWN_DEL_ARG[@]+"${OWN_DEL_ARG[@]}"} \
      --exclude "$SEEN" --print-set "$STALE" --json "$WORK/ownership-$ROUNDS.json" \
      > "$WORK/ownership-$ROUNDS.log"
    OWN_S=$(add_s "$OWN_S" "$(python3 -c "print($(now) - $A)")")
  fi

  # 4 -- merge. The removals are folded into the first round's merge, so the IR
  # is never left in a state where a deleted module is still indexed.
  A=$(now)
  deno run --allow-read --allow-write "$HERE/merge-ir.ts" --base "$IR" \
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
  deno run --allow-read --allow-write "$HERE/prune-pages.ts" --pages "$PAGES" \
    --remove "$REMOVEDF" --json "$WORK/prune.json" > "$WORK/prune.log"
fi
T3=$(now)

# 6 -- global ---------------------------------------------------------------
# Rebuilt outright rather than patched: reading all 432 module IRs and writing
# every whole-package artifact costs 0.136 s (stage 5g), which is 2-3% of an
# incremental run — the same answer the ledger gave, for the same reason.
#
# Its by-product is the reason it runs *before* the render: the diff of the
# global name -> module map names every declaration whose links can have changed
# anywhere on the site. Stage 5f measured a deletion dropping six names and
# leaving a live link to a vanished anchor in a module that shares no import and
# no reference with the deleted one (§5.5 L3-2). Those pages are found by
# scanning docstrings for the changed names, not guessed at from the graph.
deno run --allow-read --allow-write "$HERE/global.ts" build --ir "$IR" \
  --out "$PAGES" --timings "$WORK/global-timings.json" > "$WORK/global.log"
GLOBALSET="$WORK/global-set.txt"
: > "$GLOBALSET"
if [ -f "$MAPBEFORE" ]; then
  deno run --allow-read --allow-write "$HERE/global.ts" delta --before "$MAPBEFORE" \
    --after "$NAMEMAP" --ir "$IR" --print-set "$GLOBALSET" \
    --json "$WORK/global-delta.json" > "$WORK/global-delta.log"
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
deno run --allow-read --allow-write "$HERE/impact.ts" --ir "$IR" \
  --changed-file "$SEEN" --mode "$MODE_EFF" --print-set "$WORK/impact-set.txt" \
  > "$WORK/impact.log"
# The render set is the union of the two derivations, which is the whole point
# of deriving them separately: one comes from the changed modules, the other
# from the global map and reaches modules the first cannot see.
sort -u "$WORK/impact-set.txt" "$GLOBALSET" | grep . > "$RENDERSET" || : > "$RENDERSET"
T5=$(now)
ONLY=()
while read -r m; do [ -n "$m" ] && ONLY+=(--only "$m"); done < "$RENDERSET"
deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$IR" --pages "$PAGES" --source-url "$SOURCE_URL" \
  --timings "$WORK/render-timings.json" ${ONLY[@]+"${ONLY[@]}"} > "$WORK/render.log"
T6=$(now)

NPAGES=$(grep -c . "$RENDERSET" || true)
python3 - "$TIMINGS" "$T0" "$T1" "$T2" "$T3" "$T4" "$T5" "$T6" \
  "$EXTRACT_S" "$OWN_S" "$MERGE_S" "$ROUNDS" "$NSTALE_TOTAL" \
  "$NCHANGED" "$NREMOVED" "$NIRCHANGED" "$NGLOBAL" "$NPAGES" "$MODULE" "$MODE_EFF" "$L31" "$WORK" \
  "$SERVEDIR" "$SERVEFROM" <<'PY'
import json, sys, os, glob
(out, t0, t1, t2, t3, t4, t5, t6, ex, ow, mg, rounds, nstale,
 nch, nrm, nir, nglob, npg, module, mode, l31, work, servedir, servefrom) = sys.argv[1:]
t = [float(x) for x in (t0, t1, t2, t3, t4, t5, t6)]
rec = {
    "module": module, "mode": mode, "l3_1": l31,
    # Self-describing on purpose: a resident run and a fresh run are otherwise
    # indistinguishable in the record, and stage 6a's whole result is the
    # difference between them.
    "serveDir": servedir or None,
    "serveFrom": int(servefrom) if servedir else None,
    "detectSeconds": t[1] - t[0],
    "extractSeconds": float(ex),
    "ownershipSeconds": float(ow),
    "mergeSeconds": float(mg),
    "roundsSeconds": t[2] - t[1],
    "pruneSeconds": t[3] - t[2],
    "globalSeconds": t[4] - t[3],
    "impactSeconds": t[5] - t[4],
    "renderSeconds": t[6] - t[5],
    "totalSeconds": t[6] - t[0],
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
