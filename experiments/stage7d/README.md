# stage 7d — 意味解析 (抽出の 75%) を割り、並列化する

段階 7 の続き。**目的は 2 つ**:

1. `approach.md` §8 が「次に効くのはここしかない」と書いた**意味解析の中身を割る**。
   `ppUs` は 1 個の数字でしかなく、何が何割かは誰も見ていない。
2. 割った結果を見て、**出力を 1 バイトも変えずに減らせる筋**を実装して実測する。

数字とその条件は `benchmarks/results/stage7d-summary.txt` (SoT)。
ここには**何をなぜ変えたか**だけを書く。

## なぜ新しいディレクトリなのか

stage7c を書き換えず、その写しをここに置いた (CLAUDE.md「新しい段階は既存を壊さず
新ディレクトリを足す」)。stage7a / 7b / 7c は **1 行も触っていない** — それぞれの
再現率・秒数が docs から引用されている。

`experiments/stage4c/coverage.ts` は**受け入れオラクル**なので、この段階でも
**1 行も変えていない**。`render.ts` も stage7c の写しのまま (この段階はレンダラを
触らない)。

## 中身

| ファイル | stage7c との差 |
|---|---|
| `Extract.lean` | `--pp-breakdown` / `--decl-profile <p>` / `--jobs N` の 3 つ。既定値は全部 off / 1 で、そのときの経路は stage7c と同一 |
| `decl-profile-report.ts` | **新規**。`--decl-profile` の JSONL から分布・種別・blacklist の取り分を出す (診断) |
| `rounds-report.ts` | **新規**。回り持ちラウンドの生ログを集計する (cold 側と warm 側を分けて出す) |
| `render.ts` / `build-link-index.ts` / `autolink-check.ts` / `md-diff.ts` / `rev-check.ts` / `check-spans.ts` / `member-check.ts` / `instance-check.ts` | stage7c の写し (無改変) |

## (1) `ppUs` を割る — `--pp-breakdown`

Lean の pretty printer が実際に通る段は 5 つある:

    Expr --delab--> Syntax --sanitize--> Syntax --parenthesize--> Syntax
         --format--> Format --pretty--> String

`ppUs` はこの 5 段ぜんぶ + `--tagged-code` のスパン収集をひとまとめにした数字だった。
`--pp-breakdown` は段ごとに時計を挟む。加えて:

* **equation の生成と印字を分けた** — `getEqnsFor?` (equation 補題を*作る*) と
  `ppEquation` (それを*刷る*) は別の仕事で、`eqUs` はその和だった。
* **`isBlackListed` を別勘定にした** — 候補 8,824 のうち 4,074 がここで落ちる。
  「落とすだけの候補に時間を使っていないか」は測らないと分からない。
* **`--decl-profile`** で宣言ごとの数字を残す。1 宣言 2.2 ms は平均であって、
  分布が一様かどうかは平均からは出ない。

### 計装が対象を変えていないことの確認

細かく計るほど観測が対象を変える。3 つで確かめている:

1. **IR が byte 一致** — `--pp-breakdown` を付けた run の IR が、付けない run および
   stage7c の IR と `diff -r` で無差 (summary §1)。
2. **`total` が動かない** — 同一ラウンド内で `--pp-breakdown` 付き / 無しを
   回り持ちにして測り、差が run 間ばらつきに収まることを確認 (summary §2 の末尾)。
   結果は **−0.04 秒 (計装ありのほうが速い) で分離できていない** — summary はそう
   書いてある。「計装は 0 秒」とは書いていない。
3. **純粋な `let` は必ず pin する** — `Format.pretty` と `sanitizeSyntax` は純関数で、
   Lean のコンパイラは純粋な `let` を使用箇所まで沈められる。沈むと時計の外に出て
   **0 が計測される**。この実験は同じ罠を過去に 3 回踏んでいる (`RefSink.collect`
   のコメント)。`pin` (`@[noinline]`) が値を時計の内側に固定する。

## (2) 並列化 — `--jobs N`

**これが「出力を一切変えずに効きうる」唯一の筋**なので最優先で試した。

成立する理由は 1 つ: **宣言ごとの解析が環境を共有するだけで、書かない**。
`run` は宣言ごとに `job.toIO coreCtx { env := env } {} {}` で **新しい `Core.State`**
を作り、その中で環境が変わっても (equation 生成は実際に環境に定義を足す) **捨てる**。
つまり宣言間にキャッシュが無く、順序依存も無い。`importModules (leakEnv := true)` は
環境オブジェクトを persistent に印付けするので、共有しても参照カウントの更新が起きない。

### 出力順を N から独立にする

worker `k` は候補列の `k, k+N, k+2N, …` を取る (**ブロックではなくストライド**:
候補列はモジュール順に並んでいて、ブロックは互いに交換可能ではない)。各答えは
**候補の添字を持って帰り**、`slots` に添字で書き戻される。だから `results` の並びは
`--jobs 1` の逐次ループが作る並びと**同一**になる。

**これは主張ではなく検査事項**である: `--jobs` 1/2/4/8/16 と stage7c の IR を
`diff -r` で突き合わせ、さらに**独立した 2 回の `--jobs 8` run** 同士も突き合わせる
(summary §1)。順序が非決定になるのが並列化のいちばんありがちな壊れ方で、
**時間だけ見ていると気づかない。**

### 並列時の `ppUs` は壁時計ではない

`ppUs` / `eqUs` / `refUs` は**スレッドごとの経過時間の和**なので、`--jobs 8` では
1 スレッド分の壁時計を大きく超える。CPU 時間でもない: `--jobs 16` では和が
`user+sys` すら超える (スレッドが降ろされている間も各スレッドの時計は進む)。
**フェーズの `us` (= `analyze` 自身) だけが壁時計**。summary の表はその区別を明示している。

