/-!
# Attrs

One declaration per **kind** of attribute the extractor's four collectors
produce, so that the split into `(name, value)` (schema 5,
`docs/plans/feature-sweep.md` B-2) meets a real Lean environment rather than a
hand-written IR fixture. The collectors read environment extensions — which
attribute is applied and what its argument elaborated to are facts about the
*environment*, so nothing written by hand can check that the extractor put the
right name and the right value in the right halves.

| kind | `getAllAttributes` collector | here |
|---|---|---|
| custom, no value | `getCustomAttrs` | `@[simp]`, `@[reducible]` |
| tag, no value | `getTags` | `@[match_pattern]` |
| enum, name *is* the value | `getEnumValues` | `@[inline]` |
| parametric, empty value | `getParametricValues` | `@[specialize]` — prints `#[]` |
| parametric, value with spaces | `getParametricValues` | `@[deprecated … (since := …)]` |
| instance priority | `InstanceInfo.ofDefinitionInfo` | `instance (priority := 100)` |

**The two parametric ones are the point.** A reader handed the schema-4
concatenation would have to guess where the name ends, and `deprecated`'s value
contains spaces, parentheses and quotes while `specialize`'s contains brackets.
The measurement target has neither shape — 163 attribute occurrences over 6
distinct strings, all of them either bare names or one `deprecated`【実測
2026-08-21】 — so this fixture is the only place the hard cases exist at all.

This module has **no imports on purpose**: `Micro/Basic.lean` is what GATE 6
appends its probe to, and every module that imports it is re-rendered by that
edit. Importing it here would spend GATE 6's `pagesRendered < modules` margin to
buy nothing.

**Do not "tidy" the attributes away.** Each line below is the only occurrence of
its kind in this repository's e2e path.
-/

namespace Micro.Attrs

/-- An enum-valued attribute (`Compiler.inlineAttrs`), whose printed form is the
enum's own name with no argument: the IR pair is `["inline", ""]`. -/
@[inline] def scale (n : Nat) : Nat := n * 2

/-- A custom attribute with no value (`getCustomAttrs`, the `simp` extension):
`["simp", ""]`. -/
@[simp] theorem scale_zero : scale 0 = 0 := rfl

/-- A custom attribute that comes from the reducibility status rather than from a
list: `["reducible", ""]`. -/
@[reducible] def Weight : Type := Nat

/-- A tag attribute (`getTags`, `matchPatternAttr`): `["match_pattern", ""]`. -/
@[match_pattern] def zero : Nat := 0

/-- A parametric attribute whose value is empty in *this* use but is still a
value: `Compiler.specializeAttr`'s parameter is an `Array Nat`, so with no
argument the printed form is `#[]` and the pair is `["specialize", "#[]"]`.
Brackets in a value are what a downstream split would have to survive. -/
@[specialize] def applyTwice (f : Nat → Nat) (n : Nat) : Nat := f (f n)

/-- A parametric attribute whose value **contains spaces, parentheses and
quotes**: `["deprecated", "Micro.Attrs.scale (since := \"2026-08-21\")"]`. The
concatenation schema 4 carried cannot be taken apart again by anyone who does not
already know that `deprecated` is one word — which is the whole reason the
extractor emits the pair.

Nothing uses this declaration; a use would make `lake build` warn, and this
fixture's warnings are read. -/
@[deprecated scale (since := "2026-08-21")]
def scaleOld (n : Nat) : Nat := n * 2

/-- A class of its own rather than one of Lean's, so that the instance below is
the only one in scope and the priority is not competing with anything. -/
class Tiny (α : Type) where
  /-- The small element. -/
  tiny : α

/-- The first of the two attributes `InstanceInfo.ofDefinitionInfo` appends after
`getAllAttributes`: a non-default priority prints as `["instance", "100"]`. The
measurement target has no instance with a non-default priority, so this is the
only place the pair exists. -/
instance (priority := 100) tinyNat : Tiny Nat := ⟨0⟩

/-- The second: `["defaultInstance", "<priority>"]`. It comes from
`getDefaultInstances` rather than from the attribute list, which is why it needs
a declaration of its own — nothing about `tinyNat` above would reach this branch.
Also the only instance here without an explicit priority, so the two lines
together show the branch is keyed off the attribute and not off the number. -/
@[default_instance] instance tinyBool : Tiny Bool := ⟨false⟩

end Micro.Attrs
