/-
Stage 2 verification experiment for lean-doc (see `docs/approach.md` §5.2, §6.1, §7).

Stage 1 showed that a whole package can be loaded into one environment and that
declarations can be looked up through the module index instead of scanning the
environment. It did no semantic analysis at all.

This program adds the semantic analysis: for every declaration reached through
the index route it produces the information a documentation page needs.

* the pretty printed signature (binders + result type), through the same
  delaborator path doc-gen4 uses (`Info.ofConstantVal` -> `Info.ofTypedName`):
  `delabCore` with `delabForallParamsWithSignature`, `sanitizeSyntax`,
  `parenthesize`, then `format` of each binder and of the type;
* the docstring (`findDocString?`);
* the declaration kind (def / theorem / instance / structure / ...);
* the declaration range (`findDeclarationRanges?`);
* optionally the equation lemmas (`getEqnsFor?`), which is what actually
  elaborates. OFF by default, `--equations` turns it on.

The blacklist is a transcription of doc-gen4's `DocInfo.isBlackListed`, because
comparing times only means something if both tools keep the same declarations.

Only `import Lean`: lean-doc does not depend on doc-gen4.

Usage: extract <modules.txt> <out.jsonl> [options]
  --equations         generate equation lemmas (default: off)
  --dump <path>       write one JSON object per declaration to <path>
  --only <path>       restrict processing to the declaration names in <path>
                      (one per line); for inspecting individual signatures
  --open <ns>[,<ns>]  pretty print with these namespaces opened
                      (probe for scoped notation; doc-gen4 opens nothing)
  --tag               additionally run `Widget.tagCodeInfos`, the step doc-gen4
                      needs to turn a signature into linkable `RenderedCode`
-/
import Lean

open Lean System Meta PrettyPrinter

namespace Stage2

/-! ## Timing sink (same JSONL shape as the doc-gen4 instrumentation) -/

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

def fmtDur (nanos : Nat) : String :=
  let ms := nanos / 1000000
  let frac := ms % 1000
  let pad := if frac < 10 then "00" else if frac < 100 then "0" else ""
  s!"{ms / 1000}.{pad}{frac}s"

/-! ## Configuration -/

structure Cfg where
  modulesPath : FilePath
  outPath : FilePath
  genEquations : Bool := false
  dumpPath : Option FilePath := none
  onlyPath : Option FilePath := none
  openNamespaces : Array Name := #[]
  tagCode : Bool := false
  deriving Inhabited

/-! ## doc-gen4's blacklist, transcribed

`DocGen4/Process/DocInfo.lean:142-165`. Kept identical on purpose: a different
exclusion granularity changes the declaration count, and then the time
comparison against doc-gen4 means nothing.
-/

def isProjFn (declName : Name) : MetaM Bool := do
  let env ← getEnv
  match declName with
  | .str parent name =>
    let some si := getStructureInfo? env parent | return false
    return getProjFnForField? env parent (Name.mkSimple name) == declName
      || (si.parentInfo.any fun pi => pi.projFn == declName)
  | _ => return false

def isBlackListed (declName : Name) : MetaM Bool := do
  if ← isProjFn declName then
    return false
  match ← findDeclarationRanges? declName with
  | some _ =>
    let env ← getEnv
    pure declName.isInternal
    <||> (pure <| isAuxRecursor env declName)
    <||> (pure <| isNoConfusion env declName)
    <||> (pure declName.isInternalDetail)
    <||> isRec declName
    <||> isMatcher declName
  | none => return true

def isInstanceDecl (declName : Name) : MetaM Bool := do
  return (instanceExtension.getState (← getEnv)).instanceNames.contains declName

/-! ## Pretty printing -/

/-- A pretty printed signature: the binders in front of the `:` and the type after it. -/
structure Sig where
  binders : Array String
  implicits : Array Bool
  type : String
  deriving Inhabited

/--
The same path as doc-gen4's `Info.ofTypedName`, minus the tagging step: delaborate
the type as a `declSig`, sanitize, parenthesize, then format the binders one by one
and the result type separately.

