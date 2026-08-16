#!/usr/bin/env bash
# Build the **second target**: a small Mathlib-dependent Lean package that has
# never seen doc-gen4, generated from scratch by this script.
#
# Milestone **M5-b**, and the subject of plan §1's gate B ("a Mathlib-dependent
# package *other than* the measurement target, `lean-doc build` in one command,
# and the incremental path works there"). The measurement target cannot answer
# that question on its own: it carries a 736 MB doc-gen4 output tree, so a
# dependency map can always be handed to the renderer from outside. Here there
# is none, and whatever the product cannot derive itself, it does not get.
#
# WHY A GENERATOR AND NOT A CHECKED-IN TREE
#   The package lives under /private/tmp, which this machine empties; a session
#   has already lost one clone that way. What is committed is therefore the
#   thing that makes the package, not the package. Everything below — the two
#   libraries, the module list, every boundary value — is *in this file*, so a
#   number measured on target 2 can be re-derived from the repository alone.
#
# WHY THE DEPENDENCIES ARE NEVER RESOLVED
#   `lake update` needs the network and would pick today's Mathlib; this package
#   has to be built against **the same Mathlib the measurement target uses**
#   (`fabf563a7c95`, Lean v4.31.0) or its numbers are not comparable with any
#   other number in this repository. So `lake-manifest.json`, `lean-toolchain`
#   and the `[[require]]` blocks are copied verbatim from the measurement target
#   in **both** modes below, and the revisions are pinned by that manifest rather
#   than by anything this script decides.
#
# HOW THE DEPENDENCIES GET THERE — TWO MODES  (`--deps`)
#   clone   an **APFS clonefile copy** (`cp -Rc`) of the measurement target's
#           `.lake/packages`: copy-on-write, ~0 real disk, no network, and the
#           source tree is read and never written. CLAUDE.md forbids writing
#           into the measurement target, and that includes its `.lake/packages`
#           — a symlink would not do, because Lake writes into a package
#           directory it believes it owns. **macOS only**: `cp -c` is a BSD
#           extension and the filesystem has to be APFS.
#   fetch   `lake exe cache get` in the generated package: Lake clones the
#           revisions the copied manifest pins and downloads Mathlib's prebuilt
#           oleans. **Needs the network**, and is the path a Linux runner takes
#           — a GitHub runner has no measurement target to clone from, only its
#           checkout. The manifest still decides which Mathlib, so the two modes
#           agree on the thing that has to be identical.
#   auto    (default) `clone` where `cp -Rc` works, `fetch` otherwise.
#
#   The two modes are NOT interchangeable for measurement: `fetch` leaves the
#   oleans in the state a download leaves them in, `clone` in the state a
#   copy-on-write leaves them in. Numbers from one are not numbers from the
#   other; the mode is printed at the end so a log says which was used.
#
# THE POINT OF THE SOURCES: SIX BOUNDARY VALUES, ONE PER MODULE
#   Plan §5, §7 and §8 each name an input that "does not occur in the target
#   package but can occur in a second one". They are all here, one per module so
#   that a failure names itself:
#
#     Alpha/NulCode.lean      NUL inside a fenced code block  — MD4Lean SIGSEGVs
#                             (`wrapper.c:558`); Rust substitutes U+FFFD
#     Alpha/EmptyTable.lean   a GFM table with no body row    — MD4Lean SIGABRTs
#                             (`wrapper.c:389`); Rust emits an empty body
#     Alpha/AstralNames.lean  declaration names above the BMP (U1): UTF-16 code
#                             unit order and code point order **disagree**
#     Alpha/HeadingSplit.lean a heading containing U+2B96, one of the 4,802 code
#                             points where UnicodeBasic and V8 disagree (§5.3)
#     Alpha/Odd-Name.lean     a module whose name needs `«…»` (M5-a trap 4)
#     Alpha/Private.lean      `_private.` names (決定 5)
#     Beta/TokenSep.lean      U+088F inside a docstring code span — a separator
#                             for V8 and not for UnicodeBasic (§8, V6)
#     Beta/DupNames.lean      forces another module's equation lemmas, which is
#                             how one name comes to be in two modules' oleans
#
# usage:
#   make-target2.sh [--out <dir>] [--force] [--deps auto|clone|fetch]
#   make-target2.sh --print-modules      # the module list, without building
set -euo pipefail

