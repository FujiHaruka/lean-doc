# B-0 — 自動生成宣言を `(line, col)` で親子に畳めるか

**状態**: 調査完了。2026-08-21。**実装はしていない**(この文書だけが成果物)。
**位置づけ**: `docs/plans/feature-sweep.md` §5 の B-0。B-3 (f、doc-gen4 #163 / #184) の
前提を決めるためのもの。§4 B-3 は「IR を変えずにできる可能性がある」「できるなら束 B から
外して束 C だけで済ませる」と書いている。**その可能性は無い、というのがこの調査の答え。**

## 1. 結論(先に)

**`(line, col)` では親子が決まらない。決まらないどころか、`(line, col)` が指しているのは
親ではなく「生成器」——`@[...]` の中の属性トークンそのもの**だった【実測】。

帰結は 3 つ。

1. **`(line, col)` を共有する群に親は入っていない。** Mathlib 標本 144 群のうち、
   親が群の要素として在るものは **0 群**【実測】。
2. **1 つの群が別々の親の子を抱える。** 144 群のうち **47 群**が 2 つ以上の名前空間に
   またがる【実測】。畳めば `MulOne.ext` が `AddZero` の下に入る。
3. **`Decl::index` は原典の順序ではない。** 隣接ペアの **47.6%** が原典順に対して逆順
   (1,320 / 2,776)【実測】。「index が最小のものが親」という規則は**定義として成り立たない**。

**したがって B-3 は束 B (IR schema 4 → 5) に乗せる必要がある**、というのが §4 B-3 の
2 択に対する答え。ただし **B-3 が解こうとしている症状は #163 の記述とは違う**(→ §6)。

**そして推奨は「extractor に由来を出させる」ではない。** 費用と、それが何を買うのかは §7。
撤退ラインは §8、反証条件は §9。

## 2. 計測条件

**数字の出所を残すために、再現手順をこの文書に埋めてある**(§10)。
`benchmarks/results/` にログは置いていない —— この調査は**方式の判定であって性能の数字では
ない**ので、壁時計を持たない。再導出はすべて §10 のコマンドで足りる。

| | |
|---|---|
| 機材 | MacBookAir10,1 / Apple M1 / RAM 16 GB |
| litedoc4 | `57e8e16` (main)、`target/release/litedoc4` + `extractor/build/extract` |
| Lean | v4.31.0 |
| 計測対象 | `/Users/haruka/dev/lean-projects` (`InformationTheory`)、**422 モジュール / 4,584 宣言** |
| Mathlib | `fabf563a7c95` (`v4.31.0`) |
| IR | schema 4、`generator = lean-doc/experiments/stage4b` |
| 暖機 | warm (full build 20.6 s、`extract` 19.7 s)。**壁時計は判定に使っていない** |
| 標本 | 3 つ —— 計測対象 / Mathlib 10 モジュール / Mathlib 非依存の e2e フィクスチャ改 |

**計測対象も `e2e/micro` も書き換えていない。** Mathlib 標本は
`lake env .../extract` を対象リポジトリの環境で読み取り専用に回したもの、
micro 標本は `e2e/micro` を scratch に**複製**して 1 モジュール足したもの。
どちらも作業領域は削除済み。

### 標本を 3 つ取った理由 —— 計測対象だけでは答えが出ない

**計測対象には自動生成宣言がほとんど無い**【実測】。原典を grep すると:

| 属性 | 計測対象での使用箇所 |
|---|---|
| `@[ext]` | **1**(`InformationTheory/Polymatroid/Basic.lean:47` の `attribute [ext] Polymatroid`) |
| `@[simps]` | **0** |
| `@[to_additive]` | **0** |
| `@[mk_iff]` | **0** |

`(line, col)` を共有する群は **2 群 / 4 宣言 = 4,584 の 0.09%**(§3)。
**2 例から規則の可否を結論しない**、というのがこの節がある理由。
そこで Mathlib 本体から `to_additive` / `simps` / `ext` / `mk_iff` の密度が高い
10 モジュールを抜いて第 2 標本にし(2,786 宣言)、さらに
**判定に効く形が Mathlib 無しでも作れるか**を第 3 標本で確かめた。

## 3. Q1 — `(line, col)` を共有する群の数と分布

### 計測対象 (422 モジュール / 4,584 宣言)【実測】

```
size  1 : 4,580 群 / 4,580 宣言
size  2 :     2 群 /     4 宣言
```

**2 群 / 4 宣言 = 4,584 の 0.087%。**

```
InformationTheory.Polymatroid.Basic          (47,11)  Combinatorics.Polymatroid.ext_iff / .ext
InformationTheory.Shannon.EntropyPower.Ext   (46, 0)  ...differentialEntropyExt (def) / ...differentialEntropyExt_def (theorem)
```

1 群目は `attribute [ext] Polymatroid`(47 行目、col 11–14 は括弧内の `ext` トークン)。
**親の `Combinatorics.Polymatroid` は (26,0)–(45,67) にあり、群に入っていない。**
2 群目は `irreducible_def`(46 行目)で、これは #163 が挙げた属性ではない
—— 1 つの構文が def と `_def` 定理を同時に建てる形で、**こちらは親が群に入っている**。

`Decl::index` の doc コメントは「このパッケージには (line, col) を共有する宣言が
**2 つ**ある」と書いているが、**実際は 2 組 (4 宣言) ある**。
(この文書は docs を直さない。B-3 が触るときに直す。)

**「群」を広げた版**: 「別の宣言の範囲の**内側**に始点がある」まで緩めると
**194 / 4,584 = 4.23%**。ただしそのうち **190 は structure のメンバー**(§5)で、
残りは上の 2 群と同じ 4 宣言。**緩めても新しい群は 1 つも出ない。**

### Mathlib 標本 (10 モジュール / 2,786 宣言)【実測】

```
size  1 : 2,345 群 / 2,345 宣言
size  2 :    86 群 /   172 宣言
size  3 :    15 群 /    45 宣言
size  4 :    33 群 /   132 宣言
size  6 :     2 群 /    12 宣言
size  8 :     4 群 /    32 宣言
size 12 :     4 群 /    48 宣言
```

**144 群 / 441 宣言 = 2,786 の 15.83%。** これが判定に使える母数。

## 4. Q2 — 群の中で親を推測なしに決められるか →**決められない**

**候補規則をすべて Mathlib 標本 144 群に当てた**【実測】。
「95% 当たる規則は 5% で別人を親に描く規則」なので、**全滅した順に**書く。

| 規則 | 結果 |
|---|---|
| **R1 `index` が最小のものが親** | **前提が偽**。`index` は olean の `constNames` 列挙順で、原典順に対して隣接ペアの **47.6% (1,320/2,776)** が逆順【実測】。同じ `@[ext]` でも `Semigroup.ext_iff`(86) < `Semigroup.ext`(155)、`AddSemigroup.ext`(58) < `AddSemigroup.ext_iff`(524) と**向きが群ごとに反転する** |
| **R2 名前が他全部の接頭辞になっているものが親** | **0 / 144 群**。**親が群に入っている群が 1 つも無い** |
| **R3 `kind` が `structure`/`class`/`def` で他が `theorem`** | **分離しない**。群の 132/144 は kind が一様 (`theorem` のみ 110 / `definition` のみ 22)、残り 12 が混在。そもそも R2 が示すとおり**親が群に居ない**ので、kind に判定材料が無い |
| **R4 同じモジュールで群の位置を範囲に含む宣言が親** | **72 / 144 群でしか一意にならない**。**63 群は包含する宣言が 2 つ以上**、**9 群は 0 個**【実測】 |
| **R5 名前空間を遡って最初に見つかる宣言が親**(位置を使わない) | 群の全員が同じ親に合意するのは **66 / 144**。**44 群は群内で親が 2 つ以上に割れ**、**32 群はどの要素も親を見つけられない**【実測】 |

**なぜ全部落ちるのか** —— 原典を宣言の範囲で切り出すと分かる。
2,786 宣言の範囲が覆う原典テキストの先頭トークンは【実測】:

```
to_additive  887    simps  187    simp   113    class  99    simps!  96
to_dual       96    ext     77    mk_iff  29    reassoc 10   gcongr  15
/--          359    theorem 303   alias   34    deprecated 49  protected 48
```

**`findDeclarationRanges?` が自動生成宣言に返すのは、それを生成した `@[...]` の中の
属性トークンの範囲**である。つまり `(line, col)` が同定しているのは**親ではなく生成器**。
1 回の属性適用が複数の宣言を建てれば、**それらは親が違っても同じ位置を持つ**。

### 例外は個別に —— 1 群が別々の親の子を抱える 47 群【実測】

**47 / 144 群が 2 つ以上の名前空間にまたがる。** 全件:

| 位置 | 生成器 | 群の中身 |
|---|---|---|
| `Algebra.Group.Defs:355,23` | `ext` | `AddZero.ext` / `AddZero.ext_iff` / `MulOne.ext` / `MulOne.ext_iff` |
| `Algebra.Group.Defs:399,23` | `ext` | `AddZeroClass.ext_iff` / `MulOneClass.ext_iff` |
| `Group.Hom.Defs:569,23` | `ext` | `OneHom.ext_iff` / `ZeroHom.ext_iff` |
| `Group.Hom.Defs:573,23` | `ext` | `AddHom.ext_iff` / `MulHom.ext_iff` |
| `Group.Hom.Defs:577,23` | `ext` | `AddMonoidHom.ext_iff` / `MonoidHom.ext_iff` |
| `Group.Hom.Defs:947,23` | `ext` | `AddMonoid.End.ext_iff` / `Monoid.End.ext_iff` |
| `Group.Hom.Defs:587,23` | `simps` | `AddMonoidHom.mk'_apply` / `MonoidHom.mk'_apply` |
| `Group.Hom.Defs:720,23` | `simps` | `OneHom.id_apply` / `ZeroHom.id_apply` |
| `Group.Hom.Defs:727,23` | `simps` | `AddHom.id_apply` / `MulHom.id_apply` |
| `Group.Hom.Defs:734,23` | `simps` | `AddMonoidHom.id_apply` / `MonoidHom.id_apply` |
| `Group.Hom.Defs:892,23` | `simps` | `OneHom.inverse_apply` / `ZeroHom.inverse_apply` |
| `Group.Hom.Defs:900,23` | `simps` | `AddHom.inverse_apply` / `MulHom.inverse_apply` |
| `Group.Hom.Defs:925,23` | `simps` | `AddMonoidHom.inverse_apply` / `MonoidHom.inverse_apply` |
| `Limits.Cones:154,19` | `reassoc` | `Cocone.w_assoc` / `Cone.w_assoc` |
| `Limits.Cones:159,11` | `elementwise` | `Cocone.w_apply` / `Cone.w_apply` |
| `Limits.Cones:247,11` | `reassoc` | `CoconeMorphism.w_assoc` / `ConeMorphism.w_assoc` |
| `Limits.Cones:264,19` | `ext` | `CoconeMorphism.ext_iff` / `ConeMorphism.ext_iff` |
| `Limits.Cones:273,19` | `reassoc` | `CoconeMorphism.hom_inv_id_assoc` / `ConeMorphism.hom_inv_id_assoc` |
| `Limits.Cones:277,19` | `reassoc` | `CoconeMorphism.inv_hom_id_assoc` / `ConeMorphism.inv_hom_id_assoc` |
| `Limits.Cones:287,19` | `reassoc` | `CoconeMorphism.map_w_assoc` / `ConeMorphism.map_w_assoc` |
| `Limits.Cones:189,19` | `simps` | `Cocone.extend_pt` / `Cocone.extend_ι` / `Cone.extend_pt` / `Cone.extend_π` |
| `Limits.Cones:196,19` | `simps` | `Cocone.whisker_pt` / `Cocone.whisker_ι` / `Cone.whisker_pt` / `Cone.whisker_π` |
| `Limits.Cones:255,19` | `simps` | `Cocone.category_comp_hom` / `Cocone.category_id_hom` / `Cone.category_comp_hom` / `Cone.category_id_hom` |
| `Limits.Cones:296,19` | `simps` | `Cocone.ext_inv_hom_hom` / `Cocone.ext_inv_inv_hom` / `Cone.ext_hom_hom` / `Cone.ext_inv_hom` |
| `Limits.Cones:308,19` | `simps!` | `Cocone.ext_hom_hom` / `Cocone.ext_inv_hom` / `Cone.ext_inv_hom_hom` / `Cone.ext_inv_inv_hom` |
| `Limits.Cones:318,19` | `simps!` | `Cocone.eta_hom_hom` / `Cocone.eta_inv_hom` / `Cone.eta_hom_hom` / `Cone.eta_inv_hom` |
| `Limits.Cones:334,19` | `simps` | `Cocone.extendHom_hom` / `Cone.extendHom_hom` |
| `Limits.Cones:339,19` | `simps!` | `Cocone.extendId_hom_hom` / `Cocone.extendId_inv_hom` / `Cone.extendId_hom_hom` / `Cone.extendId_inv_hom` |
| `Limits.Cones:344,19` | `simps!` | `Cocone.extendComp_*` 2 件 / `Cone.extendComp_*` 2 件 |
| `Limits.Cones:351,19` | `simps` | `Cocone.extendIso_*` 2 件 / `Cone.extendIso_*` 2 件 |
| `Limits.Cones:364,19` | `simps` | `Cocone.precompose_*` 3 件 / `Cone.postcompose_*` 3 件 |
| `Limits.Cones:375,19` | `simps!` | `Cocone.precomposeComp_*` 2 件 / `Cone.postcomposeComp_*` 2 件 |
| `Limits.Cones:383,19` | `simps!` | `Cocone.precomposeId_*` 2 件 / `Cone.postcomposeId_*` 2 件 |
| `Limits.Cones:391,19` | `simps` | `Cocone.precomposeEquivalence_*` 4 件 / `Cone.postcomposeEquivalence_*` 4 件 |
| `Limits.Cones:403,19` | `simps` | `Cocone.whiskering_*` 2 件 / `Cone.whiskering_*` 2 件 |
| `Limits.Cones:412,19` | `simps` | `Cocone.whiskeringEquivalence_*` 4 件 / `Cone.whiskeringEquivalence_*` 4 件 |
| `Limits.Cones:430,19` | `simps!` | `Cocone.equivalenceOfReindexing_*` 4 件 / `Cone.equivalenceOfReindexing_*` 4 件 |
| `Limits.Cones:442,19` | `simps` | `Cocone.forget_*` 2 件 / `Cone.forget_*` 2 件 |
| `Limits.Cones:450,19` | `simps` | `Cocone.functoriality_*` 3 件 / `Cone.functoriality_*` 3 件 |
| `Limits.Cones:482,19` | `simps` | `Cocone.functorialityEquivalence_*` 4 件 / `Cone.functorialityEquivalence_*` 4 件 |
| `Limits.Cones:678,19` | `simps` | `Cocone.op_pt` / `Cocone.op_π` / `Cone.op_pt` / `Cone.op_ι` |
| `Limits.Cones:684,19` | `simps` | `Cocone.unop_pt` / `Cocone.unop_π` / `Cone.unop_pt` / `Cone.unop_ι` |
| `Order.Atoms:122,0` | `alias` | `CovBy.is_atom` / `IsAtom.bot_covBy` |
| `Order.Atoms:216,0` | `alias` | `CovBy.isCoatom` / `IsCoatom.covBy_top` |
| `Order.Atoms:1277,0` | `protected alias` | `IsAtom.of_compl` / `IsCoatom.compl` |
| `Order.Atoms:1278,0` | `protected alias` | `IsAtom.compl` / `IsCoatom.of_compl` |
| `Filter.Basic:934,0` | `alias` | `Filter.Eventually.set_eq` / `Filter.EventuallyEq.mem_iff` |

原典を読むと形は 3 つに割れる:

```lean
-- (a) to_additive / to_dual と生成属性の合わせ技 — 1 トークンが 2 系統の親に配る
@[to_additive (attr := ext)]
class MulOne (M : Type*) extends One M, Mul M     -- AddZero.ext(_iff) と MulOne.ext(_iff) が (355,23)

-- (b) 1 つの attribute コマンドが複数の宣言に付く
attribute [to_additive (attr := simp)] dite_smul smul_dite ite_smul smul_ite
--         ^ (51,11) に vadd_ite / ite_vadd / vadd_dite / dite_vadd の 4 つ、親は 4 つとも別

-- (c) 1 行が 2 宣言を建てる — どちらも親ではなく、真の親は third party
alias ⟨CovBy.is_atom, IsAtom.bot_covBy⟩ := bot_covBy_iff
```

**(c) が Q3 の「本当は兄弟なのに畳んでしまう群」の最も明快な例。**
`CovBy.is_atom` と `IsAtom.bot_covBy` は互いに親ではなく、
真の由来 `bot_covBy_iff` は**名前も位置も無関係な第 3 の宣言**である。

## 5. Q3 — 子が持つ属性 / 兄弟を畳んでしまう群

### 属性は判定材料にならない【実測】

計測対象 4,584 宣言の `attrs` の出現(先頭語):

```
simp 69   reducible 67   inline 20   implicit_reducible 4   irreducible 2   deprecated 1
```

**`ext` も `simps` も `to_additive` も 1 件も無い。**
`Combinatorics.Polymatroid.ext` / `.ext_iff` の `attrs` は**両方とも空**であり、
**親の `Combinatorics.Polymatroid` の `attrs` も空**(`@[ext]` も、このパッケージ独自の
`@[entry_point]` も記録されていない)。extractor の `getAllAttributes` は doc-gen4 の
固定リストの転写なので(`extractor/Extract.lean:845` 付近、feature-sweep §B-2 が
同じことを書いている)、**`attrs` からは「生成された」も「生成した」も読めない。**

`modifiers` と `doc` も分離しない【実測、Mathlib 標本】:
群の要素の `modifiers` は `abbrev` 68 件のみ、群外は `abbrev` 41 / `noncomputable` 13。
docstring は群の要素で 31/441 が持つのに対し、群外は 632/2,345 —— **相関はあるが分離しない**。

### 兄弟を畳む群 → **在る。47 / 144**(§4 の表が全件)

上の (b)(c) は「生成された宣言だが親が違う」形、
**(c) はそもそも親子ですらない**(2 つの別名は互いに兄弟で、真の親は群外)。
**推測で畳めば 47 群で誤った親が描かれる。**

## 6. #163 の症状は litedoc4 では再現しない —— 別の症状が出ている

**doc-gen4 #163 の記述は「生成宣言が本体より前に出る」**(`FundamentalGroupoid.ext_iff` が
`FundamentalGroupoid` より先)。**これは両標本で 0 件**【実測】:

| 標本 | 親より前に並ぶ生成候補 | 親より後 | うち親の範囲の外 | 親が見つからない |
|---|---|---|---|---|
| Mathlib 10 モジュール | **0** | 224 | **188** | 139 |
| 計測対象 422 モジュール | **0** | 2 | **2** | 2 |

理由は、Lean v4.31.0 の `declRange` が**docstring と属性を含んだ範囲**を返すことによる。
`@[ext]` は宣言の範囲の**内側**に入るので、子は親の直後に並ぶ。

**実際に出ている欠陥は「親から離れて、無関係な宣言の隣に落ちる」**である。例:

```
Mathlib.Algebra.Group.Defs
  (350ish,0)  class AddZero          ← 親
  (353, 0)    class MulOne           ← 無関係
  (355,23)    AddZero.ext            ← MulOne のブロックの中に落ちている
  (355,23)    MulOne.ext_iff
  (355,23)    AddZero.ext_iff
  (355,23)    MulOne.ext
  (356, 0)    MulOne.toMul
```

**B-3 はこの症状を直すものとして書き直すべき。** 「前に出る」を直す実装を書くと、
**直すべき入力が 1 件も無いまま緑になる**(ゲートが嘘になる形)。

## 7. Q4 — 自動生成の projection (#184) は IR に在るか →**在る。失われていない**

