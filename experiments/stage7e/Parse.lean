/-
Stage 7e experiment for lean-doc: **how much does a syntax-only pass cost, and
does it need an environment?**

`docs/approach.md` §8 lists "syntactic approximation" as option (1) for a preview
mode and says its cost is "the Lean parser without elaboration". That sentence is
an assumption, and it hides a prerequisite: Lean's parser is *table driven from
the environment*. `notation` / `macro` / `syntax` commands install tokens and
parsers into `Lean.Parser.parserExtension`, so what a file even tokenizes as
depends on what is imported and on which scoped namespaces are active.

This program measures both halves separately:

* `--env init|mathlib|full` changes only the `importModules` argument, so the
  same 432 modules can be parsed against an essentially empty environment, a
  Mathlib-only one, and the one a resident server would hold. Comparing the
  *parse error counts* across the three answers "does the parser depend on the
  environment" with numbers rather than with a claim.
* `--open <ns,..>` additionally activates scoped extensions (`Lean.activateScoped`,
  the same call `experiments/stage7d/Extract.lean` uses for `--open`) and puts the
  namespaces into `ParserModuleContext.openDecls`. Scoped notation is *not* active
  in a merely imported environment; if a module still fails to parse with the
  right namespace pre-opened, the missing piece is elaboration of the file's own
  commands, which is a stronger statement than "imports are needed".

Nothing is elaborated: the pipeline is `mkInputContext` -> `parseHeader` ->
`parseCommand` until `Lean.Parser.Command.eoi`.

**The clock traps.** `parseCommand` is pure (`Id.run`), and Lean's compiler is
free to float a pure `let` down to its use site — three phases of the stage-7
experiments measured a zero that way. Every round therefore counts syntax nodes
into a mutable accumulator and calls `pin` on it before the closing clock read,
exactly as `experiments/stage7d/Extract.lean:742` does. A `parseUs` of 0 means
the trap was stepped in, not that parsing is free.

## Second half (stage 7e-B): per-file activation, declaration names, signatures

The gate above establishes that a single global activation set cannot work: one
set that fixes 161 modules breaks 32 others. The remaining question is whether a
*per-file* set works, and what it costs. `--two-pass` implements the only shape
that can be implemented without elaboration:

1. parse the module against the base environment, with nothing activated, and
   read `open` / `open scoped` / `open .. in` / `namespace` out of the module's
   *own* syntax tree;
2. `Lean.activateScoped` exactly those namespaces on an environment derived from
   the base one, push them into `openDecls`, and parse the same module again.

`--accum` threads the activated environment forward into the next module instead
of deriving each module from the base one. It exists to *measure* leakage, not
because it is a sensible mode: the whole point is that `ContDiff`'s `ω` must not
follow one module into the next.

`--decls <out.jsonl>` writes what the syntax tree alone can say about each
declaration: the name (qualified with a namespace stack this file tracks itself,
because `ParserModuleContext.currNamespace` is always anonymous here) plus the
raw source text of the binder and type regions. That file is meant to be diffed
against an independently produced IR, not against anything this program computes.

usage:
  parse <modules.txt> <out.jsonl> [--env full|mathlib|init] [--repeat N]
                                  [--open <ns,..>] [--print-errors]
                                  [--two-pass] [--accum] [--reverse]
                                  [--decls <decls.jsonl>]

Writes one JSONL record per (module, round) and a summary on stdout.
-/
import Lean

open Lean System

namespace Stage7e

/-! ## Clock pinning -/

/--
Consumes a pure value while the clock is running.

`@[noinline]` is load-bearing, not decoration: without an opaque use the whole
parse can be floated past the closing `IO.monoNanosNow`. Copied from
`experiments/stage7d/Extract.lean`.
-/
@[noinline] def pin (n : Nat) : BaseIO Unit :=
  if n == 0 then return () else return ()

/-! ## Configuration -/

inductive EnvMode where
  /-- Import every module in `<modules.txt>` — what a resident server holds. -/
  | full
  /-- Import `Mathlib` only. -/
  | mathlib
  /-- Import `Init` only — the closest thing to "no environment" that still has
  a token table. -/
  | init
  deriving BEq, Inhabited

def EnvMode.toString : EnvMode → String
  | .full => "full"
  | .mathlib => "mathlib"
  | .init => "init"

