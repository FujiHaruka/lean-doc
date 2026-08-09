# benchmarks — doc-gen4 の実測

lean-doc の設計判断はすべてここの数字に紐づいている。
新しい設計判断を入れるときは、対応する計測をここに足す。

## 中身

| | |
|---|---|
| [`doc-gen4-report.md`](doc-gen4-report.md) | 計測レポート本文。結論・方法・限界まで |
| [`doc-gen4-report.html`](doc-gen4-report.html) | 同じ内容をブラウザで読む用 (グラフ付き) |
| [`doc-gen4-instrumentation.patch`](doc-gen4-instrumentation.patch) | doc-gen4 v4.31.0 に当てた計装パッチ |
| `tools/` | 計測実行と集計のスクリプト |
| `results/` | 生ログ (JSONL)。集計は `tools/analyze.ts` |

## 計測条件

- doc-gen4 `v4.31.0` (rev `0bc516c`) / Lean `v4.31.0` / Mathlib `v4.31.0`
- 対象: `InformationTheory` (**432 モジュール** = `InformationTheory/**.lean` の 431 +
  ルートモジュール `InformationTheory`、Mathlib 全体に依存)
- 機材: Apple M1 / 8 コア / 16 GB
- olean はビルド済み。**ページキャッシュの状態は計測ごとに違う** (下記)

### ページキャッシュ状態は必ず記録する

このワークロードは olean を mmap で読むため、**page cache が warm か cold かで
環境ロードが 5 倍変わる** (実測 2.5s ↔ 13s)。当初 `batch.jsonl` を
「暖機済み」と記録していたが、後の再計測で cold だったと判明した
(→ `docs/verification-log.md` 段階 1)。

新しい計測を足すときは、**同じ計測を連続で 5 回以上回して収束を見る**こと。
1 回目だけの数字は cold 側を掴んでいる可能性がある。
`experiments/stage1/run.sh` は条件と peak RSS を `*-summary.txt` に残す。

## 計測対象は固定

計測は**常に同じリポジトリ** (`/Users/haruka/dev/lean-projects`) に対して行う。
比較は同一ワークロード上でのみ意味を持ち、ここの数字はすべてこの対象で取られている。
別の対象を測りたくなったら、置き換えるのではなく追加し、既存の数字は残す。

対象リポジトリで触るのは `.lake/packages/doc-gen4` (ビルド生成物) だけで、対象側にはコミットしない。
パスは `TARGET_REPO` 環境変数で上書きできる (`tools/env.sh`)。

## 計装の考え方

環境変数 `DOCGEN_TIMING` が設定されているときだけ、各フェーズの実測時間を
JSONL で追記する (無効時はゼロコスト)。並列プロセスが同一ファイルに追記するため、
各レコードは PID を持つ。

あわせて、複数モジュールを 1 プロセスで抽出する `batch` コマンドを doc-gen4 に追加した。
これが「1 プロセスにまとめると 34 倍」の実証にあたる
(warm 同士の全数比較。レポート本文の 23 倍は cold なバッチ計測に基づく旧値)。

## 再現

```bash
tools/apply-instrumentation.sh --check   # 計装が当たっているか
tools/apply-instrumentation.sh           # 当て直してビルド
tools/run-serial.sh                      # 方式A (モジュールごとに 1 プロセス)
tools/run-full.sh 4 full-build           # フルビルド (LEAN_NUM_THREADS=4、再開可能)
deno run -A tools/analyze.ts results/<name>.jsonl
```

`.lake` は対象リポジトリの生成物なので `lake update` で消える。消えたら
`apply-instrumentation.sh` で当て直す。個別コマンドの詳細はレポート末尾の「再現」節にある。

## 注意 — この計測の限界

レポートに明記してあるが、要点だけ:

- **フルビルドは完走していない** (8,600 モジュール中 3,590 = 42% で打ち切り)。
  抽出フェーズと HTML 生成フェーズを分けて計測している。全体の壁時計は測れていない。
- 方式A は**レポート本文の時点では** 432 モジュール中 112 モジュールの実測で、残りは換算。
  その後 warm 条件で 432 全数を測り直してある (1,076s、`results/serial-warm.jsonl`
  → `docs/verification-log.md` 段階 1 の「判明したこと 1b」)。
- 単一マシンの結果。特にメモリ律速の議論は RAM 量に強く依存する。