**計測対象では 190 のメンバー名がすべて top-level の `Decl` として IR にある
(190 / 190)**【実測】。内訳は `ctor` 37 / `field` 156 / `parent` 1。
そしてこの **190 は `render` が報告する `declarations 4394/4584 (190 suppressed)` と同数**で、
同じ集合である —— `Suppressed::of_site`(`crates/litedoc4-render/src/page.rs`)が
**「どこかの宣言のメンバーである名前」をページの独立項目から外している**ため。

**外れているのは項目であって、宣言でもリンクでもない。**
生成されたページには各 projection の `id` 属性がある【実測】:

```
id="Combinatorics.Polymatroid.rank"  id="Combinatorics.Polymatroid.rank_empty"
id="Combinatorics.Polymatroid.rank_mono"  id="Combinatorics.Polymatroid.rank_submodular"
id="Combinatorics.Polymatroid.mk"
```

つまり **#184「自動生成の projection が出ない」は litedoc4 では起きていない** ——
structure の下にフィールドとして出て、アンカーで指せる。

- **`Foo.toBar` 形**: 計測対象に 16 件、うち 1 件 (`ErgodicProcess.toStationaryProcess`) が
  `parent` メンバー。Mathlib 標本では 206 件、うち**メンバーとして畳まれているのが 101 件
  (`parent` 96 / `field` 7)、独立に出るのが 105 件**【実測】。後者は
  `Group.toDivisionMonoid` のような **`instance` として宣言された親射影**で、
  独立に出るのが正しい。
