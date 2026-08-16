#!/usr/bin/env bash
# Measures play 2 of docs/plans/three-axes.md §5: can reading the olean ahead of the
# extractor buy back the cold start?
#
# The cold `importModules` costs 13.9-15.7 s, of which ~12.7 s is 132,343 synchronous
# major faults (benchmarks/results/ci-residency-summary.txt). The same bytes read with
# read(2) across 8 threads move at ~1.8 GB/s, so on paper the whole 2.59 GB could be in
# cache in ~2 s. This driver tests whether that paper figure survives contact with (a)
# the page cache this host will actually give us and (b) the extractor competing for the
# same disk.
#
#   prefetch  evict everything, prefetch one file set, and measure how much of the set the
#             *extractor will need* is resident afterwards. This is the honesty check: a
#             prefetcher that finishes fast because the cache dropped its bytes is worse
#             than useless, and only mincore can tell the two apart.
#   baseline  evict everything, run the extractor. The cold reference for this session.
#   sequential evict, prefetch to completion, then run the extractor. Upper bound on the
#             effect and a wall clock that must be paid in full (prefetch + extract).
#   overlapped evict, start the prefetcher in the background, start the extractor at once.
#             This is the actual play. It can lose to `sequential`: demand faults and the
#             prefetcher contend for the same queue.
#
# Every variant evicts the *same* full olean list, so the cold state is identical no matter
# which subset is prefetched, and each cold state is verifiable rather than assumed.
#
# usage:
#   measure-prefetch.sh prefetch   <outdir> <full-list> <need-list> <set-list> <jobs> <pmode> <reps> <tag>
#   measure-prefetch.sh baseline   <outdir> <full-list> <reps> <tag>
#   measure-prefetch.sh warm       <outdir> <reps> <tag>
#   measure-prefetch.sh sequential <outdir> <full-list> <set-list> <jobs> <pmode> <reps> <tag>
#   measure-prefetch.sh overlapped <outdir> <full-list> <set-list> <jobs> <pmode> <reps> <tag>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESIDENCY="$HERE/olean-residency"
EVICT="$HERE/olean-evict"
PREFETCH="$HERE/olean-prefetch"
TARGET_REPO="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
# Which extractor to time. stage1 answers "how long is the cold import", stage4b answers
# "how long is the whole first generation" because it also runs the semantic analysis and
# writes the IR. EXTRACT_ARGS goes to the binary after <modules> <out.jsonl>.
#
# EXTRACT_BIN HAS NO DEFAULT ANY MORE
#   It used to default to experiments/stage1/build/extract, and that default is what
#   produced every committed ci-prefetch-* number. `experiments/` was removed on
#   2026-08-16; stage 1 only exists at tag `experiments-frozen` and **HEAD has no
#   equivalent** (extractor/ is the schema-4 extractor, which does strictly more work
#   after the import). A run driven by any other binary is a NEW baseline, not a
#   continuation of the recorded one — do not put the two in one table.
BIN="${EXTRACT_BIN:-}"
[ -n "$BIN" ] || { echo "EXTRACT_BIN is unset: name the extractor to time." >&2
  echo "The recorded ci-prefetch-* numbers used experiments/stage1/build/extract," >&2
  echo "which only exists at tag experiments-frozen; HEAD has no equivalent." >&2; exit 2; }
read -r -a EXTRACT_ARGS_ARR <<< "${EXTRACT_ARGS:-}"
MODULES="${MODULES:-$HERE/../results/it-modules.txt}"
# Scratch for per-run extractor output; only the aggregated transcript and the event log
# land in results/.
WORK="${WORK:-${TMPDIR:-/tmp}/ci-prefetch-work}"
mkdir -p "$WORK"

for t in "$RESIDENCY" "$EVICT" "$PREFETCH"; do
  [ -x "$t" ] || { echo "not built: cc -O2 -o $t $t.c" >&2; exit 1; }
done
[ -x "$BIN" ] || { echo "not executable: EXTRACT_BIN=$BIN" >&2; exit 1; }

# --- helpers ----------------------------------------------------------------------------

