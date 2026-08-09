#!/usr/bin/env bash
# セッションを自己リフレッシュする (Claude が自分の判断で呼ぶ)。
#
#   1. sentinel (.resume-pending) を立てる
#        → 直後の /clear で SessionStart フックが自走継続の文脈を注入する
#   2. tmux 内なら /clear → /carryon をペインへ順に送る
#        - /clear だけでは新ターンが始まらない (SessionStart 注入は文脈追加のみで
#          turn を起動しない)。turn を起動する kick として /carryon を続けて送る。
#        - 現ターンが終わって prompt が idle に戻ってから発火させる (sleep)。
#      tmux 外なら sentinel だけ立て、手動 /clear を促す。
#
# 呼び出し元:
#   - Claude (carryon スキルの自走ループ) が Bash ツールで実行 → $TMUX_PANE を継承
#   - tmux keybind (任意) が `#{pane_id}` を $1 で渡して実行
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$(dirname "$HOOK_DIR")"
touch "$CLAUDE_DIR/.resume-pending"

# 対象ペイン: 明示引数優先、無ければ継承した $TMUX_PANE
PANE="${1:-${TMUX_PANE:-}}"

if [ -n "${TMUX:-}" ] && [ -n "$PANE" ] && command -v tmux >/dev/null 2>&1; then
  # detach + fd を閉じて親 (Bash ツール) をブロックしない。
  ( sleep 3; tmux send-keys -t "$PANE" '/clear' Enter; \
    sleep 3; tmux send-keys -t "$PANE" '/carryon' Enter ) >/dev/null 2>&1 &
  disown 2>/dev/null || true
  echo "REFRESH_SCHEDULED"
else
  echo "REFRESH_PENDING_MANUAL_CLEAR"
fi