- **依存側 (`deps/*.json`)**: Mathlib 396 / Init 130 / Lean 1 エントリ。うち `Foo.toBar` 形は
  Mathlib 10 / Init 2【実測】。**これは列挙ではなく「このパッケージが参照した名前 → 定義
  モジュール」の対応表**なので、projection が落ちているかを測る場所ではない。
  参照されたものは入っており、落ちていない。
- **projection の位置は親と一致しない**【実測】: 計測対象の 190 メンバーのうち
  **親と `(line, col)` が一致するものは 0 件**、190 件が親の範囲の内側、
  4 件が外側(`ErgodicProcess` に継承表示される `StationaryProcess` のフィールド)。
  **`(line, col)` は projection のグルーピングにも使えない。**
  使われているのは `members`(extractor が structure から直接列挙する)であって、位置ではない。

**#184 に対して B-3 が足すものは無い。** 触るとしたら「独立に出る 105 件の
instance 親射影をどう見せるか」だが、それは #184 とは別の話。

## 8. Q5 — `to_additive` の双子は同じ `(line, col)` か →**違う。同じ行の別の列**

【実測、Mathlib 標本】

```
(573, 0)-(575,20)  MulHom.ext          ← 手書き。範囲は宣言まるごと
(573, 2)-(573,27)  AddHom.ext          ← 双子。範囲は @[to_additive (attr := ext)] の to_additive トークン
(573,23)-(573,26)  MulHom.ext_iff      ← @[ext] 由来。範囲は同じ括弧の中の ext トークン
(573,23)-(573,26)  AddHom.ext_iff      ← 双子かつ @[ext] 由来
```

