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
#   8 ATTRS          every kind of attribute the extractor collects arrives in
#                    the IR as a `[name, value]` pair, split where the extractor
#                    knows the boundary and nowhere else.
#   9 GENERATED      the declarations `@[ext]` realizes name what they were
#                    realized from, by name and one step; the hand-written ext
#                    theorem sitting in the same environment extension does not;
#                    and the 33 declarations whose two ranges are equal without
#                    being realized keep the rule from collapsing into range
#                    equality.
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
    -h|--help) sed -n '1,82p' "$0"; exit 0 ;;
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

say "1/14 build the fixture package (Lean core only)"
(cd "$FIXTURE" && "$LAKE" build)

say "2/14 build the extractor inside the fixture's environment"
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

say "3/14 GATE 1 — one command"
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

say "4/14 GATE 2 — the second run changes nothing"
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

say "5/14 GATE 3 — a second full build is byte identical"
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

say "6/14 GATE 4 — --jobs does not change the output"
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

say "7/14 GATE 5 — the work, as integers"
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

say "8/14 GATE 7 — the three sorry shapes are three different answers"
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

say "9/14 GATE 8 — attributes arrive split into name and value"
# Schema 5 carries each attribute as a two-element `[name, value]` array where
# schema 4 carried one concatenated string (`docs/plans/feature-sweep.md` B-2).
# The split is made in the extractor because that is the only side that knows
# where the boundary is: `deprecated`'s value contains spaces, parentheses and
# quotes, `specialize`'s contains brackets, and a reader given the concatenation
# would have to guess.
#
# `Micro/Attrs.lean` holds one declaration per *kind* of attribute the four
# collectors produce. The measurement target has none of the hard shapes — 163
# occurrences over 6 distinct strings, all bare names but one `deprecated`
# 【実測 2026-08-21】 — so this fixture is where they exist at all.
#
# Reads the IR, not the site: bundle B is extraction and IR only, and the page
# still prints the rejoined string. Runs before GATE 6, which appends a probe
# declaration to the fixture and rebuilds it.
#
# THREE THINGS, AND THE LAST TWO ARE WHY THE FIRST IS NOT ENOUGH
#   the pairs        each named declaration's `attrs`, compared whole. Positive
#                    expectations, and the count of them is asserted for the
#                    reason GATE 7 gives: an expectation that never ran reports
#                    nothing.
#   the shape        *every* attrs entry in the whole IR is two strings. One
#                    collector left writing a concatenated string is invisible to
#                    the expectations above if it is not one of theirs.
#   the counts       how many declarations claim each attribute name, over the
#                    whole IR. This is GATE 7's stray check in the form this
#                    field needs: attributes are not rare here, so "nobody else
#                    claims one" is false, and a collector that answered `simp`
#                    for everything would sail past two positive expectations.
#                    The numbers are this fixture's; adding a declaration with an
#                    attribute means updating one, and the gate names which.
python3 - "$OUT/first/ir" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
index = json.loads((root / "index.json").read_text(encoding="utf-8"))
if index.get("schemaVersion") != 5:
    sys.exit(f"{root}/index.json: schemaVersion is {index.get('schemaVersion')!r}, not 5")

# Exact and ordered. Order is doc-gen4's `customs ++ tags ++ enums ++
# parametric`, with the instance attributes appended after all four, and it is
# what the printed `@[a, b]` line looks like — so it is part of the answer.
DEPRECATED_VALUE = 'Micro.Attrs.scale (since := "2026-08-21")'
expected = {
    # getCustomAttrs — the simp extension, and the reducibility status
    "Micro.Attrs.scale_zero": [["simp", ""]],
    "Micro.Attrs.Weight": [["reducible", ""]],
    # getTags — a tag attribute has no value at all
    "Micro.Attrs.zero": [["match_pattern", ""]],
    # getEnumValues — the enum's own name *is* the attribute
    "Micro.Attrs.scale": [["inline", ""]],
    # getParametricValues — the two that make the split necessary
    "Micro.Attrs.applyTwice": [["specialize", "#[]"]],
    "Micro.Attrs.scaleOld": [["deprecated", DEPRECATED_VALUE]],
    # InstanceInfo.ofDefinitionInfo — appended after the four collectors
    "Micro.Attrs.tinyNat": [["implicit_reducible", ""], ["instance", "100"]],
    "Micro.Attrs.tinyBool": [["implicit_reducible", ""], ["defaultInstance", "1000"]],
}

