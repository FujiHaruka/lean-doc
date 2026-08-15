#!/usr/bin/env bash
# Compare two trees written by tools/incremental-reference.sh and say what
# differs.
#
# usage: tools/incremental-compare.sh REFERENCE_DIR CANDIDATE_DIR
#
# The whole loop is
#   tools/incremental-reference.sh --impl ts
#   cargo build --release -p lean-doc
#   tools/incremental-reference.sh --impl rust
#   tools/incremental-compare.sh /private/tmp/lean-doc-relay/m3d3/ref \
#                                /private/tmp/lean-doc-relay/m3d3/rust
#
# WHAT IS NOT COMPARED, AND WHY IT IS NOT LAZINESS
#
#   *-stderr.txt    A diagnostic's wording belongs to the implementation
#                   (`deno run script.ts` prints no program name; the CLI prints
#                   `lean-doc: `). What *is* compared is *-complained.txt, which
#                   the harness derives from it — whether the run complained at
#                   all is a fact about the answer.
#   *-stdout.txt    **The two pipelines' stdout are different by design, not by
#                   accident.** `incremental.sh` prints exactly one line — the
#                   timings JSON — and sends every stage's chatter to a log file
#                   under `--work`; `lean-doc incremental` prints a progress line
#                   per stage as well (`pipeline.rs:339`, `:445`, `:477`, `:504`,
#                   `:543`), which its module heading states as intent. Masking
#                   the clock would not make those two comparable, and an
#                   exception that fires on all seven scenarios is not a
#                   comparison. The one thing both really print is the timings
#                   record, and the harness distils it into *-counts.json — which
#                   *is* compared, and which is the answer this milestone is
#                   about.
#   *-sitecheck.txt Written by `--impl rust` only: it is the incremental page
#                   tree diffed against a whole site rebuilt from the same IR, so
#                   there is nothing on the TS side to compare it with. It is a
#                   *within-implementation* oracle, read by hand.
#   conditions.txt  The clock, the host and the implementation's own name.
#
# HOW EVERYTHING ELSE IS COMPARED
#
#   **Byte for byte, first, including every `.json`.** That is deliberate and it
#   is the difference from tools/impact-compare.sh: M3-d2b is a statement about
#   the *order of an array inside `index.json`*, and a comparator that parses
#   JSON before comparing it cannot see an ordering difference at all. So bytes
#   decide, and only a file whose bytes differ is classified further:
#
#     REORDERED        both parse as JSON and are equal as values — the same
#                      mapping written with the keys in another order.
#     (masked)         equal once every `*Seconds` key is dropped (at any depth)
#                      and each side's own output root is masked out of the
#                      strings. Counted as identical, and reported separately so
#                      the number is never mistaken for a byte match.
#     ARRAY-REORDERED  the same elements in another sequence — every JSON array
#                      recursively sorted makes the two equal, or (for a
#                      line-oriented file, which is an array written with
#                      newlines) the two hold the same lines in another order.
#                      **This is where `index.json` and `render-set.txt` are
#                      expected to land**: the prototype's merge keeps whatever
#                      order the base index had and appends a re-added module to
#                      the end while the product orders the index by `--modules`
#                      (M3-d2b), and the prototype's `sort -u` collates in the
#                      caller's locale while the product sorts in UTF-16 code
#                      units (plan §7, U1). It still **fails the run** (exit 1).
#                      Whether a difference was intended is a judgement for a
#                      person; a comparator that made it would be an exception
#                      list with extra steps.
#     DIFFERS          everything else.
#
#   **There is no exception list.** No rule below names a file. The four classes
#   are decided by suffix and by the shape of the content, so a difference that
#   appears somewhere nobody predicted is reported rather than absorbed.

set -uo pipefail

REF="${1-}"
CAND="${2-}"

[ -n "$REF" ] && [ -n "$CAND" ] || { echo "usage: $0 REFERENCE_DIR CANDIDATE_DIR" >&2; exit 2; }
[ -d "$REF" ] || { echo "no such directory: $REF" >&2; exit 1; }
[ -d "$CAND" ] || { echo "no such directory: $CAND" >&2; exit 1; }

identical=0
masked=0
reordered=0
arrayreordered=0
differing=0
missing=0
skipped=0
status=0

