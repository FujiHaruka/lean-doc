#!/usr/bin/env bash
# L1 — is lean-doc usable as a Lake dependency?
#
# `docs/plans/lake-package.md` L1: a consumer writes `require «lean-doc»` and
# runs `lake run docs -- --out <dir>`. What that buys is two flags nobody can
# supply by hand — `--extractor-bin` (Lake builds the extractor against the
# consumer's toolchain) and `--lib` (read out of the elaborated workspace, which
# is the only honest way to read a `lakefile.lean`). This gate is where those two
# claims are checked.
#
# THE FIVE ITEMS
#   1 WIRED       `lake script list` in `e2e/consumer` offers `lean-doc/docs`.
#                 Falls: the dependency's lakefile.lean is not being loaded.
#   2 IT RUNS     `lake run docs -- --out <tmp>` writes a site. Falls: the
#                 arguments the script assembles are wrong.
#   3 IT CLOSES   `tools/site-gate.sh` finds that site internally consistent.
#                 Falls: the output is broken.
#   4 SAME IR     the extractor Lake builds and the one `extractor/build.sh`
#                 builds write **byte-identical IR** over `e2e/micro`. Falls: the
#                 two build paths have diverged.
#   5 --lib       the run above passed no `--lib`, and the site documents every
#                 library root the fixture declares. Falls: the Lake-side lookup
#                 broke.
#
# ITEM 4 IS THE BODY OF THE GATE
#   The Lake build is not byte-reproducible against `build.sh`'s: Lake prefixes
#   package-local symbols and compiles the generated C with `-O3 -DNDEBUG`, which
#   is +308,032 B 【実測 2026-08-18, benchmarks/results/lake-package-probe-
#   2026-08-18.txt §2】. So the oracle is not the binary but the **IR**, which is
#   what the rest of the product actually consumes. `extractor-uniqueness-2026-
#   08-18.txt` §6 wrote down its own limit — "「起動できる」と「正しい IR を書く」
#   は別" — and a build path that necessarily moves the bytes is precisely the
#   case it could not cover.
#
#   Two extractors that are the *same file* would make this item pass while
#   comparing nothing, so their digests are compared first and equal digests are
#   a failure with that sentence as the reason. The fixture is `e2e/micro` rather
#   than `e2e/consumer` because micro carries the declaration shapes the target
#   package does not have (`e2e/README.md`), and a comparison is only as good as
#   the shapes that reach it.
#
# WHAT IS NEVER TOUCHED
#   /Users/haruka/dev/lean-projects — the measurement target — has nothing to do
#   with this and is not read. Everything written goes under $OUT, under
#   `e2e/micro/.lake/` or under `<repo>/.lake/`, all three of which are
#   gitignored build output.
#
# usage: lake-package-gate.sh [--out DIR] [--keep]
#   --out   working directory (default: a temporary one, removed on success)
#   --keep  do not remove a temporary --out
#
#   LEAN_DOC  the `lean-doc` executable under test (default: target/debug/lean-doc)
#   LAKE      the lake executable (default: ~/.elan/bin/lake)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
FIXTURE="$ROOT/e2e/consumer"
MICRO="$ROOT/e2e/micro"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
LEAN_DOC="${LEAN_DOC:-$ROOT/target/debug/lean-doc}"
# `diff` is aliased to a colordiff that is not installed on this machine, and its
# exit 127 reads as "differences found". Always the real one.
DIFF=/usr/bin/diff

OUT=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    -h|--help) sed -n '1,53p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

# No input, no gate: every one of these is a hard exit rather than a skip. A
# `return 0` here is the failure shape this project has already paid for —
# `eprintln!("skipping: …")` does not reach the exit code (CLAUDE.md).
[ -x "$LAKE" ] || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
[ -x "$LEAN_DOC" ] || { echo "no lean-doc at $LEAN_DOC — cargo build --bin lean-doc" >&2; exit 2; }
[ -f "$ROOT/lakefile.lean" ] || { echo "no $ROOT/lakefile.lean — this gate has nothing to check" >&2; exit 2; }
[ -f "$FIXTURE/lakefile.toml" ] || { echo "no consumer fixture at $FIXTURE" >&2; exit 2; }
[ -f "$MICRO/lakefile.toml" ] || { echo "no micro fixture at $MICRO" >&2; exit 2; }

