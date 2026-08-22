#!/usr/bin/env bash
# The shape no fixture had: a **version-pinnable dependency whose module names
# need guillemets**.
#
# `e2e/micro-dep` is required by *path*, so its manifest entry has no `url` and
# no `rev`, so `crates/litedoc4/src/packages.rs` cannot build a `/blob/<rev>`
# for it, so every reference to it renders as text with no link. That branch is
# checked by `tools/e2e-micro.sh`. What it cannot check is the other half: when
# the dependency *can* be pinned, the guillemets have to come off on the way
# into the URL, and the three spellings a docstring can use for the same module
# have to agree about where they point. `e2e/README.md` said so and called the
# instance missing.
#
# **Building it is what showed that they did not agree.** `Dep-Aux.Basic` — the
# `.lidx`\'s spelling — resolved nowhere, and only a *pinnable* dependency can
# make that visible: with an unpinnable one all three spellings render as plain
# text, so they agree by accident.
#
# It was not missing for want of hardware. The same `e2e/micro-dep` is the
# fixture; only the **wiring** changes, which is what makes the comparison worth
# anything — the modules, the docstrings and the toolchain are identical to the
# path-required run, and pinnability is the only variable.
#
# # Two things this script does that are not decoration
#
#   the git remote     Lake clones from the `url` it writes into the manifest,
#                      so a `file://` remote would put `file://` into every blob
#                      URL. That is not the shape under test, and it also makes
#                      `tools/site-gate.sh` call those links *internal* and
#                      report five dead ones — a failure of the harness that
#                      reads exactly like a failure of the product. `git`'s
#                      `insteadOf` is used instead: the manifest carries
#                      `https://example.invalid/micro-dep` and the clone still
#                      happens offline, from a repository this script builds.
#
#   the commit         made with fixed author, committer and dates, so the rev
#                      is a function of `e2e/micro-dep`'s contents. Nothing
#                      pins the hash — the gate reads it back out of the
#                      manifest and requires *that* to be what it built.
#
# usage: pinned-dep-gate.sh [--out DIR] [--keep]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
LITEDOC4="${LITEDOC4:-$ROOT/target/debug/litedoc4}"

# Not a real host, on purpose: `.invalid` is reserved (RFC 2606) so nothing can
# ever resolve it, and the gate never fetches these URLs — it compares them.
DEP_URL="https://example.invalid/micro-dep"
SELF_URL="https://example.invalid/micro/blob/0000000000000000000000000000000000000000"

OUT=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    -h|--help) sed -n '1,36p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

command -v "$LAKE" >/dev/null 2>&1 || { echo "no lake at $LAKE — set LAKE" >&2; exit 2; }
[ -x "$LITEDOC4" ] || { echo "no litedoc4 at $LITEDOC4 — cargo build --bin litedoc4" >&2; exit 2; }
command -v git >/dev/null 2>&1 || { echo "no git" >&2; exit 2; }

if [ -z "$OUT" ]; then
  OUT="$(mktemp -d)"
  TEMPORARY=1
else
  mkdir -p "$OUT"
  TEMPORARY=0
fi

say() { printf '\n=== %s\n' "$1"; }
fail() { echo "  FAIL  $1" >&2; FAILED=$((FAILED + 1)); }
FAILED=0

say "1/6 build a git repository out of e2e/micro-dep"
rm -rf "$OUT/dep"
cp -R "$ROOT/e2e/micro-dep" "$OUT/dep"
rm -rf "$OUT/dep/.lake"
(
  cd "$OUT/dep"
  export GIT_AUTHOR_NAME=litedoc4 GIT_AUTHOR_EMAIL=gate@litedoc4.invalid
  export GIT_COMMITTER_NAME=litedoc4 GIT_COMMITTER_EMAIL=gate@litedoc4.invalid
  export GIT_AUTHOR_DATE="2026-01-01T00:00:00+0000"
  export GIT_COMMITTER_DATE="2026-01-01T00:00:00+0000"
  git init -q -b main .
  git add -A
  git commit -q -m "e2e/micro-dep as a pinnable dependency"
)
REV="$(cd "$OUT/dep" && git rev-parse HEAD)"
echo "  rev $REV"

say "2/6 rewire e2e/micro to require it by git"
rm -rf "$OUT/micro"
cp -R "$ROOT/e2e/micro" "$OUT/micro"
# The manifest has to be *written by Lake*, not copied: a hand-edited one would
# make this gate assert its own input.
rm -rf "$OUT/micro/.lake" "$OUT/micro/lake-manifest.json"
python3 - "$OUT/micro/lakefile.toml" "$DEP_URL" "$REV" <<'PY'
import pathlib, sys
path, url, rev = sys.argv[1], sys.argv[2], sys.argv[3]
p = pathlib.Path(path)
s = p.read_text()
old = '[[require]]\nname = "«micro-dep»"\npath = "../micro-dep"'
if old not in s:
    raise SystemExit(
        f"{path}: the path require this gate rewires is gone — e2e/micro no longer "
        "requires «micro-dep» by path, so the comparison this gate makes has lost "
        "its other half"
    )
p.write_text(s.replace(old, f'[[require]]\nname = "«micro-dep»"\ngit = "{url}"\nrev = "{rev}"'))
PY

# Offline: git rewrites the remote, Lake never learns about it, and the manifest
# keeps the URL the blob links have to be built from.
export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0="url.file://$OUT/dep.insteadOf"
export GIT_CONFIG_VALUE_0="$DEP_URL"
(cd "$OUT/micro" && "$LAKE" update)
(cd "$OUT/micro" && "$LAKE" build)

