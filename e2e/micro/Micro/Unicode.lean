import Micro.Basic

/-!
# Unicode

The two traps of `docs/implementation-plan.md` §7, on purpose:

* **U1** — sorting. `Vec<String>::sort()` is UTF-8 byte order, JavaScript's
  `Array.prototype.sort()` is UTF-16 code-unit order, and they **disagree above
  U+FFFF**. `𝒜` is U+1D49C, which is exactly there.
* **U2** — spans. IR offsets are UTF-16 code units, so a docstring holding an
  astral character (`𝒜`) makes every naive byte-offset slice land in the middle
  of a character.

The corpus caught these because it happened to contain such names; this fixture
contains them **by construction**, so a machine that has never seen the corpus
still runs into them.
-/

namespace Micro

/-- The script capital `𝒜` (U+1D49C) lives outside the BMP, which is where U1
and U2 bite. -/
def script𝒜 : Nat := 1

/-- Markdown in a docstring, so that the CommonMark path is exercised end to
end.

# A heading, whose id comes from the UnicodeBasic split table

A paragraph with a `code span`, a reference to `Micro.double`, and a list:

* first
* second

The heading above is the one shape that puts a generated id into the page's
bytes (`heading_id`), so a change to the split table shows here. -/
def documented : Nat := script𝒜 + double 1

end Micro
