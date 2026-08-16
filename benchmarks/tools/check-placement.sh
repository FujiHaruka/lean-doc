#!/usr/bin/env bash
# Check a placement A/B measurement before anyone reads its numbers.
#
# The measurement it guards is `.github/workflows/ci-placement.yml`: the same
# documentation build run in the SAME job as `lake build` (arm `same`) and in a
# SEPARATE job (arm `split`). What makes such a run worth reading is not that
# the steps were green — a measurement that silently ran four times instead of
# ten is green — but that
#
#   1. every run the workflow SAID it would do reported a result, and no result
#      appeared that was never declared (both directions, like the corpus
#      gate's `--verify-list`), and
#   2. the two arms did the SAME WORK: placement is supposed to change the
#      clock, not the bytes. Two clean builds of the measurement target produce
#      a byte-identical site 【実測 2026-08-17: 432 files, digest faf6d6c9…】,
#      so a digest that differs between runs means the arms are not comparable
#      and the ratio between them means nothing.
#
# The inventory is written BEFORE the runs, so a run that dies takes the check
# down with it instead of shrinking the denominator.
#
# usage:
#   check-placement.sh <results-dir>
#
#   <results-dir>/runs.txt              the inventory: one "<arm> <index>" per
#                                       intended run, written before running
#   <results-dir>/timings-<arm>-<i>.json   ci-build.sh --timings for that run
#   <results-dir>/site-<arm>-<i>.sha256    digest of that run's site tree
#   <results-dir>/time-<arm>-<i>.txt       that run's log, which must carry the
#                                          extractor server's `ready <ns>` line
#
# exit 0 when every declared run reported and all digests agree; 1 otherwise,
# with one line per violation.
set -euo pipefail

DIR="${1:-}"
[ -n "$DIR" ] || { echo "usage: check-placement.sh <results-dir>" >&2; exit 2; }
[ -d "$DIR" ] || { echo "no such directory: $DIR" >&2; exit 2; }

INVENTORY="$DIR/runs.txt"
[ -s "$INVENTORY" ] || {
  echo "FAIL: no inventory at $INVENTORY — nothing declares what should have run" >&2
  exit 1
}

fail=0
note () { echo "FAIL: $*" >&2; fail=1; }

# ------------------------------------------------------------------ declared
declared=""
n_declared=0
while read -r arm idx _rest; do
  case "$arm" in ''|'#'*) continue ;; esac
  [ -n "$idx" ] || { note "inventory line has no index: '$arm'"; continue; }
  declared="$declared $arm-$idx"
  n_declared=$((n_declared + 1))
done < "$INVENTORY"

[ "$n_declared" -gt 0 ] || { note "inventory is empty"; exit 1; }

# ------------------------------------------------------- every declared run
digests=""
n_reported=0
for run in $declared; do
  t="$DIR/timings-$run.json"
  d="$DIR/site-$run.sha256"

  if [ ! -s "$t" ]; then
    note "$run declared but no timings at $t"
    continue
  fi
  if ! jq -e . "$t" > /dev/null 2>&1; then
    note "$run has timings that are not JSON: $t"
    continue
  fi
  # `docs` is the phase the whole measurement is about; a run that skipped it
  # still writes a timings file.
  secs="$(jq -r '.phases.docs.seconds // empty' "$t")"
  if [ -z "$secs" ]; then
    note "$run has no 'docs' phase in $t"
    continue
  fi
  if ! awk -v s="$secs" 'BEGIN{exit !(s > 0)}'; then
    note "$run reported docs=$secs seconds, which is not a measurement"
    continue
  fi
  files="$(jq -r '.siteFiles // 0' "$t")"
  if [ "$files" -le 0 ] 2>/dev/null; then
    note "$run produced $files site files"
    continue
  fi

  if [ ! -s "$d" ]; then
    note "$run declared but no site digest at $d"
    continue
  fi

  # The phase the placement claim is actually about. `lean-doc build` starts the
  # extractor as a server and the server's `ready <ns>` line IS the environment
  # import 【実測 2026-08-17: ready 17366885792 ns against extractSeconds
  # 24.447 s, round phases 5.932 s】. A run whose log has no such line measured
  # an end-to-end number and nothing about the import — which is most of what
  # this A/B is for, so it does not count as a run.
  log="$DIR/time-$run.txt"
  if [ ! -s "$log" ]; then
    note "$run declared but no run log at $log"
    continue
  fi
  # `|| true`: no match makes grep exit 1, and under `set -e` an assignment from
  # a failing command substitution would end the run here — silently turning
  # "this log has no import time" into "the checker did not finish".
  ns="$( (grep -o 'ready [0-9]\{1,\}' "$log" || true) | head -1 | cut -d' ' -f2)"
  if [ -z "$ns" ] || [ "$ns" -le 0 ] 2>/dev/null; then
    note "$run has no environment-import time (no 'ready <ns>' in $log)"
    continue
  fi
  digests="$digests $run:$(cut -d' ' -f1 < "$d")"
  n_reported=$((n_reported + 1))
done

# ------------------------------------------------------ nothing undeclared
# The other direction: a result nobody asked for means the inventory and the
# run loop disagree, and the count in the report is then not the count that ran.
for t in "$DIR"/timings-*.json; do
  [ -e "$t" ] || continue
  run="$(basename "$t" .json)"; run="${run#timings-}"
  case " $declared " in
    *" $run "*) ;;
    *) note "$run reported a result but is not in the inventory" ;;
  esac
done

# ---------------------------------------------------------- same work, twice
uniq_digests="$(for e in $digests; do echo "${e#*:}"; done | sort -u | wc -l | tr -d ' ')"
if [ "$n_reported" -gt 1 ] && [ "$uniq_digests" != 1 ]; then
  note "the arms did not produce the same site ($uniq_digests distinct digests) — the runs are not comparable"
  for e in $digests; do echo "       ${e%%:*}  ${e#*:}" >&2; done
fi

echo "inventory $n_declared / reported $n_reported / distinct site digests ${uniq_digests:-0}"
[ "$fail" = 0 ] || exit 1
[ "$n_reported" = "$n_declared" ] || {
  echo "FAIL: $n_declared declared, $n_reported reported" >&2
  exit 1
}
echo "ok"
