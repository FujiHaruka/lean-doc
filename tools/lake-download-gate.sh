#!/usr/bin/env bash
# L2 — does `resolveLitedoc4` really fetch the Rust half from a GitHub Release?
#
# `docs/plans/lake-package.md` §5 adds two sources to the lakefile's
# `resolveLitedoc4`: a version-pinned cache and a checksum-verified download from
# the release that matches this tree's `Cargo.toml`. This gate is the only place
# either of them is executed.
#
# WHY THIS GATE EXISTS SEPARATELY FROM tools/lake-package-gate.sh
#   That gate sets `LITEDOC4_BIN`, which is source 1, so it returns before
#   sources 2..5 are reached — it has never run a single line of this code
#   【実測 2026-08-18】. **This gate must therefore never set `LITEDOC4_BIN`.**
#   Setting it would turn every item below into a check of nothing, and the
#   output would look exactly the same.
#
# THERE ARE TWO WORLDS, AND A RUN SAYS WHICH ONE IT WAS IN
#   `lakefile.lean` looks the binary up by the version in **this tree's**
#   `Cargo.toml`, so whether a release exists for it is a property of the
#   checkout, not of the machine:
#
#     ON A RELEASE TAG        a release carries v<version>. Items 1 and 2 are
#                             the download and the cache, exactly as written.
#     ON main, MID-CYCLE      no release carries v<version> — the normal state
#                             of a tree that is ahead of the last release. There
#                             is nothing to download, so items 1 and 2 grade the
#                             **documented fall-through** instead: source 3 says
#                             the asset is not there, PATH answers, a site is
#                             written, and nothing lands in the cache.
#
#   In the second world the summary and the final verdict both say **the
#   download path was NOT EXERCISED**, the way `tools/deps-docs-gate.sh` says it
#   of its branch 2. Reporting `ok` for a path that never ran is the failure this
#   repository has shipped four spellings of (CLAUDE.md 「skip で緑を返さない」);
#   reporting FAILED would be the opposite lie, because the mechanism is not
#   broken — it was not run.
#
#   Which world it is, is decided **once, up front**, by asking the asset URL for
#   its status: 200 is a release, 404 is no release, and anything else — curl
#   unable to reach GitHub at all — is a hard exit 2. Offline is a failure of
#   this gate, not a third world.
#
# THE FIVE ITEMS
#   1 DOWNLOAD   an empty cache + the real network: the release archive is
#                fetched, its SHA-256 is checked against the published
#                checksums.txt, the binary lands in the cache and builds a site.
#                Falls: the download path is broken, or the release moved.
#                *No release*: source 3 announces that there is no asset for
#                this version, PATH answers, a site is still written and the
#                cache stays empty. Falls: it goes quiet, does not fall through,
#                or writes nothing.
#   2 CACHED     the same cache with **curl replaced by one that always fails**:
#                the second run must not need the network at all. Falls: the
#                cache is not being consulted (judged by whether curl was
#                invoked, not by whether a line was printed).
#                *No release*: the mirror image — item 1 cached nothing, so the
#                second run **must** reach for curl again and fall through
#                again. Falls: a failed download was remembered as a cache hit.
#   3 CHECKSUM   a curl that serves an archive with a **wrong**
#                checksums.txt: the run must fail, leave nothing in the cache
#                and execute nothing it downloaded. Falls: verification is
#                decorative. The archive is the released one where there is a
#                release and a locally built one otherwise; either way it is a
#                **valid** archive that is really hashed and really disagrees,
#                so nothing about the product's verification is stubbed.
#   4 NO-DL      `LITEDOC4_NO_DOWNLOAD=1` with an empty cache: source 3 is
#                skipped, out loud, and source 4 (PATH) answers. Falls: the
#                opt-out does not opt out, or does it silently.
#   5 NO ASSET   a target the releases do not carry: source 3 says so and falls
#                through. Falls: a machine with no asset would hang, guess, or
#                fail instead of moving on. Releases carry two targets on
#                purpose (`.github/workflows/release.yml`), so this is a normal
#                path for every other machine, not an error path.
#
# HOW THE NETWORK IS CONTROLLED
#   Items 2..5 put a shim `curl` first on PATH which logs every invocation. That
#   is what "did not download" is judged by: a run that printed nothing about
#   downloading but still called curl would fail here. Item 1 uses the real one.
#
# WHAT IS NEVER TOUCHED
#   **The user's ~/.cache.** Every run sets XDG_CACHE_HOME to a directory under
#   $OUT, so nothing outside $OUT is written or removed. Also not touched:
#   /Users/haruka/dev/lean-projects, and `$LITEDOC4_BIN` (see above).
#
# usage: lake-download-gate.sh [--out DIR] [--keep]
#   --out   working directory (default: a temporary one, removed on success)
#   --keep  do not remove a temporary --out
#
#   LITEDOC4  a `litedoc4` executable to put **on PATH** for the items where
#             source 4 is the one that has to answer — 4 and 5 always, and 1 and
#             2 as well where no release carries this version (default:
#             target/debug/litedoc4). It is never exported as LITEDOC4_BIN.
#   LAKE      the lake executable (default: ~/.elan/bin/lake)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
FIXTURE="$ROOT/e2e/consumer"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
LITEDOC4="${LITEDOC4:-$ROOT/target/debug/litedoc4}"

