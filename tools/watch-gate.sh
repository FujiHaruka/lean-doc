#!/usr/bin/env bash
# A-2 — `litedoc4 watch` notices one changed module, rebuilds exactly that much,
# and serves it.
#
# WHAT IT PROVES WHEN IT FAILS, IN ONE LINE
#   Either "the loop rebuilt the wrong amount of work" — naming the count it got
#   and the count it expected — or "the loop rebuilt the wrong module", naming
#   both; or the server did not answer the page the rebuild had just written.
#
# WHY IT IS A GATE AND NOT A TEST
#   It needs the measurement target, its toolchain and the 171 MB Lean extractor,
#   and it starts a long-lived process and talks to it over TCP. `cargo test
#   --workspace` reaches none of those. What the tests own instead is the half
#   that has its own input: the loop's decision function and the server's routing
#   and refusals (`crates/litedoc4/src/watch.rs`, `crates/litedoc4/src/httpd.rs`).
#
# THE ASSERTIONS ARE INTEGERS. NOT ONE OF THEM IS A DURATION.
#   This workload's environment load moves 5x with the page cache (2.5 s <-> 13 s
#   【実測】, CLAUDE.md), so a second is not a threshold — a bound loose enough to
#   pass a cold runner passes a regression too, and one tight enough to catch a
#   regression fails on a cold runner. What one edit costs in *work* does not move:
#
#     modulesExtracted   == 1     the touched module, and only it
#     pagesRendered      == 1     its own page (mode self, empty map delta)
#     extractorRequests  == 1     Lean was started once, not once per round
#     render-set.txt     == that module, by name
#
#   The fourth is the one the first three cannot give: a run that re-extracted
#   one module and re-rendered one page could have done it to the wrong one, and
#   three integers would all be green. It is read from the pipeline's own
#   diagnostic rather than recomputed here.
#
#   The wall clock is printed, because a person reading this wants to know, and
#   is asserted on by nothing.
#
# HOW THE MODULE IS "TOUCHED", AND WHY NOT WITH `touch`
#   `touch` moves an mtime, and the ledger hashes **content** (sha256), so it
#   would change nothing. Rewriting the olean would change something — the
#   measurement target, which nothing in this repository is allowed to write to
#   (CLAUDE.md). So the change is injected into this run's own copy of the ledger
#   with `litedoc4 ledger touch`, which exists for exactly this reason
#   (`crates/litedoc4/src/main.rs`: "the measurement target must not be modified,
#   so 'module M changed' is injected into the ledger instead"). What the loop
#   sees is what it would see after a real `lake build` of that module: its
#   ledger entry no longer matches the olean on disk.
#
#   The ledger being written is `$OUT/build/ledger.json`, which this script's own
#   build wrote. Nothing under the target is opened for writing.
#
# HOW TO MAKE IT FAIL ON PURPOSE (and it has been, 【実測 2026-08-19】)
#     --inject wrong-module   touch a **different** module from the one the
#                             assertions expect. The three counts stay 1/1/1 —
#                             the loop did exactly one module's worth of work —
#                             and the name check is the only thing that catches
#                             it, which is the whole reason it is here.
#     --inject no-touch       touch nothing at all and still wait for a rebuild:
#                             the gate must time out and say so rather than
#                             report a green run it never saw.
#   Both break only files under this script's work area. The repository, the
#   target and their ledgers are not touched, and no `git checkout` is involved.
#
# WHAT IT REFUSES BEFORE IT STARTS
#   Another `litedoc4 watch` already using this work area or this port. `watch`
#   outlives the shell that started it, so one from an interrupted session is
#   still rewriting the IR this script is about to read and recreating the files
#   its cleanup is about to delete — which is reported as a corrupt IR tree and
#   as `rm: Directory not empty` respectively 【実測 2026-08-21】. It is named
#   and refused (exit 2), never killed: killing a process this script does not
#   own is not a gate's call.
#
# usage: tools/watch-gate.sh [--out DIR] [--keep] [--reuse] [--port N]
#                            [--target DIR] [--lib NAME] [--jobs N]
#                            [--module NAME] [--other NAME]
#                            [--inject wrong-module|no-touch]
#   --out      work area (default: /private/tmp/lean-doc-relay/watch-gate)
#   --keep     do not delete it on the way out (a site is ~35 MB)
#   --reuse    keep an existing build under --out instead of building again
#   --port     the port `watch` binds (default 8485, one above `watch`'s own
#              default so that a gate run does not collide with a person's)
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"

