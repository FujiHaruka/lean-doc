import «Dep-Aux».Basic

/-!
# Dep

The module that reaches into a dependency. Everything interesting about it is in
one docstring, because the question is not what Lean does with the import — it
is what the **renderer** does with a module it can neither give a page to nor
pin a revision for.

`../../micro-dep` is required by path, so its manifest entry has no `url` and no
`rev`. `crates/litedoc4/src/packages.rs` drops such an entry, which means the
root `«Dep-Aux»` is not in the external-link map, which means every reference
below is a dependency reference that cannot become a blob URL.
-/

/--
Names `DepAux.marker`, a declaration in a dependency module.

The module itself is spelled three ways on purpose, because the `.lidx` writes
module names **unescaped** and the IR does not:

* `«Dep-Aux».Basic` — the IR's spelling
* `Dep-Aux.Basic` — the `.lidx`'s spelling
* `Dep-Aux/Basic.lean` — a source path, which `module_for_source_path` escapes
  before it looks anything up

Whether each of the three resolves is what README's 未検証項目 #4 is about, and
`../README.md` records what was measured.
-/
def Micro.usesDep : Nat := DepAux.marker + 1