OUT=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    -h|--help) sed -n '1,92p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

# No input, no gate. Every one of these is a hard non-zero exit rather than a
# skip: `skipping: offline` + exit 0 is the failure shape this project has
# already paid for (CLAUDE.md 「skip で緑を返さない」).
[ -x "$LAKE" ] || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
[ -x "$LITEDOC4" ] || { echo "no litedoc4 at $LITEDOC4 — cargo build --bin litedoc4" >&2; exit 2; }
[ -f "$ROOT/lakefile.lean" ] || { echo "no $ROOT/lakefile.lean — nothing to check" >&2; exit 2; }
[ -f "$FIXTURE/lakefile.toml" ] || { echo "no consumer fixture at $FIXTURE" >&2; exit 2; }
command -v curl >/dev/null || { echo "no curl — this gate downloads a release" >&2; exit 2; }
command -v shasum >/dev/null || command -v sha256sum >/dev/null \
  || { echo "no shasum and no sha256sum — nothing here could verify a download" >&2; exit 2; }

# The version the lakefile will ask the release for. Read here as well so that a
# gate run says out loud which release it is grading.
VERSION="$(awk '
  /^\[/ { in_pkg = ($0 ~ /^\[workspace\.package\]/); next }
  in_pkg && /^version[[:space:]]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }
' "$ROOT/Cargo.toml")"
[ -n "$VERSION" ] || { echo "no [workspace.package] version in $ROOT/Cargo.toml" >&2; exit 2; }

# The expected target triple, derived from `uname` — **deliberately the other
# source**. The lakefile uses `System.Platform.target` (plan §6 D3); if its
# normalisation is ever wrong, the cache file will not be where this gate looks
# and item 1 fails, which is the whole point of not asking the same oracle
# twice.
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)  TRIPLE=aarch64-apple-darwin ;;
  Linux/x86_64)  TRIPLE=x86_64-unknown-linux-musl ;;
  *) echo "this machine ($(uname -s)/$(uname -m)) has no release asset, so items 1..3 cannot run" >&2
     echo "run this gate on a machine the releases carry: see releaseTargets in lakefile.lean" >&2
     exit 2 ;;
esac
BASE="https://github.com/FujiHaruka/litedoc4/releases/download/v$VERSION"
ASSET="litedoc4-$TRIPLE.tar.gz"

if [ -z "$OUT" ]; then
  OUT="$(mktemp -d)"
  TEMPORARY=1
else
  mkdir -p "$OUT"
  TEMPORARY=0
fi

