# Handoff — 2026-08-09

## State

- Branch: main
- Uncommitted: clean
- Active phase / 作業中の文脈: 設計段階。リポジトリの初期セットアップ (CLAUDE.md / skills / hooks / 検証ログ) を終えたところ。実装は未着手。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に計装パッチが当たったまま。`benchmarks/tools/apply-instrumentation.sh --check` で確認できる。olean は暖機済み。

## Tasks

なし (初期セットアップ完了時点)。

## Where we are

doc-gen4 の実測は完了し、レポートと生ログが `benchmarks/` にある。それを根拠にしたアプローチ計画が `docs/approach.md`。
検証段階 1〜6 の枠だけ `docs/verification-log.md` に作ってあり、中身は段階 1 の「前提の実測」(doc-gen4 の batch コマンドで 47.4 秒) だけが埋まっている。
lean-doc 自身のコードはまだ 1 行も無い。実装言語も意図的に未確定。

## Next step

**検証段階 1 を lean-doc 自身の抽出器で再現する最小プログラム**を書く。

- 対象パッケージのモジュール群を 1 プロセスでロードし、宣言を走査するだけの Lean 実行ファイル。HTML も IR の永続化もまだ要らない。
- 測るのは 2 つ: 環境ロードの時間と、全モジュール処理の合計時間。基準は 47.4 秒 (`benchmarks/results/batch.jsonl`)。
- 実行は対象リポジトリの `LEAN_PATH` を借りる形で行う (`cd /Users/haruka/dev/lean-projects && lake env printenv LEAN_PATH`)。このリポジトリ側に Mathlib を clone しない。
- 結果は `docs/verification-log.md` の段階 1 の節に、ラベル付きで追記する。

## Files to read first

- `docs/approach.md` — 計画本体。特に §5 (設計の柱) と §7 (検証の順序)
- `docs/verification-log.md` — 段階 1 の現状と、結果の書き方
- `CLAUDE.md` — 計測対象の固定と「計測の誠実性」の 4 ラベル
- `benchmarks/doc-gen4-report.md` — 数字の出所。末尾に再現手順

## Load-bearing context

- **このリポジトリでは `lake` が使えない** (toolchain 未設定・意図的)。Lean を動かすときは対象リポジトリ側で `lake env` する。
- doc-gen4 の内部 API `analyzeConcreteModules` は既に `Array Name` を受け取れる。batch コマンドの追加が CLI 定義の変更だけで済んだのはそのため — 自前抽出器の設計でも参考になる。
- フルビルド計測は 8,600 モジュール中 3,590 (42%) で打ち切っている。全体の壁時計は測れていないので、そう書いてある数字を「実測」に格上げしない。
- 並列度を上げるとメモリ律速でスループットが落ちる (8 並列で RSS 21.7 GB / 物理 16 GB)。計測時は並列度を必ず記録する。