if [ -z "$OUT" ]; then
  OUT="$(mktemp -d)"
  TEMPORARY=1
else
  mkdir -p "$OUT"
  TEMPORARY=0
fi

# Every item reports exactly once, and the count is compared with the inventory
# at the end. "The gate ran" and "the gate looked at something" are different
# claims, and this project has been caught believing the first one four separate
# ways in one day (CLAUDE.md 「ゲートは『走った本数』を数える」).
ITEMS=5
ran=0
failed=0

pass () { ran=$((ran + 1)); printf 'ITEM %s ok    %s\n' "$1" "$2"; }
fail () { ran=$((ran + 1)); failed=$((failed + 1)); printf 'ITEM %s FAIL  %s\n' "$1" "$2" >&2; }
say  () { printf '\n=== %s\n' "$1"; }

# ---------------------------------------------------------------------------
say "1/5 the script is wired up"
# ---------------------------------------------------------------------------
if (cd "$FIXTURE" && "$LAKE" script list) >"$OUT/script-list.txt" 2>"$OUT/script-list.err"; then
  if grep -qx 'lean-doc/docs' "$OUT/script-list.txt"; then
    pass 1 "lake script list offers lean-doc/docs"
  else
    fail 1 "lake script list in $FIXTURE does not offer \`lean-doc/docs\` — the dependency's lakefile.lean declares no such script, or is not being loaded"
    sed -n '1,20p' "$OUT/script-list.txt" >&2
  fi
else
  fail 1 "lake script list failed in $FIXTURE — the workspace does not load"
  sed -n '1,20p' "$OUT/script-list.err" >&2
fi

# ---------------------------------------------------------------------------
say "2/5 lake run docs writes a site"
# ---------------------------------------------------------------------------
# **No `--lib` and no `--extractor-bin`.** That is the whole point: the script
# has to get both out of Lake. `LEAN_DOC_BIN` is source 1 of `resolveLeanDoc`
# and pins which Rust binary is under test — without it the gate would grade
# whatever happens to be on PATH.
SITE_OUT="$OUT/docs"
rm -rf "$SITE_OUT"
site_ok=0
if (cd "$FIXTURE" && LEAN_DOC_BIN="$LEAN_DOC" "$LAKE" run docs -- --out "$SITE_OUT") \
    >"$OUT/docs.log" 2>&1; then
  if [ -f "$SITE_OUT/site/index.html" ] && [ -f "$SITE_OUT/site/modules.json" ]; then
    pass 2 "$(wc -l <"$OUT/docs.log" | tr -d ' ') line(s) of log, site at $SITE_OUT/site"
    site_ok=1
  else
    fail 2 "lake run docs exited 0 but wrote no site: $SITE_OUT/site/index.html is missing"
  fi
else
  fail 2 "lake run docs failed in $FIXTURE — see $OUT/docs.log"
  tail -20 "$OUT/docs.log" >&2
fi

# ---------------------------------------------------------------------------
say "3/5 the site closes over itself"
# ---------------------------------------------------------------------------
if [ "$site_ok" -eq 1 ]; then
  if "$HERE/site-gate.sh" "$SITE_OUT/site" >"$OUT/site-gate.log" 2>&1; then
    pass 3 "site-gate.sh: 0 dead links, index and pages agree both ways"
  else
    fail 3 "site-gate.sh rejected the site lake run docs wrote — see $OUT/site-gate.log"
    tail -20 "$OUT/site-gate.log" >&2
  fi
else
  # Not skipped: item 2 already failed, and an item that quietly disappears is
  # how a gate stops testing without anybody noticing.
  fail 3 "no site to check — item 2 did not write one"
fi

# ---------------------------------------------------------------------------
say "4/5 both extractors write the same IR over e2e/micro"
# ---------------------------------------------------------------------------
IR="$OUT/ir"
mkdir -p "$IR"

