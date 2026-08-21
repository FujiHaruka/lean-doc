/-!
# Gen

**The shapes `@[ext]` builds, and the one it does not.**

`docs/plans/b0-generated-decls.md` measured that a declaration Lean realizes
from an attribute is given the position of *the attribute token*, not of its
parent, and that no rule over `(line, col)` recovers the parent from that. B-3's
answer is that the extractor names the origin itself, and `@[ext]` is the only
attribute it can name without importing Mathlib — `extExtension` lives in Lean
core while `simps` / `to_additive` / `mk_iff` / `to_dual` are Mathlib's, and an
extractor that imported Mathlib would stop building against this very fixture.

Four positions, and the last one is why the gate cannot be a count:

| | |
|---|---|
| `Pair` | `@[ext]` written on the structure. Both realized theorems land **inside** the parent's own range |
| `Trip` | `attribute [ext] Trip` on a later line. The realized theorems land **completely outside** the parent's range |
| `Quad` / `Quint` | `attribute [ext] Quad Quint` — **one position, two parents, four realized theorems.** No rule over positions can split this group, and each theorem still has to name its own parent |
| `Solo` | a **hand-written** `@[ext] theorem`. It is in the same environment extension as the realized ones and must get the **opposite** answer; the `Solo.ext_iff` the attribute realizes *from* it must get a positive one naming `Solo.ext` |

`PairPlus` adds the parent projection, which is realized too and is *not*
`@[ext]`'s: it must stay unclaimed.

The measurement target has one `@[ext]` and two realized theorems in 4,584
declarations 【実測 2026-08-21】, and `Micro.Gen.Solo` is a shape it does not
have at all.
-/

namespace Micro.Gen

/-- A structure carrying `@[ext]` inline. `Pair.ext` and `Pair.ext_iff` are
realized at the `ext` token on the attribute line, which is inside this
declaration's own range. -/
@[ext]
structure Pair where
  /-- The first component. -/
  fst : Nat
  /-- The second component. -/
  snd : Nat

/-- A structure whose `@[ext]` arrives later, as a separate `attribute` command.
Its realized theorems land outside this range entirely. -/
structure Trip where
  /-- The first component. -/
  a : Nat
  /-- The second component. -/
  b : Nat
  /-- The third component. -/
  c : Nat

attribute [ext] Trip

/-- One of the two structures a single `attribute [ext] Quad Quint` marks. -/
structure Quad where
  /-- The only component. -/
  q : Nat

/-- The other one. Four theorems are realized at one position, with two
different parents between them. -/
structure Quint where
  /-- The only component. -/
  r : Nat

attribute [ext] Quad Quint

/-- A structure extending `Pair`, so that the parent projection `toPair` is
realized as well. It is not `@[ext]`'s and must not be claimed as such. -/
structure PairPlus extends Pair where
  /-- The extra component. -/
  extra : Nat

/-- A structure with **no** `@[ext]` on it. The extensionality theorem below is
written out by hand instead. -/
structure Solo where
  /-- The only component. -/
  s : Nat

/-- A **hand-written** extensionality theorem carrying `@[ext]`.

This is the declaration that keeps "is in `extExtension`" from being read as
"was realized": it is in the extension exactly like `Pair.ext` is, and the
source names it, so the extractor must stay silent about it. The `Solo.ext_iff`
the attribute realizes *from* it is a different matter and does get named. -/
@[ext]
theorem Solo.ext : ∀ {x y : Solo}, x.s = y.s → x = y
  | ⟨_⟩, ⟨_⟩, rfl => rfl

end Micro.Gen
