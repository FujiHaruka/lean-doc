#!/bin/bash
# Measure how far the public mathlib4 doc site (built from master) has drifted
# from the version this project is pinned to.
#
# Two levels are measured:
#   (1) module   — does <module>.html still exist on the live site?  (hard 404)
#   (2) anchor   — for modules that do exist, is each declaration id still
#                  present on the page?  (soft failure: lands at page top)
#
# Ground truth is the local checkout / doc build of the pinned version:
#   modules      <target>/.lake/packages/mathlib/Mathlib/**/*.lean
#   declarations <target>/.lake/build/doc-data/declaration-data-Mathlib.<Mod>.bmp
#
# Network only — no Lean, no lake. Usage:
#   benchmarks/tools/measure-docsite-drift.sh [out-dir]
set -u

TARGET=${TARGET:-/Users/haruka/dev/lean-projects}
BASE=${BASE:-https://leanprover-community.github.io/mathlib4_docs}
STRIDE=${STRIDE:-40}          # module sample: keep every STRIDE-th path
JOBS=${JOBS:-6}
OUTDIR=${1:-benchmarks/results}
STAMP=$(date -u +%Y-%m-%d)

SRC=$TARGET/.lake/packages/mathlib/Mathlib
DD=$TARGET/.lake/build/doc-data
ENV_LOG=$OUTDIR/docsite-drift-$STAMP-env.txt
HOST_LOG=$OUTDIR/docsite-drift-$STAMP-hosting.txt
MOD_LOG=$OUTDIR/docsite-drift-$STAMP-modules.txt
DECL_LOG=$OUTDIR/docsite-drift-$STAMP-decls.txt
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

[ -d "$SRC" ] || { echo "no mathlib checkout at $SRC" >&2; exit 1; }
[ -d "$DD" ]  || { echo "no doc-data at $DD" >&2; exit 1; }

# ---- conditions -------------------------------------------------------------
{
  echo "date (UTC)          : $(date -u +'%Y-%m-%d %H:%M:%S')"
  echo "base                : $BASE"
  echo "target              : $TARGET"
  echo "target toolchain    : $(cat "$TARGET/lean-toolchain")"
  echo "target mathlib rev  : $(rg -B5 '"name": "mathlib"' "$TARGET/lake-manifest.json" | rg -o '[0-9a-f]{40}' | tail -1)"
  echo "target mathlib pin  : $(rg -B8 '"name": "mathlib"' "$TARGET/lake-manifest.json" | rg -o '"inputRev": "[^"]+"' | tail -1 | sed 's|.*: "||; s|"$||')"
  echo "site last-modified  : $(curl -sSI "$BASE/" | rg -i '^last-modified:' | tr -d '\r' | sed 's|^[^:]*: ||')"
  echo "site lean version   : $(curl -sS "$BASE/" | rg -o 'This was built using Lean 4 <a[^>]*>[^<]+' | rg -o '[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$')"
  echo "site mathlib rev    : $(curl -sS "$BASE/Mathlib/Data/List/Zip.html" | rg -o 'mathlib4/blob/[0-9a-f]+' | head -1 | sed 's|.*/||')"
  echo "local modules       : $(find "$SRC" -name '*.lean' | wc -l | tr -d ' ')"
  echo "local doc-data bmp  : $(find "$DD" -name '*.bmp' | wc -l | tr -d ' ')"
  echo "module sample stride: $STRIDE"
  echo "parallelism         : $JOBS"
} > "$ENV_LOG"
cat "$ENV_LOG"

# ---- (0) hosting: is any version other than the live one reachable? ---------
{
  echo "# candidate version-pinned paths under the doc site"
  for p in v4.31.0 4.31.0 versions archive v4.33.0 4.33.0; do
    printf '%s %s/%s/\n' "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 "$BASE/$p/")" "$BASE" "$p"
  done
  echo "# the deploy that produces the live site (no ref: pin => default branch)"
  gh api repos/leanprover-community/mathlib4_docs/contents/.github/workflows/docs.yaml --jq '.content' \
    | base64 -d | rg -n 'cron|repository:|deploy-pages|upload-pages-artifact|ref:' || true
  echo "# the doc repo itself carries no built site"
  gh api repos/leanprover-community/mathlib4_docs --jq '"size_kb=\(.size) default_branch=\(.default_branch)"' || true
  gh api repos/leanprover-community/mathlib4_docs/contents --jq '[.[].name] | join(" ")' || true
  echo "# releases carry no doc assets"
  gh api "repos/leanprover-community/mathlib4/releases?per_page=3" \
    --jq '.[] | "\(.tag_name) assets=\(.assets | length)"' || true
} > "$HOST_LOG" 2>&1
cat "$HOST_LOG"

# ---- (1) module level -------------------------------------------------------
find "$SRC" -name '*.lean' | sed "s|^$SRC/||; s|\.lean$||" | sort \
  | awk -v s="$STRIDE" 'NR % s == 1' > "$TMP/modules.txt"
echo "== module level: $(wc -l < "$TMP/modules.txt" | tr -d ' ') sampled"

check_module() {
  code=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 "$BASE/Mathlib/$1.html")
  echo "$code Mathlib/$1.html"
}
export -f check_module
export BASE
xargs -P "$JOBS" -I{} bash -c 'check_module "$@"' _ {} < "$TMP/modules.txt" | sort > "$MOD_LOG"
awk '{print $1}' "$MOD_LOG" | sort | uniq -c

# ---- (2) anchor level -------------------------------------------------------
# live modules for which the pinned build left a declaration-data file
grep '^200' "$MOD_LOG" | sed 's|^200 Mathlib/||; s|\.html$||' | tr '/' '.' > "$TMP/live.txt"
: > "$TMP/probe.txt"
while IFS= read -r m; do
  [ -f "$DD/declaration-data-Mathlib.$m.bmp" ] && echo "$m" >> "$TMP/probe.txt"
done < "$TMP/live.txt"
echo "== anchor level: $(wc -l < "$TMP/probe.txt" | tr -d ' ') of $(wc -l < "$TMP/live.txt" | tr -d ' ') live modules have local doc-data"

check_anchors() {
  mod="$1"
  bmp="$DD/declaration-data-Mathlib.$mod.bmp"
  path=$(echo "$mod" | tr '.' '/')
  # names the pinned build placed on THIS page; source-line anchors (#L12-L20)
  # are not declarations and are excluded
  names=$(rg -o "\./Mathlib/$path\.html#[^\"\\\\]+" "$bmp" | sed 's|.*#||' \
          | rg -v '^L[0-9]+(-L[0-9]+)?$' | sort -u)
  [ -n "$names" ] || { echo "SKIP_NONAMES Mathlib.$mod"; return; }
  html=$(curl -sS --max-time 30 "$BASE/Mathlib/$path.html")
  total=0; miss=0; missing=""
  while IFS= read -r nm; do
    [ -n "$nm" ] || continue
    total=$((total + 1))
    printf '%s' "$html" | grep -qF "id=\"$nm\"" || { miss=$((miss + 1)); missing="$missing $nm"; }
  done <<< "$names"
  echo "MOD Mathlib.$mod total=$total missing=$miss ---$missing"
}
export -f check_anchors
export DD
xargs -P "$JOBS" -I{} bash -c 'check_anchors "$@"' _ {} < "$TMP/probe.txt" | sort > "$DECL_LOG"

awk '/^MOD/ {
       t = 0; m = 0
       for (i = 1; i <= NF; i++) {
         if ($i ~ /^total=/)   { split($i, a, "="); t = a[2] }
         if ($i ~ /^missing=/) { split($i, b, "="); m = b[2] }
       }
       T += t; M += m; N++; if (m > 0) BAD++
     }
     END { printf "modules=%d (with missing anchors: %d) declarations=%d missing=%d rate=%.4f%%\n",
                  N, BAD, T, M, (T ? 100 * M / T : 0) }' "$DECL_LOG"

echo "logs: $ENV_LOG $MOD_LOG $DECL_LOG"
