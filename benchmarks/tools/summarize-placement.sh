#!/usr/bin/env bash
# Tabulate a placement A/B run. Reads only what the runs left behind; it makes
# no claim `check-placement.sh` has not already checked.
#
# WHAT THE COLUMNS ARE
#   import   the environment load — the extractor server's `ready <ns>` line.
#            **This is the number the placement claim is about**; everything
#            else in a documentation build is CPU work that placement cannot
#            move 【実測 2026-08-17: ready 17.367 s of a 24.447 s extract phase
#            cold, 6.852 s of 13.784 s warm, same machine, same bytes】.
#   extract  the whole extraction phase (import + analysis + writing the IR).
#   docs     `lean-doc build` end to end, from ci-build.sh.
#   major    major page faults, from `/usr/bin/time -v` — how much of the import
#            actually came off the disk.
#
# HOW TO READ IT, AND THE ONE TRAP
#   Only **r1** of each arm is a placement measurement: r2.. ran with the cache
#   the run before them warmed. The honest comparison is therefore
#
#     across arms   same-r1 vs split-r1        <- the placement, one pair
#     within an arm r1 vs rN                   <- that runner's own cold penalty
#
#   and the second one exists because the first is confounded: the two arms are
#   on two runner instances by construction, and `ubuntu-latest` instances differ
#   by up to 2.19x on pure CPU work with no relation to their I/O speed 【実測】.
#   The per-runner calibrator in `env-before.txt` says whether that happened.
#
# usage:
#   summarize-placement.sh <results-dir> [<arms-dir>]
set -euo pipefail

DIR="${1:-results}"
ARMS="${2:-}"

field () { # field <file> <jq-path>
  [ -s "$1" ] && jq -r "$2 // empty" "$1" 2> /dev/null || true
}

# The last number on the first line matching <pattern>, or nothing. Every step
# has to be allowed to find nothing: a pipeline whose last command is a `grep`
# that misses exits 1, and under `set -e` an assignment from it ends the script
# — printing a header and no table, which is what this helper exists to stop.
# (`/usr/bin/time -l` on BSD reports none of these labels, so a local dry run
# takes that path every time.)
last_number () { # last_number <file> <grep-pattern>
  [ -s "$1" ] || return 0
  { grep -o "$2" "$1" 2> /dev/null || true; } | head -1 | { grep -o '[0-9]\{1,\}$' || true; }
}

echo "## runs"
printf '%-18s %10s %10s %10s %10s %12s %10s  %s\n' \
  run import_s extract_s render_s docs_s major_faults peak_MB site_digest

for t in "$DIR"/timings-*.json; do
  [ -e "$t" ] || continue
  run="$(basename "$t" .json)"; run="${run#timings-}"
  log="$DIR/time-$run.txt"
  ld="$DIR/leandoc-$run.json"
  dg="$DIR/site-$run.sha256"

  ns="$( (grep -o 'ready [0-9]\{1,\}' "$log" 2> /dev/null || true) | head -1 | cut -d' ' -f2)"
  imp="$(awk -v n="${ns:-}" 'BEGIN{ if (n == "") print "?"; else printf "%.3f", n/1e9 }')"
  maj="$(last_number "$log" 'Major (requiring I/O) page faults: [0-9]\{1,\}')"
  rss="$(last_number "$log" 'Maximum resident set size (kbytes): [0-9]\{1,\}')"
  rss="$(awk -v k="${rss:-}" 'BEGIN{ if (k == "") print "?"; else printf "%.0f", k/1024 }')"

  printf '%-18s %10s %10s %10s %10s %12s %10s  %s\n' \
    "$run" \
    "$imp" \
    "$(field "$ld" .extractSeconds)" \
    "$(field "$ld" .renderSeconds)" \
    "$(field "$t" .phases.docs.seconds)" \
    "${maj:-?}" \
    "$rss" \
    "$( [ -s "$dg" ] && cut -c1-16 < "$dg" || echo '?')"
done

# ------------------------------------------------------------------ the ratios
#
# Printed only for pairs where both arms' r1 exist. A missing half prints the
# pair and no number rather than a ratio against nothing.
echo
echo "## placement (r1 of each arm, per pair) — cold is the control, not a placement"
printf '%-6s %11s %11s %7s %11s %7s %10s %10s %7s\n' \
  pair same_imp split_imp s/s cold_imp c/s same_docs split_docs s/s

pairs="$(for t in "$DIR"/timings-*-p*r1.json; do
  [ -e "$t" ] || continue
  b="$(basename "$t" .json)"; echo "${b##*-p}" | sed 's/r1$//'
done | sort -un || true)"

import_of () { # import_of <arm> <pair>
  local log="$DIR/time-$1-p$2r1.txt" ns
  ns="$( (grep -o 'ready [0-9]\{1,\}' "$log" 2> /dev/null || true) | head -1 | cut -d' ' -f2)"
  awk -v n="${ns:-}" 'BEGIN{ if (n == "") print ""; else printf "%.3f", n/1e9 }'
}

ratio () { awk -v a="$1" -v b="$2" 'BEGIN{ if (a == "" || b == "" || a+0 == 0) print "-"; else printf "%.2fx", b/a }'; }

for p in $pairs; do
  si="$(import_of same "$p")"; xi="$(import_of split "$p")"; ci="$(import_of cold "$p")"
  sd="$(field "$DIR/timings-same-p${p}r1.json" .phases.docs.seconds)"
  xd="$(field "$DIR/timings-split-p${p}r1.json" .phases.docs.seconds)"
  printf '%-6s %11s %11s %7s %11s %7s %10s %10s %7s\n' \
    "$p" "${si:--}" "${xi:--}" "$(ratio "$si" "$xi")" \
    "${ci:--}" "$(ratio "$si" "$ci")" \
    "${sd:--}" "${xd:--}" "$(ratio "$sd" "$xd")"
done

echo
echo "## each runner's own cold penalty (r1 / last rep, inside one arm)"
printf '%-14s %12s %12s %8s\n' arm-pair r1_import last_import ratio
for t in "$DIR"/timings-*r1.json; do
  [ -e "$t" ] || continue
  b="$(basename "$t" .json)"; b="${b#timings-}"; stem="${b%r1}"
  last=""
  for o in "$DIR/time-$stem"r*.txt; do [ -e "$o" ] && last="$o"; done
  a="$( (grep -o 'ready [0-9]\{1,\}' "$DIR/time-${stem}r1.txt" 2> /dev/null || true) | head -1 | cut -d' ' -f2)"
  z="$( (grep -o 'ready [0-9]\{1,\}' "$last" 2> /dev/null || true) | head -1 | cut -d' ' -f2)"
  printf '%-14s %12s %12s %8s\n' "$stem" \
    "$(awk -v n="${a:-}" 'BEGIN{ if (n == "") print "?"; else printf "%.3f", n/1e9 }')" \
    "$(awk -v n="${z:-}" 'BEGIN{ if (n == "") print "?"; else printf "%.3f", n/1e9 }')" \
    "$(awk -v a="${a:-0}" -v b="${z:-0}" 'BEGIN{ if (b+0 == 0) print "-"; else printf "%.2fx", a/b }')"
done

# ------------------------------------------------------------------ conditions
if [ -n "$ARMS" ] && [ -d "$ARMS" ]; then
  echo
  echo "## conditions, per runner (the calibrator is what says the two arms are comparable)"
  for e in "$ARMS"/*/env-before.txt; do
    [ -e "$e" ] || continue
    echo
    echo "### $(basename "$(dirname "$e")")"
    cat "$e"
  done
fi
