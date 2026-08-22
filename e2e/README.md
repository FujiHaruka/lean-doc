# e2e — 本物の Lean から本物のサイトまでを 1 本通す

`crates/litedoc4/tests/` の統合テストは**抽出器を `/bin/sh` の偽物に差し替えている**。
これは正しい判断で (Lean toolchain を要求したら誰も走らせない)、代償も 1 つだけ:
**抽出器と Rust の間の契約を検査するものが 1 つも無くなる**。`Extract.lean` が書く形を変えても
`cargo test` は全部緑のまま通る。ここはその穴を塞ぐ唯一の場所。

走らせるのは `tools/e2e-micro.sh`。**フィクスチャは 2 つあり、担当が違う**
(→ 下の「`consumer/` — なぜ micro と別なのか」)。

```
cargo build --bin litedoc4
tools/e2e-micro.sh          # micro/  — 宣言の形
tools/pinned-dep-gate.sh    # micro/ + micro-dep/ — 版固定できる依存 (git require に差し替える)
tools/lake-package-gate.sh  # consumer/ — Lake の配線
tools/lake-download-gate.sh # consumer/ — Release からバイナリを取る経路 (要ネットワーク)
```

## なぜ Mathlib に依存しないのか

**計測対象は CI の判定には使えない** — import closure が数 GB あり、無料枠の外。
`micro/` が依存するのは **Lean core と、このリポジトリの中にある `micro-dep/` だけ**なので
`lake build` が約 1 秒、抽出器のビルドが約 17 秒【実測 2026-08-16、warm】で、無料のランナーで回る。
**ネットワークは要らない** — `micro-dep/` は path で require している。

抽出器が Mathlib 無しで立つのは `import Lean` しか書いていないから
(→ `extractor/Extract.lean`)。`lake env` で借りる環境は**このフィクスチャのもの**でよい。

## フィクスチャが持っているもの — 「対象が持たない形」

`crates/litedoc4-render/tests/page_parts.rs` が記録している事実:

> **41 分岐のうち 9 つは、432 モジュール全部を通しても一度も発火しない** —
> `class` も `inductive` も `class_inductive` も無い、constructor が `mk` でない structure も、
> `ctor` member を持たない structure も、structure の range 内で宣言された継承 field も、
> field の implicit binder も、import の無いモジュールも無い。

curated な単体テストは**手で書いた IR** でこれらの分岐に到達している。だが
**実際のパイプライン (抽出器 → IR → ページ) を通ってこの形が描かれたことは一度も無かった。**
`micro/` はその形を**構成として**持つ:

