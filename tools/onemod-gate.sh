#!/usr/bin/env bash
# What a one-module edit is allowed to cost, as integers.
#
# usage: tools/onemod-gate.sh <lean-doc-build.json> <serve.out>
#
# WHY THIS IS A FILE AND NOT TWO COPIES OF AN `if`
#
#   Two callers ask the same question in two places — `tools/e2e-micro.sh`'s
#   GATE 6 on the Mathlib-free fixture, and `.github/workflows/ci-template.yml`
#   on a Linux runner. The claim is one claim, so the checker is one file: a
#   second spelling of a gate is how the two stop agreeing about what passing
#   means, and the one that is easier to run is the one that gets loosened.
#
# WHAT IT CHECKS, AND WHY EACH ONE IS HERE
#
#   modulesExtracted >= 1        The edit was noticed at all. A zero here is not
#                                a fast build, it is a build that did not happen,
#                                and every other number below would then be
#                                trivially green.
#   1 <= pagesRendered < modules 段 C. The lower bound is the same argument as
#                                above; the upper bound is the symptom 段 C
#                                removed — the dependency map used to move on
#                                every edit, and its digest is a `renderKey`
#                                input, so one added declaration re-rendered the
#                                whole package. Kept as an inequality rather than
#                                a number: how many pages a *referrer* pulls in
#                                is the impact set's business and it is allowed to
#                                grow. Pinning a number here would make this file
#                                the place that argument has to happen.
#   the map was reused           段 D. Not moving and not being *written* are
#                                different claims and the bytes cannot tell them
#                                apart: a map rewritten to the same content
#                                passes a byte comparison while still costing the
#                                490,287-constant walk that produced it. The
#                                extractor says which it did; read that.
#
#   Nothing here is a duration. This workload's environment load moves 5x with
#   the page cache (CLAUDE.md), so a second is not a threshold — these four are
#   integers and a boolean, and they are the ones the design actually claims.
#
# WHAT IT DOES NOT CHECK
#
#   Whether the pages that *were* rendered are right. Under-rendering is silent
#   and this file cannot see it: the caller has to compare the tree against a
#   whole render of the same IR (`e2e-micro.sh` does, and says so). A green here
#   with no such comparison beside it is a count, not a verdict.
set -uo pipefail

BUILD_JSON="${1-}"
SERVE_OUT="${2-}"

[ -n "$BUILD_JSON" ] && [ -n "$SERVE_OUT" ] || {
  echo "usage: $0 <lean-doc-build.json> <serve.out>" >&2
  exit 2
}
[ -f "$BUILD_JSON" ] || { echo "onemod-gate: no such file: $BUILD_JSON" >&2; exit 1; }
[ -f "$SERVE_OUT" ] || { echo "onemod-gate: no such file: $SERVE_OUT" >&2; exit 1; }

status=0

python3 - "$BUILD_JSON" <<'PY' || status=1
import json
import sys

record = json.load(open(sys.argv[1], encoding="utf-8"))
modules = record["modules"]
work = record["work"]
rendered = work["pagesRendered"]
extracted = work["modulesExtracted"]
problems = []

if extracted < 1:
    problems.append(
        f"work.modulesExtracted is {extracted} — the edited module was not re-extracted, "
        "so nothing below means anything"
    )
if not 1 <= rendered < modules:
    problems.append(
        f"work.pagesRendered is {rendered} for {modules} module(s) — expected at least one "
        "and fewer than all (段 C)"
    )

print(f"onemod-gate   modules {modules}  extracted {extracted}  rendered {rendered}  "
      f"irReads.module {work['irReads']['module']}")
for problem in problems:
    print(f"onemod-gate FAIL  {problem}", file=sys.stderr)
sys.exit(1 if problems else 0)
PY

# The extractor's own word, from its log. `grep -E` rather than a JSON field
# because this is the file a person reads when the gate goes red, and the line it
# matches is the line they will be looking at.
if grep -qE '^linkIndex .* reused ' "$SERVE_OUT"; then
  echo "onemod-gate   the dependency map was reused, not rewritten (段 D)"
else
  echo "onemod-gate FAIL  the extractor rewrote the dependency map instead of reusing it (段 D)" >&2
  grep -E '^linkIndex ' "$SERVE_OUT" >&2 || echo "  (no linkIndex line in $SERVE_OUT)" >&2
  status=1
fi

exit "$status"