OUT=/private/tmp/lean-doc-relay/watch-gate
TARGET=/Users/haruka/dev/lean-projects
LIB=InformationTheory
JOBS=4
PORT=8485
# One of the 422, and its page is its own: `impact --mode self` selects exactly
# the changed module unless the whole-package map delta adds more, and this one's
# delta is empty. `tools/build-gate.sh` moves a different module on purpose (one
# whose name is quoted in another module's docstring); this gate wants the
# *simplest* answer, because it is asserting an exact number.
MODULE=InformationTheory.Shannon.ArithmeticCoding
OTHER=InformationTheory.Shannon.BroadcastChannel.Basic
INJECT=
KEEP=0
REUSE=0
LITEDOC4="${LITEDOC4:-$REPO/target/release/litedoc4}"
EXTRACT_BIN="${EXTRACT_BIN:-$REPO/extractor/build/extract}"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --lib) LIB="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --module) MODULE="$2"; shift 2 ;;
    --other) OTHER="$2"; shift 2 ;;
    --inject) INJECT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    --reuse) REUSE=1; shift ;;
    -h|--help) sed -n '1,79p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

case "$INJECT" in
  ""|wrong-module|no-touch) ;;
  *) echo "--inject takes wrong-module or no-touch, not $INJECT" >&2; exit 2 ;;
esac

[ -x "$LITEDOC4" ] || {
  echo "no litedoc4 at $LITEDOC4 — mise exec -- cargo build --release -p litedoc4" >&2; exit 2; }