# ---------------------------------------------------------------------------
# Which world is this?  (see "THERE ARE TWO WORLDS" in the header)
# ---------------------------------------------------------------------------
# The three answers are kept apart on purpose, because collapsing them is how a
# gate starts lying:
#
#   200          a release carries this version — items 1 and 2 run the download.
#   404          no release carries it — items 1 and 2 run the fall-through, and
#                every line this script prints afterwards says so.
#   anything     including curl's own failure to connect: **exit 2**. "I could
#   else         not ask" is not an answer, and reading it as "no release" would
#                turn an offline machine into a green run.
#
# No `-f`, deliberately: `-f` makes a 404 curl's exit code instead of an HTTP
# status, and then the one thing this probe exists to distinguish is gone.
PROBE_ERR="$OUT/release-probe.err"
probe_rc=0
HTTP="$(curl -sSL --head -o /dev/null -w '%{http_code}' \
          --connect-timeout 20 --max-time 60 "$BASE/$ASSET" 2>"$PROBE_ERR")" || probe_rc=$?
if [ "$probe_rc" -ne 0 ]; then
  echo "cannot reach $BASE/$ASSET (curl exited $probe_rc) — this gate needs the network," >&2
  echo "and being unable to ask whether a release exists is not the same as there being none" >&2
  cat "$PROBE_ERR" >&2
  exit 2
fi
case "$HTTP" in
  200) HAVE_RELEASE=1 ;;
  404) HAVE_RELEASE=0 ;;
  *)   echo "$BASE/$ASSET answered HTTP $HTTP, which is neither 200 nor 404 — this gate will" >&2
       echo "not guess which world it is in" >&2
       exit 2 ;;
esac

ITEMS=5
ran=0
failed=0

# Said before the first item rather than only in the summary: a reader scrolling
# a CI log meets the items first, and "ITEM 1 ok" means two different things in
# the two worlds.
if [ "$HAVE_RELEASE" -eq 1 ]; then
  printf 'world: a release carries v%s (%s answered 200) — items 1 and 2 run the download\n' \
    "$VERSION" "$BASE/$ASSET"
else
  printf 'world: NO release carries v%s (%s answered 404) — items 1 and 2 run the\n' \
    "$VERSION" "$BASE/$ASSET"
  printf '       documented fall-through instead, and the download path is NOT exercised\n'
fi

pass () { ran=$((ran + 1)); printf 'ITEM %s ok    %s\n' "$1" "$2"; }
fail () { ran=$((ran + 1)); failed=$((failed + 1)); printf 'ITEM %s FAIL  %s\n' "$1" "$2" >&2; }
say  () { printf '\n=== %s\n' "$1"; }

# ---------------------------------------------------------------------------
# Shims. All of them live under $OUT and are put *first* on PATH.
# ---------------------------------------------------------------------------
mkdir -p "$OUT/nonet" "$OUT/nocargo" "$OUT/pathbin" "$OUT/shim3" "$OUT/stage"
CURL_LOG="$OUT/curl-calls.log"

# A curl that cannot reach anything, and says it was asked to. What the items
# assert about this log is that it says whether the tool was **invoked** — not
# whether a message was printed (plan §5 L2-f item 2). Items 4 and 5 want it
# empty; item 2 wants it empty where there is a release (the cache answered) and
# non-empty where there is none (nothing was cached, so source 3 had to try).
cat > "$OUT/nonet/curl" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "$CURL_LOG"
echo "lake-download-gate: the network is disabled for this item" >&2
exit 7
EOF

# A cargo that refuses. Source 5 of resolveLitedoc4 is a release build of this
# workspace; without this, an item that is *supposed* to end in failure would
# instead spend minutes compiling and then succeed for the wrong reason.
cat > "$OUT/nocargo/cargo" <<'EOF'
#!/usr/bin/env bash
echo "lake-download-gate: cargo is disabled for this item" >&2
exit 1
EOF

