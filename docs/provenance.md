# 由来とライセンス

lean-doc のコードのうち **第三者の著作物に由来するもの**の一覧と、そこから出る義務の判断。
**この文書が由来判定の SoT。** 数字と引用はすべて【実測 2026-08-16】(ファイルを開いて確認した)。

法的助言ではなく**エンジニアリング判断**。判断の根拠を全部書いてあるので、覆したければ
根拠のどれが違うかを指摘すればよい。

---

## 1. 結論

| 問い | 答え | 根拠 |
|---|---|---|
| lean-doc は doc-gen4 の派生物か | **全体が派生物**として扱う。クリーンルームではなく、byte 一致を受け入れオラクルにして書いた | §1.1 |
| どのファイルが特に濃いか | 逐字コピーが 20 箇所、転写が 10 ファイル | §2 |
| lean-doc 自身のライセンスは | **Apache-2.0** (2026-08-16 決定)。`LICENSE` + `Cargo.toml` の `license` を設置済み | §4 (a) |
| `NOTICE` ファイルが要るか | **Apache 2.0 の義務としては不要** — doc-gen4 に NOTICE が無いから。ただし置いた (MIT の義務の置き場所として一箇所にまとまる) | §4 (d) |
| 生成サイトのフッタに表記が要るか | **不要**。配布している doc-gen4 由来物は CSS 8 行だけで、その元ファイルには著作権表示が無い | §5 |
| 抽出器 (`extractor/`) はどうか | **ここが一番濃い**。§4(b)(c) を履行済み | §2 A 表・§4 |

**§6 の 5 件はすべて完了。**

### 1.1 単位を間違えないこと — 全体が派生物

**§2 のファイル単位の棚卸しは、それだけだと関係を過小評価する。**
「オリジナルのコードに 20 箇所のコピーが混ざっている」ではなく、
**「レンダリング層は doc-gen4 の再実装で、その周りに独自の IR・増分・CLI・UI を足したもの」**
と読むのが正しい。理由は 2 つ:

1. **クリーンルームではない。** クリーンルーム実装とは「A がソースを読んで仕様書を書き、
   B が仕様書だけを見て実装する」という**人的な分離**を指す。ここにその分離は無く、
   逆に各ファイルの doc comment が "transcribed" / "Ported from" と自認している
2. **byte 一致を受け入れオラクルにした** (M6 で 99.5062%)。これは著作権の主張に要る 2 要素
   — **access** (ソースを見た) と **substantial similarity** (実質的類似) — を、
   こちらが自分の手で記録に残したのと同じ

**M7 で byte 互換を捨てたことは、由来を打ち消さない。** 書き方が転写だった事実は、
将来の方針では変わらない。

**それでも義務は増えない。** Apache-2.0 は permissive で **copyleft が無い**ので、
§4 の 4 条件は「8 行が派生」でも「全体が派生」でも同じ。むしろ §4 の末尾は
派生物**全体**に別の条件を付けることまで明示的に許している:

> You may add Your own copyright statement to Your modifications and may provide
> additional or different license terms and conditions for use, reproduction, or
> distribution of Your modifications, or **for any such Derivative Works as a
> whole**, provided Your use, reproduction, and distribution of the Work
> otherwise complies with the conditions stated in this License.

全体を Apache-2.0 にした時点で、この選択肢のうち最も安全側に倒し切っている。

**変わるのは義務ではなく表明 (§4(b) を著作物レベルで払う)。**
README が 300 行にわたって速度比較を並べたあと最後に由来へ触れる構成だと、
読者は「速い独立実装」と受け取る。これは CLAUDE.md の
「**自分に有利な数字が出たときほど疑う**」を、数字ではなく**由来**に適用したときの
失敗そのもの — 順序が有利な方向に効いている。
→ README 冒頭・`NOTICE` 冒頭に著作物レベルの告知を置いた (2026-08-16)。

