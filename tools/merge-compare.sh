#!/usr/bin/env bash
# Compare two trees written by tools/merge-reference.sh and say what differs.
#
# usage: tools/merge-compare.sh REFERENCE_DIR CANDIDATE_DIR
#
# The whole loop is
#   tools/merge-reference.sh --impl ts
#   cargo build --release -p lean-doc
#   tools/merge-reference.sh --impl rust
#   tools/merge-compare.sh /private/tmp/lean-doc-relay/m3b/ref \
#                          /private/tmp/lean-doc-relay/m3b/rust
# `cargo test -p lean-doc-incr --test merge` makes the same comparison in process
# when the base IR is on the machine.
#
# Four classes of file, compared four ways:
#
#   *timings*.json      keys and counts, ignoring every `*Seconds`: durations are
#                       wall clock and differ between runs by construction.
#   *-ownership.json    the same rule, plus the two path fields (`base` / `inc`),
#                       which name the tree the run was given and so differ by
#                       construction too.
#   *-stdout.txt        byte for byte after masking the durations and the tree
#                       root — the two stages print the prototype's exact lines,
#                       so the shape is compared even though the clock is not.
#   everything else     byte for byte. The merged IR trees, the module sets, the
#                       `--changed-out` files and the `--verify` transcripts.
#
# **There is no exception list.** M3-b changes the key order of `deps/*.json`
# and of `index.json`'s `dependencyMaps` entries on purpose (plan §7's "already
# not byte-identical" note, decided in M3: the port writes Lean's order so that
# an incremental tree is byte for byte a from-scratch one). A comparator with an
# exception list would swallow the second divergence silently, so instead every
# difference is *classified* by a rule that names no file: a file whose JSON is
# the same mapping in another key order is reported as REORDERED and counted
# separately. The run is still `DIFFERENT`, and the counts are what tells you
# whether the divergence stayed where it was meant to.
#
# Measured 2026-08-12: 4,051 files, 3,986 identical, **65 REORDERED** (48
# `deps/*.json` + 17 `index.json`), 0 differing, 0 missing.

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
status=0

compare_json_ignoring () { # <ref> <cand> <key regex to ignore> -> differences
  python3 - "$1" "$2" "$3" <<'PY'
import json, re, sys
ignore = re.compile(sys.argv[3])
def load(path):
    with open(path) as f:
        record = json.loads(f.read().strip())
    return {k: v for k, v in record.items() if not ignore.search(k)}
a, b = load(sys.argv[1]), load(sys.argv[2])
for key in sorted(set(a) | set(b)):
    if a.get(key, '<absent>') != b.get(key, '<absent>'):
        print(f"    {key}: reference {a.get(key, '<absent>')!r}, candidate {b.get(key, '<absent>')!r}")
PY
}

# A log line differs in two places by construction: the clock, and the directory
# the run was given. Both are masked by shape, not by file name.
compare_masked () { # <ref> <cand> <ref root> <cand root>
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import re, sys
def mask(path, root):
    text = open(path, encoding='utf-8').read()
    text = text.replace(root, '<TREE>')
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
  if [ ! -f "$CAND/$path" ]; then
    printf '%-58s MISSING in candidate\n' "$path"
    missing=$((missing + 1)); status=1; continue
  fi
  a=$(wc -c < "$REF/$path" | tr -d ' ')
  b=$(wc -c < "$CAND/$path" | tr -d ' ')
  case "$path" in
    *-ownership.json|*timings*.json)
      case "$path" in
        *-ownership.json) ignore='^(base|inc)$|Seconds$' ;;
        *)                ignore='Seconds$' ;;
      esac
      out=$(compare_json_ignoring "$REF/$path" "$CAND/$path" "$ignore")
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
        printf '%-58s DIFFERS (after masking the clock and the tree root)\n%s\n' "$path" "$out"
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
if [ "$status" -eq 0 ]; then echo "IDENTICAL"; else echo "DIFFERENT"; fi
exit "$status"
