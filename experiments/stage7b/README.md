# stage 7b — IR schema 4: 抽出側の未実装 3 つを埋める

段階 7 の続き。**目的は 1 つ**: `docs/approach.md` §6.1 が 13.71 秒に付けている留保
——「属性収集・instance 索引・構造体メンバの binder が未実装」——を外せる状態にする。
3 つとも「doc-gen4 がやっていてこちらがやっていない仕事」なので、埋めれば時間は増える。
**増える幅を測るのが半分の目的**で、留保を外すのが残り半分。

数字とその条件は `benchmarks/results/stage7b-summary.txt` (SoT)。
ここには**何をなぜ変えたか**だけを書く。

## なぜ新しいディレクトリなのか

stage7a を書き換えず、その写しをここに置いた。CLAUDE.md の「新しい段階は既存を壊さず
新ディレクトリを足す (数字の再現性のため)」に従う。stage7a の 96.506% /
IR 15.24 MiB / `writeIR` 0.5927 s / 抽出 13.7084 s はコミット済みで docs から
引用されており、抽出器を直接書き換えるとその列が再現できなくなる。

`experiments/stage4c/coverage.ts` は**受け入れオラクル**なので、この段階でも
**1 行も変えていない**。採点は stage7a・stage4c と同じプログラムで行っている。

## 中身

| ファイル | stage7a との差 |
|---|---|
| `Extract.lean` | 属性 / instance 索引 / メンバ追加情報の 3 つ。`irSchemaVersion` を 4 に。ablation フラグ 3 つと phase timer 3 つ |
| `render.ts` | `div.attributes` の出力、`fieldToHtml` のもう半分 (binder / docstring / `inherited_field`)、schema ゲートを 4 に |
| `check-spans.ts` | メンバ binder を fragment として歩く。schema 4 のキーの不変条件 |
| `member-check.ts` | **新規**。メンバ表マークアップの割合検算と、残った members 領域の帰属 |
| `instance-check.ts` | **新規**。ページに出ない instance 索引を doc-gen4 の `declaration-data.bmp` と突き合わせる |
| `build.sh` / `run.sh` | stage7a の写し。出力名だけ差し替え |

## (1) 属性 — `div.attributes`

doc-gen4 は `Info.ofTypedName` (`Process/NameInfo.lean:125`) で全宣言に対して
`getAllAttributes` を呼び、`Output/Module.lean:88-94` で `@[a, b]` の 1 行にする。
`Process/Attributes.lean` をそのまま移した (`import Lean` だけ、という制約は維持)。

* 合成順 `customs ++ tags ++ enums ++ parametric` は**印字される文字列そのもの**なので
  仕様の一部。doc-gen4 の `ValueAttr` 型クラスによる間接化は再現していない —
  リストが 1 個と 5 個の `def` なので展開して書いた。
* instance だけは `InstanceInfo.ofDefinitionInfo` が `instance <priority>` と
  `defaultInstance <prio>` を**後ろに足す**ので、(2) と順序で結合している。
* `Compiler.specializeAttr` は Lean v4.31.0 では `ParametricAttribute (Array Nat)`。
  doc-gen4 が持つ `ToString SpecializeAttributeKind` はその属性の型ではなく、死んでいる。
  こちらは core の `ToString (Array α)` に合わせた。この対象では出現 0 なので**未検証**。

## (2) instance 索引 — 再現率は 1 バイトも動かない

**これはページのバイトに出ない。** instance 一覧は `declaration-data.bmp` から
ブラウザが埋める (`approach.md` §5.5 L3-3)。それでも実装したのは §6.1 の
「同じ仕事ではない」を解消するため — doc-gen4 が書いていてこちらが書いていない列が
ある限り、13.71 秒と 31.50 秒は同じ仕事の比較にならない。

**再現率が上がらないのが正解**なので、上がらなかったことを結果として書くだけでは
「何も作っていない」と区別が付かない。そこで `instance-check.ts` を足した:
doc-gen4 は schema 4 が持つ 2 列から索引を組み立てる (`Output/ToJson.lean:88-97`) ので、
その出力ファイルと**両方向**で突き合わせられる。順方向 (IR にある対が bmp にある) は
型名の取りこぼしを見つけられないので、逆方向 (bmp にある対が IR にある) が要る。

## (3) 構造体メンバ — binder / docstring / 由来