# Item 3's curl: serves a genuine archive with a checksums.txt that does not
# match it. Nothing about the product's verification is stubbed — a real archive
# is really hashed, and really disagrees. Where the archive comes from is
# `stage`'s business below; item 3 never touches the network either way, so it
# is the one item that runs identically in both worlds.
cat > "$OUT/shim3/curl" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "$OUT/shim3-calls.log"
dest=""; url=""
while [ \$# -gt 0 ]; do
  case "\$1" in
    -o) dest="\$2"; shift 2 ;;
    --connect-timeout|--retry|--max-time) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
case "\$url" in
  */checksums.txt) src="$OUT/stage/checksums.txt" ;;
  *.tar.gz)        src="$OUT/stage/asset.tar.gz" ;;
  *) echo "shim curl: unexpected url \$url" >&2; exit 22 ;;
esac
cp "\$src" "\$dest"
EOF

chmod +x "$OUT/nonet/curl" "$OUT/nocargo/cargo" "$OUT/shim3/curl"
cp "$LITEDOC4" "$OUT/pathbin/litedoc4"

# PATH with every directory that holds a `litedoc4` removed, so that items which
# must fall all the way through cannot be rescued by whatever this machine
# happens to have installed. Items 4 and 5 add $OUT/pathbin back on purpose.
CLEAN_PATH="$(
  IFS=:
  first=1
  for dir in $PATH; do
    [ -n "$dir" ] || continue
    if [ -x "$dir/litedoc4" ]; then continue; fi
    if [ "$first" = 1 ]; then printf '%s' "$dir"; first=0; else printf ':%s' "$dir"; fi
  done
)"

# Stage item 3's payload: an archive, and an all-zero digest for it.
#
# The released archive when there is one — that keeps item 3 byte-for-byte the
# check it has always been. When there is none, one is built here from
# `$LITEDOC4`, laid out the way `release.yml` lays a release out
# (`litedoc4-<version>-<triple>/litedoc4`). What item 3 grades is unaffected
# either way: `fetchRelease` hashes the archive **before** it unpacks it, so it
# never reaches these bytes' contents — it has to reject them for the digest.
if [ "$HAVE_RELEASE" -eq 1 ]; then
  STAGE_ORIGIN="the released $ASSET"
  curl -fsSL -o "$OUT/stage/asset.tar.gz" "$BASE/$ASSET"
else
  STAGE_ORIGIN="a locally built archive (no release carries v$VERSION)"
  mkdir -p "$OUT/stagesrc/litedoc4-$VERSION-$TRIPLE"
  cp "$LITEDOC4" "$OUT/stagesrc/litedoc4-$VERSION-$TRIPLE/litedoc4"
  tar -C "$OUT/stagesrc" -czf "$OUT/stage/asset.tar.gz" "litedoc4-$VERSION-$TRIPLE"
fi
printf '%s  %s\n' "0000000000000000000000000000000000000000000000000000000000000000" "$ASSET" \
  > "$OUT/stage/checksums.txt"

sha256_of () { if command -v shasum >/dev/null; then shasum -a 256 "$1" | cut -d' ' -f1
               else sha256sum "$1" | cut -d' ' -f1; fi; }

# `lake run docs` in the fixture, with a controlled environment. $1 is the item
# number (names the log and the site), the rest is `env` assignments.
run_docs () {
  local n="$1"; shift
  local rc=0
  # `env -u`: whatever the caller's shell already had set for these must not
  # decide what an item measures. LITEDOC4_BIN in particular would answer as
  # source 1 and every item below would grade nothing.
  (cd "$FIXTURE" && env -u LITEDOC4_BIN -u LITEDOC4_NO_DOWNLOAD -u LITEDOC4_TARGET_OVERRIDE \
     "$@" "$LAKE" run docs -- --out "$OUT/site$n") \
    >"$OUT/run$n.log" 2>&1 || rc=$?
  echo "$rc"
}

CACHE1="$OUT/cache1"
CACHED1="$CACHE1/litedoc4/v$VERSION/$TRIPLE/litedoc4"
CACHEDIR1="$CACHE1/litedoc4/v$VERSION/$TRIPLE"
# The line `resolveLitedoc4` prints when a download it attempted did not work
# out — the announcement items 1 and 2 grade in the no-release world.
NOT_USED="litedoc4: release v$VERSION $TRIPLE not used:"
FELL_THROUGH="litedoc4: $OUT/pathbin/litedoc4 build --root"