# Declarations claiming each attribute name, over every module of the fixture.
name_counts = {
    # 19 rather than 9 since `Micro/Gen.lean` arrived: every structure
    # projection is `@[reducible]`, and that module declares six structures.
    "reducible": 19,
    "implicit_reducible": 6,
    "inline": 2,
    "simp": 1,
    "match_pattern": 1,
    "specialize": 1,
    "deprecated": 1,
    "instance": 1,
    "defaultInstance": 1,
}
# Attributes that carry a value at all. A writer that put the whole
# concatenation in the name half would still produce well-shaped pairs and would
# still be caught by `name_counts`; this is the same claim said as one number,
# and it is the one that goes to zero if the value half is ever dropped.
VALUED = 4

problems = []
found = {}
malformed = []
counts = {}
valued = 0
for entry in index["modules"]:
    module = json.loads((root / entry["file"]).read_text(encoding="utf-8"))
    for decl in module["declarations"]:
        attrs = decl.get("attrs", [])
        found[decl["name"]] = attrs
        for attr in attrs:
            ok = (
                isinstance(attr, list)
                and len(attr) == 2
                and all(isinstance(part, str) for part in attr)
            )
            if not ok:
                malformed.append((decl["name"], attr))
                continue
            counts[attr[0]] = counts.get(attr[0], 0) + 1
            if attr[1]:
                valued += 1

checked = 0
for name, want in sorted(expected.items()):
    if name not in found:
        problems.append(f"{name} is not in the IR at all — the fixture lost a shape")
        continue
    checked += 1
    got = found[name]
    if got != want:
        problems.append(f"{name}: attrs are {json.dumps(got)}, expected {json.dumps(want)}")

if checked != len(expected):
    problems.append(f"{checked} of {len(expected)} declarations were actually compared")

for decl_name, attr in malformed[:5]:
    problems.append(
        f"{decl_name}: attrs entry {json.dumps(attr)} is not a two-element "
        "[name, value] array of strings — a schema-4 string reached a schema-5 IR"
    )
if len(malformed) > 5:
    problems.append(f"... and {len(malformed) - 5} more malformed attrs entries")

for attr_name in sorted(set(counts) | set(name_counts)):
    got = counts.get(attr_name, 0)
    want = name_counts.get(attr_name, 0)
    if got != want:
        problems.append(
            f"{attr_name}: {got} declaration(s) claim it, expected {want}"
        )

if valued != VALUED:
    problems.append(f"{valued} attribute(s) carry a value, expected {VALUED}")

if problems:
    for problem in problems:
        print(f"GATE 8 FAIL  {problem}", file=sys.stderr)
    sys.exit(1)

print(f"attrs        {checked} declarations compared, {sum(counts.values())} pair(s) over "
      f"{len(counts)} attribute name(s), {valued} with a value")
PY

