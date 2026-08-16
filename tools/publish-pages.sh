#!/usr/bin/env bash
# Publish a site tree built by `lean-doc build` to a repository's Pages branch.
#
# The tree is published *as the whole branch*: the branch content is replaced,
# not merged into. That is the only way a removed module's page stops being
# served, and it is why this script clones into a scratch directory instead of
# touching the target repository's working tree.
#
# It does not push unless told to. The default run clones, stages the
# replacement and prints what would change — publishing is an outward-facing,
# hard-to-take-back action and it should not be the thing that happens when a
# flag is forgotten.
#
# usage:
#   tools/publish-pages.sh --site <dir> --repo <path-or-url> [options]
#
#   --site <dir>       the site tree to publish (as `lean-doc build` wrote it)
#   --repo <path|url>  the repository that owns the Pages branch
#   --branch <name>    branch to replace (default: gh-pages)
#   --message <text>   commit message (default: a generated one)
#   --push             actually push. Without it, nothing leaves the machine
#   --keep             leave the scratch clone in place for inspection
set -euo pipefail

SITE=""
REPO=""
BRANCH="gh-pages"
MESSAGE=""
PUSH=0
KEEP=0

die() { echo "publish-pages: $*" >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --site)    SITE="${2:?--site needs a directory}"; shift 2 ;;
    --repo)    REPO="${2:?--repo needs a path or url}"; shift 2 ;;
    --branch)  BRANCH="${2:?--branch needs a name}"; shift 2 ;;
    --message) MESSAGE="${2:?--message needs text}"; shift 2 ;;
    --push)    PUSH=1; shift ;;
    --keep)    KEEP=1; shift ;;
    -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
    *)         die "unknown argument \`$1\`" ;;
  esac
done

[ -n "$SITE" ] || die "--site is required"
[ -n "$REPO" ] || die "--repo is required"
[ -d "$SITE" ] || die "no such directory: $SITE"

# A site without an entry point is a site nobody can open, and publishing one
# would replace a working branch with a broken one.
for required in index.html style.css app.js; do
  [ -f "$SITE/$required" ] || die "$SITE has no $required — is this a finished build?"
done

PAGES=$(find "$SITE" -name '*.html' | wc -l | tr -d ' ')
FILES=$(find "$SITE" -type f | wc -l | tr -d ' ')
BYTES=$(du -sk "$SITE" | cut -f1)
echo "site    $FILES file(s), $PAGES page(s), ${BYTES} KiB"

WORK=$(mktemp -d "${TMPDIR:-/tmp}/lean-doc-pages.XXXXXX")
cleanup() { [ "$KEEP" -eq 1 ] || rm -rf "$WORK"; }
trap cleanup EXIT

# `--single-branch` keeps the clone to the branch being replaced; the history of
# a documentation branch is not interesting and can be very large.
if git ls-remote --exit-code --heads "$REPO" "$BRANCH" >/dev/null 2>&1; then
  git clone --quiet --single-branch --branch "$BRANCH" "$REPO" "$WORK/repo"
  echo "branch  $BRANCH exists, replacing its content"
else
  git clone --quiet --no-checkout "$REPO" "$WORK/repo"
  git -C "$WORK/repo" checkout --quiet --orphan "$BRANCH"
  git -C "$WORK/repo" rm -rq --cached . 2>/dev/null || true
  echo "branch  $BRANCH does not exist, creating it"
fi

# Everything tracked goes, then the tree lands. Deleting first is what removes
# files the new build no longer produces — a merge would serve them forever.
git -C "$WORK/repo" rm -rq --ignore-unmatch . >/dev/null 2>&1 || true
find "$WORK/repo" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +

cp -R "$SITE"/. "$WORK/repo"/
# Without this, GitHub Pages runs the tree through Jekyll, which drops every
# path whose name starts with an underscore.
touch "$WORK/repo/.nojekyll"

git -C "$WORK/repo" add .
if git -C "$WORK/repo" diff --cached --quiet; then
  echo "result  no change — the published branch already has this tree"
  exit 0
fi

echo "staged  $(git -C "$WORK/repo" diff --cached --numstat | wc -l | tr -d ' ') path(s) changed"
git -C "$WORK/repo" diff --cached --stat | tail -1

if [ "$PUSH" -ne 1 ]; then
  echo "result  dry run. Re-run with --push to publish."
  [ "$KEEP" -eq 1 ] && echo "clone   $WORK/repo"
  exit 0
fi

git -C "$WORK/repo" commit --quiet -m "${MESSAGE:-lean-doc で生成した API ドキュメント}"
git -C "$WORK/repo" push --quiet origin "$BRANCH"
echo "result  pushed $(git -C "$WORK/repo" rev-parse --short HEAD) to $BRANCH"
