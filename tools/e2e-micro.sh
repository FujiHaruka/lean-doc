#!/usr/bin/env bash
# End to end, on a machine that has never seen the measurement target.
#
# WHAT THIS COVERS THAT `cargo test` CANNOT
#   Every test under `crates/litedoc4/tests/` fakes the extractor with a
#   `/bin/sh` script that copies a baked IR tree — deliberately, because needing
#   a Lean toolchain would mean those tests were never run. The cost is that the
#   **contract between the extractor and the Rust side is not checked by
#   anything**: change what `Extract.lean` writes and every one of them stays
#   green. This script is the one place where a real Lean environment produces a
#   real IR and the real pipeline turns it into a real site.
#
# WHY THE FIXTURE IS TINY AND MATHLIB-FREE
#   `e2e/micro` depends on Lean core and nothing else, so `lake build` takes
#   about a second and this runs on a free CI runner. The measurement target
#   pulls in all of Mathlib and can never be what a push is judged by.
#
#   It also holds, on purpose, the declaration shapes the target does not
#   contain — `class`, `inductive`, `class inductive`, a non-`mk` constructor,
#   an inherited field, an implicit binder on a field, an astral identifier
#   (U+1D49C, the U1/U2 traps), scoped notation. Nine of the renderer's 41
#   branches never fire over the real package (crates/litedoc4-render/tests/
#   page_parts.rs), and the first run of this script found one of them silently
#   rendering nothing: an inductive's constructors were missing from their page
#   while the search index still linked to them.
#
# THE GATES
#   1 ONE COMMAND    `litedoc4 build` over the fixture writes a site, and
#                    `tools/site-gate.sh` finds it internally consistent:
#                    0 dead links, 0 external resources, and the search index
#                    and the pages agreeing in both directions.
#   2 IDEMPOTENCE    the same command again, nothing changed: not one byte of
#                    the site different. The incremental path and the full path
#                    have to agree about a world that did not move.
#   3 DETERMINISM    a *second* full build into a different directory is byte
#                    identical to the first. Anything that leaks a hash order, a
#                    timestamp or a path into the output breaks here — and this
#                    is the invariant that replaces an external oracle, because
#                    it needs nobody's opinion about what the bytes should be.
#   4 --jobs         the extractor's parallelism changes neither the IR nor the
#                    site.
#   5 WORK           how much the two runs *did*, read out of
#                    `litedoc4-build.json`'s `work` record.
#   6 ONE EDIT       what a one-declaration edit costs, and that the tree it
#                    leaves is what a whole render of its own IR writes.
#   7 SORRY          the three shapes of doc-gen4 #270 — a direct `sorry`, a
#                    declaration that only depends on one, and neither — come
#                    out of the extractor as three different answers, by name.
#
# WHY GATE 5 EXISTS, AND WHY IT IS NOT A STOPWATCH
#   This project's product is speed and it had no regression gate at all. It
#   cannot have a wall-clock one: the oleans are mmap'ed, so the same unchanged
#   run's environment load moves by 5x with the page cache (2.5 s ↔ 13 s
#   【実測】, CLAUDE.md). A threshold over seconds is either loose enough to pass
#   a regression or tight enough to fail a cold runner.
#
#   What is decidable is the *work*: deterministic integers that do not care what
#   the machine felt like doing. GATE 5 asserts the shape the whole incremental
#   design exists to produce — a second run over an unchanged package
#   re-extracts nothing, renders nothing, and starts Lean not at all — and it
#   asserts that the three full builds did **identical** work, which is GATE 3's
#   determinism claim stated about the pipeline rather than about its output.
#
#   It reads JSON rather than grepping the log on purpose. GATE 2 used to check
#   the counters with `grep -qE '^render +modules [1-9]'`, which passes silently
#   the day that line is reworded — a gate whose failure mode is "quietly stops
#   testing" is worse than none.
#
# usage: e2e-micro.sh [--out DIR] [--extractor BIN] [--keep]
#   --out        where to build (default: a temporary directory)
#   --extractor  a prebuilt extractor binary (default: build one into
#                e2e/micro/.lake/e2e-extract, which is gitignored)
#   --keep       do not delete a temporary --out on success
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
FIXTURE="$ROOT/e2e/micro"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
LITEDOC4="${LITEDOC4:-$ROOT/target/debug/litedoc4}"

OUT=""
EXTRACTOR=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --extractor) EXTRACTOR="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    -h|--help) sed -n '1,73p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