## 落とし穴 (この段階で実際に踏んだ / 確かめた)

**1. 並列化したら IR の byte 一致を必ず確認する。** 上記の通り、6 構成 + 独立 2 run で
`diff -r`。加えて抽出器が報告する件数 (considered / produced / blacklisted / equations /
refOccurrences / spans) も構成間で一致することを確認した。

**2. `--pages` は増分の数字を動かしてはいけない。** `render.ts` は無改変だが、
stage4c README の 3 点を `--jobs 8` が書いた IR に対して再実行した
(`benchmarks/results/stage7d-pitfall-recheck.txt`)。

**3. 採点は具体的な rev を埋めた `--source-url` で。** `{{SOURCE_URL}}` で描くと
未分類が跳ねて −3.1 pt ずれる (stage7c から変わらず)。

**4. 「速くなった」だけを報告しない。** equation を切る筋は速くなるが再現率が落ちる。
落ちる幅を同じ表に並べてある (summary §5)。

**5. `--jobs > 1` は `total` の比較を難しくする。** 壁時計の振れは `importModules` に
集中する (IR の書き込みが olean の mmap ページインに干渉する — CLAUDE.md)。
**この段階の主張は `analyze` フェーズタイマーで立てている。** `total` は spread 込みで
併記した。

## 結果の要約 (詳細は summary が SoT)

* **`ppUs` の中身が初めて見えた。** 支配項は **delaboration (`Expr` → `Syntax`) で
  5.7966 秒 = `ppUs + eqUs` の 55.3%**。formatting は `Syntax` → `Format` 15.4% +
  `Format` → `String` 2.9%、タグ付け (スパン収集) 12.7%、parenthesize 8.7%。
  **delaboration : formatting = 3.02 : 1** — 「pretty printer のコスト」の正体は
  レイアウトではなく delaborate である。`analyze` の 96.2% を説明できた。
* **分布はほぼ一様。** 上位 1% (48 件) の取り分は **6.97%**、中央値 1,597 µs /
  平均 2,246 µs、最大 ÷ 中央値 21 倍。**外れ値を潰す筋は無い。**
  種別の取り分も件数比でほぼ決まる (theorem: 件数 78.8% / 時間 81.5%)。
* **blacklisted 4,074 件は 0.0226 秒 = analyze の 0.21%。** 取り分は無い。
* **並列化は成立した。IR は byte 一致のまま `analyze` 10.7890 → 3.3365 秒 (3.23 倍)、
  抽出合計 14.2137 → 6.5037 秒 (2.19 倍)。** 頭打ちは 4 スレッド付近
  (M1 の性能コアが 4 本、かつ instructions +1.0% に対し cycles +66% = ストール)。
  **`--jobs 4` と `--jobs 8` の優劣は分離できていない。**
* **equation を切る筋は採らない。** `--jobs 1` で 1.18 秒速くなるが、**並列化すると
  取り分は 0.23 秒に縮み**、代償は再現率 **99.506% → 97.107% (−2.400 pt)**、
  byte 完全一致ページ **304 → 133**。
* **意味解析が抽出の 75% という前提は `--jobs 1` の話。** `--jobs 4` では 51.3% で、
  **最大項は `importModules` 側 (38.8%) に移る。**

## 再現手順

```sh
W=/private/tmp/lean-doc-relay/w7d
MODULES=benchmarks/results/it-modules.txt
URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec
LIDX=<stage 7c が作った link-index.lidx>

experiments/stage7d/build.sh

# (1) ppUs を割る (--decl-profile は --pp-breakdown を含意する)
MODULES=$MODULES RESULTS_DIR=$W/logs experiments/stage7d/run.sh stage7d-bd -- \
  --equations --refs --write-ir --tagged-code --ir-dir $W/ir-bd \
  --pp-breakdown --decl-profile $W/decl-profile.jsonl
deno run --allow-read --allow-write experiments/stage7d/decl-profile-report.ts \
  --profile $W/decl-profile.jsonl --report <OUT>

# (2) 並列化 — 構成は同一ラウンド内で回り持ちにする (単独で回した数字は比較に使わない)
#     rounds.sh は $W に置いた使い捨てドライバ。中身は run.sh と同じ呼び出し。
MODULES=$MODULES RESULTS_DIR=$W/logs experiments/stage7d/run.sh stage7d-j4 -- \
  --equations --refs --write-ir --tagged-code --ir-dir $W/ir-j4 --jobs 4
deno run --allow-read --allow-write experiments/stage7d/rounds-report.ts \
  --dir $W/rounds --configs 7c,j1,bd,j2,j4,j8,j16 --rounds 7 --drop 1 --report <OUT>

# (3) 出力が変わっていないことの確認 — これを飛ばした速度の数字は無効
diff -r $W/ir-7c $W/ir-j4        # stage7c の抽出器が同一セッションで書いた IR と
deno run --allow-read experiments/stage7d/check-spans.ts --ir $W/ir-j4

# (4) 採点 (オラクルは stage4c のまま、1 行も変えない)
deno run --allow-read --allow-write --allow-env experiments/stage7d/render.ts \
  --ir $W/ir-j4 --pages $W/pages --source-url "$URL" --link-index $LIDX --stats <OUT>
deno run --allow-read --allow-write --allow-env experiments/stage4c/coverage.ts \
  --pages $W/pages --report <OUT>
```

生ログ: `benchmarks/results/stage7d-*`。
`stage7d-extract-rounds.jsonl` の各行は `cfg` / `round` / `cold` を持つ。
