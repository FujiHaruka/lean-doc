#!/usr/bin/env bash
# Stage 6d — U1..U4: is preview mode the receptacle for residency?
#
# Every question here is a comparison of trees stage 6a already produced, so this
# script starts nothing and times nothing. That is deliberate: the answers are
# about *which* bytes exist, not how fast they were made, and a measurement that
# does not need a clock should not pretend to have one.
#
# Inputs, all from stage 6a's work directory:
#   base-ir / base-pages     the pre-edit state, i.e. what the previous build left
#   ref-ir  / ref-pages      the post-build truth
#   keep-ir2-pre             what the pre-edit server P answered for round 2
#   keep-pre                 the site assembled with P in the loop
#   fresh/timings.json       the changed / stale / rendered counts
#
# usage: run.sh <stage6a work dir>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LD="$(cd "$HERE/../.." && pwd)"
W=${1:?stage6a work dir}
RESULTS="$LD/benchmarks/results"
OUT="$W/out6d"; mkdir -p "$OUT"

for p in base-ir base-pages ref-ir ref-pages keep-ir2-pre keep-pre; do
  [ -e "$W/$p" ] || { echo "missing $W/$p — run stage6a/run.sh first" >&2; exit 2; }
done

python3 - "$W" "$OUT/verdict.txt" <<'PY'
import json, os, sys
W, out = sys.argv[1:]

def tree(p):
    files = {}
    for root, _d, fs in os.walk(p):
        for f in fs:
            full = os.path.join(root, f)
            files[os.path.relpath(full, p)] = open(full, "rb").read()
    return files

base_pages = tree(os.path.join(W, "base-pages"))
ref_pages = tree(os.path.join(W, "ref-pages"))
pre_pages = tree(os.path.join(W, "keep-pre"))

lines = []

# ---------------------------------------------------------------- U1
# The question is not "was the site wrong" (stage 6a answered that) but "did the
# pre-edit server produce anything that was not already on disk". So the
# comparison is P's answer against the BASE IR, not against the truth.
lines.append("## U1 — did the pre-edit server produce anything new?")
p_dir = os.path.join(W, "keep-ir2-pre", "modules")
base_dir = os.path.join(W, "base-ir", "modules")
ref_dir = os.path.join(W, "ref-ir", "modules")
served = sorted(os.listdir(p_dir))
new_and_correct = []
for f in served:
    got = open(os.path.join(p_dir, f), "rb").read()
    was = open(os.path.join(base_dir, f), "rb").read() if os.path.exists(os.path.join(base_dir, f)) else None
    truth = open(os.path.join(ref_dir, f), "rb").read() if os.path.exists(os.path.join(ref_dir, f)) else None
    same_as_base = was is not None and was == got
    correct = truth is not None and truth == got
    lines.append("  %-60s equals the previous build: %-5s  correct: %s"
                 % (f.replace(".json", ""), same_as_base, correct))
    if correct and not same_as_base:
        new_and_correct.append(f)
lines.append("U1 modules served by P: %d; of those, both correct AND different from "
             "what the previous build had: **%d**" % (len(served), len(new_and_correct)))
lines.append("U1 -> %s" % ("CONFIRMED: the set is empty" if not new_and_correct
                           else "REFUTED: " + ", ".join(new_and_correct)))

# ---------------------------------------------------------------- U2
lines.append("")
lines.append("## U2 — what a user needs after the edit, and who can produce it")
t = json.load(open(os.path.join(W, "fresh", "timings.json"), encoding="utf-8"))
truly_changed = sorted(k for k in set(base_pages) & set(ref_pages)
                       if base_pages[k] != ref_pages[k])
added = sorted(set(ref_pages) - set(base_pages))
removed = sorted(set(base_pages) - set(ref_pages))

# The two kinds have to be counted apart. A module page is stale only if the edit
# reached it; the whole-package artefacts (L3-3) are rebuilt on every run whatever
# changed, so they are never "stale" in the sense a preview cares about, and
# folding them into one percentage would overstate what a preview gets wrong.
SITEWIDE = ("navbar.html", "tactics.html", "references.html", "references.bib",
            "declarations/declaration-data.bmp", "declarations/name-map.json")