# Same shape as measure-warmstart.sh: headroom = free + speculative + purgeable + inactive,
# an upper bound on what could accept new file pages.
vm_json() {
  vm_stat | python3 -c '
import sys, json, re
d = {}
ps = 16384
for line in sys.stdin:
    m = re.match(r"Mach Virtual Memory Statistics: \(page size of (\d+) bytes\)", line)
    if m:
        ps = int(m.group(1)); continue
    m = re.match(r"(.+?):\s+(\d+)\.?\s*$", line)
    if m:
        d[m.group(1).strip().strip("\"").replace(" ", "_").lower()] = int(m.group(2))
g = lambda k: d.get(k, 0)
headroom = g("pages_free") + g("pages_speculative") + g("pages_purgeable") + g("pages_inactive")
print(json.dumps({
  "pagesize": ps, "free": g("pages_free"), "inactive": g("pages_inactive"),
  "speculative": g("pages_speculative"), "wired": g("pages_wired_down"),
  "file_backed": g("file-backed_pages"), "anonymous": g("anonymous_pages"),
  "compressor": g("pages_occupied_by_compressor"), "pageins": g("pageins"),
  "free_bytes": g("pages_free") * ps,
  "file_backed_bytes": g("file-backed_pages") * ps,
  "compressor_bytes": g("pages_occupied_by_compressor") * ps,
  "headroom_bytes": headroom * ps,
}))
'
}

totals() {
  python3 -c '
import sys, json
f = b = rf = rb = tf = 0
for line in open(sys.argv[1]):
    if not line.strip(): continue
    r = json.loads(line)
    f += 1; b += r["bytes"]; rf += r["resident_file_bytes"]; rb += r["resident_bytes"]
    if r["resident_pages"] > 0: tf += 1
print(json.dumps({"files": f, "bytes": b, "resident_file_bytes": rf,
                  "resident_bytes": rb, "touched_files": tf}))
' "$1"
}

# Pull the numbers the extractor and /usr/bin/time -l printed into one JSON object.
extract_json() {
  python3 -c '
import sys, json, re
txt = open(sys.argv[1], errors="replace").read()
def f(pat, cast=float):
    m = re.search(pat, txt)
    return cast(m.group(1)) if m else None
print(json.dumps({
  "import_s": f(r"importModules\s+([0-9.]+)s"),
  "total_s": f(r"(?m)^total\s+([0-9.]+)s"),
  "real_s": f(r"([0-9.]+) real"),
  "user_s": f(r"([0-9.]+) user"),
  "sys_s": f(r"([0-9.]+) sys"),
  "minor_faults": f(r"(\d+)\s+page reclaims", int),
  "major_faults": f(r"(\d+)\s+page faults", int),
  "peak_rss": f(r"(\d+)\s+maximum resident set size", int),
  "footprint": f(r"(\d+)\s+peak memory footprint", int),
  "agree": ("declaration sets agree" in txt),
  "decls": f(r"agree \((\d+) declarations\)", int),
  # stage4b does not run the cross-check, so its identity of work is the analyze line.
  "produced": f(r"produced (\d+)", int),
}))
' "$1"
}

emit() { # meta vm_before vm_after extra
  python3 -c '
import sys, json
rec = json.loads(sys.argv[1])
rec["vm_before"] = json.loads(sys.argv[2])
rec["vm_after"] = json.loads(sys.argv[3])
if sys.argv[4]: rec.update(json.loads(sys.argv[4]))
print(json.dumps(rec))
' "$1" "$2" "$3" "${4:-}" >> "$EVENTS"
}

# The extractor's own stdout + `/usr/bin/time -l` for one run. There are ~40 runs in a
# full sweep, so the per-run text is appended to one transcript instead of littering
# results/ with 80 files; the machine-readable copy is the event record.
SUMMARY=""
run_extract() { # name
  local name="$1"
  SUMMARY="$WORK/$name-summary.txt"
  ( cd "$TARGET_REPO" && "$LAKE" env /usr/bin/time -l "$BIN" "$MODULES" \
      "$WORK/$name.jsonl" ${EXTRACT_ARGS_ARR[@]+"${EXTRACT_ARGS_ARR[@]}"} ) > "$SUMMARY" 2>&1
  { echo "=== $name ==="; cat "$SUMMARY"; echo; } >> "$OUTDIR/ci-prefetch-runs.txt"
}

# NB: python's time.monotonic() is *per process* on macOS (it starts near zero in every
# interpreter), so it cannot time across forks. CLOCK_MONOTONIC_RAW is since boot.
now_s() { python3 -c 'import time; print("%.3f" % time.clock_gettime(time.CLOCK_MONOTONIC_RAW))'; }

# --- modes ------------------------------------------------------------------------------

mode="${1:?usage: see header}"

case "$mode" in

