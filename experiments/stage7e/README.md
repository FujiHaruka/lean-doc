# stage 7e — 構文ベースのプレビュー近似のコストを測る

`approach.md` §8 の「**プレビューモードを持つか**」に残っていた唯一の未計測の分岐、
選択肢 (1)「**構文ベースの近似**」を測る。§8 はコストを「elaboration 無しの Lean パーサ」
と書いていたが、**それは未計測の仮定**だった。

数字とその条件は `benchmarks/results/stage7e-*-summary.txt` (SoT)。
ここには**何をなぜ測ったか**だけを書く。

## なぜ新しいディレクトリなのか

`stage7e-rev/` は同じ「7e」でも別の問い (rev 非埋め込み) なので、混ぜずに分けた。
既存の `stage7a`〜`stage7g` は **1 行も触っていない**。
`experiments/stage4c/coverage.ts` は受け入れオラクルなので**この段階でも変えない**。

## 測る前に潰すべき前提 (gate)

§8 の「パーサは安い」は「**パーサは環境に依存しない**」を暗黙に仮定している。
だが Lean のパーサは環境に依存する — `notation` / `macro` / カスタム構文は
モジュールが宣言するもので、対象プロジェクトは実際に `H(μ; X)` や `≐` を使う。
**これが真なら構文近似の安さは「常駐環境の中にいること」が前提**になり、
段階 6d が否定した「常駐が受け皿」と同じ土俵に戻る。

だから**コストより先に環境依存を測る**。

## 中身

| ファイル | 役割 |
|---|---|
| `Parse.lean` | elaboration 抜きのパーサ。`mkInputContext` → `parseHeader` → `parseCommand`* を EOI まで |
| `build.sh` / `run.sh` | stage7d と同じ形 (`lake env` で対象リポジトリの環境を借り、`/usr/bin/time -l` で条件を残す) |

### 構成 (`--env`)

| 値 | import するもの | 意味 |
|---|---|---|
| `init` | `Init` のみ | 実質「空環境」= §8 が想定していた「パーサだけ」 |
| `mathlib` | `Mathlib` のみ | 依存だけある状態 |
| `full` | 対象の 432 モジュール | **常駐サーバが持っている環境** |

`--open <ns,..>` は加えて `Lean.activateScoped` で scoped 拡張を活性化し、
同じ名前空間を `ParserModuleContext.openDecls` に積む。
**scoped notation は import だけでは活性化しない**ので、この差を測るために要る。

### 時計の罠

`parseCommand` は純関数 (`Id.run`) なので、Lean のコンパイラは閉じ側の
`IO.monoNanosNow` の**後ろに沈められる**。沈むと 0 が計測される。
`pin` (`@[noinline]`、stage7d から複写) でノード数とコマンド数を時計の内側に固定している。
**ソースの読み込みは時計の外**なので、`parseUs` は I/O を含まない。

## 結果の要約 (詳細は summary が SoT)

### (1) gate — **パーサは環境に依存する。しかも「import だけでは足りない」側**【実測】

432 モジュールを構成ごとにパースしたときのエラー:

| 構成 | エラーの出たモジュール | エラー総数 | 構文木ノード数 |
|---|---:|---:|---:|
| `--env init` | **404 / 432** | 4,678 | 641,813 |
| `--env mathlib` | 215 / 432 | 989 | 3,140,109 |
| `--env full` | 215 / 432 | 989 | 3,140,799 |
| `--env full --open <122 名前空間>` | 86 / 432 | 326 | 3,739,218 |
| 同上 − `ContDiff` | **5 / 432** | 9 | 4,008,495 |

**同じバイト列から出る構文木がノード数で 6.2 倍動く。** 「パースできた / できない」の
二値ではなく、**環境が薄いほど木が浅くなる**。

- `--env mathlib` と `--env full` は**エラー数が完全一致** — このプロジェクトの構文は
  全部 `scoped` なので、**自分のモジュールを import してもパーサには何も足されない**。
