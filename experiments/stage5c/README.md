# 段階 5c — 参照側モジュールの olean は実際に変わるのか

**この文書は計測を実行する前に commit する。** 判断基準・否定条件・事前予測を先に固定し、
出た数字に合わせて基準を動かせないようにする (段階 5b と同じ規律)。

## 何を決めたいのか

`approach.md` §5.5 の見直し案 (判断点 3 を受けた 3 層構造) のうち、**L3-1「名前の所有権」**は
まだ**仮定の上に立っている**。仮定はこうだ:

> 宣言が別モジュールへ移動すると、その名前を参照しているモジュールの IR が腐る。
> ところが参照側モジュールの `.olean` は変わらないので、L2 (モジュール単位の olean ハッシュ)
> はそれを検出できない。だから L2 の核を「所有権が変わった名前を参照するモジュール」へ
> 広げる仕掛け (L3-1) が要る。

**もし参照側の olean が実際に変わるなら、L2 がそのまま捕まえるので L3-1 は丸ごと不要になる。**
設計から 1 層消える。逆に変わらないなら L3-1 は必須項目として確定する。

この仮定は増分 1・2 を通じて一度も測られていない。理由は「対象リポジトリを書き換えない」
という制約で `lake build` を回せなかったこと。**今回は APFS の clonefile で複製を作って外す。**

## 命題と否定条件 (計測前に宣言)

| # | 命題 | 事前予測 | 否定条件 |
|---|---|---|---|
| **P1** | olean の生成は決定的である — 同じソースから再ビルドすると byte 一致する | **真** | A の変更を戻して再ビルドしたあと、いずれかの olean が baseline と byte 不一致 |
| **P2** | 参照されていない宣言を A に足しても、A を import するモジュール C の olean は byte 一致のまま | **真** (olean は自モジュールの宣言しか持たない) | C の olean が byte 変化する |
| **P3** | **宣言が A から X へ移動する (名前は不変) と、その名前を参照する C の olean が変化する** | **偽** (変化しない) | C の olean が byte 変化する = P3 が真 |

**P3 が判断点。** P3 が**真なら L3-1 は不要**、**偽なら L3-1 は必須**。
どちらに転んでも `approach.md` §5.5 を書き換える。

`.olean` のバイト列だけでなく Lake が書く `<file>.olean.hash` も両方記録する
(L2 が実際に読むのは後者なので、両者が食い違わないことも確認する)。

## 副次的に測るもの

- **no-op の `lake build` 時間** — 増分の臨界パス上にあるのに未測定だった。
  増分パイプラインは olean が最新であることを前提にしているので、実運用ではこれが先に乗る。
- **1 モジュール変更時の `lake build` 時間** — 同上。

## 方法

対象リポジトリ `/Users/haruka/dev/lean-projects` を APFS clonefile (`cp -Rc`) で複製する。
複製は COW なので実消費ディスクはほぼゼロで、**原本は一切書き換わらない**。

- **A** = `InformationTheory.Shannon.GaussianPDFVarianceDerivative` (8.5 KB)
- **C** = `InformationTheory.Shannon.FisherDeBruijnGaussian` (50 行) — A の
  `isHeatTimeDerivHyp_gaussian` を 2 か所で参照している。A の直接 importer は C と
  root モジュールの 2 つだけなので、再ビルドが小さく収まる。
- **root** = `InformationTheory` — 431 モジュールを全部 import しているので、
  **どのモジュールを変えても必ず再ビルドされる**。

手順 (各ステップの前後で 1,090 個の olean 全部の sha256 / size / mtime / lake_hash を記録):

1. **S0** baseline。`lake build` が no-op であることを確認済み。
2. **E-A** A の末尾に、誰も参照しない宣言を 1 個足す → `lake build` → **S1**。
   S0→S1 の差分で P2 を判定。
3. **E-A-revert** A を元に戻す → `lake build` → **S2**。S0→S2 の差分で P1 を判定。
4. **E-B** A の中身を丸ごと新モジュール X (`Shannon.GaussianPDFVarianceDerivativeCore`) へ
   移し、A は `import X` だけの shim にする → `lake build` → **S3**。
   **C のソースは 1 文字も変えない**。S2→S3 の差分で P3 を判定。