prefetch)
  OUTDIR="${2:?}"; FULL="${3:?}"; NEED="${4:?}"; SET="${5:?}"; JOBS="${6:?}"; PMODE="${7:?}"
  REPS="${8:?}"; TAG="${9:?}"
  mkdir -p "$OUTDIR"; EVENTS="$OUTDIR/ci-prefetch-events.jsonl"
  for rep in $(seq 1 "$REPS"); do
    echo "== prefetch $TAG j=$JOBS m=$PMODE rep=$rep =="
    "$EVICT" < "$FULL" 2> "$OUTDIR/ci-prefetch-evict.txt"
    before="$(vm_json)"
    "$PREFETCH" -j "$JOBS" -m "$PMODE" < "$SET" 2> "$OUTDIR/.pf.json"
    # Straight into mincore: the question is what survived the prefetch, and every second
    # spent here is a second in which more of it can be dropped.
    "$RESIDENCY" < "$NEED" > "$OUTDIR/.need.jsonl" 2>/dev/null
    after="$(vm_json)"
    need_tot="$(totals "$OUTDIR/.need.jsonl")"
    # Keep the per-file snapshot: the aggregate cannot say *which* of the needed bytes
    # survived, and that is the difference between "prefetched" and "prefetched usefully".
    gzip -c "$OUTDIR/.need.jsonl" > "$OUTDIR/ci-prefetch-need-$TAG-j$JOBS-$PMODE-r$rep.jsonl.gz"
    pf="$(cat "$OUTDIR/.pf.json")"
    meta="$(printf '{"mode":"prefetch","tag":"%s","jobs":%s,"pmode":"%s","rep":%s}' \
                   "$TAG" "$JOBS" "$PMODE" "$rep")"
    emit "$meta" "$before" "$after" \
         "$(python3 -c 'import sys,json;print(json.dumps({"prefetch":json.loads(sys.argv[1]),"need":json.loads(sys.argv[2])}))' "$pf" "$need_tot")"
    python3 -c 'import sys,json;p=json.loads(sys.argv[1]);n=json.loads(sys.argv[2]);print("  %.3fs %.0f MB/s  need resident %.1f%% (%d B of %d)"%(p["elapsed_s"],p["mb_per_s"],100*n["resident_file_bytes"]/n["bytes"],n["resident_file_bytes"],n["bytes"]))' "$pf" "$need_tot"
  done
  ;;

baseline)
  OUTDIR="${2:?}"; FULL="${3:?}"; REPS="${4:?}"; TAG="${5:?}"
  mkdir -p "$OUTDIR"; EVENTS="$OUTDIR/ci-prefetch-events.jsonl"
  for rep in $(seq 1 "$REPS"); do
    name="$TAG-r$rep"
    echo "== baseline rep=$rep =="
    "$EVICT" < "$FULL" 2> "$OUTDIR/ci-prefetch-evict.txt"
    before="$(vm_json)"
    t0="$(now_s)"
    run_extract "$name"
    t1="$(now_s)"
    after="$(vm_json)"
    meta="$(printf '{"mode":"baseline","tag":"%s","rep":%s,"name":"%s","wall_s":%s}' \
                   "$TAG" "$rep" "$name" "$(python3 -c "print(round($t1-$t0,3))")")"
    emit "$meta" "$before" "$after" \
         "$(python3 -c 'import sys,json;print(json.dumps({"extract":json.loads(sys.argv[1])}))' "$(extract_json "$SUMMARY")")"
    grep -E 'importModules|real ' "$SUMMARY" | sed 's/^/  /'
  done
  ;;

sequential)
  OUTDIR="${2:?}"; FULL="${3:?}"; SET="${4:?}"; JOBS="${5:?}"; PMODE="${6:?}"; REPS="${7:?}"; TAG="${8:?}"
  mkdir -p "$OUTDIR"; EVENTS="$OUTDIR/ci-prefetch-events.jsonl"
  for rep in $(seq 1 "$REPS"); do
    name="$TAG-r$rep"
    echo "== sequential $TAG j=$JOBS m=$PMODE rep=$rep =="
    "$EVICT" < "$FULL" 2> "$OUTDIR/ci-prefetch-evict.txt"
    before="$(vm_json)"
    t0="$(now_s)"
    "$PREFETCH" -j "$JOBS" -m "$PMODE" < "$SET" 2> "$OUTDIR/.pf.json"
    t1="$(now_s)"
    run_extract "$name"
    t2="$(now_s)"
    after="$(vm_json)"
    meta="$(printf '{"mode":"sequential","tag":"%s","jobs":%s,"pmode":"%s","rep":%s,"name":"%s","prefetch_wall_s":%s,"extract_wall_s":%s,"wall_s":%s}' \
                   "$TAG" "$JOBS" "$PMODE" "$rep" "$name" \
                   "$(python3 -c "print(round($t1-$t0,3))")" \
                   "$(python3 -c "print(round($t2-$t1,3))")" \
                   "$(python3 -c "print(round($t2-$t0,3))")")"
    emit "$meta" "$before" "$after" \
         "$(python3 -c 'import sys,json;print(json.dumps({"extract":json.loads(sys.argv[1]),"prefetch":json.loads(sys.argv[2])}))' \
             "$(extract_json "$SUMMARY")" "$(cat "$OUTDIR/.pf.json")")"
    grep -E 'importModules|real ' "$SUMMARY" | sed 's/^/  /'
  done
  ;;