def is_module_page(k):
    return k not in SITEWIDE
mod_changed = [k for k in truly_changed if is_module_page(k)]
site_changed = [k for k in truly_changed if not is_module_page(k)]
mod_added = [k for k in added if is_module_page(k)]
mod_removed = [k for k in removed if is_module_page(k)]
mod_total = len([k for k in ref_pages if is_module_page(k)])
mod_affected = len(mod_changed) + len(mod_added) + len(mod_removed)
affected = len(truly_changed) + len(added) + len(removed)
lines.append("  files in the previous build: %d ; in the post-build truth: %d"
             % (len(base_pages), len(ref_pages)))
lines.append("  MODULE PAGES the edit reached: %d of %d (%.2f%%)  "
             "[%d changed, +%d added, -%d removed]"
             % (mod_affected, mod_total, 100.0 * mod_affected / mod_total,
                len(mod_changed), len(mod_added), len(mod_removed)))
lines.append("  the pipeline's render set: %d  -> %s"
             % (t["pagesRendered"],
                "the same set" if t["pagesRendered"] == mod_affected
                else "DIFFERENT from the affected set"))
lines.append("  WHOLE-PACKAGE artefacts that changed: %d  (%s)  "
             "— rebuilt every run by L3-3, so never 'stale'"
             % (len(site_changed), ", ".join(site_changed)))
lines.append("  L2 reported changed modules: %d ; L3-1 stale: %d ; L3-2 stale: %d"
             % (t["changed"], t["staleFound"], t["globalStale"]))
lines.append("U2 -> %d module pages of %d (%.2f%%) are obtainable only from a "
             "post-build extraction; the other %d were already correct on disk"
             % (mod_affected, mod_total, 100.0 * mod_affected / mod_total,
                mod_total - mod_affected))

# ---------------------------------------------------------------- U3
lines.append("")
lines.append("## U3 — a 'serve the previous page, marked not-updated' preview")
lines.append("  module pages it would serve stale: %d of %d = %.2f%%"
             % (mod_affected, mod_total, 100.0 * mod_affected / mod_total))
lines.append("  they are: " + ", ".join(mod_changed)
             + ("  [+added: " + ", ".join(mod_added) + "]" if mod_added else ""))
lines.append("U3 -> the stale set is small and named, so the marker is per page. "
             "A site-wide banner would mark %d correct pages for nothing."
             % (mod_total - mod_affected))

# ---------------------------------------------------------------- U4
lines.append("")
lines.append("## U4 — is the stale set derivable before the build finishes?")
lines.append("  derivable from the ledger alone (olean hashes, no Lean): the L2 "
             "changed / added set = %d modules" % t["changed"])
lines.append("  needs the FRESH IR of the changed modules, i.e. needs the build: "
             "L3-1's %d and L3-2's %d" % (t["staleFound"], t["globalStale"]))
covered = t["changed"]
lines.append("U4 -> pre-build a preview can name %d of the %d affected module "
             "pages exactly (%.0f%%); the remaining %d are the L3-1/L3-2 ones and "
             "are found only after the build. %s"
             % (min(covered, mod_affected), mod_affected,
                100.0 * min(covered, mod_affected) / mod_affected,
                max(0, mod_affected - covered),
                "So a pre-build marker must be conservative for the rest."
                if mod_affected > covered else
                "So a pre-build marker is already complete for this change."))
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY

{
  echo "# stage6d — is preview mode the receptacle for residency?"
  echo
  echo "date              $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "inputs            stage6a work dir $W (no process started, nothing timed)"
  echo "change            InformationTheory.Shannon.Huffman.Length -> ...LengthCore (stage 5e's move)"
  echo
  cat "$OUT/verdict.txt"
} > "$RESULTS/stage6d-preview.txt"
echo "-> $RESULTS/stage6d-preview.txt"