- `≐` (`InformationTheory/Asymptotic.lean:58:33`)、`ℝ≥0∞`、`H(μ; X)` は
  **`--env full` でも落ちる**。落ちなくなるのは `activateScoped` を足したときだけ。

### (2) 活性化集合はファイルごとに正確でないといけない【実測】

122 名前空間を一律に活性化すると **161 本が直る代わりに 32 本が壊れる**。

- 壊れた 32 本のうち **81 本ぶんの犯人は `ContDiff` 1 個** — Mathlib の
  `scoped[ContDiff] notation3 "ω"` が `ω` をトークンにし、`ω` は対象プロジェクトの
  標本点の標準変数名。`open scoped ContDiff` しているのは 432 本中 **1 本だけ**。
- 残り 5 本も同型 (`Real` の `π` が `(π := β)` の名前付き引数を壊す)。

パーサ自身が解けるのは `open X in <cmd>` だけで、**単独の `open X` / `open scoped X` /
`namespace` / `section` は後続コマンドに elaboration 経由でしか効かない**
(`ParserModuleContext.currNamespace` は常に空のまま)。

→ **構文近似の前提は「常駐環境の中にいること」だけでは足りず、
「対象ファイル自身の `open` / `namespace` 構造を解いていること」が要る。**

### (3) コスト — **1 モジュール p50 6.9 ms**【実測、warm】

| 項目 | warm |
|---|---|
| `importModules` (432 直接 → 6,021 ロード) | **2.562〜2.628 s** |
| **1 モジュールのパース** (`--env full`) | **p50 6.74〜6.95 ms / p90 23.4 ms / max 54〜56 ms** |
| 432 モジュール合計 | **4.23〜4.30 s** |
| ほぼ全部パースできる構成 (`--open` − `ContDiff`) | p50 **7.66 ms** / 合計 **4.68 s** (+9%) |

5 周 × 5 プロセスで収束を確認 (壁 24.17s ≒ CPU 24.07s = warm)。peak RSS 3.26 GB。

**撤退ラインは踏まなかった** — 「1 モジュールが秒オーダーなら構文近似の利点が消える」
に対し実測は **6.9 ms = 環境ロード 2.56 s の 0.27%**。
**§8 の「安い」は*環境の中にいれば*正しく、環境を立てる側が 370 倍。**

## 落とし穴 (この段階で踏んだ / 確かめた)

**1. `--env init` の p50 4.33 ms を「速い」と読まない。** ノードが 6.2 分の 1 しか
出ていない = 仕事量が少ないだけ。トークンテーブルの大きさによる純粋な減速と、
パースできた量の差は**分離できていない**。

**2. `parseHeader` はモジュールごとに `mkEmptyEnvironment` を呼ぶ** (全 env extension の
初期状態を作り直す)。Mathlib ロード済みだと 1 本 0.4 ms、432 本で約 200 ms
(合計の 4.4〜4.7%)。`headerUs` として分離してある。

**3. エラー数だけを見て「近似の質」を語らない。** ノード数とエラー数は
「パースが通ったか」しか言わない。宣言がどれだけ取れるか / 署名がどれだけ合うかは別問題。

## 再現手順

```sh
W=/private/tmp/lean-doc-relay/w7e-syn
MODULES=benchmarks/results/it-modules.txt

experiments/stage7e/build.sh

# (1) gate — 環境依存
for E in init mathlib full; do
  MODULES=$MODULES experiments/stage7e/run.sh stage7e-parse-$E -- --env $E --print-errors
done
MODULES=$MODULES experiments/stage7e/run.sh stage7e-parse-full-open -- \
  --env full --open "$(paste -sd, $W/all-ns.txt)" --print-errors

# (2) コスト — 同一プロセス 5 周を 5 プロセス
MODULES=$MODULES experiments/stage7e/run.sh stage7e-cost -- --env full --repeat 5
```

生ログ: `benchmarks/results/stage7e-parse-*.jsonl` / `stage7e-cost.jsonl`
(`stage7e-rev-*` は別の問いなので混ぜない)。
