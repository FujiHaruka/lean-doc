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
| `decl-diff.ts` | 抽出した宣言を **IR (独立な出所)** と突き合わせる (deno)。後半 (5)(6) の集計 |

### 後半 (`--two-pass` / `--decls`)

前半の結論「活性化集合はファイルごとに正確でないといけない」を実装しうる唯一の形が **2 パス**:

1. 活性化なしでパースし、**そのモジュール自身の構文木**から `open` / `open scoped` /
   `open X in` / `namespace` を集める
2. 集めた名前空間だけ `Lean.activateScoped` して `openDecls` に積み、**同じモジュールを再パース**

`--accum` は活性化済み環境を次のモジュールへ引き継ぐ (**漏れの計測用**であって使う方式ではない)。
`--reverse` は同じことを逆順で回し、モジュール間に状態が漏れていないことを見る。
`--decls <out.jsonl>` は構文木だけから見える宣言 (完全修飾名・binder/型の生テキスト) を出す。

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

### (4) 2 パス方式は**成立する**。しかも環境は使い回せる【実測】

| 構成 | エラーの出たモジュール | エラー総数 | 構文木ノード数 |
|---|---:|---:|---:|
| `--env full` (再掲) | 215 / 432 | 989 | 3,140,799 |
| `--env full --open <122 名前空間>` (再掲) | 86 / 432 | 326 | 3,739,218 |
| 同上 − `ContDiff` (再掲) | 5 / 432 | 9 | 4,008,495 |
| **`--two-pass` (ファイル別活性化)** | **0 / 432** | **0** | **4,010,184** |

一律活性化の最良構成 (`ContDiff` を人手で抜いたもの) より**ノードが多く、エラーが 0**。
5 プロセス × 5 ラウンド + 逆順 + 追加 2 回、全ラウンドで 0/432。
活性化する名前空間はモジュールあたり p50 15 / max 31 (432 本で延べ 5,885)。

**環境は使い回せる — 捨てるコストは 0。** `Environment` は値で、`importModules
(leakEnv := true)` の環境は persistent なので、`activateScoped` は元を書き換えず
新しい値を返す。「捨てる」とは派生値への参照を落とすだけで、`importModules` の
2.6 s は**プロセスにつき 1 回しか払わない**。根拠は 3 つとも独立に実測:

- **パス 1 のエラーが常に 215 / 989** — 前半の `--env full` と完全一致。基底環境が
  汚れていれば `ContDiff` を処理した後のモジュールが壊れるはずで、壊れない。
- **逆順でも 215 / 989** — 順序の副作用ではない。
- **`--accum` (わざと引き継ぐ) は壊れる** — パス 2 が **52 / 432 (196 エラー)**。
  `ω` 汚染が実在することの対照実験。
- **peak RSS が動かない** — 1 パス 3.266 GB / 2 パス 3.266 GB (同一 A/B 組)。
  5 ラウンド × 432 = 2,160 回派生させたプロセスでも 3.24–3.28 GB。派生環境は溜まらない。

3 パス目は不要 (`wanted grew 0 / 432`)。

**コスト (warm、同一セッションで 1 パスと交互に測った)。** 5 プロセス連続では
`importModules` が 6.1→12.9 s に**悪化**していったので、前半の warm 値と直接は
比べられない。A/B を交互に回した 3 組 (`importModules` 2.60–2.73 s = warm) が下:

| | 1 パス | 2 パス (1 モジュール = パス1+走査+活性化+パス2+走査) |
|---|---|---|
| p50 | **6.78–7.09 ms** | **15.4–16.3 ms** |
| p90 | 23.2–24.6 ms | 50.3–53.1 ms |
| max | 54–57 ms | 112–118 ms |
| 432 本合計 | 4.29–4.38 s | 9.54–9.89 s |
| errModules | 215 / 432 | **0 / 432** |

内訳 (warm、`stage7e-ab-2pass-2` round 2 の p50): パス1 6.80 / 走査 0.18 /
`activateScoped` **0.91** / パス2 7.53 / 走査 0.24 ms。**2.2–2.3 倍**で、
`activateScoped` は全体の 3.8% にすぎない (支配項は 2 回パースすること自体)。

→ **撤退ラインは踏まない。** 編集中の 1 本で **15.4–16.3 ms**、環境ロード 2.6 s の 0.6%。

### (5) 宣言名 — 構文木 vs IR: **95.7% 一致、偽陽性 0**【実測】

突き合わせ先は独立な出所である既存の IR (`w7d/ir-j4/modules/*.json`、432 本、
`.declarations[].name`、**4,750 宣言**)。ハンドオフの「5,123」は段階 1 の粗いフィルタ後の
数で、doc-gen4 が `DocInfo` を作るのは 4,750 (検証ログ §段階 2 に既出)。**ここでは 4,750 を使う。**