# Answers with `reordered`, `masked`, `array-reordered`, `differs` or `not-json`,
# and names nothing. The two roots are the only strings that differ between the
# sides by construction: a run's own output directory reaches `prune.json`'s
# `pages` field. `$REF` is a prefix of `$REF.work`, so masking it masks both.
classify () { # classify <ref> <cand> <ref root> <cand root>
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import json, sys

def load(path):
    with open(path, encoding='utf-8') as f:
        return json.loads(f.read())

def mask(value, root):
    """Drop every duration and neutralise the run's own output root, at any depth."""
    if isinstance(value, dict):
        return {k: mask(v, root) for k, v in value.items() if not k.endswith('Seconds')}
    if isinstance(value, list):
        return [mask(v, root) for v in value]
    if isinstance(value, str):
        return value.replace(root, '<OUT>')
    return value

def sorted_arrays(value):
    if isinstance(value, dict):
        return {k: sorted_arrays(v) for k, v in value.items()}
    if isinstance(value, list):
        return sorted((sorted_arrays(v) for v in value),
                      key=lambda item: json.dumps(item, sort_keys=True))
    return value

def text(path, root):
    with open(path, encoding='utf-8', errors='replace') as f:
        return f.read().replace(root, '<OUT>')

try:
    a, b = load(sys.argv[1]), load(sys.argv[2])
except Exception:
    # Not JSON. A file of one name per line **is** an array, written with
    # newlines instead of brackets, so it gets the same ladder. The trailing
    # newline is compared rather than normalised away: an empty set is an empty
    # file and not one blank line, and every stage in this project spells it
    # that way on purpose.
    ta, tb = text(sys.argv[1], sys.argv[3]), text(sys.argv[2], sys.argv[4])
    if ta == tb:
        print('masked')
    elif (sorted(ta.splitlines()) == sorted(tb.splitlines())
          and ta.endswith('\n') == tb.endswith('\n')):
        print('array-reordered')
    else:
        print('differs')
    sys.exit(0)
if a == b:
    print('reordered')
    sys.exit(0)
ma, mb = mask(a, sys.argv[3]), mask(b, sys.argv[4])
if ma == mb:
    print('masked')
    sys.exit(0)
if sorted_arrays(ma) == sorted_arrays(mb):
    print('array-reordered')
    sys.exit(0)
print('differs')
PY
}

for path in $( (cd "$REF" && find . -type f | sed 's|^\./||' | LC_ALL=C sort) ); do
  case "$path" in
    *-stderr.txt|*-stdout.txt|*-sitecheck.txt|conditions.txt)
      skipped=$((skipped + 1)); continue ;;
  esac
  if [ ! -f "$CAND/$path" ]; then
    printf '%-58s MISSING in candidate\n' "$path"
    missing=$((missing + 1)); status=1; continue
  fi
  if cmp -s "$REF/$path" "$CAND/$path"; then
    identical=$((identical + 1))
    continue
  fi
  a=$(wc -c < "$REF/$path" | tr -d ' ')
  b=$(wc -c < "$CAND/$path" | tr -d ' ')
  verdict=$(classify "$REF/$path" "$CAND/$path" "$REF" "$CAND")
  case "$verdict" in
    masked)
      identical=$((identical + 1)); masked=$((masked + 1)) ;;
    reordered)
      printf '%-58s REORDERED        same mapping, reference %s B, candidate %s B\n' \
        "$path" "$a" "$b"
      reordered=$((reordered + 1)); status=1 ;;
    array-reordered)
      printf '%-58s ARRAY-REORDERED  same elements in another sequence, reference %s B, candidate %s B\n' \
        "$path" "$a" "$b"
      arrayreordered=$((arrayreordered + 1)); status=1 ;;
    *)
      printf '%-58s DIFFERS          reference %s B, candidate %s B\n' "$path" "$a" "$b"
      printf '    %s\n' "$(cmp "$REF/$path" "$CAND/$path" 2>&1 | head -1)"
      # /usr/bin/diff: `diff` is aliased to colordiff in this shell and colordiff
      # is not installed.
      /usr/bin/diff "$REF/$path" "$CAND/$path" 2>/dev/null | head -8 | sed 's/^/    /'
      differing=$((differing + 1)); status=1 ;;
  esac
done

# The same four suffixes are dropped here. A file the comparator refuses to read
# on the reference side is not "extra" on the candidate side either — `--impl
# rust` writes *-sitecheck.txt and the TS side has no counterpart by design.
listing () { # listing <root>
  ( cd "$1" && find . -type f | sed 's|^\./||' \
    | grep -v -e '\-stderr\.txt$' -e '\-stdout\.txt$' -e '\-sitecheck\.txt$' -e '^conditions\.txt$' \
    | LC_ALL=C sort )
}
extra=$( listing "$CAND" | grep -vxF -f <( listing "$REF" ) || true )
if [ -n "$extra" ]; then
  echo
  echo "--- files the candidate wrote that the reference did not"
  printf '%s\n' "$extra"
  status=1
fi

echo
printf 'files compared:  %s\n' "$((identical + reordered + arrayreordered + differing + missing))"
printf '  identical:     %s  (of which %s only after masking the clock and the output root)\n' \
  "$identical" "$masked"
printf '  reordered:     %s  (same JSON mapping, different key order)\n' "$reordered"
printf '  array-reord.:  %s  (same elements, different sequence)\n' "$arrayreordered"
printf '  differing:     %s\n' "$differing"
printf '  missing:       %s\n' "$missing"
printf '  not compared:  %s  (*-stderr.txt, *-stdout.txt, *-sitecheck.txt, conditions.txt)\n' "$skipped"
if [ "$status" -eq 0 ]; then echo "IDENTICAL"; else echo "DIFFERENT"; fi
exit "$status"
