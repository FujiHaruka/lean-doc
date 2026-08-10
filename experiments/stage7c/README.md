# stage 7c — 生成側の残り: autolink 索引と CommonMark

段階 7 の続き。**目的は 1 つ**: 段階 7b が 97.099% で残した「到達可能なのにやって
いないだけ」の 2.407 pt を閉じ、**到達上限 99.506% に届くか**を出す。

数字とその条件は `benchmarks/results/stage7c-summary.txt` (SoT)。
ここには**何をなぜ変えたか**だけを書く。

## なぜ新しいディレクトリなのか

stage7b を書き換えず、その写しをここに置いた。CLAUDE.md の「新しい段階は既存を壊さず
新ディレクトリを足す (数字の再現性のため)」に従う。stage7b の 97.099% / 完全一致 170
ページ / 生成 0.8044 s はコミット済みで docs から引用されている。

**抽出側は 1 バイトも変えていない。** `Extract.lean` / `build.sh` / `run.sh` は
stage7b の写しで、使った IR も stage7b が書いた schema-4 IR そのもの。この段階は
レンダラだけの段階なので、抽出器を再ビルドしても同じ IR が出る。

`experiments/stage4c/coverage.ts` は**受け入れオラクル**なので、この段階でも
**1 行も変えていない**。

## 中身

| ファイル | stage7b との差 |
|---|---|
| `render.ts` | `--link-index`、CommonMark の block parser、`isNameLit` を Lean の `splitNameLitAux` に、`moduleDeclNames` を宣言範囲順に、`_`/`*` の強調を md4c の flanking 判定に。CommonMark ablation フラグ 7 つ |
| `build-link-index.ts` | **新規**。`declaration-data.bmp` → 依存クロージャ全体の「名前 → モジュール」索引 |
| `autolink-check.ts` | **新規**。doc-gen4 のアンカーとの多重集合差 (出せていない / 過剰 / href だけ違う) |
| `md-diff.ts` | **新規**。byte 一致しない prose 領域を **markdown 原文つき**で出す (CommonMark の反復ループ) |
| `rev-check.ts` | **新規**。最後に残ったラベルを中身で確認する |
| `Extract.lean` / `build.sh` / `run.sh` / `check-spans.ts` / `member-check.ts` / `instance-check.ts` | stage7b の写し (ラベル文字列のみ 7c に) |

## (1) autolink 索引 — §8 の未解決の問いそのもの

doc-gen4 の `nameToLink?` (`Output/DocString.lean:39-80`) は **環境全体**
(`name2ModIdx`) を引く。IR にはそれが無く、`deps/` は署名が参照した名前しか持たない。
足りないのは自パッケージの写像ではなく**依存クロージャ全体の写像**で、それは
doc-gen4 の出力 `declarations/declaration-data.bmp` がそのまま持っている
(`approach.md` §5.3 が「この写像だけで 99.992% 同じ URL が出る、要る列は (名前,
モジュール) の 2 列で `kind` は不要」と実測済み)。

**前提を 1 つ明記する。これは「上流パッケージの公開ドキュメントサイトから同じ
ファイルが手に入る」ことを前提にした実装である。手に入るか、バージョン対応が
取れるかは未検証** (`approach.md` §8 の未解決欄)。ローカルの doc-gen4 出力から
作っているのは**計測のため**であって、**配布方式を決めたわけではない。**
取れないなら生成側で作ることになり、そのコストはここで測った値ではない。

出した 3 数字 (詳細は summary §2):

| | |
|---|---:|
| 実サイズ | **8,508,273 B** / gzip **1,332,647 B** (宣言 258,760) |
| 作る時間 | **0.4152 s** |
| レンダラが読む時間 | **0.0764 s** (プロセス起動ごと) |

§5.3 の「下限 53 KB / 上限 34.3 MB」に対して **8.51 MB** に落ちた。

### 索引を `known` に混ぜていない理由

`known` は署名側 (`Renderer.constLink` の `findLinkableParent` フォールバック) も
使う。そこは今 byte 完全 (3,477/3,477) なので、50 倍大きい写像を下に敷いて結果が
変わる余地を作らない。`nameToLink` は `known` → 索引の順に引くので、**既に解決して
いたものの解決先は変わらない**。検証: `--out` の JSONL が stage7b と byte 一致
(`stage7c-header-identity.txt`)。

### 索引だけでは足りなかった 2 つ

* **`isNameLit`** が `'` `!` `?` を含む名前を落としていた。Lean の `isIdRest` は
  この 3 つを含む。`Init/Meta/Defs.lean:1180-1203` の `splitNameLitAux` を写した。
* **モジュール内フォールバックの探索順**。`nameToLink?` は
  `moduleInfo[…].members` を順に見る。これは**宣言範囲でソートされた列**で、
  `filterDocInfo` は `shouldRender` で絞らない (= ページに出ない宣言も入る) が
  private は除く。7b は IR の並び順を渡していて `sincN` の候補 2 本から別の方を
  選んでいた。

## (2) CommonMark — md4c の部分集合

