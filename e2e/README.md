# e2e — 本物の Lean から本物のサイトまでを 1 本通す

`crates/lean-doc/tests/` の統合テストは**抽出器を `/bin/sh` の偽物に差し替えている**。
これは正しい判断で (Lean toolchain を要求したら誰も走らせない)、代償も 1 つだけ:
**抽出器と Rust の間の契約を検査するものが 1 つも無くなる**。`Extract.lean` が書く形を変えても
`cargo test` は全部緑のまま通る。ここはその穴を塞ぐ唯一の場所。

走らせるのは `tools/e2e-micro.sh`。

```
cargo build --bin lean-doc
tools/e2e-micro.sh
```

## なぜ Mathlib に依存しないのか

**計測対象は CI の判定には使えない** — import closure が数 GB あり、無料枠の外。
`micro/` は **Lean core だけ**に依存するので `lake build` が約 1 秒、抽出器のビルドが
約 17 秒【実測 2026-08-16、warm】で、無料のランナーで回る。

抽出器が Mathlib 無しで立つのは `import Lean` しか書いていないから
(→ `extractor/Extract.lean`)。`lake env` で借りる環境は**このフィクスチャのもの**でよい。

## フィクスチャが持っているもの — 「対象が持たない形」

`crates/lean-doc-render/tests/page_parts.rs` が記録している事実:

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

## 初回に出たもの【実測 2026-08-16】

このフィクスチャを最初に通した時点で、**レンダラの実欠陥が 1 件出た**。

`inductive` と `class_inductive` の **constructor がページに 1 つも描かれていなかった**
(`decl.rs` の分岐が body を空のまま返していた)。search 索引には載っているので、
**検索で選ぶとページ先頭に着地する**という壊れ方をする。

**既存の 355 本のテストは 1 本も反応しなかった**し、**byte 再現ゲートでも原理的に出なかった** —
オラクル (doc-gen4 の参照木) 自体が inductive を 1 つも含まないページ群だったから。
「全件バイト一致は分岐被覆の証明ではない」(→ `docs/milestone-log.md` M1) の一段強い形:
**オラクルの入力に無い形は、何バイト一致しても見えない。**

回帰は `crates/lean-doc-render/src/decl.rs` の
`an_inductives_constructors_are_rendered_with_their_own_anchors` が持つ。

## ゲート

`tools/e2e-micro.sh` が順に見るもの:

1. **1 コマンド** — `lean-doc build` がサイトを書き、`tools/site-gate.sh` が
   **内部リンクの 404 = 0 / 外部リソース = 0 / 索引とページが双方向で一致**を確認する
2. **冪等** — 同じコマンドをもう一度: **サイトのバイト不動**
3. **決定性** — 別のディレクトリへのフル生成が **1 回目とバイト一致** (サイトも IR も)
4. **`--jobs` 不変** — 抽出器の並列度を変えてもサイトも IR もバイト一致
5. **仕事量** — 2 回目が実際に**何もしなかった**ことを `lean-doc-build.json` の
   `work` から読む (再抽出 0 / 描画 0 / Lean 起動 0)

**3 が外部オラクルの代わりになる**もの — 「何バイトであるべきか」を誰にも聞かずに、
ハッシュ順・時刻・パスの混入を落とせる。
**5 が壁時計の代わりになる**もの — このワークロードは page cache で環境ロードが 5 倍動く
【実測、CLAUDE.md】ので秒数は閾値にできないが、**やった仕事の量は決定的な整数**で、
増分設計が主張しているのはまさにその形 (「無変更なら 0 ページ」)。

さらにブラウザ側は [`tools/browser-gate.sh`](../tools/browser-gate.sh) が別に見る
(この出力に対して回す) — 検索・ツリー・instances・テーマ・375 px・JS 無効。

### ゲート自身が壊れていた話【実測 2026-08-16】

作った当日に 2 件出た。**残しておく価値があるのは、どちらも「通っているように見える」形で
壊れていた**から:

- e2e の**ゲート 2 が、2 回目のサイトを自分自身にコピーして比較していた** — 何をしても通る
- corpus ゲートのテスト一覧が **`cargo test` の stdout と stderr の混ざり順に依存**していて、
  CI (非 TTY) では全部 `::name` に潰れた。**捕まえたのは CI を実際に回したこと**

**ゲートは自分では自分を検査しない。**

## 触るときの注意

- **`micro/` の宣言を消さない。** 1 つ 1 つが「対象が持たない形」を担当している。
  足すのは歓迎 (担当を上の表に書くこと)
- **Mathlib を足さない。** 足した瞬間にこれは CI で回らなくなる
- `micro/.lake/` は gitignored。`lake build` で作り直せる
