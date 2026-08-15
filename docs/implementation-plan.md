# lean-doc 実装計画 — v0.1

**位置づけ**: [`approach.md`](approach.md) が**アプローチの SoT**、[`verification-log.md`](verification-log.md) が
**数字の SoT**。この文書は**実装のレベル**を書く — 何をどの順で作り、各段の合否を何で判定するか。
数字には CLAUDE.md「計測の誠実性」の 4 ラベル (実測 / 外挿 / 仮定 / 理論値) を付ける。
**最終更新**: 2026-08-15。

---

## 1. ゴールと完了条件

**遠いゴール = v0.1 — Mathlib 依存パッケージが使えるドキュメント CLI。**
ゲートは 2 段あり、**A は機械的に判定でき、B は判定に人が要る**。

| ゲート | 完了条件 | 判定 |
|---|---|---|
| **A: 移設** | Rust 版が対象リポジトリで **439/439 byte 一致** | `coverage.ts` (Deno、**製品外に残す**)。機械的 |
| **B: v0.1** | 対象リポジトリ**以外**の Mathlib 依存パッケージで `lean-doc build` 一発が通り、増分が効き、CI に置ける | 第 2 の対象で実走 |

**ゲート B の第 2 の対象は「合成」に限る**【決定 2026-08-15、ユーザー判断】 — 同じ Mathlib rev に固定して
こちらで作ったパッケージを使い、**実在の公開パッケージは v0.1 では使わない** (rev が一致するものが無ければ
`lake exe cache get` で数 GB のダウンロードが要るため)。**帰結として、B は「合成の第 2 の対象で通った」
までしか主張できない。** 合成の対象は境界値を**狙って**入れられる代わりに、**こちらが知らない形では
驚かせてくれない** — 「実在パッケージでの実走」は**未検証項目として残す**のであって、済んだことにしない。

**移設中の内側ループは `coverage.ts` ではなく「TS 版の出力との byte 差分」**を使う。プロトタイプは doc-gen4
に対して 99.506% を出しているので**それに byte 一致すればゲート A は構成上通る**し、差分は**パーセンテージ
ではなく壊れたファイル名と食い違いバイト位置**を返す。ハーネスは `tools/render-{reference,compare}.sh`、
**参照は 432 ページ / 29 MB、生成 1.02 秒**【実測、warm】。**ゲート A は実走で通過済み (M1-d3、
2026-08-11)** — **再現率 99.5% (21,919,956 / 22,028,728 B)、304/348 ページ byte 一致、不足 108,772 B は
全部 rev**【実測。既知の 99.506% を再現】。**doc-gen4 の参照木 (736 MB / 6,080 ページ) は対象リポジトリの
`.lake/build/doc` に健在**で、作り直すと約 9 時間かかるので**消さないこと**。
**A は B の必要条件であって十分条件ではない** (approach.md §9) — **byte 再現率はオラクルであって製品目標
ではない**。A を通した時点で価値のあるものは何も増えていない、という認識を持って B まで走る。

---

## 2. Context — 何が既にあり、何が無いか

検証段階 1〜8 の**動くパイプライン一式が `experiments/` に散らばっている**。「検証が終わった」の実体はこれ。
製品ツリー `crates/` は M0 で作り、M1 / M2 / M3-a〜M3-d2b 分が入っている (→ §7)。`experiments/` は数字の
再現性のため凍結してあるので、製品ツリーは既存を壊さず**別に作った**。**抽出器 (Lean)** は製品でもこのまま
使う (移設ではなく**移動**、M4)、**外側 (TS + シェル)** は Rust に移設 (残りは M3-d3 / M3-d4 と常駐まわり)、
**受け入れオラクル (Deno)** は**製品外に残す** — Rust 版を採点する側なので同じ言語で書き直すと
「両方同じ間違いをする」経路ができる。

---

## 3. Approach

> **一気に移さない。パイプラインの 1 段だけを Rust に差し替え、残りは TS のまま、
> 毎回オラクルを緑にしてから次の段へ進む。**

段の間の受け渡しが**すべてファイル** (IR / 台帳 / HTML / 全域成果物) なので、1 段だけ言語を替えても他段は
気づかない。これが成立するのは検証段階が**ファイル境界で切れている**から。**なぜこの形か。** 3,900 行を
一気に移すと 439/439 が出なくなったとき**どの段が原因か分からない** — byte 一致は 1 ビットしか返さないので、
犯人の切り分けを**移設の順序に埋め込む**しかない (増分 4〜7 が実際に成功したやり方と同じ)。

**構造上の設計判断を 1 つだけ先に入れる**: **IR へのアクセスを 1 つの抽象に集約する。** 残る IR 全読みは
5 回 (ownership・merge ×2・impact・render) で、**1 回 3.18 秒 = 5 回で約 15.9 秒**【実測 → approach.md
§5.6】。7h が 1 回ぶんを `contentHash` キャッシュで消した実績があるので、**キャッシュ層を後から 5 箇所に
足すのではなく、最初から IR ローダ 1 枚の裏に置く**。**v0.1 のゲートには入れない** — 性能であって正しさでは
ないので A の後に有効化する。入れるのは**構造**。同じ理由で**キャッシュのバージョン鍵 + それを壊すテスト**も
1 つの共通トレイトに切り出す — 7h の `STATE_VERSION` / `STATE_DERIVATION`【実測 →
`stage7h/global.ts:145-146`】と `oracle.sh` の 7 番目の状態 (**stale-state**) がその原型。5 箇所で規律を
再発明すると、腐るのは必ず後から足したほう。**やらないこと**: 移設のついでの設計改良。**byte が変わる変更は
移設と分離する** — 混ぜるとオラクルが犯人を名指しできなくなる。

---

## 4. この計画で決めること

- **決定 1 — 完了条件は 2 段** (→ §1)。`coverage.ts` は Deno のまま製品外。**採点は必ず rev 置換後の
  木に対して回す** (プレースホルダのままだと **−3.1103 pt** ずれる【実測】)。**落とし穴**: `coverage.ts` の
  revless 正規化は `/blob/[0-9a-f]{40}/` のハードコードで、`--source-url` にタグ名やブランチ名を渡す
  実装を作ると**採点が静かに下がる**。40 hex を渡すこと
- **決定 2 — `contentHash` キャッシュは「構造だけ」最初から入れる** (→ §3)
- **決定 3 — rev 注入は「ビルド時の文字列置換」を採る**【決定】。置換は **0.1552 秒 / 432 ファイル・
  制約ゼロ**【実測 7e-rev】、JS 注入は **1 ページ最悪 0.80 ms**【実測、段階 8】だが制約が 3 つ
  (順序 / 鮮度ヘッダ / `?jump=src`) 付き、**実測範囲は Chrome 151 のみ**で他エンジンは仮定。
  **この決定が守っているもの**: rev をバイトに入れない目的は「コミットごとに全ページが無効に
  ならないこと」であって「最終ファイルに rev が無いこと」ではない — **キャッシュされる中間物
  (IR・レンダラ出力) に rev が無ければ目的は満たされる**ので**再生成集合は 0 のまま**。
  JS 注入版の数字は選択肢の保存として検証ログに残す (静的ホスティングを使えない配信形態用)
- **決定 4 — 依存写像はフル生成の既定経路に入れる**【穴の修正】。`stage7h/run.sh` の `render()` は
  **`--link-index` を渡していない**【実測】ので、そのまま移すと docstring の autolink が落ち、
  同じ IR・同じ `--source-url` で有無だけを振ると **432 ページ中 150 ページ (34.7%) のバイトが変わる**
  【実測 2026-08-11。`tools/render-compare.sh`】。→ **製品では既定で渡す。** 写像の作り方は M5 で
  変わるが**インタフェースは変わらない** — レンダラが読むのは `.lidx` なので**差し替えは供給側だけで閉じる**
- **決定 5 — approach.md §8 の未解決の振り分け**: 依存写像の配布は**先に決める (M5 の前提)**、
  宣言の所有モジュール (同名 25 件) は**明文化のみ** (現行が 439/439 を出している以上、規則は実装済み)、
  `_private.` 名 (実測 8 名) は**現行の扱いを踏襲**、Lean のバージョン差は**v0.1 の範囲外** (抽出器が
  ビルドできた 1 バージョンで動けばよい)。§8 の残り 3 つ (cold な環境ロード / 意味解析 / docstring 内
  リンク 0.36%) は性能と精度の問題でゲートではない — cold は「doc 生成を `lake build` と同じジョブに
  置く」で第一の打ち手が済む (M6 の CI テンプレ)

**依存写像 — 公開サイト経路は使えない【実測 2026-08-11】。**
`declaration-data.bmp` は公開サイトに**ある** (200 / `image/bmp` / **66,418,003 B**)【実測、HEAD】が、
**バージョン対応が取れない**【実測】 — 公開サイトは **Lean 4.33.0 / mathlib `c3a9a08f`**、対象は
**Lean v4.31.0 / mathlib `fabf563a7c`** で**2 マイナー版ずれている**。公開サイトはバージョン別
アーカイブを持たない**【仮定・未確認 → V4】**ため、下流が固定した古い Mathlib 用の写像は取れない。
**したがって写像は生成側で作る。** approach.md §5.3 が「必要な列は (名前, モジュール) の 2 列」
「`docLink` は (モジュール, 名前) から 100% 復元できる」を**実測で**確定させているので、**抽出器が
既にロードしている環境を走査すれば作れる**はず — 段階 1 で 490,171 定数の走査 = **0.62 秒**
【実測、warm】。URL はベース URL (パッケージごとの設定) と §5.3 の相対規則から組む。
**これは仮説であって未検証 (→ V1)** — 「環境走査で作った写像が `declaration-data.bmp` 由来の写像
(258,760 宣言、段階 7c) と同じ結果を出すか」は M5 で実測する。**否定されたら M5 の設計をやり直す。**

---

## 5. マイルストーン — リレーのゲート

各 M は**独立にコミットでき、オラクルで判定でき、次の M の前提になる**。**M の途中でセッションが
尽きてもよい** — ゲートが数字なので、次のセッションはオラクルを 1 回回せば「いま何段目か」を知れる。

| M | やること | ゲート (合否の判定) |
|---|---|---|
| **M0** | 実装計画 + 製品ツリーの骨格 (Cargo ワークスペース / Lean パッケージ) + `experiments/` の凍結宣言 | ビルドが通る。オラクルは動かない |
| **M1** | **IR リーダ + レンダラ**を Rust へ (md4c を FFI で本物に置換) | **432 モジュールページが byte 一致** (全域 6 本は M2、移動分は M3 なので 439 の残り 7 はここでは出ない)。**通過済み** → 下の判定手順と §7 |
| **M2** | **全域成果物 6 本**を Rust へ (`contentHash` キャッシュ込み) | 6 成果物が byte 一致、サイトも byte 一致。**通過済み** → §7。**サイトの母数はここでは 438** (432 ページ + 6) — **439 の 439 番目は移動後にしか存在しない**ので M3 |
| **M3** | **増分 4 本 + パイプライン**を Rust へ | 本物の移動と本物の削除を `lake build` ごと回して**フルビルドと byte 一致**。**M3 は完遂** (a / b / c / d1 / d2 / d2b / d3 / d4) → §7。**本物の移動で 439/439、本物の削除で 437/437 バイト一致** |
| **M4** | 抽出器を製品ツリーへ + **常駐の自動起動・停止**を Rust から配線 + CLI の形を確定 | **1 コマンド**で対象リポジトリのサイトが出る。**M4 は完遂** (a / b / c / d) → §7。**`lean-doc build` 1 コマンドでクローンのサイトが出て 438/438、本物の移動をまたいで増分 == フル生成が 439/439** |
| **M5** | **第 2 の対象**で動かす (依存写像を生成側で作る経路の実測を含む) | 別パッケージで `lean-doc build` 一発が通る |
| **M6** | CI テンプレ + README + インストール手順 | CI で通る。`lake build` と**同じジョブ**に置く形 |

