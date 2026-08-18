/-
# lean-doc as a Lake package

A consumer requires this package and gets a `docs` script:

```lean
require «lean-doc» from git "https://github.com/FujiHaruka/lean-doc" @ "main"
```

A tag works too, but **only a tag whose tree contains this file** — `v0.1.3` and
earlier have no `lakefile.lean`, so Lake cannot resolve them as a package. Pin to
a released tag once one carries it; until then `main` is the honest spelling.

```
lake run docs -- --out ../mypkg-docs
```

Two things that are otherwise the consumer's problem are structurally solved by
being a dependency rather than a checkout:

* the **extractor** (`extractor/Extract.lean`, 171 MB when built) is built by
  Lake against the *root* package's toolchain, so it cannot be built against the
  wrong Lean and nobody has to pass `--extractor-bin`;
* **`--lib`** is read out of the elaborated workspace.
  `crates/lean-doc/src/lakefile.rs` refuses a `lakefile.lean` by name — reading
  one honestly means elaborating it with Lake — and this script *is* that
  elaboration, so a `lakefile.lean` package needs no `--lib` here. Mathlib and
  doc-gen4 are both `lakefile.lean` packages.

The Rust half (`lean-doc`) is *not* built by Lake: see `resolveLeanDoc`.

## There is deliberately no `lean-toolchain` next to this file

**Not an oversight. Do not add one** 【実測 2026-08-18,
`benchmarks/results/lake-package-probe-2026-08-18.txt` §1】.

`lake update` in the *consumer* compares every dependency's `lean-toolchain`
against the root's and **rewrites the root's** when a dependency names a higher
version — and it does so *before* elan tries to fetch that version, so the
consumer's `lean-toolchain` is rewritten whether or not the version exists. A
dependency naming a *lower* version is ignored with **no warning at all**, which
means a stale toolchain here would be invisible to everyone. With no file at
all, `Workspace.updateToolchain` skips lean-doc entirely (`ToolchainVer.ofDir?`
returns `none`) and the root's toolchain builds the extractor — which is exactly
what is wanted, and what `docs/approach.md` already required for other reasons.

The price is that `lake` cannot run in *this* directory (elan has no toolchain
to pick here), so the `lake-manifest.json` beside this file is written by hand
and this package is only ever built from a consumer's workspace.
-/
import Lake
open Lake DSL

open System (FilePath)

package «lean-doc» where
  -- The Lean sources are all in `extractor/`; the rest of the tree is Rust,
  -- shell and docs.
  srcDir := "extractor"

/--
The extractor. `supportInterpreter := true` is how Lake spells the `-rdynamic`
that `extractor/build.sh` passes to `leanc`: `importModules (loadExts := true)`
runs module initializers through the Lean interpreter, which resolves symbols in
the running executable.