`currNamespace := n.getPrefix` mirrors doc-gen4. Note that `openDecls` stays at
whatever the caller put in the `Core.Context` -- doc-gen4 leaves it empty, which is
why scoped notation never appears in its output.
-/
def ppSignature (tagCode : Bool) (n : Name) (t : Expr) : MetaM Sig := do
  let (sigStx, infos) ← withTheReader Core.Context ({ · with currNamespace := n.getPrefix }) <|
    delabCore t (delab := Delaborator.delabForallParamsWithSignature fun binders type =>
      `(declSig| $binders* : $type))
  let sigStx := (sanitizeSyntax sigStx).run' { options := (← getOptions) }
  let sigStx ← parenthesize Parser.Command.declSig.parenthesizer sigStx
  let `(declSig| $binders* : $type) := sigStx
    | throwError "signature pretty printer failure for {n}"
  let mut bs : Array String := #[]
  let mut imps : Array Bool := #[]
  for binder in binders do
    let fmt ← PrettyPrinter.format Parser.Term.bracketedBinder.formatter binder.raw
    if tagCode then
      let _ ← tagIt fmt infos
    bs := bs.push fmt.pretty
    imps := imps.push (!binder.raw.isOfKind ``Parser.Term.explicitBinder)
  let fmt ← PrettyPrinter.formatTerm type.raw
  if tagCode then
    let _ ← tagIt fmt infos
  return { binders := bs, implicits := imps, type := fmt.pretty }
where
  /-- What doc-gen4 additionally does to get linkable code out of a `Format`. -/
  tagIt (fmt : Std.Format) (infos : SubExpr.PosMap Elab.Info) : MetaM Unit := do
    let tt := Widget.TaggedText.prettyTagged fmt
    let ctx : Elab.ContextInfo := {
      env := ← getEnv
      mctx := ← getMCtx
      options := ← getOptions
      currNamespace := ← getCurrNamespace
      openDecls := ← getOpenDecls
      fileMap := default
      ngen := ← getNGen
    }
    let _ ← Widget.tagCodeInfos ctx infos tt

/-- doc-gen4's `prettyPrintTerm`, without the tagging. -/
def ppTerm (e : Expr) : MetaM String := do
  return (← Meta.ppExpr e).pretty

/-! ## Equations (doc-gen4's `DefinitionInfo.computeEquations?`) -/

def valueToEq (v : DefinitionVal) : MetaM Expr := withLCtx {} {} do
  withOptions (Lean.Meta.tactic.hygienic.set · false) do
    lambdaTelescope v.value fun xs body => do
      let us := v.levelParams.map mkLevelParam
      let type ← mkEq (mkAppN (mkConst v.name us) xs) body
      mkForallFVars xs type

def ppEquation (e : Expr) : MetaM String :=
  forallTelescope e.consumeMData fun _ body => ppTerm body

def computeEquations (v : DefinitionVal) : MetaM (Array String) := do
  match ← getEqnsFor? v.name with
  | some eqs =>
    eqs.mapM fun eq => do ppEquation (← mkConstWithFreshMVarLevels eq >>= inferType)
  | none => return #[← ppEquation (← valueToEq v)]

/-! ## The extracted record -/

structure Member where
  label : String
  name : Name
  text : String

structure DeclOut where
  name : Name
  module : Name
  kind : String
  sig : Sig
  doc : Option String
  line : Nat
  col : Nat
  equations : Array String
  eqFailed : Bool := false
  members : Array Member := #[]

def DeclOut.toJson (d : DeclOut) : Json :=
  Json.mkObj [
    ("name", Json.str d.name.toString),
    ("module", Json.str d.module.toString),
    ("kind", Json.str d.kind),
    ("binders", Json.arr (d.sig.binders.map Json.str)),
    ("implicits", Json.arr (d.sig.implicits.map (Json.bool ·))),
    ("type", Json.str d.sig.type),
    ("doc", match d.doc with | some s => Json.str s | none => Json.null),
    ("line", Json.num d.line),
    ("col", Json.num d.col),
    ("equations", Json.arr (d.equations.map Json.str)),
    ("eqFailed", Json.bool d.eqFailed),
    ("members", Json.arr (d.members.map fun m =>
      Json.mkObj [("label", Json.str m.label), ("name", Json.str m.name.toString),
                  ("text", Json.str m.text)]))
  ]

/-! ## Analysis of one declaration -/

structure Counters where
  ppNanos : Nat := 0
  eqNanos : Nat := 0
  docNanos : Nat := 0
  eqCount : Nat := 0
  eqFailures : Nat := 0
  deriving Inhabited

abbrev AnalyzeM := StateRefT Counters MetaM

def timedPp (act : MetaM α) : AnalyzeM α := do
  let t0 ← IO.monoNanosNow
  let r ← act
  let t1 ← IO.monoNanosNow
  modify fun c => { c with ppNanos := c.ppNanos + (t1 - t0) }
  return r

/-- doc-gen4's `Info.ofConstantVal` + `NameInfo.ofTypedName` for one name. -/
def baseInfo (cfg : Cfg) (module : Name) (kind : String) (cv : ConstantVal) :
    AnalyzeM DeclOut := do
  let e := Expr.const cv.name (cv.levelParams.map mkLevelParam)
  let t ← inferType e
  let sig ← timedPp (ppSignature cfg.tagCode cv.name t)
  let tDoc0 ← IO.monoNanosNow
  let doc ← Lean.findDocString? (← getEnv) cv.name
  let tDoc1 ← IO.monoNanosNow
  modify fun c => { c with docNanos := c.docNanos + (tDoc1 - tDoc0) }
  let some ranges ← findDeclarationRanges? cv.name
    | throwError "{cv.name} is a declaration without position"
  return {
    name := cv.name, module, kind, sig, doc,
    line := ranges.range.pos.line, col := ranges.range.pos.column,
    equations := #[]
  }

def withEquations (cfg : Cfg) (v : DefinitionVal) (d : DeclOut) : AnalyzeM DeclOut := do
  unless cfg.genEquations do return d
  let t0 ← IO.monoNanosNow
  let r ← tryCatchRuntimeEx
    (do let eqs ← computeEquations v; return Except.ok eqs)
    (fun e => do return Except.error (← e.toMessageData.toString))
  let t1 ← IO.monoNanosNow
  modify fun c => { c with eqNanos := c.eqNanos + (t1 - t0) }
  match r with
  | .ok eqs =>
    modify fun c => { c with eqCount := c.eqCount + eqs.size }
    return { d with equations := eqs }
  | .error _ =>
    modify fun c => { c with eqFailures := c.eqFailures + 1 }
    return { d with eqFailed := true }

/-- Fields and parents of a structure, as doc-gen4's `getFieldTypes` computes them. -/
def structureMembers (cfg : Cfg) (v : InductiveVal) : AnalyzeM (Array Member) := do
  let env ← getEnv
  let structName := v.name
  let us := v.levelParams.map mkLevelParam
  let ctorVal := getStructureCtor env structName
  let ctorSig ← timedPp (ppSignature cfg.tagCode ctorVal.name ctorVal.type)
  let out : Array Member := #[⟨"ctor", ctorVal.name, ctorSig.type⟩]
  let inner ← timedPp <|
    forallTelescopeReducing v.type fun params _ =>
      withLocalDeclD `self (mkAppN (mkConst structName us) params) fun s => do
        let mut acc : Array Member := #[]
        for parent in getStructureParentInfo env structName do
          let proj := mkApp (mkAppN (mkConst parent.projFn us) params) s
          acc := acc.push ⟨"parent", parent.projFn, ← ppTerm (← inferType proj)⟩
        for fieldName in getStructureFieldsFlattened env structName (includeSubobjectFields := false) do
          let proj ← mkProjection s fieldName
          let ty ← inferType proj
          let projFn := (getProjFnForField? env structName fieldName).getD (structName ++ fieldName)
          let sig ← ppSignature cfg.tagCode projFn ty
          acc := acc.push ⟨"field", projFn, sig.type⟩
        return acc
  return out ++ inner

def inductiveMembers (cfg : Cfg) (v : InductiveVal) : AnalyzeM (Array Member) := do
  let mut out : Array Member := #[]
  for ctor in v.ctors do
    let cv := (← getConstInfoCtor ctor).toConstantVal
    let e := Expr.const cv.name (cv.levelParams.map mkLevelParam)
    let sig ← timedPp (ppSignature cfg.tagCode cv.name (← inferType e))
    out := out.push ⟨"ctor", cv.name, sig.type⟩
  return out

/-- doc-gen4's `DocInfo.ofConstant`, minus attributes / instance type indices. -/
def analyze (cfg : Cfg) (module : Name) (name : Name) (ci : ConstantInfo) :
    AnalyzeM (Option DeclOut) := do
  if ← isBlackListed name then
    return none
  match ci with
  | .axiomInfo i => return some (← baseInfo cfg module "axiom" i.toConstantVal)
  | .thmInfo i =>
    let kind ←
      if ← isProjFn i.name then pure "theorem"
      else if ← isInstanceDecl i.name then pure "instance"
      else pure "theorem"
    return some (← baseInfo cfg module kind i.toConstantVal)
  | .opaqueInfo i => return some (← baseInfo cfg module "opaque" i.toConstantVal)
  | .defnInfo i =>
    let kind ←
      if ← isProjFn i.name then pure "definition"
      else if ← isInstanceDecl i.name then pure "instance"
      else pure "definition"
    let d ← baseInfo cfg module kind i.toConstantVal
    return some (← withEquations cfg i d)
  | .inductInfo i =>
    let env ← getEnv
    let isStruct := isStructure env i.name
    let isCls := isClass env i.name
    let kind :=
      if isStruct then (if isCls then "class" else "structure")
      else (if isCls then "class_inductive" else "inductive")
    let d ← baseInfo cfg module kind i.toConstantVal
    let members ← if isStruct then structureMembers cfg i else inductiveMembers cfg i
    return some { d with members }
  | .ctorInfo i => return some (← baseInfo cfg module "constructor" i.toConstantVal)
  | .quotInfo i => return some (← baseInfo cfg module "opaque" i.toConstantVal)
  | .recInfo _ => return none

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

structure Failure where
  name : Name
  message : String

def run (cfg : Cfg) : IO UInt32 := do
  let sink ← Sink.create cfg.outPath
  let tTotal0 ← IO.monoNanosNow

  let targets ← readNameList cfg.modulesPath
  if targets.isEmpty then
    IO.eprintln s!"no module names in {cfg.modulesPath}"
    return 1
  let onlyNames ← match cfg.onlyPath with
    | some p => do
      let ns ← readNameList p
      pure (some (Std.HashSet.emptyWithCapacity ns.size |>.insertMany ns))
    | none => pure none

  let tSp0 ← IO.monoNanosNow
  initSearchPath (← findSysroot)
  let tSp1 ← IO.monoNanosNow
  sink.emit "stage2.initSearchPath" (tSp1 - tSp0)

  let tImp0 ← IO.monoNanosNow
  unsafe Lean.enableInitializersExecution
  let env ← importModules (targets.map (Import.mk · false true false)) Options.empty
    (leakEnv := true) (loadExts := true)
  let tImp1 ← IO.monoNanosNow
  sink.emit "stage2.importModules" (tImp1 - tImp0) [("directImports", toString targets.size)]

  let header := env.header
  sink.emit "stage2.envStats" 0 [("loadedModules", toString header.moduleNames.size)]

  -- `--open` probe. Scoped notation lives in `ScopedEnvExtension`s, which an
  -- imported environment has *not* activated; putting the namespace in
  -- `Core.Context.openDecls` alone is not enough, the extension state has to be
  -- activated on the environment itself. doc-gen4 does neither, which is why its
  -- output never uses scoped notation.
  let env ←
    if cfg.openNamespaces.isEmpty then
      pure env
    else do
      let act : CoreM Unit := cfg.openNamespaces.forM Lean.activateScoped
      let (_, st) ← act.toIO { fileName := "<lean-doc/stage2>", fileMap := default }
        { env := env }
      pure st.env

  -- Index route: module -> declarations. First module in the input order owns a
  -- name that appears in several modules' oleans (stage 1, "index route does not
  -- decide the owning module": 25 such names on this target).
  let tIdx0 ← IO.monoNanosNow
  let mut seen : Std.HashSet Name := Std.HashSet.emptyWithCapacity (16 * targets.size)
  let mut candidates : Array (Name × Name) := #[]
  let mut enumerated := 0
  for m in targets do
    let some modIdx := env.getModuleIdx? m
      | throw <| IO.userError s!"module not present in the environment: {m}"
    for n in header.moduleData[modIdx]!.constNames do
      enumerated := enumerated + 1
      if seen.contains n then
        continue
      seen := seen.insert n
      candidates := candidates.push (n, m)
  let tIdx1 ← IO.monoNanosNow
  sink.emit "stage2.indexLookup" (tIdx1 - tIdx0)
    [("targetModules", toString targets.size),
     ("enumerated", toString enumerated),
     ("candidates", toString candidates.size)]

  -- Semantic analysis. One fresh `Core.State` per declaration, exactly like
  -- doc-gen4's `process` loop, so that neither side accumulates elaborator state.
  -- Options and heartbeat budget copied from doc-gen4 (`DocGen4/Load.lean` for the
  -- options, `Process/Analyze.lean` for the per-constant `maxHeartbeats`).
  -- `pp.funBinderTypes` in particular changes the printed text (`fun (n : ℕ) =>`
  -- instead of `fun n =>`), so without it the two tools are not printing the same
  -- thing and the times are not comparable either.
  let coreCtx : Core.Context := {
    fileName := "<lean-doc/stage2>"
    fileMap := default
    options := Options.empty
      |>.setBool `pp.tagAppFns true
      |>.setBool `pp.funBinderTypes true
      |>.setBool `debug.skipKernelTC true
      |>.setBool `Elab.async false
    maxHeartbeats := 5000000
    openDecls := cfg.openNamespaces.toList.map (OpenDecl.simple · [])
  }
  let mut results : Array DeclOut := #[]
  let mut counters : Counters := {}
  let mut blacklisted := 0
  let mut missing := 0
  let mut considered := 0
  let mut failures : Array Failure := #[]
  let tAn0 ← IO.monoNanosNow
  for (name, module) in candidates do
    if let some only := onlyNames then
      if !only.contains name then
        continue
    let some ci := env.find? name | missing := missing + 1; continue
    considered := considered + 1
    let job : MetaM (Except String (Option DeclOut) × Counters) :=
      tryCatchRuntimeEx
        (do let (r, c) ← (analyze cfg module name ci).run {}; return (Except.ok r, c))
        (fun e => do return (Except.error (← e.toMessageData.toString), {}))
    let ((outcome, c), _, _) ← job.toIO coreCtx { env := env } {} {}
    counters := {
      ppNanos := counters.ppNanos + c.ppNanos,
      eqNanos := counters.eqNanos + c.eqNanos,
      docNanos := counters.docNanos + c.docNanos,
      eqCount := counters.eqCount + c.eqCount,
      eqFailures := counters.eqFailures + c.eqFailures }
    match outcome with
    | .ok none => blacklisted := blacklisted + 1
    | .ok (some d) => results := results.push d
    | .error msg => failures := failures.push ⟨name, msg⟩
  let tAn1 ← IO.monoNanosNow
  sink.emit "stage2.analyze" (tAn1 - tAn0)
    [("considered", toString considered),
     ("produced", toString results.size),
     ("blacklisted", toString blacklisted),
     ("failed", toString failures.size),
     ("ppUs", toString (counters.ppNanos / 1000)),
     ("eqUs", toString (counters.eqNanos / 1000)),
     ("docUs", toString (counters.docNanos / 1000)),
     ("equations", toString counters.eqCount),
     ("eqFailures", toString counters.eqFailures),
     ("genEquations", if cfg.genEquations then "true" else "false"),
     ("tagCode", if cfg.tagCode then "true" else "false")]

  if let some dumpPath := cfg.dumpPath then
    let tD0 ← IO.monoNanosNow
    let h ← IO.FS.Handle.mk dumpPath .write
    for d in results do
      h.putStr d.toJson.compress
      h.putStr "\n"
    let tD1 ← IO.monoNanosNow
    sink.emit "stage2.dump" (tD1 - tD0) [("records", toString results.size)]

  let tTotal1 ← IO.monoNanosNow
  sink.emit "stage2.total" (tTotal1 - tTotal0) [("modules", toString targets.size)]

  let mut byKind : Std.HashMap String Nat := {}
  for d in results do
    byKind := byKind.insert d.kind ((byKind.getD d.kind 0) + 1)

  IO.println s!"target modules       {targets.size}"
  IO.println s!"loaded modules       {header.moduleNames.size}"
  IO.println s!"importModules        {fmtDur (tImp1 - tImp0)}"
  IO.println s!"indexLookup          {fmtDur (tIdx1 - tIdx0)}  enumerated {enumerated}, unique {candidates.size}"
  IO.println s!"analyze              {fmtDur (tAn1 - tAn0)}  considered {considered}, produced {results.size}, blacklisted {blacklisted}, failed {failures.size}"
  IO.println s!"  of which signature {fmtDur counters.ppNanos}"
  IO.println s!"  of which equations {fmtDur counters.eqNanos}  ({counters.eqCount} lemmas, {counters.eqFailures} failed)"
  IO.println s!"  of which docstring {fmtDur counters.docNanos}"
  IO.println s!"total                {fmtDur (tTotal1 - tTotal0)}"
  IO.println s!"genEquations         {cfg.genEquations}   tagCode {cfg.tagCode}   open {cfg.openNamespaces.toList}"
  IO.print "kinds               "
  for (k, n) in byKind.toArray.qsort (fun a b => a.2 > b.2) do
    IO.print s!" {k}={n}"
  IO.println ""
  if missing > 0 then
    IO.println s!"WARNING: {missing} enumerated names absent from env.constants"
  if !failures.isEmpty then
    IO.println s!"failures ({failures.size}):"
    for f in failures.toList.take 20 do
      IO.println s!"  {f.name}: {f.message.take 300}"
  return 0

end Stage2

open Stage2 in
def parseArgs (args : List String) : Except String Cfg :=
  match args with
  | modules :: out :: rest => go { modulesPath := ⟨modules⟩, outPath := ⟨out⟩ } rest
  | _ => .error "usage: extract <modules.txt> <out.jsonl> [--equations] [--dump <p>] [--only <p>] [--open <ns,..>] [--tag]"
where
  go (cfg : Cfg) : List String → Except String Cfg
  | [] => .ok cfg
  | "--equations" :: rest => go { cfg with genEquations := true } rest
  | "--tag" :: rest => go { cfg with tagCode := true } rest
  | "--dump" :: p :: rest => go { cfg with dumpPath := some ⟨p⟩ } rest
  | "--only" :: p :: rest => go { cfg with onlyPath := some ⟨p⟩ } rest
  | "--open" :: ns :: rest =>
    go { cfg with openNamespaces := (ns.splitOn ",").toArray.map (·.trimAscii.toString.toName) } rest
  | a :: _ => .error s!"unknown argument: {a}"

def main (args : List String) : IO UInt32 := do
  match parseArgs args with
  | .ok cfg => Stage2.run cfg
  | .error msg => IO.eprintln msg; return 1
