/-
Stage 4b experiment for lean-doc (see `docs/approach.md` §5.4 / §5.5 / §6.1 and
`docs/plans/three-axes.md` leg 4).

Started as a copy of `experiments/stage4/Extract.lean`, which was itself a copy of
`experiments/stage3/Extract.lean` (stages 1-4 are frozen for reproducibility of
their numbers). Everything stage 4 did is still here unchanged.

What stage 4b adds is **positional tagged code** in the IR (`--tagged-code`).
Stage 4 increment 1 measured that a *set* of `(module, name)` references is not
enough to rebuild doc-gen4's signature HTML: 44.0% of its 72,421 signature anchors
have no textual relation between the printed token and the constant (`ℕ`->`Nat`,
`≤`->`LE.le`, `{`/`}`->`Singleton.singleton`), reconstruction from plain text plus
the set tops out at 56% recall (51.7% measured), and signatures are 71.1% of the
rendered bytes. So the IR has to carry *where in the printed text* each tag sits.
See `collectSpans` for the format and `experiments/stage4b/README.md` for why it
is a flat pre-order list rather than a tree.

`--tagged-code` also adds the three small things increment 1 found missing:
the declaration range's **end** line/column (doc-gen4's `gh_link` is
`#L<start>-L<end>`), the `noncomputable` / `abbrev` / `unsafe` / `partial`
**modifiers** of the kind word, and the declaration's **index** within its module
(two modules on the fixed target have declarations whose `(line, col)` tie).

Both IR persistence and tagged code are OFF by default (`--write-ir` and
`--tagged-code` turn them on) so that the stage-3 and stage-4 baselines can still
be reproduced from this tree; see the README's "Baseline identity".

The extraction stage 2 already did, for every declaration reached through the
index route:

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

From stage 3: `--refs` collects, per declaration, every constant doc-gen4's
`renderTagged` would tag as `.const` — see `collectConsts` for the exact
semantics — and `--dump-refs` writes the unique set with its defining module.
Both are off by default so that the stage-2 baseline can still be reproduced
byte for byte from this binary.

From stage 4: `--write-ir` persists the result as module-granular IR (see the
"IR persistence" section below for the on-disk shape). `--refs` feeds it: without
`--refs` the per-declaration `refs` arrays are empty, so measure the two together.

New in stage 4b: `--tagged-code`. It makes the code walk record positions as well
as names, and puts them (plus the end position, the modifiers and the index) into
the IR. With `--refs` on as well the names are *derived from* the positions, so
the two walks are one walk and `refOccurrences` is unchanged.

Usage: extract <modules.txt> <out.jsonl> [options]
  --equations         generate equation lemmas (default: off)
  --write-ir          persist the result as one JSON file per module + an index
                      + a dependency-side map slice (default: off)
  --tagged-code       record, per printed fragment, the pre-order list of tag
                      spans `renderTagged` would produce, and add the declaration
                      range end / kind modifiers / in-module index to the IR
                      (default: off; bumps the IR schema version to 2)
  --ir-dir <path>     where to write it. Precedence: this flag, then the
                      `IR_DIR` environment variable, then `defaultIrDir` below.
                      **Never point this inside the measurement target.**
  --dump <path>       write one JSON object per declaration to <path>
  --dump-modules <p>  write one JSON object per module (docs / imports / tactics)
  --only <path>       restrict processing to the declaration names in <path>
                      (one per line); for inspecting individual signatures
  --open <ns>[,<ns>]  pretty print with these namespaces opened
                      (probe for scoped notation; doc-gen4 opens nothing)
  --tag               additionally run `Widget.tagCodeInfos`, the step doc-gen4
                      needs to turn a signature into linkable `RenderedCode`
  --refs              collect the constants doc-gen4 would link from the
                      signature, the equations and the structure parent types
  --dump-refs <path>  write the unique set of those constants, one JSON object
                      per constant, with its defining module (needs --refs)
  --skip-analyze      skip the semantic analysis (module docs / tactics only)
  --tactics-emulate   additionally run the tactic collection doc-gen4's way
                      (`allTacticDocs` once per module) for comparison
  --tactics-probe     additionally break `allTacticDocs` down into its parts
-/
import Lean

open Lean System Meta PrettyPrinter
open Lean.Elab.Tactic.Doc (TacticDoc allTacticDocs firstTacticTokens)
open Lean.Parser.Tactic.Doc (tacticTagExt alternativeOfTactic getTacticExtensions)

namespace Stage4b

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
  /-- `--refs`: collect the constants doc-gen4 would link. Independent of `--tag`,
  which stays as the stage-2 baseline for the cost of `Widget.tagCodeInfos`. -/
  collectRefs : Bool := false
  dumpRefsPath : Option FilePath := none
  skipAnalyze : Bool := false
  tacticsEmulate : Bool := false
  tacticsProbe : Bool := false
  tacticsDumpPath : Option FilePath := none
  /-- `--write-ir`: persist the result as module-granular IR. Off by default so
  that this binary still reproduces the stage-3 baseline. -/
  writeIR : Bool := false
  /-- `--ir-dir`. `none` means "fall back to `IR_DIR`, then `defaultIrDir`". -/
  irDir : Option FilePath := none
  /-- `--tagged-code`: record tag positions, not just tag names, and add the
  declaration range end / kind modifiers / in-module index to the IR. Off by
  default so that this binary still reproduces the stage-4 IR byte for byte. -/
  taggedCode : Bool := false
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

/-! ## Referenced constants — the demand side of the link map

doc-gen4 turns a `Format` into linkable `RenderedCode` in two steps:
`Widget.tagCodeInfos` (Lean core, `Lean/Widget/InteractiveCode.lean`) wraps every
tag position in a `SubexprInfo`, and doc-gen4's own `renderTagged`
(`DocGen4/RenderedCode.lean`) is what actually decides which of them become
`<a href>`. Only the second step names constants.
-/

/-- Per-declaration output of the reference collector. -/
structure RefAcc where
  /-- Every occurrence, not a set: the unique count and the occurrence count are
  both wanted, so the deduplication happens in the driver. -/
  names : Array Name := #[]
  /-- Time spent inside the collector itself (including `prettyTagged`). -/
  nanos : Nat := 0
  deriving Inhabited

/--
The names doc-gen4's `renderTagged` tags as `.const`, i.e. exactly the ones that
become links in its HTML.

`renderTagged` matches `.tag i t` where `i.info.val.info` is `Elab.Info.ofTermInfo ti`
and `ti.expr.consumeMData` is `.const c _`. Here the same test is done directly
against `infos`, skipping `Widget.tagCodeInfos`: the only thing that step adds is a
`WithRpcRef.mk` per tag (an `IO.Ref` allocation for the RPC layer), which cannot
change which names come out.

Two deliberate choices, both "reproduce doc-gen4" rather than "be complete",
because stage 3 asks whether lean-doc reaches *the same* link targets:

* `Elab.Info.ofFieldInfo` and `.ofDelabTermInfo` carry constants too, but
  `renderTagged` only matches `.ofTermInfo`, so those are not links in doc-gen4's
  output and are not collected here either. This is likely a doc-gen4 oversight;
  it is reproduced on purpose.
* `Expr.sort` gets its own (non-constant) tag in `renderTagged`, and every other
  `Expr` head falls through to `.otherExpr`. Neither yields a name.

