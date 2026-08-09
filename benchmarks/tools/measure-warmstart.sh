#!/usr/bin/env bash
# Measures whether freshly *written* olean bytes stay in the page cache — i.e. whether a
# CI job that unpacks its olean cache immediately before running the doc extractor starts
# warm rather than cold.
#
# docs/approach.md §3 claims "CI (always cold)". That claim carries no label and no log.
# This driver produces the log.
#
#   sizes  sweep a copy payload across sizes that straddle this host's page-cache headroom
#          and record the residency of the *destination* right after the copy finishes.
#          Also records the residency of the *source* — a file-to-file copy asks the cache
#          for twice the payload, which CI's real path (a 411 MiB archive) does not.
#   zeros  the same sweep writing from /dev/zero: no source competes for the cache, so this
#          isolates "how much of a write survives" from "the copy source evicted it".
#   ltar   the real CI path: leantar -x of the current-rev Mathlib cache cluster into a
#          scratch root, then residency of what it wrote.
#   import the same question in seconds: run the stage-1 extractor against a Mathlib that
#          was written moments ago (fresh) / evicted (cold) / already read (warm).
#
# Every step records vm_stat before and after, because on a 16 GiB host under memory
# pressure the answer is a function of headroom, not a constant.
#
# usage:
#   measure-warmstart.sh sizes <workroot> <source-list> <outdir> <reps> <gib>...
#   measure-warmstart.sh zeros <workroot> <outdir> <reps> <gib>...
#   measure-warmstart.sh ltar  <workroot> <ltar-list> <expected-list> <outdir> <reps>
#   measure-warmstart.sh import <workroot> <outdir> {fresh|cold|warm} <ltar-list> <real-list> <name>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESIDENCY="$HERE/olean-residency"
EVICT="$HERE/olean-evict"
LEANTAR="${LEANTAR:-$HOME/.elan/toolchains/leanprover--lean4---v4.31.0/bin/leantar}"
# Abort rather than fill the disk: the largest payload here is 7.33 GiB.
MIN_FREE_GIB="${MIN_FREE_GIB:-9}"

[ -x "$RESIDENCY" ] || { echo "not built: cc -O2 -o $RESIDENCY $RESIDENCY.c" >&2; exit 1; }
[ -x "$EVICT" ] || { echo "not built: cc -O2 -o $EVICT $EVICT.c" >&2; exit 1; }

# --- helpers ----------------------------------------------------------------------------

free_gib() { df -g /private/tmp | awk 'NR==2 {print $4}'; }

check_disk() {
  local f; f="$(free_gib)"
  [ "$f" -ge "$MIN_FREE_GIB" ] || { echo "refusing to continue: only ${f} GiB free" >&2; exit 1; }
}

# One JSON object describing the current memory state. `headroom` is the memory that could
# accept new file pages without evicting anything active: free + speculative + purgeable +
# inactive. It is an upper bound — inactive anonymous pages have to be compressed first.
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
out = {
  "pagesize": ps,
  "free": g("pages_free"), "active": g("pages_active"), "inactive": g("pages_inactive"),
  "speculative": g("pages_speculative"), "wired": g("pages_wired_down"),
  "purgeable": g("pages_purgeable"), "file_backed": g("file-backed_pages"),
  "anonymous": g("anonymous_pages"), "compressor": g("pages_occupied_by_compressor"),
  "pageins": g("pageins"), "pageouts": g("pageouts"),
  "swapins": g("swapins"), "swapouts": g("swapouts"),
  "free_bytes": g("pages_free") * ps,
  "file_backed_bytes": g("file-backed_pages") * ps,
  "compressor_bytes": g("pages_occupied_by_compressor") * ps,
  "headroom_bytes": headroom * ps,
}
print(json.dumps(out))
'
}

# files / apparent bytes / bytes covered by resident pages / page-cache memory occupied
totals() {
  python3 -c '
import sys, json
f = b = rf = rb = 0
for line in open(sys.argv[1]):
    if not line.strip(): continue
    r = json.loads(line)
    f += 1; b += r["bytes"]; rf += r["resident_file_bytes"]; rb += r["resident_bytes"]
print(json.dumps({"files": f, "bytes": b, "resident_file_bytes": rf, "resident_bytes": rb}))
' "$1"
}

