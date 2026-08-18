#!/usr/bin/env bash
# Compare two trees written by tools/impact-reference.sh and say what differs.
#
# usage: tools/impact-compare.sh REFERENCE_DIR CANDIDATE_DIR
#
# It takes both trees as arguments and cares about neither's provenance, so it
# still works — but **the loop it was built for is gone**. The reference side used
# to be `tools/impact-reference.sh --impl ts`, the frozen prototype, and
# `experiments/` was removed on 2026-08-16; it exists only at tag
# `experiments-frozen`. What remains is a diff of two recordings of the Rust side:
#   cargo build --release -p litedoc4
#   tools/impact-reference.sh --out /private/tmp/lean-doc-relay/m3c/before
#   ...change something...
#   tools/impact-reference.sh --out /private/tmp/lean-doc-relay/m3c/after
#   tools/impact-compare.sh /private/tmp/lean-doc-relay/m3c/before \
#                           /private/tmp/lean-doc-relay/m3c/after
# `cargo test -p litedoc4-incr --test impact` makes the same comparison in
# process when the base IR and the page trees are on the machine.
#
# Four classes of file, compared four ways — **by suffix, never by name**:
#
#   *-stderr.txt        NOT compared, and the reason is not laziness: a
#                       diagnostic's wording belongs to the implementation that
#                       wrote it. What *is* compared is
#                       *-complained.txt, which the harness derives from it —
#                       whether the run complained at all is a fact about the
#                       answer, and it is checked for every scenario.
#   *-prune.json        keys and values, ignoring every `*Seconds` (wall clock,
#                       different every run by construction) and with the run's
#                       own output root masked out of the strings — `pages` is
#                       the directory the run was given.
#   *-stdout.txt        byte for byte after masking the durations and the output
#                       root: both stages print the prototype's exact lines (that
#                       is what the port reproduced), so the shape is compared
#                       even though the clock is not.
#   everything else     byte for byte. The selected sets, the census, the
#                       summaries, the exit statuses, the surviving page trees
#                       and their counts.
#
# **There is no exception list.** Where a difference is still a difference, it is
# *classified* by a rule that names no file: a JSON file holding the same mapping
# in another key order is REORDERED, as in tools/merge-compare.sh. Nothing in
# M3-c is expected to reorder anything, so a non-zero REORDERED count is news.
#
# Measured 2026-08-12: **3,631 files compared, 3,631 identical**, 0 reordered,
# 0 differing, 0 missing, 29 not compared (*-stderr.txt).
#
# **What the denominator is made of**, because 3,631 flatters the stages: 3,458
# of them are the *surviving pages* of the eight page trees `prune` ran over —
# files neither implementation wrote, and the check is that neither deleted them.
# That leaves **165 computed records** (29 exit statuses, 29 "did it complain",
# 29 stdouts, 16 "was --print-set written", 12 selected sets, 12 impact
# summaries, 9 prune summaries, 2 censuses, and 27 listings and counts of what
# survived) and 8 input fixtures. Quote the 165 when the question is "does the
# port compute the same answers"; quote the 3,458 when it is "did it delete
# exactly the right files".

set -uo pipefail

REF="${1-}"
CAND="${2-}"

[ -n "$REF" ] && [ -n "$CAND" ] || { echo "usage: $0 REFERENCE_DIR CANDIDATE_DIR" >&2; exit 2; }
[ -d "$REF" ] || { echo "no such directory: $REF" >&2; exit 1; }
[ -d "$CAND" ] || { echo "no such directory: $CAND" >&2; exit 1; }

identical=0
reordered=0
differing=0
missing=0
skipped=0
status=0

# The durations and the run's own output root are the two things that differ by
# construction. Both are masked by shape, not by file name.
compare_json_masked () { # <ref> <cand> <ref root> <cand root>
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import json, sys
def load(path, root):
    with open(path, encoding='utf-8') as f:
        record = json.loads(f.read().strip())
    out = {}
    for key, value in record.items():
        if key.endswith('Seconds'):
            continue
        if isinstance(value, str):
            value = value.replace(root, '<OUT>')
        out[key] = value
    return out
a = load(sys.argv[1], sys.argv[3])
b = load(sys.argv[2], sys.argv[4])
for key in sorted(set(a) | set(b)):
    if a.get(key, '<absent>') != b.get(key, '<absent>'):
        print(f"    {key}: reference {a.get(key, '<absent>')!r}, candidate {b.get(key, '<absent>')!r}")
PY
}

