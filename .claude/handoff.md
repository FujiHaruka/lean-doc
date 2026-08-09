# Handoff — 2026-08-09 (3)

## State

- Branch: main / Uncommitted: clean (最新 `45fed92`)
- Active phase: **relay leg 1 の開始点**。検証段階 1・2 完了、段階 3 に着手するところ。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に
  計装パッチが当たったまま (`benchmarks/tools/apply-instrumentation.sh --check` で確認)。
  olean 暖機済み。対象リポジトリは clean。同リポジトリには doc-gen4 の完成 HTML 出力
  (`.lake/build/doc`、876 MB) があり、段階 3 の突き合わせ先になる。**読むだけ。**

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  段階 3 → IR 永続化 → 段階 4 / cold 節 / 段階 5 の順。**判断基準を満たさなければそこで停止し、
  否定を `docs/verification-log.md` に記録して `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 1 / cap 10
- Predecessor: none  # leg 1 はユーザーの元セッション由来。kill しない
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - (まだ無し)

## Tasks

なし。leg 1 の最初に `docs/plans/stage3.md` の増分 1〜4 から立てる。

## Where we are

段階 1・2 が完了し、抽出フェーズは warm 11.82 秒の実測になっている (環境ロード 21% /
意味解析 78%)。ただし署名は文字列に落としているだけで、依存へのリンクは 1 本も張っていない。
今セッションでは計測をせず、3 軸 (初回・CI・増分) のゴールと段階 3 の実行計画を書いた。

## Next step

`docs/plans/stage3.md` の**増分 1** から入る。`experiments/stage3/` を新設し
(`experiments/stage2/` をコピーして起点にする。stage1/2 は壊さない)、doc-gen4 の
`tagCodeInfos` 相当を入れて**署名中に現れる定数名**を回収する。出す数字は参照定数の
ユニーク数と自パッケージ/依存側の内訳、そしてタグ付けの追加コスト (段階 2 の `--tag`
+0.4 秒と整合するか)。

## Files to read first

- `docs/plans/three-axes.md` — relay のゴール本体。leg 構成・完遂の定義・**事前決定の表**
- `docs/plans/stage3.md` — leg 1〜3 の中身。増分・落とし穴・撤退ライン
- `docs/verification-log.md` — 段階 1・2 の結果。**数字の SoT**。計測条件もここ
- `experiments/stage2/README.md` — 現役プロトタイプの構造と実行方法。段階 3 の出発点
- `CLAUDE.md` — 計測の誠実性 / オーケストレーション規律

## Load-bearing context

- **事前決定は `three-axes.md` §6 にある。** 段階 4 は Deno/TS の使い捨て (製品言語の決定ではない)、
  所有モジュールは doc-gen4 踏襲、IR は 1 モジュール 1 JSON、`--open` 既定 OFF、
  **ネットワーク不使用**、**cold 計測で sudo を使わない**。ここを蒸し返すと relay が PAUSED で止まる。
- **このリポジトリでは `lake` が使えない** (toolchain 未設定・意図的)。Lean は対象リポジトリ側で
  `lake env`。`--root=<experiments/stageN>` が必要、`leanc` には `-rdynamic` が必須。
  計測はネイティブビルドで (`lean --run` は不可)。
- **タグ付けは pp オプション `pp.tagAppFns` に依存する。** doc-gen4 と同じオプションで測ること。
  段階 2 でここを取り違えて「9.5% 速く見えた」事故がある。
- **1 回だけ測った数字を信じない。** 連続 5 回以上、壁時計 ≒ user+sys なら warm。
  時間を比べる前に doc-gen4 と同じ仕事をしていることを (件数ではなく集合で) 先に確認する。
- 段階 3 の成果物は**バイト数と一致率であって秒ではない**。新しい時間の数字は 1 つだけ。
- 未検証の残: `experiments/stage2` は属性収集・instance 索引・`renderTagged`・本物の IR 永続化を
  出していない。**11.82 秒を「完成品の値」として引用しない。**
