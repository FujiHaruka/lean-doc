#!/usr/bin/env bash
# Start / drive / stop a resident extractor (`Extract.lean --serve`) whose
# lifetime is independent of any one pipeline run.
#
# WHY A HOLDER PROCESS
#   The server's protocol ends its loop on EOF. If each request opened the FIFO,
#   wrote, and closed it, the *first* request would look like EOF to the server
#   and it would exit. So `start` leaves one long-lived writer (`sleep`) attached
#   to the FIFO, and every request is an extra, short-lived writer. Killing the
#   holder is therefore also the clean way to stop the server: it sees EOF and
#   returns 0 through its own exit path rather than being signalled.
#
# WHY REPLIES ARE COUNTED BY PREFIX
#   `run` prints a human-readable report to the same stdout as the protocol
#   lines, so `ok` replies cannot be located by line number. Each `request`
#   counts the `^ok ` lines already present before it writes, then waits for one
#   more. That makes the verb stateless: separate invocations of this script,
#   which is what `incremental.sh` does, need no shared file descriptor.
#
# THE ENVIRONMENT IS A SNAPSHOT AND THAT IS THE POINT
#   Whatever the oleans said when `start` ran is what every later request sees.
#   The caller owns the question of whether that is the right answer; this script
#   records the generation so the caller can be held to it (see `--generation`).
#
# usage:
#   serve-ctl.sh start   <dir> <modules.txt> [generation-tag]
#   serve-ctl.sh request <dir> <modules.txt> <out.jsonl> <ir-dir>
#   serve-ctl.sh stop    <dir>
#   serve-ctl.sh info    <dir>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
BIN="$LD/experiments/stage4b/build/extract"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"

CMD=${1:?start|request|stop|info}
D=${2:?server dir}
FIFO="$D/req.fifo"
OUT="$D/serve.out"

now () { python3 -c 'import time; print(repr(time.time()))'; }

# `grep -c` prints 0 *and* exits 1 when there is no match, so `|| echo 0` would
# emit "0\n0" and every arithmetic test downstream would fail. Swallow the status
# instead of adding a second value.
count () { # count <pattern> <file>
  local n
  n=$(grep -c "$1" "$2" 2>/dev/null || true)
  echo "${n:-0}"
}

# The server `cd`s into the target repository, so every path it is handed has to
# be absolute. A relative one does not fail loudly — it resolves against the
# target and the server dies with "no such file or directory" while the caller
# waits out its whole timeout.
abspath () {
  case "$1" in
    /*) echo "$1" ;;
    *) echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")" ;;
  esac
}

# Waits for the Nth line matching a prefix. Gives up early if the server is gone:
# waiting out a 600 s timeout on a process that died in the first second is the
# difference between a readable failure and a hung experiment.
wait_for () { # wait_for <pattern> <n>
  local pat="$1" n="$2" i=0 pid=""
  [ -f "$D/server.pid" ] && pid=$(cat "$D/server.pid")
  until [ "$(count "$pat" "$OUT")" -ge "$n" ]; do
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
      # One more look: the reply may have landed between the last poll and the
      # exit, which is ordinary for the final request before a stop.
      [ "$(count "$pat" "$OUT")" -ge "$n" ] && break
      echo "serve-ctl: the server exited before '$pat' #$n" >&2
      tail -20 "$D/serve.err" >&2 || true
      exit 1
    fi
    i=$((i + 1))
    if [ "$i" -gt 6000 ]; then
      echo "serve-ctl: no '$pat' #$n after 600 s" >&2
      tail -20 "$D/serve.err" >&2 || true
      exit 1
    fi
    sleep 0.1
  done
}

case "$CMD" in
start)
  MODULES=$(abspath "${3:?module list}")
  GEN=${4:-unlabelled}
  [ -f "$MODULES" ] || { echo "no such module list: $MODULES" >&2; exit 2; }
  mkdir -p "$D"
  rm -f "$FIFO" "$OUT" "$D/serve.err" "$D/holder.pid" "$D/server.pid"
  mkfifo "$FIFO"
  : > "$OUT"
  # The holder blocks on open until a reader appears, which is why it is
  # backgrounded before the server rather than after it.
  sleep 1000000 > "$FIFO" &
  echo $! > "$D/holder.pid"
  T0=$(now)
  ( cd "$TARGET" && exec "$LAKE" env "$BIN" "$MODULES" "$D/serve-events.jsonl" \
      --equations --refs --write-ir --tagged-code --serve ) \
    < "$FIFO" > "$OUT" 2> "$D/serve.err" &
  echo $! > "$D/server.pid"
  wait_for '^ready ' 1
  T1=$(now)
  READY=$(grep '^ready ' "$OUT" | head -1)
  {
    echo "generation $GEN"
    echo "modules    $MODULES"
    echo "startedAt  $T0"
    echo "readyAt    $T1"
    echo "startSeconds $(python3 -c "print(repr($T1 - $T0))")"
    echo "ready      $READY"
  } > "$D/info.txt"
  cat "$D/info.txt"
  ;;
request)
  MODULES=$(abspath "${3:?module list}")
  [ -f "$MODULES" ] || { echo "no such module list: $MODULES" >&2; exit 2; }
  # `abspath` resolves through `dirname`, so the parent has to exist first.
  mkdir -p "$(dirname "${4:?out jsonl}")" "$(dirname "${5:?ir dir}")"
  REQOUT=$(abspath "$4")
  IRDIR=$(abspath "$5")
  case "$IRDIR" in
    "$TARGET"|"$TARGET"/*)
      echo "refusing to write into the measurement target" >&2; exit 2 ;;
  esac
  [ -p "$FIFO" ] || { echo "serve-ctl: no server at $D" >&2; exit 2; }
  BEFORE=$(count '^ok ' "$OUT")
  mkdir -p "$IRDIR"
  A=$(now)
  printf '%s %s %s\n' "$MODULES" "$REQOUT" "$IRDIR" > "$FIFO"
  wait_for '^ok ' $((BEFORE + 1))
  B=$(now)
  REPLY=$(grep '^ok ' "$OUT" | sed -n "$((BEFORE + 1))p")
  CODE=$(echo "$REPLY" | awk '{print $2}')
  echo "$REPLY $(python3 -c "print('wall %.4f' % ($B - $A))")"
  [ "$CODE" = 0 ] || { echo "serve-ctl: request failed with code $CODE" >&2; exit 1; }
  ;;
stop)
  if [ -f "$D/holder.pid" ]; then
    kill "$(cat "$D/holder.pid")" 2>/dev/null || true
  fi
  if [ -f "$D/server.pid" ]; then
    # EOF should end the loop on its own; the wait bounds how long that is
    # allowed to take before the process is signalled.
    S=$(cat "$D/server.pid")
    for _ in $(seq 1 100); do
      kill -0 "$S" 2>/dev/null || break
      sleep 0.1
    done
    kill -0 "$S" 2>/dev/null && kill "$S" 2>/dev/null || true
  fi
  rm -f "$FIFO"
  echo "stopped: $(count '^ok ' "$OUT") request(s) served"
  ;;
info)
  cat "$D/info.txt"
  ;;
*)
  echo "usage: serve-ctl.sh start|request|stop|info <dir> ..." >&2
  exit 2 ;;
esac