command -v "$LAKE" >/dev/null 2>&1 || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
[ -x "$LITEDOC4" ] || { echo "no litedoc4 at $LITEDOC4 — cargo build --bin litedoc4" >&2; exit 2; }

if [ -z "$OUT" ]; then
  OUT="$(mktemp -d)"
  TEMPORARY=1
else
  mkdir -p "$OUT"
  TEMPORARY=0
fi

say() { printf '\n=== %s\n' "$1"; }

say "1/10 build the fixture package (Lean core only)"
(cd "$FIXTURE" && "$LAKE" build)

say "2/10 build the extractor inside the fixture's environment"
# The extractor is `import Lean` and nothing else, which is what lets it be
# built against a package that has no Mathlib. `-rdynamic` is load-bearing:
# `importModules (loadExts := true)` resolves symbols in the running executable
# through the Lean interpreter (extractor/build.sh says the same thing).
if [ -z "$EXTRACTOR" ]; then
  EXTRACTOR="$FIXTURE/.lake/e2e-extract/extract"
  # Rebuilt when the source is newer, not only when the binary is missing. The
  # first version of this reused whatever was in `.lake/e2e-extract`, which means
  # every gate below could pass against an extractor built before the change
  # under test — "the contract between the extractor and Rust" is the one thing
  # this script exists to check, and a stale binary makes it check nothing.
  if [ ! -x "$EXTRACTOR" ] || [ "$ROOT/extractor/Extract.lean" -nt "$EXTRACTOR" ]; then
    mkdir -p "$FIXTURE/.lake/e2e-extract"
    (cd "$FIXTURE" && "$LAKE" env lean --root="$ROOT/extractor" \
      -o "$FIXTURE/.lake/e2e-extract/Extract.olean" \
      -c "$FIXTURE/.lake/e2e-extract/Extract.c" \
      "$ROOT/extractor/Extract.lean")
    (cd "$FIXTURE" && "$LAKE" env leanc -rdynamic \
      -o "$EXTRACTOR" "$FIXTURE/.lake/e2e-extract/Extract.c")
  else
    echo "reusing $EXTRACTOR"
  fi
fi

say "3/10 GATE 1 — one command"
rm -rf "$OUT/first"
"$LITEDOC4" build --root "$FIXTURE" --lib Micro --out "$OUT/first" \
  --extractor-bin "$EXTRACTOR" | tee "$OUT/first.log"
[ -f "$OUT/first/site/index.html" ] || { echo "no site was written" >&2; exit 1; }

"$HERE/site-gate.sh" "$OUT/first/site"

# Snapshot *before* the second run touches the same directory. Comparing the
# tree with a copy taken afterwards compares it with itself, which passes
# whatever happens — the first version of this script did exactly that and its
# GATE 2 was checking nothing at all.
rm -rf "$OUT/first-snapshot"
cp -R "$OUT/first/site" "$OUT/first-snapshot"
# The marker too: the second run overwrites it in place, and its `work` record is
# half of GATE 5. Snapshotting it here is the same lesson the site snapshot above
# was learned from — a copy taken afterwards is a copy of the wrong run.
cp "$OUT/first/litedoc4-build.json" "$OUT/first-build.json"

say "4/10 GATE 2 — the second run changes nothing"
"$LITEDOC4" build --root "$FIXTURE" --lib Micro --out "$OUT/first" \
  --extractor-bin "$EXTRACTOR" | tee "$OUT/second.log"

# Bytes only. What the run *did* is GATE 5, out of the marker — a diff has to be
# reported as a diff rather than as whatever the log happened to say, and the
# counters have to be read from a record rather than grepped out of prose.
if ! diff -r "$OUT/first-snapshot" "$OUT/first/site"; then
  echo "the second run changed the site" >&2
  exit 1
fi
if ! grep -qE 'incremental|0 module\(s\)|nothing to' "$OUT/second.log"; then
  echo "note: the second run's log does not name an incremental path:" >&2
  sed -n '1,20p' "$OUT/second.log" >&2
fi

say "5/10 GATE 3 — a second full build is byte identical"
rm -rf "$OUT/again"
"$LITEDOC4" build --root "$FIXTURE" --lib Micro --out "$OUT/again" \
  --extractor-bin "$EXTRACTOR" >"$OUT/again.log"
if ! diff -r "$OUT/first/site" "$OUT/again/site"; then
  echo "two full builds of the same world disagree — determinism is broken" >&2
  exit 1