Walking *every* tag is equivalent to `renderTagged`'s recursion: it descends into
the subtree of a `.const` tag whenever that subtree is not a bare `.text`, and a
bare `.text` subtree carries no tags at all.
-/
partial def collectConsts (infos : SubExpr.PosMap Elab.Info)
    (tt : Widget.TaggedText (Nat × Nat)) (acc : Array Name) : Array Name :=
  match tt with
  | .text _ => acc
  | .append xs => xs.foldl (init := acc) fun acc x => collectConsts infos x acc
  | .tag (n, _) t =>
    let acc :=
      match infos.get? n with
      | some (.ofTermInfo ti) =>
        match ti.expr.consumeMData with
        | .const c _ => acc.push c
        | _ => acc
      | _ => acc
    collectConsts infos t acc

/-! ### Positional tags — the same walk, keeping the offsets (stage 4b)

`collectConsts` above throws the positions away. `collectSpans` below does not:
it is the same traversal, but every tag that survives into doc-gen4's HTML comes
out as a half-open interval over the fragment's **plain text**, in pre-order.

Offsets are in **UTF-16 code units**, not characters and not UTF-8 bytes. See the
README ("Offsets") for why; the short version is that the consumer is a
`String`-slicing runtime and this is the unit that makes `s.slice(start, stop)`
correct there without a conversion pass. Lean's own `String.length` is code
points, so nothing here may use it.
-/

/-- Width of one character in UTF-16 code units. -/
@[inline] def charUtf16 (c : Char) : Nat := if c.val < 0x10000 then 1 else 2

/-- Length of `s` in UTF-16 code units. -/
def utf16Len (s : String) : Nat :=
  String.foldl (fun n c => n + charUtf16 c) 0 s

/--
One tag position over a printed fragment: `[start, stop)` in UTF-16 code units.

`kind` is exactly the three `RenderedCode.Tag`s that reach doc-gen4's HTML as an
element (`DocGen4/Output/Base.lean:334-389`):

| kind | tag | element |
|---|---|---|
| 0 | `.otherExpr` | `<span class="fn">` |
| 1 | `.const name` | `<a href="…#name">`, or `<span class="fn">` when the name is not linkable |
| 2 | `.sort _` | `<a href="…foundational_types.html">` |

`.keyword` and `.string` are deliberately absent: `renderedCodeToHtmlAux` renders
both as plain content, so they produce no element and no bytes.
-/
structure Span where
  start : Nat
  stop : Nat
  kind : Nat
  /-- Only meaningful for `kind = 1`. -/
  name : Name := .anonymous
  deriving Inhabited

/--
doc-gen4's `splitWhitespaces` (`DocGen4/RenderedCode.lean:150-157`) in offset
form: a `.const` tag whose body is bare text links only the trimmed token, the
surrounding whitespace stays outside the `<a>`.

Returns `(leading whitespace, total UTF-16 width, trailing whitespace)`. Both
whitespace counts are ASCII, so characters and UTF-16 units coincide there. The
all-whitespace case matches doc-gen4's: `trimAsciiStart` empties the string first,
so `back` is 0 and the (empty) anchor lands at the end.
-/
def wsTrim (s : String) : Nat × Nat × Nat :=
  let r := String.foldl
    (fun (acc : Nat × Nat × Nat × Bool) c =>
      let (front, total, back, leading) := acc
      if c.isWhitespace then
        (if leading then front + 1 else front, total + charUtf16 c, back + 1, leading)
      else
        (front, total + charUtf16 c, 0, false))
    (0, 0, 0, true) s
  let (front, total, back, _) := r
  (front, total, if front == total then 0 else back)

/-- doc-gen4's sort split (`DocGen4/RenderedCode.lean:258-269`): when a `.sort`
tag's body is bare text, only the part before the first space (`Type` / `Prop` /
`Sort`) is inside the link; the universe that follows is not. -/
def sortPrefixLen (s : String) : Nat :=
  (String.foldl
    (fun (acc : Nat × Bool) c =>
      let (n, done) := acc
      if done || c == ' ' then (n, true) else (n + charUtf16 c, false))
    (0, false) s).1

/--
The pre-order span list of one formatted fragment.

Node for node the same walk as `renderTagged` (`DocGen4/RenderedCode.lean:240-274`)
composed with `Widget.tagCodeInfos`:

* a tag position that is not in `infos` is **dropped** by `tagCodeInfos`
  (`Lean/Widget/InteractiveCode.lean`, the `none` branch returns the subtree
  untagged), so `renderTagged` never sees it and no span is emitted — the subtree
  is still walked;
* `.ofTermInfo` whose expression is `.const c _` -> kind 1, `.sort _` -> kind 2,
  anything else -> kind 0; any other `Elab.Info` -> kind 0. This reproduces
  doc-gen4, including its blind spot for `.ofFieldInfo` / `.ofDelabTermInfo`
  (see `collectConsts`);
* the two bare-text special cases (`wsTrim`, `sortPrefixLen`) narrow the span the
  way `renderTagged` narrows the tag.