The two builds do **not** produce the same bytes (Lake adds a package symbol
prefix and compiles the generated C with `-O3 -DNDEBUG`; +308,032 B 【実測】)
but they write **byte-identical IR**, which is the property that matters and the
one `tools/lake-package-gate.sh` item 4 re-checks on every run.
-/
lean_exe extract where
  root := `Extract
  supportInterpreter := true

/-! ## The `docs` script -/

/-- What `lake run docs` accepts. Everything else `lean-doc build` offers is
deliberately not plumbed through: this script's job is to fill in the three
flags a consumer cannot know (`--root`, `--lib`, `--extractor-bin`). -/
structure DocsOptions where
  out : Option String := none
  jobs : Option String := none
  sourceUrl : Option String := none
  help : Bool := false
  deriving Inhabited

def docsUsage : String :=
"usage: lake run docs -- --out <dir> [--jobs <n>] [--source-url <url>]

  --out         where the documentation goes. Required, with no default: `lean-doc
                build` refuses an --out inside the package it documents, so no
                path inside this workspace would be right. A relative path is
                resolved against the package directory — `lake` only runs where
                the lakefile is — so `--out docs` is refused and `--out ../docs`
                is the shortest spelling that works.
  --jobs        extractor threads (default 1)
  --source-url  https://host/owner/repo/blob/<40-hex-rev>; read from the
                package's git HEAD when left out

  LEAN_DOC_BIN  the `lean-doc` executable to run (see `resolveLeanDoc`)"

/-- Parse the script's arguments. Structural recursion, and an unknown flag is an
error rather than something skipped — a `docs` run that quietly ignored
`--source-url` would produce a site full of links to the wrong revision. -/
def parseDocsArgs : List String → DocsOptions → Except String DocsOptions
  | [], acc => .ok acc
  | "--help" :: _, acc => .ok {acc with help := true}
  | "-h" :: _, acc => .ok {acc with help := true}
  | "--out" :: value :: rest, acc => parseDocsArgs rest {acc with out := some value}
  | "--jobs" :: value :: rest, acc => parseDocsArgs rest {acc with jobs := some value}
  | "--source-url" :: value :: rest, acc => parseDocsArgs rest {acc with sourceUrl := some value}
  -- Spelled out one by one rather than as a catch-all `[flag]`: a single
  -- trailing `--nope` matches any one-element pattern, and "--nope needs a
  -- value" sends the reader looking for a value it never had.
  | ["--out"], _ => .error "--out needs a value"
  | ["--jobs"], _ => .error "--jobs needs a value"
  | ["--source-url"], _ => .error "--source-url needs a value"
  | flag :: _, _ => .error s!"unknown argument `{flag}`"

/-- Exists and is not a directory. Lean core exposes no permission bits, so this
is as far as "is it runnable" can be taken here; spawning it is what finds out. -/
def isFileAt (path : FilePath) : IO Bool := do
  if ← path.pathExists then return !(← path.isDir) else return false

/-- The first `exe` on `PATH`, or nothing. -/
def findOnPath (exe : String) : IO (Option FilePath) := do
  let some raw ← IO.getEnv "PATH" | return none
  for dir in System.SearchPath.parse raw do
    let candidate := dir / exe
    if ← isFileAt candidate then return some candidate
  return none

/--
Which `lean-doc` (the Rust half) this script runs, and in what order it is looked
for. **One function on purpose**: L2 fills in sources 2 and 3 and nothing else in
the tree gets an opinion about where the binary comes from (CLAUDE.md 「判断は 1
箇所に集める」).

| | source | |
|---:|---|---|
| 1 | `$LEAN_DOC_BIN` | here |
| 2 | version-pinned cache | L2 |
| 3 | GitHub Release | L2 |
| 4 | `lean-doc` on `PATH` | here, with its version printed |
| 5 | `cargo build` in this package | here |
| 6 | an error naming every source above | here |

`PATH` sits **below** the download on purpose: whatever answers to that name may
write an IR schema older than this checkout's renderer reads. Until L2 fills in
2 and 3, `PATH` is the source that usually answers — but the *order* is already
the final one, because slotting a step in later is how one decision ends up
living in two places.

`$LEAN_DOC_BIN` set to something that is not a file is an **error, not a
fallthrough**: a caller who named a binary and silently got a different one
would never find out.
-/
def resolveLeanDoc (pkgDir : FilePath) : IO (Except String FilePath) := do
  let mut tried : Array String := #[]

  -- 1. $LEAN_DOC_BIN.
  match ← IO.getEnv "LEAN_DOC_BIN" with
  | some raw =>
    if raw.isEmpty then
      -- `LEAN_DOC_BIN=` in a wrapper script is how a shell spells "I did not set
      -- this" (`crates/lean-doc/src/extract.rs` `or_env` reads it the same way).
      tried := tried.push "$LEAN_DOC_BIN: set but empty"
    else
      let bin : FilePath := raw
      if ← isFileAt bin then
        return .ok bin
      else
        return .error s!"$LEAN_DOC_BIN is {raw}, which is not a file"
  | none => tried := tried.push "$LEAN_DOC_BIN: unset"

  -- 2. A version-pinned cache under $XDG_CACHE_HOME/lean-doc/v<version>/<target>.
  --    L2 goes here (docs/plans/lake-package.md §5, L2-d).
  -- 3. The GitHub Release for the version in this tree's Cargo.toml, checksum
  --    verified. L2 goes here too (§5, L2-a..c). Both are deliberately left
  --    empty rather than absent: the order below is written against the final
  --    list, not against L1's.

  -- 4. PATH.
  match ← findOnPath "lean-doc" with
  | some bin =>
    let probe ← IO.Process.output {cmd := bin.toString, args := #["--version"]}
    let version := probe.stdout.trimAscii.toString
    IO.println s!"lean-doc: {bin} ({if version.isEmpty then "no --version" else version})"
    IO.println "lean-doc: warning: that is whatever is on PATH. Nothing here checks that its \
      IR schema matches this checkout's — set LEAN_DOC_BIN to pin one."
    return .ok bin
  | none => tried := tried.push "PATH: no `lean-doc`"

  -- 5. Build it from this package's own source, if this is a checkout with cargo
  --    available. Slow (a release build), and the last thing tried, but it is
  --    the only source that cannot be out of step with this tree.
  let manifest := pkgDir / "Cargo.toml"
  if ← isFileAt manifest then
    match ← findOnPath "cargo" with
    | some cargo =>
      IO.println s!"lean-doc: not found; building it from {manifest}"
      let child ← IO.Process.spawn {
        cmd := cargo.toString
        args := #["build", "--release", "--bin", "lean-doc"]
        cwd := some pkgDir
      }
      let code ← child.wait
      let built := pkgDir / "target" / "release" / "lean-doc"
      if code == 0 && (← isFileAt built) then
        return .ok built
      tried := tried.push s!"cargo build --release --bin lean-doc in {pkgDir}: exited {code}"
    | none => tried := tried.push "PATH: no `cargo` to build it with"
  else
    tried := tried.push s!"no {manifest} to build from"

  -- 6. Say what was looked for and where.
  return .error <|
    "no `lean-doc` executable (the Rust half of lean-doc). Looked, in order:\n"
      ++ String.intercalate "\n" (tried.toList.map ("  - " ++ ·))
      ++ "\n\nSet LEAN_DOC_BIN to one, or put `lean-doc` on PATH. README.md \
          §Running it locally has both the release download and the cargo build."

/--
Generate this package's documentation.

Fills in the three flags of `lean-doc build` a consumer cannot supply by hand —
`--root`, `--lib` (out of the elaborated workspace) and `--extractor-bin` (the
executable Lake just built) — and passes the rest through.
-/
script docs (args) do
  -- `lake run docs -- --out X` hands the `--` to the script as an argument;
  -- Lake does not strip it 【実測 2026-08-18, probe §4】. Both spellings have to
  -- work, so the leading `--` is dropped here.
  let args := match args with
    | "--" :: rest => rest
    | other => other
  let opts ← match parseDocsArgs args {} with
    | .ok opts => pure opts
    | .error message =>
      IO.eprintln s!"lake run docs: {message}"
      IO.eprintln docsUsage
      return 2
  if opts.help then
    IO.println docsUsage
    return 0
  let some outRaw := opts.out
    | IO.eprintln "lake run docs: --out <dir> is required and has no default: `lean-doc build` \
        refuses an --out inside the package it documents (crates/lean-doc/src/build.rs), so no \
        path inside this workspace would be right."
      IO.eprintln docsUsage
      return 2

  let ws ← getWorkspace
  let root := ws.root

  -- `--lib` names a *library root* (`<Name>.lean` and `<Name>/`), which is
  -- `LeanLib.roots` rather than the library's name: they differ whenever a
  -- lakefile sets `roots` explicitly. Every library of the root package is
  -- documented — `defaultTargets` answers a different question (what `lake
  -- build` builds with no arguments, which may name executables), and
  -- `crates/lean-doc/src/lakefile.rs` already made the same call for the TOML
  -- path.
  let libs := root.leanLibs.foldl (fun acc lib => acc ++ lib.roots) #[]
  if libs.isEmpty then
    IO.eprintln s!"lake run docs: {root.prettyName} declares no `lean_lib`, so there is nothing to \
      document. This script fills in `lean-doc build --lib` from the workspace, and the \
      workspace has no library root to name."
    return 3

  -- The package being documented has to be built first. `lake exe` and the
  -- `runBuild` below build the *extractor* and nothing else, and an extractor
  -- run against an unbuilt package dies with "No directory 'Micro' or file
  -- 'Micro.olean' in the search path" 【実測 2026-08-18, probe §3】.
  for lib in root.leanLibs do
    let _ ← runBuild lib.fetch

  -- D1: build the extractor **without running it**. `Lake.exe` does both
  -- (`Lake/CLI/Actions.lean:23-29`: `runBuild exe.fetch` and then `env`), so
  -- this is its first half with the `env` call dropped. `Workspace.runBuild` was
  -- the plan's first candidate and it works, so the fallback — `lake build
  -- lean-doc/extract` as a subprocess — is not used: a subprocess would re-read
  -- the workspace this script is already holding, and would report failures as a
  -- shell exit code instead of as Lake's own build log.
  let extractBin ← runBuild extract.fetch

  let leanDoc ← match ← resolveLeanDoc __dir__ with
    | .ok bin => pure bin
    | .error message =>
      IO.eprintln s!"lake run docs: {message}"
      return 4

  -- Resolved here rather than handed over relative: `lean-doc build` refuses an
  -- `--out` under `--root`, and it resolves a relative path against *its own*
  -- working directory. Printing the absolute path below is what makes a
  -- surprising answer visible.
  let cwd ← IO.currentDir
  let outDir : FilePath := if (FilePath.mk outRaw).isAbsolute then outRaw else cwd / outRaw

  -- The toolchain's own `lake`, not the name `lake` on PATH: this is the one the
  -- workspace was loaded with, and `lean-doc` looks for the `lean` that answers
  -- `--githash` as its **sibling**.
  let lake := (← getLakeEnv).lake.lake

  let mut cmdArgs := #[
    "build",
    "--root", root.dir.toString,
    "--out", outDir.toString,
    "--extractor-bin", extractBin.toString,
    "--lake", lake.toString]
  for lib in libs do
    cmdArgs := cmdArgs ++ #["--lib", lib.toString]
  if let some jobs := opts.jobs then
    cmdArgs := cmdArgs ++ #["--jobs", jobs]
  if let some url := opts.sourceUrl then
    cmdArgs := cmdArgs ++ #["--source-url", url]

  IO.println s!"lean-doc: {leanDoc} {String.intercalate " " cmdArgs.toList}"
  -- No augmented environment: `lake run` does not put `LEAN_PATH` in the
  -- script's environment 【実測, probe §4】 and `lean-doc build` does not want
  -- one — it runs `lake env` inside `--root` itself for every extraction, which
  -- is where the Lean environment comes from (`crates/lean-doc/src/extract.rs`).
  -- That is what `--lake` above is for.
  let child ← IO.Process.spawn {cmd := leanDoc.toString, args := cmdArgs}
  child.wait
