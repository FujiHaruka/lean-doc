#!/usr/bin/env bash
# How long the Rust `detect` stage takes to build the ledger of the 432 target
# modules — cold and warm, against the frozen prototype in the same session.
#
# Series:
#   rust-warm   ./target/release/lean-doc ledger build, sha256, concurrency 1
#   ts-warm     experiments/stage5/ledger.ts build, the same arguments
#   rust-cold   rust-warm with the 432 oleans evicted from the page cache first
#   ts-cold     ts-warm with the same eviction
#
# The two warm series are interleaved with each other so that neither gets the
# machine in a state the other did not; the cold series run afterwards, each on
# its own. They cannot be interleaved with the warm ones — the eviction that
# makes one run cold would make the next warm run cold too, and there would be
# four cold series and no warm ones.
#
# Eviction is benchmarks/tools/olean-evict (msync(MS_INVALIDATE), no sudo) over
# exactly the 432 target oleans, so it is surgical: the binaries, deno's
# transpile cache and the module list stay warm, and the only thing that has to
# be paged back in is what is being hashed. **The target is only read.**
#
# Both implementations do the same work: the same 432 modules, the same
# 237,909,832 B of olean, SHA-256, one read at a time, one ledger written. The
# ratio between them is therefore a ratio between two implementations of one
# job — not between a job and a smaller job (CLAUDE.md「倍率は分母を明示する」).
#
# usage: benchmarks/tools/measure-ledger.sh [rounds]   (default 8)

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$LD/.." && pwd)"
RESULTS="$LD/results"
WORK="${WORK_DIR:-/private/tmp/lean-doc-relay/m3/measure}"
TARGET="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"
IR="${IR_DIR:-/private/tmp/lean-doc-relay/w7h/base-ir}"
MODULES="$RESULTS/it-modules.txt"
ROUNDS="${1:-8}"
RUST_BIN="$REPO/target/release/lean-doc"
LEDGER_TS="$REPO/experiments/stage5/ledger.ts"
URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec

for p in "$RUST_BIN" "$LEDGER_TS" "$MODULES" "$TARGET" "$IR" "$HERE/olean-evict"; do
  [ -e "$p" ] || { echo "missing: $p" >&2; exit 1; }
done

mkdir -p "$WORK"
RAW="$WORK/raw"; mkdir -p "$RAW"

# The exact file set the cold series evicts: the 432 target modules' oleans.
OLEANS="$WORK/target-oleans.txt"
: > "$OLEANS"
while read -r m; do
  [ -n "$m" ] || continue
  for s in .olean .olean.server .olean.private; do
    p="$TARGET/.lake/build/lib/lean/$(echo "$m" | tr '.' '/')$s"
    [ -f "$p" ] && echo "$p" >> "$OLEANS"
  done
done < "$MODULES"
OLEAN_FILES=$(grep -c . "$OLEANS")
OLEAN_BYTES=$(xargs stat -f '%z' < "$OLEANS" | awk '{s+=$1} END {printf "%d", s}')
echo "eviction set: $OLEAN_FILES files, $OLEAN_BYTES B"

one_run () { # one_run <series> <index>
  local s="$1" i="$2"
  local tl="$RAW/time-$s-$i.txt" tj="$RAW/timings-$s-$i.json"
  printf '{}\n' > "$tj"
  case "$s" in
    *cold) "$HERE/olean-evict" < "$OLEANS" > "$RAW/evict-$s-$i.txt" 2>&1 ;;
  esac
  case "$s" in
    rust-*)
      python3 "$REPO/experiments/stage4c/merge-timing.py" --name "m3a-ledger-$s" --run "$i" \
        --time-l "$tl" --timings "$tj" --exec -- \
        "$RUST_BIN" ledger build --modules "$MODULES" --target "$TARGET" --ir "$IR" \
        --source-url "$URL" --algorithm sha256 --concurrency 1 \
        --out "$WORK/ledger-$s.json" --timings "$tj" \
        >> "$RESULTS/m3a-ledger-$s.jsonl"
      ;;
    ts-*)
      python3 "$REPO/experiments/stage4c/merge-timing.py" --name "m3a-ledger-$s" --run "$i" \
        --time-l "$tl" --timings "$tj" --exec -- \
        "$(command -v deno)" run --allow-read --allow-write --allow-env "$LEDGER_TS" build \
        --modules "$MODULES" --target "$TARGET" --ir "$IR" \
        --source-url "$URL" --algorithm sha256 --concurrency 1 \
        --out "$WORK/ledger-$s.json" --timings "$tj" \
        >> "$RESULTS/m3a-ledger-$s.jsonl"
      ;;
  esac
}

WARM=(rust-warm ts-warm)
COLD=(rust-cold ts-cold)
for s in "${WARM[@]}" "${COLD[@]}"; do rm -f "$RESULTS/m3a-ledger-$s.jsonl"; done

for i in $(seq 1 "$ROUNDS"); do
  for s in "${WARM[@]}"; do one_run "$s" "$i"; done
  echo "  warm round $i done"
done
for s in "${COLD[@]}"; do
  for i in $(seq 1 "$ROUNDS"); do one_run "$s" "$i"; done
  echo "  cold series $s done"
done

cache_note () {
  case "$1" in
    *cold) echo "cold: olean-evict over the $OLEAN_FILES oleans before every run" ;;
    *)     echo "warm (interleaved with ${WARM[*]})" ;;
  esac
}
impl_note () {
  case "$1" in
    rust-*) echo "Rust  $("$RUST_BIN" --version) ($(rustc --version))" ;;
    ts-*)   echo "Deno  $(deno --version | head -1)" ;;
  esac
}

KEYS=keySeconds,hashSeconds,writeSeconds,totalSeconds,hashedBytes
for s in "${WARM[@]}" "${COLD[@]}"; do
  {
    echo "# m3a-ledger-$s"
    echo
    echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host              $(uname -srm) / $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo '?') / $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024)) GB"
    echo "implementation    $(impl_note "$s")"
    echo "hashed            432 modules / $OLEAN_FILES olean files / $OLEAN_BYTES B"
    echo "algorithm         sha256, concurrency 1"
    echo "cache             $(cache_note "$s")"
    echo "runs              $ROUNDS (run 1 dropped)"
    echo "Lean              never started"
    echo
    python3 "$REPO/experiments/stage5/summarize.py" "$RESULTS/m3a-ledger-$s.jsonl" --keys "$KEYS"
  } > "$RESULTS/m3a-ledger-$s.txt"
  echo "-> $RESULTS/m3a-ledger-$s.txt"
done