doc-gen4 は MD4Lean = C の md4c を
`MD_DIALECT_GITHUB ||| MD_FLAG_LATEXMATHSPANS ||| MD_FLAG_NOHTML` で呼ぶ。
**外部依存は足していない** — このリポジトリに TS の依存管理機構は無く、
`docs/plans/three-axes.md` 事前決定 5 がネットワークを禁じ、そもそも npm/jsr の
CommonMark 実装はどれも md4c と byte 一致しない。`renderDocString` を
ブロックパーサに書き直し、`renderBlock` / `renderLi` を
`Output/DocString.lean:287-363` から写した。

**実装したのはこの対象が使う構文だけで、md4c 全体ではない。** 無いもの:
GFM tables / strikethrough / task list / hard break / 参照リンク / 画像 /
permissive autolink。doc-gen4 の 348 ページに `<table>` `<del>` `<br>`
checkbox が 0 個であることは確認済み (実測)。**使う対象が来たら
`coverage.ts` がその領域を落とす** — 黙って近似したことにはならない。

### 効いた規則は ablation で測った

「どの規則が何領域を閉じたか」はコードを読んで割り振らず、**規則を 1 つずつ外して
採点し直した** (段階 7b が 3 つの抽出追加を ablation で測ったのと同じ)。
`--md-no-indented-code` など 7 フラグがそのため。**これらを付けた run のページは
意図的に間違っている**ので、`--stats` に ablation 名を印字して、点数の出所が
混ざらないようにしてある。結果は `stage7c-commonmark-ablation.txt`。

一番効いたのは **lazy continuation (49,663 B / 23 領域)** と
**項目境界 (46,595 B / 18 領域)**。後者は cmark の
`interrupts_paragraph = (container->type == PARAGRAPH)` の帰結で、
**同じ「リストマーカーは段落を割れるか」という問いが 2 つの階層で別々の答えになる**:

* 段落の中 (`parseBlocks`) — `2006. Theorem` は段落の続き (順序付きは `1.` だけが割れる)
* 項目の続きの判定 (`parseList`) — `2. Cauchy-Schwarz` は新しい項目
  (字下げが足りない時点で項目は閉じているので、容器は段落ではなくリスト)

7b はここを一様に扱って、`1. 2. 3.` を全部 1 項目に押し込んでいた。

## 落とし穴 (実測で確認済み、踏むと静かに壊れる)

**1. 採点は具体的な rev を埋めた `--source-url` で。** `{{SOURCE_URL}}` で描くと
未分類が跳ねて −3.1 pt ずれる。

**2. `coverage.ts` の原因ラベルを鵜呑みにしない。** 段階 7b で members の規則が
前提を失った。今回**最後に残ったラベル (rev) も中身で確認した** — 540/540 が
rev のみの差 (`rev-check.ts`)。

**3. 母数を静かに縮めない。** タイル検査 (segmentation gap/overlap で throw) が
母数を守る唯一の仕組み。今回もそこは触っていない (領域合計 = 22,028,728 B)。

**4. `coverage.ts` は並び順を見ない。** 領域を宣言**名**で対応付けるので、同じ
集合の並べ替えは一致と数える。実際に 2 ページが並び順だけで byte 不一致のまま残る
(段階 7b から変わっていない)。**99.506% はその 2 ページを含んだ数字。**

**5. 単位が 2 つ生きている。** スパンのオフセットは UTF-16 code unit、
`RenderedCode.textLength` は code point。**CommonMark のインライン処理でも
`[...s]` を使わない** — block parser も `renderInline` も添字は code unit のまま
扱い、code point が要る場所 (`codePointAt` の桁送り) だけ明示的にずらしている。

**6. `--pages` は増分の数字を動かしてはいけない。** stage4c README の 3 点を
再実行して 3/3 PASS (`stage7c-pitfall-recheck.txt`)。加えて `--out` の JSONL が
stage7b と byte 一致することも確認した。

**7. 正規表現でのブロック照合は「割合」で検算する。** `member-check.ts` の 6 種類
マーカーのページ単位突き合わせは今回も 6/6 一致。

## 結果の要約 (詳細は summary が SoT)

* byte 再現率 **97.099% → 99.506%** (+2.407 pt) = **到達上限**。
  byte 完全一致ページ **170 → 304**。残りは rev の 108,772 B (540 領域) だけで、
  doc-gen4 の参照ツリー自身が 2 revision を持つため到達不能。
* autolink 索引 **410,114 B → 0**、CommonMark **120,261 B → 0**。
  出せていないアンカー 297 → **0**、過剰アンカー 4 → **0**。
* 索引の 3 数字: **8.51 MB (gzip 1.33 MB) / 作る 0.4152 s / 読む 0.0764 s**。
* **§6.6 の「+0.36 秒」は外れた。** docstring 描画は 3 倍どころか
  **0.1809 → 0.1616 s (0.89 倍)**。生成合計は 0.8204 → **0.8852 s (+0.0648)** で、
  増分は全部 autolink 索引の読み込み (索引を渡さなければ 0.8063 s で 7b より速い)。
  **§6.3 の結論は動かない** (warm 合計 14.89 → 14.97 s)。
* 代償は時間よりメモリ: peak RSS **222 MB → 288 MB (+29.6%)**。