fi
# The IR too: the site could agree while the tree it came from does not.
if ! diff -r "$OUT/first/ir" "$OUT/again/ir"; then
  echo "two extractions of the same world disagree" >&2
  exit 1
fi

say "6/10 GATE 4 — --jobs does not change the output"
# The extractor splits declarations across threads inside one environment
# (approach.md §5.1). That the IR comes out identical was measured once at stage
# 7d; that the *site* does has never been checked, and a parallel step that
# reorders its output is exactly the kind of thing that shows up as a diff on one
# machine and not another.
rm -rf "$OUT/jobs4"
"$LITEDOC4" build --root "$FIXTURE" --lib Micro --out "$OUT/jobs4" \
  --extractor-bin "$EXTRACTOR" --jobs 4 >"$OUT/jobs4.log"
if ! diff -r "$OUT/first/ir" "$OUT/jobs4/ir"; then
  echo "--jobs 4 extracted a different IR than --jobs 1" >&2
  exit 1
fi
if ! diff -r "$OUT/first/site" "$OUT/jobs4/site"; then
  echo "--jobs 4 rendered a different site than --jobs 1" >&2
  exit 1
fi

say "7/10 GATE 5 — the work, as integers"
# Four markers: the first full build (snapshotted before the second run
# overwrote it), the incremental run over an unchanged world, and the two other
# full builds. Python because this repository's other gates use it and because a
# nested JSON record is not a thing to take apart with `sed`.
python3 - \
  "$OUT/first-build.json" \
  "$OUT/first/litedoc4-build.json" \
  "$OUT/again/litedoc4-build.json" \
  "$OUT/jobs4/litedoc4-build.json" <<'PY'
import json
import sys

full_path, incr_path, again_path, jobs4_path = sys.argv[1:5]
problems = []


def load(path):
    with open(path) as handle:
        marker = json.load(handle)
    # `complete: false` writes `work: null` on purpose (crates/litedoc4/src/
    # build.rs): a half-finished run's zeros are indistinguishable from a
    # successful incremental run's, so the marker refuses to look like one.
    if marker.get("complete") is not True:
        sys.exit(f"{path}: complete is {marker.get('complete')!r}, not true")
    work = marker.get("work")
    if not isinstance(work, dict):
        sys.exit(f"{path}: no `work` record ({work!r})")
    return marker, work


full_marker, full = load(full_path)
incr_marker, incr = load(incr_path)
again_marker, again = load(again_path)
jobs4_marker, jobs4 = load(jobs4_path)

modules = full_marker["modules"]
if modules < 1:
    sys.exit(f"{full_path}: {modules} module(s) — the fixture is empty")


def want(label, record, key, expected):
    got = record
    for part in key.split("."):
        got = got[part]
    if got != expected:
        problems.append(f"{label}: work.{key} is {got}, expected {expected}")


# 1 -- the first run does everything. Not a floor but an equality: a full
#      generation that extracted or rendered *fewer* modules than the package has
#      left something out, and one that did more counted something twice.
want("full", full, "modulesExtracted", modules)
want("full", full, "pagesRendered", modules)
want("full", full, "extractorRequests", 1)
# From-scratch, so nothing can be cached yet — and a cache that suddenly hits on
# a run with no previous state is a cache reading somebody else's state.
want("full", full, "globalCacheHits", 0)
want("full", full, "globalCacheMisses", modules)

# 2 -- the second run, over a world that did not move, does nothing. THIS IS THE
#      GATE. Every one of these three zeros is the incremental path's whole
#      reason to exist, and `extractorRequests` is the sharpest: its zero says
#      Lean was never started.
want("incremental", incr, "modulesExtracted", 0)
want("incremental", incr, "pagesRendered", 0)
want("incremental", incr, "extractorRequests", 0)
want("incremental", incr, "globalCacheHits", modules)
want("incremental", incr, "globalCacheMisses", 0)

# 3 -- and it reads less of the IR than a full build does. The count is not
#      pinned to a number here (approach.md §5.6 owns that claim, and pinning it
#      would make this script the place a deliberate change has to be argued);
#      what is pinned is the direction, which no correct change reverses.
full_reads = full["irReads"]["module"]
incr_reads = incr["irReads"]["module"]
if full_reads < modules:
    problems.append(
        f"full: work.irReads.module is {full_reads} for {modules} module(s) — "
        "a full build reads every module at least once"
    )