**Pre-order, parent before child, outer before inner at equal offsets.** That
ordering is the whole nesting rule: a consumer replays the list on a stack and
closes a span when the next one starts beyond its `stop`. The parent's slot is
therefore reserved *before* its subtree is walked and patched afterwards.
-/
partial def collectSpans (infos : SubExpr.PosMap Elab.Info)
    (tt : Widget.TaggedText (Nat × Nat)) (acc : Array Span) (off : Nat) : Array Span × Nat :=
  match tt with
  | .text s => (acc, off + utf16Len s)
  | .append xs => xs.foldl (init := (acc, off)) fun (acc, off) x => collectSpans infos x acc off
  | .tag (n, _) t =>
    match infos.get? n with
    | none => collectSpans infos t acc off
    | some info =>
      let (kind, name) : Nat × Name :=
        match info with
        | .ofTermInfo ti =>
          match ti.expr.consumeMData with
          | .const c _ => (1, c)
          | .sort _ => (2, .anonymous)
          | _ => (0, .anonymous)
        | _ => (0, .anonymous)
      match kind, t with
      | 1, .text s =>
        let (front, total, back) := wsTrim s
        (acc.push ⟨off + front, off + total - back, 1, name⟩, off + total)
      | 2, .text s =>
        (acc.push ⟨off, off + sortPrefixLen s, 2, name⟩, off + utf16Len s)
      | _, _ =>
        let idx := acc.size
        let acc := acc.push ⟨off, off, kind, name⟩
        let (acc, off') := collectSpans infos t acc off
        (acc.set! idx ⟨off, off', kind, name⟩, off')

/-- The per-declaration code-walk sink. `--refs` wants the names, `--tagged-code`
wants the positions; either flag creates it, and with both on there is still only
one walk (the names are read off the spans). -/
structure CodeSink where
  ref : IO.Ref RefAcc
  /-- `--tagged-code`. -/
  tagged : Bool
  /-- `--refs`. -/
  wantNames : Bool

/-- `none` when both `--refs` and `--tagged-code` are off, and then every
collector call is a no-op. -/
abbrev RefSink := Option CodeSink

/--
Walks one formatted fragment, folds the constant names into the per-declaration
accumulator and hands the spans back to the caller (empty without `--tagged-code`).

`text` must be `fmt.pretty` of the same `fmt`; it is what the spans index.
-/
def RefSink.collect (sink : RefSink) (fmt : Std.Format) (text : String)
    (infos : SubExpr.PosMap Elab.Info) : MetaM (Array Span) := do
  let some s := sink | return #[]
  let r := s.ref
  let t0 ← IO.monoNanosNow
  let mut spans : Array Span := #[]
  if s.tagged then
    let (sp, width) := collectSpans infos (Widget.TaggedText.prettyTagged fmt) #[] 0
    -- Not a debug assertion. It is the only thing between a correct offset and a
    -- silently shifted one, and it is also what keeps the walk inside the timer:
    -- a pure `let` whose consumers are all below the next clock read gets sunk
    -- past it, and three phases of this experiment measured zero that way.
    if width != utf16Len text then
      throwError "tagged-code width {width} does not match the printed width {utf16Len text}"
    if s.wantNames then
      r.modify fun a =>
        { a with names := sp.foldl (init := a.names) fun ns x =>
            if x.kind == 1 then ns.push x.name else ns }
    spans := sp
  else if s.wantNames then
    -- The walk is written *inside* `modify` rather than in a `let` above it. A pure
    -- `let` whose only consumer is the closure below can be sunk into that closure,
    -- and then nothing happens between `t0` and `t1` and `refUs` measures zero.
    -- Appending forces the array, so this way the work is inside the timer. Stage 2's
    -- `--tactics-probe` was bitten by the same class of mistake.
    r.modify fun a =>
      { a with names := a.names ++ collectConsts infos (Widget.TaggedText.prettyTagged fmt) #[] }
  let t1 ← IO.monoNanosNow
  r.modify fun a => { a with nanos := a.nanos + (t1 - t0) }
  return spans

/-! ## Pretty printing -/

/-- A pretty printed signature: the binders in front of the `:` and the type after it.
The `*Spans` fields are empty without `--tagged-code`; each one indexes the string
next to it. -/
structure Sig where
  binders : Array String
  implicits : Array Bool
  type : String
  binderSpans : Array (Array Span) := #[]
  typeSpans : Array Span := #[]
  deriving Inhabited

/--
The same path as doc-gen4's `Info.ofTypedName`, minus the tagging step: delaborate
the type as a `declSig`, sanitize, parenthesize, then format the binders one by one
and the result type separately.

`currNamespace := n.getPrefix` mirrors doc-gen4. Note that `openDecls` stays at
whatever the caller put in the `Core.Context` -- doc-gen4 leaves it empty, which is
why scoped notation never appears in its output.
-/
def ppSignature (tagCode : Bool) (refs : RefSink) (n : Name) (t : Expr) : MetaM Sig := do
  let (sigStx, infos) ← withTheReader Core.Context ({ · with currNamespace := n.getPrefix }) <|
    delabCore t (delab := Delaborator.delabForallParamsWithSignature fun binders type =>
      `(declSig| $binders* : $type))
  let sigStx := (sanitizeSyntax sigStx).run' { options := (← getOptions) }
  let sigStx ← parenthesize Parser.Command.declSig.parenthesizer sigStx
  let `(declSig| $binders* : $type) := sigStx
    | throwError "signature pretty printer failure for {n}"
  let mut bs : Array String := #[]
  let mut imps : Array Bool := #[]
  let mut bspans : Array (Array Span) := #[]
  for binder in binders do
    let fmt ← PrettyPrinter.format Parser.Term.bracketedBinder.formatter binder.raw
    if tagCode then
      let _ ← tagIt fmt infos
    let txt := fmt.pretty
    bspans := bspans.push (← refs.collect fmt txt infos)
    bs := bs.push txt
    imps := imps.push (!binder.raw.isOfKind ``Parser.Term.explicitBinder)
  let fmt ← PrettyPrinter.formatTerm type.raw
  if tagCode then
    let _ ← tagIt fmt infos
  let txt := fmt.pretty
  let tspans ← refs.collect fmt txt infos
  return { binders := bs, implicits := imps, type := txt,
           binderSpans := bspans, typeSpans := tspans }
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

/--
doc-gen4's `prettyPrintTerm` (`Process/Base.lean`), without the tagging.

With `--refs` off this is stage 2's `Meta.ppExpr` verbatim, so the baseline output
and timing are unchanged. With `--refs` on it switches to `ppExprWithInfos`, the
call doc-gen4 makes, because that is the only way to get the `infos` the constants
live in; `Meta.ppExpr` is `ppExprWithInfos` with the map thrown away, so the
printed text is the same either way.
-/
def ppTermTagged (refs : RefSink) (e : Expr) : MetaM (String × Array Span) := do
  match refs with
  | none => return ((← Meta.ppExpr e).pretty, #[])
  | some _ =>
    let ⟨fmt, infos⟩ ← PrettyPrinter.ppExprWithInfos e
    let txt := fmt.pretty
    let spans ← refs.collect fmt txt infos
    return (txt, spans)

def ppTerm (refs : RefSink) (e : Expr) : MetaM String := do
  return (← ppTermTagged refs e).1

/-! ## Equations (doc-gen4's `DefinitionInfo.computeEquations?`) -/

def valueToEq (v : DefinitionVal) : MetaM Expr := withLCtx {} {} do
  withOptions (Lean.Meta.tactic.hygienic.set · false) do
    lambdaTelescope v.value fun xs body => do
      let us := v.levelParams.map mkLevelParam
      let type ← mkEq (mkAppN (mkConst v.name us) xs) body
      mkForallFVars xs type

def ppEquation (refs : RefSink) (e : Expr) : MetaM (String × Array Span) :=
  forallTelescope e.consumeMData fun _ body => ppTermTagged refs body

def computeEquations (refs : RefSink) (v : DefinitionVal) :
    MetaM (Array (String × Array Span)) := do
  match ← getEqnsFor? v.name with
  | some eqs =>
    eqs.mapM fun eq => do ppEquation refs (← mkConstWithFreshMVarLevels eq >>= inferType)
  | none => return #[← ppEquation refs (← valueToEq v)]

/-! ## The extracted record -/

structure Member where
  label : String
  name : Name
  text : String
  /-- Spans over `text`. Empty without `--tagged-code`. -/
  spans : Array Span := #[]

structure DeclOut where
  name : Name
  module : Name
  kind : String
  sig : Sig
  doc : Option String
  line : Nat
  col : Nat
  /-- End of `DeclarationRanges.range`, the same range doc-gen4 stores
  (`DocGen4/Process/NameInfo.lean:124`) and feeds to `mkGithubSourceLinker`
  (`DocGen4/Output/SourceLinker.lean:12-14`) as `#L<line>-L<endLine>`. Stage 4
  dropped it, so the IR could not produce a `gh_link`. -/
  endLine : Nat := 0
  endCol : Nat := 0
  equations : Array String
  eqFailed : Bool := false
  members : Array Member := #[]
  /-- Every constant doc-gen4 would link from this declaration's signature,
  equations and structure parent types, in order of appearance and with
  duplicates. Empty unless `--refs`. -/
  refs : Array Name := #[]
  /-- Spans over `equations`, index for index. Empty without `--tagged-code`. -/
  equationSpans : Array (Array Span) := #[]
  /-- The words doc-gen4's `getKindDescription` puts in front of the kind word.
  Empty without `--tagged-code`; see `declModifiers`. -/
  modifiers : Array String := #[]

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
                  ("text", Json.str m.text)])),
    ("refs", Json.arr (d.refs.map (Json.str ·.toString)))
  ]

/-! ## Analysis of one declaration -/