# Emits one event record joining the payload description, the timing, the before/after
# vm_stat and the residency totals.
emit() {
  python3 -c '
import sys, json
rec = json.loads(sys.argv[1])
for k, v in (("vm_before", sys.argv[2]), ("vm_after", sys.argv[3])):
    rec[k] = json.loads(v)
for k, v in (("dest", sys.argv[4]), ("source", sys.argv[5])):
    if v: rec[k] = json.loads(v)
print(json.dumps(rec))
' "$1" "$2" "$3" "$4" "${5:-}" >> "$EVENTS"
}

# Build the destination path list without walking the tree: tar strips the leading "/",
# so dest = <root>/<source path minus its leading slash>. Walking would cost a second of
# metadata I/O between "copy finished" and "measure", which is exactly the window at issue.
dest_list() {
  local root="$1" src="$2" out="$3"
  python3 -c '
import sys
root, src, out = sys.argv[1], sys.argv[2], sys.argv[3]
with open(out, "w") as o:
    for line in open(src):
        line = line.rstrip("\n")
        if line: o.write(root + "/" + line.lstrip("/") + "\n")
' "$root" "$src" "$out"
}

prefix_list() {
  local src="$1" want_bytes="$2" out="$3"
  python3 -c '
import os, sys
src, want, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
tot = 0
with open(out, "w") as o:
    for line in open(src):
        p = line.rstrip("\n")
        if not p: continue
        o.write(p + "\n")
        tot += os.path.getsize(p)
        if tot >= want: break
print(tot)
' "$src" "$want_bytes" "$out"
}

# --- modes ------------------------------------------------------------------------------

mode="${1:?usage: see header}"

case "$mode" in

sizes)
  WORK="${2:?}"; SRCLIST="${3:?}"; OUTDIR="${4:?}"; REPS="${5:?}"; shift 5
  mkdir -p "$OUTDIR" "$WORK"
  EVENTS="$OUTDIR/ci-warmstart-events.jsonl"
  for gib in "$@"; do
    want=$(python3 -c "print(int($gib * 1024 ** 3))")
    LIST="$WORK/list-$gib.txt"
    actual="$(prefix_list "$SRCLIST" "$want" "$LIST")"
    for rep in $(seq 1 "$REPS"); do
      DEST="$WORK/payload"
      echo "== sizes gib=$gib rep=$rep (payload $actual B) =="
      rm -rf "$DEST"
      check_disk
      # Drop the source's own residency: without this, the previous repetition's reads
      # would show up as headroom that is already spoken for.
      "$EVICT" < "$SRCLIST" 2> "$OUTDIR/ci-warmstart-evict.txt"
      mkdir -p "$DEST"
      dest_list "$DEST" "$LIST" "$WORK/destlist.txt"
      before="$(vm_json)"
      /usr/bin/time -p sh -c "tar cf - -T '$LIST' 2>/dev/null | tar xf - -C '$DEST' 2>/dev/null" \
        2> "$WORK/copy.time"
      # Nothing between the last write and mincore but a fork.
      "$RESIDENCY" < "$WORK/destlist.txt" > "$WORK/dest.jsonl" 2>/dev/null
      after="$(vm_json)"
      "$RESIDENCY" < "$SRCLIST" > "$WORK/src.jsonl" 2>/dev/null
      real="$(awk '/^real/ {print $2}' "$WORK/copy.time")"
      dest_tot="$(totals "$WORK/dest.jsonl")"
      src_tot="$(totals "$WORK/src.jsonl")"
      meta="$(printf '{"mode":"sizes","gib":%s,"rep":%s,"payload_bytes":%s,"copy_real_s":%s,"free_gib_before":%s}' \
                     "$gib" "$rep" "$actual" "$real" "$(free_gib)")"
      emit "$meta" "$before" "$after" "$dest_tot" "$src_tot"
      if [ "$gib" = "${KEEP_SNAP_GIB:-none}" ]; then
        gzip -c "$WORK/dest.jsonl" > "$OUTDIR/ci-warmstart-sizes-${gib}gib-r$rep.jsonl.gz"
      fi
      rm -rf "$DEST"
    done
  done
  ;;

