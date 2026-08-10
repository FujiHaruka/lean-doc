#!/usr/bin/env bash
# Why did the referring module C change here when stage 5c measured it byte
# identical? The one difference between the two E-B moves is the shim style, so
# that is what this isolates.
#
# For each style: reset the clone, rebuild, confirm the ledger is back to zero
# (that confirmation is load-bearing — it proves the base ledger still describes
# the tree, so a later "changed" is about the move and not about drift), apply
# the move, rebuild, and report exactly which modules the ledger now calls
# changed.
#
# usage: probe-shim.sh <work-dir> <clone-dir>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
S5="$LD/experiments/stage5"
W=${1:?work dir}
CLONE=${2:?clone dir}
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"
C=InformationTheory.Shannon.FisherDeBruijnGaussian
OUT="$W/out"; mkdir -p "$OUT"

deno_ () { deno run --allow-read --allow-write --allow-env "$@"; }

check () { # check <label> <modules>
  deno_ "$S5/ledger.ts" check --ledger "$W/ledger.json" --ir "$W/base-ir" \
    --source-url "$URL" --modules "$2" --changed-out "$W/probe-changed.txt" \
    --removed-out "$W/probe-removed.txt" --render-all-out /dev/null
}

modlist () {
  (cd "$CLONE" && find InformationTheory.lean InformationTheory -name '*.lean' | sort) \
    | sed 's/\.lean$//; s#/#.#g' > "$1"
}

REPORT="$OUT/probe-shim.txt"
: > "$REPORT"
for style in minimal keep-imports; do
  echo "=== style: $style" | tee -a "$REPORT"
  "$HERE/setup-clone.sh" reset "$CLONE" > "$W/probe-reset-$style.log" 2>&1
  modlist "$W/probe-modules.txt"
  echo "-- after reset (must be 0 changed, 0 added, 0 removed):" | tee -a "$REPORT"
  check reset "$W/probe-modules.txt" | tee -a "$REPORT"

  "$HERE/setup-clone.sh" move "$CLONE" "$style" > "$W/probe-move-$style.log" 2>&1
  modlist "$W/probe-modules.txt"
  echo "-- after the move:" | tee -a "$REPORT"
  check move "$W/probe-modules.txt" | tee -a "$REPORT"
  if grep -qx "$C" "$W/probe-changed.txt"; then
    echo "   C IS in the changed set" | tee -a "$REPORT"
  else
    echo "   C is NOT in the changed set" | tee -a "$REPORT"
  fi
  echo | tee -a "$REPORT"
done
echo "-> $REPORT"