[ -x "$EXTRACT_BIN" ] || { echo "no extractor at $EXTRACT_BIN — extractor/build.sh" >&2; exit 2; }
command -v "$LAKE" >/dev/null 2>&1 || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 2; }
[ -d "$TARGET" ] || { echo "no target package at $TARGET" >&2; exit 2; }
case "$OUT" in
  "$TARGET"|"$TARGET"/*) echo "--out may not be inside the target" >&2; exit 2 ;;
  "$REPO"|"$REPO"/*) echo "--out may not be inside this repository" >&2; exit 2 ;;
esac

BUILD="$OUT/build"
LOG="$OUT/watch.log"
WATCH_PID=
# **Whether this run owns the work area.** 0 until the guard below has passed,
# and the cleanup deletes nothing while it is 0 — a run that *refuses* to use a
# directory must not then delete it. This script removed a work area another
# process was using, on the way out of the refusal that existed to protect it
# 【実測 2026-08-21】.
OWNED=0

# ------------------------------------------------- nobody else is watching this
#
# `litedoc4 watch` is long-lived and **outlives the shell that started it**, so
# one left over from an interrupted session is still holding a work area that
# looks free. Two things then happen, and neither of them looks like what it is
# 【実測 2026-08-21】:
#
#   * it rewrites the IR while this script's build reads it — the gate fails with
#     `EOF while parsing a value at line 1 column 0` on an IR module, which reads
#     as a corrupt tree rather than as a second writer (the incremental pipeline
#     writes IR module files in place, `litedoc4-incr::merge`'s `copy`/`write`,
#     so a half-written file is a state a reader can see);
#   * it recreates files while the cleanup deletes them — `rm: Directory not
#     empty`, three times.
#
# So: **refused by name, and not killed.** Killing a process this script does not
# own is not a gate's call; saying which pid it is, is. The difference this makes
# is between "the gate is broken" and "the machine has a leftover process", and
# the first reading costs a full diagnosis.
others () { pgrep -f 'litedoc4 watch' 2>/dev/null; }

guard_no_other_watch () {
  local found=0 pid line
  for pid in $(others); do
    line="$(ps -o command= -p "$pid" 2>/dev/null)"
    [ -n "$line" ] || continue
    case "$line" in
      *"$OUT"*)
        echo "watch-gate REFUSED  pid $pid is a \`litedoc4 watch\` already using this work area ($OUT) — stop it, or pass --out DIR" >&2
        echo "  $line" >&2
        found=1 ;;
      *"--port $PORT"*)
        echo "watch-gate REFUSED  pid $pid is a \`litedoc4 watch\` already on port $PORT — stop it, or pass --port N" >&2
        echo "  $line" >&2
        found=1 ;;
    esac
  done
  # Anything at all on the port, `litedoc4` or not: the run would fail at bind
  # and the message would be about a port rather than about this.
  if command -v lsof >/dev/null 2>&1; then
    local listening
    # Into a variable, not into a file under --out: this runs before the run
    # owns that directory.
    listening="$(lsof -nP -iTCP:"$PORT" -sTCP:LISTEN 2>/dev/null)"
    if [ -n "$listening" ]; then
      echo "watch-gate REFUSED  something is already listening on port $PORT — stop it, or pass --port N" >&2
      printf '%s\n' "$listening" | sed 's/^/  /' >&2
      found=1
    fi
  fi
  if [ "$found" != 0 ]; then exit 2; fi
}

# `if`, never `[ … ] && …`: an EXIT trap's last command decides the script's exit
# status, so a trailing test that comes out false turns a passing run into
# exit 1 — which `tools/e2e-micro.sh` actually did while printing "ok"
# 【実測 2026-08-18】.
cleanup () {
  if [ -n "$WATCH_PID" ]; then
    if kill -0 "$WATCH_PID" 2>/dev/null; then
      kill "$WATCH_PID" 2>/dev/null
      wait "$WATCH_PID" 2>/dev/null
    fi
  fi
  if [ "$KEEP" -eq 0 ] && [ "$OWNED" -eq 1 ]; then
    if [ -d "$OUT" ]; then rm -rf "$OUT"; fi
  fi
}
trap cleanup EXIT

say () { printf '\n=== %s\n' "$1"; }

# Every assertion this run made, and every one that failed. **The two are
# reported together** — a gate that prints only failures cannot be told from a
# gate that checked nothing (CLAUDE.md, 「ゲートは『走った本数』を数える」).
CHECKS=0
FAILURES=0
check () { # check <ok|fail> <one line>
  CHECKS=$((CHECKS + 1))
  if [ "$1" = ok ]; then
    printf '  ok    %s\n' "$2"
  else
    FAILURES=$((FAILURES + 1))
    printf '  FAIL  %s\n' "$2" >&2
  fi
}

# Wait for a line, bounded, and **notice when the thing being waited on has
# died**. A wait that cannot end is the failure this whole feature exists to
# remove (doc-gen4 #404), so the gate does not have one either.
wait_for () { # wait_for <regex> <seconds> <what>
  local pattern="$1" limit="$2" what="$3" waited=0
  while [ "$waited" -lt "$limit" ]; do
    if grep -qE "$pattern" "$LOG" 2>/dev/null; then return 0; fi
    if [ -n "$WATCH_PID" ] && ! kill -0 "$WATCH_PID" 2>/dev/null; then
      echo "watch-gate FAIL  the watch process exited while waiting for $what" >&2
      tail -20 "$LOG" >&2
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  echo "watch-gate FAIL  ${limit}s passed and $what never happened" >&2
  tail -20 "$LOG" >&2
  return 1
}

guard_no_other_watch
# Past the guard: from here the work area is this run's, and the cleanup may
# remove it.
mkdir -p "$OUT"
OWNED=1

# ------------------------------------------------------------------- 1. build

say "1/5 a site to watch"
if [ "$REUSE" -eq 1 ] && [ -f "$BUILD/ledger.json" ]; then
  echo "  reusing $BUILD"
else
  rm -rf "$BUILD"
  # Redirected to a file rather than piped: a pipeline's exit status is its last
  # stage's, and this project has read a refusal as a success twice that way
  # 【実測 2026-08-18】.
  "$LITEDOC4" build --root "$TARGET" --out "$BUILD" --lib "$LIB" \
    --extractor-bin "$EXTRACT_BIN" --lake "$LAKE" --jobs "$JOBS" > "$OUT/build.log" 2>&1
  status=$?
  if [ "$status" != 0 ]; then
    echo "watch-gate FAIL  the first build exited $status; see $OUT/build.log" >&2
    tail -20 "$OUT/build.log" >&2
    exit 1
  fi
  grep -E '^(modules|work|build) ' "$OUT/build.log" | sed 's/^/  /'
fi
MODULES="$(grep -c . "$BUILD/work/modules.txt")"

# ------------------------------------------------------------------- 2. watch

say "2/5 start watch on port $PORT"
: > "$LOG"
"$LITEDOC4" watch --root "$TARGET" --out "$BUILD" --lib "$LIB" \
  --extractor-bin "$EXTRACT_BIN" --lake "$LAKE" --jobs "$JOBS" --port "$PORT" \
  >> "$LOG" 2>&1 &
WATCH_PID=$!
if ! wait_for '^watch   asks the ledger' 60 "the watch banner"; then exit 1; fi
sed 's/^/  /' "$LOG"

code="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/" 2>/dev/null)"
if [ "$code" = 200 ]; then
  check ok "the server answers / with 200 before anything changed"
else
  check fail "the server answered / with $code, expected 200"
fi

# ------------------------------------------------------- 3. change one module

say "3/5 make one module stale"
TOUCHED="$MODULE"
case "$INJECT" in
  wrong-module)
    TOUCHED="$OTHER"
    echo "  injected: touching $TOUCHED while the assertions still expect $MODULE" ;;
  no-touch)
    echo "  injected: touching nothing, and still waiting for a rebuild" ;;
esac
if [ "$INJECT" != no-touch ]; then
  "$LITEDOC4" ledger touch --ledger "$BUILD/ledger.json" --module "$TOUCHED" > "$OUT/touch.log" 2>&1
  status=$?
  if [ "$status" != 0 ]; then
    echo "watch-gate FAIL  ledger touch exited $status; see $OUT/touch.log" >&2
    cat "$OUT/touch.log" >&2
    exit 1
  fi
  sed 's/^/  /' "$OUT/touch.log"
fi

# The rebuild is one module's extraction on top of one Lean import; 300 s is
# generous for a cold page cache and is a bound, not an expectation. With
# nothing touched there is nothing to wait for, so the wait is short and its
# timeout *is* the answer.
DEADLINE=300
if [ "$INJECT" = no-touch ]; then DEADLINE=30; fi
if ! wait_for '^watch   #1 reload' "$DEADLINE" "the first rebuild"; then exit 1; fi
sed -n '/^watch   #1 the ledger reports/,$p' "$LOG" | sed 's/^/  /'

# ------------------------------------------------------------ 4. the integers

say "4/5 what the pass cost, as integers"
python3 - "$BUILD/litedoc4-build.json" "$BUILD/work/render-set.txt" "$MODULE" "$MODULES" "$LOG" \
  > "$OUT/counts.txt" 2>&1 <<'PY'
import json
import re
import sys

marker_path, render_set_path, expected, modules, log_path = sys.argv[1:6]
modules = int(modules)
record = json.load(open(marker_path, encoding="utf-8"))
work = record["work"] or {}
rendered = work.get("pagesRendered")
extracted = work.get("modulesExtracted")
requests = work.get("extractorRequests")

with open(render_set_path, encoding="utf-8") as handle:
    render_set = [line.strip() for line in handle if line.strip()]

# The pipeline's own count of the affected set, out of the line it printed —
# a **second** number for the same claim, produced by `impact` rather than by
# the renderer. Comparing the renderer's `pagesWritten` with the map/impact
# union is the one comparison here that is not the record agreeing with itself.
affected = None
mode = None
with open(log_path, encoding="utf-8") as handle:
    for line in handle:
        found = re.match(r"impact\s+mode (\S+) -> (\d+) page\(s\)", line)
        if found:
            mode, affected = found.group(1), int(found.group(2))

results = []
def check(ok, line):
    results.append(("ok  " if ok else "FAIL", line))

check(record.get("complete") is True, "the marker says the run finished")
check(extracted == 1,
      f"work.modulesExtracted is {extracted}, expected 1 (one module went stale)")
check(rendered == 1,
      f"work.pagesRendered is {rendered}, expected 1 (mode self, empty map delta)")
check(requests == 1,
      f"work.extractorRequests is {requests}, expected 1 (one Lean import for the pass)")
check(render_set == [expected],
      "the render set is "
      + (", ".join(render_set) if render_set else "(empty)")
      + f", expected {expected}")
check(affected == rendered,
      f"impact (mode {mode}) selected {affected} page(s) and the renderer wrote {rendered} — "
      "two derivations of the same set")
check(rendered is not None and rendered < modules,
      f"work.pagesRendered is {rendered} of {modules} module(s): one edit did not re-render "
      "the package")

for verdict, line in results:
    print(f"{verdict}  {line}")
print(f"checks  {len(results)}, failed {sum(1 for v, _ in results if v == 'FAIL')}")
sys.exit(1 if any(v == "FAIL" for v, _ in results) else 0)
PY
status=$?
sed 's/^/  /' "$OUT/counts.txt"
made="$(grep -cE '^(ok|FAIL)  ' "$OUT/counts.txt" 2>/dev/null)"
failed="$(grep -cE '^FAIL  ' "$OUT/counts.txt" 2>/dev/null)"
CHECKS=$((CHECKS + ${made:-0}))
FAILURES=$((FAILURES + ${failed:-0}))
# A python that died before it printed anything is a failure with no FAIL line,
# which is the shape that would otherwise be counted as zero problems.
if [ "$status" != 0 ] && [ "${failed:-0}" = 0 ]; then
  echo "watch-gate FAIL  the count check itself did not run (exit $status)" >&2
  FAILURES=$((FAILURES + 1))
fi

# ------------------------------------------------------------- 5. the server

say "5/5 the server serves what the rebuild wrote"
PAGE="/$(printf '%s' "$MODULE" | tr '.' '/').html"
code="$(curl -sS -o "$OUT/page.html" -w '%{http_code}' "http://127.0.0.1:$PORT$PAGE" 2>/dev/null)"
if [ "$code" = 200 ]; then
  check ok "GET $PAGE -> 200 ($(wc -c < "$OUT/page.html" | tr -d ' ') B)"
else
  check fail "GET $PAGE -> $code, expected 200 (the page the rebuild just wrote)"
fi
code="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/no/such/page.html" 2>/dev/null)"
if [ "$code" = 404 ]; then
  check ok "GET /no/such/page.html -> 404"
else
  check fail "GET /no/such/page.html -> $code, expected 404"
fi
# `--path-as-is`, or curl collapses the `..` before it leaves the client and the
# server is never asked the question this line exists to ask.
code="$(curl -sS --path-as-is -o /dev/null -w '%{http_code}' \
  "http://127.0.0.1:$PORT/../../../etc/passwd" 2>/dev/null)"
if [ "$code" = 403 ]; then
  check ok "GET /../../../etc/passwd -> 403"
else
  check fail "GET /../../../etc/passwd -> $code, expected 403"
fi

# The 3 GB process is the loop's, not the machine's: nothing may be left holding
# an imported Lean environment once watch is gone.
kill "$WATCH_PID" 2>/dev/null
wait "$WATCH_PID" 2>/dev/null
WATCH_PID=
left="$(pgrep -f "$EXTRACT_BIN" | wc -l | tr -d ' ')"
if [ "$left" = 0 ]; then
  check ok "no resident extractor is left behind"
else
  check fail "$left resident extractor process(es) survived the watch loop"
fi

# ------------------------------------------------------------------ the verdict

{
  printf '\n'
  printf 'date              %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'host              %s / %s / %s GB\n' \
    "$(uname -srm)" \
    "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?')" \
    "$(($(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 / 1024 / 1024))"
  printf 'target            %s (%s modules, lib %s)\n' "$TARGET" "$MODULES" "$LIB"
  printf 'toolchain         %s\n' "$(tr -d '\n' < "$TARGET/lean-toolchain" 2>/dev/null || echo '?')"
  printf 'extractor         %s\n' "$EXTRACT_BIN"
  printf 'port / jobs       %s / %s\n' "$PORT" "$JOBS"
  printf 'module touched    %s\n' "$TOUCHED"
  printf 'inject            %s\n' "${INJECT:-none}"
  printf 'rebuild wall      %s\n' \
    "$(grep -oE 'in [0-9.]+ s$' "$LOG" | tail -1 || echo '?') (printed, asserted on by nothing)"
} | tee "$OUT/conditions.txt"

printf '\n'
if [ "$FAILURES" = 0 ]; then
  echo "WATCH GATE: ok — $CHECKS check(s), 0 failed"
  exit 0
fi
echo "WATCH GATE: FAIL — $CHECKS check(s), $FAILURES failed" >&2
exit 1
