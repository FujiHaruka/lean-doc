#!/usr/bin/env bash
# PreToolUse(Bash) gate: block `rg` invocations that use the short -r flag.
#
# Why: rg's -r is --replace (it consumes a value). The grep habit of -r=recursive
# silently becomes --replace under rg, and bundled forms (-rn, -ril) eat the
# following flags into the replacement string with NO error — the output looks
# plausible while -n/-i/-l were never applied. See project CLAUDE.md
# "⚠️ rg -rn フットガン". rg is recursive by default, so -r is never needed for that.
#
# Detection: split the command into pipeline/list segments, and within any segment
# whose command word is `rg` (or */rg), flag a single-dash cluster containing `r`
# (^-[A-Za-z]*r[A-Za-z]*$). The long form --replace and grep -rn are left alone.
cmd="$(jq -r '.tool_input.command // empty' 2>/dev/null)"
[ -z "$cmd" ] && exit 0

hit="$(printf '%s\n' "$cmd" | awk '
{
  # Mask quoted strings first so a search PATTERN (which may itself contain
  # `|` alternations or the text "rg -rn") never participates in detection.
  line = $0
  gsub(/"[^"]*"/, " ", line)
  gsub(/\047[^\047]*\047/, " ", line)
  n = split(line, seg, /[|;&]/)
  for (i = 1; i <= n; i++) {
    m = split(seg[i], t, /[ \t]+/)
    seen = 0
    for (j = 1; j <= m; j++) {
      if (seen && t[j] ~ /^-[A-Za-z]*r[A-Za-z]*$/) { print "BLOCK"; exit }
      if (t[j] == "rg" || t[j] ~ /\/rg$/) seen = 1
    }
  }
}')"

if [ "$hit" = "BLOCK" ]; then
  cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"rg の短縮 -r は --replace(値を取る)。-rn / -ril のように束ねると後続フラグが置換文字列に食われ、エラーなく出力が壊れます(grep の -r=recursive の誤適用)。rg は再帰がデフォルトで -r は不要。行番号=-n、大小無視=-i、ファイル名のみ=-l を個別指定してください。本当に置換したい時だけ長形式 --replace を使うこと。"}}
JSON
  exit 0
fi
exit 0