if incr_reads >= full_reads:
    problems.append(
        f"incremental: work.irReads.module is {incr_reads}, not fewer than the "
        f"full build's {full_reads} — the incremental path stopped saving IR reads"
    )

# 4 -- the same world costs the same work. GATE 3 says two full builds produce
#      the same bytes; this says they did the same amount of work to get there,
#      which is the half that would otherwise be free to double silently.
for label, other in (("again", again), ("--jobs 4", jobs4)):
    if other != full:
        problems.append(
            f"{label}: did different work than the first full build\n"
            f"    first: {json.dumps(full, sort_keys=True)}\n"
            f"    {label}: {json.dumps(other, sort_keys=True)}"
        )

for line in ("full", full), ("incremental", incr):
    print(f"{line[0]:12} {json.dumps(line[1], sort_keys=True)}")
passes = full["irReads"]["module"] / modules
print(f"{'ir passes':12} full {passes:.2f}  incremental {incr_reads / modules:.2f}")

if problems:
    for problem in problems:
        print(f"GATE 5 FAIL  {problem}", file=sys.stderr)
    sys.exit(1)
PY

say "8/10 GATE 7 — the three sorry shapes are three different answers"
# doc-gen4 #270 asks for two claims and not one: a declaration that uses `sorry`
# itself, and a declaration that merely depends on such a one. `Micro/Sorry.lean`
# holds one of each plus a control, and **this is the only place the extractor's
# answer meets a real Lean environment**: `sorry` is a property of the elaborated
# term, so a hand-written IR fixture can check what the renderer does with the
# key but never that the extractor put the right value there.
#
# Reads the IR rather than the site on purpose — bundle B is extraction and IR
# only, and no page shows this yet.
python3 - "$OUT/first/ir" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
index = json.loads((root / "index.json").read_text(encoding="utf-8"))

# `sorry` absent means "no sorry" **because the file says schema 5**. In a
# schema-4 file the key could not exist at all, so the same absence would mean
# "nobody was asked" — reading one as the other is the whole reason
# litedoc4-ir's MIN_SCHEMA_VERSION moved with the writer.
if index.get("schemaVersion") != 5:
    sys.exit(f"{root}/index.json: schemaVersion is {index.get('schemaVersion')!r}, not 5")

expected = {
    "Micro.Sorry.sorryHole": "direct",
    "Micro.Sorry.usesHole": "transitive",
    "Micro.Sorry.noHole": None,
}
found = {}
for entry in index["modules"]:
    module = json.loads((root / entry["file"]).read_text(encoding="utf-8"))
    if module.get("schemaVersion") != 5:
        sys.exit(f"{entry['file']}: schemaVersion is {module.get('schemaVersion')!r}, not 5")
    for decl in module["declarations"]:
        found[decl["name"]] = decl.get("sorry")

problems = []
checked = 0
for name, want in sorted(expected.items()):
    if name not in found:
        problems.append(f"{name} is not in the IR at all — the fixture lost a shape")
        continue
    checked += 1
    got = found[name]
    if got != want:
        problems.append(f"{name}: sorry is {got!r}, expected {want!r}")

# The count, not the absence of complaints. An expectation that never ran
# reports nothing, and this script's own history is of gates that passed by not
# looking (see "GATE ITSELF WAS BROKEN" in e2e/README.md).
if checked != len(expected):
    problems.append(f"{checked} of {len(expected)} shapes were actually compared")

# `noHole` shows the key can be absent for one declaration; this shows it is not
# being sprayed over the package. A classifier that answers "transitive" to
# everything passes the two positive expectations above and fails here.
strays = sorted(name for name, value in found.items() if value is not None and name not in expected)
if strays:
    problems.append(f"{len(strays)} other declaration(s) claim a sorry: {', '.join(strays[:5])}")

if problems:
    for problem in problems:
        print(f"GATE 7 FAIL  {problem}", file=sys.stderr)
    sys.exit(1)

print(f"sorry        {checked} shapes compared over {len(found)} declarations: " +
      ", ".join(f"{name.rpartition('.')[2]}={found[name]}" for name in sorted(expected)))
PY

