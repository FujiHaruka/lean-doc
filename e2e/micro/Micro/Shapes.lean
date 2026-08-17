import Micro.Basic

/-!
# Shapes

**The declaration shapes the measurement target does not contain.**

`crates/lean-doc-render/tests/page_parts.rs` records that nine of the renderer's
41 branches never fire over the whole 432-module package: there is no `class`,
no `inductive` and no `class_inductive` declaration, no structure whose
constructor is not `mk`, no structure without a `ctor` member, no inherited
field whose projection is declared inside the structure's range, no implicit
binder on a field, and no module without imports.

Curated unit tests reach those branches with hand-written IR. Nothing reached
them **through the real pipeline** — extractor to IR to page — until this
module, which is why the fixture carries one of each. A branch that only ever
runs against IR somebody typed by hand is a branch whose *input* has never been
checked.

`Micro.Basic` is the module without imports; the rest live here.
-/

namespace Micro

/-- A class, so that `class` is not a branch only curated IR reaches. -/
class Greet (α : Type) where
  /-- How to greet a value. -/
  greet : α → String

/-- An instance of that class, so the class has a member in the instance table. -/
instance : Greet Nat where
  greet n := s!"hello {n}"

/-- A `class inductive`: a class whose declaration is an inductive type rather
than a structure. -/
class inductive Decision (p : Prop) where
  /-- The negative case, carrying a refutation. -/
  | no (refutation : p → False)
  /-- The positive case, carrying a proof. -/
  | yes (proof : p)

/-- A structure whose constructor is **not** `mk`, which the renderer prints
differently from the anonymous one. -/
structure Named where
  /-- The named constructor. -/
  make ::
  /-- The only field. -/
  value : Nat

/-- The base of an `extends`, so that `Derived` below has an inherited field. -/
structure Base where
  /-- A field that `Derived` inherits. -/
  b : Nat

/-- A structure with an inherited field, so the field-inheritance branch fires
through the real pipeline. -/
structure Derived extends Base where
  /-- A field of its own. -/
  d : Nat

/-- A structure with an **implicit binder** on a field. -/
structure Poly where
  /-- A field whose own signature binds a type implicitly. -/
  apply : {α : Type} → α → α

/-- A class extending one that belongs to **Lean core rather than this package**.

`Derived` above inherits from `Base`, which this package declares, so the IR
this run is handed knows where the inherited field lives. Here it does not:
`Inhabited.default` is core's, so `declNameToLink`'s lookup — the IR's own map,
which is what doc-gen4 uses — has nothing for it.

**The measurement target never produced this shape and `batteries` did on the
first page it was pointed at**【実測 2026-08-17】: `class LawfulLTCmp … extends
Std.OrientedCmp` made `lean-doc build` stop with `no defining module for
Std.OrientedCmp.eq_swap`, having rendered nothing. The name is in the `.lidx`
all along — the whole environment is — so the fix was to let the lookup fall
through to it. This declaration is what keeps that path exercised. -/
class Preferred (α : Type) extends Inhabited α where
  /-- Why the inherited `default` is the preferred value. -/
  reason : String

/-- An instance, so the class above reaches the instance table like `Greet`. -/
instance : Preferred Nat where
  default := 7
  reason := "seven is small and not zero"

end Micro
