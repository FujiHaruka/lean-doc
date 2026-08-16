#!/usr/bin/env bash
# The tests that need the measurement target — run on purpose, never by accident.
#
# WHY THESE ARE NOT ORDINARY TESTS
#   A test owns its input. These do not: they read the 432-module package at
#   /Users/haruka/dev/lean-projects, its doc-gen4 reference tree, generated IR
#   trees and multi-megabyte `--full` recordings, none of which is in the
#   repository. `crates/lean-doc/tests/resident.rs` already draws this line —
#   "that needs a Lean toolchain, a built package and a 3 GB process, so it is a
#   gate and not a test" — and this script is where the line is enforced.
#
# WHY `#[ignore]` AND NOT A SILENT SKIP
#   They used to print "skipping: …" and return, which is invisible in an exit
#   code: on CI, where the corpus can never exist, every one of them passed
#   without running and the green said nothing about them. Worse, it hid real
#   rot — two of them had been skipping for want of a fixture that had been
#   deleted from this machine, and nobody could have told. `#[ignore]` makes
#   `cargo test` report them by count, and this script makes the set of them
#   auditable.
#
# THE INVENTORY
#   tools/corpus-tests.txt lists every ignored test. `--verify-list` fails if the
#   two sides drift, which is what CI runs: a test that quietly stops being in
#   the gate, and a gate that names a test nobody wrote, are the same bug seen
#   from two ends. Adding an ignored test means adding a line, on purpose.
#
# usage:
#   corpus-gate.sh                 run every corpus test (needs the corpus)
#   corpus-gate.sh --verify-list   only check the inventory (needs nothing)
#   corpus-gate.sh --list          print what cargo currently ignores
#   corpus-gate.sh --update-list   rewrite the inventory from cargo's answer
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
INVENTORY="$HERE/corpus-tests.txt"
PYTHON="${PYTHON:-python3}"

# Every `#[ignore]`d test cargo knows about, as `<target>::<name>`.
#
# The target prefix is not decoration: three different tests are called
# `the_corpus_matches_the_prototype` (ledger, impact, merge) and two are called
# `the_whole_corpus`, so a bare name collapses the inventory from 24 entries to
# 20 and hides four tests inside their namesakes. The prefix comes from cargo's
# own `Running … (target/debug/deps/NAME-HASH)` line.
# Asking each test binary directly, rather than reading `cargo test`'s combined
# output: cargo prints `Running …` on stderr and the binary prints the names on
# stdout, so the interleaving depends on whether stdout is a terminal. On a
# CI runner it is not, the names arrive in one block after the last `Running`
# line, and an awk script that pairs them up produces `::name` for everything.
# That is exactly the shape of bug this gate exists to catch, so it should not
# have one of its own.
listed() {
  (cd "$ROOT" && cargo test --workspace --no-run --message-format=json 2>/dev/null) \
    | "$PYTHON" -c '
import json, os, subprocess, sys

for line in sys.stdin:
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("reason") != "compiler-artifact":
        continue
    if not message.get("profile", {}).get("test"):
        continue
    exe = message.get("executable")
    if not exe:
        continue
    # `target/debug/deps/global-0e4a257eabf6c141` -> `global`
    target = os.path.basename(exe).rsplit("-", 1)[0]
    listed = subprocess.run(
        [exe, "--ignored", "--list"], capture_output=True, text=True
    ).stdout
    suffix = ": test"
    for entry in listed.splitlines():
        if entry.endswith(suffix):
            print(target + "::" + entry[: -len(suffix)])
' | sort -u
}

# The inventory, minus comments and section headers. A trailing `# note` on a
# line is documentation of *why* that test needs the corpus and is not part of
# the name.
entries() {
  sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$1" | grep -v '^$' | sort -u
}

expected() {
  entries "$INVENTORY"
}

# The subset that can actually run here. Everything after the `## frozen`
# header needs a generator that is not in HEAD (tag `experiments-frozen`), so
# running it can only ever panic — and a gate that is permanently red is a gate
# nobody reads.
runnable() {
  sed '/^## frozen/,$d' "$INVENTORY" > "$TMP_RUNNABLE"
  entries "$TMP_RUNNABLE"
}

frozen() {
  sed -n '/^## frozen/,$p' "$INVENTORY" > "$TMP_FROZEN"
  entries "$TMP_FROZEN"
}

TMP_RUNNABLE="$(mktemp)"
TMP_FROZEN="$(mktemp)"
trap 'rm -f "$TMP_RUNNABLE" "$TMP_FROZEN"' EXIT

