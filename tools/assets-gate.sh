#!/usr/bin/env bash
# Is the site's TypeScript sound? — the half of `app.js` that `cargo` cannot see.
#
# `crates/litedoc4-render/build.rs` bundles `web/src` into cargo's `OUT_DIR` on
# every build, so a **syntax** error already fails `cargo build`. Everything
# else about that code is invisible to Rust: types, lint, format, and the tests
# over the index reader, the ranking and the theme. This is where those run.
# (The count is printed rather than written here — a number in a comment is a
# number that goes stale silently.)
#
# A gate rather than a test, by CLAUDE.md's boundary: it needs node, which
# `cargo test --workspace` must not.
#
# Four stages, and **each one has to be able to fail on its own** — a gate whose
# stages all fail the same way is one stage wearing four hats. The stage that
# failed is named on the way out, and the counts are printed rather than
# assumed: "vitest ran" and "vitest ran something" are different claims, and
# this project has been bitten by the difference (`docs/plans/quality-gates.md`
# Q6, where `cargo test --exact` matched nothing and exited 0).
#
# usage: assets-gate.sh [--json FILE]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB="$(cd "$HERE/../crates/litedoc4-render/web" && pwd)"
JSON=""

while [ $# -gt 0 ]; do
  case "$1" in
    --json) JSON="${2:?--json needs a path}"; shift 2 ;;
    *) echo "usage: assets-gate.sh [--json FILE]" >&2; exit 2 ;;
  esac
done

command -v npm >/dev/null 2>&1 || {
  echo "no npm on PATH — this gate builds and tests the site's TypeScript." >&2
  echo "The version this tree pins is in mise.toml; \`mise install\` puts it on PATH." >&2
  exit 2
}

cd "$WEB"

# One version, named in two kinds of file. `mise.toml` is what a developer's
# shell reads and `node-version:` is what the runners read, and nothing makes
# them agree — so this does, here rather than in a workflow, because a check
# that only runs on a runner cannot say the runner is wrong.
ROOT="$(cd "$HERE/.." && pwd)"
PINNED="$(sed -n 's/^node[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/mise.toml")"
[ -n "$PINNED" ] || { echo "mise.toml names no node version" >&2; exit 1; }
DISAGREE="$(grep -rn 'node-version:' "$ROOT/.github/workflows" "$ROOT/action.yml" \
  | grep -v "\"$PINNED\"" || true)"
if [ -n "$DISAGREE" ]; then
  echo "mise.toml pins node $PINNED and these do not:" >&2
  echo "$DISAGREE" >&2
  exit 1
fi
echo "== node $PINNED, in mise.toml and $(grep -rc 'node-version:' "$ROOT/.github/workflows" "$ROOT/action.yml" | awk -F: '{n+=$2} END{print n}') workflow steps"


# `npm ci` and not `npm install`: the lockfile is the version everything here
# was tested against. Only when there is nothing installed — on a CI runner
# there never is, and locally it would be a slow no-op.
if [ ! -d node_modules ]; then
  echo "== install"
  npm ci --no-audit --no-fund
fi

echo "== lint and format (biome)"
npx biome ci .

echo "== types (tsc, two projects: the page's code and node's)"
npx tsc -p tsconfig.json
npx tsc -p tsconfig.node.json

echo "== tests (vitest)"
# A full template, not `mktemp -t <prefix>`: BSD mktemp appends the random part
# to a prefix, GNU mktemp demands the X's be in the template and fails with
# "too few X's" — which is how this first ran red on ubuntu 【実測 2026-08-19】.
REPORT="$(mktemp "${TMPDIR:-/tmp}/assets-gate-vitest.XXXXXX")"
# The exit code has to come from vitest and not from a pipe or a `tee`
# (CLAUDE.md: "パイプを噛ませた瞬間、見ている終了コードは最後のコマンドのもの").
npx vitest run --reporter=json --outputFile="$REPORT" >/dev/null
TESTS="$(python3 -c "
import json,sys
with open(sys.argv[1]) as f: r = json.load(f)
print(r.get('numTotalTests', 0), r.get('numPassedTests', 0), r.get('numFailedTests', 0))
" "$REPORT")"
rm -f "$REPORT"
read -r TOTAL PASSED FAILED <<<"$TESTS"

# vitest already fails on zero test files. This says the number out loud anyway:
# the failure this guards against is a suite that shrank to nothing without
# anyone noticing, which reads exactly like a suite that passed.
if [ "$TOTAL" -lt 1 ]; then
  echo "vitest reported $TOTAL tests. A green suite that ran nothing is not green." >&2
  exit 1
fi
echo "   $PASSED/$TOTAL passed, $FAILED failed"

echo "== bundle (vite)"
rm -rf dist
npm run build >/dev/null
[ -f dist/app.js ] || { echo "vite wrote no dist/app.js" >&2; exit 1; }
[ -f dist/theme-boot.js ] || { echo "vite wrote no dist/theme-boot.js" >&2; exit 1; }
BYTES="$(wc -c < dist/app.js | tr -d ' ')"
GZIP="$(gzip -c dist/app.js | wc -c | tr -d ' ')"
BOOT="$(wc -c < dist/theme-boot.js | tr -d ' ')"
# The second number is per *page*, not per site: `frame.rs` inlines it into
# every `<head>`, so it is the one that multiplies.
echo "   dist/app.js $BYTES B, gzip $GZIP B; dist/theme-boot.js $BOOT B (inlined per page)"

# The bundle is not committed (docs/plans/assets-typescript.md 決定 1), so this
# is a build, not a comparison. `dist/` is the by-hand output path; the one that
# reaches the binary is cargo's OUT_DIR, written by build.rs from these sources.
rm -rf dist

if [ -n "$JSON" ]; then
  printf '{"tests":%s,"passed":%s,"failed":%s,"bundleBytes":%s,"bundleGzip":%s,"bootBytes":%s}\n' \
    "$TOTAL" "$PASSED" "$FAILED" "$BYTES" "$GZIP" "$BOOT" > "$JSON"
fi

echo
echo "ASSETS GATE: ok"
