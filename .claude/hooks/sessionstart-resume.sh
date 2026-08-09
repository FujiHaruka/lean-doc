#!/usr/bin/env bash
# SessionStart(matcher=clear) hook.
#
# Claude が自己リフレッシュ (claude-refresh.sh) した直後の /clear のときだけ、
# 新セッションへ「自己リフレッシュした、自走を継続せよ」を additionalContext
# として注入する。素の /clear (sentinel 無し) では何も出力せず通常挙動を壊さない。
#
# sentinel は claude-refresh.sh が touch する。
set -euo pipefail

# .claude ディレクトリをスクリプト位置から解決 (.claude/hooks/<this>)
HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$(dirname "$HOOK_DIR")"
SENTINEL="$CLAUDE_DIR/.resume-pending"

[ -f "$SENTINEL" ] || exit 0
rm -f "$SENTINEL"

cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"このセッションはあなた自身が context リフレッシュのために自己リセットしたものです (ユーザー操作ではありません)。直後に /carryon が走り前作業を再開します。carryon スキルに従い `.claude/handoff.md` から自走を継続してください。ユーザーへの確認は不要。停止するのは『タスク完了』『ユーザー判断が要る論点』『同じ壁で進捗が出ない』のいずれかに達したときだけです。"}}
JSON