case "${1:-run}" in
  --list)
    listed
    ;;

  --update-list)
    # Deliberately additive: new tests are appended under `## corpus` and the
    # frozen section is preserved, because which section a test belongs in is a
    # judgement about whether its input can be regenerated from HEAD — not
    # something cargo knows.
    added=0
    for name in $(listed); do
      if ! expected | grep -qxF "$name"; then
        printf '%s\n' "$name" >> "$TMP_RUNNABLE.add"
        added=$((added + 1))
      fi
    done
    if [ "$added" -eq 0 ]; then
      echo "nothing to add: $INVENTORY already lists every ignored test"
    else
      # Insert before the frozen header so the sections stay meaningful.
      awk -v add="$TMP_RUNNABLE.add" '
        /^## frozen/ && !done { while ((getline line < add) > 0) print line; done=1 }
        { print }
        END { if (!done) { while ((getline line < add) > 0) print line } }
      ' "$INVENTORY" > "$INVENTORY.new"
      mv "$INVENTORY.new" "$INVENTORY"
      echo "added $added test(s) to $INVENTORY — move any that cannot be regenerated"
      echo "from HEAD into the '## frozen' section by hand."
    fi
    rm -f "$TMP_RUNNABLE.add"
    ;;

  --verify-list)
    got="$(listed)"
    want="$(expected)"
    if [ "$got" = "$want" ]; then
      echo "corpus gate inventory: ok ($(printf '%s\n' "$want" | wc -l | tr -d ' ') tests)"
      exit 0
    fi
    echo "corpus gate inventory: DRIFT" >&2
    echo >&2
    comm -13 <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
      | sed 's/^/  ignored but not in the inventory: /' >&2
    comm -23 <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
      | sed 's/^/  in the inventory but not ignored: /' >&2
    echo >&2
    echo "  If the change was intended: tools/corpus-gate.sh --update-list" >&2
    exit 1
    ;;

  run)
    target="${LEAN_DOC_TARGET:-/Users/haruka/dev/lean-projects}"
    echo "== what this machine has"
    printf '  %-24s %s\n' "target" "$target $([ -d "$target" ] && echo '(present)' || echo '(MISSING)')"
    for var in LEAN_DOC_IR LEAN_DOC_BASE_IR LEAN_DOC_DOCGEN4_TREE LEAN_DOC_LINK_INDEX \
               LEAN_DOC_REF_PAGES LEAN_DOC_REFERENCE_PAGES LEAN_DOC_REFERENCE_GLOBAL \
               LEAN_DOC_PAGES LEAN_DOC_SITE LEAN_DOC_PROTOTYPE_STATE LEAN_DOC_MERGE_FIXTURES \
               LEAN_DOC_DECL_URLS LEAN_DOC_AUTOLINK_FULL LEAN_DOC_FRAGMENT_FULL \
               LEAN_DOC_PAGE_PARTS_FULL LEAN_DOC_DOCGEN4_FULL LEAN_DOC_MD4LEAN_FULL; do
      value="${!var:-}"
      if [ -n "$value" ]; then
        printf '  %-24s %s %s\n' "$var" "$value" "$([ -e "$value" ] && echo '(present)' || echo '(MISSING)')"
      else
        printf '  %-24s %s\n' "$var" "(unset — the test's default path is used)"
      fi
    done
    echo
    frozen_list="$(frozen)"
    if [ -n "$frozen_list" ]; then
      echo "== not run: no regenerator in HEAD (tag experiments-frozen)"
      printf '%s\n' "$frozen_list" | sed 's/^/  /'
      echo
    fi

    echo "== the tests"
    # A missing input is a failure here, not a skip: these tests panic naming the
    # variable they wanted. That is the whole point of the move to #[ignore].
    #
    # Each runnable test is named explicitly rather than passing `--ignored`
    # alone, so that the frozen ones do not turn this gate permanently red.
    # Names, deduplicated: `the_corpus_matches_the_prototype` exists in three
    # crates and one `--exact` run covers all three. (If a frozen test ever
    # shares a name with a runnable one it will be pulled in here and the gate
    # will go red — which is the right way round: the alternative is running the
    # wrong test set silently.)
    status=0
    for test_name in $(runnable | sed 's/.*:://' | sort -u); do
      echo "-- $test_name"
      (cd "$ROOT" && cargo test --workspace -- --ignored --exact "$test_name" --nocapture) || status=1
    done
    exit "$status"
    ;;

  *)
    echo "usage: corpus-gate.sh [--verify-list | --list | --update-list]" >&2
    exit 2
    ;;
esac