# (a) the extractor Lake builds. Built here rather than taken from item 2 so
#     that this item fails on its own terms when the target is gone.
(cd "$FIXTURE" && "$LAKE" build lean-doc/extract) >"$OUT/lake-build.log" 2>&1 \
  || { echo "lake build lean-doc/extract failed — see $OUT/lake-build.log" >&2; tail -20 "$OUT/lake-build.log" >&2; }
LAKE_EXTRACT="$ROOT/.lake/build/bin/extract"

# (b) the extractor `extractor/build.sh` builds, in the two steps that script
#     uses, inside the fixture's environment (`tools/e2e-micro.sh` step 2 builds
#     it exactly this way). Rebuilt when the source is newer, not only when the
#     binary is missing: a stale binary would make this item compare a change
#     against itself.
MANUAL_EXTRACT="$MICRO/.lake/e2e-extract/extract"
if [ ! -x "$MANUAL_EXTRACT" ] || [ "$ROOT/extractor/Extract.lean" -nt "$MANUAL_EXTRACT" ]; then
  mkdir -p "$MICRO/.lake/e2e-extract"
  (cd "$MICRO" && "$LAKE" env lean --root="$ROOT/extractor" \
    -o "$MICRO/.lake/e2e-extract/Extract.olean" \
    -c "$MICRO/.lake/e2e-extract/Extract.c" \
    "$ROOT/extractor/Extract.lean") >"$OUT/manual-build.log" 2>&1
  (cd "$MICRO" && "$LAKE" env leanc -rdynamic \
    -o "$MANUAL_EXTRACT" "$MICRO/.lake/e2e-extract/Extract.c") >>"$OUT/manual-build.log" 2>&1
fi

# The extractor needs the fixture's own oleans (probe §3).
(cd "$MICRO" && "$LAKE" build) >"$OUT/micro-build.log" 2>&1

# The same six flags `crates/lean-doc/src/extract.rs` fixes, in the same order:
#   <bin> <modules> <events> --equations --refs --write-ir --tagged-code
#         --jobs 1 --ir-dir <dir> --link-index <file>
run_extractor () { # $1 binary  $2 label
  (cd "$MICRO" && "$LAKE" env "$1" \
    "$IR/modules.txt" "$IR/$2-events.jsonl" \
    --equations --refs --write-ir --tagged-code \
    --jobs 1 --ir-dir "$IR/$2" --link-index "$IR/$2.lidx") >"$IR/$2.log" 2>&1
}

if [ ! -x "$LAKE_EXTRACT" ] || [ ! -x "$MANUAL_EXTRACT" ]; then
  fail 4 "missing an extractor to compare: lake=$LAKE_EXTRACT manual=$MANUAL_EXTRACT"
else
  lake_digest="$(shasum -a 256 "$LAKE_EXTRACT" | cut -d' ' -f1)"
  manual_digest="$(shasum -a 256 "$MANUAL_EXTRACT" | cut -d' ' -f1)"
  if [ "$lake_digest" = "$manual_digest" ]; then
    # Not a happy coincidence to wave through: the two builds differ by
    # construction (+308,032 B 【実測, probe §2】), so identical bytes mean one
    # binary has been copied over the other and this item is comparing an
    # extractor with itself.
    fail 4 "the two extractors are the same bytes ($lake_digest) — this item would compare one extractor with itself"
  else
    "$LEAN_DOC" modules --root "$MICRO" --out "$IR/modules.txt" >"$IR/modules.log" 2>&1
    modules="$(grep -cvE '^[[:space:]]*(#|$)' "$IR/modules.txt" || true)"
    if [ "${modules:-0}" -lt 1 ]; then
      fail 4 "no modules to extract from $MICRO — see $IR/modules.log"
    else
      rm -rf "$IR/lake" "$IR/manual"
      lake_rc=0; run_extractor "$LAKE_EXTRACT" lake || lake_rc=$?
      manual_rc=0; run_extractor "$MANUAL_EXTRACT" manual || manual_rc=$?
      if [ "$lake_rc" -ne 0 ] || [ "$manual_rc" -ne 0 ]; then
        fail 4 "an extractor exited non-zero (lake=$lake_rc manual=$manual_rc) — see $IR/lake.log and $IR/manual.log"
      else
        ir_files="$(find "$IR/lake" -type f | wc -l | tr -d ' ')"
        if "$DIFF" -r "$IR/lake" "$IR/manual" >"$IR/diff.txt" 2>&1 \
            && cmp -s "$IR/lake.lidx" "$IR/manual.lidx"; then
          pass 4 "$modules module(s), $ir_files IR file(s) + link-index identical across both builds (lake $lake_digest / manual $manual_digest)"
        else
          fail 4 "the Lake-built extractor and build.sh's wrote different IR over $MICRO — see $IR/diff.txt"
          head -20 "$IR/diff.txt" >&2
          cmp "$IR/lake.lidx" "$IR/manual.lidx" >&2 || true
        fi
      fi
    fi
  fi
