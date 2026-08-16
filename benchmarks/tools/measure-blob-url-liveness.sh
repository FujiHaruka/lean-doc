#!/bin/bash
# Check that version-pinned GitHub blob URLs actually resolve (M7-d).
#
# Counterpart to `measure-docsite-drift.sh`: that one measures how much of the
# *public doc site* has drifted away from the version this project is pinned to;
# this one measures whether the blob URLs M7 emits instead are live. The two
# share a rule — the product never does this at build time. Liveness is measured
# here, offline the rest of the time.
#
# Input is a file of URLs, one per line (e.g. cut out of a generated site, or out
# of the reference tree's oracle). A deterministic every-Nth sample is taken so a
# rerun checks the same URLs.
#
# Usage: benchmarks/tools/measure-blob-url-liveness.sh <urls.txt> [out-prefix]
set -u

URLS=${1:?usage: measure-blob-url-liveness.sh <urls.txt> [out-prefix]}
OUTDIR=${OUTDIR:-benchmarks/results}
STAMP=$(date -u +%Y-%m-%d)
PREFIX=${2:-$OUTDIR/m7d-blob-liveness-$STAMP}
STRIDE=${STRIDE:-500}
JOBS=${JOBS:-6}

[ -f "$URLS" ] || { echo "no url list at $URLS" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# One URL per distinct file is enough for existence: the line anchor is a
# fragment, which the server never sees. Anchors are checked separately, below.
sed -E 's/#L[0-9]+(-L[0-9]+)?$//' "$URLS" | sort -u > "$TMP/files.txt"
awk -v s="$STRIDE" '(NR - 1) % s == 0' "$TMP/files.txt" > "$TMP/sample.txt"
# An empty sample reads exactly like a clean run — no non-200 lines, no SHORT
# lines — so it has to stop here rather than be reported as a pass.
[ -s "$TMP/sample.txt" ] || { echo "empty sample: $(wc -l < "$TMP/files.txt") file(s), stride $STRIDE" >&2; exit 1; }

{
  echo "date (UTC)   : $(date -u +'%Y-%m-%d %H:%M:%S')"
  echo "url list     : $URLS"
  echo "urls in list : $(wc -l < "$URLS" | tr -d ' ')"
  echo "distinct files: $(wc -l < "$TMP/files.txt" | tr -d ' ')"
  echo "sampled      : $(wc -l < "$TMP/sample.txt" | tr -d ' ') (every ${STRIDE}th)"
  echo "parallelism  : $JOBS"
  echo "note         : the fragment (#L..) is never sent to the server; file"
  echo "               existence is what a status code can prove."
} > "$PREFIX-env.txt"
cat "$PREFIX-env.txt"

check() {
  code=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 30 -L "$1")
  echo "$code $1"
}
export -f check
xargs -P "$JOBS" -I{} bash -c 'check "$@"' _ {} < "$TMP/sample.txt" | sort > "$PREFIX-files.txt"

echo "=== status distribution ==="
awk '{print $1}' "$PREFIX-files.txt" | sort | uniq -c
echo "=== non-200 ==="
grep -v '^200' "$PREFIX-files.txt" | head -40

# A 200 proves the file is there, not that the anchor points inside it. Because
# the rev is pinned the file cannot move under the anchor — but "cannot" is a
# claim about construction, so check it: fetch the raw file at that same rev and
# assert it has at least as many lines as the anchor asks for.
#
# Fetched per *file*, not per URL: 738 anchors over 484 files is 484 requests,
# and every anchor into a file is then checked, not a sample of them.
ANCHOR_FILE_STRIDE=${ANCHOR_FILE_STRIDE:-1}
grep -E '#L[0-9]+-L[0-9]+$' "$URLS" | sort -u > "$TMP/anchored.txt"
sed -E 's/#L[0-9]+(-L[0-9]+)?$//' "$TMP/anchored.txt" | sort -u \
  | awk -v s="$ANCHOR_FILE_STRIDE" '(NR - 1) % s == 0' > "$TMP/anchor-files.txt"
echo "=== anchors: $(wc -l < "$TMP/anchored.txt" | tr -d ' ') over $(wc -l < "$TMP/anchor-files.txt" | tr -d ' ') file(s) ==="

export ANCHORED="$TMP/anchored.txt"
check_file_anchors() {
  file="$1"
  raw=$(printf '%s' "$file" | sed -E 's|^https://github.com/|https://raw.githubusercontent.com/|; s|/blob/|/|')
  lines=$(curl -sS --max-time 30 -L "$raw" | wc -l | tr -d ' ')
  grep -F "$file#L" "$ANCHORED" | while IFS= read -r url; do
    end=${url##*-L}
    if [ "$lines" -ge "$end" ]; then echo "OK $end/$lines $url"; else echo "SHORT $end/$lines $url"; fi
  done
}
export -f check_file_anchors
xargs -P "$JOBS" -I{} bash -c 'check_file_anchors "$@"' _ {} < "$TMP/anchor-files.txt" | sort > "$PREFIX-anchors.txt"
awk '{print $1}' "$PREFIX-anchors.txt" | sort | uniq -c
grep '^SHORT' "$PREFIX-anchors.txt" | head -20

echo "logs: $PREFIX-env.txt $PREFIX-files.txt $PREFIX-anchors.txt"