overlapped)
  OUTDIR="${2:?}"; FULL="${3:?}"; SET="${4:?}"; JOBS="${5:?}"; PMODE="${6:?}"; REPS="${7:?}"; TAG="${8:?}"
  mkdir -p "$OUTDIR"; EVENTS="$OUTDIR/ci-prefetch-events.jsonl"
  for rep in $(seq 1 "$REPS"); do
    name="$TAG-r$rep"
    echo "== overlapped $TAG j=$JOBS m=$PMODE rep=$rep =="
    "$EVICT" < "$FULL" 2> "$OUTDIR/ci-prefetch-evict.txt"
    before="$(vm_json)"
    t0="$(now_s)"
    # Background, unwaited: the extractor must not block on it. A real implementation
    # would run these as threads of the extractor process; the head start here is the
    # few ms until the shell forks, which is the same order.
    ( "$PREFETCH" -j "$JOBS" -m "$PMODE" < "$SET" 2> "$OUTDIR/.pf.json"; \
      python3 -c 'import time; print("%.3f" % time.clock_gettime(time.CLOCK_MONOTONIC_RAW))' > "$OUTDIR/.pf.end" ) &
    pf_pid=$!
    run_extract "$name"
    t1="$(now_s)"
    wait "$pf_pid"
    t2="$(now_s)"
    after="$(vm_json)"
    pf_end="$(cat "$OUTDIR/.pf.end")"
    meta="$(printf '{"mode":"overlapped","tag":"%s","jobs":%s,"pmode":"%s","rep":%s,"name":"%s","extract_wall_s":%s,"prefetch_end_rel_s":%s,"wall_s":%s}' \
                   "$TAG" "$JOBS" "$PMODE" "$rep" "$name" \
                   "$(python3 -c "print(round($t1-$t0,3))")" \
                   "$(python3 -c "print(round($pf_end-$t0,3))")" \
                   "$(python3 -c "print(round($t2-$t0,3))")")"
    emit "$meta" "$before" "$after" \
         "$(python3 -c 'import sys,json;print(json.dumps({"extract":json.loads(sys.argv[1]),"prefetch":json.loads(sys.argv[2])}))' \
             "$(extract_json "$SUMMARY")" "$(cat "$OUTDIR/.pf.json")")"
    grep -E 'importModules|real ' "$SUMMARY" | sed 's/^/  /'
  done
  ;;

warm)
  # No eviction at all: back-to-back runs that inherit the previous run's page cache.
  # Needed in the *same session* as the cold variants, because the cold total is a
  # delta against warm and CLAUDE.md forbids pairing a cold number with a warm number
  # taken under a different memory state.
  OUTDIR="${2:?}"; REPS="${3:?}"; TAG="${4:?}"
  mkdir -p "$OUTDIR"; EVENTS="$OUTDIR/ci-prefetch-events.jsonl"
  for rep in $(seq 1 "$REPS"); do
    name="$TAG-r$rep"
    echo "== warm rep=$rep =="
    before="$(vm_json)"
    t0="$(now_s)"
    run_extract "$name"
    t1="$(now_s)"
    after="$(vm_json)"
    meta="$(printf '{"mode":"warm","tag":"%s","rep":%s,"name":"%s","wall_s":%s}' \
                   "$TAG" "$rep" "$name" "$(python3 -c "print(round($t1-$t0,3))")")"
    emit "$meta" "$before" "$after" \
         "$(python3 -c 'import sys,json;print(json.dumps({"extract":json.loads(sys.argv[1])}))' "$(extract_json "$SUMMARY")")"
    grep -E 'importModules|real ' "$SUMMARY" | sed 's/^/  /'
  done
  ;;

*) echo "unknown mode: $mode" >&2; exit 1 ;;
esac

echo "events -> $EVENTS"