if [ "$HAVE_RELEASE" -eq 1 ]; then
  # -------------------------------------------------------------------------
  say "1/5 an empty cache downloads the release and builds a site"
  # -------------------------------------------------------------------------
  # Real curl, no `litedoc4` anywhere on PATH and no cargo: if the download does
  # not work there is nothing left to rescue the run, which is what makes this
  # item's exit code mean something.
  rc1="$(run_docs 1 "PATH=$OUT/nocargo:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE1")"
  if [ "$rc1" -ne 0 ]; then
    fail 1 "lake run docs exited $rc1 with an empty cache — see $OUT/run1.log"
    tail -20 "$OUT/run1.log" >&2
  elif ! grep -qF "downloading $BASE/$ASSET" "$OUT/run1.log"; then
    fail 1 "nothing announced a download of $BASE/$ASSET — source 3 did not run"
  elif ! grep -qE "sha256 [0-9a-f]{64} matches $BASE/checksums.txt" "$OUT/run1.log"; then
    fail 1 "the archive was fetched but no SHA-256 was reported as matching — see $OUT/run1.log"
  elif [ ! -x "$CACHED1" ]; then
    fail 1 "nothing at $CACHED1 — the download did not land in the version-pinned cache"
  elif ! grep -qF "litedoc4: $CACHED1 build --root" "$OUT/run1.log"; then
    fail 1 "the site was built by something other than $CACHED1 — see the \`litedoc4: … build\` line in $OUT/run1.log"
  elif [ ! -f "$OUT/site1/site/index.html" ]; then
    fail 1 "lake run docs exited 0 but wrote no site: $OUT/site1/site/index.html is missing"
  elif [ "$(sha256_of "$CACHED1")" = "$(sha256_of "$LITEDOC4")" ]; then
    # Not pedantry: if these ever match, this item is grading the local build
    # under a cache-shaped path and has stopped saying anything about releases.
    fail 1 "$CACHED1 is byte-identical to $LITEDOC4 — that is the local build, not the release"
  elif [ -e "$CACHEDIR1/.download" ]; then
    fail 1 "a .download directory was left behind in the cache"
  else
    pass 1 "v$VERSION $TRIPLE: $(sha256_of "$CACHED1" | cut -c1-12)… ($("$CACHED1" --version)), site at $OUT/site1/site"
  fi

  # -------------------------------------------------------------------------
  say "2/5 the second run needs no network"
  # -------------------------------------------------------------------------
  # Same cache, curl replaced by one that always fails and logs. Nothing is said
  # about what the run *prints*: the assertion is that curl was never called.
  rm -f "$CURL_LOG"
  rc2="$(run_docs 2 "PATH=$OUT/nonet:$OUT/nocargo:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE1")"
  if [ ! -x "$CACHED1" ]; then
    fail 2 "no cached binary from item 1 to reuse — this item has nothing to check"
  elif [ "$rc2" -ne 0 ]; then
    fail 2 "lake run docs exited $rc2 with a warm cache and no network — the cache is not being used; see $OUT/run2.log"
    tail -20 "$OUT/run2.log" >&2
  elif [ -s "$CURL_LOG" ]; then
    fail 2 "curl was invoked $(wc -l <"$CURL_LOG" | tr -d ' ') time(s) on a cache hit — see $CURL_LOG"
  elif ! grep -qF "litedoc4: $CACHED1 (cached, v$VERSION)" "$OUT/run2.log"; then
    fail 2 "the run did not report using $CACHED1 from the cache — see $OUT/run2.log"
  elif [ ! -f "$OUT/site2/site/index.html" ]; then
    fail 2 "lake run docs exited 0 but wrote no site: $OUT/site2/site/index.html is missing"
  else
    pass 2 "cache hit, 0 curl invocations, site at $OUT/site2/site"
  fi