structure Cfg where
  modulesPath : FilePath
  outPath : FilePath
  envMode : EnvMode := .full
  rounds : Nat := 1
  openNamespaces : Array Name := #[]
  printErrors : Bool := false
  /-- Per-file activation: parse, read the file's own `open`/`namespace`, activate
  those, parse again. -/
  twoPass : Bool := false
  /-- Carry the activated environment into the next module (leak probe). -/
  accum : Bool := false
  /-- Walk the module list backwards; used to show the pass-1 result is order
  independent, i.e. that nothing leaks between modules. -/
  reverse : Bool := false
  /-- Where to write per-declaration records, if anywhere. -/
  declsPath : Option FilePath := none
  deriving Inhabited

/-! ## JSON helpers -/

def jsonEscape (s : String) : String :=
  s.foldl (init := "") fun acc c =>
    match c with
    | '"' => acc ++ "\\\""
    | '\\' => acc ++ "\\\\"
    | '\n' => acc ++ "\\n"
    | '\r' => acc ++ "\\r"
    | '\t' => acc ++ "\\t"
    | c => if c.toNat < 0x20 then acc ++ "?" else acc.push c

def jsonStr (s : String) : String := "\"" ++ jsonEscape s ++ "\""

/-! ## Syntax tree size -/

/-- Total node count, atoms and idents included. The result is what `pin` keeps
the parse from being optimised away. -/
partial def countNodes : Syntax → Nat
  | .missing => 1
  | .atom .. => 1
  | .ident .. => 1
  | .node _ _ args => args.foldl (init := 1) fun acc a => acc + countNodes a

/-! ## Walking the syntax tree

Everything below reads a parsed module and nothing else. In particular there is
no `Environment` lookup, no name resolution and no elaboration: if a fact is not
in the token stream of the file itself, this code cannot know it. That is the
point — the question being measured is exactly "how far does the file's own
syntax get you".
-/

/-- One scope pushed by `namespace`/`section`. `isNamespace := false` is a
`section`, whose (optional) label does *not* contribute to the current name. -/
structure Scope where
  isNamespace : Bool
  name : Name
  deriving Inhabited

/-- What `namespace A.B` / `section` / `end` say the current namespace is.

`ParserModuleContext.currNamespace` is anonymous for every command in this
program (nothing is elaborated), so this has to be tracked by hand. -/
def curNamespace (stack : Array Scope) : Name :=
  stack.foldl (init := .anonymous) fun acc s =>
    if s.isNamespace then acc ++ s.name else acc

/-- Pops the scopes closed by `end x?`.

`namespace A.B` is pushed as one scope, so `end A.B` normally pops one. Files
that write `namespace A` / `namespace B` / `end A.B` need more, hence the search
for the shortest suffix of the stack whose names concatenate to the target. A
target that matches nothing pops one scope and is counted as a mismatch. -/
def popScopes (stack : Array Scope) : Option Name → Array Scope × Bool
  | none => (stack.pop, true)
  | some target => Id.run do
    for k in [1:stack.size + 1] do
      let mut acc : Name := .anonymous
      for i in [stack.size - k : stack.size] do
        acc := acc ++ stack[i]!.name
      if acc == target then
        return (stack.shrink (stack.size - k), true)
    return (stack.pop, false)

/-- A declaration as the parser alone can see it. -/
structure DeclRec where
  /-- `curNamespace ++ declId`, i.e. what the file says the full name is. -/
  name : Name
  /-- The identifier exactly as written after the keyword. -/
  asWritten : Name
  /-- `theorem` / `def` / `instance` / ... -/
  kind : String
  line : Nat
  col : Nat
  /-- Raw source text of the binder group, verbatim (no reformatting). -/
  binders : String
  /-- Raw source text after `:`, verbatim. Empty when the signature omits it. -/
  typeText : String
  hasType : Bool
  /-- The declaration sat under `open .. in` / `set_option .. in` / ... -/
  underIn : Bool
  /-- The source wrote `private`. doc-gen4 does not document private
  declarations, so without this flag every `private theorem` in the file counts
  as a false positive against the IR. -/
  isPrivate : Bool
  deriving Inhabited

structure Scan where
  decls : Array DeclRec := #[]
  /-- Namespaces the file itself asks for: every `open`ed name and every prefix
  of every namespace the file enters. -/
  wanted : Array Name := #[]
  /-- `example`s and anonymous `instance`s: declarations the file has but does
  not name. They cannot be matched against an IR by name, so they are counted
  rather than emitted. -/
  unnamed : Nat := 0
  /-- `end` commands whose target did not match the scope stack. -/
  endMismatch : Nat := 0
  /-- Every command kind seen, handled or not. Printed so that a command the
  walker silently ignores shows up as a number instead of as a missing
  declaration nobody can explain. -/
  kinds : NameMap Nat := {}
  deriving Inhabited

