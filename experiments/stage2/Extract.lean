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

It also collects what doc-gen4's `getAllModuleDocs` collects (module docstrings,
direct imports, tactic documentation) — but enumerating the tactic table *once*
for the whole environment and bucketing by defining module, instead of once per
module. That is `approach.md` §5.2's "1 モジュールを処理するために全体を舐める"
pattern; on the fixed target doc-gen4 pays 16.7 s for it.

Usage: extract <modules.txt> <out.jsonl> [options]
  --equations         generate equation lemmas (default: off)
  --dump <path>       write one JSON object per declaration to <path>
  --dump-modules <p>  write one JSON object per module (docs / imports / tactics)
  --only <path>       restrict processing to the declaration names in <path>
                      (one per line); for inspecting individual signatures
  --open <ns>[,<ns>]  pretty print with these namespaces opened
                      (probe for scoped notation; doc-gen4 opens nothing)
  --tag               additionally run `Widget.tagCodeInfos`, the step doc-gen4
                      needs to turn a signature into linkable `RenderedCode`
  --skip-analyze      skip the semantic analysis (module docs / tactics only)
  --tactics-emulate   additionally run the tactic collection doc-gen4's way
                      (`allTacticDocs` once per module) for comparison
  --tactics-probe     additionally break `allTacticDocs` down into its parts
-/
import Lean

open Lean System Meta PrettyPrinter
open Lean.Elab.Tactic.Doc (TacticDoc allTacticDocs firstTacticTokens)
open Lean.Parser.Tactic.Doc (tacticTagExt alternativeOfTactic getTacticExtensions)

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
  dumpModulesPath : Option FilePath := none
  onlyPath : Option FilePath := none
  openNamespaces : Array Name := #[]
  tagCode : Bool := false
  skipAnalyze : Bool := false
  tacticsEmulate : Bool := false
  tacticsProbe : Bool := false
  tacticsDumpPath : Option FilePath := none
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

/-! ## Module docs and tactics — doc-gen4's `getAllModuleDocs`, restructured

doc-gen4 (`DocGen4/Process/Analyze.lean`) loops over the relevant modules and calls
`collectTactics module env` for each one; `collectTactics` calls
`Elab.Tactic.Doc.allTacticDocs`, which rebuilds the *whole* environment's tactic
table from the parser tables, and then throws away everything not defined in
`module`. That is 432 rebuilds of the same table.

Here the table is built once and bucketed by defining module. Everything else
(`getModuleDoc?`, the direct imports out of `moduleData`) is per module either way
and is a hash lookup.
-/

structure ModDocOut where
  line : Nat
  col : Nat
  text : String

structure TacticOut where
  internalName : Name
  userName : String
  tags : Array Name
  docString : String
  definingModule : Name

structure ModuleOut where
  name : Name
  imports : Array Name
  docs : Array ModDocOut
  tactics : Array TacticOut

def ModuleOut.toJson (m : ModuleOut) : Json :=
  Json.mkObj [
    ("module", Json.str m.name.toString),
    ("imports", Json.arr (m.imports.map (Json.str ·.toString))),
    ("docs", Json.arr (m.docs.map fun d =>
      Json.mkObj [("line", Json.num d.line), ("col", Json.num d.col), ("text", Json.str d.text)])),
    ("tactics", Json.arr (m.tactics.map fun t =>
      Json.mkObj [("internalName", Json.str t.internalName.toString),
                  ("userName", Json.str t.userName),
                  ("tags", Json.arr (t.tags.map (Json.str ·.toString))),
                  ("docString", Json.str t.docString),
                  ("definingModule", Json.str t.definingModule.toString)]))
  ]

/-- doc-gen4's `collectTactics` body for one `TacticDoc`, kept byte-identical. -/
def mkTacticOut (doc : TacticDoc) (definingModule : Name) : TacticOut :=
  { internalName := doc.internalName
    userName := doc.userName
    tags := doc.tags.toArray
    docString := doc.docString.getD "This tactic has no documentation." ++
      ("\n\n".intercalate doc.extensionDocs.toList)
    definingModule }

