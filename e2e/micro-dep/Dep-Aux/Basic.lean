/-!
# Dep-Aux.Basic

A dependency module with something worth linking to. The module's own name is
`«Dep-Aux».Basic` in the IR and in the import list of anything that imports it,
and `Dep-Aux.Basic` in the `.lidx` — the extractor writes module names
unescaped there. Whether that difference costs a link is what
`Micro/Dep.lean`'s docstring measures.
-/

/-- A declaration a docstring in the documenting package names by hand, so that
the name-resolution path has something in a dependency module to find. -/
def DepAux.marker : Nat := 41