structure Counters where
  ppNanos : Nat := 0
  eqNanos : Nat := 0
  docNanos : Nat := 0
  eqCount : Nat := 0
  eqFailures : Nat := 0
  /-- Time inside the reference collector. It runs *inside* the pretty printing it
  measures, so this is a part of `ppNanos` (and of `eqNanos` for the equations),
  not an addition to it — same as what `--tag` does to `ppNanos` in stage 2. -/
  refNanos : Nat := 0
  /-- Occurrences, not unique names. -/
  refCount : Nat := 0
  deriving Inhabited

abbrev AnalyzeM := StateRefT Counters MetaM

def timedPp (act : MetaM α) : AnalyzeM α := do
  let t0 ← IO.monoNanosNow
  let r ← act
  let t1 ← IO.monoNanosNow
  modify fun c => { c with ppNanos := c.ppNanos + (t1 - t0) }
  return r

/-- doc-gen4's `Info.ofConstantVal` + `NameInfo.ofTypedName` for one name. -/
def baseInfo (cfg : Cfg) (refs : RefSink) (module : Name) (kind : String) (cv : ConstantVal) :
    AnalyzeM DeclOut := do
  let e := Expr.const cv.name (cv.levelParams.map mkLevelParam)
  let t ← inferType e
  let sig ← timedPp (ppSignature cfg.tagCode refs cv.name t)
  let tDoc0 ← IO.monoNanosNow
  let doc ← Lean.findDocString? (← getEnv) cv.name
  let tDoc1 ← IO.monoNanosNow
  modify fun c => { c with docNanos := c.docNanos + (tDoc1 - tDoc0) }
  let some ranges ← findDeclarationRanges? cv.name
    | throwError "{cv.name} is a declaration without position"
  return {
    name := cv.name, module, kind, sig, doc,
    line := ranges.range.pos.line, col := ranges.range.pos.column,
    endLine := ranges.range.endPos.line, endCol := ranges.range.endPos.column,
    equations := #[]
  }

def withEquations (cfg : Cfg) (refs : RefSink) (v : DefinitionVal) (d : DeclOut) :
    AnalyzeM DeclOut := do
  unless cfg.genEquations do return d
  let t0 ← IO.monoNanosNow
  let r ← tryCatchRuntimeEx
    (do let eqs ← computeEquations refs v; return Except.ok eqs)
    (fun e => do return Except.error (← e.toMessageData.toString))
  let t1 ← IO.monoNanosNow
  modify fun c => { c with eqNanos := c.eqNanos + (t1 - t0) }
  match r with
  | .ok eqs =>
    modify fun c => { c with eqCount := c.eqCount + eqs.size }
    return { d with equations := eqs.map (·.1), equationSpans := eqs.map (·.2) }
  | .error _ =>
    modify fun c => { c with eqFailures := c.eqFailures + 1 }
    return { d with eqFailed := true }

