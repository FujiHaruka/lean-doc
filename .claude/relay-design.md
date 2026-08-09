# relay スキル設計 — セッション連鎖による自律完遂

長いタスクを、context 上限に阻まれず**無人で完遂**するためのスキル設計。1 セッションの context が重くなる前に handoff → 新 tmux セッション起動 → carryon で引き継ぎ、これを完遂まで自動で繰り返す。ユーザーは最初にゴールを与えるだけ。

## 解く問題 / なぜ既存手段で足りないか

- **context 上限**: 単一セッションでは長大タスクを最後まで持てない。判断精度も context 肥大で落ちる（malformed tool call cascade は総 context 量と単調相関 — CLAUDE.md interrupt trigger / memory `pitfall-agent-invoke-malformed`）。
- **auto-compaction では不十分**: ハーネスの自動要約は同一セッション・同一プロセス内の lossy 圧縮。fresh プロセスにならないので特殊トークン忠実度の劣化はリセットされない。curated な handoff の方が状態の質も高い。
- **`/loop` / cron では不十分**: 同一セッションで context が累積する。relay の眼目は「leg ごとに fresh プロセス」。
- 既存 3 スキル（handoff / carryon / tmux-new）は揃っているが、「context が埋まったら自分で次セッションを起こす」最後の自動化が無い。carryon の停止条件 3 は今『ユーザーに promote して止まる』。relay はそこを自動化する。

## Approach（解の形）

relay は **新規ロジックを足すのではなく、既存 3 スキルを駆動する薄いオーケストレーション層**。1 つの relay leg = carryon で状態復元 → オーケストレーターとして自走 → refresh トリガで handoff + 次 leg 起動 → 自セッションは idle 化（後続に kill される）。これを「完遂 / 要ユーザー判断 / 無進捗 / leg 上限」のいずれかに当たるまで連鎖。

設計の要は 3 点:

1. **逐次実行（並列でない）**: 常にアクティブな leg は 1 つ。同一作業ディレクトリを共有するので、後続を起こしたら現 leg は即 idle 化して git 競合（`.git/index.lock` / 二重 commit）を避ける。worktree 不要。
2. **handoff.md に relay 制御ブロックを載せる**: ゴール / leg 番号 / 進捗台帳 / 停止条件 / 前任セッション名。これが連鎖の状態キャリア。
3. **暴走防止が第一級の関心事**: leg 上限 + 無進捗検知 + 各 leg に最低 1 つの具体成果（commit）を要求。

## relay ループ（状態機械）

```
 ┌──────────────────────── leg N ────────────────────────┐
 │ 1. carryon で状態復元 (relay ブロック読込)              │
 │ 2. 前任 (predecessor) を kill ※後続が起動確認後に行う   │
 │ 3. オーケストレーターとして自走 (dispatch/検証/commit)  │
 │ 4. refresh トリガ? ──no──▶ 完遂 or 自走継続            │
 │            │yes                                        │
 │  a. 成果を commit/push  b. 進捗台帳に追記              │
 │  c. handoff 上書き (leg N+1, predecessor=自分)         │
 │  d. tmux-new で <goal>-r(N+1) 起動 → /relay 送信       │
 │  e. 後続の走り出しを確認 → 自 leg は idle 化           │
 └───────────────────────────┬───────────────────────────┘
                             ▼  (後続が step 2 で自分を kill)
                          leg N+1 ...

 完遂/停止条件 → handoff を DONE/PAUSED/ABORTED + サマリに更新
              → PushNotification で通知 → 新 leg を起こさず終了
```

### leg の手順

1. **(起動時) carryon で状態復元** — handoff.md を読み、relay ブロックでゴール / leg 番号 / 進捗台帳を把握。
2. **前任 kill** — carryon が走り出し作業開始できると確認できたら、handoff の `predecessor` を `tmux kill-session` で掃除（idle 堆積防止 / 常時アクティブ 1・idle 高々 1）。
3. **自走** — carryon のデフォルト挙動を継承（subagent dispatch / 検証 / commit / push、オーケストレーター規律準拠）。
4. **refresh トリガ到達時**:
   - a. 未コミット成果を commit/push（tree をクリーンに = carryon の git-status 整合を保つ）。
   - b. 進捗台帳に「leg N で何をしたか + commit hash」を追記。
   - c. handoff スキルで relay ブロック込みの handoff.md を**上書き**（leg N+1、predecessor=自分、台帳更新）。
   - d. tmux-new の起動機構で fresh セッション `<goal>-r<N+1>` を起こし **`/relay`** を送る（`/carryon` でなく `/relay` = 連鎖を再武装）。
   - e. 後続の走り出しを capture-pane で確認したら**自 leg は idle 化**（以降ツール呼び出しをしない）。後続が step 2 で自分を kill する。
5. **完遂 / 停止条件到達時** — 連鎖を止め、handoff を `Relay: DONE/PAUSED/ABORTED + サマリ` に更新、PushNotification で通知。新セッションは起こさない。

## refresh トリガ（いつ次 leg へ渡すか）