compare_masked () { # <ref> <cand> <ref root> <cand root>
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import re, sys
def mask(path, root):
    text = open(path, encoding='utf-8').read()
    text = text.replace(root, '<OUT>')
    return re.sub(r'\d+\.\d+ s', '<T> s', text)
a = mask(sys.argv[1], sys.argv[3])
b = mask(sys.argv[2], sys.argv[4])
if a != b:
    for i, (x, y) in enumerate(zip(a.splitlines(), b.splitlines()), 1):
        if x != y:
            print(f"    line {i}: reference {x!r}")
            print(f"    line {i}: candidate {y!r}")
            break
    else:
        print(f"    line counts {len(a.splitlines())} vs {len(b.splitlines())}")
PY
}

# Same JSON mapping, different key order? Answers with `reordered`, `differs` or
# `not-json`, and names nothing.
classify_json () { # <ref> <cand>
  python3 - "$1" "$2" <<'PY'
import json, sys
def load(path):
    with open(path, encoding='utf-8') as f:
        return json.load(f)
try:
    a, b = load(sys.argv[1]), load(sys.argv[2])
except Exception:
    print('not-json')
    sys.exit(0)
print('reordered' if a == b else 'differs')
PY
}

for path in $( (cd "$REF" && find . -type f | sed 's|^\./||' | sort) ); do
  case "$path" in
    *-stderr.txt) skipped=$((skipped + 1)); continue ;;
  esac
  if [ ! -f "$CAND/$path" ]; then
    printf '%-58s MISSING in candidate\n' "$path"
    missing=$((missing + 1)); status=1; continue
  fi
  a=$(wc -c < "$REF/$path" | tr -d ' ')
  b=$(wc -c < "$CAND/$path" | tr -d ' ')
  case "$path" in
    *.json)
      out=$(compare_json_masked "$REF/$path" "$CAND/$path" "$REF" "$CAND")
      if [ -z "$out" ]; then
        identical=$((identical + 1))
      else
        printf '%-58s DIFFERS\n%s\n' "$path" "$out"
        differing=$((differing + 1)); status=1
      fi
      ;;
    *-stdout.txt)
      out=$(compare_masked "$REF/$path" "$CAND/$path" "$REF" "$CAND")
      if [ -z "$out" ]; then
        identical=$((identical + 1))
      else
        printf '%-58s DIFFERS (after masking the clock and the output root)\n%s\n' "$path" "$out"
        differing=$((differing + 1)); status=1
      fi
      ;;
    *)
      if cmp -s "$REF/$path" "$CAND/$path"; then
        identical=$((identical + 1))
      else
        verdict=$(classify_json "$REF/$path" "$CAND/$path")
        if [ "$verdict" = reordered ]; then
          printf '%-58s REORDERED  same mapping, reference %s B, candidate %s B\n' \
            "$path" "$a" "$b"
          reordered=$((reordered + 1)); status=1
        else
          printf '%-58s DIFFERS    reference %s B, candidate %s B\n' "$path" "$a" "$b"
          printf '    %s\n' "$(cmp "$REF/$path" "$CAND/$path" 2>&1 | head -1)"
          # /usr/bin/diff: `diff` is aliased to colordiff in this shell and
          # colordiff is not installed.
          /usr/bin/diff "$REF/$path" "$CAND/$path" | head -6 | sed 's/^/    /'
          differing=$((differing + 1)); status=1
        fi
      fi
      ;;
  esac
done

extra=$( (cd "$CAND" && find . -type f | sed 's|^\./||' | sort) \
  | grep -vxF -f <( (cd "$REF" && find . -type f | sed 's|^\./||' | sort) ) || true )
if [ -n "$extra" ]; then
  echo
  echo "--- files the candidate wrote that the reference did not"
  printf '%s\n' "$extra"
  status=1
fi

echo
printf 'files compared: %s\n' "$((identical + reordered + differing + missing))"
printf '  identical:    %s\n' "$identical"
printf '  reordered:    %s  (same JSON mapping, different key order)\n' "$reordered"
printf '  differing:    %s\n' "$differing"
printf '  missing:      %s\n' "$missing"
printf '  not compared: %s  (*-stderr.txt; *-complained.txt carries the fact)\n' "$skipped"
if [ "$status" -eq 0 ]; then echo "IDENTICAL"; else echo "DIFFERENT"; fi
exit "$status"
