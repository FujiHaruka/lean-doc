#!/usr/bin/env bash
# stage 7e-rev — close the two "untested" items on approach.md §8's revision row.
#
# Point A: does the acceptance oracle (stage4c/coverage.ts) survive the
#          placeholder contract if it is run *after* injection?
#          -> render the same IR twice (concrete rev / {{SOURCE_URL}}), inject
#             into the second, compare byte-for-byte with /usr/bin/diff -r, and
#             score all three page sets.
#
# Usage: experiments/stage7e-rev/run.sh
# Env:   W        work dir            (default /private/tmp/lean-doc-relay/w7e)
#        IR       extractor IR dir    (default w7d/ir-j4, written by stage 7d)
#        LIDX     link index          (default w7c/linkindex/link-index.lidx)
#        OUT      report dir          (default benchmarks/results)
set -euo pipefail

REPO=/Users/haruka/dev/lean-doc
W=${W:-/private/tmp/lean-doc-relay/w7e}
IR=${IR:-/private/tmp/lean-doc-relay/w7d/ir-j4}
LIDX=${LIDX:-/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx}
OUT=${OUT:-$REPO/benchmarks/results}
URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec
TOKEN='{{SOURCE_URL}}'
RENDER=$REPO/experiments/stage7d/render.ts
COVERAGE=$REPO/experiments/stage4c/coverage.ts

mkdir -p "$W" "$OUT"

render() { # <pages-dir> <source-url> <stats>
  deno run --allow-read --allow-write --allow-env "$RENDER" \
    --ir "$IR" --pages "$1" --source-url "$2" --link-index "$LIDX" --stats "$3"
}

# ---------------------------------------------------------------- (i) / (ii)
echo "### render (i) concrete rev, (ii) placeholder — same IR"
rm -rf "$W/pages-rev" "$W/pages-ph"
render "$W/pages-rev" "$URL"   "$W/render-rev.txt"   > "$W/render-rev.log"
render "$W/pages-ph"  "$TOKEN" "$W/render-ph.txt"    > "$W/render-ph.log"

# ------------------------------------------------------------- (ii-injected)
# Same injector as stage 6b's V4/V6 (whole-file read / replace / write) so the
# cost is comparable; the *tree* is not the same one, so the seconds are not.
cat > "$W/inject.py" <<'PY'
import os, sys, time
root, token, url = sys.argv[1:4]
tb, ub = token.encode(), url.encode()
n = files = 0
t0 = time.time()
for r, _d, fs in os.walk(root):
    for f in fs:
        p = os.path.join(r, f)
        with open(p, "rb") as fh: data = fh.read()
        files += 1
        if tb in data:
            with open(p, "wb") as fh: fh.write(data.replace(tb, ub))
            n += 1
t1 = time.time()
print("%.4f %d %d" % (t1 - t0, n, files))
PY

echo "### injection — 5 runs, fresh copy each time (the copy is not timed)"
: > "$W/inject-timings.txt"
for i in 1 2 3 4 5; do
  rm -rf "$W/pages-inj"
  cp -R "$W/pages-ph" "$W/pages-inj"
  /usr/bin/time -l python3 "$W/inject.py" "$W/pages-inj" "$TOKEN" "$URL" \
    > "$W/inject-$i.out" 2> "$W/inject-$i.time"
  echo "run $i: $(cat "$W/inject-$i.out")" | tee -a "$W/inject-timings.txt"
done

# --------------------------------------------------------------------- diff
echo "### /usr/bin/diff -r (i) vs (ii-injected)"
set +e
/usr/bin/diff -r "$W/pages-rev" "$W/pages-inj" > "$W/diff-rev-vs-inj.txt" 2>&1
echo "diff exit=$?" | tee "$W/diff-status.txt"
set -e
wc -l < "$W/diff-rev-vs-inj.txt" | tr -d ' ' > "$W/diff-lines.txt"

# ------------------------------------------------------------------ scoring
echo "### coverage.ts on (i) / (ii) / (ii-injected)"
for v in rev ph inj; do
  deno run --allow-read --allow-write --allow-env "$COVERAGE" \
    --pages "$W/pages-$v" --report "$W/coverage-$v.txt" > "$W/coverage-$v.log"
done

echo "done. artefacts in $W"
