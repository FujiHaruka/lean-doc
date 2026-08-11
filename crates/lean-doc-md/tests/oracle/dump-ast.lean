/-
dump-ast.lean -- print MD4Lean's own AST for a corpus of docstrings.

WHY THIS FILE EXISTS
--------------------
`tests/md4lean.rs` checks that this crate's parser builds the same tree as
`MD4Lean.parse`, which is the tree doc-gen4 renders. An expected value derived
from reading `wrapper.c` would prove nothing: both sides would share whatever
mistake the reading made. So the expected values are produced here, by running
MD4Lean itself under Lean, against the flags doc-gen4 uses.

Input:  one JSON `[flags, markdown]` pair per line. `flags` is an
        `MD_FLAG_*` bitmask; it is per line so that the corners the docstring
        dialect cannot reach (underline, wiki links, raw HTML) can still be
        answered by MD4Lean rather than by hand.
Output: one JSON value per line, in the same order -- the encoding below, or
        `null` if `MD4Lean.parse` returned `none`.

The encoding is all arrays, never objects, so that neither side has to agree
about key order. It is tagged with the Lean constructor name so that a tree
which differs only in *which* constructor was used still differs here.

usage (from the measurement target, which is where MD4Lean is built):
  cd /Users/haruka/dev/lean-projects
  lake env lean \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libleanmd4c.dylib \
    --load-dynlib=.lake/packages/MD4Lean/.lake/build/lib/libMD4Lean_MD4Lean.dylib \
    --run dump-ast.lean <corpus.jsonl> <out.jsonl>

Nothing is written inside the target package: both paths are given by the
caller.
-/
import MD4Lean
import Lean.Data.Json

open Lean

/--
The flags doc-gen4 parses docstrings with (`DocGen4/Output/DocString.lean:393`),
expressed in MD4Lean's own constants. Printed so that the generator's idea of
the default and this one cannot drift apart silently; the flags actually used
arrive per input line.
-/
def docstringFlags : UInt32 :=
  MD4Lean.MD_DIALECT_GITHUB ||| MD4Lean.MD_FLAG_LATEXMATHSPANS ||| MD4Lean.MD_FLAG_NOHTML

def tag (name : String) (args : Array Json) : Json :=
  Json.arr (#[Json.str name] ++ args)

def encChar (c : Char) : Json := Json.str (String.singleton c)

def encOptChar : Option Char → Json
  | none => Json.null
  | some c => encChar c

def encNat (n : Nat) : Json := Json.num (JsonNumber.fromNat n)

def encStrings (ss : Array String) : Json := Json.arr (ss.map Json.str)

def encAttrText : MD4Lean.AttrText → Json
  | .normal s => tag "normal" #[Json.str s]
  | .entity s => tag "entity" #[Json.str s]
  | .nullchar => tag "nullchar" #[]

def encAttrTexts (a : Array MD4Lean.AttrText) : Json := Json.arr (a.map encAttrText)

partial def encText : MD4Lean.Text → Json
  | .normal s => tag "normal" #[Json.str s]
  | .nullchar => tag "nullchar" #[]
  | .br s => tag "br" #[Json.str s]
  | .softbr s => tag "softbr" #[Json.str s]
  | .entity s => tag "entity" #[Json.str s]
  | .em ts => tag "em" #[Json.arr (ts.map encText)]
  | .strong ts => tag "strong" #[Json.arr (ts.map encText)]
  | .u ts => tag "u" #[Json.arr (ts.map encText)]
  | .a href title isAuto ts =>
    tag "a" #[encAttrTexts href, encAttrTexts title, Json.bool isAuto, Json.arr (ts.map encText)]
  | .img src title alt =>
    tag "img" #[encAttrTexts src, encAttrTexts title, Json.arr (alt.map encText)]
  | .code ss => tag "code" #[encStrings ss]
  | .del ts => tag "del" #[Json.arr (ts.map encText)]
  | .latexMath ss => tag "latexMath" #[encStrings ss]
  | .latexMathDisplay ss => tag "latexMathDisplay" #[encStrings ss]
  | .wikiLink target ts => tag "wikiLink" #[encAttrTexts target, Json.arr (ts.map encText)]

def encTexts (ts : Array MD4Lean.Text) : Json := Json.arr (ts.map encText)

mutual

partial def encLi (li : MD4Lean.Li MD4Lean.Block) : Json :=
  Json.arr #[
    Json.bool li.isTask,
    encOptChar li.taskChar,
    (match li.taskMarkOffset with
      | none => Json.null
      | some o => encNat o.toNat),
    Json.arr (li.contents.map encBlock)
  ]

partial def encBlock : MD4Lean.Block → Json
  | .p ts => tag "p" #[encTexts ts]
  | .ul tight mark items =>
    tag "ul" #[Json.bool tight, encChar mark, Json.arr (items.map encLi)]
  | .ol tight start mark items =>
    tag "ol" #[Json.bool tight, encNat start, encChar mark, Json.arr (items.map encLi)]
  | .hr => tag "hr" #[]
  | .header level ts => tag "header" #[encNat level, encTexts ts]
  | .code info lang fenceChar content =>
    tag "code" #[encAttrTexts info, encAttrTexts lang, encOptChar fenceChar, encStrings content]
  | .html ss => tag "html" #[encStrings ss]
  | .blockquote bs => tag "blockquote" #[Json.arr (bs.map encBlock)]
  | .table head body =>
    tag "table" #[
      Json.arr (head.map encTexts),
      Json.arr (body.map fun row => Json.arr (row.map encTexts))
    ]

end

def encDocument (doc : MD4Lean.Document) : Json := Json.arr (doc.blocks.map encBlock)

/-- One input line: `[flags, markdown]`. -/
def parseLine (json : Json) : Except String (UInt32 × String) := do
  let items ← json.getArr?
  if items.size = 2 then
    let flags ← items[0]!.getNat?
    let md ← items[1]!.getStr?
    return (UInt32.ofNat flags, md)
  else
    throw s!"expected [flags, markdown], got {items.size} elements"

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
          | .ok (flags, md) =>
            let out :=
              match MD4Lean.parse md flags with
              | none => Json.null
              | some doc => encDocument doc
            handle.putStr out.compress
            handle.putStr "\n"
            -- Flushed per line on purpose: two inputs kill this process
            -- outright (see gen-md4lean-expected.ts), and the caller finds
            -- which one by counting the answers that made it to disk.
            handle.flush
    IO.eprintln s!"{lines.length} docstrings -> {outPath} (docstringFlags = {docstringFlags})"
    return 0
  | _ =>
    IO.eprintln "usage: dump-ast.lean <corpus.jsonl> <out.jsonl>"
    return 2