**M1 は 4 分割で完了** (**M1-a** IR リーダ / **M1-b** 下回り / **M1-c** docstring / **M1-d** ページ描画と
主ループ。移設元はすべて `stage7d/render.ts`)。**参照は決定的**【実測 2026-08-11】 — `render.ts` を
2 回走らせて 432/432 byte 一致、マニフェストも同一で、**バイト差分をオラクルにしてよい**根拠になっている。

#### 内側ループのオラクルは M1-c 以降 1 段ゆるむ (M2 以降もこの手順で判定する)

**自前 CommonMark サブセット (TS) → 本物の md4c (Rust) は、原理的にバイト一致を保証しない。**
内側ループ (§1) は「TS 版の出力との byte 差分」なので**不一致が出たときに Rust 側が正しい**場合がある。判定手順:

1. TS 差分が `IDENTICAL` → そのまま次へ
2. 差分が出たページは **doc-gen4 の参照木** (`lean-projects/.lake/build/doc`) と突き合わせる。doc-gen4 は
   本物の md4c を使うので、**Rust が doc-gen4 に一致して TS が外れているなら TS のサブセットの限界**
3. その場合は既知の乖離としてこの節に記録し、最終判定は `coverage.ts` で行う。
   **TS に合わせて md4c を歪めない** — それは移設ではなく劣化の移植

**M1-d3 で実際に 1 回発火し、手順 3 で終わった**【実測 2026-08-11】 — `render-compare.sh` は
**431/432 で `IDENTICAL` を出さない**が、差分 1 ページは登録済みの乖離だけで、`coverage.ts` の出力は
TS 版とパス・日付以外バイト一致。→ **M1 のゲートは「`IDENTICAL`」ではなく「この手順で決着した
こと」で通っている。** `render-compare.sh` に例外リストは入れていない【判断】 — 例外を持つ比較器は
2 件目の乖離を黙って飲む。差分集合のピン留めは Rust 側のテスト (`tests/pages.rs`、集合ごと assert)。
**登録済みの乖離は 3 件**【1・2 は実測 2026-08-11、3 は実測 2026-08-12。**すべて Rust が doc-gen4 側**】:

1. **CommonMark サブセット** — 同じ 4,987 入力 (実 docstring 4,858 + 手書き 129) を TS の
   `renderDocString` と doc-gen4 の `docStringToHtml` に通すと **TS が外れるのは 41 件、うち実 docstring は
   1 件だけ** (`InformationTheory.Shannon.TimeBandLimiting.Count` の module doc = 入れ子バッククォートの
   code span)。残り 40 件は TS が「この対象には出現しないので実装しない」と宣言した機能
   (`render.ts:1091-1096`)。オラクル `crates/lean-doc-md/tests/ts_docstring.rs`
2. **autolink の `inLink`** — doc-gen4 の `renderText` は `<a>` の中の code span を素通しする
   (`DocString.lean:264`) が `render.ts:1622` は無条件に autolink する。リンクを引かない状態では
   両者は同一バイトなので 1 の比較では見えない。**実 docstring 4,858 件中 0 件**なのでゲート A の
   バイトは動かない。オラクル `crates/lean-doc-render/tests/docgen4_linked.rs`
3. **heading id の分割表** — `render.ts:1058` は **V8** の `\p{P}\p{Z}\p{C}`、Rust の `heading_id` は
   doc-gen4 (`DocString.lean:155-165`) と同じ **UnicodeBasic** で、**4,802 コードポイントで食い違い、
   向きは一方向 (UnicodeBasic ⊂ V8)**。トークン (→ V6) と違い heading id は `id=` / `href=#` に出るので
   **ページのバイトを作る**が、対象の docstring 1,634,037 コードポイントに**出現 0 件**【実測】。
   第 2 の対象では出うる (→ V6 と同じ再測定で拾える)

#### 移設元から掘り出した落とし穴 — §7 の表に無いもの

1. **`linkIndexBytes` は UTF-16 code unit 数であってバイト数ではない** (8,494,819 ≠ 実ファイル
   8,508,273 B)【実測】。`metadata().len()` から再現しようとすると静かにずれる
2. **`knownModules` は 3 つの供給源の和集合** (`render.ts:2051-2052, 2079`: IR のモジュール名 ∪
   `known` の値 ∪ `.lidx` の `@` 節)。**`LinkIndex` 単体で autolink を解決すると取りこぼす** →
   M1-c で `NameIndexBuilder::build` が `.lidx` を**引数で要求する**形にして構造的に塞いだ (取りこぼしは
   「リンクが 1 本消える」形で出るのでテストが弱いと通る)
3. **`.lidx` パーサにはエラー経路が無い** — 不明な行は無条件にグループ見出し、`#lidx1` すら検証しない。
   **そのまま写してある**。厳しくするのは「移設のついでの設計改良」なので分離する

### 各 M で持ち込んではいけないもの (既知の穴)

- **M1**: `render.ts` は `--only` が無いと**全モジュールを描く**。rev を外した今、**空の再生成集合は
  常時通る経路**なので、Rust 版で「`--only` を 0 個渡す」が「全部描く」に化ける穴を**再現しない**
  (→ approach.md §5.5)。**空集合と未指定を型で区別する**
- **M1-d**: `suppressed` (= 他の宣言の `members` に出る名前) は**全モジュール横断の集合**
  (`render.ts:2043-2048`)。モジュール単位で作ると余分なページ項目が出る。
  **`Member.isDirect` の既定値は TS と Rust で逆**だった (TS は `=== false` 判定なので
  キーが無ければ direct、`#[serde(default)] bool` は `false` = 継承)。実 IR の field 156 件
  すべてにキーがあり**バイトでは検出できない**【実測】 → M1-d2 で `Option<bool>` にして塞いだ
- **M2**: `global.ts` の **module doc 由来の autolink トークンは死んでいる**【実測】 — `md.doc` を
  読んでいるが IR の `moduleDocs` 要素のキーは `line` / `col` / `text` で `doc` は無い
  (書き手 `Extract.lean:2004-2006`)。**そのまま写す** — 直すと全域成果物のバイトが動くので、
  移設と分離して扱う (→ §3「移設のついでの設計改良をしない」)
- **M3**: 鍵の比較は**和集合**で行う — 片側にしか無い鍵も変化と数え、**黙って過少にレンダリングしない**
- **M3-d**: モジュール一覧は**ソースの glob** で作る。`.lake/build` を走査すると孤児 olean 659 個を
  拾う【実測】。**`detect` の担当ではない**【実測 2026-08-12】 — 台帳は `--modules` を受け取るだけ。
  **`lean-doc modules --root --lib` に切り出し済み (M3-d2)**。`--lib` の出所 (lakefile) は M4
- **M5**: olean の内容ハッシュには**ソースの絶対パスが 429/432 モジュールで埋まっている**【実測】ので、
  CI と開発機で IR キャッシュを共有するなら鍵をパス非依存にする

---

## 6. ファイル別内訳

**移設対象は約 4,250 行 + シェル約 1,100 行**【調査で実測】。

### 移設する (TS/シェル → Rust)

**移設完了** (行数は移設元): `stage7d/render.ts` (2,227) + `build-link-index.ts` (213) → M1 (後者は M5 で
供給側を差し替え)、`stage7h/global.ts` (492) → M2、`stage5/ledger.ts` (422) → M3-a、`ownership.ts` (219) +
`merge-ir.ts` (307) → M3-b、`impact.ts` (231) + `prune-pages.ts` (138) → M3-c、`stage7h/run.sh` の
`render()` 部 (3 行) → M3-d1、`incremental.sh` (441) + `run.sh` の `modlist()` (5 行) → M3-d2、
`stage7g/extract-once.sh` (87) → **M4-b** (`lean-doc extract`)。

`stage7g/serve-ctl.sh` (185) → **M4-c** (`crates/lean-doc/src/resident.rs`)。
**これで `experiments/` からの未移設はゼロ。**

### 移動する (Lean のまま) — **M4-a で完了**

`stage7d/{Extract.lean,build.sh}` → **`extractor/`** (リポジトリ直下)。**`leanc -rdynamic` は load-bearing**
(`importModules (loadExts := true)` が Lean インタプリタでモジュール初期化子を走らせ、実行中の実行ファイルから
シンボルを解決する)。**lean-doc 側に toolchain も lakefile も Mathlib も置かない** — 環境は `lake env` で借りる。
`extractor/build/` は `.gitignore` 済み (バイナリ 171 MB + C 2.7 MB は再生成可能)。
**残るハードコード**は `build-link-index.ts:42-45` と `incremental.sh:106` / `run.sh:55`
(既定 `SOURCE_URL` に 40 桁 rev 直書き) — どれも `experiments/` 側なので凍結のまま。

### 製品外に残す / 破棄する