say "10/14 GATE 9 — the origin of a realized declaration, and the three ways of not having one"
# `docs/plans/b0-generated-decls.md` measured that Lean gives a declaration it
# realizes from an attribute the position of **the attribute token**, and that no
# rule over `(line, col)` gets from there to the parent: in a 144-group Mathlib
# sample the parent was in the group 0 times and 47 groups spanned two or more
# namespaces. B-3's answer is that the extractor names the origin itself, from
# core's `extExtension` plus `selectionRange`.
#
# `Micro/Gen.lean` holds the four positions and the counter-example. Reads the
# IR, not the site: bundle B is extraction and IR only.
#
# FIVE THINGS, AND THE LAST THREE ARE WHY THE FIRST IS NOT ENOUGH
#   the origins       each named declaration's `generated`, compared whole,
#                     positives *and* negatives. `Micro.Gen.Solo.ext` is the
#                     sharp one: it is hand written and sits in the same
#                     environment extension as the realized theorems, so a rule
#                     that only asked the extension would claim it.
#   the count         how many expectations actually ran — GATE 7's lesson.
#   the strays        nobody else claims an origin. 9 in the whole fixture, and
#                     the number is asserted rather than the absence of
#                     surprises: a rule that answered "realized by ext" for
#                     everything passes every positive expectation above.
#   the trap          `selectionRange == range` is **42** declarations here and
#                     only 9 of them are realized by `@[ext]` — the other 33 are
#                     projections, constructors and a `scoped notation`. A rule
#                     that read the range equality as "generated" would claim 42
#                     and fail here 【実測 2026-08-21, and the same shape over
#                     2,786 Mathlib declarations →
#                     benchmarks/results/generated-decls-2026-08-21.txt】.
#   the falsifier     every origin named is a declaration this IR has, and none
#                     of the realized ones sorts *before* it. B-0 §13.2 records
#                     that as a property of one Lean version's `declRange`
#                     rather than a law, so it is counted rather than assumed;
#                     the three-toolchain version of this count is in the log
#                     above.
python3 - "$OUT/first/ir" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
index = json.loads((root / "index.json").read_text(encoding="utf-8"))
if index.get("schemaVersion") != 5:
    sys.exit(f"{root}/index.json: schemaVersion is {index.get('schemaVersion')!r}, not 5")

# Exact, by name, and negatives are expectations too. Each `None` below is a
# *different* way of not being realized by `@[ext]`.
expected = {
    # `@[ext]` written on the structure: both theorems land inside its range.
    "Micro.Gen.Pair.ext": ["ext", "Micro.Gen.Pair"],
    "Micro.Gen.Pair.ext_iff": ["ext", "Micro.Gen.Pair.ext"],
    # `attribute [ext] Trip` on a later line: both land outside its range.
    "Micro.Gen.Trip.ext": ["ext", "Micro.Gen.Trip"],
    "Micro.Gen.Trip.ext_iff": ["ext", "Micro.Gen.Trip.ext"],
    # `attribute [ext] Quad Quint`: one position, two parents, four theorems.
    # No rule over positions splits this group; each still names its own.
    "Micro.Gen.Quad.ext": ["ext", "Micro.Gen.Quad"],
    "Micro.Gen.Quad.ext_iff": ["ext", "Micro.Gen.Quad.ext"],
    "Micro.Gen.Quint.ext": ["ext", "Micro.Gen.Quint"],
    "Micro.Gen.Quint.ext_iff": ["ext", "Micro.Gen.Quint.ext"],
    # Realized from a *hand-written* ext theorem, so it names the theorem.
    "Micro.Gen.Solo.ext_iff": ["ext", "Micro.Gen.Solo.ext"],
    # Hand written, and in `extExtension` exactly like the realized ones.
    "Micro.Gen.Solo.ext": None,
    # Realized, but by `extends` rather than by `@[ext]`.
    "Micro.Gen.PairPlus.toPair": None,
    # A projection: `selectionRange == range`, and not realized by `@[ext]`.
    "Micro.Gen.Pair.fst": None,
    # The parent itself.
    "Micro.Gen.Pair": None,
}

GENERATED = 9        # declarations with an origin, over the whole fixture
SELECTION_EQ_RANGE = 42   # declarations whose two ranges are equal

found = {}
positions = {}
malformed = []
selection_eq = []
for entry in index["modules"]:
    module = json.loads((root / entry["file"]).read_text(encoding="utf-8"))
    if module.get("schemaVersion") != 5:
        sys.exit(f"{entry['file']}: schemaVersion is {module.get('schemaVersion')!r}, not 5")
    for decl in module["declarations"]:
        found[decl["name"]] = decl.get("generated")
        positions[decl["name"]] = (module["module"], decl["line"], decl["col"])
        sel = decl.get("selectionRange")
        ok = (
            isinstance(sel, list)
            and len(sel) == 4
            and all(isinstance(part, int) for part in sel)
        )
        if not ok:
            malformed.append((decl["name"], "selectionRange", sel))
            continue
        if sel == [decl["line"], decl["col"], decl["endLine"], decl["endCol"]]:
            selection_eq.append(decl["name"])
        gen = decl.get("generated")
        if gen is not None and not (
            isinstance(gen, list)
            and len(gen) == 2
            and all(isinstance(part, str) for part in gen)
        ):
            malformed.append((decl["name"], "generated", gen))

