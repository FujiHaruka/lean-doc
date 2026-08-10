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

usage:
  parse <modules.txt> <out.jsonl> [--env full|mathlib|init] [--repeat N]
                                  [--open <ns,..>] [--print-errors]

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
  commands : Nat
  nodes : Nat
  errors : Nat
  warnings : Nat
  firstError : Option String
  firstErrorPos : Option (Nat × Nat)

/-- `mkInputContext` -> `parseHeader` -> `parseCommand`*, timed as one unit.

The file contents are read *before* the clock starts (all sources are slurped in
`run` up front) so that page-cache state cannot leak into `parseUs`. -/
def parseOne (env : Environment) (openDecls : List OpenDecl) (path : String) (text : String) :
    IO ParseResult := do
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
  repeat
    let (stx, st', msgs') := Parser.parseCommand inputCtx pmctx st msgs
    st := st'
    msgs := msgs'
    nodes := nodes + countNodes stx
    if stx.isOfKind ``Parser.Command.eoi then
      break
    commands := commands + 1
  -- Pin before the closing read: `parseCommand` and `countNodes` are pure.
  pin nodes
  pin commands
  let t1 ← IO.monoNanosNow
  let all := msgs.toArray
  let errs := all.filter fun m => match m.severity with | .error => true | _ => false
  let warns := all.size - errs.size
  let (firstError, firstErrorPos) ←
    match errs[0]? with
    | none => pure (none, none)
    | some m => do
      let s ← m.data.toString
      pure (some s, some (m.pos.line, m.pos.column))
  return { parseNanos := t1 - t0, headerNanos := tHdr - t0, commands, nodes,
           errors := errs.size, warnings := warns, firstError, firstErrorPos }

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

  let handle ← IO.FS.Handle.mk cfg.outPath .write
  let mut roundStats : Array (Nat × Stats × Nat) := #[]

  for round in [0:cfg.rounds] do
    let mut times : Array Nat := #[]
    let mut hdrTimes : Array Nat := #[]
    let mut errModules := 0
    let tRound0 ← IO.monoNanosNow
    for (m, path, text) in sources do
      let r ← parseOne env openDecls path text
      let us := r.parseNanos / 1000
      times := times.push us
      hdrTimes := hdrTimes.push (r.headerNanos / 1000)
      if r.errors > 0 then
        errModules := errModules + 1
      let firstErrJson :=
        match r.firstError, r.firstErrorPos with
        | some msg, some (l, c) =>
          s!"\{\"line\":{l},\"col\":{c},\"msg\":{jsonStr msg}}"
        | _, _ => "null"
      handle.putStr <|
        s!"\{\"round\":{round},\"env\":{jsonStr cfg.envMode.toString},\"mod\":{jsonStr m.toString},\
\"bytes\":{text.utf8ByteSize},\"parseUs\":{us},\"headerUs\":{r.headerNanos / 1000},\
\"commands\":{r.commands},\"nodes\":{r.nodes},\
\"errors\":{r.errors},\"warnings\":{r.warnings},\"firstError\":{firstErrJson}}\n"
      if cfg.printErrors && r.errors > 0 then
        let pos := match r.firstErrorPos with | some (l, c) => s!"{l}:{c}" | none => "?"
        IO.println s!"  ERR {m} ({r.errors}) {pos}  {(r.firstError.getD "").take 160}"
    let tRound1 ← IO.monoNanosNow
    handle.flush
    let st := mkStats times
    let hst := mkStats hdrTimes
    roundStats := roundStats.push (round, st, errModules)
    IO.println s!"round {round}           wall {fmtUs ((tRound1 - tRound0) / 1000)}  \
sum {fmtUs st.total}  p50 {fmtUs st.p50}  p90 {fmtUs st.p90}  max {fmtUs st.max}  \
mean {fmtUs st.mean}  modules {st.n}  errModules {errModules}"
    IO.println s!"  of which header  sum {fmtUs hst.total}  p50 {fmtUs hst.p50}  \
p90 {fmtUs hst.p90}  max {fmtUs hst.max}  (mkEmptyEnvironment inside parseHeader)"

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
[--repeat N] [--open <ns,..>] [--print-errors]"
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
  | a :: _ => .error s!"unknown argument: {a}"

def main (args : List String) : IO UInt32 := do
  match parseArgs args with
  | .ok cfg => Stage7e.run cfg
  | .error msg => IO.eprintln msg; return 1