| | 件数 |
|---|---:|
| 構文木から取れた宣言 (生) | 5,253 |
| うち `private` (doc-gen4 は出さないので除外) | 707 |
| 比較対象 | 4,546 |
| IR | 4,750 |
| **一致 (同一モジュール・同一完全修飾名)** | **4,546 = IR の 95.7% / 構文側の 100.0%** |
| 構文にしか無い (偽陽性) | **0** |
| IR にしか無い (取りこぼし) | 204 = IR の 4.3% |

取りこぼし 204 の内訳 — **elaboration が作るものだけ**:

| 件数 | 種別 | 構文から機械的に復元できるか |
|---:|---|---|
| 152 | structure のフィールド射影 (`structFields` の中) | できる (walker が降りていないだけ) |
| 37 | structure のコンストラクタ `.mk` | できる (合成) |
| 6 | `notation` が作る `«term...»` | できる (合成) |
| 3 | 無名 instance (名前を elaborator が合成) | **できない** |
| 6 | その他 (`irreducible_def` 2 / `initialize` 1 / `@[ext]` の `ext`・`ext_iff` 2 / `extends` の親射影 1) | マクロ次第 |

**equation lemma は 0 件** — この IR では別宣言ではなく `.equations` フィールドに入るため。
モジュール別再現率は p10/p50/p90 とも 100%、宣言集合が完全一致するモジュールが 369 / 432。
落ちるのは structure を持つ 17 本に集中している。

**1 パスと 2 パスで宣言名は byte 一致** (`--decls` の出力を突き合わせ済み)。
`parseCommand` はコマンド境界で復帰するので、**名前を拾うだけなら活性化は要らない**。

### (6) 署名 — 生テキスト vs IR の pretty-print: **byte 一致 7.5%**【実測】

「署名」の定義: `<binders> " : " <type>`。IR 側は `.binders` を空白 1 個で連結 + `.type`
(`typeCode` / `binderCode` / `modifiers` は使わない)、構文側は**同じ 2 領域の生ソース**。
**素朴な一致率であって `stage4c/coverage.ts` の採点ではない** (受け入れオラクルは 1 行も触っていない)。

| 正規化 | 一致 (4,546 中) |
|---|---:|
| 何もしない (byte 一致) | **340 (7.5%)** |
| + 空白・改行を潰す | 350 (7.7%) |
| + 名前修飾を落とす (`MeasureTheory.Measure` ↔ `Measure`) | 418 (9.2%) |
| + IR 側にしか無い先頭 binder を無視 (型は一致必須) | 1,301 (28.6%) |

binder だけ: byte 一致 16.5% / 型だけ: byte 一致 31.3%。

外れ方 (先に当たった理由で 1 件 1 分類):

| 割合 | 外れ方 |
|---:|---|
| 32.1% | binder も型も、修飾を落としてもなお違う |
| 26.2% | 型が違う (binder は先頭差で説明できる) |
| 19.4% | **IR がファイルに書かれていない binder を先頭に足している** (`variable` / auto-bound) |
| 12.4% | binder が違う (型は一致) |
| 7.5% | byte 一致 |
| 1.5% | 名前修飾だけの差 |
| 0.7% | ソースが型を書いていない (`def f := ...`) |
| 0.2% | 空白・改行だけの差 |

外れの中身は「構文が間違っている」ではなく **別のレンダリング**:
`fun T ↦ ..` ↔ `fun (T : ℝ) => ..`、`𝓝 0` ↔ `nhds 0`、`(· ≤ ·)` ↔ `fun (x1 x2 : ℝ) => x1 ≤ x2`、
`∀ w ∈ S, P` ↔ `w ∈ S → P`、`(n : ℝ)` ↔ `↑n`。

**ただし 1 つだけ本物の欠落がある**: binder group は IR 48,499 に対しソース 25,263 =
**52.1% しかソースに書かれていない**。1,692 宣言 (37.2%) で IR の binder 列がソースの
真の拡張になっていて、**14,225 の binder group (型クラス仮定を含む) がソースに現れない**。
`variable` ブロックが別の場所にあるから — これは整形の差ではなく**情報の欠落**。

**活性化は署名にだけ効く。** 1 パスだと型領域が途中で切れる宣言が **347 (7.6%)**、
2 パスでは **32 (0.7%)**。名前は同じでも、`≐` などを取りこぼした行の署名は壊れる。

→ **これは「プレビュー」ではなく「アウトライン」。**
名前は 95.7% (機械的復元を足せば ~99.9%) 揃うが、署名は IR と 7.5% しか一致せず、
37.2% の宣言で型クラス仮定が見えない。**doc-gen4 出力の代用にはならない**が、
「どの宣言があるか + ソースそのままの署名」を出す一覧としては成立する。

## 落とし穴 (この段階で踏んだ / 確かめた)

**1. `--env init` の p50 4.33 ms を「速い」と読まない。** ノードが 6.2 分の 1 しか
出ていない = 仕事量が少ないだけ。トークンテーブルの大きさによる純粋な減速と、
パースできた量の差は**分離できていない**。

**2. `parseHeader` はモジュールごとに `mkEmptyEnvironment` を呼ぶ** (全 env extension の
初期状態を作り直す)。Mathlib ロード済みだと 1 本 0.4 ms、432 本で約 200 ms
(合計の 4.4〜4.7%)。`headerUs` として分離してある。