problems = []
checked = 0
for name, want in sorted(expected.items()):
    if name not in found:
        problems.append(f"{name} is not in the IR at all — the fixture lost a shape")
        continue
    checked += 1
    got = found[name]
    if got != want:
        problems.append(
            f"{name}: generated is {json.dumps(got)}, expected {json.dumps(want)}"
        )

if checked != len(expected):
    problems.append(f"{checked} of {len(expected)} declarations were actually compared")

for decl_name, key, value in malformed[:5]:
    problems.append(f"{decl_name}: {key} is {json.dumps(value)}, not the shape the writer emits")
if len(malformed) > 5:
    problems.append(f"... and {len(malformed) - 5} more malformed keys")

claimed = sorted(name for name, value in found.items() if value is not None)
if len(claimed) != GENERATED:
    problems.append(
        f"{len(claimed)} declaration(s) claim an origin, expected {GENERATED}: "
        + ", ".join(claimed[:8])
    )
strays = [name for name in claimed if expected.get(name) is None]
if strays:
    problems.append(f"{len(strays)} unexpected declaration(s) claim an origin: {', '.join(strays[:5])}")

# The trap. If these two numbers ever become equal, something started reading
# range equality as "generated".
if len(selection_eq) != SELECTION_EQ_RANGE:
    problems.append(
        f"{len(selection_eq)} declaration(s) have selectionRange == range, "
        f"expected {SELECTION_EQ_RANGE}"
    )
equal_and_claimed = [name for name in selection_eq if found.get(name) is not None]
if len(equal_and_claimed) != GENERATED:
    problems.append(
        f"{len(equal_and_claimed)} of the {len(selection_eq)} declarations with "
        f"selectionRange == range claim an origin, expected {GENERATED}"
    )
if len(selection_eq) == len(equal_and_claimed):
    problems.append(
        "every declaration whose two ranges are equal claims an origin — the "
        "rule has collapsed into range equality, which is not what it means"
    )

# The falsifier, on this fixture. The three-toolchain form of the same count is
# in benchmarks/results/generated-decls-2026-08-21.txt.
before = []
for name in claimed:
    origin = found[name][1]
    if origin not in positions:
        problems.append(f"{name} names {origin} as its origin, which is not in this IR")
        continue
    # Follow the chain to something that is not itself realized.
    seen = set()
    while found.get(origin) is not None and origin not in seen:
        seen.add(origin)
        origin = found[origin][1]
        if origin not in positions:
            break
    if origin not in positions:
        problems.append(f"{name}'s origin chain leaves this IR at {origin}")
        continue
    if positions[name][0] == positions[origin][0] and positions[name][1:] < positions[origin][1:]:
        before.append(f"{name} at {positions[name][1:]} sorts before {origin} at {positions[origin][1:]}")
if before:
    problems.append(
        "a realized declaration sorts before its origin, which B-0 §6 measured "
        "as 0 on both samples: " + "; ".join(before[:3])
    )

if problems:
    for problem in problems:
        print(f"GATE 9 FAIL  {problem}", file=sys.stderr)
    sys.exit(1)

print(f"generated    {checked} declarations compared, {len(claimed)} realized by @[ext] over "
      f"{len(found)} declarations; {len(selection_eq)} have selectionRange == range and "
      f"{len(selection_eq) - len(claimed)} of those are not realized; "
      f"{len(before)} sort before their origin")
PY