/-- Fields and parents of a structure, as doc-gen4's `getFieldTypes` computes them. -/
def structureMembers (cfg : Cfg) (refs : RefSink) (v : InductiveVal) :
    AnalyzeM (Array Member) := do
  let env ← getEnv
  let structName := v.name
  let us := v.levelParams.map mkLevelParam
  let ctorVal := getStructureCtor env structName
  let ctorSig ← timedPp (ppSignature cfg.tagCode refs ctorVal.name ctorVal.type)
  let out : Array Member := #[⟨"ctor", ctorVal.name, ctorSig.type, ctorSig.typeSpans⟩]
  let inner ← timedPp <|
    forallTelescopeReducing v.type fun params _ =>
      withLocalDeclD `self (mkAppN (mkConst structName us) params) fun s => do
        let mut acc : Array Member := #[]
        for parent in getStructureParentInfo env structName do
          let proj := mkApp (mkAppN (mkConst parent.projFn us) params) s
          let (text, spans) ← ppTermTagged refs (← inferType proj)
          acc := acc.push ⟨"parent", parent.projFn, text, spans⟩
        for fieldName in getStructureFieldsFlattened env structName (includeSubobjectFields := false) do
          let proj ← mkProjection s fieldName
          let ty ← inferType proj
          let projFn := (getProjFnForField? env structName fieldName).getD (structName ++ fieldName)
          let sig ← ppSignature cfg.tagCode refs projFn ty
          acc := acc.push ⟨"field", projFn, sig.type, sig.typeSpans⟩
        return acc
  return out ++ inner

def inductiveMembers (cfg : Cfg) (refs : RefSink) (v : InductiveVal) :
    AnalyzeM (Array Member) := do
  let mut out : Array Member := #[]
  for ctor in v.ctors do
    let cv := (← getConstInfoCtor ctor).toConstantVal
    let e := Expr.const cv.name (cv.levelParams.map mkLevelParam)
    let sig ← timedPp (ppSignature cfg.tagCode refs cv.name (← inferType e))
    out := out.push ⟨"ctor", cv.name, sig.type, sig.typeSpans⟩
  return out

/--
The words doc-gen4's `getKindDescription` (`DocGen4/Process/DocInfo.lean:211-247`)
puts in front of the kind word, as flags. Stage 4's IR only had `kind`, and on the
fixed target that mislabels 456 of 650 `definition`s and 1 of 56 `instance`s.

The composition rule is doc-gen4's, and the consumer has to reapply it (README,
"Kind modifiers"):

| `kind` | `span.decl_kind` |
|---|---|
| `definition` | `unsafe`? `noncomputable`? then `abbrev` if present else `def` |
| `instance` | `unsafe`? `noncomputable`? then `instance` |
| `axiom` | `unsafe`? then `axiom` |
| `opaque` | `partial def` if `partial`, else `unsafe opaque` if `unsafe`, else `opaque` |
| `inductive` | `unsafe`? then `inductive` |
| everything else | the kind word alone |

Where each flag comes from, in doc-gen4:

* `unsafe`, `noncomputable`, `abbrev`: `DefinitionInfo.ofDefinitionVal`
  (`Process/DefinitionInfo.lean:41-60`) — `v.safety`, `isNoncomputable`, `v.hints`;
* `partial`: `OpaqueInfo.ofOpaqueVal` (`Process/OpaqueInfo.lean:15-29`) — the
  existence of `Compiler.mkUnsafeRecName v.name`, which wins over `unsafe`;
* nothing for a theorem, even when it is an instance:
  `InstanceInfo.ofTheoremVal` (`Process/InstanceInfo.lean:66-85`) hard-codes
  `isUnsafe := false` and `isNonComputable := false`;
* nothing for `structure` / `class` / `class inductive`: those `getKindDescription`
  branches ignore `isUnsafe`, and `.quotInfo` becomes an `opaque` with
  `definitionSafety := .safe`.
-/
def declModifiers (ci : ConstantInfo) (kind : String) : MetaM (Array String) := do
  let env ← getEnv
  match ci with
  | .axiomInfo i => return if i.isUnsafe then #["unsafe"] else #[]
  | .opaqueInfo i =>
    if (env.find? (Compiler.mkUnsafeRecName i.name)).isSome then return #["partial"]
    else if i.isUnsafe then return #["unsafe"]
    else return #[]
  | .defnInfo i =>
    let mut m : Array String := #[]
    if i.safety == DefinitionSafety.unsafe then m := m.push "unsafe"
    if isNoncomputable env i.name then m := m.push "noncomputable"
    if kind == "definition" && i.hints.isAbbrev then m := m.push "abbrev"
    return m
  | .inductInfo i => return if kind == "inductive" && i.isUnsafe then #["unsafe"] else #[]
  | _ => return #[]

/-- doc-gen4's `DocInfo.ofConstant`, minus attributes / instance type indices. -/
def analyzeCore (cfg : Cfg) (refs : RefSink) (module : Name) (name : Name) (ci : ConstantInfo) :
    AnalyzeM (Option DeclOut) := do
  if ← isBlackListed name then
    return none
  match ci with
  | .axiomInfo i => return some (← baseInfo cfg refs module "axiom" i.toConstantVal)
  | .thmInfo i =>
    let kind ←
      if ← isProjFn i.name then pure "theorem"
      else if ← isInstanceDecl i.name then pure "instance"
      else pure "theorem"
    return some (← baseInfo cfg refs module kind i.toConstantVal)
  | .opaqueInfo i => return some (← baseInfo cfg refs module "opaque" i.toConstantVal)
  | .defnInfo i =>
    let kind ←
      if ← isProjFn i.name then pure "definition"
      else if ← isInstanceDecl i.name then pure "instance"
      else pure "definition"
    let d ← baseInfo cfg refs module kind i.toConstantVal
    return some (← withEquations cfg refs i d)
  | .inductInfo i =>
    let env ← getEnv
    let isStruct := isStructure env i.name
    let isCls := isClass env i.name
    let kind :=
      if isStruct then (if isCls then "class" else "structure")
      else (if isCls then "class_inductive" else "inductive")
    let d ← baseInfo cfg refs module kind i.toConstantVal
    let members ← if isStruct then structureMembers cfg refs i else inductiveMembers cfg refs i
    return some { d with members }
  | .ctorInfo i => return some (← baseInfo cfg refs module "constructor" i.toConstantVal)
  | .quotInfo i => return some (← baseInfo cfg refs module "opaque" i.toConstantVal)
  | .recInfo _ => return none

/--
`analyzeCore` plus the per-declaration reference accumulator: one `IO.Ref` is
created here (only with `--refs`), every pretty printing path below appends to it,
and what comes out lands in `DeclOut.refs` and in the counters.
-/
def analyze (cfg : Cfg) (module : Name) (name : Name) (ci : ConstantInfo) :
    AnalyzeM (Option DeclOut) := do
  let refs : RefSink ←
    if cfg.collectRefs || cfg.taggedCode then
      let r ← IO.mkRef {}
      pure (some { ref := r, tagged := cfg.taggedCode, wantNames := cfg.collectRefs })
    else
      pure none
  let d? ← analyzeCore cfg refs module name ci
  let acc ← match refs with
    | some s => s.ref.get
    | none => pure {}
  modify fun c => { c with
    refNanos := c.refNanos + acc.nanos, refCount := c.refCount + acc.names.size }
  match d? with
  | none => return none
  | some d =>
    let d := { d with refs := acc.names }
    -- Behind the flag on purpose: with `--tagged-code` off this function must do
    -- exactly what stage 4 did, timings included.
    if cfg.taggedCode then
      return some { d with modifiers := ← declModifiers ci d.kind }
    else
      return some d

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

/-! ## IR persistence (stage 4)

`approach.md` §5.4: the granularity is the module, so the layout is

```
<irDir>/index.json                     package index: module list, hash, path
<irDir>/modules/<Module.Name>.json     one file per module (the IR proper)
<irDir>/deps/<Root>.json               dependency-side map slice, one per package
```

Three properties are deliberate, all of them conclusions of stage 3:

* **Absolute identifiers only.** A reference is a `(defining module, name)` pair.
  No URL — relative or absolute — is stored; relativisation happens at output
  time (`approach.md` §5.6, verification log stage 3 increment 3).
* **The dependency slice is two columns** (name -> module) and covers only the
  constants this package actually refers to. Stage 3 increment 2 measured that
  slice at 53 KB against 34.3 MB for the whole of doc-gen4's `declaration-data.bmp`.
* **Every module carries a content hash**, which `approach.md` §5.5 wants as the
  single source of truth for "what has to be re-extracted / re-rendered".

The format is JSON because §5.4's point is that the *granularity* decides, not the
format; this is a throwaway experiment, not a format decision.
-/

/-- Schema version of the on-disk IR (`approach.md` §5.4). Part of every module
file, therefore part of every module hash: a schema change invalidates the cache.

Version 1 is stage 4's; version 2 is what `--tagged-code` writes. The flag picks
the version rather than always bumping it, so that with the flag off this binary
still reproduces stage 4's IR byte for byte. -/
def irSchemaVersion (tagged : Bool) : Nat := if tagged then 2 else 1

/-- Default IR output directory. Chosen **outside the measurement target**
(`/Users/haruka/dev/lean-projects` must never be written to) and outside this
repository (the IR is several MB and regenerable). Overridden by `IR_DIR` and by
`--ir-dir`; see `resolveIrDir`. -/
def defaultIrDir : FilePath :=
  "/private/tmp/claude-502/-Users-haruka-dev-lean-doc/2dbcb565-edbc-4bd9-846b-574772a9c30c/scratchpad/ir-tagged"

def resolveIrDir (cfg : Cfg) : IO FilePath := do
  match cfg.irDir with
  | some p => return p
  | none =>
    match ← IO.getEnv "IR_DIR" with
    | some s => return ⟨s⟩
    | none => return defaultIrDir

/-- 16 hex digits of Lean's `String.hash`.

**This is a 64-bit non-cryptographic hash**, not SHA-256: Lean core (v4.31.0)
ships no digest, and pulling one in would put an unrelated implementation inside a
number this experiment is trying to measure. For 432 modules the collision
probability is ~5e-15, which is fine for change detection; it is *not* fine as a
tamper-evident content address, and leg 7 should revisit it if the IR ever becomes
a distributed artifact. `lean_string_hash` is also only stable within a Lean
version — harmless here, because §5.4 already puts the Lean version in the cache
key. -/
def hashHex (s : String) : String :=
  let digits := String.ofList (Nat.toDigits 16 (hash s).toNat)
  "".pushn '0' (16 - digits.length) ++ digits

/-- First component of a module name, used as a stand-in for "which package does
this dependency belong to". A heuristic: Lake package membership is not derivable
from the environment alone, and `Mathlib.*` / `Init.*` / `Std.*` / `Batteries.*`
happen to coincide with it on this target. -/
def moduleRoot : Name → Name
  | .str p s => if p.isAnonymous then .str p s else moduleRoot p
  | .num p n => if p.isAnonymous then .num p n else moduleRoot p
  | .anonymous => .anonymous

/-- One declaration as it goes into the IR.

Two differences from `DeclOut.toJson` (the stage-2/3 debug dump, which stays
byte-compatible so the baseline check keeps working): the owning module is the
file's rather than a per-declaration field, and `refs` are deduplicated
`(module, name)` pairs instead of bare names in order of occurrence. Occurrence
order is dropped on purpose — it only means something together with the tagged
text it indexes, which schema 1 does not carry.

Schema 2 (`--tagged-code`) does carry it: every printed fragment gets a flat
pre-order list of `[start, stop, kind]` / `[start, stop, 1, name]` over its own
plain text (`collectSpans`). The plain text stays as it is — redundant with the
spans, but it lets a consumer check one against the other. -/
def spanToJson (s : Span) : Json :=
  if s.kind == 1 then
    Json.arr #[Json.num s.start, Json.num s.stop, Json.num 1, Json.str s.name.toString]
  else
    Json.arr #[Json.num s.start, Json.num s.stop, Json.num s.kind]

def spansToJson (sp : Array Span) : Json := Json.arr (sp.map spanToJson)

/-- Per-kind tally of the spans actually written, for the size report. -/
structure SpanTally where
  total : Nat := 0
  const : Nat := 0
  sort : Nat := 0
  other : Nat := 0
  deriving Inhabited

def SpanTally.add (t : SpanTally) (sp : Array Span) : SpanTally :=
  sp.foldl (init := t) fun t s =>
    match s.kind with
    | 1 => { t with total := t.total + 1, const := t.const + 1 }
    | 2 => { t with total := t.total + 1, sort := t.sort + 1 }
    | _ => { t with total := t.total + 1, other := t.other + 1 }

/-- `index` is the declaration's position in the order the extractor enumerated
its module (`moduleData.constNames`, blacklisted names dropped). Increment 1 found
2 modules / 4 declarations whose `(line, col)` are equal, so position on the page
is not recoverable from the range alone. -/
def declToIrJson (tagged : Bool) (index : Nat) (d : DeclOut) (refs : Array (Name × Name)) : Json :=
  Json.mkObj (
    [ ("name", Json.str d.name.toString),
      ("kind", Json.str d.kind),
      ("binders", Json.arr (d.sig.binders.map Json.str)),
      ("implicits", Json.arr (d.sig.implicits.map (Json.bool ·))),
      ("type", Json.str d.sig.type),
      ("doc", match d.doc with | some s => Json.str s | none => Json.null),
      ("line", Json.num d.line),
      ("col", Json.num d.col),
      ("equations", Json.arr (d.equations.map Json.str)),
      ("members", Json.arr (d.members.map fun m =>
        Json.mkObj (
          [ ("label", Json.str m.label), ("name", Json.str m.name.toString),
            ("text", Json.str m.text) ] ++
          (if tagged then [("code", spansToJson m.spans)] else [])))),
      ("refs", Json.arr (refs.map fun (m, n) =>
        Json.arr #[Json.str m.toString, Json.str n.toString]))
    ] ++
    (if tagged then
      [ ("index", Json.num index),
        ("endLine", Json.num d.endLine),
        ("endCol", Json.num d.endCol),
        ("modifiers", Json.arr (d.modifiers.map Json.str)),
        ("binderCode", Json.arr (d.sig.binderSpans.map spansToJson)),
        ("typeCode", spansToJson d.sig.typeSpans),
        ("equationCode", Json.arr (d.equationSpans.map spansToJson)) ]
     else []))

structure IrStats where
  moduleFiles : Nat := 0
  moduleBytes : Nat := 0
  declarations : Nat := 0
  /-- Deduplicated `(declaration, reference)` pairs actually written. -/
  refPairs : Nat := 0
  /-- References whose defining module the environment could not name. Dropped
  from the IR; expected to be 0 on this target (stage 3 increment 1). -/
  refsUnresolved : Nat := 0
  depFiles : Nat := 0
  depEntries : Nat := 0
  depBytes : Nat := 0
  indexBytes : Nat := 0
  /-- Fragments (binder / result type / equation / member) carrying a span list.
  Zero without `--tagged-code`. -/
  spanFragments : Nat := 0
  /-- Spans written, in total and per kind. -/
  spans : SpanTally := {}
  /-- Building the `Json` and compressing it to a `String`. -/
  serializeNanos : Nat := 0
  /-- `hashHex` over that string. Separated out because §5.5 makes the hash
  load-bearing, so its cost has to be visible rather than folded into "writing". -/
  hashNanos : Nat := 0
  /-- `IO.FS.writeFile`. -/
  writeNanos : Nat := 0
  deriving Inhabited

/--
Writes the whole IR. Everything is derived from data already computed by the
analysis; nothing here consults the environment except `getModuleIdxFor?` for the
defining module of a referenced constant.
-/
def writeIRTree (tagged : Bool) (dir : FilePath) (env : Environment) (targets : Array Name)
    (mods : Array ModuleOut) (results : Array DeclOut) : IO IrStats := do
  let modulesDir := dir / "modules"
  let depsDir := dir / "deps"
  IO.FS.createDirAll modulesDir
  IO.FS.createDirAll depsDir

  -- `EnvironmentHeader.moduleNames` is a `def`, not a field: a fresh
  -- 6,021-element array per call (this cost stage 2 13 s). Hoist it.
  let modNames := env.header.moduleNames
  let targetSet : Std.HashSet Name :=
    Std.HashSet.emptyWithCapacity targets.size |>.insertMany targets

  let mut byModule : Std.HashMap Name (Array DeclOut) :=
    Std.HashMap.emptyWithCapacity targets.size
  for d in results do
    byModule := byModule.insert d.module ((byModule.getD d.module #[]).push d)

  let mut st : IrStats := {}
  let mut indexEntries : Array Json := #[]
  -- Dependency-side map, accumulated while the declarations are walked so the
  -- reference resolution is paid exactly once.
  let mut depMap : Std.HashMap Name Name := Std.HashMap.emptyWithCapacity 1024

  for m in mods do
    let decls := byModule.getD m.name #[]
    let tSer0 ← IO.monoNanosNow
    let mut declJson : Array Json := Array.emptyWithCapacity decls.size
    for hd : i in [0 : decls.size] do
      let d := decls[i]
      let mut seen : Std.HashSet Name := Std.HashSet.emptyWithCapacity d.refs.size
      let mut pairs : Array (Name × Name) := Array.emptyWithCapacity d.refs.size
      for n in d.refs do
        if seen.contains n then continue
        seen := seen.insert n
        match (env.getModuleIdxFor? n).map (modNames[·]!) with
        | some defMod =>
          pairs := pairs.push (defMod, n)
          unless targetSet.contains defMod do
            depMap := depMap.insert n defMod
        | none => st := { st with refsUnresolved := st.refsUnresolved + 1 }
      st := { st with refPairs := st.refPairs + pairs.size }
      if tagged then
        let mut tally := st.spans
        let frags := st.spanFragments + 1 + d.sig.binderSpans.size
                     + d.equationSpans.size + d.members.size
        for sp in d.sig.binderSpans do
          tally := tally.add sp
        tally := tally.add d.sig.typeSpans
        for sp in d.equationSpans do
          tally := tally.add sp
        for mem in d.members do
          tally := tally.add mem.spans
        st := { st with spans := tally, spanFragments := frags }
      declJson := declJson.push (declToIrJson tagged i d pairs)
    let body := Json.mkObj [
      ("schemaVersion", Json.num (irSchemaVersion tagged)),
      ("module", Json.str m.name.toString),
      ("imports", Json.arr (m.imports.map (Json.str ·.toString))),
      ("moduleDocs", Json.arr (m.docs.map fun d =>
        Json.mkObj [("line", Json.num d.line), ("col", Json.num d.col),
                    ("text", Json.str d.text)])),
      ("tactics", Json.arr (m.tactics.map fun t =>
        Json.mkObj [("internalName", Json.str t.internalName.toString),
                    ("userName", Json.str t.userName),
                    ("tags", Json.arr (t.tags.map (Json.str ·.toString))),
                    ("docString", Json.str t.docString)])),
      ("declarations", Json.arr declJson)
    ]
    let text := body.compress
    let bytes := text.utf8ByteSize
    -- The `throw` branch is what keeps the serialisation inside this timer. A
    -- plain `let` is not enough: its only consumers are below the next clock
    -- read, and the compiler sinks it there. Same class of mistake as stage 2's
    -- `--tactics-probe` and stage 3's `refUs`; measured here first as 0 µs for
    -- the hash, which is physically impossible for 8.6 MB.
    if bytes == 0 then throw <| IO.userError s!"empty IR body for {m.name}"
    let tSer1 ← IO.monoNanosNow
    let h := hashHex text
    if h.length != 16 then throw <| IO.userError s!"bad digest width for {m.name}"
    let tHash ← IO.monoNanosNow
    let file := s!"modules/{m.name}.json"
    IO.FS.writeFile (dir / file) text
    let tWrite ← IO.monoNanosNow
    st := { st with
      moduleFiles := st.moduleFiles + 1
      moduleBytes := st.moduleBytes + bytes
      declarations := st.declarations + decls.size
      serializeNanos := st.serializeNanos + (tSer1 - tSer0)
      hashNanos := st.hashNanos + (tHash - tSer1)
      writeNanos := st.writeNanos + (tWrite - tHash) }
    indexEntries := indexEntries.push <| Json.mkObj [
      ("module", Json.str m.name.toString),
      ("file", Json.str file),
      ("bytes", Json.num bytes),
      ("declarations", Json.num decls.size),
      ("contentHash", Json.str h)]

  -- Dependency-side map slice, one file per package (§5.3: two columns, name ->
  -- module; `kind` is only needed by a search UI).
  let mut byRoot : Std.HashMap Name (Array (Name × Name)) := {}
  for (n, defMod) in depMap do
    let r := moduleRoot defMod
    byRoot := byRoot.insert r ((byRoot.getD r #[]).push (n, defMod))
  let mut depEntriesJson : Array Json := #[]
  for (r, entries) in byRoot.toArray.qsort (fun a b => a.1.toString < b.1.toString) do
    let tSer0 ← IO.monoNanosNow
    let body := Json.mkObj [
      ("schemaVersion", Json.num (irSchemaVersion tagged)),
      ("package", Json.str r.toString),
      ("declarations", Json.mkObj
        (entries.toList.map fun (n, defMod) => (n.toString, Json.str defMod.toString)))
    ]
    let text := body.compress
    let bytes := text.utf8ByteSize
    if bytes == 0 then throw <| IO.userError s!"empty dependency map for {r}"
    let tSer1 ← IO.monoNanosNow
    let file := s!"deps/{r}.json"
    IO.FS.writeFile (dir / file) text
    let tWrite ← IO.monoNanosNow
    st := { st with
      depFiles := st.depFiles + 1
      depEntries := st.depEntries + entries.size
      depBytes := st.depBytes + bytes
      serializeNanos := st.serializeNanos + (tSer1 - tSer0)
      writeNanos := st.writeNanos + (tWrite - tSer1) }
    depEntriesJson := depEntriesJson.push <| Json.mkObj [
      ("package", Json.str r.toString), ("file", Json.str file),
      ("entries", Json.num entries.size), ("bytes", Json.num bytes)]

  -- Package index. §5.4 wants Lean version, extractor version and schema version
  -- in the cache key; the olean hash is the piece still missing (leg 7).
  let tIdx0 ← IO.monoNanosNow
  let index := Json.mkObj [
    ("schemaVersion", Json.num (irSchemaVersion tagged)),
    ("generator", Json.str "lean-doc/experiments/stage4b"),
    ("leanVersion", Json.str Lean.versionString),
    ("hashAlgorithm", Json.str "lean-string-hash-64/hex16"),
    ("moduleCount", Json.num st.moduleFiles),
    ("declarationCount", Json.num st.declarations),
    ("modules", Json.arr indexEntries),
    ("dependencyMaps", Json.arr depEntriesJson)]
  let text := index.compress
  let bytes := text.utf8ByteSize
  if bytes == 0 then throw <| IO.userError "empty index"
  let tIdx1 ← IO.monoNanosNow
  IO.FS.writeFile (dir / "index.json") text
  let tIdx2 ← IO.monoNanosNow
  return { st with
    indexBytes := bytes
    serializeNanos := st.serializeNanos + (tIdx1 - tIdx0)
    writeNanos := st.writeNanos + (tIdx2 - tIdx1) }

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
  sink.emit "stage4b.initSearchPath" (tSp1 - tSp0)

  let tImp0 ← IO.monoNanosNow
  unsafe Lean.enableInitializersExecution
  let env ← importModules (targets.map (Import.mk · false true false)) Options.empty
    (leakEnv := true) (loadExts := true)
  let tImp1 ← IO.monoNanosNow
  sink.emit "stage4b.importModules" (tImp1 - tImp0) [("directImports", toString targets.size)]

  let header := env.header
  sink.emit "stage4b.envStats" 0 [("loadedModules", toString header.moduleNames.size)]

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
      let (_, st) ← act.toIO { fileName := "<lean-doc/stage4b>", fileMap := default }
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
  sink.emit "stage4b.indexLookup" (tIdx1 - tIdx0)
    [("targetModules", toString targets.size),
     ("enumerated", toString enumerated),
     ("candidates", toString candidates.size)]

  -- Options and heartbeat budget copied from doc-gen4 (`DocGen4/Load.lean` for the
  -- options, `Process/Analyze.lean` for the per-constant `maxHeartbeats`).
  -- `pp.funBinderTypes` in particular changes the printed text (`fun (n : ℕ) =>`
  -- instead of `fun n =>`), so without it the two tools are not printing the same
  -- thing and the times are not comparable either.
  let coreCtx : Core.Context := {
    fileName := "<lean-doc/stage4b>"
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
  sink.emit "stage4b.moduleDocs" (tMd1 - tMd0)
    [("modules", toString mods.size), ("moduleDocs", toString modDocCount),
     ("modulesWithDocs", toString modsWithDocs), ("imports", toString importCount)]

  let tTac0 ← IO.monoNanosNow
  let (mods, tacticsInEnv, tacticsAssigned) ← runMeta (collectTacticsOnce mods)
  let tTac1 ← IO.monoNanosNow
  sink.emit "stage4b.tactics" (tTac1 - tTac0)
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
    sink.emit "stage4b.tacticsPerModule" tEmu
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
    sink.emit "stage4b.tacticsProbe" (t1 - t0)
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
      eqFailures := counters.eqFailures + c.eqFailures,
      refNanos := counters.refNanos + c.refNanos,
      refCount := counters.refCount + c.refCount }
    match outcome with
    | .ok none => blacklisted := blacklisted + 1
    | .ok (some d) => results := results.push d
    | .error msg => failures := failures.push ⟨name, msg⟩
  let tAn1 ← IO.monoNanosNow
  sink.emit "stage4b.analyze" (tAn1 - tAn0)
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
     ("tagCode", if cfg.tagCode then "true" else "false"),
     -- `refUs` is contained in `ppUs` + `eqUs`, it is not an extra term.
     ("refUs", toString (counters.refNanos / 1000)),
     ("refOccurrences", toString counters.refCount),
     ("collectRefs", if cfg.collectRefs then "true" else "false"),
     ("taggedCode", if cfg.taggedCode then "true" else "false")]

  -- IR persistence. This is the phase `approach.md` §6.1 carried as a 仮定.
  let mut irStats : IrStats := {}
  let mut irDirUsed : Option FilePath := none
  if cfg.writeIR then
    let dir ← resolveIrDir cfg
    irDirUsed := some dir
    let t0 ← IO.monoNanosNow
    irStats ← writeIRTree cfg.taggedCode dir env targets mods results
    let t1 ← IO.monoNanosNow
    sink.emit "stage4b.writeIR" (t1 - t0)
      [("taggedCode", if cfg.taggedCode then "true" else "false"),
       ("schemaVersion", toString (irSchemaVersion cfg.taggedCode)),
       ("spanFragments", toString irStats.spanFragments),
       ("spans", toString irStats.spans.total),
       ("spansConst", toString irStats.spans.const),
       ("spansSort", toString irStats.spans.sort),
       ("spansOther", toString irStats.spans.other),
       ("moduleFiles", toString irStats.moduleFiles),
       ("moduleBytes", toString irStats.moduleBytes),
       ("declarations", toString irStats.declarations),
       ("refPairs", toString irStats.refPairs),
       ("refsUnresolved", toString irStats.refsUnresolved),
       ("depFiles", toString irStats.depFiles),
       ("depEntries", toString irStats.depEntries),
       ("depBytes", toString irStats.depBytes),
       ("indexBytes", toString irStats.indexBytes),
       ("serializeUs", toString (irStats.serializeNanos / 1000)),
       ("hashUs", toString (irStats.hashNanos / 1000)),
       ("writeUs", toString (irStats.writeNanos / 1000))]

  if let some dumpPath := cfg.dumpPath then
    let tD0 ← IO.monoNanosNow
    let h ← IO.FS.Handle.mk dumpPath .write
    for d in results do
      h.putStr d.toJson.compress
      h.putStr "\n"
    let tD1 ← IO.monoNanosNow
    sink.emit "stage4b.dump" (tD1 - tD0) [("records", toString results.size)]

  if let some dumpPath := cfg.dumpModulesPath then
    let tD0 ← IO.monoNanosNow
    let h ← IO.FS.Handle.mk dumpPath .write
    for m in mods do
      h.putStr m.toJson.compress
      h.putStr "\n"
    let tD1 ← IO.monoNanosNow
    sink.emit "stage4b.dumpModules" (tD1 - tD0) [("records", toString mods.size)]

  -- The unique set of referenced constants: the demand side of the link map
  -- (`docs/plans/stage4.md` §3, increment 1). One line per constant, in order of
  -- first appearance so that two runs diff cleanly.
  let mut refUnique := 0
  let mut refOwn := 0
  let mut refUnresolved := 0
  if let some dumpPath := cfg.dumpRefsPath then
    if !cfg.collectRefs then
      IO.eprintln "WARNING: --dump-refs without --refs; nothing was collected"
    let tD0 ← IO.monoNanosNow
    -- `EnvironmentHeader.moduleNames` is a `def`, not a field: it allocates a fresh
    -- 6,021-element array per call (this cost 13 s in stage 2). Hoist it.
    let modNames := header.moduleNames
    let targetSet : Std.HashSet Name :=
      Std.HashSet.emptyWithCapacity targets.size |>.insertMany targets
    let mut counts : Std.HashMap Name Nat := Std.HashMap.emptyWithCapacity 4096
    let mut order : Array Name := #[]
    let mut occurrences := 0
    for d in results do
      for n in d.refs do
        occurrences := occurrences + 1
        match counts[n]? with
        | some k => counts := counts.insert n (k + 1)
        | none => counts := counts.insert n 1; order := order.push n
    let h ← IO.FS.Handle.mk dumpPath .write
    for n in order do
      let module? := (env.getModuleIdxFor? n).map (modNames[·]!)
      let own := match module? with
        | some m => targetSet.contains m
        | none => false
      if own then refOwn := refOwn + 1
      if module?.isNone then refUnresolved := refUnresolved + 1
      h.putStr (Json.mkObj [
        ("name", Json.str n.toString),
        ("module", match module? with | some m => Json.str m.toString | none => Json.null),
        ("occurrences", Json.num (counts.getD n 0)),
        ("own", Json.bool own)] |>.compress)
      h.putStr "\n"
    refUnique := order.size
    let tD1 ← IO.monoNanosNow
    sink.emit "stage4b.dumpRefs" (tD1 - tD0)
      [("records", toString refUnique),
       ("occurrences", toString occurrences),
       ("own", toString refOwn),
       ("unresolved", toString refUnresolved)]

  let tTotal1 ← IO.monoNanosNow
  sink.emit "stage4b.total" (tTotal1 - tTotal0) [("modules", toString targets.size)]

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
  if cfg.collectRefs then
    IO.println s!"  of which refs      {fmtDur counters.refNanos}  ({counters.refCount} occurrences; inside the two above)"
    if cfg.dumpRefsPath.isSome then
      IO.println s!"refs                 {refUnique} unique, {refOwn} in target modules, {refUnique - refOwn - refUnresolved} in dependencies, {refUnresolved} without a module"
  if let some dir := irDirUsed then
    let total := irStats.moduleBytes + irStats.depBytes + irStats.indexBytes
    IO.println s!"writeIR              {fmtDur irStats.serializeNanos} serialize + {fmtDur irStats.hashNanos} hash + {fmtDur irStats.writeNanos} write"
    IO.println s!"  files              {irStats.moduleFiles} modules + {irStats.depFiles} dep maps + 1 index"
    IO.println s!"  bytes              {total} ({irStats.moduleBytes} modules, {irStats.depBytes} deps, {irStats.indexBytes} index)"
    IO.println s!"  content            {irStats.declarations} declarations, {irStats.refPairs} ref pairs, {irStats.depEntries} dep map entries, {irStats.refsUnresolved} unresolved refs"
    IO.println s!"  schema             {irSchemaVersion cfg.taggedCode}  (taggedCode {cfg.taggedCode})"
    if cfg.taggedCode then
      IO.println s!"  spans              {irStats.spans.total} in {irStats.spanFragments} fragments — {irStats.spans.const} const, {irStats.spans.sort} sort, {irStats.spans.other} other"
    IO.println s!"  dir                {dir}"
  IO.println s!"total                {fmtDur (tTotal1 - tTotal0)}"
  IO.println s!"genEquations         {cfg.genEquations}   tagCode {cfg.tagCode}   refs {cfg.collectRefs}   open {cfg.openNamespaces.toList}"
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

end Stage4b

open Stage4b in
def parseArgs (args : List String) : Except String Cfg :=
  match args with
  | modules :: out :: rest => go { modulesPath := ⟨modules⟩, outPath := ⟨out⟩ } rest
  | _ => .error "usage: extract <modules.txt> <out.jsonl> [--equations] [--dump <p>] [--dump-modules <p>] [--only <p>] [--open <ns,..>] [--tag] [--refs] [--dump-refs <p>] [--write-ir] [--ir-dir <p>] [--tagged-code] [--skip-analyze] [--tactics-emulate] [--tactics-probe]"
where
  go (cfg : Cfg) : List String → Except String Cfg
  | [] => .ok cfg
  | "--equations" :: rest => go { cfg with genEquations := true } rest
  | "--write-ir" :: rest => go { cfg with writeIR := true } rest
  | "--tagged-code" :: rest => go { cfg with taggedCode := true } rest
  -- Deliberately does *not* imply `--write-ir`: "the IR is off unless --write-ir"
  -- is the one rule that keeps the stage-3 baseline reproducible from this tree.
  | "--ir-dir" :: p :: rest => go { cfg with irDir := some ⟨p⟩ } rest
  | "--tag" :: rest => go { cfg with tagCode := true } rest
  | "--refs" :: rest => go { cfg with collectRefs := true } rest
  | "--dump-refs" :: p :: rest => go { cfg with dumpRefsPath := some ⟨p⟩ } rest
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
  | .ok cfg => Stage4b.run cfg
  | .error msg => IO.eprintln msg; return 1