else
  # -------------------------------------------------------------------------
  say "1/5 no asset for this version: source 3 says so, PATH answers, a site is written"
  # -------------------------------------------------------------------------
  # **This is not the download path with the assertions relaxed.** It is the
  # other branch of `resolveLitedoc4`, the one every checkout ahead of a release
  # takes, and what is asserted below is exactly what its documentation promises:
  # it announces the attempt, it announces the failure, it falls through to the
  # next source, it writes a site, and it leaves nothing behind that a later run
  # would take for a cache.
  #
  # $OUT/pathbin is on PATH here — items 4 and 5 do the same — because source 4
  # is the one that has to answer. Without it the run would end at source 6 and
  # this item would grade "a missing release is fatal", which is the opposite of
  # what the lakefile says.
  rc1="$(run_docs 1 "PATH=$OUT/nocargo:$OUT/pathbin:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE1")"
  if [ "$rc1" -ne 0 ]; then
    fail 1 "lake run docs exited $rc1 with no release to download — a checkout ahead of every release cannot build docs; see $OUT/run1.log"
    tail -20 "$OUT/run1.log" >&2
  elif ! grep -qF "downloading $BASE/$ASSET" "$OUT/run1.log"; then
    fail 1 "nothing announced an attempt at $BASE/$ASSET — source 3 did not run at all"
  elif ! grep -qF "$NOT_USED" "$OUT/run1.log"; then
    fail 1 "source 3 failed without saying so — nothing in $OUT/run1.log says \`$NOT_USED\`"
  elif [ -e "$CACHED1" ]; then
    fail 1 "something was written to $CACHED1 although there was nothing to download"
  elif [ -e "$CACHEDIR1/.download" ]; then
    fail 1 "a .download directory was left behind in the cache after a download that never succeeded"
  elif ! grep -qF "$FELL_THROUGH" "$OUT/run1.log"; then
    fail 1 "the run did not fall through to the \`litedoc4\` on PATH — see $OUT/run1.log"
  elif [ ! -f "$OUT/site1/site/index.html" ]; then
    fail 1 "lake run docs exited 0 but wrote no site: $OUT/site1/site/index.html is missing"
  else
    pass 1 "v$VERSION $TRIPLE has no asset: announced, nothing cached, PATH answered, site at $OUT/site1/site (DOWNLOAD NOT EXERCISED)"
  fi

  # -------------------------------------------------------------------------
  say "2/5 a download that could not happen is not remembered as a cache hit"
  # -------------------------------------------------------------------------
  # The mirror image of the cache item. There, curl **must not** be called on the
  # second run; here it **must**, because item 1 had nothing to put in the cache
  # — and an empty log would mean source 2 answered from a cache entry that a
  # failed download had no business creating.
  #
  # The `nonet` curl is used rather than the real one so that this says something
  # about the resolver and not about GitHub: whatever curl the resolver reaches
  # for, it fails, and the run still has to end at PATH with a site.
  rm -f "$CURL_LOG"
  rc2="$(run_docs 2 "PATH=$OUT/nonet:$OUT/nocargo:$OUT/pathbin:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE1")"
  if [ "$rc2" -ne 0 ]; then
    fail 2 "lake run docs exited $rc2 on the second run with no release — see $OUT/run2.log"
    tail -20 "$OUT/run2.log" >&2
  elif [ ! -s "$CURL_LOG" ]; then
    fail 2 "curl was never invoked, so something answered before source 3 — item 1 cached nothing, so nothing should have; see $OUT/run2.log"
  elif [ -e "$CACHED1" ]; then
    fail 2 "something was written to $CACHED1 by a run whose every download failed"
  elif ! grep -qF "$NOT_USED" "$OUT/run2.log"; then
    fail 2 "source 3 failed without saying so — nothing in $OUT/run2.log says \`$NOT_USED\`"
  elif ! grep -qF "$FELL_THROUGH" "$OUT/run2.log"; then
    fail 2 "the run did not fall through to the \`litedoc4\` on PATH — see $OUT/run2.log"
  elif [ ! -f "$OUT/site2/site/index.html" ]; then
    fail 2 "lake run docs exited 0 but wrote no site: $OUT/site2/site/index.html is missing"
  else
    pass 2 "$(wc -l <"$CURL_LOG" | tr -d ' ') curl invocation(s) — the cache stayed empty, PATH answered, site at $OUT/site2/site (CACHE HIT NOT EXERCISED)"
  fi