def rangeText (text : String) (stx : Syntax) : String :=
  match stx.getPos? (canonicalOnly := false), stx.getTailPos? (canonicalOnly := false) with
  | some b, some e => if b.byteIdx ≤ e.byteIdx then String.Pos.Raw.extract text b e else ""
  | _, _ => ""

/-- Mathlib's `lemma` is *not* `Lean.Parser.Command.declaration`: it is its own
`syntax` that macro-expands to `theorem`. Without elaboration the expansion never
happens, so a syntax-only extractor has to know the command by name. This project
writes 1,919 of them against 2,322 `theorem`s, which is why it is special-cased
rather than left as a known gap. -/
def lemmaKind : SyntaxNodeKind := `lemma

/-- Maps a `declaration` body node onto the keyword the source used. -/
def bodyKind (stx : Syntax) : Option String :=
  let k := stx.getKind
  if k == ``Parser.Command.abbrev then some "abbrev"
  else if k == ``Parser.Command.definition then some "def"
  else if k == ``Parser.Command.theorem then some "theorem"
  else if k == ``Parser.Command.opaque then some "opaque"
  else if k == ``Parser.Command.instance then some "instance"
  else if k == ``Parser.Command.axiom then some "axiom"
  else if k == ``Parser.Command.example then some "example"
  else if k == ``Parser.Command.inductive then some "inductive"
  else if k == ``Parser.Command.classInductive then some "classInductive"
  else if k == ``Parser.Command.structure then some "structure"
  else none

/-- First child of the given kind, looking through `optional`.

Indices are not usable here: `declId` sits at position 1 in most declaration
bodies but at 3 in `instance` — and there it is written `optional (ppSpace >>
declId)`, so it is wrapped in a null node rather than being a direct child.
Missing that wrapper silently drops every *named* instance, which is why this
looks one level into null nodes. -/
def childOfKind (stx : Syntax) (k : SyntaxNodeKind) : Option Syntax :=
  match stx.getArgs.find? (·.isOfKind k) with
  | some c => some c
  | none => stx.getArgs.findSome? fun a =>
      if a.isOfKind nullKind then a.getArgs.find? (·.isOfKind k) else none

/-- Whether a `declModifiers` node carries `private`. `optional visibility`
wraps the `private` node in a null node, so this looks one level down. -/
def hasPrivate (mods : Syntax) : Bool :=
  mods.getArgs.any fun a =>
    a.isOfKind ``Parser.Command.private ||
      (a.isOfKind nullKind && a.getArgs.any (·.isOfKind ``Parser.Command.private))

/-- `declSig`/`optDeclSig` differ only in whether the type is optional, and
`optType` is a bare `optional`, i.e. a null node — so the `typeSpec` is either
the sig's second child or that child's only element. -/
def sigParts (sig : Syntax) : Syntax × Option Syntax :=
  let binders := sig.getArg 0
  let second := sig.getArg 1
  if second.isOfKind ``Parser.Term.typeSpec then (binders, some second)
  else match second.getArgs.find? (·.isOfKind ``Parser.Term.typeSpec) with
    | some ts => (binders, some ts)
    | none => (binders, none)

/-- The namespaces named by one `open` command body. `open A (f g)` and
`open A hiding f` name only `A`; `open A B` and `open scoped A B` name both. -/
def openedNames (openDecl : Syntax) : Array Name :=
  let k := openDecl.getKind
  if k == ``Parser.Command.openSimple then
    (openDecl.getArg 0).getArgs.filterMap fun s => if s.isIdent then some s.getId else none
  else if k == ``Parser.Command.openScoped then
    (openDecl.getArg 1).getArgs.filterMap fun s => if s.isIdent then some s.getId else none
  else
    match openDecl.getArgs.find? (·.isIdent) with
    | some s => #[s.getId]
    | none => #[]

/-- Every prefix of `n`, longest last. Entering `A.B.C` activates `A`, `A.B` and
`A.B.C`, the same way the elaborator pushes one scope per component. -/
def namePrefixes (n : Name) : Array Name := Id.run do
  let comps := n.components
  let mut acc : Name := .anonymous
  let mut out : Array Name := #[]
  for c in comps do
    acc := acc ++ c
    out := out.push acc
  return out

mutual

/-- Records one command. `underIn` is set for the right-hand side of `cmd in cmd`.

Returns the scope stack after the command, since `namespace`/`section`/`end`
change it for everything that follows. -/
partial def scanCommand (text : String) (fileMap : FileMap) (stack : Array Scope)
    (underIn : Bool) (acc0 : Scan) (stx : Syntax) : Array Scope × Scan :=
  let k := stx.getKind
  let acc := { acc0 with kinds := acc0.kinds.insert k ((acc0.kinds.find? k).getD 0 + 1) }
  if k == ``Parser.Command.declaration then
    (stack, scanDeclaration text fileMap stack underIn acc (stx.getArg 0) (stx.getArg 1) none)
  else if k == lemmaKind then
    -- `declModifiers >> group("lemma " declId declSig declVal)`: the group is a
    -- null node, so the body is arg 1 and the keyword has to be supplied here.
    (stack, scanDeclaration text fileMap stack underIn acc (stx.getArg 0) (stx.getArg 1) (some "lemma"))
  else if k == ``Parser.Command.namespace then
    let n := (stx.getArg 1).getId
    let stack := stack.push { isNamespace := true, name := n }
    let wanted := (namePrefixes (curNamespace stack)).foldl (init := acc.wanted) (·.push ·)
    (stack, { acc with wanted })
  else if k == ``Parser.Command.section then
    let lbl := stx.getArgs.find? (·.isIdent)
    (stack.push { isNamespace := false, name := lbl.map (·.getId) |>.getD .anonymous }, acc)
  else if k == ``Parser.Command.end then
    let target := (stx.getArgs.find? (·.isIdent)).map (·.getId)
    let (stack, ok) := popScopes stack target
    (stack, if ok then acc else { acc with endMismatch := acc.endMismatch + 1 })
  else if k == ``Parser.Command.open then
    let cur := curNamespace stack
    let names := openedNames (stx.getArg 1)
    let wanted := names.foldl (init := acc.wanted) fun w n =>
      -- `open X` inside namespace `C` may mean `C.X` or `X`; nothing here can
      -- resolve that, so both are asked for. Activating a namespace that does
      -- not exist is a no-op.
      let w := w.push n
      if cur.isAnonymous then w else w.push (cur ++ n)
    (stack, { acc with wanted })
  else if k == ``Parser.Command.in then
    -- `<cmd> in <cmd>`: the left side is the modifier (`open`, `set_option`,
    -- `variable`, ...), the right side the command it applies to. The scope
    -- stack does not survive either side.
    let (_, acc) := scanCommand text fileMap stack underIn acc (stx.getArg 0)
    let (_, acc) := scanCommand text fileMap stack true acc (stx.getArg 2)
    (stack, acc)
  else if k == ``Parser.Command.mutual then
    let inner := (stx.getArg 1).getArgs
    let acc := inner.foldl (init := acc) fun a c =>
      (scanCommand text fileMap stack underIn a c).2
    (stack, acc)
  else
    (stack, acc)

partial def scanDeclaration (text : String) (fileMap : FileMap) (stack : Array Scope)
    (underIn : Bool) (acc : Scan) (mods : Syntax) (body : Syntax) (kind? : Option String) : Scan :=
  match kind?.orElse fun _ => bodyKind body with
  | none => acc
  | some kind =>
    match childOfKind body ``Parser.Command.declId with
    | none => { acc with unnamed := acc.unnamed + 1 }
    | some declId =>
      let asWritten := (declId.getArg 0).getId
      if asWritten.isAnonymous then { acc with unnamed := acc.unnamed + 1 } else
      let sig? :=
        (childOfKind body ``Parser.Command.declSig).orElse fun _ =>
          childOfKind body ``Parser.Command.optDeclSig
      let (binders, typeText, hasType) :=
        match sig? with
        | none => ("", "", false)
        | some sig =>
          let (b, ts?) := sigParts sig
          match ts? with
          | none => (rangeText text b, "", false)
          | some ts => (rangeText text b, rangeText text (ts.getArg 1), true)
      let pos := (declId.getPos? (canonicalOnly := false)).getD ⟨0⟩
      let p := fileMap.toPosition pos
      -- `_root_.Foo` escapes the enclosing namespaces; the resulting name is
      -- `Foo`, not `<current>.«_root_».Foo`.
      let full :=
        if asWritten.getRoot == `_root_ then asWritten.replacePrefix `_root_ .anonymous
        else curNamespace stack ++ asWritten
      let rec_ : DeclRec :=
        { name := full, asWritten, kind
          line := p.line, col := p.column, binders, typeText, hasType, underIn
          isPrivate := hasPrivate mods }
      { acc with decls := acc.decls.push rec_ }

end

/-! ## Statistics -/

structure Stats where
  total : Nat
  mean : Nat
  p50 : Nat
  p90 : Nat
  max : Nat
  n : Nat

def mkStats (xs : Array Nat) : Stats :=
  if xs.isEmpty then { total := 0, mean := 0, p50 := 0, p90 := 0, max := 0, n := 0 }
  else
    let sorted := xs.qsort (· < ·)
    let total := sorted.foldl (· + ·) 0
    let n := sorted.size
    let idx (q : Nat) : Nat := min (n - 1) (q * n / 100)
    { total, mean := total / n
      p50 := sorted[idx 50]!, p90 := sorted[idx 90]!
      max := sorted[n - 1]!, n }

def fmtUs (us : Nat) : String :=
  let ms := us / 1000
  let frac := us % 1000
  let pad := if frac < 10 then "00" else if frac < 100 then "0" else ""
  s!"{ms}.{pad}{frac} ms"

/-! ## Driver -/

def readNameList (path : FilePath) : IO (Array Name) := do
  let text ← IO.FS.readFile path
  let mut out : Array Name := #[]
  for rawLine in text.splitOn "\n" do
    let line := rawLine.trimAscii.toString
    if line.isEmpty || line.startsWith "#" || line.startsWith "--" then
      continue
    out := out.push line.toName
  return out

structure ParseResult where
  parseNanos : Nat
  /-- `mkInputContext` + `parseHeader`. Split out because `parseHeader` calls
  `mkEmptyEnvironment`, which runs `mkInitialExtensionStates` over every
  registered environment extension — with Mathlib imported that is an API
  artifact of the header parser, not a cost of parsing the file. -/
  headerNanos : Nat
  /-- Time spent walking the parsed commands for `open`/`namespace`/declarations.
  Zero unless the caller asked for a scan; measured *outside* `parseNanos` so the
  gate's parse numbers stay comparable. -/
  scanNanos : Nat
  commands : Nat
  nodes : Nat
  errors : Nat
  warnings : Nat
  firstError : Option String
  firstErrorPos : Option (Nat × Nat)
  scan : Scan
  deriving Inhabited

/-- `mkInputContext` -> `parseHeader` -> `parseCommand`*, timed as one unit.

The file contents are read *before* the clock starts (all sources are slurped in
`run` up front) so that page-cache state cannot leak into `parseUs`.

`collect := true` additionally retains the command syntax trees and walks them
after the clock has stopped. The retention is why it is opt-in: the default path
lets each command tree die immediately, and that is the state the gate numbers
were taken in. -/
def parseOne (env : Environment) (openDecls : List OpenDecl) (path : String) (text : String)
    (collect : Bool := false) : IO ParseResult := do
  let t0 ← IO.monoNanosNow
  let inputCtx := Parser.mkInputContext text path
  let (headerStx, state, messages) ← Parser.parseHeader inputCtx
  pin state.pos.byteIdx
  let tHdr ← IO.monoNanosNow
  let pmctx : Parser.ParserModuleContext :=
    { env := env, options := {}, currNamespace := .anonymous, openDecls := openDecls }
  let mut nodes := countNodes headerStx.raw
  let mut commands := 0
  let mut st := state
  let mut msgs := messages
  let mut cmds : Array Syntax := #[]
  repeat
    let (stx, st', msgs') := Parser.parseCommand inputCtx pmctx st msgs
    st := st'
    msgs := msgs'
    nodes := nodes + countNodes stx
    if stx.isOfKind ``Parser.Command.eoi then
      break
    commands := commands + 1
    if collect then
      cmds := cmds.push stx
  -- Pin before the closing read: `parseCommand` and `countNodes` are pure.
  pin nodes
  pin commands
  let t1 ← IO.monoNanosNow
  let mut scan : Scan := {}
  if collect then
    let mut stack : Array Scope := #[]
    for c in cmds do
      let (stack', scan') := scanCommand text inputCtx.fileMap stack false scan c
      stack := stack'
      scan := scan'
    pin scan.decls.size
    pin scan.wanted.size
  let t2 ← IO.monoNanosNow
  let all := msgs.toArray
  let errs := all.filter fun m => match m.severity with | .error => true | _ => false
  let warns := all.size - errs.size
  let (firstError, firstErrorPos) ←
    match errs[0]? with
    | none => pure (none, none)
    | some m => do
      let s ← m.data.toString
      pure (some s, some (m.pos.line, m.pos.column))
  return { parseNanos := t1 - t0, headerNanos := tHdr - t0, scanNanos := t2 - t1,
           commands, nodes, errors := errs.size, warnings := warns,
           firstError, firstErrorPos, scan }

def dedupNames (xs : Array Name) : Array Name := Id.run do
  let mut seen : NameSet := {}
  let mut out : Array Name := #[]
  for x in xs do
    unless x.isAnonymous || seen.contains x do
      seen := seen.insert x
      out := out.push x
  return out

/-- Runs `Lean.activateScoped` for each namespace, returning the resulting
environment. The argument environment is *not* modified: `Environment` is a
value, and the one produced by `importModules (leakEnv := true)` is marked
persistent, so the extension array is copied rather than updated in place. That
claim is what `--accum` and `--reverse` exist to check empirically. -/
def activateNamespaces (env : Environment) (ns : Array Name) : IO Environment := do
  if ns.isEmpty then return env
  let act : CoreM Unit := ns.forM Lean.activateScoped
  let (_, st) ← act.toIO { fileName := "<lean-doc/stage7e>", fileMap := default } { env := env }
  return st.env

def declRecJson (m : Name) (d : DeclRec) : String :=
  s!"\{\"mod\":{jsonStr m.toString},\"name\":{jsonStr d.name.toString},\
\"asWritten\":{jsonStr d.asWritten.toString},\"kind\":{jsonStr d.kind},\
\"line\":{d.line},\"col\":{d.col},\"binders\":{jsonStr d.binders},\
\"type\":{jsonStr d.typeText},\"hasType\":{if d.hasType then "true" else "false"},\
\"underIn\":{if d.underIn then "true" else "false"},\
\"private\":{if d.isPrivate then "true" else "false"}}\n"

def run (cfg : Cfg) : IO UInt32 := do
  let targets ← readNameList cfg.modulesPath
  if targets.isEmpty then
    IO.eprintln s!"no module names in {cfg.modulesPath}"
    return 1

  let tSp0 ← IO.monoNanosNow
  initSearchPath (← findSysroot)
  let srcPath ← getSrcSearchPath
  let tSp1 ← IO.monoNanosNow

  let imports : Array Name :=
    match cfg.envMode with
    | .full => targets
    | .mathlib => #[`Mathlib]
    | .init => #[`Init]

  let tImp0 ← IO.monoNanosNow
  unsafe Lean.enableInitializersExecution
  let env ← importModules (imports.map (Import.mk · false true false)) Options.empty
    (leakEnv := true) (loadExts := true)
  let tImp1 ← IO.monoNanosNow

  -- `--open`: scoped notation lives in `ScopedEnvExtension`s that an imported
  -- environment has NOT activated. Activating them is what an `open` command
  -- would do during elaboration; `openDecls` alone is not enough.
  let tAct0 ← IO.monoNanosNow
  let env ←
    if cfg.openNamespaces.isEmpty then pure env
    else do
      let act : CoreM Unit := cfg.openNamespaces.forM Lean.activateScoped
      let (_, st) ← act.toIO { fileName := "<lean-doc/stage7e>", fileMap := default } { env := env }
      pure st.env
  let tAct1 ← IO.monoNanosNow
  let openDecls : List OpenDecl := cfg.openNamespaces.toList.map (OpenDecl.simple · [])

  IO.println s!"env               {cfg.envMode.toString} ({imports.size} direct imports, \
{env.header.moduleNames.size} loaded)"
  IO.println s!"initSearchPath    {fmtUs ((tSp1 - tSp0) / 1000)}"
  IO.println s!"importModules     {fmtUs ((tImp1 - tImp0) / 1000)}"
  unless cfg.openNamespaces.isEmpty do
    IO.println s!"activateScoped    {fmtUs ((tAct1 - tAct0) / 1000)}  \
({", ".intercalate (cfg.openNamespaces.toList.map (·.toString))})"

  -- Resolve sources once; the path lookup is not part of the parse cost.
  let mut sources : Array (Name × String × String) := #[]
  let mut missing : Array Name := #[]
  for m in targets do
    let r ← (show IO (String × String) from do
      let p ← Lean.findLean srcPath m
      let text ← IO.FS.readFile p
      return (p.toString, text)).toBaseIO
    match r with
    | .ok (p, text) => sources := sources.push (m, p, text)
    | .error _ => missing := missing.push m
  unless missing.isEmpty do
    IO.println s!"sources missing   {missing.size}: {missing.toList.take 5}"
  if cfg.reverse then
    sources := sources.reverse
    IO.println s!"order             reversed"

  let handle ← IO.FS.Handle.mk cfg.outPath .write
  let declsHandle ← match cfg.declsPath with
    | some p => some <$> IO.FS.Handle.mk p .write
    | none => pure none
  -- Collecting means retaining the command trees; only pay for it when a
  -- consumer exists.
  let collect := cfg.twoPass || declsHandle.isSome
  let baseEnv := env
  let mut roundStats : Array (Nat × Stats × Nat) := #[]

  for round in [0:cfg.rounds] do
    let mut times : Array Nat := #[]
    let mut hdrTimes : Array Nat := #[]
    let mut scanTimes : Array Nat := #[]
    let mut actTimes : Array Nat := #[]
    let mut unitTimes : Array Nat := #[]
    let mut errModules := 0
    let mut errTotal := 0
    let mut errModules1 := 0
    let mut errTotal1 := 0
    let mut declTotal := 0
    let mut unnamedTotal := 0
    let mut endMismatchTotal := 0
    let mut nsTotal := 0
    let mut grewModules := 0
    let mut allKinds : NameMap Nat := {}
    let mut envCur := baseEnv
    let tRound0 ← IO.monoNanosNow
    for (m, path, text) in sources do
      let mut r : ParseResult := default
      let mut wanted : Array Name := #[]
      let mut actNanos := 0
      let mut unitNanos := 0
      let mut first : ParseResult := default
      if cfg.twoPass then
        -- Pass 1 runs against the *unmodified* environment (or the accumulated
        -- one under `--accum`), so its error count is directly comparable with
        -- the `--env full` gate run.
        let envIn := if cfg.accum then envCur else baseEnv
        let r1 ← parseOne envIn openDecls path text (collect := true)
        wanted := dedupNames r1.scan.wanted
        let tA0 ← IO.monoNanosNow
        let env2 ← activateNamespaces envIn wanted
        let tA1 ← IO.monoNanosNow
        let decls2 := openDecls ++ wanted.toList.map (OpenDecl.simple · [])
        let r2 ← parseOne env2 decls2 path text (collect := true)
        if cfg.accum then
          envCur := env2
        -- Would a third pass see more? Only if pass 2 recovered `open`s that
        -- pass 1 could not even tokenise.
        if (dedupNames r2.scan.wanted).size > wanted.size then
          grewModules := grewModules + 1
        first := r1
        r := r2
        actNanos := tA1 - tA0
        unitNanos := r1.parseNanos + r1.scanNanos + actNanos + r2.parseNanos + r2.scanNanos
        errModules1 := errModules1 + (if r1.errors > 0 then 1 else 0)
        errTotal1 := errTotal1 + r1.errors
      else
        let r0 ← parseOne env openDecls path text (collect := collect)
        first := r0
        r := r0
        unitNanos := r0.parseNanos + r0.scanNanos
      let us := r.parseNanos / 1000
      times := times.push us
      hdrTimes := hdrTimes.push (r.headerNanos / 1000)
      scanTimes := scanTimes.push (r.scanNanos / 1000)
      actTimes := actTimes.push (actNanos / 1000)
      unitTimes := unitTimes.push (unitNanos / 1000)
      if r.errors > 0 then
        errModules := errModules + 1
      errTotal := errTotal + r.errors
      declTotal := declTotal + r.scan.decls.size
      unnamedTotal := unnamedTotal + r.scan.unnamed
      endMismatchTotal := endMismatchTotal + r.scan.endMismatch
      nsTotal := nsTotal + wanted.size
      for (k, v) in r.scan.kinds.toList do
        allKinds := allKinds.insert k ((allKinds.find? k).getD 0 + v)
      let firstErrJson :=
        match r.firstError, r.firstErrorPos with
        | some msg, some (l, c) =>
          s!"\{\"line\":{l},\"col\":{c},\"msg\":{jsonStr msg}}"
        | _, _ => "null"
      let twoPassJson :=
        if cfg.twoPass then
          s!",\"pass1Us\":{first.parseNanos / 1000},\"scan1Us\":{first.scanNanos / 1000},\
\"actUs\":{actNanos / 1000},\"pass2Us\":{r.parseNanos / 1000},\
\"scan2Us\":{r.scanNanos / 1000},\"unitUs\":{unitNanos / 1000},\
\"ns\":{wanted.size},\"errors1\":{first.errors},\"nodes1\":{first.nodes},\
\"decls1\":{first.scan.decls.size}"
        else ""
      handle.putStr <|
        s!"\{\"round\":{round},\"env\":{jsonStr cfg.envMode.toString},\"mod\":{jsonStr m.toString},\
\"bytes\":{text.utf8ByteSize},\"parseUs\":{us},\"headerUs\":{r.headerNanos / 1000},\
\"commands\":{r.commands},\"nodes\":{r.nodes},\
\"errors\":{r.errors},\"warnings\":{r.warnings},\"firstError\":{firstErrJson}\
{twoPassJson}}\n"
      if let some dh := declsHandle then
        if round == 0 then
          for d in r.scan.decls do
            dh.putStr (declRecJson m d)
      if cfg.printErrors && r.errors > 0 then
        let pos := match r.firstErrorPos with | some (l, c) => s!"{l}:{c}" | none => "?"
        IO.println s!"  ERR {m} ({r.errors}) {pos}  {(r.firstError.getD "").take 160}"
    let tRound1 ← IO.monoNanosNow
    handle.flush
    if let some dh := declsHandle then dh.flush
    let st := mkStats times
    let hst := mkStats hdrTimes
    roundStats := roundStats.push (round, st, errModules)
    IO.println s!"round {round}           wall {fmtUs ((tRound1 - tRound0) / 1000)}  \
sum {fmtUs st.total}  p50 {fmtUs st.p50}  p90 {fmtUs st.p90}  max {fmtUs st.max}  \
mean {fmtUs st.mean}  modules {st.n}  errModules {errModules}  errors {errTotal}"
    IO.println s!"  of which header  sum {fmtUs hst.total}  p50 {fmtUs hst.p50}  \
p90 {fmtUs hst.p90}  max {fmtUs hst.max}  (mkEmptyEnvironment inside parseHeader)"
    if collect then
      let sst := mkStats scanTimes
      IO.println s!"  scan (walk)      sum {fmtUs sst.total}  p50 {fmtUs sst.p50}  \
p90 {fmtUs sst.p90}  max {fmtUs sst.max}  decls {declTotal}  unnamed {unnamedTotal}  \
endMismatch {endMismatchTotal}"
      let ranked := allKinds.toList.toArray.qsort fun a b => a.2 > b.2
      IO.println "  top-level command kinds (all of them, handled or not):"
      for (k, v) in ranked.toList.take 25 do
        IO.println s!"    {v}  {k}"
    if cfg.twoPass then
      let ast := mkStats actTimes
      let ust := mkStats unitTimes
      IO.println s!"  activateScoped   sum {fmtUs ast.total}  p50 {fmtUs ast.p50}  \
p90 {fmtUs ast.p90}  max {fmtUs ast.max}  namespaces {nsTotal}"
      IO.println s!"  UNIT (1 module)  sum {fmtUs ust.total}  p50 {fmtUs ust.p50}  \
p90 {fmtUs ust.p90}  max {fmtUs ust.max}  mean {fmtUs ust.mean}"
      IO.println s!"  pass1            errModules {errModules1}  errors {errTotal1}  \
(must equal the --env full gate run when the base env is intact)"
      IO.println s!"  wanted grew      {grewModules} / {sources.size} modules \
(a third pass would only help these)"

  IO.println ""
  IO.println s!"modules parsed    {sources.size}"
  match roundStats[0]? with
  | some (_, _, e) => IO.println s!"errModules        {e} / {sources.size}"
  | none => pure ()
  return 0

