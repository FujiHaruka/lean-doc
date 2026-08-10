# stage 7h — L3-3 (全域成果物) を増分化する

段階 7 の残り 1 項。`experiments/stage5/global.ts` は**全パッケージに依存する成果物**
(`declarations/declaration-data.bmp` / `navbar.html` / `tactics.html` / `references.html` /
`references.bib` / `declarations/name-map.json`) を毎回ゼロから作り直していた。

`approach.md` §8 上は**既に決着済みの行**なので、これは最適化であって未解決の問いではない。
**だから最初の成果物は実装ではなく「天井を正しく測ること」**。

数字とその条件は `benchmarks/results/stage7h-{main,probe,oracle}.txt` (SoT)。
ここには**何をなぜ変えたか**だけを書く。

## なぜ新しいディレクトリなのか

`stage5/global.ts` は**この段階の対照**であり、docs がその数字を引用している。
だから **1 行も触っていない**。`stage7d` / `stage7e` / `stage7g` も同様。
`experiments/stage4c/coverage.ts` は受け入れオラクルなので**当然変えない**。

## (1) 天井を測った — `probe.sh`

「増分実行の 1.5〜2.6%」という増分 7 の判断は**別の分母**の話だったので、
現行パイプライン (7g 配線) で `globalSeconds` を部品に割った【実測、9 rep 中央値】:

| 部品 | 秒 | 判定 |
|---|---:|---|
| deno 起動 (`build` プロセス) | 0.0717 | **床** (増分化後も 1 プロセスは要る) |
| `build`: 433 モジュールの IR 全読み | 0.1339 | **消せる** |
| `build`: 6 成果物の導出 + 書き出し | 0.0241 | 消せない (各成果物は集合全体の関数) |
| deno 起動 (`delta` プロセス) | 0.0570 | **消せる** (1 プロセスに統合) |
| `delta`: 2 つの name-map の差分 | 0.0066 | 消せない |
| `delta`: 433 モジュールの IR 全走査 | 0.1276 | **消せる** |
| **合計** | **0.4210** | **消せる 0.3186 / 床 0.1024** |

`deno run` 自体の床は 0.0557 秒 (空スクリプト)。
名前が 1 つも動かない run では `delta` が走査を自分でスキップするので、
そのケースの取り分は 0.128 秒小さい。

### 診断 — このパイプラインは IR を 7 回全読みしていた

`count-reads.ts` が `Deno.readTextFile` を包んで実スクリプトを走らせた実測:

| step | 現行 | 新版 |
|---|---:|---:|
| ownership-r1 / merge-r1 / ownership-r2 / merge-r2 | 437 / 433 / 2 / 433 | 同じ |
| global | 433 | **6** |
| global-delta | 433 | — (統合) |
| impact / render | 433 / 433 | 同じ |
| **合計** | **3,037 読み (全読み 7 回)** | **2,177 (全読み 5 回)** |

**L3-3 が消すのは全読み 7 回のうち 2 回**で、**残る 5 回のほうが大きい**。
この段階の取り分を「パイプラインの固定費を消した」と読むのは誇大。

## (2) 実装 — `global.ts`

天井 0.319 秒 = 総時間の **2.94% (fresh) / 1.57% (resident)** で撤退ライン (1%) を超えたので実装した。

- 導出に要る**モジュール単位の事実だけ**をキャッシュに置く (imports / tactic 数 /
  (name, kind) / instance の型頭定数 / docstring の autolink トークン)。**842 KB** で済む
  (IR は 16 MB)。置き場所は**サイトの外**の `--state` ディレクトリ。
- **導出コードは stage5 のまま**で、毎回 433 モジュール全部を回す。増分になったのは
  **「読む」ところだけ**。バイトを作るコードを触らないのが byte 一致を保つ最も安い方法。
- 有効性の判定は `index.json` の **`contentHash`** — **呼び手が渡す changed-set ではない**ので、
  呼び手が間違えてもキャッシュは腐らない。
- `build` と `delta` は同じ事実を要るので **1 プロセスに統合**した (deno 起動 1 回分)。
  `delta` の述語は無改変で、走査対象がキャッシュのトークン集合になっただけ。

## オラクル (先に決めてから書いた)

1. **6 成果物が from-scratch 版と byte 一致** — `oracle.sh` が
   base / rerun / modified / **removed** / **added** / restored / **stale-state** の
   7 状態で確認。**全状態 PASS**。
2. **サイト全体が byte 一致** — 7g と同じ 439 ファイルの木を REFERENCE と突き合わせ。
   **4 構成すべて missing 0 / extra 0 / differing 0**。
3. **step 6 が step 7 に渡す `--print-set` が old/new で一致** — 全域写像の差分は
   L3-2 の入力なので、ここがずれると再生成集合が静かに変わる。

## 結果 (詳細は summary が SoT)

`globalSeconds` の中央値【実測、同一セッション、7 run × 4 構成を交互、run 1 破棄】:

| 構成 | 現行 | 新版 | 差 |
|---|---:|---:|---:|
| fresh | 0.4527 | **0.1257** | **−0.3270** |
| resident | 0.4470 | **0.1461** | **−0.3010** |

内訳: `readSeconds` 0.1525 → 0.0098、スクリプト内部 total 0.1785 → 0.0449
(新版のこの値は `delta` 込み、旧版は含まない)。キャッシュ 427 hit / 6 miss、842,210 B。

**スケール** (5 rep 中央値): 無効化 0 / 6 / 25 / 100 / 433 件で
0.107 / 0.103 / 0.110 / 0.142 / 0.234 秒。対照 (stage5 `build`) 0.241 秒。
**全件ミスでも劣化しない。**

## 落とし穴 (この段階で実際に踏んだ)

**1. 総時間には帰属できない。** 同じペアで `extract` + `serveStart` だけが 0.80 秒 (fresh) /
0.37 秒 (resident) 動いていて、**効果の 2.5 倍 / 1.2 倍**。引用してよいのは step 6 の行だけで、
`total` の行ではない。分母つきで言えるのは「削減 0.327 秒 = 総時間 11.13 秒の 2.94%」まで。

**2. 最初の A/B は位置と処置がエイリアスしていた。** 構成の順序が固定だったので、
**常に先頭の構成が直前の 3 GB 常駐サーバによるページキャッシュ退避を被っていた**
(page fault 29,779 ↔ 2,013、3.3 秒差)。ラウンドの偶奇で順序を反転して測り直した。
**時間だけ見ていると気づかない壊れ方。**

**3. このセッションは 7g より重い** (`serveStart` 15.0 秒 ↔ 7g 3.71 秒)。
**7g の総時間と並べていない** (CLAUDE.md「cold と warm を混ぜて比較しない」)。

## 再現手順

```sh
experiments/stage7h/run.sh setup      # 対象の複製と REFERENCE の作成
experiments/stage7h/run.sh ab         # 4 構成 × 7 run を交互に (計測)
experiments/stage7h/run.sh counts     # IR 全読みの回数 (診断)
experiments/stage7h/run.sh report     # 集計だけ (再計測を要求しない)
experiments/stage7h/probe.sh          # 天井の内訳
experiments/stage7h/oracle.sh         # 7 状態の byte 一致
```

生ログ: `benchmarks/results/stage7h-*`。作業ディレクトリは `/private/tmp/lean-doc-relay/w7h`。
