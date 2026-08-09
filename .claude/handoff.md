# Handoff — 2026-08-10 (3)

## State

- Branch: main / Uncommitted: clean / push 済み (`555a754`)
- Active phase: `docs/plans/three-axes.md` の **leg 6 (CI 軸 = cold の打ち手)**。
  段階 3・4 は完結し、**3 軸のうち「初回生成」は仮定ゼロで閉じた** (14.53s = 13.71 + 0.82、両方 warm 実測)。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に計装パッチが
  当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。対象リポジトリ clean、
  `.lake/build/doc` の 348 ページは**正解データなので読むだけ**。`fromDb` を再実行すると壊れる。
- IR (schema 2) は前セッションの scratchpad にあり、消えていたら `experiments/stage4b/run.sh` で再生成:
  `/private/tmp/claude-502/-Users-haruka-dev-lean-doc/2dbcb565-edbc-4bd9-846b-574772a9c30c/scratchpad/ir-tagged`

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  **判断基準を満たさなければそこで停止し、否定を `docs/verification-log.md` に記録して
  `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 5 / cap 10
- Predecessor: three-axes-r4
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 段階 3 増分 1・2。`7380856` / `b0da5a0` / `da91c89`
  - r2: 段階 3 増分 3・4 (完結) + IR 永続化。`6c5a845` / `bcc4851` / `4aa119f`
  - r3: 段階 4 増分 1・2 (位置つきタグ + 再計測)。`70aeba0` / `f623de3` / `88c1e8a`
  - r4: **段階 4 完結 =【判断点 2】通過**。`454c77a` / `291d5e6` / `e7bcac0` / `506ee0a` / `555a754`

## Tasks

- #2 [pending] leg 6 CI 節 — cold の打ち手を切り分けて実測
- #3 [pending] leg 7・8 段階 5 — 増分生成 (ハッシュ差分 + stale 検証)
- #4 [pending] leg 9 全体整合 — approach.md §6 の全面更新と引用箇所の一致

## Where we are

段階 4 は完結した。`experiments/stage4c/` が **Lean を起動せずに** IR から HTML を作り、
`div.decl_header` のアンカー (位置, テキスト, href) の列は **3,477/3,477 一致**、
ページ全体では doc-gen4 の 22,028,728 バイトのうち **63.6% を byte 完全再現**、
生成時間は **warm 0.82s / cold 0.93s** (§6.2 の 1.28s 仮定は 1.55 倍の過大だった)。
`approach.md` §6.2 / §6.3 / §6.4 と README・plans は同じコミットで更新済み。

## Next step

`docs/plans/three-axes.md` の **leg 6 (CI 軸)**。§5 の「効きそうな順ではなく切り分けやすい順」で:

1. **まず「CI の cold は本当に cold か」を潰す。** CI は `lake exe cache get` で olean を
   展開した直後に doc 生成が走るので、**展開直後なら page cache に載っている**可能性がある。
   これが正なら 13 秒は CI では起きず、**CI 軸の前提そのものが変わる**。
2. 次に**先読み** (抽出開始と並行に olean を舐める) を「舐めた後との差」で測る。
3. IR キャッシュは段階 5 と同じ機構なので leg 7〜8 の後。バイナリのページイン 3.5s は最後。

成果物は `docs/verification-log.md` の cold 節と、`approach.md` §7 の検証表への CI 行の追加。

## Files to read first

- `docs/plans/three-axes.md` §5 — leg 6 で切り分ける 4 候補と、その順序の理由
- `docs/approach.md` §3 / §6.1 の cold 側 — 環境ロード 13s とバイナリ 3.5s の出所
- `docs/verification-log.md` の「段階 1 — 前提の実測」 — cold/warm の既存の取り方
- `benchmarks/results/stage4c-render-summary.txt` — 直近の計測ハーネスの書式 (真似する)

## Load-bearing context

- **warm 判定に `壁時計 ≒ user+sys` を使えない相手がいる。** Deno のようなマルチスレッド
  ランタイムでは `(user+sys)/壁` が 1.3 になり、cold に見える。**page fault を主指標にする**
  (今回は cold 148 対 warm 3 で判定した)。CLAUDE.md の判定基準は単一プロセスの Lean 向け。
- **cold は「セッション最初の 1 回」で取る。sudo (`purge`) は使わない** (計画 §6 事前決定 #6)。
  ただし直前に他のコマンドを打つとバイナリだけ warm になる。**何が cold で何が warm かを書く。**
- **`fromDb` を再実行しない。** `.lake/build/doc` の 798 MB を上書きし、増分 3・4 の
  正解データ 348 ページが壊れる。doc-gen4 側の warm は既存ログで解決済み
  (`fromdb-6k.jsonl` 44.52s / `fromdb-6k-2nd.jsonl` 42.31s、差 5.0%)。
- **段階 5 の証拠が偶然出てきた**: ディスク上の doc は同一ビルドなのに **git rev が 2 つ混在**
  (`573793b…` 305 ページ / `5e38aec…` 43 ページ・540 リンク、独立に検算済み)。
  43 ページは**署名は現行 IR と一致していてリンクだけが古い** — コンテンツは新しく
  メタデータだけ古いという、**ハッシュ差分では原理的に捕まらない stale**。leg 8 の検証項目に落とす。
- **IR の既知の欠落 (schema 3 候補)**: 定数タグの*トリム前*の範囲。これが無いので
  `splitWhitespaces` の空白復元ができず、772/3,477 宣言が byte 不一致 (差は `\n`→` ` が 1,765 文字、
  平文の 0.115%、**アンカーへの影響 0**)。推測で埋めると 407 直して 100 壊すことを実測済み。
  **今は入れない** — 入れると抽出の再計測が必要になる。
- **「IR 読み出し 0.100s」は streaming する消費者にだけ言える。** IR を保持する
  `render.ts` では 0.130s (保持の有無だけの対照で +20% を実測)。
- `benchmarks/tools/read-ir.ts` の `DEFAULT_IR` が**古い scratchpad**を指したまま。`--ir` を渡すこと。
- **431 と 432 の混在**: doc-gen4 側の計測は 431 モジュール、抽出器側は 432。
  README と approach.md で両方使っている。**leg 9 (全体整合) で決着させる。**
- `docs/verification-log.md` は 1,510 行。**数字の SoT なので圧縮しない。**
  600 行ルールは `docs/plans/*.md` に対して適用 (approach.md 555 / three-axes.md 148)。