**商標 (§6)**: doc-gen4 の名前を「由来の説明」と「比較」に使うのは §6 が明示的に
許す範囲 ("reasonable and customary use in describing the origin of the Work")。
**推奨・提携を示唆しないこと**が線。`NOTICE` に「doc-gen4 ではない / 著者の推奨を
受けていない」と明記した。

**Apache-2.0 を選んだ理由**: doc-gen4 由来の部分に Apache-2.0 の条件が残る以上、
リポジトリ全体を別ライセンスにすると「ここは例外」の但し書きが要る。Lean・Mathlib・
doc-gen4・計測対象リポジトリがすべて Apache-2.0 なので、揃えれば `LICENSE` 1 本で §4(a) が
閉じ、特許条項も付く。MIT とのデュアルは、MIT 側が自作部分にしか及ばないので説明が長くなるだけ。

---

## 2. 4 段階の棚卸し

分類の定義:

| | |
|---|---|
| **A: コピー** | doc-gen4 のコードをそのまま (または同言語で機械的に) 持ってきた。**著作権が及ぶ** |
| **B: 移設** | 実装を読んで別言語で書き直した。構造・出力が一致するよう意図している。**及ぶ可能性がある** |
| **C: 設計の踏襲** | class 名・URL 形・ファイル配置などインタフェースだけ合わせ、実装は独立。**及びにくい** (Apache 2.0 §1 が "merely link (or bind by name) to the interfaces of the Work" を派生物から除いている) |
| **D: 無関係** | doc-gen4 を通っていない |

### A — コピー (20 箇所)

| パス | 規模 | doc-gen4 側 |
|---|---|---|
| `extractor/Extract.lean` の 15 箇所 (`isProjFn` `isBlackListed` `tagAttributes` `inlineAttrString` `externEntryString` `externAttrString` `deprecationString` `getTags` `getAllAttributes` `getInstanceTypes` `getInstPriority` `getDefaultInstanceAttr` `getFieldOrigin` `mkTacticOut` / `Core.Context` の 4 options) | **計 約 112 行** (ファイル 2,995 行の 3.7%) | `Process/{DocInfo,Attributes,InstanceInfo,StructureInfo,Analyze}.lean`, `Load.lean:30-42` |
| `crates/lean-doc-render/assets/style.css:320-325` (`.fn`) / `:346-347` (`.break_within`) | **8 行** | `static/style.css:608-615` / `:664-670` |
| `crates/lean-doc-md/tests/data/docgen4-expected.json`, `crates/lean-doc-render/tests/data/docgen4-linked-expected.json` | **371,488 B** | doc-gen4 の**出力**。ソースではない |
| `benchmarks/doc-gen4-instrumentation.patch` | 全 441 行のうち **context 187 行が doc-gen4 のソース逐語** (Apache ヘッダ行を含む) | `Load.lean` `Output.lean` `Process/Analyze.lean` `Main.lean` を改変 + `Timing.lean` 新設 |

**Lean → Lean は「移設」ではなく「コピー」。** 同言語なので書き直しの余地が無く、実際に逐字一致する。
例 (`Extract.lean:270-282` ↔ `DocInfo.lean:151-165`) — 差分はコメントの削除と `(declName.isInternal)`
の括弧だけ。ファイル内のコメント自身が "Kept identical on purpose" (`:256`) と宣言している。

### B — 移設 (10 ファイル)

