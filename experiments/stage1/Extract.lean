/-
Stage 1 verification experiment for lean-doc (see `docs/approach.md` §5.1 and §5.2).

A self-contained Lean program that depends on `import Lean` only. It loads every
module of the target package into ONE environment with a single `importModules`
call, then enumerates the declarations of those modules in two different ways:

* index lookup -- module -> declarations, reading
  `env.header.moduleData[i].constNames` for each target module (§5.2, the
  proposed direction).
* scan lookup  -- environment -> all constants, keeping only those whose
  defining module is a target (what doc-gen4 does today).

Both routes must produce the same declaration set; the program exits non-zero if
they do not.

IMPORTANT: this program does NO pretty printing and no semantic analysis, so its
total time is a *floor* ("environment load + declaration enumeration") and is not
comparable with doc-gen4's `batch.total`. Only the import phase is comparable.

Usage: extract <modules.txt> <out.jsonl>
  modules.txt  one module name per line ('#' and '--' comments and blanks skipped)
  out.jsonl    timing sink, one JSON record per line, appended
-/
import Lean

open Lean System

namespace Stage1

/-- Timing sink. Same JSONL shape as the doc-gen4 instrumentation
(`benchmarks/doc-gen4-instrumentation.patch`): one record per line,
`{"phase":<name>,"pid":<pid>,"us":<microseconds>,<extra>}`. -/
structure Sink where
  handle : IO.FS.Handle
  pid : UInt32

def Sink.create (path : FilePath) : IO Sink := do
  let handle ← IO.FS.Handle.mk path .append
  let pid ← IO.Process.getPID
  return { handle, pid }

/-- Appends one record. `extra` values are spliced in as raw JSON. -/
def Sink.emit (s : Sink) (phase : String) (nanos : Nat)
    (extra : List (String × String) := []) : IO Unit := do
  let extraStr := extra.foldl (init := "") fun acc (k, v) => acc ++ s!",\"{k}\":{v}"
  s.handle.putStr s!"\{\"phase\":\"{phase}\",\"pid\":{s.pid},\"us\":{nanos / 1000}{extraStr}}\n"
  s.handle.flush

/-- Nanoseconds as `"12.910s"`, for the human-readable summary only. -/
def fmtDur (nanos : Nat) : String :=
  let ms := nanos / 1000000
  let frac := ms % 1000
  let pad := if frac < 10 then "00" else if frac < 100 then "0" else ""
  s!"{ms / 1000}.{pad}{frac}s"

/--
Declarations that no documentation tool wants: compiler-generated and internal
names. Deliberately pure and cheap -- doc-gen4's real blacklist additionally
needs `MetaM` (`findDeclarationRanges?`, `isMatcher`, `isRec`, ...), which this
experiment does not run. Both enumeration routes use exactly this filter, so the
comparison between them is unaffected.
-/
@[inline] def isSkipped (n : Name) : Bool := n.isInternal || n.isInternalDetail

/-- One module name per line; blank lines and `#`/`--` comments are skipped. -/
def readModuleList (path : FilePath) : IO (Array Name) := do
  let text ← IO.FS.readFile path
  let mut out : Array Name := #[]
  for rawLine in text.splitOn "\n" do
    let line := rawLine.trimAscii.toString
    if line.isEmpty || line.startsWith "#" || line.startsWith "--" then
      continue
    out := out.push line.toName
  return out