zeros)
  WORK="${2:?}"; OUTDIR="${3:?}"; REPS="${4:?}"; shift 4
  mkdir -p "$OUTDIR" "$WORK"
  EVENTS="$OUTDIR/ci-warmstart-events.jsonl"
  for gib in "$@"; do
    mb=$(python3 -c "print(int($gib * 1024))")
    for rep in $(seq 1 "$REPS"); do
      DEST="$WORK/zeros"
      echo "== zeros gib=$gib rep=$rep =="
      rm -rf "$DEST"; mkdir -p "$DEST"
      check_disk
      echo "$DEST/z.bin" > "$WORK/destlist.txt"
      before="$(vm_json)"
      /usr/bin/time -p dd if=/dev/zero "of=$DEST/z.bin" bs=1m "count=$mb" 2> "$WORK/copy.time"
      "$RESIDENCY" < "$WORK/destlist.txt" > "$WORK/dest.jsonl" 2>/dev/null
      after="$(vm_json)"
      real="$(awk '/^real/ {print $2}' "$WORK/copy.time")"
      dest_tot="$(totals "$WORK/dest.jsonl")"
      meta="$(printf '{"mode":"zeros","gib":%s,"rep":%s,"copy_real_s":%s,"free_gib_before":%s}' \
                     "$gib" "$rep" "$real" "$(free_gib)")"
      emit "$meta" "$before" "$after" "$dest_tot" ""
      rm -rf "$DEST"
    done
  done
  ;;

ltar)
  WORK="${2:?}"; LTARLIST="${3:?}"; EXPECTED="${4:?}"; OUTDIR="${5:?}"; REPS="${6:?}"
  mkdir -p "$OUTDIR" "$WORK"
  EVENTS="$OUTDIR/ci-warmstart-events.jsonl"
  # leantar's `-` (read more files from stdin) expects JSON, so pass the archives as
  # argv instead. 8,516 content-addressed names are ~434 KB, well inside ARG_MAX.
  ltars=()
  while IFS= read -r p; do [ -n "$p" ] && ltars+=("$p"); done < "$LTARLIST"
  for rep in $(seq 1 "$REPS"); do
    DEST="$WORK/ltar-root"
    echo "== ltar rep=$rep =="
    rm -rf "$DEST"
    check_disk
    # The .ltar inputs are read every time; drop them so each repetition pays the same
    # decompression-input cost and does not inherit the previous run's cache.
    "$EVICT" < "$LTARLIST" 2> "$OUTDIR/ci-warmstart-evict-ltar.txt"
    mkdir -p "$DEST"
    before="$(vm_json)"
    /usr/bin/time -l "$LEANTAR" -x -f -C "$DEST" "${ltars[@]}" > "$WORK/leantar.out" 2> "$WORK/leantar.time"
    "$RESIDENCY" < "$EXPECTED" > "$WORK/dest.jsonl" 2> "$WORK/dest.stderr"
    after="$(vm_json)"
    real="$(awk '/real/ {print $1}' "$WORK/leantar.time" | head -1)"
    cp "$WORK/leantar.time" "$OUTDIR/ci-warmstart-ltar-r$rep.time"
    gzip -c "$WORK/dest.jsonl" > "$OUTDIR/ci-warmstart-ltar-r$rep.jsonl.gz"
    dest_tot="$(totals "$WORK/dest.jsonl")"
    meta="$(printf '{"mode":"ltar","rep":%s,"extract_real_s":%s,"free_gib_before":%s}' \
                   "$rep" "$real" "$(free_gib)")"
    emit "$meta" "$before" "$after" "$dest_tot" ""
    [ "${KEEP_LTAR:-0}" = "1" ] && [ "$rep" = "$REPS" ] || rm -rf "$DEST"
  done
  ;;