3 シグナルの OR:

- (a) **context 圧迫の自己判断** — 会話が長大化し判断精度低下が懸念される（carryon 停止条件 3 と同じソフト信号）。
- (b) **malformed tool call interrupt** — 2 度目の malformed（CLAUDE.md interrupt trigger）。高 context の確信シグナルなので、relay モード下では『handoff + セッション終了してユーザーに promote』を『handoff + 次 leg 自動起動』に格上げ。
- (c) **leg あたりステップ予算（任意の backstop）** — ソフト信号が鈍い時の保険。

トリガ前に leg 内で完遂できれば relay を DONE で閉じる。

## 終了 & 暴走防止（最重要）

自己増殖ループなので hard guardrail を第一級に置く:

- **leg 上限** — 既定 8 leg（起動時に変更可）。到達で停止 → 通知 → ユーザー待ち。
- **無進捗検知** — 各 leg は進捗台帳に具体成果（新 commit / 完了タスク / 新しい計測結果 等）を記す。2 leg 連続で測定可能な進捗ゼロなら abort → 通知。
- **各 leg に最低 1 成果** — commit を残せないなら、なぜかを台帳に明示。
- **停止条件（carryon 由来 + 追加）**:
  1. **完遂** — ゴール / プラン目標を満たし残タスク無し → DONE。
  2. **要ユーザー判断** — 方針分岐 / 不可逆操作の承認 / 撤退ライン該当 → PAUSED（新 leg を起こさず通知）。fresh セッションに渡しても同じ所で詰まるので渡さない。
  3. **無進捗 / leg 上限** → ABORTED / PAUSED。

### carryon 停止条件との対応（整合の核心）

relay は carryon のオーケストレーター自走をそのまま継承するので、carryon の 3 停止条件との整合が連鎖の正しさを決める。本質は **#1/#2 は終端としてそのまま継承、#3 だけを「終端停止」から「refresh トリガ（次 leg 自動起動）」へ格上げ**する点。

| carryon 停止条件 | relay での扱い |
|---|---|
| #1 タスク完了 | 終端そのまま → `Relay: DONE`。連鎖停止 + 通知。 |
| #2 要ユーザー判断 | 終端そのまま → `Relay: PAUSED`。新 leg を起こさず通知。無人運用なので `AskUserQuestion` でブロックさせず **PushNotification** で起こす。fresh セッションに渡しても同じ所で詰まるので渡さない。 |
| #3 context 圧迫 | **唯一の差分** → 終端停止しない。relay の refresh トリガ (a) に再分類し、handoff して**次 leg を自動起動**。carryon は『ユーザーに promote して止まる』、relay は『自分で次セッションを起こす』。これが relay の自動化そのもの。 |

relay が追加する終端条件（carryon に無い — carryon は自己 spawn しないので不要だった）:

- **無進捗×2** → ABORTED
- **leg 上限到達** → PAUSED

**実装上の含意**: leg として自走中に carryon stop #3 が発火したら、**プレーン carryon の挙動（止まってユーザーに promote）ではなく relay の refresh（次 leg 起動）を取る**。この override を明示しないと、relay 下のセッションが #3 でただ停止してユーザーに促し、自動化が無効化される。relay スキルは「自分は relay leg として動いている間、stop #3 は refresh トリガに読み替える」を冒頭で宣言する。

## セッション命名 & 前任掃除

- **名前**: `<goal>-r<N>`（短い英語ハイフン名 + leg 番号）。例 `footprint-r2`, `footprint-r3`。tmux 名 = claude 名（tmux-new 準拠）。
- **後続が前任を kill**（successor kills predecessor）— 親が子起動直後に親を殺すと、子起動失敗時に連鎖が死ぬ。なので「子が carryon で走り出し作業開始を確認してから親を kill」。常時アクティブ 1 + idle 高々 1。
- 親（現 leg）は子起動後**即 idle**（git 競合回避）。観察したいユーザーはリモートアプリで最新 `-r<N>` を選ぶだけ（切替不要、tmux-new 準拠）。

## handoff.md の relay 制御ブロック

handoff スキルの通常状態に加え、relay 専用セクション:

```
## Relay control
- Mode: ON | DONE | PAUSED | ABORTED
- Goal: <完遂すべきゴール（1 セッションでは収まらない単位）>
- Leg: N / cap K
- Predecessor: <session-name>     # 後続が起動確認後に kill
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: <成果 / commit hash>
  - r2: <成果 / commit hash>
```

進捗台帳は (1) 無進捗検知の根拠、(2) ユーザーが連鎖を後追い監査する材料。

## トリガー（ユーザー語彙）

relay を carryon の通常自走から分ける鍵語は **「完遂まで / 最後まで / 無人で / セッションをまたいで」**。ただの「自走して」は carryon の通常挙動（停止条件内の自走）と紛れるので relay は起こさない（誤起動防止）。「タスク完遂まで自走して」は鍵語「完遂まで」を含むので relay トリガーとして適切。

トリガー語（OR）:

- 「<タスク> を完遂まで自走して」「最後まで自走して完遂して」
- 「無人で最後までやって」「放っておいても完遂して」
- 「セッションをまたいで完遂して」「リレーして」「何セッションかけても完遂して」
- `/relay <タスク> [cap=N]`

ユーザーは**初回だけ**「ゴール + 完遂までの自律指示」で起動する。以降の継続 leg は前 leg が自動で `/relay` を送るのでユーザーは打たない。

## 起動される側のブートストラップ

- spawn 時に送るのは **`/relay`**（`/carryon` でなく）。relay の step 1 が「carryon で状態復元」なので carryon 機能を内包しつつ連鎖を再武装する。
- relay は起動時に分岐:
  - handoff.md に `Relay: ON` の活きたブロックがあり、自セッションが fresh（作業履歴なし）→ **継続 leg** として carryon へ。
  - ユーザーが `/relay <タスク>` と明示引数で呼んだ → **初回 leg** として現セッションで直接着手（carryon 不要、relay ブロックを新規作成）。

## carryon 合成メカニズム（「内部で carryon を読んで override」の実体）

スキルは実行コードでなく **プロンプト断片**。「relay が内部で carryon を読む」とは、relay leg が Skill ツールで carryon を 1 ステップとして呼び、その上に relay の支配的指示を被せる prompt 合成のこと。tool-call 列で書くと:

1. fresh セッション起動 → 入力 `/relay` を受信。
2. `Skill(relay)` 発火 → relay SKILL.md が context にロード。
3. relay の step 1「carryon で状態復元」に従い **`Skill(carryon)` を発火** → carryon SKILL.md がロードされ、その手順（handoff.md Read / Task 復元 / git 確認 / 計測環境の確認 / next step 宣言 / 自走開始）をそのまま実行。**carryon は復元ロジックの SoT のまま**（relay は再実装しない = DRY）。
4. relay SKILL.md は context に居続け、**支配的指示**として宣言:「お前は relay leg。carryon stop #1/#2 は終端のまま、stop #3 は終端でなく refresh トリガに読み替えよ。追加終端 = 無進捗×2 / leg 上限」。
5. 自走中に context 圧迫（= carryon stop #3 のシグナル）を感じたら、carryon の『止まって promote』分岐を**実行せず**、relay の refresh を実行（commit/push → 台帳追記 → `Skill(handoff)` で handoff.md を上書き → `tmux-new` 機構で次 leg 起動 + `/relay` 送信 → 確認 → idle）。

### override が成り立つ根拠（soft だが二重化で堅くする）

この override は code レベルの強制でなく **prompt の指示優先**（より外側・より具体の relay 指示が carryon の一般指示に勝つ）に依存する soft な仕組み。単独では context 劣化時に揺らぐので二重化する:

- relay SKILL.md が carryon stop #3 を**逐語引用して**「この分岐は取るな、代わりに refresh せよ」と明示。
- handoff.md の relay ブロック `Mode: ON` 自体が override のリマインダ。各 leg は起動時にこれを再読込するので、override が in-context のスキル文だけに依存せず**ファイルからも再注入**される（context が劣化しても handoff から復元される）。

### 「上書き」は 2 つある（混同しやすい）

- **(1) 挙動の override** — relay が carryon の stop #3 を読み替える（上記、prompt 優先）。「振る舞いの上書き」。
- **(2) ファイルの上書き** — refresh 時に `Skill(handoff)` が `.claude/handoff.md` を**上書き**（handoff の単一ファイル上書き規約）。leg ごとに最新状態で塗り替える。「状態ファイルの上書き」。

両者は別物。質問の「carryon を読んで上書き」は主に (1) を指す。

## 既存スキルとの合成 / 必要な最小改修

- **handoff** — relay 制御ブロックを書けるよう軽微拡張（relay が内容を渡す形でも可）。
- **tmux-new** — 送信トリガを引数化（既定 `/carryon`、relay は `/relay`）。または relay が tmux-new の起動手順を踏襲して `/relay` を送ると記述。
- **carryon** — 変更不要（relay が内部で呼ぶ）。
- **CLAUDE.md の malformed interrupt** — relay モード時は『次 leg 自動起動』に振る舞いを差し替える旨を追記（相互参照）。

## 決定事項

1. **スキル名 = `relay`**（セッション間の baton pass）。
2. **自律レベル = 完遂まで完全自走** — 停止条件（完遂 / 要ユーザー判断 / 無進捗×2 / leg 上限）に当たるまで人を介さず連鎖。チェックポイント承認待ちは設けない。代わりに guardrail を厚くする（下記）。
3. **refresh トリガ = ハイブリッド** — context 圧迫の自己判断 + malformed interrupt（2 度目）+ 任意の step 予算 backstop の OR。
4. **leg 上限の既定 = 8**（`/relay <task> cap=N` 等で変更可）。

完全自走を選んだぶん暴走防止が生命線。leg 上限 8 + 無進捗検知（2 leg 連続で成果ゼロ → abort）+ 各 leg 最低 1 commit + 終端で必ず PushNotification、をハード制約として実装する。