**3. エラー数だけを見て「近似の質」を語らない。** ノード数とエラー数は
「パースが通ったか」しか言わない。宣言がどれだけ取れるか / 署名がどれだけ合うかは別問題。
実際、宣言名は 1 パスでも 2 パスでも同じで、差が出たのは署名の方だけだった。

**4. 「構文に出ない」と決めつけない。** 最初の walker は
`Lean.Parser.Command.declaration` しか見ておらず、そのままだと
**`lemma` (Mathlib のマクロコマンドで、`Lean.Parser.Command.declaration` ではない)** と、
**名前付き `instance` 全部** (`optional (ppSpace >> declId)` が null node に包むので
直下の子ではない) を落としていた。walker が数えたコマンドは
`declaration` 3,298 / `lemma` 1,971 / `open` 1,132 / `in` 1,040 / `variable` 970 …
で、この 2 種を落としたままの再現率を「構文の限界」と報告していたら数字は嘘になる。
`Parse.lean` は**見たコマンド種別を全部数えて出す**ので、walker が黙って無視している
ものが件数として出る。

**5. 計測セッションの状態が途中で悪化する。** 2 パスを 5 プロセス連続で回したら
`importModules` が 6.1 → 12.9 s に伸び、1 モジュールの p50 も 15.1 → 17.5 ms に流れた
(**warm への収束ではなく cold への発散**)。前半の warm 値と並べると 2 パスが不当に高く出る。
だから 1 パスと 2 パスを**交互に**回し直し、`importModules` が 2.6 s に戻っている
3 組だけを比較に使っている。

**6. `private` を落とさないと偽陽性が水増しされる。** doc-gen4 は private 宣言を
出さないので、707 件が「構文にしか無い宣言」として出る。除外して初めて偽陽性 0。

## 測れていないこと (この段階では分離できなかった)

- **`variable` の復元可能性。** 署名から消えている 14,225 binder group の出どころは
  `variable` ブロックで、構文木には `variable` 970 / `omit` 953 / `include` 25 コマンドとして
  **見えている**。ただし「どの変数がその宣言で実際に使われるか」は elaboration の問題なので、
  構文だけでできるのは過剰近似。**その過剰近似がどれだけ当たるかは未計測。**
- **2 パスを 1.5 パスにできるか。** コストの支配項は「2 回パースすること」で、
  最初の `open` より前を再利用する / ヘッダを使い回す等で削れるかは**試していない**。
- **`open X in` の位置依存。** 活性化集合はファイル単位の和で近似している。
  この対象では `ContDiff` を開くファイルが 1 本だけなので害が出なかっただけで、
  1 宣言のためだけに `open scoped ContDiff in` する書き方があると同じファイルの残りが壊れる。
  **位置ごとの活性化は実装も計測もしていない。**
- **署名の一致率は素朴なテキスト比較**であって `stage4c/coverage.ts` の採点ではない。
  「読めるかどうか」は一致率ではなく外れ方の表で判断すること。

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

# (4) 2 パス。1 パスと交互に回す (セッションの状態が動くため)
for i in 1 2 3 4; do
  MODULES=$MODULES experiments/stage7e/run.sh stage7e-ab-1pass-$i -- --env full --repeat 3
  MODULES=$MODULES experiments/stage7e/run.sh stage7e-ab-2pass-$i -- --env full --two-pass --repeat 3
done
MODULES=$MODULES experiments/stage7e/run.sh stage7e-2pass-rev   -- --env full --two-pass --reverse
MODULES=$MODULES experiments/stage7e/run.sh stage7e-2pass-accum -- --env full --two-pass --accum --print-errors

# (5)(6) 宣言と署名。IR と突き合わせる
MODULES=$MODULES experiments/stage7e/run.sh stage7e-decls-2pass -- \
  --env full --two-pass --decls $W/decls-2pass.jsonl
MODULES=$MODULES experiments/stage7e/run.sh stage7e-decls-1pass -- \
  --env full           --decls $W/decls-1pass.jsonl
deno run --allow-read --allow-write experiments/stage7e/decl-diff.ts \
  $W/decls-2pass.jsonl /private/tmp/lean-doc-relay/w7d/ir-j4/modules
```

生ログ (`stage7e-rev-*` は別の問いなので混ぜない):

| | |
|---|---|
| `stage7e-parse-*.jsonl` / `stage7e-cost*.jsonl` | 前半 (gate・コスト) |
| `stage7e-2pass-{1..5}.jsonl.gz` / `-{rev,accum,errors}.jsonl` | 2 パスの成立とリーク対照 |
| `stage7e-ab-{1,2}pass-{1..5}.jsonl.gz` | 同一セッション交互 A/B (コスト比較はここだけ使う) |
| `stage7e-declrecs-{1,2}pass.jsonl.gz` | 構文木から出た宣言レコード (名前・binder/型の生テキスト) |
| `stage7e-decl-diff-{1,2}pass.txt` / `-permodule.jsonl` | IR との突き合わせ結果 |
