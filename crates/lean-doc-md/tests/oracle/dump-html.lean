/-
dump-html.lean -- print *doc-gen4's own* HTML for a corpus of docstrings.

WHY THIS FILE EXISTS
--------------------
`crates/lean-doc-md/src/html.rs` claims to be `DocGen4/Output/DocString.lean`
transcribed. An expected value written by reading that file would prove nothing:
the reading and the port would share whatever mistake was made. So the expected
HTML comes from doc-gen4 itself — this script calls `docStringToHtml`, the same
entry point the real generator calls, and prints the bytes it produces.

WHY NO ENVIRONMENT IS NEEDED
----------------------------
`docStringToHtml` runs in `HtmlM`, whose context carries an `AnalyzerResult`.
That is only ever consulted through `nameToLink?`, and with the *default*
(empty) result every lookup misses: no `name2ModIdx`, no `moduleNames`, and
`currentName := none` so the "similar name in this module" branch returns
immediately. That is exactly the behaviour of `NoLinks` on the Rust side, so
the two can be compared byte for byte over the whole corpus rather than over a
subset with the auto-links filtered out.

The rest of the context is inert for this path: `sourceLinker` is never called,
`refsMap` is empty (the target package has no `.bib`, so `findAllReferences`
finds nothing and `findBibitem?` never matches), and `hierarchy` is only read by
the page templates.

WHAT IS COMPARED
----------------
`Html.toStringAux`, concatenated over the returned array — *not* `Html.toString`,
which additionally trims the end. Trimming happens once per page, around the
whole document; a docstring is one node inside it, and trimming here would hide
trailing whitespace that the real output keeps.

SOME INPUTS CRASH IT
--------------------
The same two that kill `MD4Lean.parse` (see `gen-md4lean-expected.ts`): a NUL
inside a fenced code block, and a GFM table with a header and no body rows. The
caller resumes past them by counting the answers that reached disk.

Input:  one JSON `[depthToRoot, markdown]` pair per line. `depthToRoot` decides
        `getRoot`, which is prepended to every relative link, so it is part of
        the bytes and the caller varies it.
Output: one JSON string per line, in the same order.

usage (from the measurement target, which is where doc-gen4 is built):
  cd /Users/haruka/dev/lean-projects
  lake env lean \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libleanmd4c.dylib \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libMD4Lean_MD4Lean.dylib \
    --run dump-html.lean <corpus.jsonl> <out.jsonl>

Nothing is written inside the target package: both paths are given by the caller.
-/
import DocGen4.Output.DocString
import Lean.Data.Json

open Lean DocGen4 DocGen4.Output

/-- Everything the page templates would fill in, emptied. `currentName := none`
is load-bearing: it is what makes `nameToLink?`'s last branch give up instead of
indexing `moduleInfo`. -/
def baseContext (depthToRoot : Nat) : SiteBaseContext := {
  buildDir := "."
  hierarchy := Hierarchy.empty Name.anonymous false
  depthToRoot := depthToRoot
  currentName := none
  refs := #[]
}

/-- `result := default` is the empty `AnalyzerResult`, so every name lookup
misses. -/
def siteContext : SiteContext := {
  result := default
  sourceLinker := fun _ _ => ""
  refsMap := {}
}

def renderOne (depthToRoot : Nat) (md : String) : String :=
  let (html, _) := (docStringToHtml (.inl md) "").run {} siteContext (baseContext depthToRoot)
  String.join (html.toList.map Html.toStringAux)

/-- One input line: `[depthToRoot, markdown]`. -/
def parseLine (json : Json) : Except String (Nat × String) := do
  let items ← json.getArr?
  if items.size = 2 then
    let depth ← items[0]!.getNat?
    let md ← items[1]!.getStr?
    return (depth, md)
  else
    throw s!"expected [depthToRoot, markdown], got {items.size} elements"

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
          | .ok (depth, md) =>
            handle.putStr (Json.str (renderOne depth md)).compress
            handle.putStr "\n"
            -- Flushed per line on purpose: two inputs kill this process
            -- outright, and the caller finds which one by counting the answers
            -- that made it to disk.
            handle.flush
    IO.eprintln s!"{lines.length} docstrings -> {outPath}"
    return 0
  | _ =>
    IO.eprintln "usage: dump-html.lean <corpus.jsonl> <out.jsonl>"
    return 2