SRC="${TARGET_SRC:-/Users/haruka/dev/lean-projects}"
OUT="${TARGET2:-/private/tmp/lean-doc-relay/m5b/target2}"
DEPS="${TARGET2_DEPS:-auto}"
FORCE=0
PRINT_ONLY=0

usage () {
  echo "usage: make-target2.sh [--out <dir>] [--force] [--deps auto|clone|fetch] [--print-modules]" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
    --force) FORCE=1; shift ;;
    --deps) DEPS=$2; shift 2 ;;
    --print-modules) PRINT_ONLY=1; shift ;;
    *) usage ;;
  esac
done

case "$DEPS" in auto | clone | fetch) ;; *) echo "unknown --deps: $DEPS" >&2; usage ;; esac

# The one rule this script cannot get wrong: it never writes inside the
# measurement target. Stated against the path rather than assumed.
case "$OUT" in
  "$SRC"|"$SRC"/*) echo "refusing to write inside the measurement target" >&2; exit 2 ;;
esac

MODULES="Alpha Alpha.AstralNames Alpha.Basic Alpha.EmptyTable Alpha.HeadingSplit
Alpha.NulCode Alpha.Odd-Name Alpha.Private Beta Beta.Basic Beta.DupNames
Beta.Referrer Beta.TokenSep"

if [ "$PRINT_ONLY" = 1 ]; then
  for m in $MODULES; do echo "$m"; done
  exit 0
fi

[ -d "$SRC" ] || { echo "measurement target not found: $SRC" >&2; exit 1; }

if [ -e "$OUT" ]; then
  if [ "$FORCE" = 1 ]; then
    echo "### removing $OUT"
    chmod -R u+w "$OUT" 2>/dev/null || true
    rm -rf "$OUT"
  else
    echo "$OUT already exists (pass --force to rebuild it)" >&2
    exit 3
  fi
fi

mkdir -p "$OUT/.lake"

# ---------------------------------------------------------------- the packages
#
# Decided here and reported at the end, because the two modes leave the oleans
# in different states and a number taken on one is not a number taken on the
# other. The probe is the operation itself: `cp -c` is a BSD flag and clonefile
# needs APFS, so asking the kernel is more honest than asking `uname`.
if [ "$DEPS" = auto ]; then
  probe="$OUT/.lake/.clonefile-probe"
  : > "$probe"
  if cp -c "$probe" "$probe.copy" 2> /dev/null; then DEPS=clone; else DEPS=fetch; fi
  rm -f "$probe" "$probe.copy"
  echo "### --deps auto -> $DEPS"
fi

if [ "$DEPS" = clone ]; then
  echo "### cloning $SRC/.lake/packages -> $OUT/.lake/packages (APFS clonefile)"
  time cp -Rc "$SRC/.lake/packages" "$OUT/.lake/packages"
fi

# Both modes: the revisions come from the measurement target's manifest, never
# from a resolution run here.
cp "$SRC/lean-toolchain" "$OUT/lean-toolchain"
cp "$SRC/lake-manifest.json" "$OUT/lake-manifest.json"

# ---------------------------------------------------------------- the lakefile
#
# **Two `[[lean_lib]]` blocks**, which is the shape the measurement target does
# not have (it declares one). `lean-doc build` derives `--lib` from this file
# (`crates/lean-doc/src/lakefile.rs`), and a recogniser that reads only the first
# block would document half the package and report success — the silent
# under-read that file is written against. Here it has two to find.
#
# The `[[require]]` blocks are the target's, verbatim: they have to agree with
# the copied `lake-manifest.json` or Lake re-resolves them, which needs the
# network and would move Mathlib.
cat > "$OUT/lakefile.toml" <<'TOML'
name = "target2"
version = "0.1.0"
defaultTargets = ["Alpha", "Beta"]

[[require]]
name = "loogle"
git = "https://github.com/nomeata/loogle"
rev = "3a988dbfa601ddbbfd9330d90c45e7a68263b9c7"

[[require]]
name = "mathlib"
scope = "leanprover-community"
rev = "v4.31.0"

[[require]]
name = "doc-gen4"
git = "https://github.com/leanprover/doc-gen4"
rev = "v4.31.0"

[[lean_lib]]
name = "Alpha"
globs = ["Alpha", "Alpha.+"]

[[lean_lib]]
name = "Beta"
globs = ["Beta", "Beta.+"]
TOML

# ----------------------------------------------------------------- the sources
mkdir -p "$OUT/Alpha" "$OUT/Beta"

cat > "$OUT/Alpha.lean" <<'LEAN'
import Alpha.AstralNames
import Alpha.Basic
import Alpha.EmptyTable
import Alpha.HeadingSplit
import Alpha.NulCode
import Alpha.«Odd-Name»
import Alpha.Private
LEAN

cat > "$OUT/Alpha/Basic.lean" <<'LEAN'
import Mathlib.Data.Nat.Basic

/-!
# The ordinary module

No boundary value at all. It is here so that the gates have pages whose bytes
nothing exotic can explain, and so that every other module has something of
another module's to point at.
-/

namespace Alpha.Basic

/-- The constant the rest of this package points at. -/
def alphaConst : Nat := 41

/-- `alphaConst` is positive. Mentions `Alpha.Basic.alphaConst` in a code span. -/
theorem alphaConst_pos : 0 < alphaConst := by decide

/-- Defined by cases, so its equation lemmas are generated on demand rather than
at declaration time — which is what `Beta.DupNames` forces. -/
def step : Nat → Nat
  | 0 => 0
  | n + 1 => n

end Alpha.Basic
LEAN

# The NUL byte cannot go through a heredoc, so it is written as a marker and
# substituted afterwards. `perl -0777` because the file is read whole.
cat > "$OUT/Alpha/NulCode.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.NulCode

/-- A fenced code block with a NUL byte in it.

```
before@@NUL@@after
```

MD4Lean dies on this input with SIGSEGV (`wrapper.c:558`); `lean-doc-md`
substitutes U+FFFD for the byte and keeps going. See `Alpha.Basic.alphaConst`.
-/
def nulInCode : Nat := 1

end Alpha.NulCode
LEAN
perl -0777 -i -pe 's/\@\@NUL\@\@/\x00/g' "$OUT/Alpha/NulCode.lean"

cat > "$OUT/Alpha/EmptyTable.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.EmptyTable

/-- A GFM table with a header row, a delimiter row and no body row.

| left | right |
|------|-------|

MD4Lean dies on this input with SIGABRT (`wrapper.c:389`, an assert);
`lean-doc-md` emits an empty `<tbody>` and keeps going.
-/
def emptyTable : Nat := 2

end Alpha.EmptyTable
LEAN

# U+1D49C MATHEMATICAL SCRIPT CAPITAL A is inside Lean's letter-like range, so
# it is an ordinary identifier character; U+FB00 is not, so `«…»` is needed. The
# pair is the point: `«𝒜-z»` sorts **before** `«ﬀ-z»` in UTF-16 code unit order
# (D835 < FB00) and **after** it by code point (1D49C > FB00). Every file whose
# bytes are made by one of the prototype's bare `.sort()` calls — name-map.json,
# declaration-data.bmp, navbar.html — is decided by that order (plan §7, U1).
cat > "$OUT/Alpha/AstralNames.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.AstralNames

/-- A declaration whose name is outside the BMP. -/
def 𝒜 : Nat := 3

/-- Escaped, astral first character: `«𝒜-z»`. -/
def «𝒜-z» : Nat := 4

/-- Escaped, BMP but above the surrogate range: `«ﬀ-z»`. -/
def «ﬀ-z» : Nat := 5

/-- Points at `Alpha.AstralNames.𝒜` from a code span. -/
theorem astral_eq : 𝒜 = 3 := rfl

end Alpha.AstralNames
LEAN

# U+2B96 is unassigned in V8's UCD (general category Cn ⊂ C) and assigned in the
# UnicodeBasic the target's doc-gen4 was built against. A heading id is built by
# splitting on that table, so this is the input that separates them — and unlike
# the token separators, a heading id reaches `id=` and `href=#` (plan §5.3).
cat > "$OUT/Alpha/HeadingSplit.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.HeadingSplit

/-- A docstring with a heading that contains U+2B96.

# Head@@U2B96@@ing

The heading id above is decided by the general-category table: V8 calls U+2B96 a
separator (it is unassigned there), UnicodeBasic does not.
-/
def headingSplit : Nat := 6

end Alpha.HeadingSplit
LEAN
perl -CSD -0777 -i -pe 's/\@\@U2B96\@\@/\x{2b96}/g' "$OUT/Alpha/HeadingSplit.lean"

# The file name gives the module name `Alpha.«Odd-Name»`: `-` is not an
# identifier character, so `Name.toString` escapes the component and
# `Name.toString (escape := false)` does not. The `.lidx` writer uses the second
# spelling for module names and the first for declaration names
# (`Extract.lean:writeLinkIndex`), and M5-a recorded that as a divergence this
# target cannot exhibit. This is the target that can.
cat > "$OUT/Alpha/Odd-Name.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.OddName

/-- Lives in a module Lean spells `Alpha.«Odd-Name»` and Lake spells
`Alpha/Odd-Name.lean`. -/
def oddNameConst : Nat := 7

end Alpha.OddName
LEAN

cat > "$OUT/Alpha/Private.lean" <<'LEAN'
import Alpha.Basic

namespace Alpha.Private

/-- Private: its real name begins with `_private.`, and doc-gen4 drops it on the
way into the JSON (`Output/ToJson.lean:136`). -/
private def hidden : Nat := 8

/-- Private theorem, same treatment. -/
private theorem hidden_pos : 0 < hidden := by decide

/-- Public, and defined in terms of the private one. -/
def visible : Nat := hidden + 1

end Alpha.Private
LEAN

cat > "$OUT/Beta.lean" <<'LEAN'
import Beta.Basic
import Beta.DupNames
import Beta.Referrer
import Beta.TokenSep
LEAN

cat > "$OUT/Beta/Basic.lean" <<'LEAN'
import Alpha.Basic

/-!
# The second library

`Beta` exists so that `--lib` has to find two `[[lean_lib]]` blocks rather than
one.
-/

namespace Beta.Basic

/-- One more than `Alpha.Basic.alphaConst`. The code span is what the
whole-package map delta (L3-2) reads. -/
def betaConst : Nat := Alpha.Basic.alphaConst + 1

end Beta.Basic
LEAN

# U+088F ARABIC HALF MADDA OVER MADDA: the first of the 4,803 code points V8
# splits tokens on and UnicodeBasic does not (plan §8, V6 —
# `benchmarks/results/m2b-v6-token-separators.json`). It is inside a code span
# because that is the only place `ModuleFacts::tokens` looks
# (`lean-doc-global/src/facts.rs`).
cat > "$OUT/Beta/TokenSep.lean" <<'LEAN'
import Alpha.Basic

namespace Beta.TokenSep

/-- A code span whose two halves are separated by U+088F:
`Alpha.Basic.alphaConst@@U088F@@tail`.

The delta splits on the **union** of both tables and therefore sees
`Alpha.Basic.alphaConst`; the renderer splits on UnicodeBasic's and therefore
does not link it. Widening the delta is the safe direction. -/
def tokenSep : Nat := 9

end Beta.TokenSep
LEAN
# Written as a marker rather than as a literal: the code point is invisible in
# every editor, so a literal here would be a byte nobody can review.
perl -CSD -0777 -i -pe 's/\@\@U088F\@\@/\x{088f}/g' "$OUT/Beta/TokenSep.lean"

cat > "$OUT/Beta/Referrer.lean" <<'LEAN'
import Alpha.Basic

namespace Beta.Referrer

/-- Names `Alpha.Basic.alphaConst` in its printed signature, which is what makes
this module a referrer in the IR's `refs` sense (L3-1). -/
def referrer : Nat := Alpha.Basic.alphaConst

/-- And again in a theorem statement. -/
theorem referrer_eq : referrer = Alpha.Basic.alphaConst := rfl

end Beta.Referrer
LEAN

cat > "$OUT/Beta/DupNames.lean" <<'LEAN'
import Alpha.Basic

namespace Beta.DupNames

/-- Forces `Alpha.Basic.step`'s equation lemmas to be realised here, so that a
name declared in one module ends up in a second module's olean — the shape
plan 決定 5 calls "one name, several modules" (25 of them on the measurement
target). -/
theorem step_zero : Alpha.Basic.step 0 = 0 := by simp [Alpha.Basic.step]

/-- And the successor case. -/
theorem step_succ (n : Nat) : Alpha.Basic.step (n + 1) = n := by
  simp [Alpha.Basic.step]

end Beta.DupNames
LEAN

# ------------------------------------------------------------ the oleans (CI)
#
# Last, because Lake needs the lakefile and the sources to exist before it will
# resolve anything, and because a failure here should leave a package that is
# complete except for its dependencies rather than a half-written tree.
if [ "$DEPS" = fetch ]; then
  echo "### lake exe cache get in $OUT (network; revisions pinned by the copied manifest)"
  (cd "$OUT" && lake exe cache get)
fi

# -------------------------------------------------------------------- the repo
#
# `lean-doc build` derives `--source-url` from git: `HEAD` has to be 40 hex
# digits (plan 決定 1 — the acceptance oracle normalises `/blob/[0-9a-f]{40}/`
# and nothing else) and the remote has to be a github.com one, because the
# `/blob/<rev>/<path>` shape is GitHub's and a guessed one 404s on every
# declaration of every page (M4-d). Nothing is ever pushed there.
cat > "$OUT/.gitignore" <<'IGNORE'
/.lake
IGNORE
#
# **The commit's dates are fixed**, so HEAD is the same 40 hex digits every time
# this script runs. That is not cosmetic: `--source-url` carries the revision and
# the revision reaches every page's bytes, so a re-generated target 2 with a new
# HEAD would make every number measured on the old one incomparable — and
# /private/tmp is emptied often enough that re-generating is the normal case.
git -C "$OUT" init -q
git -C "$OUT" add -A
GIT_AUTHOR_DATE="2026-08-15T00:00:00+0000" GIT_COMMITTER_DATE="2026-08-15T00:00:00+0000" \
  git -C "$OUT" -c user.name=lean-doc -c user.email=lean-doc@example.invalid \
  commit -q -m "target2: two libraries and six boundary values"
git -C "$OUT" remote add origin https://github.com/lean-doc/target2.git

echo "### target2 at $OUT"
echo "    deps    $DEPS"
echo "    HEAD    $(git -C "$OUT" rev-parse HEAD)"
echo "    remote  $(git -C "$OUT" config --get remote.origin.url)"
echo "    modules $(for m in $MODULES; do echo "$m"; done | wc -l | tr -d ' ')"
echo
echo "next: (cd $OUT && lake build)"
