---
name: relay
description: タスクを context 上限に阻まれず完遂まで無人で自走する。1 セッション（leg）が重くなる前に handoff → 新 tmux セッション起動 → carryon で引き継ぎ、を完遂まで自動で連鎖する。ユーザーが「完遂まで自走して」「最後まで自走して完遂して」「無人で最後までやって」「セッションをまたいで完遂して」「リレーして」「/relay」と言ったときに起動する。鍵語は「完遂まで/最後まで/無人で/セッションをまたいで」。単なる「自走して」では起動しない（carryon の通常自走と区別）。
---

# relay: セッション連鎖でタスクを完遂まで自走する

長いタスクを context 上限に阻まれず**無人で完遂**するための連鎖。1 セッション（leg）が重くなる前に handoff → 新 tmux セッション起動 → carryon で引き継ぎ、を完遂まで自動で繰り返す。ユーザーは初回にゴールを与えるだけ。設計の根拠は `.claude/relay-design.md`。

**これは自己増殖ループ**なので暴走防止が生命線。leg 上限・無進捗検知・各 leg 最低 1 commit を**ハード制約**として守る（→ guardrail）。

## トリガー

relay を carryon の通常自走から分ける鍵語は **「完遂まで / 最後まで / 無人で / セッションをまたいで」**。ただの「自走して」では起動しない（carryon の通常挙動と紛れるため）。

- 「<タスク> を完遂まで自走して」「最後まで自走して完遂して」
- 「無人で最後までやって」「放っておいても完遂して」
- 「セッションをまたいで完遂して」「リレーして」「何セッションかけても完遂して」
- `/relay <タスク> [cap=N]`（cap 省略時は 8）

## 起動時の分岐（初回 leg か継続 leg か）

`.claude/handoff.md` を見て分岐する:

- **継続 leg** — handoff.md に `## Relay control` があり `Mode: ON` → 前 leg が起こした継続。下の「leg のライフサイクル」を step 1（carryon 復元）から実行。
- **初回 leg** — ユーザーが `/relay <タスク>` 等で明示起動（活きた Relay control が無い）→ 現セッションで leg 1 として直接着手。`Skill(handoff)` を呼ばずとも、まず Relay control ブロックの中身を用意（`Leg: 1 / cap K`、`Predecessor: none`、`Goal: <タスク>`、空の進捗台帳）し、ライフサイクル step 3（自走）から入る。carryon は不要（ゴールはユーザーが今くれた）。

## leg のライフサイクル

1. **(継続 leg のみ) carryon で状態復元** — `Skill(carryon)` を実行。handoff.md 読込 / Task 復元 / git 確認 / 計測環境の確認 / next step 宣言 / 自走開始まで carryon に任せる（**復元ロジックの SoT は carryon、relay は再実装しない**）。
2. **(継続 leg のみ) 前任を kill** — carryon が走り出し作業を始められると確認できたら、Relay control の `Predecessor` が relay 起動の named tmux セッションなら掃除する:
   ```bash
   tmux kill-session -t <predecessor>
   ```
   `Predecessor: none`（leg 1 = ユーザーの元セッションで tmux 名を持たない）なら kill しない。
3. **オーケストレーターとして自走** — carryon のデフォルト挙動（subagent dispatch / 検証 / commit / push、CLAUDE.md のオーケストレーター規律）を継承して作業を進める。
4. **stop / refresh の判定** — 下の「carryon stop の読み替え」に従って分岐:
   - 完遂 / 要ユーザー判断 / 無進捗 / leg 上限 → **終端**（DONE/PAUSED/ABORTED、新 leg を起こさず通知）
   - context 圧迫 / malformed 2 度目 → **refresh**（次 leg へ継投）

## carryon stop の読み替え（最重要・override の本体）

relay leg は carryon の自走をそのまま使うが、carryon の 3 停止条件のうち **#3 だけ振る舞いを差し替える**。

- **#1 タスク完了** → そのまま終端。`Relay control` を `Mode: DONE` + サマリに更新 → 通知 → 終了。
- **#2 要ユーザー判断** → そのまま終端。`Mode: PAUSED` + 何を聞きたいかを書く → 通知 → 終了。**新 leg を起こさない**（fresh セッションに渡しても同じ所で詰まる）。無人運用なので `AskUserQuestion` でブロックせず通知で起こす。
- **#3 context 圧迫** → **終端しない**。carryon の当該指示文（逐語）:
  > 3. **コンテキスト圧迫で判断精度の低下が懸念される** — 会話が長大化し、自分の判断の正確さが落ちるリスクを感じたら、無理に続けず `handoff` スキルで状態を書き出してリフレッシュを促す (次セッションが `/carryon` で拾える形にする)。

  この分岐の『ユーザーにリフレッシュを促して止まる』を**取らない**。代わりに下の refresh で**自分で次 leg を起こす**。これが relay の自動化そのもの。

加えて relay 固有のシグナル:

- **malformed tool call 2 度目**（CLAUDE.md interrupt trigger）→ 高 context の確信シグナル。『handoff + セッション終了してユーザーに promote』ではなく **refresh（次 leg 自動起動）** を取る。

## refresh（次 leg への継投）手順

context 圧迫 / malformed 2 度目を感じたら:

1. **成果を commit/push** — 未コミットの作業を commit & push（tree をクリーンに = 次 leg の carryon git 整合を保つ）。
2. **進捗台帳に追記** — Relay control の Progress ledger に `r<N>: <成果 + commit hash>`。
3. **無進捗チェック** — 直近 2 leg（自分含む）が測定可能な進捗ゼロ（新 commit なし / 完了タスクなし / 新しい計測結果なし）なら **refresh せず `Mode: ABORTED` 終端** → 通知。
4. **leg 上限チェック** — `N+1 > cap` なら **refresh せず `Mode: PAUSED` 終端**（「leg 上限到達、続けるか？」）→ 通知。
5. **handoff 上書き** — `Skill(handoff)` で `.claude/handoff.md` を上書き。Relay control を更新（`Leg: N+1 / cap K`、`Predecessor: <自分の session 名 or none>`、`Mode: ON`、台帳）。
6. **次 leg を起動**（`tmux-new` の起動機構を踏襲）— 名前は `<goal>-r<N+1>`（短い英語ハイフン名 + leg 番号、tmux 名 = claude 名）。**送るトリガは `/carryon` でなく `/relay`**:
   ```bash
   .claude/skills/relay/relay-launch-leg.sh <goal>-r<N+1> <project-dir>
   # 起動完了（⏵⏵ auto mode on + /rc active）を capture-pane で確認後:
   tmux send-keys -t <goal>-r<N+1> '/relay' Enter
   ```
7. **後続の走り出しを確認** — capture-pane で次 leg が `/relay` → carryon 復元を始めたのを確認。
8. **自 leg は idle 化** — 以降ツール呼び出しをしない（git 競合回避）。後続が step 2 で自分を kill する。

## guardrail（暴走防止・ハード制約）

完全自走なので以下は曲げない:

- **leg 上限**: 既定 8（起動時 `cap` で変更可）。超過で PAUSED 終端。
- **無進捗検知**: 2 leg 連続で成果ゼロ → ABORTED 終端。
- **各 leg 最低 1 commit**: 成果を残せないなら理由を台帳に明記（無進捗検知の根拠になる）。
- **終端は必ず通知**: DONE/PAUSED/ABORTED いずれも PushNotification（無人運用なので push で拾えるように）。
- 迷ったら**止まって通知する側に倒す**。新 leg を起こし続けるより、PAUSED で人に返す方が安全。

## handoff.md の Relay control ブロック

handoff の通常状態に加え、relay 連鎖中は必ずこのセクションを入れる（handoff スキルのテンプレートにも記載）:

```
## Relay control
- Mode: ON | DONE | PAUSED | ABORTED
- Goal: <完遂すべきゴール>
- Leg: N / cap K
- Predecessor: <session 名 or none>   # 後続が起動確認後に kill
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: <成果 / commit hash>
  - r2: <成果 / commit hash>
```

`Mode: ON` は override のリマインダも兼ねる — 各 leg は起動時にこれを再読込し、「stop #3 は refresh に読み替える」を context だけでなくファイルからも再注入する（context 劣化に対する二重化）。

## セッション命名と前任掃除

- 名前: `<goal>-r<N>`（例 `footprint-r2`, `footprint-r3`）。tmux 名 = claude 名。
- **後続が前任を kill**: 親が子起動直後に親を殺すと子起動失敗で連鎖が死ぬ。なので「子が走り出し確認後に親を kill」（ライフサイクル step 2）。常時アクティブ 1・idle 高々 1。
- 親（現 leg）は子起動後**即 idle**。観察したいユーザーはリモートアプリで最新 `-r<N>` を選ぶだけ（切替不要、tmux-new 準拠）。

## 終端通知

DONE/PAUSED/ABORTED いずれも:

- `## Relay control` を終端状態 + 1〜2 行サマリに更新。
- **PushNotification** でユーザーに通知（完了 / 要判断 / 異常を無人でも拾えるように）。
- 新 leg は起こさない。

## 原則

- **暴走防止が最優先**: 完全自走なので guardrail（leg 上限・無進捗・最低 1 commit・終端通知）を守る。
- **常にアクティブは 1 leg**: 逐次実行。後続を起こしたら即 idle。git 競合を作らない。
- **carryon を再実装しない**: 状態復元は `Skill(carryon)` に委ねる。relay が足すのは stop #3 の読み替えと refresh の継投だけ。
- 設計の根拠 / carryon 停止条件との対応 / 合成メカニズムは `.claude/relay-design.md`。

`handoff` / `carryon` / `tmux-new` を駆動する上位スキル。