`structureMembers` は `ppSignature` を呼びながら `sig.binders` / `sig.binderSpans` を
**捨てていた**。捨てるのをやめたのが主。つまり (3) の抽出コストは「整形しなおす」ではなく
「捨てるのをやめる」——増えるのは時間ではなく IR バイト。

足したのは 2 つだけ:

* `getFieldOrigin` (`Process/StructureInfo.lean:39-47`)。継承フィールドでは
  **親の projection function** が答えで、`fieldToHtml` はそれをリンク先にし、
  docstring を出さず、`<li>` を `inherited_field` にする。
* field の docstring。doc-gen4 は DB から読み戻すとき、**projection function 自身の
  `name_info` 行**から取る (`DB/Read.lean:355-380`) ので、`findDocString? projFn` と
  同じもの。

**やっていないこと**: doc-gen4 は `getFieldTypes` の中で projection function の
`getAllAttributes` をもう一度計算し、出力経路ではその結果を使わない (使うのは
`name_info` 行の側)。projection function はこちらも 1 宣言として抽出しているので、
その二度目は再現していない。doc-gen4 の冗長さは仕事の量ではない。

### `containedNames` — `<li>` に id が付くかどうか

継承フィールドの `<li>` に id が付くのは、Lean が実際にその projection を生成していて、
かつその宣言範囲が構造体の範囲の中にあるときだけ (`DB/Read.lean:177-185` の SQL)。
IR は全宣言の `(line, col)` と `(endLine, endCol)` を——描画されないものも含めて——
持っているので、これは IR から計算できる。この対象では 4 件とも id 無しが正解で、
生成側も 4 件とも id 無しになった。

## 落とし穴 (実測で確認済み、踏むと静かに壊れる)

**1. 正規表現でのブロック照合は「割合」で検算する。** stage4c では
`li.structure_field` の正規表現が属性順に依存して 153 本中 4 本しか拾えていなかった。
`fieldToHtml` の direct / inherited は**まさにその分岐**なので、`member-check.ts` は
6 種類のマーカーを doc-gen4 側と**ページごとに**突き合わせる。合計だけ合っていて
配り方が違う、という壊れ方を落とさないため。

**2. 母数を静かに縮めない。** `div.attributes` は
`Html.element "div" false … #[text s]` で、`Html.toStringAux` が
`<div …>escape s</div>\n` と**末尾改行付き**で出す。この改行は 42 ページで
二重計上されていたことがあり、`coverage.ts` のタイル検査が捕まえた。今回まさにそこを
再び触ったので、タイル検査が通っていること (領域合計 = 22,028,728 B) を毎回確認している。

**3. 単位が 2 つ同時に生きている。** スパンのオフセットは UTF-16 code unit、
`RenderedCode.textLength` は code point。メンバの binder も span を持つので同じ規則。

**4. ablation の IR は描画してはいけない。** `--no-attrs` / `--no-inst-index` /
`--no-member-extra` は「その分だけ欠けた IR」を書く。`index.json` に `ablations` 印が
付き、`render.ts` はそれを見たら**拒否して終了する** — それらしく見えて間違っている
ページを黙って出すのが一番まずい。

**5. `--tagged-code` を外したときの互換は検査で持つ。** 3 つの追加はすべて
`Cfg.wantAttrs` / `wantInstIndex` / `wantMemberExtra` の内側にあり、フラグを外すと
stage7a と byte 一致した IR を書く (`stage7b-untagged-identity.txt`)。

## 結果の要約 (詳細は summary が SoT)

* byte 再現率 **96.506% → 97.099%** (+0.593 pt)。byte 完全一致ページ 148 → 170。
* 目標だった 97.136% には **8,127 B (0.037 pt)** 届かない。その 8,127 B は
  members 領域に残った 2 件で、中身は field docstring の **autolink** —
  アンカーを剥がすと両側とも byte 一致する。**メンバのデータ欠落は 0 B**。
  目標値のほうが、members 領域に入り込んだ autolink 分を二重に数えていた。
* 3 つの抽出コストは phase timer で **属性 0.0199 s / instance 索引 0.0020 s /
  メンバ 0.0014 s** = 合計 0.023 s (抽出 14 s の 0.17%)。IR は +36,901 B (+0.23%)。
* 抽出 `total` の差 (+0.035 s) は run 間ばらつき (±0.15 s) の中にあり、**分離できていない**。
  分離できているのは phase timer とバイトだけ。