import)
  # The question A and B answer in percent, answered in seconds: how long does
  # importModules take when Mathlib's olean was written moments ago?
  #
  # Mathlib is served out of the leantar scratch root by swapping that one entry of
  # LEAN_PATH; every other olean in the closure (Lean core, the project, the 14 other
  # packages) is evicted, so the only thing that varies between variants is Mathlib.
  #   fresh  extract runs immediately after leantar wrote Mathlib
  #   cold   same tree, but Mathlib evicted first — the reference cold
  #   warm   run again with nothing evicted — the reference warm
  WORK="${2:?}"; OUTDIR="${3:?}"; VARIANT="${4:?}"; LTARLIST="${5:?}"; REALLIST="${6:?}"; NAME="${7:?}"
  DEST="$WORK/ltar-root"
  SCRATCH_LIB="$DEST/.lake/build/lib/lean"
  TARGET_REPO="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
  BIN="$HERE/../../experiments/stage1/build/extract"
  MODULES="$OUTDIR/it-modules.txt"
  EVENTS="$OUTDIR/ci-warmstart-events.jsonl"
  LP="$(cd "$TARGET_REPO" && "$HOME/.elan/bin/lake" env printenv LEAN_PATH 2>/dev/null \
        | tr ':' '\n' \
        | sed "s|^$TARGET_REPO/\.lake/packages/mathlib/\.lake/build/lib/lean\$|$SCRATCH_LIB|" \
        | paste -sd: -)"
  case "$VARIANT" in
    fresh)
      rm -rf "$DEST"; check_disk
      ltars=()
      while IFS= read -r p; do [ -n "$p" ] && ltars+=("$p"); done < "$LTARLIST"
      "$EVICT" < "$LTARLIST" 2> "$OUTDIR/ci-warmstart-evict-ltar.txt"
      "$EVICT" < "$REALLIST" 2> "$OUTDIR/ci-warmstart-evict-real.txt"
      mkdir -p "$DEST"
      /usr/bin/time -l "$LEANTAR" -x -f -C "$DEST" "${ltars[@]}" > /dev/null 2> "$WORK/leantar.time"
      cp "$WORK/leantar.time" "$OUTDIR/ci-warmstart-$NAME-leantar.time"
      ;;
    cold)
      find "$SCRATCH_LIB" -type f \( -name '*.olean' -o -name '*.olean.server' \
        -o -name '*.olean.private' \) | LC_ALL=C sort > "$WORK/scratch-olean.txt"
      "$EVICT" < "$WORK/scratch-olean.txt" 2> "$OUTDIR/ci-warmstart-evict-scratch.txt"
      "$EVICT" < "$REALLIST" 2> "$OUTDIR/ci-warmstart-evict-real.txt"
      ;;
    warm) ;;
    *) echo "unknown variant: $VARIANT" >&2; exit 1 ;;
  esac
  before="$(vm_json)"
  ( cd "$TARGET_REPO" && "$HOME/.elan/bin/lake" env env "LEAN_PATH=$LP" \
      /usr/bin/time -l "$BIN" "$MODULES" "$OUTDIR/ci-warmstart-$NAME.jsonl" ) \
    > "$OUTDIR/ci-warmstart-$NAME-summary.txt" 2>&1
  after="$(vm_json)"
  real="$(awk '/ real / {print $1}' "$OUTDIR/ci-warmstart-$NAME-summary.txt" | head -1)"
  meta="$(printf '{"mode":"import","variant":"%s","name":"%s","extract_real_s":%s,"free_gib_before":%s}' \
                 "$VARIANT" "$NAME" "${real:-0}" "$(free_gib)")"
  emit "$meta" "$before" "$after" '{"files":0,"bytes":0,"resident_file_bytes":0,"resident_bytes":0}' ""
  grep -E 'importModules|real|page faults' "$OUTDIR/ci-warmstart-$NAME-summary.txt" | head -5
  ;;

*) echo "unknown mode: $mode" >&2; exit 1 ;;
esac

echo "events -> $EVENTS"