def run (modulesPath outPath : FilePath) : IO UInt32 := do
  let sink ← Sink.create outPath
  let tTotal0 ← IO.monoNanosNow

  let targets ← readModuleList modulesPath
  if targets.isEmpty then
    IO.eprintln s!"no module names in {modulesPath}"
    return 1

  -- (a) search path
  let tSp0 ← IO.monoNanosNow
  initSearchPath (← findSysroot)
  let tSp1 ← IO.monoNanosNow
  sink.emit "stage1.initSearchPath" (tSp1 - tSp0)

  -- (b) one environment for the whole package
  let tImp0 ← IO.monoNanosNow
  -- Needed for modules that register syntax via `initialize add_parser_alias ..`.
  unsafe Lean.enableInitializersExecution
  let env ← importModules (targets.map (Import.mk · false true false)) Options.empty
    (leakEnv := true) (loadExts := true)
  let tImp1 ← IO.monoNanosNow
  sink.emit "stage1.importModules" (tImp1 - tImp0) [("directImports", toString targets.size)]

  let header := env.header
  let allModules := header.moduleNames
  let moduleData := header.moduleData
  sink.emit "stage1.envStats" 0 [("loadedModules", toString allModules.size)]

  let mut targetSet : Std.HashSet Name := Std.HashSet.emptyWithCapacity targets.size
  for m in targets do
    targetSet := targetSet.insert m

  -- (c) index lookup: module -> declarations
  let mut indexSet : Std.HashSet Name := Std.HashSet.emptyWithCapacity (2 * targets.size)
  let mut indexEnumerated := 0
  let tIdx0 ← IO.monoNanosNow
  for m in targets do
    let some modIdx := env.getModuleIdx? m
      | throw <| IO.userError s!"module not present in the environment: {m}"
    let data := moduleData[modIdx]!
    for n in data.constNames do
      indexEnumerated := indexEnumerated + 1
      if isSkipped n then
        continue
      indexSet := indexSet.insert n
  let tIdx1 ← IO.monoNanosNow
  sink.emit "stage1.indexLookup" (tIdx1 - tIdx0)
    [("targetModules", toString targets.size),
     ("enumerated", toString indexEnumerated),
     ("kept", toString indexSet.size)]

  -- (d) scan lookup: the whole environment, doc-gen4 style
  let constants := env.constants
  let mut scanSet : Std.HashSet Name := Std.HashSet.emptyWithCapacity (2 * targets.size)
  let mut scanned := 0
  let mut relevant := 0
  let tScan0 ← IO.monoNanosNow
  for (name, _) in constants do
    scanned := scanned + 1
    let some modIdx := env.getModuleIdxFor? name | continue
    if !targetSet.contains allModules[modIdx]! then
      continue
    relevant := relevant + 1
    if isSkipped name then
      continue
    scanSet := scanSet.insert name
  let tScan1 ← IO.monoNanosNow
  sink.emit "stage1.scanLookup" (tScan1 - tScan0)
    [("scanned", toString scanned),
     ("relevant", toString relevant),
     ("kept", toString scanSet.size)]

  -- correctness: the two routes must agree on the declaration set
  let mut onlyIndex : Array Name := #[]
  for n in indexSet do
    if !scanSet.contains n then
      onlyIndex := onlyIndex.push n
  let mut onlyScan : Array Name := #[]
  for n in scanSet do
    if !indexSet.contains n then
      onlyScan := onlyScan.push n
  let agree := onlyIndex.isEmpty && onlyScan.isEmpty
  sink.emit "stage1.compare" 0
    [("onlyIndex", toString onlyIndex.size),
     ("onlyScan", toString onlyScan.size),
     ("agree", if agree then "true" else "false")]

  let tTotal1 ← IO.monoNanosNow
  sink.emit "stage1.total" (tTotal1 - tTotal0) [("modules", toString targets.size)]

  IO.println s!"target modules       {targets.size}"
  IO.println s!"loaded modules       {allModules.size}"
  IO.println s!"initSearchPath       {fmtDur (tSp1 - tSp0)}"
  IO.println s!"importModules        {fmtDur (tImp1 - tImp0)}"
  IO.println s!"indexLookup          {fmtDur (tIdx1 - tIdx0)}  enumerated {indexEnumerated}, kept {indexSet.size}"
  IO.println s!"scanLookup           {fmtDur (tScan1 - tScan0)}  scanned {scanned}, in target modules {relevant}, kept {scanSet.size}"
  IO.println s!"total                {fmtDur (tTotal1 - tTotal0)}"

  if agree then
    IO.println s!"declaration sets agree ({indexSet.size} declarations)"
    return 0
  else
    IO.eprintln s!"MISMATCH: index-only {onlyIndex.size}, scan-only {onlyScan.size}"
    for n in onlyIndex.toList.take 10 do
      IO.eprintln s!"  index only: {n}"
    for n in onlyScan.toList.take 10 do
      IO.eprintln s!"  scan only:  {n}"
    return 1

end Stage1

def main (args : List String) : IO UInt32 := do
  match args with
  | [modulesPath, outPath] => Stage1.run ⟨modulesPath⟩ ⟨outPath⟩
  | _ =>
    IO.eprintln "usage: extract <modules.txt> <out.jsonl>"
    return 1