fi

# ---------------------------------------------------------------------------
say "5/5 --lib came from Lake, not from the caller"
# ---------------------------------------------------------------------------
# The fixture declares two `lean_lib`s and puts only one of them in
# `defaultTargets` (`e2e/consumer/lakefile.toml` says why). So this is not the
# tautology "the flag we passed came out again": nothing above passed `--lib`,
# and a script that read `defaultTargets`, took the first library, or used the
# package name would produce a *shorter* site — the exact failure shape
# `crates/lean-doc/src/lakefile.rs` is built against.
#
# Two assertions, cause and effect. The command line is the cause; the module
# index is the effect, and it is the one that cannot be satisfied by printing.
EXPECTED_LIBS="Consumer ConsumerExtra"
EXPECTED_MODULES="Consumer Consumer.Basic ConsumerExtra"
if [ "$site_ok" -eq 1 ]; then
  got_libs="$(python3 - "$OUT/docs.log" <<'PY'
import sys

# The one line the script prints before spawning: `lean-doc: <bin> build …`.
# Read out of the log rather than reconstructed, because what is being checked
# is the command line that actually ran.
line = ""
with open(sys.argv[1], encoding="utf-8") as handle:
    for text in handle:
        if text.startswith("lean-doc: ") and " build " in text:
            line = text
words = line.split()
print(" ".join(word for before, word in zip(words, words[1:]) if before == "--lib"))
PY
)"
  got_modules="$(python3 - "$SITE_OUT/site/modules.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    index = json.load(handle)
print(" ".join(sorted(entry["n"] for entry in index["modules"])))
PY
)"
  if [ "$got_libs" != "$EXPECTED_LIBS" ]; then
    fail 5 "the script passed --lib [$got_libs], not [$EXPECTED_LIBS] — the workspace lookup no longer returns every library root"
  elif [ "$got_modules" != "$EXPECTED_MODULES" ]; then
    fail 5 "the site documents [$got_modules], not [$EXPECTED_MODULES] — a library root was resolved but not documented"
  else
    pass 5 "no --lib was passed in; the script derived [$got_libs] and the site has [$got_modules]"
  fi
else
  fail 5 "no run to inspect — item 2 did not produce one"
fi

# ---------------------------------------------------------------------------
say "summary"
# ---------------------------------------------------------------------------
printf 'items reported : %s of %s\n' "$ran" "$ITEMS"
printf 'failed         : %s\n' "$failed"
printf 'out            : %s\n' "$OUT"

if [ "$ran" -ne "$ITEMS" ]; then
  echo "LAKE PACKAGE GATE: FAILED — $ran of $ITEMS items reported a result; the rest never ran" >&2
  exit 1
fi
if [ "$failed" -ne 0 ]; then
  echo "LAKE PACKAGE GATE: FAILED ($failed of $ITEMS)" >&2
  exit 1
fi

if [ "$TEMPORARY" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
  rm -rf "$OUT"
fi
echo
echo "LAKE PACKAGE GATE: ok"