say "9/10 GATE 6 — one edited module does not re-render the package"
# The question the other five cannot ask. GATE 2 asks what an *unchanged* world
# costs; this asks what a one-declaration edit costs, which is the shape a user
# actually produces and the one where the dependency map used to force every page
# to be written again (`docs/plans/reextract-count.md` §6, 段 C).
#
# Three assertions, and the first is the sharp one:
#
#   the map does not move       `link-index.lidx` is byte-identical across the
#                               edit. It is the *cause*: its SHA-256 is a
#                               `renderKey` input (`litedoc4-incr/src/ledger.rs`
#                               `render_key`), and a moved `renderKey` overrides
#                               --mode to `all` (`litedoc4-incr/src/impact.rs`).
#                               This fails on any extractor that writes the
#                               package's own declarations into the map.
#   fewer pages than modules    the *effect*. An inequality rather than a number:
#                               the fixture's import graph is allowed to grow,
#                               and pinning "1" would make this script the place
#                               that argument has to happen.
#   the tree is a whole render  the *oracle*. Under-rendering is silent, so a
#                               page count on its own is not evidence. What the
#                               incremental run left on disk has to be what a
#                               whole render of its own IR writes.
PROBE="$FIXTURE/Micro/Basic.lean"
cp "$PROBE" "$OUT/probe.orig"
# `set -e` must not leave the fixture edited: everything below this line runs
# under a trap that puts the file back, including the failure paths.
# `if`, not `[ … ] && cp`: an EXIT trap's last command decides the script's exit
# status, and with a temporary --out this function runs *after* `$OUT` has been
# deleted, so the test is false and the `&&` form returned 1 — this script
# printed "E2E MICRO: ok" and exited 1 【実測 2026-08-18】. CI never saw it
# because it always passes `--out … --keep`. A failing `cp` still fails here.
restore_probe () {
  if [ -f "$OUT/probe.orig" ]; then cp "$OUT/probe.orig" "$PROBE"; fi
}
trap restore_probe EXIT

cp "$OUT/first/link-index.lidx" "$OUT/lidx-before"
printf '\n/-- A probe appended by GATE 6; removed before this script exits. -/\ndef e2eGate6Probe_ : Nat := 13\n' >> "$PROBE"
(cd "$FIXTURE" && "$LAKE" build)

"$LITEDOC4" build --root "$FIXTURE" --lib Micro --out "$OUT/first" \
  --extractor-bin "$EXTRACTOR" | tee "$OUT/edited.log"

restore_probe
(cd "$FIXTURE" && "$LAKE" build)

if ! cmp -s "$OUT/lidx-before" "$OUT/first/link-index.lidx"; then
  echo "GATE 6: link-index.lidx moved for a one-declaration edit" >&2
  /usr/bin/diff "$OUT/lidx-before" "$OUT/first/link-index.lidx" | head -6 >&2
  exit 1
fi

# `site` writes the pages and the global artefacts; the static assets `build`
# copies are not its business, so a name present on one side only is not a
# difference here — a *shared* name whose bytes differ is.
EDITED_URL="$(python3 - "$OUT/edited.log" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
found = re.search(r"^source  (\S+://\S+)$", text, re.M)
print(found.group(1) if found else "")
PY
)"
[ -n "$EDITED_URL" ] || { echo "GATE 6: no source URL in the edited run's log" >&2; exit 1; }
rm -rf "$OUT/gate6-oracle" "$OUT/gate6-state"
"$LITEDOC4" site --ir "$OUT/first/ir" --out "$OUT/gate6-oracle" \
  --source-url "$EDITED_URL" --link-index "$OUT/first/link-index.lidx" \
  --state "$OUT/gate6-state" --root "$FIXTURE" >"$OUT/gate6-oracle.log"
gate6_diff="$(/usr/bin/diff -r -q "$OUT/gate6-oracle" "$OUT/first/site" | grep -v '^Only in' || true)"
if [ -n "$gate6_diff" ]; then
  echo "GATE 6: the incremental tree is not what a whole render of its IR writes" >&2
  printf '%s\n' "$gate6_diff" | head -10 >&2
  exit 1
fi

# The integers, out of the one file that owns them. The same script runs on the
# Linux runner against the generated package (`ci-template.yml`), so what "a
# one-module edit is allowed to cost" is written down once and both callers get
# the same answer.
"$HERE/onemod-gate.sh" "$OUT/first/litedoc4-build.json" "$OUT/first/work/serve.out"

say "10/10 summary"
printf 'site files : %s\n' "$(find "$OUT/first/site" -type f | wc -l | tr -d ' ')"
printf 'ir files   : %s\n' "$(find "$OUT/first/ir" -type f | wc -l | tr -d ' ')"
printf 'out        : %s\n' "$OUT"

if [ "$TEMPORARY" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
  rm -rf "$OUT"
fi
echo
echo "E2E MICRO: ok"