| モジュール | 担当する形 |
|---|---|
| `Micro/Basic.lean` | **import の無いモジュール**。docstring 付きの def / theorem / structure / instance / `abbrev` (L3-1 が名指しした形) / inductive |
| `Micro/Notation.lean` | **`scoped notation`** — doc-gen4 が出せない唯一のもの (approach.md §10)。署名が `⟦n⟧` と印字されなくなったらここで出る |
| `Micro/Unicode.lean` | **U1 / U2 の罠** — `𝒜` (U+1D49C) は BMP 外なので、UTF-16 順ソートと UTF-8 順ソートが食い違う唯一の領域。docstring 内の markdown (heading / code span / リスト) も |
| `Micro/Shapes.lean` | **`class` / `class inductive` / 非 `mk` constructor / `extends` の継承 field / field の implicit binder** |
| `Micro/Dep.lean` + `../micro-dep/` | **版固定できない依存** — path require なので manifest entry に `url` も `rev` も無い。モジュール名は **`«Dep-Aux»`** (ギュメが要る形)。**版固定できない依存へのリンク**と**`.lidx` の綴り差**がここを通る (下記) |
| `Micro/Gen.lean` | **`@[ext]` が実現する宣言と、しない宣言** — inline の `@[ext]` / 後から来る `attribute [ext] Trip` / **1 つの位置に 2 つの親の子が 4 つ** (`attribute [ext] Quad Quint`) / `extends` の親射影 / そして**手書きの `@[ext] theorem`**。最後のものが要点で、**拡張に居ることは「生成された」を意味しない**ことをここだけが示す |
| `litedoc4.toml` + `docs/index.md` | **サイト設定** (feature-sweep C-3) — `title` と `index`。**何も設定していないパッケージでは 4 経路が自明に一致する**ので、`tools/config-gate.sh` が比較するものを持たせるために置いてある |
| `Micro/Math.lean` | **docstring の数式** (feature-sweep C-1) — インライン `$…$` / ブロック `$$…$$` / HTML が気にする文字を含む式 / **変換できない `\colim`**。最後のものが要点で、**失敗が `$…$` のまま残り、その件数が `work.mathFallbacks` に出る**ことをここだけが示す。対象は 5,079 docstring 中 3 span しか数式を持たないので、**対象では一度も通らない経路** |
| `Micro/Sorry.lean` | **`sorry` の 3 形** (doc-gen4 #270) — 直接 `sorry` を書いた定理 / それに依存するだけの定理 / どちらでもない定理。**`sorry` は elaborate 済みの項の性質**なので、手書き IR では「抽出器が正しい値を入れたか」を検査できない。ここが唯一の経路 |

## 初回に出たもの【実測 2026-08-16】

このフィクスチャを最初に通した時点で、**レンダラの実欠陥が 1 件出た**。

`inductive` と `class_inductive` の **constructor がページに 1 つも描かれていなかった**
(`decl.rs` の分岐が body を空のまま返していた)。search 索引には載っているので、
**検索で選ぶとページ先頭に着地する**という壊れ方をする。

**既存の 355 本のテストは 1 本も反応しなかった**し、**byte 再現ゲートでも原理的に出なかった** —
オラクル (doc-gen4 の参照木) 自体が inductive を 1 つも含まないページ群だったから。
「全件バイト一致は分岐被覆の証明ではない」(→ `docs/milestone-log.md` M1) の一段強い形:
**オラクルの入力に無い形は、何バイト一致しても見えない。**

回帰は `crates/litedoc4-render/src/decl.rs` の
`an_inductives_constructors_are_rendered_with_their_own_anchors` が持つ。

## path 依存を足して出たもの【実測 2026-08-17】

**2 件目も、フィクスチャを足した初回に出た。** `micro-dep/` を path で require した瞬間、
`tools/site-gate.sh` が **DEAD internal links 3 (1 distinct destination)** を出して落ちた。

**壊れていたのは相対リンクへのフォールバックそのもの。** `litedoc4` は依存のモジュールに
ページを書かず版固定 blob URL でリンクする (M7)。`ExternalLinks::href` は
「マップに root が無いモジュール」を**自パッケージのモジュール**とみなして相対ページリンクに
落としていたが、**版固定できない依存も同じ枝に落ちる** — 結果、**このサイトが決して書かない
ページへのリンク**が出る。死んだ 3 本は 3 経路とも別物だった:

| 経路 | 出ていたもの |
|---|---|
| ページ枠の import リスト | `<li><a href=".././Dep-Aux/Basic.html">«Dep-Aux».Basic</a></li>` |
| docstring 中の名前参照 | `<code><a href=".././Dep-Aux/Basic.html#DepAux.marker">DepAux.marker</a></code>` |
| 署名・equation 中の定数リンク | 同じ href |

**直した方針は「版固定できない依存にはリンクを張らない」** (名前はテキストで残す)。根拠は
このリポジトリが既に書いている原則 — `autolink.rs` の `module_for_source_path`:
「A link to the wrong page is worse than no link」。404 する相対リンクは、リンクが無い状態より
厳密に悪い。実装は `ExternalLinks` が **空 base の root** を持てるようにし、`href` を
`Option<String>` にしたもの。**自パッケージのリンクと、解決できた依存リンクはバイト不動。**

**単体テストでは出なかった。** `packages.rs` には「40 桁 hex でない rev は落ちる」テストが
以前からあり、それは**通っていた** — 落ちること自体は正しく、**落ちた後に何が描かれるか**を
見るものが 1 つも無かった。ページまで作らないと出ない形だった、というのがこの段の収穫。

### ギュメ付きモジュール名 — `.lidx` の綴り差は実在した【実測 2026-08-17】

`.lidx` はモジュール名を**非エスケープ**で書く (`Dep-Aux.Basic`)。IR と import リストは
**エスケープ済み** (`«Dep-Aux».Basic`)。`Micro/Dep.lean` の docstring が同じモジュールを
3 通りに綴っていて、**修正前**の解決結果は次のとおり割れた:

| docstring の綴り | 解決したか |
|---|---|
| `«Dep-Aux».Basic` (IR の綴り) | **する** |
| `Dep-Aux.Basic` (`.lidx` の綴り) | **しない** |
| `Dep-Aux/Basic.lean` (source path) | **する** — `module_for_source_path` が escape してから引くので |

**修正後は 3 綴りとも「リンクを張らない」に落ちる** (依存であって版固定できないので、それが正しい)。
つまり**この綴り差が出力に出るのは、版固定できる依存がギュメ付きモジュールを持つときだけ**。

### その実物を作った — `tools/pinned-dep-gate.sh`【実測 2026-08-22】

**フィクスチャは同じ `micro-dep` で、変えたのは配線だけ**。path require を git require に
差し替えると manifest entry が `type: git` + 40 桁 rev になり、版固定できる側に落ちる。
**モジュールも docstring も toolchain も同一なので、動く変数は版固定可能性だけ**。

ネットワークは要らない: ゲートが `e2e/micro-dep` から git リポジトリを作り、
git の `insteadOf` で remote を書き換える。**manifest には https の URL が残る** —
`file://` を入れると `site-gate.sh` がそれを**内部リンクと判定して dead link 5 本**を出し、
**製品の失敗と見分けがつかない**【実測: 最初にこれを踏んだ】。

測った結果:

| | |
|---|---|
| blob URL | `https://…/micro-dep/blob/<rev>/Dep-Aux/Basic.lean` — **ギュメは落ちている** |
| dead internal links | **0** |
| `«Dep-Aux».Basic` (IR の綴り) | **リンクする** |
| `Dep-Aux/Basic.lean` (source path) | **リンクする。上と同じ URL** |
| `Dep-Aux.Basic` (`.lidx` の綴り) | **リンクしない** |

**恐れていた壊れ方は無かった** — 版固定できる依存でギュメ付きモジュールへの blob URL は
正しく組める。**残ったのは 3 綴りのうち 1 つだけが解決しないという非対称**で、これは
測定ではなく**仕様の判断**。`module_for_source_path` は escape してから引き、
名前解決器は引かない — **同じ問い (「これはどのモジュールか」) に答える経路が 2 本あって
答えが違う**。判断は `docs/plans/residual-sweep.md` R3 に置いてある。
ゲートは**今の答えを pin している**ので、解決するようになったら落ちる。

**番号ではなく名前で指すこと** — README の一覧は `e744f79` で消えており、
番号参照はそれより先に腐っていた。

## `consumer/` — なぜ micro と別なのか

**フィクスチャは 2 つある。担当が違う。**

| | 担当 | 走らせるもの |
|---|---|---|
| `micro/` (+ `micro-dep/`) | **宣言の形** — 対象が持たない 9 分岐と版固定できない依存 | `tools/e2e-micro.sh` |
| `consumer/` | **Lake の配線** — litedoc4 を `require` した利用者の経路 | `tools/lake-package-gate.sh` / `tools/lake-download-gate.sh` |

`consumer/` は litedoc4 を **path で `require`** する最小パッケージで、
`lake run docs -- --out <dir>` が動くかだけを見る (計画は `docs/plans/lake-package.md` L1)。
検査しているのは、利用者が手で書けない 2 つの引数を Lake から取れているか:

- **`--extractor-bin`** — 抽出器を Lake が建てる (root の toolchain に対して建つので、
  版がずれようがない)
- **`--lib`** — `crates/litedoc4/src/lakefile.rs` は `lakefile.lean` を**名前で拒否する**
  (正直に読むには Lake で elaborate するしかないため)。`script docs` は
  **その elaborate の後に走る**ので Lake に聞ける

**`micro/` を流用しない**のは、あちらの母数と不変量を動かさないため。
`e2e-micro.sh` は「サイトのバイト不動」「フル生成 2 回がバイト一致」を主張していて、
そこに require を足すと**両方の主張の前提が変わる**。

### このフィクスチャが持っている形

**`lean_lib` が 2 つ、`defaultTargets` は 1 つだけ。** どちらも意図的で、
ゲートの 5 項目目 (`--lib` が Lake から来ているか) を**推測ではなく失敗**にするためにある:

- **2 つある**ので、最初の `lean_lib` だけを渡す実装・パッケージ名を渡す実装は
  **短いサイトを書いて成功を報告する** (`lakefile.rs` が名指ししている失敗の形)
- **`ConsumerExtra` が `defaultTargets` に無い**ので、`lake build` だけに頼った実装は
  olean の無いモジュールに当たって**大きな音で落ちる**。`script docs` が
  `defaultTargets` ではなく root パッケージの `lean_lib` を全部建てるのはこのため

宣言の形は網羅していない — **それは `micro/` の担当**で、ここに増やす理由は無い。

## ゲート

`tools/e2e-micro.sh` が順に見るもの:

1. **1 コマンド** — `litedoc4 build` がサイトを書き、`tools/site-gate.sh` が
   **内部リンクの 404 = 0 / 外部リソース = 0 / 索引とページが双方向で一致**を確認する
2. **冪等** — 同じコマンドをもう一度: **サイトのバイト不動**
3. **決定性** — 別のディレクトリへのフル生成が **1 回目とバイト一致** (サイトも IR も)
4. **`--jobs` 不変** — 抽出器の並列度を変えてもサイトも IR もバイト一致
5. **仕事量** — 2 回目が実際に**何もしなかった**ことを `litedoc4-build.json` の
   `work` から読む (再抽出 0 / 描画 0 / Lean 起動 0)
6. **1 モジュール編集** — 編集で `.lidx` が動かないこと、描いたページ数がモジュール数未満、
   残った木が**自分の IR の全描画と一致**すること
7. **`sorry` の 3 形** — `Micro/Sorry.lean` の 3 宣言が IR で **`"direct"` /
   `"transitive"` / キー無し**の 3 通りに分かれること。**名前で照合し、比較した本数を数える**
   (期待値が 1 つも走らなくても「問題なし」に見えるので)。加えて**他の宣言が `sorry` を
   名乗っていないこと** — 全部に `"transitive"` を返す分類器は前 2 つを通る
8. **属性が name と value に分かれて届く** — 宣言ごとの `attrs` を丸ごと照合し、
   IR 全体で**どの要素も 2 要素の文字列配列**であること、**属性名ごとの主張数**も数える
9. **生成宣言の由来** — `Micro/Gen.lean` の 9 宣言が `["ext", <実現の入力>]` を名乗り、
   **手書きの `Micro.Gen.Solo.ext` は名乗らない**こと。名前で照合し、比較した本数と
   由来を主張した総数を数える。加えて**`selectionRange == range` の 42 件のうち
   33 件は名乗っていない**ことを数える — この等式を「生成」と読み替えた実装は
   42 件を名乗ってここで落ちる。最後に**由来より前に並ぶ生成宣言が 0 件**であること
   (B-0 §13.2 の反証条件。3 版ぶんは
   [`../benchmarks/results/generated-decls-2026-08-21.txt`](../benchmarks/results/generated-decls-2026-08-21.txt))

**3 が外部オラクルの代わりになる**もの — 「何バイトであるべきか」を誰にも聞かずに、
ハッシュ順・時刻・パスの混入を落とせる。
**5 が壁時計の代わりになる**もの — このワークロードは page cache で環境ロードが 5 倍動く
【実測、CLAUDE.md】ので秒数は閾値にできないが、**やった仕事の量は決定的な整数**で、
増分設計が主張しているのはまさにその形 (「無変更なら 0 ページ」)。

さらにブラウザ側は [`tools/browser-gate.sh`](../tools/browser-gate.sh) が別に見る
(この出力に対して回す) — 検索・ツリー・instances・テーマ・375 px・JS 無効・**等幅フォントの字形**。

**最後の 1 つだけ入力がここに無い**: 検査 8 は**計測対象由来の非 ASCII 178 種**
(`benchmarks/tools/mono-charset.json`) を等幅スタックで描いて字形の欠けを見る。
**このフィクスチャに出る文字で判定すると意味が無い**からで — micro は意図的に小さいので、
`ℝ` を描けないフォントでも通ってしまう。**サイトは micro、文字集合は対象**という組み合わせは
このゲートだけ。

### ゲート自身が壊れていた話【実測 2026-08-16】

作った当日に 2 件出た。**残しておく価値があるのは、どちらも「通っているように見える」形で
壊れていた**から:

- e2e の**ゲート 2 が、2 回目のサイトを自分自身にコピーして比較していた** — 何をしても通る
- corpus ゲートのテスト一覧が **`cargo test` の stdout と stderr の混ざり順に依存**していて、
  CI (非 TTY) では全部 `::name` に潰れた。**捕まえたのは CI を実際に回したこと**

**ゲートは自分では自分を検査しない。**

## 触るときの注意

- **`micro/` の宣言を消さない。** 1 つ 1 つが「対象が持たない形」を担当している。
  足すのは歓迎 (担当を上の表に書くこと)。ただし**属性を持つ宣言を足すとゲート 8 の
  属性名ごとの本数が動く** — ゲートが名指しするので、その数を直す
  (structure を 1 つ足すと射影のぶん `reducible` が増える)
- **`Micro/Gen.lean` の `Micro.Gen.Solo.ext` を `@[ext] structure Solo` に「まとめない」。**
  手書きの ext 定理は**環境拡張には入る**ので、拡張だけを見る規則を落とすのはこの 1 形だけ。
  まとめるとゲート 9 の否定側の期待が消える (Mathlib 標本ではこの形が 20 件ある【実測】)
- **`Micro/Sorry.lean` の `sorry` を「直さない」。** `sorryHole` は**入力**で、
  他の 2 つはそれと違う答えでなければならない。`lake build` の
  ``declaration uses `sorry` `` 警告はこのフィクスチャの一部
- **`micro-dep/` を git 依存に変えない。** path であること (= manifest に `url` も `rev` も
  無いこと) がこのフィクスチャの担当。GitHub にすると**そのまま版固定リンクが組めてしまい**、
  上の 3 経路を守るものが消える
- **Mathlib を足さない。** 足した瞬間にこれは CI で回らなくなる
- `micro/.lake/` は gitignored。`lake build` で作り直せる
- **`consumer/` の `lean_lib` を 1 つに減らさない / `defaultTargets` に 2 つとも書かない。**
  どちらもゲート 5 項目目が見ている形そのもの (上の表)
- **`consumer/` に宣言の形を足さない。** 増やす先は `micro/`。ここを太らせると
  「Lake の配線を見るフィクスチャ」が 2 つ目の母数になる
