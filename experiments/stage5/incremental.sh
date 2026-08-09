#!/usr/bin/env bash
# One end-to-end incremental generation: "module M changed" in, an updated IR
# and updated HTML pages out.
#
# The four stages are the ones `approach.md` §5.5 splits the problem into, and
# they are timed separately inside one wall clock so that the total is a
# measurement rather than a sum of medians taken at different times:
#
#   1 detect   hash ledger over the 432 oleans -> the changed module set
#   2 extract  the stage-4b extractor over exactly that set (Lean runs here)
#   3 merge    fold the partial IR back in; the IR content hash decides which
#              pages are stale
#   4 render   stage4c's render.ts over the page set
#
# THE ONE THING THAT IS FAKED, AND EXACTLY HOW MUCH
#   The measurement target must not be modified, so no `.lean` file is edited
#   and no `lake build` is run. "M changed" is injected by invalidating M's
#   ledger entry (`ledger.ts touch`). Everything else is real work on real
#   inputs: the extractor really re-reads M's import closure and re-analyses M,
#   the merge really rewrites the IR, the renderer really rewrites the pages.
#   What the numbers therefore exclude is `lake build` itself, which is outside
#   lean-doc but on the critical path of a real edit-to-preview loop.
#
#   A consequence worth stating rather than hiding: because nothing actually
#   changed, stage 3 finds the re-extracted IR byte-identical and reports zero
#   stale pages. That is the correct answer and it is checked (`irChanged`), but
#   it would make stage 4 free, so `--mode` forces a page set instead:
#     self       the re-extracted modules (what a change that only alters M's
#                own page would produce)
#     referrers  self + the modules whose printed text names something of M's
#     importers  self + everything that transitively imports M (the sound bound)
#
# usage:
#   incremental.sh --module M --ir <live ir> --pages <live pages> --ledger <file>
#                  --work <dir> --mode self|referrers|importers [--timings <p>]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
SOURCE_URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

MODULE=""; IR=""; PAGES=""; LEDGER=""; WORK=""; MODE=self; TIMINGS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --module) MODULE="$2"; shift 2 ;;
    --ir) IR="$2"; shift 2 ;;
    --pages) PAGES="$2"; shift 2 ;;
    --ledger) LEDGER="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --mode) MODE="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODULE" ] && [ -n "$IR" ] && [ -n "$PAGES" ] && [ -n "$LEDGER" ] && [ -n "$WORK" ] || {
  echo "usage: incremental.sh --module M --ir <dir> --pages <dir> --ledger <f> --work <dir>" >&2
  exit 2; }
mkdir -p "$WORK"
CHANGED="$WORK/changed.txt"
IRCHANGED="$WORK/ir-changed.txt"
RENDERSET="$WORK/render-set.txt"
INCIR="$WORK/inc-ir"
rm -rf "$INCIR"

# `time.monotonic()` is per-process on this platform (Python 3.9 / macOS returns
# time since interpreter start), so it cannot be sampled from separate
# processes. `time.time()` is the wall clock and is comparable across them; the
# stages here are 0.05 s and up, well above its resolution and above the ~0.03 s
# it costs to sample. The outer `/usr/bin/time -l` wall clock is the authority
# on the total; these stamps only split it.
now () { python3 -c 'import time; print(repr(time.time()))'; }
T0=$(now)

# 1 -- detect ---------------------------------------------------------------
deno run --allow-read --allow-write --allow-env "$HERE/ledger.ts" check \
  --ledger "$LEDGER" --changed-out "$CHANGED" > "$WORK/detect.log"
T1=$(now)
NCHANGED=$(grep -c . "$CHANGED" || true)

# 2 -- extract --------------------------------------------------------------
if [ "$NCHANGED" -gt 0 ]; then
  "$HERE/extract-once.sh" --modules "$CHANGED" --ir-dir "$INCIR" \
    --timings "$WORK/extract-timings.json" --events "$WORK/extract-events.jsonl"
fi
T2=$(now)

# 3 -- merge ----------------------------------------------------------------
if [ "$NCHANGED" -gt 0 ]; then
  deno run --allow-read --allow-write "$HERE/merge-ir.ts" --base "$IR" --inc "$INCIR" \
    --out "$IR" --changed-out "$IRCHANGED" --timings "$WORK/merge-timings.json" \
    > "$WORK/merge.log"
else
  : > "$IRCHANGED"
fi
T3=$(now)
NIRCHANGED=$(grep -c . "$IRCHANGED" || true)

# 4 -- render ---------------------------------------------------------------
# `--mode` decides the page set (see the header): the injected change leaves the
# IR byte-identical, so the honest render set is empty and has to be forced.
deno run --allow-read --allow-write "$HERE/impact.ts" --ir "$IR" \
  --changed-file "$CHANGED" --mode "$MODE" --print-set "$RENDERSET" > "$WORK/impact.log"
T4=$(now)
ONLY=()
while read -r m; do [ -n "$m" ] && ONLY+=(--only "$m"); done < "$RENDERSET"
deno run --allow-read --allow-write "$LD/experiments/stage4c/render.ts" \
  --ir "$IR" --pages "$PAGES" --source-url "$SOURCE_URL" \
  --timings "$WORK/render-timings.json" ${ONLY[@]+"${ONLY[@]}"} > "$WORK/render.log"
T5=$(now)

NPAGES=$(grep -c . "$RENDERSET" || true)
python3 - "$TIMINGS" "$T0" "$T1" "$T2" "$T3" "$T4" "$T5" \
  "$NCHANGED" "$NIRCHANGED" "$NPAGES" "$MODULE" "$MODE" "$WORK" <<'PY'
import json, sys, os
out, t0, t1, t2, t3, t4, t5, nch, nir, npg, module, mode, work = sys.argv[1:]
t = [float(x) for x in (t0, t1, t2, t3, t4, t5)]
rec = {
    "module": module, "mode": mode,
    "detectSeconds": t[1] - t[0],
    "extractSeconds": t[2] - t[1],
    "mergeSeconds": t[3] - t[2],
    "impactSeconds": t[4] - t[3],
    "renderSeconds": t[5] - t[4],
    "totalSeconds": t[5] - t[0],
    "changed": int(nch), "irChanged": int(nir), "pagesRendered": int(npg),
}
for name, path in (("extract", "extract-timings.json"), ("merge", "merge-timings.json"),
                   ("render", "render-timings.json")):
    p = os.path.join(work, path)
    if os.path.exists(p):
        try:
            rec[name] = json.load(open(p, encoding="utf-8"))
        except Exception:
            pass
line = json.dumps(rec)
print(line)
if out:
    open(out, "w", encoding="utf-8").write(line + "\n")
PY
