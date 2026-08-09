# Handoff — 2026-08-09 (2)

## State

- Branch: main
- Uncommitted: clean (最新 `28f4a1b`)
- Active phase: 検証段階 **1・2 が完了**。抽出フェーズの数字は理論値から**実測**に置き換わった。
  次は段階 3 (依存パッケージの IR なしでリンクが解決できる)。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に
  計装パッチが当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。
  olean 暖機済み。対象リポジトリは clean。
- 抽出器のプロトタイプ: `experiments/stage1/` (最小)、`experiments/stage2/` (意味解析込み、現役)。
  ビルドは各ディレクトリの `build.sh`、計測は `run.sh`。

## Tasks

なし (段階 2 完了時点で全クローズ)。

## Where we are

このセッションでやったこと:

1. **段階 1** — `import Lean` だけに依存する自前抽出器で、432 モジュールを 1 プロセスで
   ロードし索引方式で宣言を引けることを実証。
2. **計測のやり直し** — doc-gen4 の「環境ロード 12.9 秒の床」が **page cache のアーティファクト**
   だと判明 (warm 2.5 秒 / cold 13 秒)。doc-gen4 batch と方式A (逐次) を warm 同士で全数測り直し、
   23 倍 → **34 倍**に訂正。`approach.md` / `README.md` / `benchmarks/` の該当箇所を全部直した。
3. **段階 2** — 意味解析 (pretty print / docstring / equation) とモジュール docs 収集を実装。
   doc-gen4 と**同一の 4,750 宣言・同一出力**を確認した上で **11.82 秒** (目標 14.5 秒)。

主な発見 (詳細はすべて `docs/verification-log.md`):

- doc-gen4 の `getAllModuleDocs` 16.7 秒 (バッチの 54%) の正体は
  **`EnvironmentHeader.moduleNames` が `def`** で、呼ぶたび 6,021 要素の配列を作り直すこと。
  lean-doc 側は **0.011 秒**。§5.2 の一般形を「1 モジュールの処理で import closure の
  サイズに比例するものに触らない」に強めた。
- **doc-gen4 はパッケージ自身の `scoped notation` を出力できていない**
  (`Lean.activateScoped` 未呼出)。lean-doc は出せる。コスト +6%。§10 に上流還元として記載。
- 意味解析は自前実装でも減らない (9.26s 対 10.51s、差は未実装分)。**ここが本質的な壁**で、
  最適化後の 78% を占める。

## Next step

**検証段階 3 — 依存パッケージの IR なしでリンクが解決できるか。**

- 判断基準: Mathlib へのリンクが正しく張れる / 必要な写像 (宣言名 → モジュール → URL) の
  **実サイズが分かる**。
- 出発点は `experiments/stage2/Extract.lean`。いま署名を文字列にしているだけなので、
  **署名中に現れる依存側の定数を列挙できる形**にするのが最初の一手
  (doc-gen4 の `tagCodeInfos` 相当。段階 2 で計測済み、コストは +0.4 秒)。
- 論点: 写像を誰が作るか。`approach.md` §8 の「依存パッケージの写像をどう配布するか」が
  未解決のまま。既存の公開ドキュメント (Mathlib の doc-gen4 出力) から導けるかを見る。
- 新しい実験は `experiments/stage3/` を足す (stage1/2 は壊さない)。

## Files to read first

- `docs/verification-log.md` — 段階 1・2 の結果。**数字の SoT**。長いが段階 3 の前提が全部ここ
- `docs/approach.md` §5.3 (依存は外部参照) / §5.4 (IR の粒度) / §8 (未解決の問い)
- `experiments/stage2/README.md` — 現役プロトタイプの構造と実行方法
- `CLAUDE.md` — 「計測の誠実性」と、今回追加した「1 回だけ測った数字を信じない」

## Load-bearing context

- **このリポジトリでは `lake` が使えない** (toolchain 未設定・意図的)。
  Lean を動かすときは対象リポジトリ側で `lake env`。`--root=<experiments/stageN>` が必要、
  `leanc` には `-rdynamic` が必須。計測はネイティブビルドで (`lean --run` は不可)。
- **1 回だけ測った数字を信じない。** 連続 5 回以上回して収束を見る。
  壁時計 ≒ CPU 時間 (user+sys) なら warm。このセッションで 2 回、
  「都合の良い方向」に外れた計測を踏みかけた (pp オプション取り違え / 強制されない `let _`)。
- **`experiments/stage2` はまだ出していない情報がある** — 属性収集・instance 索引・
  `renderTagged`・本物の IR 永続化。11.82 秒を「完成品の値」として引用しない。
- フルビルド計測は 8,600 モジュール中 3,590 (42%) で打ち切ったまま。外挿値をそう書くこと。
- 対象パッケージはタクティクを 0 個しか定義しない。タクティク関連の検証をするときは
  `benchmarks/results/stage2-tacmods.txt` (対象 + タクティク定義元 141 モジュール) を使う。
