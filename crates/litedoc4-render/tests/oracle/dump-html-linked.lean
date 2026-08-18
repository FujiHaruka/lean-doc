/-
dump-html-linked.lean -- print *doc-gen4's own* HTML for docstrings whose names
actually resolve.

WHY THIS FILE EXISTS
--------------------
`crates/litedoc4-md/tests/oracle/dump-html.lean` runs `docStringToHtml` with an
empty `AnalyzerResult`, which makes every lookup miss. That was the right oracle
for M1-c's first two steps -- it matches `NoLinks` -- but it is blind to the
step that turns the lookups on: with no name resolving, `autoLinkInline` emits
text either way, and `renderText`'s `inLink` flag has nothing to suppress.

This script fills in the two fields `nameToLink?` needs for its first three
branches, so doc-gen4 answers questions it could not be asked before. It is a
*referee*, not the primary oracle: the thing this port has to reproduce is
`experiments/stage7d/render.ts` (plan §5), whose `nameToLink` is a different
function. Where the two specifications provably agree, doc-gen4 settles it.

THE AGREEMENT ZONE
------------------
doc-gen4 and the prototype differ in three places, and the caller keeps all
three out of the corpus rather than papering over them here:

  * `isPrivateName` (name mangling) vs a `_private.` prefix test;
  * doc-gen4 links an auto-generated eliminator to its parent type;
  * the "similar name in this module" branch, which needs `moduleInfo`.

The last one is disabled *structurally*: `currentName := none` makes doc-gen4
return `none` there, and the caller renders the Rust side with an empty
declaration list so that it does too. The first two are avoided by construction
(the caller does not put such names into the world).

Input:  one JSON `[depthToRoot, markdown, moduleNames, name2ModIdx]` per line,
        where `moduleNames` is an array of module name strings and
        `name2ModIdx` an array of `[name, index]` pairs. Names are decoded with
        `Lean.Syntax.decodeNameLit`, which is what `nameToLink?` itself uses, so
        the keys are the `Name`s it will look for.
Output: one JSON string per line, in the same order.

usage (from the measurement target, which is where doc-gen4 is built):
  cd /Users/haruka/dev/lean-projects
  lake env lean \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libleanmd4c.dylib \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libMD4Lean_MD4Lean.dylib \
    --load-dynlib=.lake/packages/UnicodeBasic/.lake/build/lib/libUnicodeBasic_UnicodeBasic.dylib \
    --run dump-html-linked.lean <corpus.jsonl> <out.jsonl>

Nothing is written inside the target package: both paths are given by the caller.
-/
import DocGen4.Output.DocString
import Lean.Data.Json

open Lean DocGen4 DocGen4.Output

/-- `currentName := none` is load-bearing: it is what makes `nameToLink?`'s last
branch give up instead of indexing `moduleInfo`, which this script cannot build. -/
def baseContext (depthToRoot : Nat) : SiteBaseContext := {
  buildDir := "."
  hierarchy := Hierarchy.empty Name.anonymous false
  depthToRoot := depthToRoot
  currentName := none
  refs := #[]
}

/-- The same decoding `nameToLink?` performs on the word it is given. -/
def decodeName (s : String) : Except String Name :=
  match Lean.Syntax.decodeNameLit ("`" ++ s) with
  | some n => .ok n
  | none => .error s!"not a name literal: {s}"

def siteContext (moduleNames : Array Name) (entries : Array (Name × Nat)) : SiteContext := {
  result := {
    name2ModIdx := entries.foldl (fun m (n, i) => m.insert n i) {}
    moduleNames := moduleNames
    moduleInfo := {}
  }
  sourceLinker := fun _ _ => ""
  refsMap := {}
}

def renderOne (depthToRoot : Nat) (md : String) (ctx : SiteContext) : String :=
  let (html, _) := (docStringToHtml (.inl md) "").run {} ctx (baseContext depthToRoot)
  String.join (html.toList.map Html.toStringAux)

/-- One input line: `[depthToRoot, markdown, moduleNames, name2ModIdx]`. -/
def parseLine (json : Json) : Except String (Nat × String × Array Name × Array (Name × Nat)) := do
  let items ← json.getArr?
  if items.size != 4 then
    throw s!"expected 4 elements, got {items.size}"
  let depth ← items[0]!.getNat?
  let md ← items[1]!.getStr?
  let modulesJson ← items[2]!.getArr?
  let mut modules : Array Name := #[]
  for m in modulesJson do
    modules := modules.push (← decodeName (← m.getStr?))
  let entriesJson ← items[3]!.getArr?
  let mut entries : Array (Name × Nat) := #[]
  for e in entriesJson do
    let pair ← e.getArr?
    if pair.size != 2 then
      throw s!"expected [name, index], got {pair.size} elements"
    entries := entries.push (← decodeName (← pair[0]!.getStr?), ← pair[1]!.getNat?)
  return (depth, md, modules, entries)

def main (args : List String) : IO UInt32 := do
  match args with
  | [inPath, outPath] =>
    let input ← IO.FS.readFile inPath
    let lines := input.splitOn "\n" |>.filter (· != "")
    IO.FS.withFile outPath IO.FS.Mode.write fun handle => do
      let mut lineno := 0
      for line in lines do
        lineno := lineno + 1
        match Json.parse line with
        | .error e => throw <| IO.userError s!"{inPath}:{lineno}: {e}"
        | .ok json =>
          match parseLine json with
          | .error e => throw <| IO.userError s!"{inPath}:{lineno}: {e}"
          | .ok (depth, md, modules, entries) =>
            handle.putStr (Json.str (renderOne depth md (siteContext modules entries))).compress
            handle.putStr "\n"
            handle.flush
    IO.eprintln s!"{lines.length} docstrings -> {outPath}"
    return 0
  | _ =>
    IO.eprintln "usage: dump-html-linked.lean <corpus.jsonl> <out.jsonl>"
    return 2
