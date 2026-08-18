#!/usr/bin/env bash
# The tests that need the measurement target — run on purpose, never by accident.
#
# WHY THESE ARE NOT ORDINARY TESTS
#   A test owns its input. These do not: they read the 432-module package at
#   /Users/haruka/dev/lean-projects, its doc-gen4 reference tree, generated IR
#   trees and multi-megabyte `--full` recordings, none of which is in the
#   repository. `crates/litedoc4/tests/resident.rs` already draws this line —
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

# `<cargo test target> <the binary cargo built for it>`, from cargo's own output
# rather than by guessing at `target/debug/deps/*`: the hash suffix is cargo's
# and stale binaries from earlier builds sit in the same directory.
executables() {
  (cd "$ROOT" && cargo test --workspace --no-run --message-format=json 2>/dev/null) \
    | "$PYTHON" -c '
import json, os, sys

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
    print(os.path.basename(exe).rsplit("-", 1)[0], exe)
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
    target="${LITEDOC4_TARGET:-/Users/haruka/dev/lean-projects}"
    echo "== what this machine has"
    printf '  %-24s %s\n' "target" "$target $([ -d "$target" ] && echo '(present)' || echo '(MISSING)')"
    for var in LITEDOC4_IR LITEDOC4_BASE_IR LITEDOC4_DOCGEN4_TREE LITEDOC4_LINK_INDEX \
               LITEDOC4_REF_PAGES LITEDOC4_REFERENCE_PAGES LITEDOC4_REFERENCE_GLOBAL \
               LITEDOC4_PAGES LITEDOC4_SITE LITEDOC4_PROTOTYPE_STATE LITEDOC4_MERGE_FIXTURES \
               LITEDOC4_DECL_URLS LITEDOC4_AUTOLINK_FULL LITEDOC4_FRAGMENT_FULL \
               LITEDOC4_PAGE_PARTS_FULL LITEDOC4_DOCGEN4_FULL LITEDOC4_MD4LEAN_FULL; do
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
    # **One inventory entry, one test binary, one test.** The gate used to hand
    # `cargo test --workspace -- --exact NAME` the bare test name, which is wrong
    # in three ways that each hid tests rather than failing:
    #
    #   1. `--exact` matches the whole path a binary prints, so entries of the
    #      form `litedoc4::packages::tests::NAME` cut down to `NAME` matched
    #      nothing — and `cargo test` exits 0 when a filter selects no tests, so
    #      three tests were reported run, and green, without running.
    #   2. Without `--no-fail-fast`, cargo stopped at the first binary that
    #      failed, so a red test hid its namesakes in other crates.
    #   3. A name shared by a runnable entry and a frozen one pulls the frozen
    #      one in — which is exactly the case now that `impact::` is frozen and
    #      `merge::` is not, both being `the_corpus_matches_the_prototype`.
    #
    # Asking cargo for the binaries and running each **by target** removes all
    # three: the target prefix in the inventory is what disambiguates, and it is
    # in the inventory precisely because these names collide. The count below
    # stays as the guard — one entry has to report exactly one result.
    status=0
    ran_total=0
    want_total="$(runnable | wc -l | tr -d ' ')"
    EXES="$(mktemp)"
    trap 'rm -f "$TMP_RUNNABLE" "$TMP_FROZEN" "$EXES"' EXIT
    executables > "$EXES"
    while read -r entry; do
      target_name="${entry%%::*}"
      test_path="${entry#*::}"
      exe="$(awk -v t="$target_name" '$1 == t { print $2 }' "$EXES")"
      echo "-- $target_name :: $test_path"
      if [ -z "$exe" ]; then
        echo "   NO SUCH TEST TARGET: $target_name — the inventory names a binary cargo did not build" >&2
        status=1
        continue
      fi
      log="$(mktemp)"
      "$exe" --ignored --exact "$test_path" --nocapture > "$log" 2>&1 || status=1
      cat "$log"
      # `test result: ok. 1 passed; 0 failed; …` — one binary, so one line.
      ran="$(sed -n 's/^test result:[^0-9]*\([0-9]*\) passed; \([0-9]*\) failed.*/\1 \2/p' "$log" \
             | awk '{ total += $1 + $2 } END { print total + 0 }')"
      echo "   reported a result: $ran"
      if [ "$ran" -ne 1 ]; then
        echo "   NOT ONE TEST — $target_name has no test at $test_path" >&2
        status=1
      fi
      ran_total=$((ran_total + ran))
      rm -f "$log"
    done < <(runnable)
    echo
    echo "tests that reported a result: $ran_total (the inventory lists $want_total)"
    if [ "$ran_total" -ne "$want_total" ]; then
      echo "the gate did not run what it claims to run" >&2
      status=1
    fi
    exit "$status"
    ;;

  *)
    echo "usage: corpus-gate.sh [--verify-list | --list | --update-list]" >&2
    exit 2
    ;;
esac