| ファイル | 行数 | 由来の度合い | doc-gen4 側 |
|---|---|---|---|
| `crates/lean-doc-md/src/html.rs` | 680 | **ほぼ全体**。全分岐が 1 対 1、出力バイトを保存する意図。冒頭が "`DocString.lean:112-402`, transcribed" | `Output/DocString.lean:112-402` |
| `crates/lean-doc-render/src/code.rs` | 873 (テスト 366 を除き約 500) | **レンダリング本体**。§3 参照 | `Output/Base.lean:247-288, 327-395` |
| `crates/lean-doc-render/src/autolink.rs` | 930 | 一部 (`nameToLink?` / `moduleNameToLink` / `getRoot`)。上流の写像構築は独自 | `Output/DocString.lean:39-80` |
| `crates/lean-doc-render/src/whitespace.rs` | 203 | 全体。ただしオフセット形への作り直しで実装は別 | `Output/Base.lean:281-288` |
| `crates/lean-doc-md/src/escape.rs` | 83 | 全体 (`Html.escape`)。アルゴリズムは別、出力集合は同一 | `Output/ToHtmlFormat.lean:35-55` |
| `crates/lean-doc-render/src/decl.rs` | 1,053 | **判断のみ数十行相当**、マークアップは M8-b で自前に置換 | `Output/{Module,Definition,Structure,…}.lean` |
| `crates/lean-doc-render/src/page.rs` | 401 | **判断のみ 2 つ** (抑止集合・並び順) | `Output/Module.lean:181-188` |
| `extractor/Extract.lean:706-777` (`collectSpans` 周辺) | 約 90 | `renderTagged` の walk を span 列に作り直し | `RenderedCode.lean:150-157, 240-274` |
| `extractor/Extract.lean:1403-1447` (kind/modifiers) | 約 45 | `getKindDescription` を分解して IR に載せる | `Process/DocInfo.lean:211-246` |
| `extractor/Extract.lean:1328-1400` (`structureMembers`) | 約 70 | `getFieldTypes` の計算内容 | `Process/StructureInfo.lean:49-` |

**`html.rs` の 680 行が本件の最大の判断ポイント。** Lean → Rust で言語は変わっているが、
分岐構造・順序・出力バイトのすべてを保存する意図で書かれていて、「機械的な言語変換」に
近い。A に寄せて扱うのが安全側。

### C — 設計の踏襲 (10 ファイル)

`external.rs` / `packages.rs` (ソース URL の形)、`link_index.rs`、`span.rs`、`frame.rs`、
`order.rs` (移設元は Lean core)、`flags.rs` (由来は md4c、doc-gen4 由来は式 1 本)、
`style.css` の残り 525 行、`ast.rs` (由来は MD4Lean)、`prune.rs`。

`style.css` が doc-gen4 と共有するセレクタは 7 個 (`.break_within` `.decl` `.fn` `.hover-link`
`.imports` `.js` `.name`) だけで、宣言まで一致するのは A に挙げた 2 箇所のみ。
`.decl` は doc-gen4 が `margin-top:20px;margin-bottom:20px`、lean-doc は
`padding-block:1.5rem;border-top:…` で**全く別物**。

### D — 無関係

`crates/` の `.rs` は 62 ファイル。doc-gen4 に言及するのは 41 ファイルあるが、
**言及の大半はコメント内の設計根拠の説明**で、コードが由来しているのは B/C の 20 ファイル。
`tools/` 9 本は全て D。`assets/app.js` (546 行) と `assets/favicon.svg` は**新規** — doc-gen4 の
12 本の JS と突き合わせて共有識別子は DOM API と英単語のみ、`favicon.svg` は共通要素ゼロ。

**ビルド時に doc-gen4 をリンクしない。** `extractor/Extract.lean` は `import Lean` だけ (`:145`)。
Rust 側も依存しない。`import DocGen4` するのは**テストオラクル 2 本だけ** — 製品には入らない。

---

## 3. `code.rs` — 最内層の HTML は doc-gen4 のまま

`code.rs` が出す要素は `span.fn` / `span.name` / `a[href]` の 3 つで、doc-gen4 の
`renderedCodeToHtmlAux` が出す集合と一致する。