原典は:

```lean
@[to_additive (attr := ext)]
theorem MulHom.ext [Mul M] [Mul N] ⦃f g : M →ₙ* N⦄ (h : ∀ x, f x = g x) : f = g := ...
```

- **双子どうしは `(line, col)` を共有しない**(col 0 と col 2)。
  だから **`(line, col)` の群は `to_additive` の双子を捉えない。**
  捉えるのは `ext_iff` の側(col 23)で、**そこには双子が 2 つ混ざる**。
- 双子は原典と**同じ行**の**内側**に落ちるので、並びとしては原典の直後に来る。
  **並びの問題としては `to_additive` は壊れていない。**
- 一方 `attribute [to_additive (attr := simp)] dite_smul smul_dite ite_smul smul_ite` の形
  (`Algebra.Group.Basic:51,11`)は **4 つの双子が 1 つの位置に集まり、親は 4 つとも別**で、
  しかも**親は原典のずっと前の行にいる**。この形は `(line, col)` でも名前でも解けない
  (`vadd_ite` の親は `smul_ite` で、**名前空間も接頭辞も共有しない**)。

## 9. Mathlib 非依存で再現するか →**する**(`@[ext]` は Lean core)

`e2e/micro` は Lean core + path 依存 1 本だけで、Mathlib も Batteries も無い。
**`@[ext]` は Lean core にある**(`Init/Ext.lean` / `Lean/Elab/Tactic/Ext.lean`、
v4.31.0 で確認)。**`@[simps]` / `@[to_additive]` / `@[mk_iff]` は Mathlib** なので、
`e2e/micro` では書けない。

