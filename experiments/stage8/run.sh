#!/usr/bin/env bash
# stage 8 — close B-3 (b): the ordering constraint of the *runtime* (JS) rev
# injection, in a real browser instead of a DOM shim.
#
#   V1..V4  which URL reaches location.replace, per configuration
#   V5      cost of the rewrite, by number of links on the page (1 / 9 / 73)
#   V6      no ?jump=src -> no location.replace, in every configuration
#   V7      the extra round trip for source-url.js: parse blocking, caching
#
# Usage: experiments/stage8/run.sh
# Env:   W      work dir     (default /private/tmp/lean-doc-relay/w8)
#        PAGES  source tree  (default w7e/pages-ph, copied, never written to)
#        OUT    report dir   (default benchmarks/results)
set -euo pipefail

REPO=/Users/haruka/dev/lean-doc
W=${W:-/private/tmp/lean-doc-relay/w8}
PAGES=${PAGES:-/private/tmp/lean-doc-relay/w7e/pages-ph}
STATIC=${STATIC:-/Users/haruka/dev/lean-projects/.lake/packages/doc-gen4/static}
OUT=${OUT:-$REPO/benchmarks/results}
DENO=${DENO:-/opt/homebrew/bin/deno}
CHROME=${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}
PORT=${PORT:-8931}
CDP_PORT=${CDP_PORT:-9331}
S8=$REPO/experiments/stage8

mkdir -p "$W" "$OUT"

serve() { # <log> [extra args...]
  local log=$1; shift
  "$DENO" run --allow-read --allow-net --allow-write "$S8/serve.ts" \
    --root "$W" --port "$PORT" --log "$log" "$@" > "$W/server-stdout.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 50); do
    if curl -fsS -o /dev/null "http://127.0.0.1:$PORT/b0/InformationTheory.html"; then return 0; fi
    perl -e 'select(undef,undef,undef,0.1)'
  done
  echo "server did not come up" >&2; exit 1
}
stop() { kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }
trap 'stop' EXIT

# ------------------------------------------------------------------- build
echo "### build the trees (pages-ph is copied, never written to)"
"$DENO" run --allow-read --allow-write --allow-run "$S8/build-sites.ts" \
  --pages "$PAGES" --static "$STATIC" --script "$S8/source-url.js" --out "$W" \
  | tee "$W/build-sites.log"

# --------------------------------------------------------- hook feasibility
echo "### can location.replace be wrapped from JS?"
serve "$W/server-hook.jsonl"
"$DENO" run -A "$S8/probe.ts" --suite hook --tag hook \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-hook.log"
stop

# ------------------------------------------------------------- V1..V4 + V6
echo "### V1..V4 (?jump=src) and V6 (no ?jump=src)"
serve "$W/server-nav.jsonl"
"$DENO" run -A "$S8/probe.ts" --suite nav --tag nav --runs 5 \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-nav.log"
stop

# --------------------------------------------------------------------- V5
echo "### V5 rewrite cost (min 1 / p50 9 / target 10 / max-links 73 / biggest page 47)"
serve "$W/server-cost.jsonl"
"$DENO" run -A "$S8/probe.ts" --suite cost --tag cost --runs 8 \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-cost.log"
stop

# --------------------------------------------------------------------- V7
echo "### V7a no artificial delay, plus the cache walk"
serve "$W/server-v7-d0.jsonl"
"$DENO" run -A "$S8/probe.ts" --suite v7 --tag v7-d0 --runs 8 \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-v7-d0.log"
stop

echo "### V7b source-url.js delayed 400 ms — does the synchronous tag stall the parse?"
serve "$W/server-v7-d400.jsonl" --delay-source-url 400
"$DENO" run -A "$S8/probe.ts" --suite v7 --tag v7-d400 --runs 8 --cache-walk no \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-v7-d400.log"
stop

# ------------------------------------------------------------ V7, caching
# Two laps over five pages, twice: once with a server that sends no validators
# and no freshness (what a naive static host does), once with max-age.
echo "### V7c does source-url.js come back over the wire on pages 2..N?"
serve "$W/server-cache-nocc.jsonl"
"$DENO" run -A "$S8/probe.ts" --suite cache --tag cache-nocc \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-cache-nocc.log"
stop

serve "$W/server-cache-maxage.jsonl" --cache-control "max-age=3600"
"$DENO" run -A "$S8/probe.ts" --suite cache --tag cache-maxage \
  --port "$PORT" --cdp-port "$CDP_PORT" --out "$W" --chrome "$CHROME" \
  | tee "$W/probe-cache-maxage.log"
stop

echo "done. artefacts in $W"