/-- Module docstrings and direct imports. One hash lookup per target module. -/
def collectModuleDocs (targets : Array Name) : MetaM (Array ModuleOut) := do
  let env ← getEnv
  let header := env.header
  let mut out : Array ModuleOut := Array.emptyWithCapacity targets.size
  for m in targets do
    let some modIdx := env.getModuleIdx? m
      | throwError "module not present in the environment: {m}"
    let docs := (getModuleDoc? env m |>.getD #[]).map fun d =>
      { line := d.declarationRange.pos.line, col := d.declarationRange.pos.column,
        text := d.doc : ModDocOut }
    out := out.push {
      name := m
      imports := header.moduleData[modIdx]!.imports.map Import.module
      docs
      tactics := #[]
    }
  return out

/-- One enumeration of the tactic table for the whole environment, bucketed by
defining module. Returns the updated modules, the total number of tactics in the
environment, and how many of them landed in a target module. -/
def collectTacticsOnce (mods : Array ModuleOut) : MetaM (Array ModuleOut × Nat × Nat) := do
  let env ← getEnv
  let header := env.header
  let mut idxOf : Std.HashMap Name Nat := Std.HashMap.emptyWithCapacity mods.size
  for h : i in [0 : mods.size] do
    idxOf := idxOf.insert mods[i].name i
  -- `EnvironmentHeader.moduleNames` is a *function*, not a field: it maps over
  -- `header.modules` and allocates a fresh 6,021-element array on every call.
  -- doc-gen4's `collectTactics` calls it inside its per-tactic loop; hoisting it is
  -- what actually removes the 12.98 s (see the `--tactics-probe` numbers).
  let modNames := header.moduleNames
  let allDocs ← allTacticDocs
  let mut mods := mods
  let mut assigned := 0
  for doc in allDocs do
    let some modIdx := env.getModuleIdxFor? doc.internalName | continue
    let definingModule := modNames[modIdx]!
    let some i := idxOf[definingModule]? | continue
    mods := mods.modify i fun m => { m with tactics := m.tactics.push (mkTacticOut doc definingModule) }
    assigned := assigned + 1
  return (mods, allDocs.size, assigned)

/-- Every tactic in the environment with its defining module, regardless of the
target list. Used to build a module list on which the bucketing can be checked
against doc-gen4 (the fixed target defines no tactics of its own). -/
def dumpAllTactics (path : FilePath) : MetaM Nat := do
  let env ← getEnv
  let modNames := env.header.moduleNames
  let docs ← allTacticDocs
  let h ← IO.FS.Handle.mk path .write
  let mut n := 0
  for doc in docs do
    let mod := match env.getModuleIdxFor? doc.internalName with
      | some i => modNames[i]!.toString
      | none => "<none>"
    h.putStr s!"{mod}\t{doc.internalName}\n"
    n := n + 1
  return n

/-- doc-gen4's shape, for comparison only: `allTacticDocs` once per module.
Returns the number of assignments, the time inside `allTacticDocs`, and the time in
doc-gen4's filter loop over its result. Each iteration gets a fresh result array, so
neither number can be flattered by a memoised thunk from an earlier iteration. -/
def collectTacticsPerModule (targets : Array Name) : MetaM (Nat × Nat × Nat) := do
  let env ← getEnv
  let header := env.header
  let mut assigned := 0
  let mut allNanos := 0
  let mut filterNanos := 0
  for module in targets do
    let t0 ← IO.monoNanosNow
    let docs ← allTacticDocs
    let t1 ← IO.monoNanosNow
    allNanos := allNanos + (t1 - t0)
    for doc in docs do
      let some modIdx := env.getModuleIdxFor? doc.internalName | continue
      if module != header.moduleNames[modIdx]! then continue
      assigned := assigned + 1
    let t2 ← IO.monoNanosNow
    filterNanos := filterNanos + (t2 - t1)
  return (assigned, allNanos, filterNanos)

/-! ### Probe: where does one `allTacticDocs` call spend its time?

A transcription of `Lean.Elab.Tactic.Doc.allTacticDocs` (and of the
`firstTacticTokens` it calls) with timers between the parts. The sum is printed
next to a plain `allTacticDocs` call so the transcription can be checked against
the real thing.
-/

structure TacticProbe where
  tagFold : Nat := 0
  nameExtFold : Nat := 0
  leadingTable : Nat := 0
  trailingTable : Nat := 0
  kindLoop : Nat := 0
  docString : Nat := 0
  extensions : Nat := 0
  kinds : Nat := 0
  produced : Nat := 0
  extStrings : Nat := 0
  tagsSeen : Nat := 0
  leadingToks : Nat := 0
  trailingToks : Nat := 0
  collectKindsCalls : Nat := 0
  firstTokens : Nat := 0
  /-- 5 × the real `Elab.Tactic.Doc.allTacticDocs`, results forced. -/
  direct5 : Nat := 0
  /-- 5 × doc-gen4's filter loop over the result (`getModuleIdxFor?` per tactic). -/
  bucket5 : Nat := 0
  /-- 5 × forcing `extensionDocs` of every returned `TacticDoc`. -/
  forceExt5 : Nat := 0
  /-- 5 × `env.getModuleIdxFor?` alone. -/
  idxOnly5 : Nat := 0
  /-- 5 × the same lookup with `env.const2ModIdx` hoisted out of the loop. -/
  hoisted5 : Nat := 0
  /-- The filter loop again, on the same (now already touched) array. -/
  bucket5b : Nat := 0
  /-- 5 × touching `internalName` only. -/
  touch5 : Nat := 0
  /-- 5 × `internalName` + `getModuleIdxFor?`. -/
  lookup5 : Nat := 0
  /-- 5 × the above + `header.moduleNames[idx]!` and the name comparison. -/
  full5 : Nat := 0
  /-- 5 × the same loop with `header.moduleNames` hoisted out of it. -/
  fullHoisted5 : Nat := 0
  deriving Inhabited

def probeAllTacticDocs : MetaM TacticProbe := do
  let env ← getEnv
  let mut p : TacticProbe := {}

  let t0 ← IO.monoNanosNow
  let allTags :=
    tacticTagExt.toEnvExtension.getState env |>.importedEntries
      |>.push ((tacticTagExt.exportEntriesFn env (tacticTagExt.getState env)).exported)
  let mut tacTags : NameMap NameSet := {}
  for arr in allTags do
    for (tac, tag) in arr do
      tacTags := tacTags.insert tac (tacTags.getD tac {} |>.insert tag)
  let t1 ← IO.monoNanosNow
  p := { p with tagFold := t1 - t0 }

  let some tactics := (Lean.Parser.parserExtension.getState env).categories.find? `tactic
    | return p

  -- `firstTacticTokens`, split into its three parts.
  let t2 ← IO.monoNanosNow
  let mut firstTokens : NameMap String :=
    Lean.Parser.Tactic.Doc.tacticNameExt.toEnvExtension.getState env
      |>.importedEntries
      |>.push ((Lean.Parser.Tactic.Doc.tacticNameExt.exportEntriesFn env
          (Lean.Parser.Tactic.Doc.tacticNameExt.getState env)).exported)
      |>.foldl (init := {}) fun names inMods =>
        inMods.foldl (init := names) fun names (k, n) => names.insert k n
  let t3 ← IO.monoNanosNow
  p := { p with nameExtFold := t3 - t2 }

  let addFirstTokens table (firsts : NameMap String) : NameMap String × Nat × Nat := Id.run do
    let mut firsts := firsts
    let mut toks := 0
    let mut calls := 0
    for (tok, ps) in table do
      if tok == `«$» then continue
      toks := toks + 1
      for (pa, _) in ps do
        calls := calls + 1
        for (k, ()) in pa.info.collectKinds {} do
          if tactics.kinds.contains k then
            let tok := tok.toString (escape := false)
            firsts := firsts.alter k (·.getD tok)
    return (firsts, toks, calls)

  let t4 ← IO.monoNanosNow
  let (ft, lt, lc) := addFirstTokens tactics.tables.leadingTable firstTokens
  firstTokens := ft
  let t5 ← IO.monoNanosNow
  p := { p with leadingTable := t5 - t4, leadingToks := lt, collectKindsCalls := lc }

  let (ft, tt, tc) := addFirstTokens tactics.tables.trailingTable firstTokens
  firstTokens := ft
  let t6 ← IO.monoNanosNow
  p := { p with trailingTable := t6 - t5, trailingToks := tt,
                collectKindsCalls := p.collectKindsCalls + tc,
                firstTokens := firstTokens.size }

  -- The `tactics.kinds` loop, with the docstring and extension lookups timed
  -- separately. Every result is folded into a counter: a plain `let _ := ...` is
  -- never forced, and the probe then measures nothing (this bit them once).
  let mut docNanos := 0
  let mut extNanos := 0
  let mut produced := 0
  let mut kinds := 0
  let mut extStrings := 0
  let mut tagsSeen := 0
  let mut docsFound := 0
  let t7 ← IO.monoNanosNow
  for (tac, _) in tactics.kinds do
    kinds := kinds + 1
    if let some _ := alternativeOfTactic env tac then continue
    let userName : String := (firstTokens.get? tac).getD tac.toString
    if userName.isEmpty then continue
    let d0 ← IO.monoNanosNow
    let doc ← findDocString? env tac
    let d1 ← IO.monoNanosNow
    docNanos := docNanos + (d1 - d0)
    produced := produced + 1
    docsFound := docsFound + (if doc.isSome then 1 else 0)
    let e0 ← IO.monoNanosNow
    extStrings := extStrings + (getTacticExtensions env tac).size
    let e1 ← IO.monoNanosNow
    extNanos := extNanos + (e1 - e0)
    tagsSeen := tagsSeen + (tacTags.getD tac {}).size
  let t8 ← IO.monoNanosNow
  if docsFound > kinds then throwError "unreachable"  -- keeps `docsFound` live
  p := { p with kindLoop := t8 - t7, docString := docNanos, extensions := extNanos,
                kinds, produced, extStrings, tagsSeen }

  -- The transcription above only accounts for part of one `allTacticDocs` call, so
  -- measure the real function and doc-gen4's filter loop over its result directly.
  let header := env.header
  let mut sink := 0
  let t9 ← IO.monoNanosNow
  for _ in [0 : 5] do
    let ds ← allTacticDocs
    sink := sink + ds.size
  let t10 ← IO.monoNanosNow
  p := { p with direct5 := t10 - t9 }

  let ds ← allTacticDocs
  let names := ds.map (·.internalName)
  let t11 ← IO.monoNanosNow
  for _ in [0 : 5] do
    for d in ds do
      if let some modIdx := env.getModuleIdxFor? d.internalName then
        sink := sink + (if header.moduleNames[modIdx]! == d.internalName then 1 else 0)
  let t12 ← IO.monoNanosNow
  p := { p with bucket5 := t12 - t11 }
  for _ in [0 : 5] do
    for d in ds do
      if let some modIdx := env.getModuleIdxFor? d.internalName then
        sink := sink + (if header.moduleNames[modIdx]! == d.internalName then 1 else 0)
  let t12c ← IO.monoNanosNow
  p := { p with bucket5b := t12c - t12 }

  -- Three nested versions of the same loop; the differences name the cost.
  for _ in [0 : 5] do
    for d in ds do
      sink := sink + (if d.internalName.isAnonymous then 1 else 0)
  let t12d ← IO.monoNanosNow
  p := { p with touch5 := t12d - t12c }
  for _ in [0 : 5] do
    for d in ds do
      sink := sink + (if (env.getModuleIdxFor? d.internalName).isSome then 1 else 0)
  let t12e ← IO.monoNanosNow
  p := { p with lookup5 := t12e - t12d }
  for _ in [0 : 5] do
    for d in ds do
      if let some modIdx := env.getModuleIdxFor? d.internalName then
        sink := sink + (if header.moduleNames[modIdx]! == d.internalName then 1 else 0)
  let t12f ← IO.monoNanosNow
  p := { p with full5 := t12f - t12e }
  let modNames := header.moduleNames
  for _ in [0 : 5] do
    for d in ds do
      if let some modIdx := env.getModuleIdxFor? d.internalName then
        sink := sink + (if modNames[modIdx]! == d.internalName then 1 else 0)
  let t12g ← IO.monoNanosNow
  p := { p with fullHoisted5 := t12g - t12f }

  -- Bisect the filter loop: the hash lookup itself vs. everything around it.
  let c2m := env.const2ModIdx
  let t14 ← IO.monoNanosNow
  for _ in [0 : 5] do
    for n in names do
      sink := sink + (if (env.getModuleIdxFor? n).isSome then 1 else 0)
  let t15 ← IO.monoNanosNow
  p := { p with idxOnly5 := t15 - t14 }
  for _ in [0 : 5] do
    for n in names do
      sink := sink + (if c2m[n]?.isSome then 1 else 0)
  let t16 ← IO.monoNanosNow
  p := { p with hoisted5 := t16 - t15 }

  let t12b ← IO.monoNanosNow
  for _ in [0 : 5] do
    for d in ds do
      sink := sink + d.extensionDocs.size
  let t13 ← IO.monoNanosNow
  p := { p with forceExt5 := t13 - t12b }

  if sink > 1000000000 then throwError "unreachable"
  return p

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
  let runMeta {α : Type} (act : MetaM α) : IO α := do
    let (a, _, _) ← act.toIO coreCtx { env := env } {} {}
    return a

  -- doc-gen4's `getAllModuleDocs`, split in two so the two halves can be told
  -- apart: the per-module part (module docstrings + direct imports) and the part
  -- doc-gen4 repeats per module (the tactic table).
  let tMd0 ← IO.monoNanosNow
  let mods ← runMeta (collectModuleDocs targets)
  let tMd1 ← IO.monoNanosNow
  let modDocCount := mods.foldl (init := 0) fun a m => a + m.docs.size
  let modsWithDocs := mods.foldl (init := 0) fun a m => a + (if m.docs.isEmpty then 0 else 1)
  let importCount := mods.foldl (init := 0) fun a m => a + m.imports.size
  sink.emit "stage2.moduleDocs" (tMd1 - tMd0)
    [("modules", toString mods.size), ("moduleDocs", toString modDocCount),
     ("modulesWithDocs", toString modsWithDocs), ("imports", toString importCount)]

  let tTac0 ← IO.monoNanosNow
  let (mods, tacticsInEnv, tacticsAssigned) ← runMeta (collectTacticsOnce mods)
  let tTac1 ← IO.monoNanosNow
  sink.emit "stage2.tactics" (tTac1 - tTac0)
    [("tacticsInEnv", toString tacticsInEnv), ("tacticsAssigned", toString tacticsAssigned)]

  -- Diagnosis only: the same collection done doc-gen4's way.
  let mut tEmu := 0
  let mut emuAll := 0
  let mut emuFilter := 0
  if cfg.tacticsEmulate then
    let t0 ← IO.monoNanosNow
    let (assigned, allNanos, filterNanos) ← runMeta (collectTacticsPerModule targets)
    let t1 ← IO.monoNanosNow
    tEmu := t1 - t0
    emuAll := allNanos
    emuFilter := filterNanos
    sink.emit "stage2.tacticsPerModule" tEmu
      [("calls", toString targets.size), ("tacticsAssigned", toString assigned),
       ("allTacticDocsUs", toString (allNanos / 1000)),
       ("filterLoopUs", toString (filterNanos / 1000))]

  if let some p := cfg.tacticsDumpPath then
    let n ← runMeta (dumpAllTactics p)
    IO.println s!"dumped {n} tactics (all modules) -> {p}"

  let mut probe : Option TacticProbe := none
  if cfg.tacticsProbe then
    let t0 ← IO.monoNanosNow
    let p ← runMeta probeAllTacticDocs
    let t1 ← IO.monoNanosNow
    probe := some p
    sink.emit "stage2.tacticsProbe" (t1 - t0)
      [("tagFoldUs", toString (p.tagFold / 1000)),
       ("nameExtFoldUs", toString (p.nameExtFold / 1000)),
       ("leadingTableUs", toString (p.leadingTable / 1000)),
       ("trailingTableUs", toString (p.trailingTable / 1000)),
       ("kindLoopUs", toString (p.kindLoop / 1000)),
       ("docStringUs", toString (p.docString / 1000)),
       ("extensionsUs", toString (p.extensions / 1000)),
       ("extStrings", toString p.extStrings), ("tagsSeen", toString p.tagsSeen),
       ("kinds", toString p.kinds), ("produced", toString p.produced),
       ("leadingToks", toString p.leadingToks), ("trailingToks", toString p.trailingToks),
       ("collectKindsCalls", toString p.collectKindsCalls),
       ("firstTokens", toString p.firstTokens),
       ("direct5Us", toString (p.direct5 / 1000)),
       ("bucket5Us", toString (p.bucket5 / 1000)),
       ("forceExt5Us", toString (p.forceExt5 / 1000)),
       ("idxOnly5Us", toString (p.idxOnly5 / 1000)),
       ("hoisted5Us", toString (p.hoisted5 / 1000)),
       ("bucket5bUs", toString (p.bucket5b / 1000)),
       ("touch5Us", toString (p.touch5 / 1000)),
       ("lookup5Us", toString (p.lookup5 / 1000)),
       ("full5Us", toString (p.full5 / 1000)),
       ("fullHoisted5Us", toString (p.fullHoisted5 / 1000))]

  -- Semantic analysis. One fresh `Core.State` per declaration, exactly like
  -- doc-gen4's `process` loop, so that neither side accumulates elaborator state.
  let mut results : Array DeclOut := #[]
  let mut counters : Counters := {}
  let mut blacklisted := 0
  let mut missing := 0
  let mut considered := 0
  let mut failures : Array Failure := #[]
  let tAn0 ← IO.monoNanosNow
  for (name, module) in (if cfg.skipAnalyze then #[] else candidates) do
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

  if let some dumpPath := cfg.dumpModulesPath then
    let tD0 ← IO.monoNanosNow
    let h ← IO.FS.Handle.mk dumpPath .write
    for m in mods do
      h.putStr m.toJson.compress
      h.putStr "\n"
    let tD1 ← IO.monoNanosNow
    sink.emit "stage2.dumpModules" (tD1 - tD0) [("records", toString mods.size)]

  let tTotal1 ← IO.monoNanosNow
  sink.emit "stage2.total" (tTotal1 - tTotal0) [("modules", toString targets.size)]

  let mut byKind : Std.HashMap String Nat := {}
  for d in results do
    byKind := byKind.insert d.kind ((byKind.getD d.kind 0) + 1)

  IO.println s!"target modules       {targets.size}"
  IO.println s!"loaded modules       {header.moduleNames.size}"
  IO.println s!"importModules        {fmtDur (tImp1 - tImp0)}"
  IO.println s!"indexLookup          {fmtDur (tIdx1 - tIdx0)}  enumerated {enumerated}, unique {candidates.size}"
  IO.println s!"moduleDocs           {fmtDur (tMd1 - tMd0)}  {modDocCount} docs in {modsWithDocs} modules, {importCount} imports"
  IO.println s!"tactics              {fmtDur (tTac1 - tTac0)}  {tacticsInEnv} in env, {tacticsAssigned} in target modules"
  if cfg.tacticsEmulate then
    IO.println s!"tacticsPerModule     {fmtDur tEmu}  (doc-gen4's shape: {targets.size} × allTacticDocs)"
    IO.println s!"  of which allTactic {fmtDur emuAll}"
    IO.println s!"  of which filter    {fmtDur emuFilter}"
  if let some p := probe then
    IO.println s!"tacticsProbe         one allTacticDocs, broken down:"
    IO.println s!"  tagFold            {fmtDur p.tagFold}"
    IO.println s!"  nameExtFold        {fmtDur p.nameExtFold}"
    IO.println s!"  leadingTable       {fmtDur p.leadingTable}  ({p.leadingToks} tokens)"
    IO.println s!"  trailingTable      {fmtDur p.trailingTable}  ({p.trailingToks} tokens)"
    IO.println s!"  kindLoop           {fmtDur p.kindLoop}  ({p.kinds} kinds, {p.produced} produced)"
    IO.println s!"    of which docstr  {fmtDur p.docString}"
    IO.println s!"    of which extens  {fmtDur p.extensions}  ({p.extStrings} extension strings, {p.tagsSeen} tags)"
    IO.println s!"  collectKinds calls {p.collectKindsCalls}, firstTokens {p.firstTokens}"
    IO.println s!"  5x allTacticDocs   {fmtDur p.direct5}  ({fmtDur (p.direct5 / 5)} each)"
    IO.println s!"  5x filter loop     {fmtDur p.bucket5}  ({fmtDur (p.bucket5 / 5)} each)"
    IO.println s!"  5x filter again    {fmtDur p.bucket5b}  ({fmtDur (p.bucket5b / 5)} each)"
    IO.println s!"  5x touch name      {fmtDur p.touch5}"
    IO.println s!"  5x + getModuleIdx  {fmtDur p.lookup5}"
    IO.println s!"  5x + moduleNames[] {fmtDur p.full5}"
    IO.println s!"  5x moduleNames hoisted {fmtDur p.fullHoisted5}"
    IO.println s!"  5x force extDocs   {fmtDur p.forceExt5}  ({fmtDur (p.forceExt5 / 5)} each)"
    IO.println s!"  5x getModuleIdxFor {fmtDur p.idxOnly5}  ({fmtDur (p.idxOnly5 / 5)} each)"
    IO.println s!"  5x hoisted const2ModIdx {fmtDur p.hoisted5}  ({fmtDur (p.hoisted5 / 5)} each)"
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
  | _ => .error "usage: extract <modules.txt> <out.jsonl> [--equations] [--dump <p>] [--dump-modules <p>] [--only <p>] [--open <ns,..>] [--tag] [--skip-analyze] [--tactics-emulate] [--tactics-probe]"
where
  go (cfg : Cfg) : List String → Except String Cfg
  | [] => .ok cfg
  | "--equations" :: rest => go { cfg with genEquations := true } rest
  | "--tag" :: rest => go { cfg with tagCode := true } rest
  | "--skip-analyze" :: rest => go { cfg with skipAnalyze := true } rest
  | "--tactics-emulate" :: rest => go { cfg with tacticsEmulate := true } rest
  | "--tactics-probe" :: rest => go { cfg with tacticsProbe := true } rest
  | "--dump" :: p :: rest => go { cfg with dumpPath := some ⟨p⟩ } rest
  | "--dump-modules" :: p :: rest => go { cfg with dumpModulesPath := some ⟨p⟩ } rest
  | "--dump-tactics" :: p :: rest => go { cfg with tacticsDumpPath := some ⟨p⟩ } rest
  | "--only" :: p :: rest => go { cfg with onlyPath := some ⟨p⟩ } rest
  | "--open" :: ns :: rest =>
    go { cfg with openNamespaces := (ns.splitOn ",").toArray.map (·.trimAscii.toString.toName) } rest
  | a :: _ => .error s!"unknown argument: {a}"

def main (args : List String) : IO UInt32 := do
  match parseArgs args with
  | .ok cfg => Stage2.run cfg
  | .error msg => IO.eprintln msg; return 1