→ **feature-sweep §4 B-3 のテスト計画「`e2e/micro` に `@[ext]` を持つ structure と
`@[simps]` を持つ def を足し」は、後半が実行できない。** `@[ext]` だけで書き直すこと。

そして **`@[ext]` だけで判定に効く形は全部作れる**。scratch に複製した micro に
1 モジュール足して実測した結果【実測】:

```
( 3, 0)-( 9,11)  structure   Pair              ← @[ext] を inline で持つ親
( 4, 2)-( 4, 5)  theorem     Pair.ext_iff      ← 親は群に居ない
( 4, 2)-( 4, 5)  theorem     Pair.ext

(11, 0)-(16, 9)  structure   Trip              ← attribute [ext] Trip が 18 行目
(18,11)-(18,14)  theorem     Trip.ext          ← 親の範囲の完全に外
(18,11)-(18,14)  theorem     Trip.ext_iff

(20, 0)-(23, 9)  structure   Quad
(25, 0)-(27, 9)  structure   Quint
(29,11)-(29,14)  theorem     Quad.ext          ← attribute [ext] Quad Quint
(29,11)-(29,14)  theorem     Quint.ext_iff     ← 1 つの位置に 2 つの親の子が 4 つ
(29,11)-(29,14)  theorem     Quad.ext_iff
(29,11)-(29,14)  theorem     Quint.ext

(31, 0)-(34,11)  structure   PairPlus
(32, 0)-(34,11)  definition  PairPlus.toPair   ← 親射影。親と (line,col) が違う。members 経由で畳まれる
```

`index` も原典順ではない(`Quad` が 0、`Trip.ext` が 1、`Trip.a` が 2)【実測】。
**§4 の全 5 規則が、Mathlib 無しの 26 宣言で落とせる。**

## 10. 再現手順

```sh
# 標本 1: 計測対象
./target/release/litedoc4 build --root /Users/haruka/dev/lean-projects \
  --out <scratch>/b0 --lib InformationTheory \
  --extractor-bin ./extractor/build/extract --jobs 4

# 標本 2: Mathlib 10 モジュール (対象の環境を読み取り専用で借りる)
cat > <scratch>/modules.txt <<'EOF'
Mathlib.Algebra.Group.Defs
Mathlib.Algebra.Group.Basic
Mathlib.Algebra.Group.Hom.Defs
Mathlib.Logic.Equiv.Defs
Mathlib.Order.Atoms
Mathlib.Topology.Defs.Induced
Mathlib.CategoryTheory.Limits.Cones
Mathlib.Algebra.Ring.Ext
Mathlib.Order.Filter.Basic
Mathlib.Algebra.Order.GroupWithZero.Defs
EOF
cd /Users/haruka/dev/lean-projects && lake env <litedoc4>/extractor/build/extract \
  <scratch>/modules.txt <scratch>/events.jsonl \
  --equations --refs --write-ir --tagged-code --jobs 4 --ir-dir <scratch>/ml/ir
# → 2,786 宣言 / 2.038 s

# 標本 3: Mathlib 非依存 (e2e/micro を scratch に複製してから足す。リポジトリの
#         e2e/micro は触らない)
cp -R e2e/micro e2e/micro-dep <scratch>/micro/
# <scratch>/micro/micro/Micro/Gen.lean に §9 の 4 形を書き、Micro.lean に import を足す
cd <scratch>/micro/micro && lake build
lake env lean --root=<litedoc4>/extractor -o .lake/e2e-extract/Extract.olean \
  -c .lake/e2e-extract/Extract.c <litedoc4>/extractor/Extract.lean
lake env leanc -rdynamic -o .lake/e2e-extract/extract .lake/e2e-extract/Extract.c
<litedoc4>/target/release/litedoc4 build --root <scratch>/micro/micro --lib Micro \
  --out <scratch>/micro/out --extractor-bin <scratch>/micro/micro/.lake/e2e-extract/extract \
  --source-url https://example.invalid/blob/0000000000000000000000000000000000000000
```

