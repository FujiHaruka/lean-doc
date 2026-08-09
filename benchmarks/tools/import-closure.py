#!/usr/bin/env python3
"""Compute the import closure of a module list from .lean headers, and turn it into the
list of olean files a prefetcher could ask for.

Why: the prefetch measurement (benchmarks/results/ci-prefetch-summary.txt) compares an
"oracle" file set — the 17,528 files a cold run was *observed* to touch — against a
"blind" set of every olean under LEAN_PATH. The oracle set is only interesting if
something an implementation could actually compute comes close to it. The import closure
is that something: it is derivable before the extractor starts, from text, without
loading any olean.

Header parsing only: read each module's source, take the `import` lines that precede the
first non-header line, recurse. This is what `Lean.parseImports` does, minus the parser.

usage:
  import-closure.py <modules.txt> [--olean-out FILE] [--modules-out FILE]
"""
import os
import re
import sys

TARGET = os.environ.get("TARGET_REPO", "/Users/haruka/dev/lean-projects")
TOOLCHAIN = os.path.expanduser(
    os.environ.get("LEAN_TOOLCHAIN_DIR",
                   "~/.elan/toolchains/leanprover--lean4---v4.31.0"))

# (source root, olean root) pairs, in the order LEAN_PATH lists them.
PKGS = ["batteries", "Qq", "aesop", "proofwidgets", "importGraph", "LeanSearchClient",
        "plausible", "MD4Lean", "BibtexQuery", "UnicodeBasic", "Cli", "leansqlite",
        "loogle", "mathlib", "doc-gen4"]
ROOTS = [(f"{TARGET}/.lake/packages/{p}", f"{TARGET}/.lake/packages/{p}/.lake/build/lib/lean")
         for p in PKGS]
ROOTS.append((TARGET, f"{TARGET}/.lake/build/lib/lean"))
ROOTS.append((f"{TOOLCHAIN}/src/lean", f"{TOOLCHAIN}/lib/lean"))

# Anchored at both ends: prose inside a module docstring ("*import statements*") otherwise
# parses as an import of a module named `statements`.
IMPORT_RE = re.compile(r"^\s*(?:public\s+|private\s+|meta\s+)*import\s+(?:all\s+)?"
                       r"([A-Za-z_À-￿][A-Za-z_0-9'.À-￿«»]*)\s*(?:--.*)?$")
# A header ends at the first line that is neither blank, a comment, a `module`/`prelude`
# marker, nor an import.
HEADER_OK = re.compile(r"^\s*(--|/-|$|module\b|prelude\b|set_option\b|import\b)")


def source_of(mod):
    rel = os.path.join(*mod.split(".")) + ".lean"
    for src, _ in ROOTS:
        p = os.path.join(src, rel)
        if os.path.exists(p):
            return p
    return None


def imports_of(path):
    out = []
    with open(path, encoding="utf-8", errors="replace") as f:
        in_block = False
        for line in f:
            if in_block:
                if "-/" in line:
                    in_block = False
                continue
            if line.lstrip().startswith("/-"):
                if "-/" not in line:
                    in_block = True
                continue
            m = IMPORT_RE.match(line)
            if m:
                out.append(m.group(1))
                continue
            if not HEADER_OK.match(line):
                break
    return out


def closure(seeds):
    seen, missing, stack = set(), set(), list(seeds)
    while stack:
        mod = stack.pop()
        if mod in seen or mod in missing:
            continue
        src = source_of(mod)
        if src is None:
            missing.add(mod)
            continue
        seen.add(mod)
        stack.extend(imports_of(src))
    return seen, missing


def oleans(mods):
    out = []
    for mod in sorted(mods):
        rel = os.path.join(*mod.split("."))
        for _, oroot in ROOTS:
            base = os.path.join(oroot, rel)
            found = [base + ext for ext in (".olean", ".olean.server", ".olean.private")
                     if os.path.exists(base + ext)]
            if found:
                out.extend(found)
                break
    return sorted(out)


if __name__ == "__main__":
    seeds = [l.strip() for l in open(sys.argv[1]) if l.strip()]
    mods, missing = closure(seeds)
    files = oleans(mods)
    total = sum(os.path.getsize(p) for p in files)
    print(f"seed modules      {len(seeds)}")
    print(f"closure modules   {len(mods)}")
    print(f"unresolved names  {len(missing)}" +
          ("  e.g. " + ", ".join(sorted(missing)[:5]) if missing else ""))
    print(f"olean files       {len(files)}")
    print(f"olean bytes       {total} ({total / 2 ** 30:.3f} GiB)")
    for flag, path in (("--olean-out", None), ("--modules-out", None)):
        if flag in sys.argv:
            path = sys.argv[sys.argv.index(flag) + 1]
            with open(path, "w") as f:
                for x in (files if flag == "--olean-out" else sorted(mods)):
                    f.write(x + "\n")
            print(f"wrote {path}")