| | doc-gen4 | lean-doc |
|---|---|---|
| `.fn` ラッパ | `Base.lean:389` `#[<span class="fn">[html]</span>]` | `code.rs:199,221` |
| sort のリンク先 | `Base.lean:375` `s!"{← getRoot}foundational_types.html"` | `code.rs:208-209` |
| anchor 抑止 | `Base.lean:342-345` 内側に `<a>` があれば自分は出さない、戻り値は `true` | `code.rs:205-211, 224-228` |
| `breakWithin` | `Base.lean:247-251` `.` で分割し各片を `span.name` に | `code.rs:494-505` |
| 宣言 URL | `Base.lean:188-190, 231-234` `{root}{parts}/…html#{name}` | `autolink.rs:70-86`, `decl.rs:90-104` |
| `.const` 解決 | `Base.lean:337-373` の 4 段 | `code.rs:248-273` (同じ 4 段) |

**一致していない側**: doc-gen4 の `.keyword` / `.string` / `.otherExpr` 分岐は IR に無い。
`findLinkableParent` は doc-gen4 が `Name` 構造を見るのに対し `code.rs:363-377` は印字済み
文字列しか無いので「最終成分が全部 ASCII 数字か」で `.num` を判定する。
そして **`code.rs` の外側** (`section.decl` / `header.decl-head` / `div.sig` / `details.extra`) は
M8-b で全部書き直され、doc-gen4 の `div.decl_header` / `span.decl_kind` / `div.decl_type` /
`nav.internal_nav` とは**一つも一致しない**。

→ **一致しているのは最内層だけ。**

---

## 4. Apache 2.0 の義務

doc-gen4 のライセンスは **Apache License 2.0**
(`/Users/haruka/dev/lean-projects/.lake/packages/doc-gen4/LICENSE`、rev `0bc516c1`)。
各 `.lean` の先頭に `Copyright (c) 2021 Henrik Böving. All rights reserved. /
Released under Apache 2.0 license as described in the file LICENSE. / Authors: Henrik Böving`。

§4 は**配布したとき**に発動する。**2026-08-16 の public 化で発動した** — 4 条件はいずれも
発動前 (§6、同日) に払ってあるので、public 化そのものが要求した作業は無い:

| | 条件 | 本件での状況 |
|---|---|---|
| **(a)** | 派生物の受領者にライセンス本文の複製を渡す | **履行済**。`LICENSE` (canonical Apache-2.0 201 行) がツリーにある |
| **(b)** | 変更したファイルに「変更した」旨の目立つ告知を付ける | **履行済**。逐字コピー 6 箇所それぞれ + README / NOTICE 冒頭 (→ §6) |
| **(c)** | Source 形式の派生物に、原著作物の著作権・帰属表示を保持する | **履行済**。`Extract.lean` / `style.css` / md クレート各ファイル (→ §6 の表) |
| **(d)** | 原著作物が NOTICE ファイルを含むなら、その内容を派生物にも入れる | **発動しない** — **doc-gen4 に NOTICE ファイルが無い**【実測: 直下は `LICENSE` のみ】 |

**いま何を配布しているか**が結論を分ける:

| 配布物 | 状態 | doc-gen4 由来物 |
|---|---|---|
| **lean-doc リポジトリ** | **public = 配布中** (2026-08-16 に private から変更) | A の 20 箇所すべて。**§4 は発動した。義務は発動前に払ってあるので、新たな履行は無い** — (a) `LICENSE`・(b)(c) 各ファイルの帰属表示と README / NOTICE 冒頭の告知はすべて §6 で置いた (2026-08-16)、(d) は不発動 |
| **生成サイト** (<https://fujiharuka.github.io/information-theory/>) | **public = 配布中** | **`style.css` の 8 行のみ**。HTML の class 名と URL 形は C (インタフェース)。ホスト先の `FujiHaruka/information-theory` は**既に Apache-2.0**【実測: `gh api`】 |
| **Release のバイナリ** (`lean-doc`, `.tar.gz`) | **これから配布する** (→ `plans/distribution.md` D2) | **Object form**。§4 は "in Source **or Object** form" なので発動する。(a) **`LICENSE` を書庫に入れる**。(b) 著作物レベルの告知は `NOTICE` が持つ。**(c) は "in the Source form of any Derivative Works" と限定されているので Object 単体では不発動**。(d) 不発動。**新しく効くのは Apache ではなく MIT** — 下の枠 |

> **配布形態が増えたとき、参照で済ませていた義務は崩れる**【2026-08-18】。
> `NOTICE` は md4c について「全文は `vendor/md4c/LICENSE.md` にある」と**指していただけ**だった。
> リポジトリ配布ではその通りだが、**バイナリの書庫には vendor/ が入らない** — md4c の MIT は
> 許諾文を "all copies or substantial portions" に求めており、コンパイル済みの `lean-doc` は
> md4c を含む。指し先が同梱されない配布形態では、ポインタは義務を運ばない。
> → **`NOTICE` に md4c の MIT 全文を入れた** (MD4Lean は最初からこの形だった)。
> **書庫に入れるのは `LICENSE` と `NOTICE` の 2 つで足りる**、が結論。

**非対称が要点**: UI-4 ゲート (`docs/plans/ui-redesign.md:21`) が「doc-gen4 の資産が 1 本も残って
いない」を達成した結果、**配布物からは doc-gen4 由来物がほぼ抜けた**。残っているのは
ソースツリー側 — つまり**まだ配布していない方**に集中している。

---

## 5. 生成サイトのフッタは要らない

理由を 3 つとも満たすので不要:

1. **§4(d) が発動しない。** doc-gen4 に NOTICE ファイルが無い。フッタ表記
   ("within a display generated by the Derivative Works") は (d) の履行手段の一つであって、
   (d) が無ければ手段も要らない
2. **配布している doc-gen4 由来物が CSS 2 規則 8 行だけ。** class 名・URL 形・HTML 構造は
   §1 の "bind by name to the interfaces" 側。生成される HTML 自体は IR から作った
   このパッケージの内容であって doc-gen4 の著作物ではない
3. **§4(a) の「受領者にライセンス本文を渡す」はホスト先が満たしている。**
   `FujiHaruka/information-theory` は Apache-2.0 のリポジトリで、`LICENSE` が同じ配布物の中にある

**§4(c) も、この 8 行については発動しない**【実測】 — doc-gen4 の `static/style.css` は
**著作権表示を 1 行も持たない** (`static/` 配下に `copyright` / `Böving` / `apache` の
文字列がゼロ)。(c) は「原著作物の Source 形式にある表示を保持せよ」なので、
**保持すべき表示が存在しない**。

それでも `style.css` に帰属を書いたのは義務の履行ではなく**方針**: 出典の分からない
コード片を残さない方が、後で由来を辿り直すコストが安い。

**未配布の差分が 1 つある**: いま公開されている `style.css` は帰属コメントが入る前の版。
**次にサイトを作り直すときに自然に入る**ので、そのためだけの再デプロイはしない
(義務が無いことは上のとおり)。**再デプロイしたらこの段落を消す。**

---

## 6. やったこと (2026-08-16 完了)

| # | やったこと | 置いた場所 |
|---|---|---|
| 1 | **ライセンスを Apache-2.0 に決めた** | §1 |
| 2 | `LICENSE` (canonical Apache-2.0 201 行) を置き、`[workspace.package]` に `license = "Apache-2.0"`、各 crate に `license.workspace = true` | `LICENSE`, `Cargo.toml`, `crates/*/Cargo.toml` |
| 3 | `NOTICE` を置いた — doc-gen4 / md4c / MD4Lean / UnicodeBasic + Unicode® / V8 の 5 件 | `NOTICE` |
| 4 | **§4(b)(c) の履行** — 下表。加えて **§4(b) を著作物レベルでも払った** (README 冒頭 / `NOTICE` 冒頭。→ §1.1) | 各ファイル + README + NOTICE |
| 5 | **第三者コードの記録** — 生成フィクスチャに `PROVENANCE.md` を足した (`vendor/md4c/PROVENANCE.md` と同じ作法) | `crates/lean-doc-{md,render}/tests/data/PROVENANCE.md` |

§4(b)(c) を書いた場所:

| ファイル | 何を書いたか |
|---|---|
| `extractor/Extract.lean` | 冒頭に全体の告知 + **逐字コピー 6 箇所それぞれに** Apache ヘッダ (blacklist / attributes / InstanceInfo / `getFieldOrigin` / `mkTacticOut` / `Core.Context` の options) |
| `crates/lean-doc-render/assets/style.css` | ファイル冒頭に Apache ヘッダ、`.fn` と `.break_within` の各規則に出典行 |
| `html.rs` `escape.rs` `code.rs` `whitespace.rs` `autolink.rs` | 冒頭 2 行の告知 (移設だが安全側に倒した) |
| `parse.rs` `ffi.rs` `gc.rs` `v8_gc.rs` | 同上 (MD4Lean / md4c / UnicodeBasic + Unicode® / V8) |
| `benchmarks/doc-gen4-instrumentation.patch` | diff の前に前書き。**`git apply` は前書きを読み飛ばす** — `apply-instrumentation.sh --check` が `APPLIED` を返すことと、diff 本体が 1 バイトも変わっていないことを確認済【実測】 |
| `tests/oracle/gen-gc-table.ts` / `gen-v8-gc-table.ts` | **生成器側**に書いた。`gc.rs` / `v8_gc.rs` は `--check` が生成器の出力と突き合わせるので、生成物を直接編集すると赤くなる |

---

## 7. doc-gen4 以外の第三者コード

| | ライセンス | 規模 | 由来ファイル |
|---|---|---|---|
| `crates/lean-doc-md/vendor/md4c/` (md4c 0.5.2) | MIT © 2016-2024 Martin Mitáš | 6,489 行の C | **有り** — `LICENSE.md` + `PROVENANCE.md`。**唯一の前例** |
| `crates/lean-doc-md/src/parse.rs` (MD4Lean `wrapper/wrapper.c` の transliteration) | MD4Lean = MIT © Jz Pan | 743 行 | **無し** |
| `crates/lean-doc-md/src/ffi.rs` (`md4c.h` の転写) | md4c = MIT | 342 行 | 無し (vendor の PROVENANCE が間接的に覆う) |
| `crates/lean-doc-md/src/gc.rs` (UnicodeBasic の出力を列挙したデータ) | UnicodeBasic = Apache 2.0 | 1,691 行 | 無し (冒頭に rev `a2e430a4…` の記録のみ) |
| `crates/lean-doc-global/src/v8_gc.rs` (V8 を総当たりした出力データ) | V8 = BSD-3 | 818 行 | 無し (deno 2.7.14 / V8 rev の記録のみ) |

`gc.rs` / `v8_gc.rs` は**プログラムの出力**であって元のソースではない。
`tests/data/docgen4-*.json` も同じ性質。ソースの複製とは扱いが違うが、
**由来の記録は等しく要る** — 再生成の手順が分からなくなる方が実害が大きい。

---

## 8. この判断が外れるとしたら

- **`html.rs` の 680 行を「移設」ではなく「コピー」と見るべきだった場合。**
  結論は変わらない (どちらでも §4(b)(c) を払う) が、**ファイル冒頭に帰属表示が要る**度合いが上がる
- **~~`experiments/` を配布物に含めた場合~~ → 決着した (2026-08-16)。** `experiments/` は
  **HEAD から撤去した**ので、v0.1 の配布物にも作業ツリーにも入らない。棚卸しの対象外で確定。
  ただし**履歴には残っている** (tag `experiments-frozen`) — **2026-08-16 の public 化で
  実際に読める状態になった**。中身は doc-gen4 を読んで書いた TS で、リポジトリ全体が
  Apache-2.0 + `NOTICE` 済なので**追加の義務は発生しない** (§4 が既に覆っている)。
  **撤去の理由は方針であって法務ではない。**
- **`benchmarks/doc-gen4-instrumentation.patch` を配布した場合。** context 187 行が
  doc-gen4 のソース逐語なので、patch 単体で §4(a)(c) が発動する