集計は `<ir>/modules/*.json` を読んで `(line, col)` でグループ化するだけ。
**「その範囲が原典で何を覆っているか」を見るのがこの調査の中心**なので、
原典を `(line-1)[col:endCol]` で切り出す一手間を必ず入れること
(**行は 1 始まり、列は 0 始まり**【実測】)。

## 11. 推奨

**`(line, col)` では足りない。B-3 は extractor に由来を出させる必要がある。**
schema 4 → 5 に相乗りする(feature-sweep §3 束 B のとおり、schema を上げるのは 1 回だけ)。

### extractor が出すもの

**位置ではなく名前の対**。Lean 側が持っているのは
`Lean.Elab.Term.TermElabM` の宣言由来ではなく、**属性ごとの拡張が持つ写像**である:

| 由来 | Lean 側で引けるもの | 費用 |
|---|---|---|
| structure projection | `getStructureInfo?` / `getProjFnForField?` — **既に使っている**(`isProjFn`) | 0。`members` として既に出ている |
| `@[ext]` | `Lean.Elab.Tactic.Ext` の `extExtension`(core)。**直接ではない** → 下記 | 環境拡張 1 本の読み取り + 判定 |
| `@[to_additive]` | Mathlib の `ToAdditive` 拡張 | **extractor は Mathlib を import しない** → 引けない |
| `@[simps]` / `@[mk_iff]` | Mathlib 側 | 同上 |

**`@[ext]` の欄は「引ける」と読まないこと**【調査 2026-08-21、Lean v4.31.0 の
`Lean/Elab/Tactic/Ext.lean` を読んだもの。**実行して確かめてはいない**】。
`extExtension` が持つのは `ExtTheorem { declName, priority, keys }` で、

- **入っているのは `.ext` だけ**。`ext_iff` は拡張に登録されない
  (名前が `.str extName "ext_iff"` で導出される、同ファイル 140–143 行)。
- **親は 2 経路で復元できるが、どちらも 1 段の推論を挟む** ——
  `keys` の先頭が型の定数(`DiscrTree.mkPath ty`)なのでそこから取るか、
  `Foo.ext` から `.ext` を剥がして `isStructure` で確かめるか。
- **`@[ext]` は手書きの定理にも付く**(`isStructure` でない場合は生成が起きない)。
  つまり **拡張に居ることは「生成された」を意味しない。**

→ **B-3 は「拡張に居る」と「範囲が属性トークンを指す」の両方を要求すること。**
片方だけだと、手書きの `@[ext] theorem MulHom.ext` を生成物として畳む
(この形は Mathlib 標本に実在する、§8)。**「判断は 1 箇所に集める」規則どおり、
この 2 条件の合成を 1 つの関数に置く。**

**ここが費用の本体で、額ではなく形の問題**: extractor は `import Lean` だけで建つ
(それが `e2e/micro` で回せる理由であり、`tools/e2e-micro.sh` の前提)。
**Mathlib の環境拡張を読むには extractor が Mathlib に依存することになり、
Mathlib 非依存パッケージで extractor が建たなくなる。** これは
「Mathlib に依存する Lean パッケージのための基盤」という前提を裏返す変更で、
**採らない。**

したがって **extractor が Mathlib 無しで出せるのは、次の 2 つだけ**:

1. **`(line, col)` に加えて `selectionRange`**(`findDeclarationRanges?` はこちらも返す)。
   これで「範囲が属性トークンを指している」ことが**この側で判定できる**ようになる
   —— 今は原典を読まないと分からない。**これは「生成された」の判定材料であって、
   「誰から」ではない。**
2. **`@[ext]` の由来**(core の拡張なので引ける)。#163 が挙げた 5 つのうち 1 つ。

### だから推奨は「B-3 を縮める」

**B-3 が推測なしに畳めるのは次の 2 つだけ**:

- **structure の projection** —— **既に畳まれている**(§7)。**やることは無い。**
- **`@[ext]` 由来** —— core の拡張から親が引ける。計測対象での該当は
  **1 structure / 2 宣言**、Mathlib 標本で **17 群**。

**残り(`simps` / `to_additive` / `mk_iff` / `to_dual` / `reassoc` / `alias`)は畳まない。**
§4 B-3 の出口条件「親の判定が曖昧な群が対象で 1 つでも出たら、その群は畳まない」に
そのまま従う —— **曖昧な群は 47 / 144 出た。**

畳まない群に対して**できることはある**: §6 が示した実際の症状は
「親から離れて無関係な宣言の隣に落ちる」なので、
**「この宣言は `<位置>` の属性が生成した」と 1 行出す**だけでも読者の混乱は消える。
これは親を主張しないので、上の禁止に触れない。**ただしそれは描画 (束 C) の話で、
IR の追加は `selectionRange` 1 つで足りる。**

## 12. 撤退ライン

- **`selectionRange` を足しても「生成された」の判定が両標本で一致しなければ、
  B-3 ごと落とす。** 「たぶん生成」を描かない。
- **`@[ext]` の由来を core の拡張から引く実装が、Mathlib を import せずに書けなければ落とす。**
  extractor が Mathlib に依存する版は作らない(`e2e/micro` が回らなくなり、
  CI のゲートが 1 本消える)。
- **畳めた群だけ畳む。** 47 群のような曖昧な群が新しく 1 つでも出たら、その群は並べたまま。

## 13. 反証条件 —— B-3 はこれを検査する

**この文書の結論を壊す入力の形**を先に書いておく。B-3 は「たぶん大丈夫」と読まず、
**実際にこの形があるかをゲートで数える**こと。

1. **`(line, col)` を共有する群に親が入る形が出たら、§4 R2 の「0/144」が壊れる。**
   実際 §3 の 2 群目 (`irreducible_def`) が**その形**である —— def と `_def` 定理が
   同じ位置で、片方が親。**この 1 形だけは既に反例**。だから
   「群 = 子の集合」と決め打つ実装を書かない。**群のサイズと種別で分岐しない**
   (§4 R3 が示すとおり kind は分離しない)。
2. **Lean の版が `declRange` の始点を変えたら、§6 の「親より前に並ぶ = 0 件」が壊れる。**
   docstring / 属性を範囲に含めるかは Lean 側の実装で、**v4.31.0 で観測した性質にすぎない**。
   litedoc4 は 3 版 (v4.31.0 / v4.32.2 / v4.33.0) を建てる約束をしているので、
   **「親より前に並ぶ件数」を 3 版で数える**こと。0 でなくなったら §6 を書き直す。
3. **`attribute [ext] A B C` 形が対象に現れたら、名前空間規則 (§4 R5) の
   「66/144 は合意する」も崩れる方向に動く。** 計測対象には現在 0 件だが、
   利用者のパッケージには在りうる。**§9 の micro フィクスチャがこの形を持っているので、
   ゲートが常に見ている状態にできる。**
4. **projection が `members` に出ない structure が現れたら、§7 の「190/190」が壊れる。**
   Mathlib 標本では 284/294 しか一致しなかったが、**これは 10 モジュールしか抽出して
   いないことによる打ち切り**(`Mul.mul` など 10 件はサンプル外のモジュール定義)で、
   全 422 モジュールの計測対象では 190/190。**「メンバー名は必ず Decl である」を
   不変量としてゲートに置ける** —— 落ちたら extractor か Suppressed のどちらかが壊れている。
5. **1 パッケージしか測っていない。** 計測対象は `@[ext]` 1 件・`simps` 0 件で、
   **判定はほぼ全部 Mathlib 10 モジュールに乗っている**。Mathlib は
   `to_additive` / `simps` の使い方が濃い方に偏っているので、**割合 (15.83%) は
   一般のパッケージの値ではない**。**規則が落ちるという結論**は 47 件の反例で足りるが、
   **母数の大きさは他のパッケージで測り直すまで一般化しない**。

## 14. B-3 への申し送り(この調査が触らなかったもの)

- **`crates/litedoc4-ir/src/model.rs` の `Decl::index` の doc**:
  「(line, col) を共有する宣言が 2 つある」→ 実際は **2 組 4 宣言**(§3)。
- **`crates/litedoc4-render/src/page.rs` の module doc**:
  「190 of the target package's **4,750** declarations」→ 現在の母数は **4,584**、
  モジュールは 422(`model.rs` の `ModuleFile::tactics` の doc も **432** のまま)。
  **190 という数字自体は今も合っている**。
- どちらも**この調査では直していない**(B-3 が触るファイルなので、同じコミットで直す方がよい)。