- **残す**: `stage4c/coverage.ts` (740、オラクル。**Deno のまま**)、`stage7h/oracle.sh` (157 → Rust の
  統合テストへ。7 状態 base/rerun/modified/removed/added/restored/**stale-state**)、診断チェッカ群
  (`check-spans` / `md-diff` / `autolink-check` / `member-check` / `instance-check` / `rev-check`)
- **破棄**: `stage7e/Parse.lean` (783) + `decl-diff.ts` (253) — プレビューを持たない決定 (approach.md §9)。
  `stage4c/compare.ts` / `bytes.ts` — `coverage.ts` の前世代。`stage8/source-url.js` (67) — 決定 3 で
  採らない。**いずれも experiments には残す** (選択肢の保存)

### パイプラインの段の順序 — Rust CLI が守る制約【すべて実測】

1. **`ownership` は `merge` より前** — merge が上書きしてしまう「旧 IR の所有者」が要る
2. **`global` は `impact` より前** — 全域写像 delta (`global-set.txt`) が L3-2 の入力
3. **抽出 → ownership → merge はラウンド**。`--max-rounds` 既定 5、超過は exit 5
4. **`--render-all` (renderKey 変化) は `--mode` を上書きして `all` に落とす**
5. **空の再生成集合は render をスキップする** — プロトタイプでは呼び手側の
   `if [ ${#ONLY[@]} -eq 0 ]` が唯一の砦だった (→ §5)。**Rust では `ModuleSet::These(空)` が
   「何も描かない」を意味するので砦は型に移り、スキップは IR 全読みを省く最適化だけになった (M3-d2)**
6. **`--jobs` は常駐の起動時 cfg 固定** — 要求ごとに並列度は変えられない

### データフォーマット — 移設先が合わせる相手【すべて実物を開いて確認】

| | 実体 |
|---|---|
| **IR (schema 4)** | `index.json` (88,541 B) + `modules/<Module.Full.Name>.json` (432 個) + `deps/<PackageRoot>.json`。**テキスト JSON、非圧縮、合計 16 MB**。ファイル名はモジュール完全名そのまま (ディレクトリを掘らない)。**キーはアルファベット順** (Lean の `Json.mkObj` がそう出す) |
| **台帳** | `ledger.json` 1 ファイル (`ledgerSchema: 2`)。**`extractKey` / `renderKey` はこの中**。値は平文 (不一致がログで名乗るため)。ハッシュ対象は `.olean` / `.olean.server` / `.olean.private` の存在するもの全部 |
| **依存写像** | `link-index.lidx` **8,508,273 B** (行指向テキスト。レンダラが読むのはこちら、`render.ts:2067-2084` の手書きパーサ) / `link-index.json` 9,990,592 B |
| **サイト** | 432 ページ + 全域 6 本 = 438、移動後 **439**。**`style.css` / `jump-src.js` などの静的資産はこの木に入っていない** = byte 再現率の母数外 |

**`extractKey.extractor` / `renderKey.renderer` はソース中のハードコード文字列**
(`"lean-doc/experiments/stage4b"` / `"…/stage4c"`)【実測】。**Rust 版では必ず更新する** —
更新し忘れると「別実装なのに鍵が同じ」になり、キャッシュが誤ヒットする。
**罠がもう 1 段深い**【実測 2026-08-12】 — `extractKey.irGenerator` は IR の `generator` の値で、
**`extractor` と同一文字列**。こちらは「ディスク上の IR を書いたのは誰か」の事実なので**更新しない**。
参照台帳を素朴に全文置換すると**動いてはいけない鍵まで静かに書き換わる**。置換は鍵名でアンカーする。

---

## 7. Rust 側の構成

移設の順序 (§3 のストラングラー) がそのまま crate 境界になる。

| crate | 中身 | 移設 M |
|---|---|---|
| `lean-doc-ir` | IR (schema 4) の読み書き + **キャッシュ層の抽象** + 台帳 + 鍵 | M1 |
| `lean-doc-md` | md4c の FFI + doc-gen4 と同じ AST → HTML の組み立て | M1 |
| `lean-doc-render` | レンダラ (モジュールページ) | M1 |
| `lean-doc-global` | 全域成果物 6 本 + 検索索引 | M2 |
| `lean-doc-incr` | 増分 (detect / ownership / merge / prune / impact) | M3 |
| `lean-doc` (bin) | CLI。抽出器プロセスの起動・常駐の制御 | M4 |

### md4c — 移植ではなく本物をリンクする【すべて実測】

doc-gen4 は md4c の HTML レンダラを使わず `MD4Lean.parse` の AST から**自前**に HTML を組む
(`DocGen4/Output/DocString.lean:202-393`)。Rust 側も同じ形で、vendor するのは `md4c.c` / `md4c.h` だけ。
**ヘッダのレイアウトは C コンパイラに答え合わせさせる** — `csrc/layout_probe.c` の 123 項目を `tests/abi.rs`
が突き合わせる (**間違ったレイアウトがたまたまリンクする**のがこの crate の最大の失敗様式)。
**MD4Lean は 2 つの入力で死ぬ**: fenced code block 中の NUL は **SIGSEGV** (`wrapper.c:558`)、本文行の無い
GFM テーブルは **SIGABRT** (`wrapper.c:389` の assert)。どちらも Lean 側が未定義動作でバイト一致のしようが
なく、**Rust は落ちない方に倒した** (U+FFFD 置換 / 空 body)。対象パッケージの docstring には**どちらも
出現しない**【実測。4,858 件を通して確認】が、**第 2 の対象では出うる** (→ M5)。
**FFI が消すのは自前の CommonMark サブセット (`render.ts:1079-1672` の 594 行 = レンダラの 27%) だけ**
【実測】 — **autolink 解決系 (`render.ts:925-1077` の 153 行) は残る**。つまり **approach.md §5.6 の
「Rust なら本物のパーサに置き換えられる」は 594 行に効き、153 行には効かない。**

### M1 の結果 — オラクルの所在【すべて実測 2026-08-11】

移設元はすべて `stage7d/render.ts`。**オラクルは 2 種類しかない** — 「TS の関数を切り出したもの」と
「doc-gen4 / MD4Lean 本体を対象環境で実走させたもの」。所在は `crates/lean-doc-{md,render}/tests/`、
生成器は各 `tests/oracle/`。母数と結果【実測】: md4c AST **4,947/4,947**、AST → HTML **4,987/4,987**、
autolink **4,857/4,857** (+ referee 139/139)、コード片 **55,514/55,514**、`declHeader` **4,750/4,750** /
`declHtml` **4,560/4,560** / frame **432/432**、ページ **431/432** (差分 1 = §5 の登録済み乖離)。
コーパス【実測】: `known` 5,296 / `.lidx` 258,760 宣言 / `knownModules` 6,158 / コード片のアンカー
120,868 本 / `getRoot` 5 通り / `suppressed` 190。
**doc-gen4 をオラクルにできたのは環境が要らないから** — `nameToLink?` が読むのは `SiteContext.result`
だけなので、**空の `AnalyzerResult` を渡すと全ルックアップが外れ** Rust の `NoLinks` と一致し、referee では
2 フィールドだけ詰めれば第 1〜3 分岐が動く。**両仕様が一致する範囲**に絞れば doc-gen4 が審判になり、
**これで `inLink` の乖離が出た**。**referee は「プロトタイプが常に正しい」を仮定しないために作った**
(**M5 でも同じ手を使う**)。ただし **autolink だけは合わせる相手が `render.ts`**【判断】 — `nameToLink` は
両者別物で 439/439 を出しているのは後者。**副オラクル (参照ページ) が要るのは「入力が両側で同じ」を破るため**
— `moduleDeclNames` だけは関数を切り出せず、間違っていれば**両側が同じ誤った入力で一致してしまう**。
**Unicode 一般カテゴリの表は UnicodeBasic から吐かせた**【実測】 — Rust の crate を持ってくると**別の UCD 版**
になり V8 の `\p{P}\p{Z}\p{C}` と **4,802 コードポイントで食い違う**。`dump-gc.lean` が同じビルドから範囲表を
吐き `src/gc.rs` を生成する (`gen-gc-table.ts --check` が陳腐化を検出)。
**crate 境界は 2 つ動かした**【判断】 — `escapeHtml` は `lean-doc-md` に置いて再エクスポート、
`nameToLink?` の**第 1 分岐 (ソースパス) だけは注入点の外** (向こうに置くと **4,987 件中 131 件が外れる**)。

#### 全件バイト一致は分岐被覆の証明ではない — M1 で得た方法論【実測。M2 以降にそのまま効く】

**実データが到達しない分岐が、どの段でも無視できない割合ある**【実測。各段の `branchTotals`】。
**mutation で裏を取ると、全件バイト比較が緑のまま通る変異が各段にある。止めているのは手書きケースだけ。**
→ **全段で同じ形を取る**: 到達しない分岐を `branchTotals` で数え、**全部を手書きケースで埋め**、
`the_curated_cases_cover_what_the_package_does_not` 相当のテストが「テストがこの事実に依存している」ことを
明示的に守る。**「全件一致したから移設できた」と書かない。** 各段の実績【すべて実測】:

| 段 | 実データが到達しない分岐 | 全件バイト比較で捕まる mutation | 逃げた変異 |
|---|---|---|---|
| M1-d1 / d2 / d3 | 10 中 3 / 41 中 9 / 21 中 8 | 5 中 3 / 6 中 2 / 6 中 1 | — |
| M2 | 37 中 14 / 35 中 9 | **8 中 0** | — |
| M3-a (`detect`) | 53 中 19 | 7 中 1 (モジュールハッシュがファイルパスを無視する変異だけ) | 和集合→積集合 / `cmp_utf16`→`str::cmp` / `renderKey` 比較の省略は**台帳のバイトを 1 つも動かさない** |
| M3-b (`ownership`+`merge`) | 65 中 18 | 8 中 4 | 4 件 (deps の last-writer-wins / stale の UTF-16 ソート / deps root の順序 / `lostNames` の数え方)。**deps root の順序は本物の穴** — 最初の測定ではテストでも捕まらず、分岐 `depRootsAboveBmp` と手書きケースを足して塞いだ |
| M3-c (`impact`+`prune`) | 69 中 16 | 13 中 7 | 6 件 (U1 のソート / `selectedDeclarations` の出所 / `selectedIrBytes` の重複除去 / 空パスの symlink / `orphanPages` の打ち切り 20→50 / `metadata`→`symlink_metadata`)。**最後の 1 件は本物の穴** — ぶら下がりシンボリックリンクだけが両者を分ける。分岐 `pageIsDanglingSymlink` と手書きケースで塞いだ |
| M3-d2 (パイプライン) | 59 中 27 (7 状態に到達しない) | 14 中 5 (手書きケースが 7) | 2 件、どちらも**等価変異** (→ M3-d2 の節) |
| M3-d2b (`merge --modules`) | 71 中 23 (うち 5 件は手書きケースだけが到達) | 6 中 2 | 4 件、**すべて手書きケースが捕まえる** (7 状態の world には重複も依存名の衝突も無い) |

### M2 の結果 — 全域成果物・キャッシュ層・delta・ゲート【すべて実測 2026-08-11 / 12】

移設元は `stage7h/global.ts` → `crates/lean-doc-global`、ハーネス `tools/global-{reference,compare}.sh`。
**6 成果物すべて TS 版とバイト一致 (計 1,877,124 B)**。**`global-state.json` もバイト一致**させた
(Rust **841,947 B** / TS 841,949 B、差は `derivation` の 1 文字列 26 B ↔ 24 B のみ)。`ModuleFacts::tokens` は
6 成果物のどのバイトにも届かないので**全件比較では原理的に見えなかった**が、state を合わせたことで
**432 モジュール分の facts が丸ごとバイト照合された** (この `tokens` が L3-2 の入力で、M3-d4 で効いた)。
`STATE_DERIVATION` は `extractKey` / `renderKey` と同じ規律で**敢えてプロトタイプと違う文字列**。
**7 状態オラクル**は Rust の統合テストに移し、合成 IR と実 IR の両方で回す。実 IR の hit/miss は
0/432 → 432/0 → 431/1 → 430/0 → 430/1 → 429/3 → 0/432、**全 14 ステップで 6 成果物がバイト一致**。
**delta もプロトタイプと突き合わせた** — 参照 `name-map.json` に「移動 3 + before だけ 2 + after だけ 1」を
仕込み、`changedNames` **6** / `affected` **14** が一致、`--print-set` もバイト一致 (610 B / 14 行)。
変化ゼロなら**両者とも 0 B** (改行なし) — レンダラ側の「空集合と未指定を型で区別する」と対になっている。
**第 2 のオラクル (doc-gen4) は M2 では使えない**【実測】 — 同名 4 本のうち一致するのは `references.bib`
(0 B) だけで、`declaration-data.bmp` は**別物** (依存閉包 258,760 宣言 / `instancesFor` 有り
`dependencies` 無し / `kind` が `def`・`ctor`)。
**M2 のゲート = サイト 438 中 437 が byte 一致**【実測 2026-08-12】 (差分 1 = §5 の登録済み乖離のみ)。
**`w7h/base-pages` は参照に使えない**【実測】 — `run.sh` の `render()` が `--link-index` を渡していない
(→ 決定 4) ので、製品の既定とは別物。参照は `m1/ref-pages` 432 + `m2/ref-global` 6 = `m2/gate/ref-site`。

### byte 一致で Rust の既定が壊す箇所【すべて実測】

**最大の罠は UTF-16 で、2 つある。** TS は文字列を UTF-16 で持ち、Rust は UTF-8 で持つ。

| # | 何が | どこ | Rust 側で |
|---|---|---|---|
| **U1** | **`.sort()` は UTF-16 code unit 順**。移設対象は全部**引数なしの `Array.prototype.sort()`** (`global.ts` 7 箇所 / `impact.ts:211` / `ownership.ts:186` / `ledger.ts:213,338`) | `name-map.json` / `declaration-data.bmp` / `navbar.html` の**バイトがこの順序で決まる** | `Vec<String>::sort()` は **UTF-8 バイト順**。BMP 内は一致するが **U+10000 以上で逆転する** (`isLetterLike` が明示的に扱う数学英数字 𝒜 U+1D49C 系がまさにそれ)。**UTF-16 code unit 順の比較器を明示的に書く** |
| **U2** | **IR のスパンは UTF-16 code unit オフセット** (`render.ts:567-568` が明言)。`applyWsWidths` も `text[i]` で直接添字し、**その戻り値は再びスパンで slice される** (`render.ts:608-609`) | 位置つきタグを扱う**全パス** | **UTF-16 ↔ UTF-8 の変換層が要る**。`applyWsWidths` は長さ保存の書き換えなので offset は動かないが、**`String` に平坦化すると静かに壊れる** |

その他:

| 項目 | 事実 | Rust 側で |
|---|---|---|
| **HTML エスケープ** | `escapeHtml` は **`& < > "` の 4 種のみ**。**`'` を逃がさない** (`render.ts:330-341` = Lean の `Html.escape` の転写) | 一般的な HTML エスケープ crate は `'` や `/` も逃がす。**自前で書く** |
| **`<script>` 内の文字列** | `leanQuote` (`render.ts:785-797`) = Lean の `String.quote`。`\n \t \\ \"` と cp ≤ 31 / 127 の `\x%02x` のみ。**`\r` は `\x0d`、`\0` は `\x00`** に落ち、**`'` と U+0080 以上は素通し** | `format!("{:?}")` は `\r` / `\0` と書く — **この 1 文字でバイトが動く**。移植する |
| **`stringLt` / `nameLt`** | `stringLt` は code point 比較 (Rust の `str::cmp` と同値)、`nameLt` は**親を先に比較する再帰** (`render.ts:800-825`) で**成分の辞書順ではない** (成分数が少ない名前が文字列に関係なく先: `Zzz` < `Aaa.Bbb`)。import 一覧のソートに使う。**U1 の `.sort()` (UTF-16 順) と 100 行以内に同居している** | `nameLt` は独自論理なので必ず移植 — `Vec<&str>` の `Ord` に置き換えると**全く別のバイト**。U1 と取り違えても **BMP 内では一致する**ので `𝒜` が出るまで発覚しない |
| **`global.ts` の 6 成果物** | 明示的に事前ソートした配列から `Record` を組んで `JSON.stringify` (`global.ts:297-337`) = **挿入順依存だが決定的** | **`serde_json` の既定 (`BTreeMap`) だと再ソートされて `sortedNames + depNames` の混在順序が変わる。`preserve_order` / `IndexMap` が必須** |
| **`index.json` の書き出し** | `merge-ir.ts:243-250` はスプレッドで既存キー順 (= ソート順) を**偶然**維持している | 順序つきペア列で**明示的に**再現済み (M3-b)。ただし `dependencyMaps` 要素だけは**敢えて Lean 順**に変えた (→ M3-b の決着)。`modules` 配列は**渡されたモジュール一覧の順** (M3-d2b) |
| **`contentHash`** | Lean の `String.hash` (`lean_string_hash`、64bit) の 16 桁 hex (`Extract.lean:1786-1788`) | **Rust 側は再計算しない設計を推奨** — 抽出器が Lean のままなので**読むだけ**で足りる。再計算する設計にすると `lean_string_hash` の移植が必要になる |
| **台帳のハッシュ** | `crypto.subtle.digest("SHA-256")` / `--algorithm lake` は Lake の `<file>.olean.hash` を**読むだけ** | `sha2` crate で同値。`lake` 経路は文字列読み取りのみ。**`features = ["asm"]` が要る**【実測 2026-08-12】 — 既定だと同じ 237,909,832 B が 0.8631 s、asm 有りで 0.3022 s。プロトタイプ側は BoringSSL のハードウェア実装に届いているので、落とすと**同じ仕事の比較にならない** |
| **equation の長さ制限** | `[...s].length < 200` = **code point 数** (`render.ts:1683`) | `chars().count()`。バイト長でも UTF-16 長でもない |
| **浮動小数** | **生成バイトに float は入らない** (`toFixed` はレポートと timings のみ) | 影響なし |

**Deno 固有 API はほぼ無い**【実測 — 移設対象 8 ファイルの `Deno.*` を全列挙した】。`writeTextFile` /
`readTextFile` / `exit` / `args` / `mkdir` などは `std::fs` + `std::env` に 1:1。置き換えが要るのは 3 つだけ:
`crypto.subtle.digest` → `sha2`、`CompressionStream("gzip")` (**gzip サイズの計測専用なので製品では不要**)、
`performance.now()` → `Instant`。

### M3-a の結果 — `detect` (olean ハッシュ台帳)【すべて実測 2026-08-12】

`stage5/ledger.ts` → `lean-doc-incr/src/{ledger,detect}.rs`、ハーネス `tools/ledger-{reference,compare}.sh`。
**12 シナリオを 1 箇所で定義して両実装に回す**【判断、以降の段でも同じ】 — 一致すべきはファイルではなく
**問いへの答え**なので、問いを両側で書くと誰も意図していない 2 つを比べることになる。**78 ファイルすべて一致**
(台帳 8 本は**意図的に変えた 2 文字列 = 計 15 B** だけが違う。`--concurrency 8` の台帳は 1 と完全一致 =
**バイトはスケジューリングに依存しない**)。対象の 432 モジュールには **module system の 3 ファイル形が
構造的に無い**ので、Mathlib 8 モジュール (24 ファイル / 10,323,184 B) を参照集合に足して埋めた【判断】。
**速度**【実測 2026-08-12。Apple M1 / 16 GB / 並列度 1 / 237,909,832 B / 同一セッション各 8 回。生ログ
`benchmarks/results/m3a-ledger-*.jsonl`】 — warm 中央値 Rust **0.1424 s** / TS **0.2468 s**、cold 0.2883 /
0.3797 s。**分母は同一の仕事**だが**言語の速さの話にはできない** — 内側の `hashSeconds` は両者ともハードウェア
SHA-256 に届いており、差の出所は読み取り経路 (peak RSS 147 MB 対 8.2 MB) とプロセス起動 (0.005 s 対 0.037 s)。

### M3-b の結果 — `ownership` + `merge`【すべて実測 2026-08-12】

`stage5/{ownership.ts,merge-ir.ts}` → `lean-doc-incr/src/{ownership,merge}.rs`、ハーネス
`tools/merge-{reference,compare}.sh` (1 ラウンド = 抽出 → ownership → merge なので 2 段を 1 本で回す)。
編集は**対象リポジトリではなく IR に注入**。**母数 4,051 / 一致 3,986 / REORDERED 65 / 差分 0 / 欠落 0**
(REORDERED は下記の意図した乖離のみ = `deps/*.json` 48 + `index.json` 17)。
**BMP 外の名前は対象 IR に 0 件**なので隣のパッケージの手では埋まらず、手書きケースのまま【実測】。
**`--modules` はここでは再現しなかった**【実測】 — `merge-ir.ts:29,40` が usage に出しながら一度も読まない
**未実装のフラグ**だったため。M3-d2b でこちらの設計として足した。

**`deps/*.json` と `index.json` の `dependencyMaps` は Lean 順で書く**【決着】 — キーは `Json.mkObj` の
アルファベット順、配列はコードポイント順 (`Extract.lean:2050-2057`)。**ここだけプロトタイプ (UTF-16
`.sort()`) と分かれる。** 推測ではなく実物 (`w7h/base-ir/deps/*.json`) を開いて決めた —
**`merge-ir.ts:44` の「Lean の外では再現できない」は事実として誤りだった**【実測】。
**移設元のコメントを根拠に使うときは実物で裏を取る。**
結果【実測】: 何も変わらない再抽出を合流した木は **from-scratch と 436/436 バイト一致**、
**サイトのバイトには届かない** (キー順だけ違う 2 本の IR から **438/438 = 31,617,612 B**)。
乖離は `tests/merge.rs` が差分集合 34 件を `assert_eq!` で固定し、**プロトタイプと違うことと、独立に書いた
Lean 順 writer と同じことの両方**を assert する (片方だけだと「壊れた」と「意図どおり」が区別できない)。
**M3-d4 のゲート 1 (439/439 / 437/437) が、この向きが正しかったことを実地で裏づけた。**

### M3-c の結果 — `impact` + `prune`【すべて実測 2026-08-12】

`stage5/{impact.ts,prune-pages.ts}` → `lean-doc-incr/src/{impact,prune}.rs`、CLI `lean-doc impact|prune`、
ハーネス `tools/impact-{reference,compare}.sh` は**シナリオ 29 本 (impact 18 / prune 11) を 1 箇所で定義**。
**母数 3,631 / 一致 3,631 / 差分 0 / 欠落 0**。**この 3,631 は 1 つの問いの答えではない** — 内訳は
生き残ったページ **3,458** (`prune` を回した 8 本のページ木の残存ファイル =「どちらも消さなかった」の確認) +
計算された記録 **165** (終了コード / stdout / `--print-set` の有無 / 選択集合 / summary / census / 残存一覧) +
入力 fixture 8。比較対象外が 29 (`*-stderr.txt` は診断文言が実装のものなので「文句を言ったか」だけ全件比較)。
→ **「同じ答えを計算するか」の分母は 165、「正しいファイルだけを消したか」は 3,458。**
**`prune` は破壊的なので防御を構造で入れた**【判断】 — ①`PageRoot` を字句 (`..` / NUL) と物理
(`canonicalize` した親がルート配下か) の両方で**削除直前に**検査、②パス組み立ては**連結であって
`Path::join` ではない** (`join` は絶対パスで左辺を捨てるので `/etc/passwd` 行がルート外を指す)、
③walk は `file_type()` で symlink を追わない、④`--dry-run` は全部計算して何も書かない。
**実地確認**【実測】 — ルート外を指す絶対パス / `..` / ページ木内の symlink 経由の 3 形を投げて、
ルート外は 1 バイトも消えず (symlink 経由は exit 3 で拒否)、ページ木も無傷。

### M3-d1 の結果 — `site` (フル生成を 1 コマンドに)【すべて実測 2026-08-12】

移設元は `stage7h/run.sh:78-80` の `render()` **3 行** (独立したスクリプトは存在しない)、移設先
`lean-doc site`、ハーネス `tools/site-compare.sh`。**母数 438 / 一致 437 / 差分 1 (= §5 の登録済み乖離のみ)
/ 欠落 0 / 余分 0**。**合成が何も足していない**ことも確かめた (`render` + `global` を別プロセスで回した木と
**438/438 一致**)。
**決定 4 を既定ではなくフラグの形で塞いだ**【判断】 — `--link-index` / `--no-link-index` の**どちらか必須**。
**ガードが効いていることを実測した** — 同じ IR・同じ `--source-url` で `.lidx` の有無だけを振ると
**438 中 150 が差分** (= 432 ページ中 150。全域 6 本は不動)。**移設元はこれを渡していない**ので、
そのまま写せば 3 度目を踏んでいた。呼び手側の実在フラグは「unknown argument」にせず**名指しで断る**【判断】。
**フル生成と増分で 2 段の順序が逆なのは矛盾ではない**【実測】 — 制約 2 (§6) は**全域 delta が再生成集合の
半分だから**であって成果物の依存ではなく、delta の無いフル生成では順序は自由 (両段の書き込みは 1 ファイルも
衝突しない)。

### M3-d2 の結果 — `incremental` + `modules` (並べる人)【すべて実測 2026-08-12】

`stage7h/incremental.sh` + `run.sh:82-86` の `modlist()` → `crates/lean-doc/src/pipeline.rs`、CLI
`lean-doc incremental|modules`。**段はすべてライブラリ呼び出しで、外部プロセスは抽出器だけ**。
**`--extractor <program>` は必須で既定値なし**【判断】 — 起動形は `<program> [<extractor-arg>…] --modules
--ir-dir --timings` = `stage7g/extract-once.sh:24,47` の必須引数そのもの。役目は ①**M4 の境界**
(既定値を持たせると製品が凍結済み `experiments/` に依存する)、②**Lean 無しでパイプラインをテストできる形**
(テストは偽抽出器を渡す。無いと全テストが Lean 依存になり事実上テストされない)。
**ゲート = `oracle.sh` の 7 状態をパイプライン全体に回し、各状態でフル生成 (`lean-doc site`) と byte 比較**
【実測】。fixture は合成パッケージを source / olean / IR の 3 層で持ち、olean と IR を**独立に**動かせる
(宣言の移動で referrer の olean が動かない = L3-1 が要る理由を fixture が含む)。
**サイト 7 状態すべて全件一致 (83/83)、IR は 52/55 → M3-d2b で 55/55**。

**負債 8 件の決着**: ①②③ **構造で消した** — 2 つの再生成集合の合流は**メモリ上の和集合**で、ファイルを
経由しないので「無いファイル」という状態が存在しない (3 本は診断として同名で残すが**読み戻さない**)。
④ **型で閉じた** (`prune_removed` に IR 木を名指す引数が無い)。⑤ `lean-doc modules --root --lib` を新設。
⑥ ラウンド開始前に `--work` へ退避。⑦ **`incremental` でだけ 40 桁 hex を検査** (`render` / `site` の緩さは
不変【判断】)。⑧ フル生成 (`site --state`) が書き、増分が毎回更新する。
**プロトタイプと意図的に違えた点**: `--l3-1` / `--global` / `--count-reads` / `--module` / `--serve*` /
`--jobs` は**名指しで断る**。`--mode` の未知値は**起動時に exit 2**。抽出器の失敗は **exit 4** (5 は
「収束しなかった」専用)。**`--ledger` は読むだけで書き戻さない** — 連続実行のあいだ誰が更新するかは **M4**。

**モジュール一覧の順序は 2 つの成果物のバイトを作る**【実測 2026-08-12。以降の全段に効く】 —
①**台帳の `modules` 配列順** (同じ 432 モジュールで 2 通りの順から作ると 120,103 B 同士でバイトが違う)、
②**IR の `index.json` の `modules` 配列順** — **抽出器は渡されたリスト順をそのまま保つ**。
**サイトのバイトには届かない**。プロトタイプの `sort` はロケール照合で `.` と大小文字を無視するので、
**同じ 432 件・同じ集合だが 163 行が別の位置**に来る。**U1 の UTF-16 順**に固定した (`LC_ALL=C sort` と
432/432 一致)。→ **増分側と from-scratch 側には必ず同じ一覧を渡すこと**。違う一覧だとパイプラインと
無関係に `index.json` だけで落ちる (M3-d4 で実際に 1 度踏んだ)。
**`diff` のハンク数を移動量に使わない** — ブロック移動を 1 と数えるので大幅に過小評価する。分母は
「同じ添字で食い違う行数 / 総数」。
**逃げた mutation 2 件はどちらも等価変異**【実測】。**「捕まらないこと」を assert で固定**した
(3 巡目が可能になったら落ちる)。

### M3-d2b の結果 — `merge --modules` (index の並び)【すべて実測 2026-08-15】

M3-d2 が残した「モジュールが増えると `index.json` だけ from-scratch と違う」を塞いだ。**`MergeOptions.modules`
/ `lean-doc merge --modules <file>` を新設**し、パイプラインは `detect` に渡すのと**同じ一覧**を `merge` にも
渡す (渡さないときの挙動は不変 = base 順 + 末尾 push)。**プロトタイプが usage に出して一度も読まないフラグ**
なので移設ではなく**こちらの設計**。前提は実測済み — **抽出器は渡されたリスト順をそのまま保つ**
(`w7h/base-ir/index.json` の 432 件はロケール `sort` の出力と 432/432 一致)。
**7 状態オラクル: IR 55/55 バイト一致** (状態別 8・8・8・7・8・8・8。**前は 52/55**)。**サイトは 83/83 の
まま**。差分集合の固定は消さず**空であることを assert** にしたので、戻ったら状態名が出る。**M3-b の
436/436 は不動**【実測】 — 何も変わらない再抽出を合流した木は from-scratch 木と `/usr/bin/diff -r` で差分 0
= `--modules` を渡さない経路は 1 バイトも動いていない。
**リストと木が食い違ったら exit 3 で拒否**【判断】 — 黙って末尾に足せば増分と from-scratch が永久にずれ、
黙って落とせばファイルがあるのに index に居ないモジュールができる。どちらも `index.json` だけの差なので
**ページのバイトに出ず、ゲートで見えない**。**書き出す前に**判定するので拒否された木は作り直せる。
**並びは `deps/*.json` のバイトにも届く**【実測】 — 依存名は「後に来たモジュールが勝つ」ので、同じ名前を
別の定義モジュール経由で参照する 2 モジュールの順序が変わるとスライスのバイトが変わる。手書きケースで
両方の順の出力を並べて固定した。

### M3-d3 の結果 — 差分ハーネス (計測対象で両実装に同じ問いを回す)【すべて実測 2026-08-15】

ハーネス `tools/incremental-{reference,compare}.sh` (558 / 219 行)。**7 シナリオを 1 箇所で定義**し、
`lean-doc incremental` と `experiments/stage7h/incremental.sh --global new` に回す。抽出器は**両側とも**
`stage7g/extract-once.sh`。変化の注入は `ledger touch` (計測対象を編集しないため — `incremental.sh:82-87` の protocol)。
**台帳と state は impl ごとに種を作る** — `extractKey.extractor` / `STATE_DERIVATION` は「別実装なら別文字列」が設計
(§6) なので、TS の種を Rust に食わせると鍵不一致で 432 件全部が再抽出になり、パイプラインと無関係に落ちる。

**母数 3,213 / 一致 3,189 / REORDERED 14 / ARRAY-REORDERED 8 / 差分 2 / 欠落 0。**
内訳 — IR のファイル **3,050** (modules 3,023 = 432×6 + 431、deps 20、index 7) + 全域成果物 **48** (8 木 × 6 本) +
**計算された記録 113** (work 診断 69・counts 7・exit 7・complained 7・work-present 7・ページ一覧 8・件数 8) + 種 2。
→ **「同じ答えを計算するか」の分母は 113 + IR の index/deps 27**。3,023 は「どちらも壊さなかった」を言うだけ。
**中核は 7/7 バイト一致** — `counts.json` (`rounds`/`staleFound`/`changed`/`removed`/`irChanged`/`globalStale`/
`pagesRendered`/`mode`) が全シナリオで一致。`pagesRendered` は census で説明がつく
(`importers-hub` 262 = 1 + importersTransitive 261、`referrers-two` 52 = referrersDirect 49 + 1 + 2)。

**残った差 24 件はすべて登録済みの意図した差**:
**REORDERED 14** = `deps/*.json` のトップレベル鍵順 (M3-b の決着)。**Rust 側が凍結 fixture
`w7h/base-ir/deps/*.json` = 抽出器自身の出力とキー順まで一致し、TS 側は一致しない**ことを実測で確認した。
**ARRAY-REORDERED 5** = `index.json` の `modules` 配列順 (M3-d2b)。**merge が走る 5 シナリオ全部に出る** —
凍結 fixture の index 自体がロケール順で、code point 順の `--modules` を渡す製品が毎回並べ直すため
(`w7h/base-ir/index.json` は code point 順と **163/432 行が別位置**)。集合は同一、バイト長も同一。
**ARRAY-REORDERED 3** = `render-set.txt` の行順 (U1。`sort -u` のロケール照合 対 UTF-16 順、163/432・57/262・2/52)。
**差分 2** = `nochange` / `removed-one` の `*-complained.txt` (ts=yes / rust=no)。**中身は §7 の負債 1/2/3 そのもの** —
`impact-set.txt` が書かれない run でプロトタイプの `sort -u` が `sort: No such file or directory` を吐き、
`|| : > "$RENDERSET"` が再生成集合を空にする。**製品をプロトタイプに合わせて壊した箇所はゼロ。**

**ページのバイトは両実装間で比べていない** (決定 4 — プロトタイプ step 7 は `--link-index` を渡さない)。
代わりに **Rust 側だけ「増分の木 vs 同じ最終 IR からの `lean-doc site`」を 7 シナリオで比較し、全部 identical**
(438/438、`removed-one` は 437/437)。**`added-one` は自分で site を作り直して `/usr/bin/diff -r` 差分 0 を再現した**。
種の検査は `m2/gate/ref-site` と **438 中 437 一致** (差分 1 = §5 の登録済み乖離)。

**M3-d3 が踏んでいない経路**【実測】 — `ledger touch` は olean の内容を変えないので、**L3-1 と L3-2 は
7 シナリオすべてで空の答えしか返していない** (`staleFound` 0 / `globalStale` 0 が 7/7。`irChanged` も
3 シナリオで 0)。ラウンドが 2 巡した run も無い。**両実装が一致したのは「同じ空集合を出した」ことを含む。**
→ これを踏んだのが M3-d4。**同じ罠は M5 の第 2 の対象でも待っている** — 変化の注入が本物でなければ、
一致は「両方とも何もしなかった」を意味しうる。

### M3-d4 の結果 — クローンでの本ゲート (M3 の完了条件)【すべて実測 2026-08-15】

`tools/clone-gate.sh` (735 行、`incremental-reference.sh` の兄弟なので `incremental-compare.sh` が無改造で読む)。
**クローンでは何も偽装しない** — ソースを本当に編集し `lake build` を本当に回す。前提として
`stage5e/rebuild-own.sh` で自パッケージ 432 モジュールを**クローンのパスで焼き直す** (677.06 s / peak RSS 3.41 GB)。
**これは load-bearing** — 焼き直さないと「参照側の olean は移動で動かない」という L3-1 の前提が壊れ、**ゲートが誤った
理由で通る**。`crates/` は 1 行も直さずに通った。

| ゲート 1 (増分 == フル生成) | 母数 | 一致 | 差分 | 欠落 | 余分 |
|---|---:|---:|---:|---:|---:|
| **move** (`BroadcastChannel.Basic` → `…BasicCore`、shim 化 + `lake build`) | **439** | **439** | 0 | 0 | 0 |
| **delete** (`Shannon.ArithmeticCoding` を消して root の import 1 行も消す + `lake build`) | **437** | **437** | 0 | 0 | 0 |

from-scratch 側は**編集後のソースを独立に全抽出して `lean-doc site`**。M3-d3 の sitecheck (同じ IR を再レンダ) より
強く、merge が誤った IR を作れば打ち消せない。IR も別レイヤで一致 (437/437・435/435)。

**ゲート 2 (両実装の一致)**: 母数 928 / 一致 920 / REORDERED 7 / ARRAY-REORDERED 1 / **差分 0** / 欠落 0。
`counts.json` は 2 シナリオとも**両実装バイト一致**。眼目の 3 数字【実測】:

| | rounds | changed | removed | **staleFound** (L3-1) | **globalStale** (L3-2) | irChanged | pagesRendered |
|---|---:|---:|---:|---:|---:|---:|---:|
| move | **2** | 5 | 0 | **30** | **1** | 35 | 36 |
| delete | 1 | 1 | 1 | 0 | 0 | 1 | 1 |

#### L3-2 が load-bearing であることを、初めて生成バイトで示した【実測】

**render set 36 = seen 35 (changed 5 ∪ stale 30) ∪ `{…BroadcastChannel.Marton.Basic}`。**
`Marton.Basic` は **seen に入っていない** — その olean も refs も動いていないので L3-1 は届かない。届いたのは
全域写像 delta で、witness は「`InBCCapacityRegion` を docstring の code span で名指ししている」1 件
(`changedNames` 25 → `affected` 1)。**そのページのバイトは実際に動いた**: `Marton/Basic.html` が
**16,611 → 16,615 B** で、差は autolink の宛先 `…/BroadcastChannel/Basic.html#InBCCapacityRegion` →
`…/BasicCore.html#InBCCapacityRegion` (+4 B = `Core`) の 1 箇所だけ。
→ **L3-2 が無ければゲート 1 は 439 中 438 で落ちていた。** 2 つの再生成集合を別々に導出する設計 (approach.md §5.5)
が、移設後の実装で必要だったことの直接の証拠。

**ページ間リンクの出所は 2 つで、2 つの導出がそれぞれを担当している**【この move で確認、n=1】 —
印字シグネチャ由来 (IR の `refs`) は **L3-1** が参照側を再抽出して直し、docstring 由来 (`ModuleFacts::tokens`) は
**L3-2** が拾う。ゲート 1 の 439/439 は、この move についてカバーが**漏れていない**ことを言う。

#### 移動対象の選び方 — referrer を持つだけでは足りない【実測。ここが今回の発見】

`stage5e/setup-clone.sh:18-25` は「A は referrer を持つものを選べ」と書いている。**必要だが十分ではない。**
最初 `Shannon.Huffman.Length` (referrersDirect 4) で完走させたが **`globalStale` は 0 のまま**だった
(ゲート 1 は **439/439 PASS**、rounds 2 / staleFound 1)。L3-2 が読むのは `ModuleFacts::tokens` =
**宣言 docstring の code span と markdown link target だけ** (`lean-doc-global/src/facts.rs:94-108`) なので、
効くのは「自分の宣言名が**他モジュールの docstring に出てくる**」モジュールに限られる。
**このパッケージでそれは 432 中 7 個だけ**【実測。base state の `tokens` × `name-map.json` の
パッケージ内 4,750 名で独立に計算】 — `AEP.Basic.Core` 2 / `DifferentialEntropy` 2 /
`BroadcastChannel.Basic` 1 / `EPI.G2.HeatFlowContinuity` 1 / `CondKLIntegral` 1 / `FisherInfo.Gaussian` 1 /
`Fano.Measure` 1。`Huffman.Length` は **0**。→ **L3-2 を踏みたいならこの 7 個から選ぶ。**

#### 削除シナリオが担当するもの【実測】

`staleFound` / `globalStale` はどちらも 0 で、これは**事前予測どおり** (`ArithmeticCoding` は referrersDirect 0、
6 名はどの docstring にも出ない)。削除の担当は **prune / index からの脱落 / 孤児 olean** であって L3-1・L3-2 ではない。
**Lake は消えたソースの olean を消さない** — 孤児が `.lake/build/lib` に残ったまま台帳は fixed point を保った。
モジュール一覧をソースの glob で作る規則 (§5 の M3-d) が実地で効いた。

#### クローンの復帰

`setup-clone.sh reset` の後、`git status` 空 / HEAD `ca4fd931…` / `lake build --no-build` =
**All targets up-to-date (3779 jobs)** / **base 台帳が fixed point** (0 changed / 0 removed / 0 render-all)
= 432 個の olean が sha256 レベルでベースラインと同一に戻った (2 度の move と 1 度の delete をまたいで)。
孤児 olean 2 個 (`LengthCore` / `BasicCore`) は**意図して残した** — 削除シナリオが検証している現象そのもの。

### M4-a / M4-b の結果 — 抽出器の移動と `lean-doc extract`【すべて実測 2026-08-15】

**M4-a**: `stage7d/{Extract.lean,build.sh}` → `extractor/` (2,822 / 39 行 + `README.md` 85)。
**ゲート = 凍結バイナリとの IR バイト一致** — `experiments/stage7d/build/extract` (**実行のみ。再ビルドしない**)
と `extractor/build/extract` を**同じ 432 件の一覧・同じフラグ**で走らせ、**436/436 バイト一致**
(432 モジュール + `index.json` + `deps/*.json` 3)。差分 0 / 欠落 0 / 余分 0。
ソース差分は **7 hunk / 86 行**で、**IR 生成のコードには 1 行も触れていない**。

**挙動を変えたのは 1 箇所だけ**【判断】 — `resolveIrDir` (フラグ → `IR_DIR` → `defaultIrDir` の 3 供給源) を
`getIrDir` (フラグのみ) にし、**`--write-ir` に `--ir-dir` が無ければ引数解析の時点で exit 1**。
`Extract.lean:1766` に焼かれていた旧セッションの scratchpad 絶対パスは**定義ごと消えた**。
**発火歴が無いことこそが穴の性質** — フラグを忘れた `--write-ir` が誰も名指ししていない場所に数 MB 書いて
成功を報告する。`IR_DIR` は同じ穴の入口違い (コマンドラインの外からフラグの値が来る) なので同時に塞いだ。
書き出しは 20 秒の抽出の**最後**なので、使用箇所で落とすと全額払ってから usage エラーが届く。

**M4-b**: `stage7g/extract-once.sh` (87) → `crates/lean-doc/src/extract.rs` (399) = `lean-doc extract`。
**サブコマンドにしたので継ぎ目は閉じない**【判断】 — 製品は `--extractor lean-doc --extractor-arg extract` で
自分自身を抽出器にできる一方、パイプラインのテストは今も偽抽出器を渡せる (171 MB のバイナリは対象の
toolchain に対して作られるのでリンクインは不可能)。抽出器と対象の場所は**フラグ + 環境変数、既定なし**
(`defaultIrDir` と同じ穴)。`--ir-dir` が `--target` の下なら **exit 3**、しかも**ディレクトリを作る前に**判定する
(作ること自体が対象への書き込み)。`--serve*` は「M4-c で来る」と名指しで断る。

ゲート 3 本【すべて実測】: ①`extract-once.sh` と **IR 436/436 バイト一致**。②timings は鍵集合 **91/91 一致**、
時間フィールド 32 個を除いた **59 個の値が全一致** (`targetModules` 432 / `jobsRequested` 4 を含む)。
③**M3-d3 のハーネスを `--extractor product` で回して、`experiments/` のシェルで取った記録と 3,213/3,213
IDENTICAL** (reordered 0 / array-reordered 0 / 差分 0)。**抽出器を製品側に差し替えても答えが 1 バイトも動かない。**

**壁時計を実装の差として読まないこと**【実測】 — 全抽出 432 モジュールの user 時間は 4 本とも
**15.1〜15.5 秒に収まる (1% 以内)** 一方、壁時計は 7.7〜22.2 秒に散る。差は page cache であって実装ではない。
**`--jobs 4` では「壁時計 ≒ CPU 時間なら warm」の判定法は使えない** (user > wall が正常)。

### M4-c の結果 — 常駐抽出器【すべて実測 2026-08-15】

`stage7g/serve-ctl.sh` (185) → `crates/lean-doc/src/resident.rs` (1,085) + `tests/resident.rs` (310)、
`incremental --serve`。**これで `experiments/` からの未移設はゼロ。**

**FIFO + holder ではなくパイプにした**【判断】 — 駆動側が 1 プロセスなので要求チャネルは子の stdin でよい。
holder / pid ファイル / `mkfifo` / ポーリング / prefix カウントが全部消え、代わりに構造的な性質が入る:
**パイプは書き手より長生きできない。** 駆動プロセスがどう死んでも (エラー経路・panic・SIGINT・**SIGKILL**)
カーネルが write 端を閉じ、サーバは EOF を見て自分の exit path から出る。**プロトタイプの holder は別プロセス
(`sleep`) なのでこの性質を持たず、だから `trap` が要り、その `trap` は SIGKILL を覆えない。**

**`--serve-dir` / `--serve-from` は受けない**【判断】 — 正しさはサーバの olean 世代から来る【実測、stage 6a】
以上、**自分が起動していないサーバは世代を保証できない**。`--serve` は run の内側で起動するので、
stage 6a が間違いと測った形 (編集前に import されたサーバ) は**書けない**。
**「書けないはず」を測れる形にした** — `Generation` = `--modules` 全モジュールの olean を Lake 自身の
content hash で読んだもの (**台帳が見るのと同じファイル・同じ順序**なので、ガードと `detect` が「世界」の
定義でずれない)。run の先頭で 1 回取り、①各要求の前 ②`ready` 直後 (import 窓を閉じる) ③各要求の後、で
突き合わせ、食い違えば **exit 3**。
**サーバは遅延起動、ラウンドループ直後に停止**【判断】 — 抽出が 0 件の run (このパイプラインが最も多く出す
答え) は 3 GB を一切払わない。実測: ハーネス 7 シナリオ中 **4 つだけが起動、3 つは起動しなかった**。
**サーバの stderr は継承する** (プロトタイプは `serve.err` に落とす) — 実測で load-bearing だった:
`lake` の doc-gen4 警告が **7 シナリオ中 5 つの `complained=yes` の出所**で、捕捉すると答えが動く。

**ゲート 4 本**【すべて実測】: ①常駐と単発の IR が**全体 436/436・部分集合 3/3 バイト一致**
(部分集合を測るのはパイプラインが投げるのが部分要求だから)。②M3-d3 のハーネスを常駐経路で回して
**3,213/3,213 IDENTICAL** — この母数は `extract-once.sh` + 凍結バイナリで取った記録なので、**常駐経路の IR =
プロトタイプの抽出器が出す IR** が 7 シナリオ分の IR 木ごと一致している。③**実 3 GB サーバの駆動プロセスを
SIGKILL** して (`trap` も `Drop` も走らない経路) `pgrep` **空**。④**stale なサーバを実際に作って拒否させた** —
クローンでサーバの import 中に `Bridge.olean.hash` を書き換え、**exit 3 + モジュール名**。後始末まで確認
(`.hash` をバイト復元、クローンの `git status` 0 行、台帳が不動点に復帰)。

#### 実際に発火した穴 — 計測対象に書き込みが起きた【実測。M4-b の欠陥】

**`lean-doc extract` は相対パスを対象リポジトリの中に解決していた。** 子の cwd が `--target` なので
`--ir-dir out` は**「対象配下なら拒否」のガードを通り抜けて** `<target>/out` に書く — 「計測対象には決して
書かない」と見出しに書いてある当のコマンドから。M4-c のゲート実行で発覚し、
**`/Users/haruka/dev/lean-projects/oneshot-432-events.jsonl` (0 B) が実際に作られていた**。削除済み・対象の
`git status` は `-uall` で 0 行に復帰済み。修正は子に渡す 4 つのパスを **symlink を解決しない `absolute()`**
で絶対化 (絶対パスは 1 文字も変えないので M4-b の記録との比較可能性は不動) + 回帰テスト。
→ **教訓: 「対象配下か」の検査は、検査した文字列と子が解決する文字列が同じときにしか効かない。**
子の cwd を変えるなら、パスは渡す前に絶対化する。

**副産物**: `serve-ctl.sh:120-121` の起動コマンドラインは**製品バイナリでは起動しない** — `--ir-dir` 無しで
`--write-ir --serve` を渡すので M4-a が入れた相互フラグ検査が exit 1 で断る (凍結バイナリには既定値が
あったので通っていた)。M4-a の締め付けに歯があったことの実証。
プロトコルは空白区切りなので**空白を含むパスは送れない** — 黙って 2 本のパスに化けるので送信前に exit 3。

**時間は方式比較に使えない**【実測】 — 432 モジュール全抽出で単発 24.18 s / 常駐 8.40 s (壁時計) だが
**user は 15.35 / 15.66 s = 1.4% 差**で、差は page cache (単発が先に走って暖めた)。さらに
**M4-c は常駐の利得を測っていない** — ゲートのどのシナリオも `rounds` 1 で要求は 1 回きり。
**常駐が効くのは 2 回目の要求から**なので、**分母のある倍率はまだ存在しない**。

### M4-d の結果 — `lean-doc build` (M4 の完了条件)【すべて実測 2026-08-15】

`crates/lean-doc/src/{build.rs,lakefile.rs}` (985 / 227) + `tests/build.rs` (1,194、偽抽出器で Lean 不要) +
`tools/build-gate.sh` (370)。実行形:
`lean-doc build --root <repo> --out <dir> --link-index <lidx> --extractor-bin <bin> [--jobs N]`。
**`--lib` / モジュール一覧 / `--source-url` / 初回か 2 回目か / state・台帳・work の場所は全部コマンド側。**

**ゲート**【すべて実測。全部クローンで回した — 計測対象は読むだけ】:
①clean なクローンで `build` 1 回 → **`lean-doc site` の木と 438/438 バイト一致**、IR も参照と 436/436。
②何も変えずもう 1 回 → **0 changed / `nothing to render` / sha256 マニフェスト 438 行が完全一致 = 1 バイトも動かず**、0.30 s。
③本物の移動 (`BroadcastChannel.Basic` → `…BasicCore` + `lake build`) → `build` は
**5 changed / rounds 2 / staleFound 30 / globalStale 1 / irChanged 35 / 36 ページ** (**M3-d4 の数字と完全一致**)、
**もう 1 回 `build` で 0 changed** = 書き戻しが効いている。④③の木と**編集後ソースからの from-scratch**
が **439/439 バイト一致**、IR 437/437、**台帳もバイト一致** (142,889 B)。⑤`reset` でクローンがベースラインに復帰。

#### 台帳をいつ書くか — 規則は 1 文【判断。これが M4-d の要点】

**抽出の前にハッシュし、レンダの後に書く。** 両側に失敗様式がある:
- **早く書くと** (ページより前) — レンダラで落ちた run が「全部最新」の台帳を残し、次の run は 0 changed /
  0 render。**サイトが永久に半分古いまま、診断ゼロ。**
- **遅く読むと** (終了時に再ハッシュ) — run の最中に焼き直された olean が「その IR の出所」として記録され、
  そのモジュールは**二度と再抽出されない。**

実装はハッシュを `CheckSummary.fresh` (= detect が見た世界) から取り、ファイルは成功した run の最後に書く。
**`extractKey` だけは例外で、いま存在する IR から再計算する** — detect のコピーを書き戻すと `irGenerator` が
前の木のままになり、以後永久に全 432 件再抽出になる。失敗経路 (exit 1/3/4/5) はどれも書かない。
**やり直しは安全な向き**で、次の run の `changed` が名乗る。
**mutation で裏を取った**【実測】 — 「抽出の後・レンダの前に書く」変異は **14 本中 13 本が緑のまま通る**。
捕まえるのは「壊れた IR を吐く偽抽出器でレンダラを落とす」1 本だけ。増分経路では同じ変異が**書けない**
(ハッシュは成功時にだけ返る値の中にしかない = 型で閉じている)。

#### 出力先と `--lib` — どちらも「静かな過少」を防ぐ形にした【判断】

**`--out` は必須・既定なし。** 素直な既定 `<root>/.lake/build/doc` は **doc-gen4 の出力木** (対象では
736 MB / 約 9 時間) で、他ツールの成果物を上書きする既定はデータ消失。`--out` が `--root` 配下なら **exit 3**。
所有権は `lean-doc-build.json` マーカー (run の前に `complete:false`、台帳の後に `complete:true`) で見て、
**マーカーの無い非空ディレクトリは exit 3**。**`--full` も所有権検査を飛ばさない** — フル生成は site と ir を
**消す**経路なので、先に答えるとマーカーを一度も読まないディレクトリを削除できた (実装中に作って塞いだ穴)。
**`--lib` は `lakefile.toml` の素の `[[lean_lib]]` + `name = "<Ident>"` だけ読む。** `lakefile.lean` を含む
読めない形は**全部 exit 3 + 「`--lib` を渡せ」**。防いでいるのは**静かな過少読み** — ライブラリを 1 つ
取りこぼした一覧は「全モジュールが削除されたパッケージ」と見分けがつかない。**推測でパースしない。**
`--source-url` は git から導出し、**github.com の remote だけ** (`/blob/<rev>/` は GitHub の形で、採点器の
正規化も `/blob/[0-9a-f]{40}/` 固定 → 決定 1)。他ホストは名指しで拒否。

#### 残した穴 — `renderKey` は依存写像を覆っていない【実測。直すのは M5】

鍵は `renderer` + `sourceUrl` だけなので、**IR が同じで `.lidx` だけ変わった run は検出されない** —
対象では 432 ページ中 150 ページのバイトが動く (決定 4)。逃げ道は `--full`。**いま鍵に足すと M3-a の
台帳バイト比較ハーネスが動く**ので、写像が「誰かが渡すファイル」から「製品が作る物」になり
**鍵に入れる identity を持つ M5** で直す。

**時間**【実測。方式比較ではない】 — フル生成 432 モジュールは cold 21.53 s / warm 9.79 s (同じ仕事が
page cache で 2 倍動く)、無変更 0.30〜0.40 s、本物の移動 (35 抽出 / 36 ページ) 5.80 s、
rev だけ変更 (433 ページ再レンダ・Lean 起動 0 回) 0.65 s。
**常駐が 2 回目の要求を受けた初めての実測**が③にある — round 1 は 5 モジュールで 3.969 s (import 込み)、
**round 2 は 30 モジュールで 0.765 s** (import 無し)。**単発との A/B ではないので倍率は書けない**
(`build` に `--extractor` の継ぎ目が残っているのでそのまま測れる)。

### M5-a の結果 — V1 は成立した (依存写像を環境走査で作る)【すべて実測 2026-08-15】

**計画でいちばん否定されうる仮説が持ちこたえた。** `extractor/Extract.lean` に `--link-index <path>` を足し
(+133 行)、**抽出のために既にロードしている環境**を走査して `.lidx` を書く。入れる集合は doc-gen4 の選択を
そのまま写した 3 つの述語 (blacklist + 再帰子を落とす / private 名を落とす / **モジュールは
`env.const2ModIdx`**)。走査順は `header.moduleNames` にしたので出力は決定的 —
**5 回走らせて 8,465,776 B が 5/5 バイト一致**。

| | bmp 由来 (参照) | 環境走査 |
|---|---:|---:|
| 宣言 | 258,760 | 255,975 |
| `@` 節 | 6,115 | 6,021 |

**両方にありモジュールも一致 255,205 / モジュールが食い違う 0 件 / A のみ 3,555 / B のみ 770。**
**決着はバイトで付けた**【実測】 — 同じ IR・同じ `--source-url` で **`.lidx` だけ振って 438/438 一致**。
**比較に検出力があることを対照で確かめた**: `--no-link-index` は **150/432 ページ**を動かし (決定 4 の 150 と
一致)、`.lidx` でしか解決できない名前を 1 つ偽モジュールに向けると **1 ページ / 58 バイト**動く。

#### 参照の 258,760 は「この対象の写像」ではない【実測。引用するときは分母を書くこと】

差の 99.3% は方式ではなく**参照側の範囲と鮮度**だった。A のみ 3,555 のうち **3,523 は `Lake.*`**
(doc-gen4 が走った環境には Lake が入り、対象の import クロージャには入らない)、B のみ 770 のうち
**768 は対象パッケージの現行モジュール** (bmp を載せた doc 木は `InformationTheory` が **348 ページしかない** —
現行 432 に対して 84 足りず、CLAUDE.md の「フルビルドは 42% で打ち切っている」と整合する)。
→ **V1 の判定は集合の総数ではなく、共有スコープの残差とバイトで行うのが正しい。**

**唯一の方式差は 2 件**【実測】 — `Qq.Impl.mkLambdaQ` / `Qq.Impl.withLetHave`。**doc-gen4 の意味解析が落ちる
宣言**で (同じ 2 モジュールを抽出器に食わせると `failed 2` で名前まで一致する)、doc-gen4 は WARNING を出して
出力から落とすが**環境走査は「解析が通るか」を知らない**ので拾う。向きは「リンクが増える」側で、上流サイトへ
リンクすると**ページはあるがアンカーが無い**リンクになりうる (この対象では 0 バイト)。**V1 の否定ではないが、
第 2 の対象で再測定する項目。** 塞ぐには写像を作る側で解析を走らせるしかなく、それは写像が避けている仕事そのもの。

**バイト一致が直接押さえているのは 229 項**【実測】 — サイト 435 HTML の href が指すアンカー 5,392 種のうち、
**`.lidx` でしか解決できないのは 229 種**。B のみの 770 件は 768 件が出現するが、対象パッケージ自身の名前なので
レンダラは IR 側で解決していて `.lidx` の項は参照されない。**255,205 の一致は集合レベルの主張**であって、
バイトが押さえているのは 229 項。**両方の数字を書くこと。**

**費用**【実測、warm。cold と分けて記録】 — 環境走査は **0.899〜0.910 s** (7 回、median 0.908、cold 1.61 s)、
製品構成の抽出器プロセス全体は warm 7.12〜7.17 s なので**限界費用 +14.6%**。要点は
**環境ロード (warm 2.4 s / cold 13 s) を共有していること** — 単独プロセスで作れば cold 15 秒級になる。
段階 1 の「490,171 定数の走査 = 0.62 秒」は**やっている仕事が違う**ので外挿ではなく新しい実測
(blacklist 判定と 8.5 MB の書き出し込み、走査した定数 490,613)。**IR は 1 バイトも動かない** (436/436)。

**残っている配線**: `lean-doc extract` / `build` からはまだ `--link-index` を渡せない。
レンダラが読む形式は変わらないので**差し替えは供給側だけで閉じる**という決定 4 の前提は保たれている。

### M5-b の結果 — 第 2 の対象 (合成) でゲート B【すべて実測 2026-08-15】

生成器 `tools/make-target2.sh` (422)、ゲート `tools/target2-gate.sh` (357)、境界値の読み手
`tools/target2-boundary.ts` (291、**出たバイトから読む**)。**木ではなく生成器を置いた** — `/private/tmp` は
揮発する (このセッションで実際にクローンが壊れた)。第 2 の対象は**モジュール 13 / ライブラリ 2**
(`[[lean_lib]]` 2 ブロック = 対象 1 と違う形)、依存は同じ Mathlib rev を `cp -Rc` で複製 (23.3 s)、
**HEAD は決定的** (commit の日付を固定。rev はバイトに入るので動くと過去の数字と比較不能になる)。

**ゲート 1 = doc-gen4 の木ゼロで `build` 一発**【実測】 — `--lib` も `--source-url` も **`--link-index` も
渡さない**。lakefile から 2 ライブラリを拾い、git から URL を導出し、**写像を自分で作り**
(1,692,605 B / 宣言 57,039)、サイト 19 ファイルが `lean-doc site` の木と **19/19 バイト一致**。
**これが V1 が本当に効いていることの唯一の証拠** — 対象 1 には 736 MB の doc-gen4 木があるので写像を外から
渡せてしまう。ゲート 2 = 2 回作って **19/19・IR 15/15・`.lidx` バイト一致**。ゲート 3 = 無変更で
**0.048 s / 1 バイトも動かず / サーバも起動しない**。ゲート 4 = 本物の移動 + `lake build` →
**rounds 2 / staleFound 2 / 写像 delta 6 / 14 ページ**、続けてもう 1 回で **0 changed**、
**編集後ソースからの from-scratch と 20/20・IR 16/16・台帳も `.lidx` もバイト一致**。

#### 境界値 6 項目 — 計画が「第 2 の対象では出うる」と書いたものを実際に出した【すべて実測】

| 入れたもの | 出た挙動 | 落ちたか |
|---|---|---|
| fenced code 中の **NUL** (MD4Lean は SIGSEGV) | ページは出るが **U+FFFD に加えて生の NUL が 1 バイト残る** | **欠陥 (B)** → 下記 |
| 本文行の無い **GFM テーブル** (MD4Lean は SIGABRT) | `<tbody></tbody>` = 空 body。意図どおり | 落ちない |
| **BMP 外の宣言名** (U1) | `name-map.json` / `declaration-data.bmp` が **UTF-16 code unit 順**。Rust の既定なら逆になる組を選んである | **U1 が守られていることの初めての実バイト証拠** |
| 見出し中の **U+2B96** (登録済み乖離 3) | heading id が `Head⮖ing` = UnicodeBasic は区切らない。V8 の表なら分割されていた | **`id=` と `href=#` の両方に出た** |
| **`«…»` を要するモジュール名** | **最初は抽出器が死んだ** → 欠陥 (A)、修正済み | **落ちた** |
| **`_private.` 名** | ページ・`.lidx`・`name-map.json` に 0 件 (決定 5 の踏襲) | 落ちない |
| code span 中の **U+088F** (V6) | delta 側 (**両表の和**) は分割し、レンダラ側 (UnicodeBasic) は分割しない | **和にしたことが両方向で見えた** |

**未達が 1 件**【正直に記録する】 — 「**同名宣言を複数モジュールが持つ形**」(対象 1 では 25 件) は**再現できなかった**。
equation lemma を強制したが blacklist で IR に出ず、IR の 18 宣言で >1 モジュールは 0 件。**この項目は測れていない。**

#### 欠陥 (A) — `«…»` モジュール名で抽出器が死ぬ。修正済み【実測】

M5-a は「静かに壊れる」と書いたが**静かではなかった** — `import failed, trying to import module with anonymous
name` で exit 1。機構: `lean-doc modules` はソースの glob なので `Alpha/Odd-Name.lean` → `Alpha.Odd-Name` を出すが、
抽出器の `String.toName` は**パーサ**で、成分は識別子か `«…»` か数値でなければならない → **行全体が
`Name.anonymous`** になる。**正しい綴りは 1 つではなく 2 つ**で、どちらが正典かは選択ではない:
**名前** `Alpha.«Odd-Name»` (抽出器が `index.json` に書く綴り。`merge --modules` が食い違いを exit 3 で拒否する相手) と
**パス** `Alpha/Odd-Name` (olean の位置であり doc-gen4 がページパスを組む形)。
→ `crates/lean-doc-ir/src/name.rs` (258) に分け、パスを作る 6 箇所に通した。
**対象 1 の 432 名はすべて素の識別子なので恒等** — `site` を `w7h/base-ir` に回して `m2/gate/ref-site` と
**438 中 437 一致 (差分 1 = 登録済み乖離のみ)** = M2 のゲートの数字と完全に同じ。
**残っている乖離 (未修正、登録)**: `.lidx` は今もモジュール名を**非エスケープ**で書く。href は同じパスに解決するので
この対象ではバイトに出ないが、**ルックアップ鍵としては別物**なので `«…»` を含む**依存側**モジュールを docstring から
名指しすると解決が外れうる。**target2 にそれを名指しする docstring は無いので測っていない。**

#### `renderKey` に写像を入れた — M4-d が残した穴を塞いだ【実測】

identity は**写像ファイルの SHA-256** (パスでもサイズでもない — レンダラが読むのはバイト)。**比較は 2 回**:
①`detect` (run の頭) が「誰かが別の写像を渡した」を、②ラウンド終了後が「**この run 自身が書き換えた**」を捕まえる。
**頭でしか見ないと**、台帳に新しい写像が記録され次の run は新 vs 新を比べて何も見つけず、**陳腐化が恒久的かつ無音**になる。
写像が無ければ鍵は不在で、`KeySet::diff` は和集合なので不在↔存在は変化と数える (うるさい向き)。
**台帳ハーネスは動かなかった** — `tools/ledger-{reference,compare}.sh` は **78/78 IDENTICAL**。設計どおり
(`linkIndex` は写像を名指したときだけ現れる鍵で、12 シナリオはどれも渡さない) だが、**裏返せばハーネスの
新鍵カバレッジはゼロ**。`tests/build.rs` に 2 本足して埋めた。
**ゲート 4 で発火した検出が動かしたページのバイトは、from-scratch と同じだった** — つまりあの run が示したのは
**検出が効くこと**であって**バイトが動くこと**ではない。バイトが動く証拠は対象 1 の **150/432**【実測、決定 4】のほう。

---

## 8. 実装中に実測すること (検証項目)

| # | 問い | いつ | 否定されたら |
|---|---|---|---|
| V1 | 環境走査で作った依存写像が `declaration-data.bmp` 由来と同じ結果を出すか | **M5-a で成立【実測 2026-08-15】** → §7 | (否定されていたら M5 の設計をやり直しだった。代替は「依存の olean から作る」) |
| V2 | `contentHash` キャッシュを残る IR 全読み 5 回に当てたときの取り分 | M3 の後 | 取り分が小さければ入れない (構造は残す) |
| V3 | Rust 版のフル生成・増分の実時間 (TS 版と**同じセッション・同じ暖機状態**で) | M1〜M3 の各ゲート後 | — (数字を取るだけ。**速度は移設の目的ではない**) |
| V4 | 公開サイトがバージョン別アーカイブを持たないという**仮定** | M5 | 持つなら公開サイト経路が復活する |
| V5 | 第 2 の対象で `lake build` と同じジョブに置いたときの CI 時間 | M6 | — |
| V6 | `autolinkTokens` の分割を UnicodeBasic に替えた件が**再生成集合を過少にしないか** | **M2-b で是正済み**。残るのは **M5** | **和で分割する**を実装済み (下記)。残る問いは「第 2 の対象で出現するか」だけ |

**V6 の決着【実測 2026-08-12。生ログ `benchmarks/results/m2b-v6-token-separators.json`】** —
**食い違いは 4,803 で、向きが完全に一方向だった** — **UnicodeBasic の集合は V8 の真部分集合**
(V8 のみが区切る 4,803 / UnicodeBasic のみ 0、47 区間)。全部が「V8 の UCD では未割当 (`Cn` ⊂ `C`)、
UnicodeBasic では割当済み」という **UCD 版差**で、食い違いの 100% が**取りこぼす側**。
→ **`is_token_separator` = 両表の和**にした。`lean_doc_md::gc` とレンダラは 1 バイトも触らず
(あちらは doc-gen4 に合わせる義務がある)、V8 の表は delta 側の crate に生成物として置いた
(`lean-doc-global/src/v8_gc.rs`)。**6 成果物 1,877,124 B と state 841,947 B はバイト不動**。
**対象パッケージには 1 件も出現しない**【実測】 — docstring 4,909 件 / 1,634,037 コードポイントに 0 件で、
分岐台帳 `tokenSeparatorV8Only` がその事実への依存を明示している。**逆向きの分岐は作っていない**
【判断】 — 実測で空集合なので作れば恒久的に到達不能。代わりに「空である」を assert してある。
**和にしたことは mutation では守れない**【実測】 — UnicodeBasic ⊆ V8 なので「和」と「V8 のみ」は
**等価なプログラム**。第 1 項が冗長でなくなる日は、上の assert が落ちて告げる。
**V3 の書き方に注意** — CLAUDE.md「倍率は分母を明示する」。移設で速くなった分があっても、それは
**言語ではなく「やらなくてよい仕事をやめた」分**である可能性が高い。**同じ仕事をしているか**を確認する。

---

## 9. この計画が外れるとしたら

| 筋 | 外れる向き | 気づく場所 |
|---|---|---|
| **ストラングラーが成立しない** — 段の間の受け渡しがファイルで閉じておらず、1 段だけ替えられない | M1 で「レンダラだけ Rust」が組めない | M1 の最初。**成立しなければ移設の順序を組み直す**のであって、一気に移す口実にはしない |
| **UTF-16 と UTF-8 の食い違い** (→ §7 の U1 / U2) | M1・M2 の byte 一致が、**数学英数字 (U+10000 以上) を含む名前があるページ・成果物でだけ**落ちる | **一致率がほぼ 100% なのに数枚だけ落ちる**という出方をする。落ちたページの名前に BMP 外の文字が無いかを最初に見る |
| **依存写像を環境走査で作れない** (V1) | M5 | approach.md §5.3 の「2 列で足りる」は実測なので、作れないとしたら**走査で全宣言を引けない**とき |
| **byte 一致が Rust の既定と噛み合わない箇所が多すぎる** | M1・M2 で一致率が上がりきらない | §7 の 4 項目を先に潰しておく |

**撤退ラインではないもの**: 速度。移設で遅くなっても v0.1 のゲートは A も B も速度を含まない。
**遅くなったら数字を残して次に進む** — approach.md §6.4 の倍率の出所は**言語ではなく作業範囲**なので、
外側が多少遅くても全体の主張は動かない。