fi

# ---------------------------------------------------------------------------
say "3/5 a checksum that does not match stops the run"
# ---------------------------------------------------------------------------
CACHE3="$OUT/cache3"
CACHED3="$CACHE3/litedoc4/v$VERSION/$TRIPLE/litedoc4"
rm -f "$OUT/shim3-calls.log"
rc3="$(run_docs 3 "PATH=$OUT/shim3:$OUT/nocargo:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE3")"
shim_calls=0
if [ -f "$OUT/shim3-calls.log" ]; then
  shim_calls="$(wc -l <"$OUT/shim3-calls.log" | tr -d ' ')"
fi
if [ "$shim_calls" -lt 2 ]; then
  fail 3 "the download was attempted $shim_calls time(s), not 2 (archive + checksums.txt) — this item never reached verification"
elif ! grep -q "SHA-256 mismatch for $ASSET" "$OUT/run3.log"; then
  fail 3 "a corrupted checksums.txt was accepted: no mismatch reported — see $OUT/run3.log"
elif [ "$rc3" -eq 0 ]; then
  fail 3 "lake run docs exited 0 after a checksum mismatch — see $OUT/run3.log"
elif [ -e "$CACHED3" ]; then
  fail 3 "an unverified binary was cached at $CACHED3"
elif [ -e "$CACHE3/litedoc4/v$VERSION/$TRIPLE/.download" ]; then
  fail 3 "the unverified archive was left in $CACHE3/litedoc4/v$VERSION/$TRIPLE/.download"
elif [ -f "$OUT/site3/site/index.html" ]; then
  fail 3 "a site was built even though no verified binary was ever obtained"
else
  pass 3 "$shim_calls fetch(es), mismatch reported, exit $rc3, nothing cached, nothing built"
fi

# ---------------------------------------------------------------------------
say "4/5 LITEDOC4_NO_DOWNLOAD=1 skips the release, out loud"
# ---------------------------------------------------------------------------
CACHE4="$OUT/cache4"
rm -f "$CURL_LOG"
rc4="$(run_docs 4 "PATH=$OUT/nonet:$OUT/pathbin:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE4" \
        "LITEDOC4_NO_DOWNLOAD=1")"
if [ "$rc4" -ne 0 ]; then
  fail 4 "lake run docs exited $rc4 with LITEDOC4_NO_DOWNLOAD=1 — see $OUT/run4.log"
  tail -20 "$OUT/run4.log" >&2
elif [ -s "$CURL_LOG" ]; then
  fail 4 "curl was invoked despite LITEDOC4_NO_DOWNLOAD=1 — see $CURL_LOG"
elif ! grep -q 'LITEDOC4_NO_DOWNLOAD is set; not downloading' "$OUT/run4.log"; then
  fail 4 "source 3 was skipped without saying so — nothing in $OUT/run4.log mentions LITEDOC4_NO_DOWNLOAD"
elif ! grep -qF "litedoc4: $OUT/pathbin/litedoc4 build --root" "$OUT/run4.log"; then
  fail 4 "the run did not fall through to the \`litedoc4\` on PATH — see $OUT/run4.log"
elif [ -e "$CACHE4/litedoc4/v$VERSION/$TRIPLE/litedoc4" ]; then
  fail 4 "something was written to the cache with downloads turned off"
elif [ ! -f "$OUT/site4/site/index.html" ]; then
  fail 4 "lake run docs exited 0 but wrote no site: $OUT/site4/site/index.html is missing"