say "3/6 GATE 1 — the manifest really is the shape under test"
# "測った" の前に "測れる状態か": if this entry is not git-with-a-rev then every
# assertion below is about the *unpinnable* branch again and passes for the
# wrong reason.
python3 - "$OUT/micro/lake-manifest.json" "$DEP_URL" "$REV" <<'PY' || FAILED=1
import json, pathlib, re, sys
manifest, url, rev = sys.argv[1], sys.argv[2], sys.argv[3]
packages = json.loads(pathlib.Path(manifest).read_text())["packages"]
entries = [p for p in packages if p.get("name") == "«micro-dep»"]
if not entries:
    raise SystemExit(f"  FAIL  no «micro-dep» entry in {manifest}: {[p.get('name') for p in packages]}")
entry = entries[0]
problems = []
if entry.get("type") != "git":
    problems.append(f"type is {entry.get('type')!r}, not 'git' — this is the unpinnable branch again")
if entry.get("url") != url:
    problems.append(f"url is {entry.get('url')!r}, not {url!r}")
if not re.fullmatch(r"[0-9a-f]{40}", entry.get("rev") or ""):
    problems.append(f"rev is {entry.get('rev')!r}, not 40 hex digits")
elif entry["rev"] != rev:
    problems.append(f"rev is {entry['rev']} but the repository was built at {rev}")
if "«" not in entry.get("name", ""):
    problems.append(f"name is {entry.get('name')!r} — the guillemets are the point")
for problem in problems:
    print(f"  FAIL  {problem}")
raise SystemExit(1 if problems else 0)
PY
[ "$FAILED" = 1 ] && { echo "the fixture is not the shape under test; nothing below would mean anything" >&2; exit 1; }
echo "  ok  git + 40-hex rev + «micro-dep»"

say "4/6 build the extractor and the site"
mkdir -p "$OUT/micro/.lake/e2e-extract"
(cd "$OUT/micro" && "$LAKE" env lean --root="$ROOT/extractor" \
  -o "$OUT/micro/.lake/e2e-extract/Extract.olean" \
  -c "$OUT/micro/.lake/e2e-extract/Extract.c" \
  "$ROOT/extractor/Extract.lean")
(cd "$OUT/micro" && "$LAKE" env leanc -rdynamic \
  -o "$OUT/micro/.lake/e2e-extract/extract" "$OUT/micro/.lake/e2e-extract/Extract.c")
rm -rf "$OUT/site"
"$LITEDOC4" build --root "$OUT/micro" --lib Micro --out "$OUT/site" \
  --source-url "$SELF_URL" \
  --extractor-bin "$OUT/micro/.lake/e2e-extract/extract" > "$OUT/build.log"

say "5/6 GATE 2 — the guillemets come off on the way into the URL"
python3 - "$OUT/site/site/Micro/Dep.html" "$DEP_URL" "$REV" <<'PY' || FAILED=$((FAILED + 1))
import pathlib, re, sys
page, url, rev = sys.argv[1], sys.argv[2], sys.argv[3]
html = pathlib.Path(page).read_text()
want = f"{url}/blob/{rev}/Dep-Aux/Basic.lean"

def linked(spelling):
    return re.findall(r'<a href="([^"]+)"[^>]*>' + re.escape(spelling) + r"</a>", html)

problems = []
# The two spellings that resolve, and the one number that says the guillemets
# were handled: a `«` anywhere in a path is a link nobody can follow.
for spelling in ("«Dep-Aux».Basic", "Dep-Aux/Basic.lean"):
    hrefs = set(linked(spelling))
    if not hrefs:
        problems.append(f"{spelling}: no link at all — a pinnable dependency has a blob URL")
    elif hrefs != {want}:
        problems.append(f"{spelling}: links to {sorted(hrefs)}, wanted {want}")
if "«" in html.split("<body")[0] + "".join(re.findall(r'href="([^"]*)"', html)):
    problems.append("a href still carries a guillemet")

# **The third spelling, and all three at the same URL**【決定 2026-08-22、
# ユーザー判断】. The `.lidx` writes module names unescaped and the IR does not,
# so `Dep-Aux.Basic` is a third way to name the same module. It used to get no
# link at all — not because the map lacked it, but because it is not a Lean name
# literal and never reached a lookup. `NameIndex::module_for_unescaped` is the
# answer, and what is asserted here is that the three *agree*, not merely that
# each of them resolves.
hrefs = set(linked("Dep-Aux.Basic"))
if not hrefs:
    problems.append(
        "Dep-Aux.Basic: no link. This is the .lidx's spelling of a module a pinnable "
        "dependency owns, and it resolves since residual-sweep R3 — see "
        "NameIndex::module_for_unescaped"
    )
elif hrefs != {want}:
    problems.append(f"Dep-Aux.Basic: links to {sorted(hrefs)}, wanted {want}")

for problem in problems:
    print(f"  FAIL  {problem}")
raise SystemExit(1 if problems else 0)
PY
[ "$FAILED" -eq 0 ] && echo "  ok  blob URLs carry no guillemets; all 3 spellings resolve, to the same URL"

say "6/6 GATE 3 — the site is still closed over itself"
"$HERE/site-gate.sh" "$OUT/site/site" || FAILED=$((FAILED + 1))

echo
if [ "$FAILED" -ne 0 ]; then
  echo "PINNED DEP GATE: FAILED ($FAILED check(s))" >&2
  echo "  Work kept at $OUT" >&2
  exit 1
fi
echo "PINNED DEP GATE: ok"
if [ "$TEMPORARY" = 1 ] && [ "$KEEP" = 0 ]; then
  rm -rf "$OUT"
else
  echo "  work kept at $OUT"
fi
