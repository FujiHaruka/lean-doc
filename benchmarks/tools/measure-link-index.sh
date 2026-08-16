#!/usr/bin/env bash
# Measures the extractor's `--link-index` phase alone: how long writing the
# dependency closure's `.lidx` takes, and how large the file is.
#
# `--skip-analyze` is deliberate. The phase runs right after `importModules` and
# before any declaration is analysed (`Extract.lean`'s main), so skipping the
# analysis leaves the *same* environment loaded and the *same* work in the
# phase being timed, at a fraction of the run time. What this measures is
# therefore the phase, not a full extraction.
#
# Cold means the closure's oleans have been evicted from the page cache
# (`olean-evict`, msync(MS_INVALIDATE), no sudo) — this workload reads olean
# through mmap, so the phase's own cost depends on whether `declRangeExt`'s
# pages are resident (CLAUDE.md「ベンチマーク」).
#
# usage: EXTRACT_BIN=<binary> measure-link-index.sh <label> [rounds]
# out:   benchmarks/results/m7a-linkindex-<label>.jsonl (one object per run)
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./env.sh
source "$HERE/env.sh"

LABEL="${1:?usage: measure-link-index.sh <label> [rounds]}"
ROUNDS="${2:-5}"
WORK="${WORK_DIR:?WORK_DIR is required: where the .lidx and the raw logs go}"
MODULES="${MODULES:?MODULES is required: the module list to import}"
EXTRACT_BIN="${EXTRACT_BIN:-$LEAN_DOC_ROOT/extractor/build/extract}"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
OUT="$RESULTS_DIR/m7a-linkindex-$LABEL.jsonl"

for p in "$EXTRACT_BIN" "$MODULES" "$HERE/olean-evict"; do
  [ -e "$p" ] || { echo "missing: $p" >&2; exit 1; }
done
mkdir -p "$WORK/raw"

# The eviction set: every olean of the dependency closure, since the phase reads
# one declaration range per constant out of every loaded module.
OLEANS="$WORK/closure-oleans.txt"
( cd "$TARGET_REPO" && find .lake/build/lib/lean .lake/packages/*/.lake/build/lib/lean \
    -name '*.olean' 2>/dev/null | sed "s|^|$TARGET_REPO/|" ) > "$OLEANS"
OLEAN_FILES=$(grep -c . "$OLEANS")
echo "eviction set: $OLEAN_FILES oleans"

# The conditions, next to the numbers (CLAUDE.md「計測条件を毎回記録する」).
{
  echo "date (UTC)          : $(date -u '+%Y-%m-%d %H:%M:%S')"
  echo "host                : $(uname -sr) $(uname -m), $(sysctl -n hw.ncpu) CPU, \
$(( $(sysctl -n hw.memsize) / 1073741824 )) GB RAM"
  echo "target              : $TARGET_REPO"
  echo "target toolchain    : $(cat "$TARGET_REPO/lean-toolchain" 2>/dev/null)"
  echo "modules imported    : $(grep -c . "$MODULES") (the target's own library)"
  echo "extractor           : $EXTRACT_BIN ($(stat -f '%Sm' "$EXTRACT_BIN"))"
  echo "command             : lake env extract <modules> <events> --skip-analyze --link-index <p>"
  echo "parallelism         : 1 (--skip-analyze; --jobs is not used)"
  echo "rounds              : $ROUNDS warm then $ROUNDS cold, after one discarded run"
  echo "cold                : olean-evict over $OLEAN_FILES closure oleans before every run"
  echo "warm                : consecutive runs, no eviction"
} > "$RESULTS_DIR/m7a-linkindex-$LABEL-env.txt"
cat "$RESULTS_DIR/m7a-linkindex-$LABEL-env.txt"

one_run () { # one_run <series> <index>
  local s="$1" i="$2"
  local ev="$WORK/raw/events-$LABEL-$s-$i.jsonl"
  local tl="$WORK/raw/time-$LABEL-$s-$i.txt"
  local lidx="$WORK/raw/link-index-$LABEL-$s-$i.lidx"
  rm -f "$ev" "$lidx"
  case "$s" in
    cold) "$HERE/olean-evict" < "$OLEANS" > "$WORK/raw/evict-$LABEL-$s-$i.txt" 2>&1 ;;
  esac
  ( cd "$TARGET_REPO" && /usr/bin/time -l "$LAKE" env "$EXTRACT_BIN" \
      "$MODULES" "$ev" --skip-analyze --link-index "$lidx" ) \
    > "$WORK/raw/stdout-$LABEL-$s-$i.txt" 2> "$tl"
  python3 - "$LABEL" "$s" "$i" "$ev" "$tl" "$lidx" >> "$OUT" <<'PY'
import json, os, re, sys
label, series, run, events, timel, lidx = sys.argv[1:7]
rec = {"label": label, "series": series, "run": int(run)}
for line in open(events):
    line = line.strip()
    if not line:
        continue
    e = json.loads(line)
    phase = e.get("phase", "").replace("stage4b.", "")
    if phase in ("linkIndex", "importModules"):
        rec[phase + "Us"] = e.get("us")
        for k, v in e.items():
            if k not in ("phase", "pid", "us"):
                rec[phase + ":" + k] = v
text = open(timel).read()
m = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", text)
if m:
    rec["realS"], rec["userS"], rec["sysS"] = (float(g) for g in m.groups())
m = re.search(r"(\d+)\s+maximum resident set size", text)
if m:
    rec["peakRssB"] = int(m.group(1))
rec["lidxBytes"] = os.path.getsize(lidx) if os.path.exists(lidx) else None
print(json.dumps(rec))
PY
  echo "  $s $i: $(tail -1 "$OUT")"
}

: > "$OUT"
# One discarded run first: the cache state at the start of the series is
# whatever the machine was doing before it, so calling it "warm" would be a
# guess. It is recorded under its own series rather than dropped.
one_run warmup 0
for i in $(seq 1 "$ROUNDS"); do one_run warm "$i"; done
for i in $(seq 1 "$ROUNDS"); do one_run cold "$i"; done
echo "out: $OUT"