E-B が「宣言の移動」として正しい形になっている理由: 名前空間は `namespace` コマンドで決まり
ファイルパスとは独立なので、ファイルを移しても**宣言の完全名は変わらない**。変わるのは
「どのモジュールがその名前を定義しているか」だけ — これが L3-1 が対象にしている変化そのもの。

## 計測条件

- 機材: Apple M1 / 16 GiB / macOS 15 (Darwin 25.6.0)、ディスク空き 16 GiB
- Lean 4.31.0 / Mathlib v4.31.0 / Lake 5.0.0-src+68218e8
- 複製先: session scratchpad の `lp-clone` (APFS clone、12 GB を COW で 0 消費)
- 暖機: 全ステップ warm (`lake build` no-op で壁時計 ≒ user+sys を確認済み)
- 並列度: Lake の既定 (指定なし)
- 複製には doc-gen4 の計装パッチが当たったまま入っている (`lake build` は
  `warning: doc-gen4: repository ... has local changes` を出すが、ビルド対象ではない)

## ツール

- `import-graph.py` — `.lean` ヘッダからパッケージ内 import グラフを作る
- `snapshot-oleans.py` — olean 全数の sha256 / lake_hash を記録・比較する
- `measure-lake-build.sh` — `lake build` を n 回、`/usr/bin/time -l` つきで回す
- `run-fanin.sh` — fan-in の大きいモジュールを 1 宣言だけ変えて `lake build` を測る

---

## 結果 (2026-08-10)

**全数字は【実測】。** 出所は `benchmarks/results/stage5c-olean-propagation-summary.txt`
(生ログは同 `stage5c-lake-build-times.jsonl`)。

| # | 事前予測 | 結果 | 決め手 |
|---|---|---|---|
| P1 | 真 | **真 (パスを固定すれば)** | 同一ソースからの再ビルド 4 回すべて byte 一致 |
| P2 | 真 | **真** | A に未参照の宣言を足しても C の olean は sha256 完全一致 (再ビルドはされる) |
| **P3** | **偽** | **偽** | 宣言を A → X へ移動しても C の olean は **1 byte も変わらない** |

**P3 が偽なので L3-1 は必須で確定した。** L2 (モジュール単位 olean ハッシュ) は
「名前は同じまま定義モジュールが変わる」を原理的に検出できない。
Lake の `<file>.olean.hash` と sha256 は全ステップで一致したので、
これはハッシュの精度の問題ではなく**参照側に変化が存在しない**という構造の問題。

P2 が真だったのは設計上の朗報でもある — **再抽出集合を import 閉包へ広げる必要はない**。
広げるべきは出力側 (ページ) であって入力側ではない、という段階 5b の結論と一致する。

### 副次的に出た 3 つ

1. **`lake build` は増分の臨界パスの 74〜79% を占める** — 1 モジュール変更で
   warm 12.40〜12.73s (CPU 11.55〜14.18s で安定、壁時計は page cache 依存で 2.2 倍動く)。
   doc 側の増分は 3.30〜4.39s なので、**doc 側をゼロにしても端から端では 12.4 秒より速くならない**。
   無変更なら `lake build` 2.40s + 検出 0.080s = 2.48s。
2. **olean にソースの絶対パスが 429/432 で埋まっている** — 正体は Mathlib style linter の
   ログを持つ環境拡張 `lintLogExt` で、警告文に絶対パスが入ったまま保存されている。
   **olean の内容ハッシュがチェックアウト先のパスに依存する**ので、CI のワークスペースパスが
   開発機と違うだけで IR キャッシュが全滅する。Lean の olean 形式の性質ではなく、
   警告を出したままにしていることの帰結 (Mathlib 自身の olean にはパスが 0 個)。
3. **Lake は孤児 olean を消さない** — 現行ソース 432 モジュールに対し `.lake` の olean は
   1,090 個で、**659 個が現行ソースに対応しない**。§5.5 の L2 改善案
   「モジュール一覧をディスク走査で作る」は、走査先を `.lake/build` にすると幽霊を拾う。
   列挙元は Lake のライブラリ target (ソースの glob) でなければならない。