end Stage7e

open Stage7e in
def parseArgs (args : List String) : Except String Cfg :=
  match args with
  | modules :: out :: rest => go { modulesPath := ⟨modules⟩, outPath := ⟨out⟩ } rest
  | _ => .error "usage: parse <modules.txt> <out.jsonl> [--env full|mathlib|init] \
[--repeat N] [--open <ns,..>] [--print-errors] [--two-pass] [--accum] [--reverse] \
[--decls <decls.jsonl>]"
where
  go (cfg : Cfg) : List String → Except String Cfg
  | [] => .ok cfg
  | "--env" :: m :: rest =>
    match m with
    | "full" => go { cfg with envMode := .full } rest
    | "mathlib" => go { cfg with envMode := .mathlib } rest
    | "init" => go { cfg with envMode := .init } rest
    | _ => .error s!"--env expects full|mathlib|init, got {m}"
  | "--repeat" :: n :: rest =>
    match n.toNat? with
    | some k => if k == 0 then .error "--repeat must be at least 1" else go { cfg with rounds := k } rest
    | none => .error s!"--repeat expects a number, got {n}"
  | "--open" :: ns :: rest =>
    go { cfg with openNamespaces := (ns.splitOn ",").toArray.map (·.trimAscii.toString.toName) } rest
  | "--print-errors" :: rest => go { cfg with printErrors := true } rest
  | "--two-pass" :: rest => go { cfg with twoPass := true } rest
  | "--accum" :: rest => go { cfg with accum := true } rest
  | "--reverse" :: rest => go { cfg with reverse := true } rest
  | "--decls" :: p :: rest => go { cfg with declsPath := some ⟨p⟩ } rest
  | a :: _ => .error s!"unknown argument: {a}"

def main (args : List String) : IO UInt32 := do
  match parseArgs args with
  | .ok cfg => Stage7e.run cfg
  | .error msg => IO.eprintln msg; return 1