say "11/14 GATE 10 — docstring math becomes MathML, and unreadable math does not"
# `docs/plans/feature-sweep.md` C-1. Five assertions over `Micro/Math.html` plus
# one over the run's marker, and that last one is what the others cannot make:
#
#   the count            `work.mathFallbacks` is **1**, and one is the number
#                        `Micro/Math.lean` was written to produce. A fallback
#                        leaves a *valid page* — no byte count, no page count and
#                        no exit code moves when a formula fails — so without
#                        this number a run that converted nothing would look
#                        exactly like a run that converted everything.
#
#   conversion happened  three inline `<math>` and one `<math display="block">`.
#                        Exact, not "at least one": a renderer emitting a
#                        `<math>` per character would pass a lower bound.
#
#   the fallback is the source  `$\colim_k F(k)$` is on the page, dollars and
#                        all — what doc-gen4 emits for every span. A page whose
#                        mathematics failed is no worse than doc-gen4's page.
#
#   nothing is half-escaped  no `<` that opens no tag and no `&` that opens no
#                        entity, inside the `<math>` elements. This is the defect
#                        that decided the crate: `pulldown-latex` writes
#                        `<mo><</mo>` for `$a < b$` (61 of Mathlib's 2,123
#                        spans) and `math-core` does not. Swap the dependency
#                        back and this line is what says so.
#
#   the dollars are gone where it worked  `$a &lt; b$` must **not** be on the
#                        page. It is the fallback's spelling for the escape
#                        case, and finding it would mean conversion silently
#                        stopped while `mathFallbacks` stayed at 1.
python3 - "$OUT/first/site/Micro/Math.html" "$OUT/first-build.json" <<'MATHPY'
import json
import re
import sys

page_path, marker_path = sys.argv[1:3]
page = open(page_path, encoding="utf-8").read()
with open(marker_path) as handle:
    work = json.load(handle)["work"]
problems = []


def want(label, got, expected):
    if got != expected:
        problems.append(f"{label}: {got}, expected {expected}")


# Read by name out of the record the run wrote. A missing key exits here rather
# than defaulting to a number that would pass — a gate that looks for a key that
# is not there checks nothing at all, and has done exactly that in this
# repository before (CLAUDE.md, 2026-08-18).
if "mathFallbacks" not in work:
    sys.exit("GATE 10 FAIL  the marker has no work.mathFallbacks")
want("work.mathFallbacks", work["mathFallbacks"], 1)

want("inline <math>", len(re.findall(r"<math>", page)), 3)
want('<math display="block">', len(re.findall(r'<math display="block">', page)), 1)
want("merror elements", page.count("<merror"), 0)

if "$\\colim_k F(k)$" not in page:
    problems.append("the unconvertible span is not on the page as its own source")
if "$a &lt; b$" in page:
    problems.append("`$a < b$` was written back as LaTeX — it is supposed to convert")

strays = []
for formula in re.findall(r"<math[^>]*>.*?</math>", page, re.S):
    for hit in re.finditer(r"<(?![/a-zA-Z])", formula):
        strays.append(("<", formula[max(0, hit.start() - 20):hit.start() + 20]))
    for hit in re.finditer(r"&(?![a-zA-Z]+;|#[0-9]+;|#x[0-9a-fA-F]+;)", formula):
        strays.append(("&", formula[max(0, hit.start() - 20):hit.start() + 20]))
if strays:
    problems.append(
        f"{len(strays)} character(s) of markup are unescaped inside <math>: "
        + "; ".join(f"{c} near {ctx!r}" for c, ctx in strays[:3])
    )

if problems:
    for problem in problems:
        print(f"GATE 10 FAIL  {problem}", file=sys.stderr)
    sys.exit(1)

print(f"math         {len(re.findall(r'<math', page))} formula(s) rendered, "
      f"{work['mathFallbacks']} kept as LaTeX, {len(strays)} unescaped character(s)")
MATHPY

say "12/14 GATE 11 — every reverse reference agrees with the IR, both ways"
# `docs/plans/feature-sweep.md` C-2 / doc-gen4 #77. The script is its own file
# because it is worth running against the measurement target too, where the
# numbers are 849 targets over 10,163 edges 【実測 2026-08-22】 rather than the
# fixture's handful. Its heading says what it asserts and why both directions
# are needed.
"$HERE/usedby-gate.sh" --ir "$OUT/first/ir" --site "$OUT/first/site"

say "13/14 GATE 6 — one edited module does not re-render the package"
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

say "14/14 summary"
printf 'site files : %s\n' "$(find "$OUT/first/site" -type f | wc -l | tr -d ' ')"
printf 'ir files   : %s\n' "$(find "$OUT/first/ir" -type f | wc -l | tr -d ' ')"
printf 'out        : %s\n' "$OUT"

if [ "$TEMPORARY" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
  rm -rf "$OUT"
fi
echo
echo "E2E MICRO: ok"
