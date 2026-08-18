/-
# litedoc4 as a Lake package

A consumer requires this package and gets a `docs` script:

```lean
require «litedoc4» from git "https://github.com/FujiHaruka/litedoc4" @ "main"
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
  `crates/litedoc4/src/lakefile.rs` refuses a `lakefile.lean` by name — reading
  one honestly means elaborating it with Lake — and this script *is* that
  elaboration, so a `lakefile.lean` package needs no `--lib` here. Mathlib and
  doc-gen4 are both `lakefile.lean` packages.

The Rust half (`litedoc4`) is *not* built by Lake: see `resolveLitedoc4`.

## There is deliberately no `lean-toolchain` next to this file

**Not an oversight. Do not add one** 【実測 2026-08-18,
`benchmarks/results/lake-package-probe-2026-08-18.txt` §1】.

`lake update` in the *consumer* compares every dependency's `lean-toolchain`
against the root's and **rewrites the root's** when a dependency names a higher
version — and it does so *before* elan tries to fetch that version, so the
consumer's `lean-toolchain` is rewritten whether or not the version exists. A
dependency naming a *lower* version is ignored with **no warning at all**, which
means a stale toolchain here would be invisible to everyone. With no file at
all, `Workspace.updateToolchain` skips litedoc4 entirely (`ToolchainVer.ofDir?`
returns `none`) and the root's toolchain builds the extractor — which is exactly
what is wanted, and what `docs/approach.md` already required for other reasons.

The price is that `lake` cannot run in *this* directory (elan has no toolchain
to pick here), so the `lake-manifest.json` beside this file is written by hand
and this package is only ever built from a consumer's workspace.
-/
import Lake
open Lake DSL

open System (FilePath)

package «litedoc4» where
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

/-- What `lake run docs` accepts. Everything else `litedoc4 build` offers is
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

  --out         where the documentation goes. Required, with no default: `litedoc4
                build` refuses an --out inside the package it documents, so no
                path inside this workspace would be right. A relative path is
                resolved against the package directory — `lake` only runs where
                the lakefile is — so `--out docs` is refused and `--out ../docs`
                is the shortest spelling that works.
  --jobs        extractor threads (default 1)
  --source-url  https://host/owner/repo/blob/<40-hex-rev>; read from the
                package's git HEAD when left out

  LITEDOC4_BIN          the `litedoc4` executable to run. Set it and nothing else
                        below is consulted (see `resolveLitedoc4` for the whole order)
  LITEDOC4_NO_DOWNLOAD  set to anything non-empty to never fetch a release. An
                        already-downloaded one is still used
  XDG_CACHE_HOME        where a fetched release is kept
                        (default ~/.cache; litedoc4/v<version>/<target>/litedoc4)"

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

/-- An environment variable that is "set" only when it is set to something.
`FOO=` in a wrapper script is how a shell spells "I did not set this", and
`resolveLitedoc4` already reads `LITEDOC4_BIN` that way. -/
def envIsSet (name : String) : IO Bool := do
  match ← IO.getEnv name with
  | some raw => return !raw.isEmpty
  | none => return false

/-! ## Sources 2 and 3 — the version-pinned cache and the GitHub Release

Everything in this section exists to make one sentence true: **a binary this
script downloads is never run unless its SHA-256 matched the `checksums.txt`
published beside it.** Every failure to verify — no `curl`, no `sha256sum`, a
`checksums.txt` that does not name the asset, a digest that does not match — is
an error that leaves the cache empty and falls through to `PATH` and `cargo`.
None of them is a warning.
-/

/--
The target triples a release actually carries 【実測 2026-08-18】.

Only these two, and not by accident: `.github/workflows/release.yml` builds no
`x86_64-apple-darwin` (there is no Intel runner to *test* one on) and no
`aarch64-unknown-linux-*` (no arm Linux runner), because shipping a binary
nobody has executed is exactly what that workflow's smoke job exists to refuse
(`docs/plans/distribution.md` D2). So **"this machine has no asset" is a normal
path, not a fault** — source 3 says which target it wanted and falls through.
-/
def releaseTargets : List String :=
  ["x86_64-unknown-linux-musl", "aarch64-apple-darwin"]

/-- Where a release's assets live. The asset names carry no version — the
version is in the directory the archive unpacks to (`release.yml`, "Work out
the version"). -/
def releaseBaseUrl (version : String) : String :=
  s!"https://github.com/FujiHaruka/litedoc4/releases/download/v{version}"

/--
This machine's target triple, spelled the way `release.yml` spells it.

**D3 (plan §6) is answered by `System.Platform`, not by `uname -m`.** Lean does
carry the architecture: `System.Platform.target` is the LLVM triple Lean was
compiled for, and reads `arm64-apple-darwin24.6.0` here 【実測 2026-08-18】, so
the architecture is its first field. `uname -m` was measured beside it (`arm64`
— the same answer) and is deliberately *not* used: it costs a subprocess, and a
subprocess is one more way for this to fail on a machine where nothing else is
wrong.

The triple is rebuilt rather than passed through, because neither end of it
matches an asset name: `arm64` has to become `aarch64`, the `darwin24.6.0`
suffix has to go, and on Linux the asset is **musl** where Lean says `-gnu`.
Asking for the musl asset on a glibc machine is right, not a bug — it is
statically linked and `release.yml` asserts that with `ldd` on every build.

`System.Platform.target` is documented as possibly empty (Lean compiled without
it). Then the architecture comes out empty, the triple matches nothing in
`releaseTargets`, and source 3 falls through saying so — the same already
designed path as an Intel Mac.
-/
def hostTarget : IO String := do
  -- **Test-only, and the only reason it exists**: `tools/lake-download-gate.sh`
  -- item 5 has to reach the "no asset for this machine" branch *from* a machine
  -- that has one. Without it that branch is code only Intel-Mac and arm-Linux
  -- users ever run, i.e. an untested branch in the path that decides what gets
  -- executed. It is not a supported knob and nothing in this file weakens
  -- because of it: it chooses which asset is looked for, and the release table,
  -- the download and the checksum all still run unchanged afterwards.
  match ← IO.getEnv "LITEDOC4_TARGET_OVERRIDE" with
  | some raw => if !raw.isEmpty then return raw
  | none => pure ()
  let arch := (System.Platform.target.splitOn "-").headD ""
  let arch := if arch == "arm64" then "aarch64" else arch
  if System.Platform.isWindows then return s!"{arch}-pc-windows-msvc"
  if System.Platform.isOSX then return s!"{arch}-apple-darwin"
  return s!"{arch}-unknown-linux-musl"

/--
The version in *this tree's* `Cargo.toml`, which is what the release is looked
up by.

**This is the general form of what `action.yml` already paid for**: take the
version from the tree that was required, never from the name of the ref. A
consumer on `@main` gets main's version, and if no release carries it source 3
fails and falls through — rather than silently installing an older binary whose
IR schema this checkout's extractor no longer writes.

Section-aware rather than "the first line starting with `version`", because
`[workspace.dependencies]` is full of `version = "1"`.
-/
def cargoWorkspaceVersion (manifest : FilePath) : IO (Option String) := do
  unless ← isFileAt manifest do return none
  let mut inWorkspacePackage := false
  for raw in (← IO.FS.readFile manifest).splitOn "\n" do
    let line := raw.trimAscii.toString
    if line.startsWith "[" then
      inWorkspacePackage := line == "[workspace.package]"
    else if inWorkspacePackage && line.startsWith "version" then
      match line.splitOn "\"" with
      | _ :: value :: _ => return some value
      | _ => return none
  return none

/--
Where the downloaded binary is kept: `$XDG_CACHE_HOME/litedoc4`, else
`~/.cache/litedoc4`, else nowhere.

**Not under `.lake/`**: `lake update` removes that directory, so a cache there
would be re-downloaded whenever a consumer updated any dependency. The caller
adds `v<version>/<target>/` under this, so two checkouts at different versions
— or one machine that has run under two targets — never contend for one path.
-/
def cacheRoot : IO (Option FilePath) := do
  match ← IO.getEnv "XDG_CACHE_HOME" with
  | some raw => if !raw.isEmpty then return some (FilePath.mk raw / "litedoc4")
  | none => pure ()
  match ← IO.getEnv "HOME" with
  | some home => if !home.isEmpty then return some (FilePath.mk home / ".cache" / "litedoc4")
  | none => pure ()
  return none

/--
The SHA-256 that `checksums.txt` publishes for `asset`, if it names it at all.

The file is `sha256sum`'s own output (`release.yml`: `sha256sum *.tar.gz >
checksums.txt`), i.e. `<64 hex><two spaces><name>`. The separator is read as
"a run of spaces" and a leading `*` (sha256sum's binary-mode marker) is dropped,
because a checksum file that is *almost* parsed is how verification quietly
becomes no verification.
-/
def checksumFor (text : String) (asset : String) : Option String := Id.run do
  for raw in text.splitOn "\n" do
    match (raw.trimAscii.toString.splitOn " ").filter (!·.isEmpty) with
    | [digest, name] =>
      let name := if name.startsWith "*" then name.drop 1 else name
      if name == asset then return some digest
    | _ => pure ()
  return none

/--
SHA-256 of a file, via whichever of the two standard tools is on `PATH`
(`shasum` on macOS, `sha256sum` on Linux; both print `<digest>  <name>`).

`none` means **no verification is possible on this machine**, and the caller
treats that as a hard stop for source 3 rather than as permission to proceed.
-/
def sha256OfFile (path : FilePath) : IO (Option String) := do
  for (exe, flags) in [("shasum", #["-a", "256"]), ("sha256sum", (#[] : Array String))] do
    match ← findOnPath exe with
    | none => pure ()
    | some bin =>
      let out ← IO.Process.output {cmd := bin.toString, args := flags.push path.toString}
      if out.exitCode == 0 then
        match ((out.stdout.splitOn " ").filter (!·.isEmpty)).head? with
        | some digest => return some digest.trimAscii.toString
        | none => pure ()
  return none

/-- Fetch one URL to one path. -/
def curlTo (curl : FilePath) (url : String) (dest : FilePath) : IO (Except String Unit) := do
  -- `-L` is not optional: a release asset URL answers a redirect to
  -- `release-assets.githubusercontent.com` 【実測 2026-08-18】, and without it
  -- curl writes the redirect body and exits 0 — a "successful" download of a
  -- few hundred bytes of HTML. `-f` is what turns a 404 into a non-zero exit
  -- for the same reason. Neither is a nicety: both are the difference between
  -- failing here and failing later with a confusing message.
  let out ← IO.Process.output {
    cmd := curl.toString
    args := #["-fsSL", "--connect-timeout", "20", "-o", dest.toString, url]}
  if out.exitCode == 0 then return .ok ()
  return .error s!"curl exited {out.exitCode} for {url}: {out.stderr.trimAscii.toString}"

/--
Download the release archive into `work`, verify it, and — only then — put the
executable at `targetDir/litedoc4`.

`work` is a subdirectory of `targetDir` so that the final `IO.FS.rename` stays
inside one filesystem, and so that a half-finished download is visibly beside
the thing it would become rather than in a shared temporary directory. The
caller removes it on both paths.
-/
def fetchRelease (version triple : String) (targetDir work : FilePath) :
    IO (Except String FilePath) := do
  let asset := s!"litedoc4-{triple}.tar.gz"
  let base := releaseBaseUrl version
  let some curl ← findOnPath "curl"
    | return .error "no `curl` on PATH to download with"
  let some tar ← findOnPath "tar"
    | return .error "no `tar` on PATH to unpack with"
  IO.FS.createDirAll work
  let archive := work / asset
  let sums := work / "checksums.txt"
  -- Printed *before* the first request. A build tool that reaches the network
  -- without saying so is the thing this line exists to prevent (plan §5 L2-e);
  -- the size cannot be printed here because it is not known until afterwards,
  -- so it goes on the line below, with the digest.
  IO.println s!"litedoc4: downloading {base}/{asset}"
  match ← curlTo curl s!"{base}/{asset}" archive with
  | .error message => return .error message
  | .ok () => pure ()
  match ← curlTo curl s!"{base}/checksums.txt" sums with
  | .error message =>
    return .error s!"{message} — the archive downloaded but nothing can verify it, so it is not used"
  | .ok () => pure ()
  let some expected := checksumFor (← IO.FS.readFile sums) asset
    | return .error s!"{base}/checksums.txt names no {asset}"
  let some actual ← sha256OfFile archive
    | return .error "no `shasum` and no `sha256sum` on PATH: the archive cannot be verified, so it is not used"
  if actual != expected then
    return .error s!"SHA-256 mismatch for {asset}: checksums.txt says {expected}, the download is {actual}"
  IO.println s!"litedoc4: {(← archive.metadata).byteSize} bytes, sha256 {actual} matches {base}/checksums.txt"
  let untar ← IO.Process.output {
    cmd := tar.toString, args := #["xzf", archive.toString, "-C", work.toString]}
  if untar.exitCode != 0 then
    return .error s!"tar exited {untar.exitCode} on {asset}: {untar.stderr.trimAscii.toString}"
  -- The archive unpacks to a **versioned** directory even though its own name
  -- is not versioned (`release.yml`, "Archive"), which is what keeps two
  -- versions from colliding on disk.
  let unpacked := work / s!"litedoc4-{version}-{triple}" / "litedoc4"
  unless ← isFileAt unpacked do
    return .error s!"{asset} does not contain litedoc4-{version}-{triple}/litedoc4"
  let bin := targetDir / "litedoc4"
  IO.FS.rename unpacked bin
  return .ok bin

/-- `fetchRelease` plus the cleanup that has to happen on **both** paths: an
archive whose checksum did not match must not be left anywhere a later run
could take it for a cache. (Only `targetDir/litedoc4` is ever read as one, so
this is belt and braces — but the belt is cheap and the alternative is a
directory named after a version holding bytes nobody vouched for.) -/
def downloadRelease (version triple : String) (targetDir : FilePath) :
    IO (Except String FilePath) := do
  let work := targetDir / ".download"
  if ← work.pathExists then IO.FS.removeDirAll work
  IO.FS.createDirAll targetDir
  let outcome ← fetchRelease version triple targetDir work
  if ← work.pathExists then IO.FS.removeDirAll work
  return outcome

/--
Which `litedoc4` (the Rust half) this script runs, and in what order it is looked
for. **One function on purpose**: L2 fills in sources 2 and 3 and nothing else in
the tree gets an opinion about where the binary comes from (CLAUDE.md 「判断は 1
箇所に集める」).

| | source | |
|---:|---|---|
| 1 | `$LITEDOC4_BIN` | an error if it is not a file |
| 2 | `$XDG_CACHE_HOME/litedoc4/v<version>/<target>/litedoc4` | version from this tree's `Cargo.toml` |
| 3 | the GitHub Release for that version | SHA-256 checked against `checksums.txt` |
| 4 | `litedoc4` on `PATH` | its version is printed, with a warning |
| 5 | `cargo build` in this package | slow, but cannot be out of step |
| 6 | an error naming every source above | |

`PATH` sits **below** the download on purpose: whatever answers to that name may
write an IR schema older than this checkout's renderer reads. Sources 2 and 3
know which version this tree is, so they cannot.

`$LITEDOC4_BIN` set to something that is not a file is an **error, not a
fallthrough**: a caller who named a binary and silently got a different one
would never find out.

Two things sources 2 and 3 will not do (plan §5 L2-e):

* **reach the network without saying so** — the URL is printed before the first
  request, and `LITEDOC4_NO_DOWNLOAD=1` turns source 3 off entirely while
  leaving source 2 (a cache that is already on disk costs nothing and needs no
  network);
* **run something unverified** — every way of failing to check the SHA-256 is a
  failure of source 3, not a reason to continue.
-/
def resolveLitedoc4 (pkgDir : FilePath) : IO (Except String FilePath) := do
  let mut tried : Array String := #[]

  -- 1. $LITEDOC4_BIN.
  match ← IO.getEnv "LITEDOC4_BIN" with
  | some raw =>
    if raw.isEmpty then
      -- `LITEDOC4_BIN=` in a wrapper script is how a shell spells "I did not set
      -- this" (`crates/litedoc4/src/extract.rs` `or_env` reads it the same way).
      tried := tried.push "$LITEDOC4_BIN: set but empty"
    else
      let bin : FilePath := raw
      if ← isFileAt bin then
        return .ok bin
      else
        return .error s!"$LITEDOC4_BIN is {raw}, which is not a file"
  | none => tried := tried.push "$LITEDOC4_BIN: unset"

  -- 2 and 3 both hang off the same two answers — which version this tree is,
  -- and which target this machine is — so they are worked out once here. Not
  -- knowing either is a failure of *both* sources, reported as one line.
  let manifest := pkgDir / "Cargo.toml"
  let triple ← hostTarget
  match ← cargoWorkspaceVersion manifest, ← cacheRoot with
  | none, _ =>
    tried := tried.push s!"cache and release: no [workspace.package] version in {manifest}"
  | _, none =>
    tried := tried.push "cache and release: neither $XDG_CACHE_HOME nor $HOME is set"
  | some version, some cache =>
    let targetDir := cache / s!"v{version}" / triple
    let cached := targetDir / "litedoc4"

    -- 2. The version-pinned cache (§5 L2-d). Checked before $LITEDOC4_NO_DOWNLOAD
    --    is consulted: an offline machine that downloaded this once should keep
    --    working.
    if ← isFileAt cached then
      IO.println s!"litedoc4: {cached} (cached, v{version})"
      return .ok cached
    tried := tried.push s!"cache: no {cached}"

    -- 3. The GitHub Release for *this tree's* version (§5 L2-a..c).
    if !(releaseTargets.contains triple) then
      -- Loud, because it is not a fault: releases carry two targets on purpose
      -- (see `releaseTargets`), so this is the designed path for every other
      -- machine and it has to be legible rather than silent.
      let carried := String.intercalate " and " releaseTargets
      IO.println s!"litedoc4: no release asset for {triple}; releases carry {carried}. \
        Trying PATH next."
      tried := tried.push s!"release v{version}: no asset for {triple}"
    else if ← envIsSet "LITEDOC4_NO_DOWNLOAD" then
      IO.println "litedoc4: $LITEDOC4_NO_DOWNLOAD is set; not downloading. Trying PATH next."
      tried := tried.push s!"release v{version} {triple}: skipped ($LITEDOC4_NO_DOWNLOAD)"
    else
      match ← downloadRelease version triple targetDir with
      | .ok bin =>
        IO.println s!"litedoc4: {bin} (downloaded, v{version})"
        return .ok bin
      | .error message =>
        -- Printed as well as recorded: a download that failed and was recovered
        -- from by a later source is still the most interesting thing that
        -- happened, and source 6 never runs when a later source answers.
        IO.println s!"litedoc4: release v{version} {triple} not used: {message}"
        tried := tried.push s!"release v{version} {triple}: {message}"

  -- 4. PATH.
  match ← findOnPath "litedoc4" with
  | some bin =>
    let probe ← IO.Process.output {cmd := bin.toString, args := #["--version"]}
    let version := probe.stdout.trimAscii.toString
    IO.println s!"litedoc4: {bin} ({if version.isEmpty then "no --version" else version})"
    IO.println "litedoc4: warning: that is whatever is on PATH. Nothing here checks that its \
      IR schema matches this checkout's — set LITEDOC4_BIN to pin one."
    return .ok bin
  | none => tried := tried.push "PATH: no `litedoc4`"

  -- 5. Build it from this package's own source, if this is a checkout with cargo
  --    available. Slow (a release build), and the last thing tried, but it is
  --    the only source that cannot be out of step with this tree. `manifest` is
  --    the same file sources 2 and 3 read the version out of.
  if ← isFileAt manifest then
    match ← findOnPath "cargo" with
    | some cargo =>
      IO.println s!"litedoc4: not found; building it from {manifest}"
      let child ← IO.Process.spawn {
        cmd := cargo.toString
        args := #["build", "--release", "--bin", "litedoc4"]
        cwd := some pkgDir
      }
      let code ← child.wait
      let built := pkgDir / "target" / "release" / "litedoc4"
      if code == 0 && (← isFileAt built) then
        return .ok built
      tried := tried.push s!"cargo build --release --bin litedoc4 in {pkgDir}: exited {code}"
    | none => tried := tried.push "PATH: no `cargo` to build it with"
  else
    tried := tried.push s!"no {manifest} to build from"

  -- 6. Say what was looked for and where.
  return .error <|
    "no `litedoc4` executable (the Rust half of litedoc4). Looked, in order:\n"
      ++ String.intercalate "\n" (tried.toList.map ("  - " ++ ·))
      ++ "\n\nSet LITEDOC4_BIN to one, or put `litedoc4` on PATH. README.md \
          §Running it locally has both the release download and the cargo build."

/--
Generate this package's documentation.

Fills in the three flags of `litedoc4 build` a consumer cannot supply by hand —
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
    | IO.eprintln "lake run docs: --out <dir> is required and has no default: `litedoc4 build` \
        refuses an --out inside the package it documents (crates/litedoc4/src/build.rs), so no \
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
  -- `crates/litedoc4/src/lakefile.rs` already made the same call for the TOML
  -- path.
  let libs := root.leanLibs.foldl (fun acc lib => acc ++ lib.roots) #[]
  if libs.isEmpty then
    IO.eprintln s!"lake run docs: {root.prettyName} declares no `lean_lib`, so there is nothing to \
      document. This script fills in `litedoc4 build --lib` from the workspace, and the \
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
  -- litedoc4/extract` as a subprocess — is not used: a subprocess would re-read
  -- the workspace this script is already holding, and would report failures as a
  -- shell exit code instead of as Lake's own build log.
  let extractBin ← runBuild extract.fetch

  let litedoc4 ← match ← resolveLitedoc4 __dir__ with
    | .ok bin => pure bin
    | .error message =>
      IO.eprintln s!"lake run docs: {message}"
      return 4

  -- Resolved here rather than handed over relative: `litedoc4 build` refuses an
  -- `--out` under `--root`, and it resolves a relative path against *its own*
  -- working directory. Printing the absolute path below is what makes a
  -- surprising answer visible.
  let cwd ← IO.currentDir
  let outDir : FilePath := if (FilePath.mk outRaw).isAbsolute then outRaw else cwd / outRaw

  -- The toolchain's own `lake`, not the name `lake` on PATH: this is the one the
  -- workspace was loaded with, and `litedoc4` looks for the `lean` that answers
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

  IO.println s!"litedoc4: {litedoc4} {String.intercalate " " cmdArgs.toList}"
  -- No augmented environment: `lake run` does not put `LEAN_PATH` in the
  -- script's environment 【実測, probe §4】 and `litedoc4 build` does not want
  -- one — it runs `lake env` inside `--root` itself for every extraction, which
  -- is where the Lean environment comes from (`crates/litedoc4/src/extract.rs`).
  -- That is what `--lake` above is for.
  let child ← IO.Process.spawn {cmd := litedoc4.toString, args := cmdArgs}
  child.wait