else
  pass 4 "release skipped, 0 curl invocations, PATH answered, site at $OUT/site4/site"
fi

# ---------------------------------------------------------------------------
say "5/5 a target with no asset falls through instead of going quiet"
# ---------------------------------------------------------------------------
# x86_64-apple-darwin is the honest example: `release.yml` says in a comment why
# it is not built (no Intel runner to test one on), so this is what an Intel Mac
# actually meets.
NO_ASSET=x86_64-apple-darwin
CACHE5="$OUT/cache5"
rm -f "$CURL_LOG"
rc5="$(run_docs 5 "PATH=$OUT/nonet:$OUT/pathbin:$CLEAN_PATH" "XDG_CACHE_HOME=$CACHE5" \
        "LITEDOC4_TARGET_OVERRIDE=$NO_ASSET")"
if [ "$rc5" -ne 0 ]; then
  fail 5 "lake run docs exited $rc5 for a target with no asset — a machine the releases do not carry cannot build docs; see $OUT/run5.log"
  tail -20 "$OUT/run5.log" >&2
elif ! grep -qF "no release asset for $NO_ASSET" "$OUT/run5.log"; then
  fail 5 "source 3 went quiet on a target with no asset — nothing in $OUT/run5.log names $NO_ASSET"
elif [ -s "$CURL_LOG" ]; then
  fail 5 "curl was invoked for a target that has no asset — see $CURL_LOG"
elif ! grep -qF "litedoc4: $OUT/pathbin/litedoc4 build --root" "$OUT/run5.log"; then
  fail 5 "the run did not fall through to the \`litedoc4\` on PATH — see $OUT/run5.log"
elif [ ! -f "$OUT/site5/site/index.html" ]; then
  fail 5 "lake run docs exited 0 but wrote no site: $OUT/site5/site/index.html is missing"
else
  pass 5 "$NO_ASSET announced as unavailable, 0 curl invocations, PATH answered, site at $OUT/site5/site"
fi

# ---------------------------------------------------------------------------
say "summary"
# ---------------------------------------------------------------------------
printf 'version        : v%s %s (from %s)\n' "$VERSION" "$TRIPLE" "$ROOT/Cargo.toml"
if [ "$HAVE_RELEASE" -eq 1 ]; then
  printf 'release        : %s answered 200\n' "$BASE/$ASSET"
  printf 'download path  : exercised — items 1 and 2 fetched, verified and cached the release\n'
else
  printf 'release        : NONE — %s answered 404\n' "$BASE/$ASSET"
  printf 'download path  : NOT EXERCISED — no release carries v%s, so items 1 and 2 checked the\n' "$VERSION"
  printf '                 documented fall-through (announce, do not cache, hand over to PATH)\n'
  printf '                 instead. Nothing here says the download or the cache works.\n'
fi
printf 'item 3 archive : %s\n' "$STAGE_ORIGIN"
printf 'items reported : %s of %s\n' "$ran" "$ITEMS"
printf 'failed         : %s\n' "$failed"
printf 'out            : %s\n' "$OUT"

if [ "$ran" -ne "$ITEMS" ]; then
  echo "LAKE DOWNLOAD GATE: FAILED — $ran of $ITEMS items reported a result; the rest never ran" >&2
  exit 1
fi
if [ "$failed" -ne 0 ]; then
  echo "LAKE DOWNLOAD GATE: FAILED ($failed of $ITEMS)" >&2
  exit 1
fi

if [ "$TEMPORARY" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
  rm -rf "$OUT"
fi
echo
# The verdict carries the caveat, the way `tools/deps-docs-gate.sh` carries
# "branch 2 not exercised": whoever reads only the last line of a green run has
# to be told which of the two worlds produced it.
if [ "$HAVE_RELEASE" -eq 1 ]; then
  echo "LAKE DOWNLOAD GATE: ok"
else
  echo "LAKE DOWNLOAD GATE: ok (items 1 and 2 not exercised — no release carries v$VERSION, so the download and the cache were not run)"
fi
