# lean-doc 実装計画 — v0.1

**位置づけ**: [`approach.md`](approach.md) が**アプローチの SoT**、[`verification-log.md`](verification-log.md) が
**数字の SoT**。この文書は**実装のレベル**を書く — 何をどの順で作り、各段の合否を何で判定するか。
数字には CLAUDE.md「計測の誠実性」の 4 ラベル (実測 / 外挿 / 仮定 / 理論値) を付ける。
**最終更新**: 2026-08-11。

---

## 1. ゴールと完了条件

**遠いゴール = v0.1 — Mathlib 依存パッケージが使えるドキュメント CLI。**
検証で分かったことを製品にする段。ゲートは 2 段あり、**A は機械的に判定でき、B は判定に人が要る**。

| ゲート | 完了条件 | 判定 |
|---|---|---|
| **A: 移設** | Rust 版が対象リポジトリで **439/439 byte 一致** | `coverage.ts` (Deno、**製品外に残す**)。機械的 |
| **B: v0.1** | 対象リポジトリ**以外**の Mathlib 依存パッケージで `lean-doc build` 一発が通り、増分が効き、CI に置ける | 第 2 の対象で実走 |

**移設中の内側ループは `coverage.ts` ではなく「TS 版の出力との byte 差分」**を使う。
プロトタイプは doc-gen4 に対して 99.506% を出しているので**それに byte 一致すればゲート A は
構成上通る**し、差分は**パーセンテージではなく壊れたファイル名と食い違いバイト位置**を返す。
ハーネス: `tools/render-reference.sh` (参照生成) / `tools/render-compare.sh` (比較)。
**参照は 432 ページ / 29 MB、生成 1.02 秒**【実測、warm】。

**ゲート A は実走で通過済み (M1-d3、2026-08-11)** — **再現率 99.5% (21,919,956 / 22,028,728 B)、
304/348 ページ byte 一致、不足 108,772 B は全部 rev**【実測。既知の 99.506% を再現】。
**doc-gen4 の参照木 (736 MB / 6,080 ページ) は対象リポジトリの `.lake/build/doc` に健在**で、
作り直すと約 9 時間かかるので**消さないこと**。

**A は B の必要条件であって十分条件ではない。** approach.md §9 のとおり
**byte 再現率はオラクルであって製品目標ではない** — doc-gen4 と同じバイトを出すことは
「移設で何も壊していない」を安く判定するための道具で、製品の価値ではない。
A を通した時点で価値のあるものは何も増えていない、という認識を持って B まで走る。

---

## 2. Context — 何が既にあり、何が無いか

検証段階 1〜8 で **動くパイプライン一式が `experiments/` に散らばっている**。「検証が終わった」の実体はこれ。
**製品ツリー `crates/` は M0 で作り、M1 で IR リーダ・md4c・レンダラが入った** (→ §7)。

- **抽出器 (Lean)** — 製品でもこのまま使う。移設ではなく**移動** (M4)
- **外側 (TS + シェル)** — Rust に移設する。残りは全域成果物・増分・パイプライン (M2〜M4)
- **受け入れオラクル (Deno)** — **製品外に残す**。Rust 版を採点する側なので同じ言語で書き直すと
  「両方同じ間違いをする」経路ができる

`experiments/stageN/` は使い捨てで**後の段階で書き直してよい**規約だったので、製品ツリーは
既存を壊さず**別に作った** (数字の再現性のため experiments は凍結する)。

---

## 3. Approach

> **一気に移さない。パイプラインの 1 段だけを Rust に差し替え、残りは TS のまま、
> 毎回オラクルを緑にしてから次の段へ進む。**

段の間の受け渡しが**すべてファイル** (IR / 台帳 / HTML / 全域成果物) なので、
1 段だけ言語を替えても他段は気づかない。これが成立するのは検証段階が**ファイル境界で切れている**から。

**なぜこの形か。** 3,900 行を一気に移すと 439/439 が出なくなったとき**どの段が原因か分からない**。
byte 一致は「一致 / 不一致」の 1 ビットしか返さないので、犯人の切り分けを**移設の順序に埋め込む**しかない。
これは新しい発明ではなく、**増分 4〜7 が実際に成功したやり方**と同じ (1 層ずつ足して毎回 byte 一致を確認した)。

**構造上の設計判断を 1 つだけ先に入れる**: **IR へのアクセスを 1 つの抽象に集約する。**
残る IR 全読みは 5 回 (ownership・merge ×2・impact・render) で、Mathlib 規模なら
**1 回 3.18 秒 = 5 回で約 15.9 秒**【実測 → approach.md §5.6】。7h が 1 回ぶんを `contentHash` キャッシュで
消した実績があるので、**キャッシュ層を後から 5 箇所に足すのではなく、最初から IR ローダ 1 枚の裏に置く**。
**v0.1 のゲートには入れない** — 性能であって正しさではないので、A を通した後に有効化する。
入れるのは**構造**であって実装ではない。

**7h が既に規律を持っている**【実測 → `stage7h/global.ts:145-146`】 — `STATE_VERSION` /
`STATE_DERIVATION` は「導出規則が変わったら文字列を変えて全ミスさせる」ための鍵で、
`oracle.sh` の 7 番目の状態 (**stale-state**) がそれをテストしている。
**この「キャッシュのバージョン鍵 + それを壊すテスト」を 1 つの共通トレイトに切り出す** —
5 箇所それぞれで規律を再発明すると、腐るのは必ず後から足したほうになる。

**やらないこと**: 移設のついでの設計改良。移設中に「こう書いた方がきれい」と思っても、
**byte が変わる変更は移設と分離する** — 混ぜるとオラクルが犯人を名指しできなくなる。

---

## 4. この計画で決めること

### 決定 1 — 完了条件は 2 段 (→ §1)

`coverage.ts` は Deno のまま製品外。**採点は必ず rev 置換後の木に対して回す**
(プレースホルダのまま採点すると **−3.1103 pt** ずれる【実測】)。

**落とし穴**: `coverage.ts` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード。
`--source-url` にタグ名やブランチ名を渡す実装を作ると**採点が静かに下がる**。40 hex を渡すこと。

### 決定 2 — `contentHash` キャッシュは「構造だけ」最初から入れる (→ §3)

### 決定 3 — rev 注入は**ビルド時の文字列置換**を採る【決定】

approach.md §5.6 が「未決」としていた 2 方式のうち**配信時 (ビルド時) 置換**を採る。
置換は **0.1552 秒 / 432 ファイル・制約ゼロ**【実測 7e-rev】。JS 注入は **1 ページ最悪 0.80 ms**
【実測、段階 8】だが順序制約 + 鮮度ヘッダ + `?jump=src` のディープリンクの制約が 3 つ付き、
しかも**実測範囲は Chrome 151 のみ**で他エンジンは仮定。

**0.1552 秒のために制約を 3 つ負う理由が無い。** 増分の外側合計 1.5〜1.9 秒
【実測 → approach.md §6.5】に対して
置換は 10% で、rev が変わるたびに 432 ファイルを舐めても増分の性質は変わらない
(**再生成集合は 0 のまま** — 置換は IR にもレンダラ出力にも触らず、**配置段でのみ起きる**)。

**この決定が守っているもの**: rev をバイトに入れない目的は「コミットごとに全ページが無効にならないこと」
であって「最終ファイルに rev が無いこと」ではない。**キャッシュされる中間物 (IR・レンダラ出力) に
rev が無ければ目的は満たされる。** 置換段の出力は配置物であってキャッシュ対象ではない。

段階 8 の JS 注入版の数字は**選択肢の保存**として検証ログに残す (静的ホスティングを使えない
配信形態が出てきたら成立する形があると分かっている、という価値)。

### 決定 4 — 依存写像はフル生成の既定経路に入れる【穴の修正】

`stage7h/run.sh` の `render()` は **`--link-index` を渡していない**【実測】 — 渡しているのは
7c/7d の byte 再現率計測のときだけで**既定経路には入っていない**。このまま移すと **docstring の
autolink が落ち**、段階 7c で写像を入れて **410,114 B ぶん byte 一致した**分【実測】が消える。
**ページ数でも測った**: 同じ IR・同じ `--source-url` で `--link-index` の有無だけを振ると
**432 ページ中 150 ページ (34.7%) のバイトが変わる**【実測 2026-08-11。`tools/render-compare.sh`】。

→ **製品では既定で渡す。** `build-link-index` も M1 で一緒に移す (§6 の勘定に入れる)。

**写像の作り方は M5 で変わるが、インタフェースは変わらない。** M1 では現行どおり
`declaration-data.bmp` から作る (対象環境には doc-gen4 の出力がある)。M5 で**環境走査から作る経路**に
差し替える (決定 5) — レンダラが読むのは `.lidx` なので、**差し替えは供給側だけで閉じる**。
この境界を M1 の時点で守っておくこと。

### 決定 5 — approach.md §8 の未解決の振り分け

| 問い | 振り分け | 理由 |
|---|---|---|
| **依存パッケージの写像の配布** | **先に決める (M5 の前提)** | 下記のとおり**公開サイト経路は使えない**と実測で判明。v0.1 の必須要件 |
| **宣言の所有モジュール** (同名 25 件) | **先に決める (M1 の前提)** | ページ配置とリンク先が変わる。ただし**現行プロトタイプが 439/439 を出している以上、規則は既に実装されている** — やるのは**明文化**であって設計ではない |
| **`_private.` 名** (実測 8 名) | **実装中に決める (M1)** | 影響が局所。byte 一致に効くので**現行の扱いを踏襲**し、変えたくなったら A を通した後 |
| **Lean のバージョン差** | **v0.1 の範囲外** | approach.md §9「あらゆる Lean バージョンへの後方互換」はやらない。v0.1 は**抽出器がビルドできた 1 バージョンで動く**でよい。IR にスキーマ版と toolchain を持つことは既に決まっている (`extractKey`) |

§8 の残り 3 つ (cold な環境ロード / 意味解析 / docstring 内リンク 0.36%) は**性能と精度の問題**で、
v0.1 のゲートではない。**cold は「doc 生成を `lake build` と同じジョブに置く」で第一の打ち手が済む** (M6 の CI テンプレ)。

#### 依存写像 — 公開サイト経路は使えない【実測 2026-08-11】

approach.md §8 は「上流の公開サイトに同じファイルがあるか / バージョン対応が取れるか」を未決としていた。**答えが出た。**

| 問い | 答え | 根拠 |
|---|---|---|
| ファイルはあるか | **ある** — `https://leanprover-community.github.io/mathlib4_docs/declarations/declaration-data.bmp` が 200 / `image/bmp` / **66,418,003 B** | **実測** (HEAD、2026-08-11) |
| バージョン対応が取れるか | **取れない** | **実測** (下記) |

公開サイトは **Lean 4.33.0 / mathlib `c3a9a08f`**【実測。前者は `index.html` の本文、後者はページ内 blob URL】。
対象リポジトリは **Lean v4.31.0 / mathlib `fabf563a7c`**【実測。`lean-toolchain` と `lake-manifest.json`】。
**2 マイナー版ずれている。** 公開サイトはバージョン別アーカイブを持たない**【仮定・未確認】**ため、
下流プロジェクトが固定した古い Mathlib に対応する写像は原理的に取れない。

**したがって写像は生成側で作る。** approach.md §5.3 が「必要な列は (名前, モジュール) の 2 列」
「`kind` は要らない」「`docLink` は (モジュール, 名前) から 100% 復元できる」と**実測で**確定させているので、
**抽出器が既にロードしている環境を走査すれば作れる**はず — 段階 1 で走査 490,171 定数 = **0.62 秒**【実測、warm】。
URL はベース URL (パッケージごとの設定) と §5.3 の相対規則から生成側で組む。

> **これは仮説であって未検証。** 「環境走査で作った写像が `declaration-data.bmp` 由来の写像
> (258,760 宣言、段階 7c) と同じ結果を出すか」は M5 で実測する。**否定されたら M5 の設計をやり直す。**

---

## 5. マイルストーン — リレーのゲート

各 M は**独立にコミットでき、オラクルで判定でき、次の M の前提になる**。
**M の途中でセッションが尽きてもよい** — ゲートが数字なので、次のセッションは「いま何段目か」を
オラクルを 1 回回せば知れる。

| M | やること | ゲート (合否の判定) |
|---|---|---|
| **M0** | 実装計画 + 製品ツリーの骨格 (Cargo ワークスペース / Lean パッケージ) + `experiments/` の凍結宣言 | ビルドが通る。オラクルは動かない |
| **M1** | **IR リーダ + レンダラ**を Rust へ (md4c を FFI で本物に置換) | **432 モジュールページが byte 一致** (全域 6 本 + 移動分は M2 なので 439 の残り 7 はここでは出ない)。**通過済み** → 下の判定手順と §7 |
| **M2** | **全域成果物 6 本**を Rust へ (`contentHash` キャッシュ込み) | 6 成果物が byte 一致、サイト 439 も byte 一致。**M2-a (ゼロから作る経路) は通過済み** → §7 |
| **M3** | **増分 4 本 + パイプライン**を Rust へ | 本物の移動と本物の削除を `lake build` ごと回して**フルビルドと byte 一致** |
| **M4** | 抽出器を製品ツリーへ + **常駐の自動起動・停止**を Rust から配線 + CLI の形を確定 | **1 コマンド**で対象リポジトリのサイトが出て 439/439 |
| **M5** | **第 2 の対象**で動かす (依存写像を生成側で作る経路の実測を含む) | 別パッケージで `lean-doc build` 一発が通る |
| **M6** | CI テンプレ + README + インストール手順 | CI で通る。`lake build` と**同じジョブ**に置く形 |

**M1 は完了** — 最も重い (レンダラ 2,227 行 + md4c FFI) ので 4 つに割った: **M1-a** IR リーダ
(schema 4、UTF-16 スパン) / **M1-b** 下回り (`escapeHtml` / `applyWsWidths` / `leanQuote` /
`stringLt`・`nameLt` / `.lidx` パーサ) / **M1-c** docstring (md4c FFI + AST → HTML + autolink 解決) /
**M1-d** ページ描画と主ループ。**移設元はすべて `stage7d/render.ts`** (行番号の対応は git が持つ)。
**結果とオラクルの所在は §7。参照は決定的**【実測 2026-08-11】 — `render.ts` を 2 回走らせて
432/432 byte 一致、マニフェストも同一で、**バイト差分をオラクルにしてよい**根拠になっている。

#### 内側ループのオラクルは M1-c 以降 1 段ゆるむ (M2 以降もこの手順で判定する)

**自前 CommonMark サブセット (TS) → 本物の md4c (Rust) は、原理的にバイト一致を保証しない。**
内側ループ (§1) は「TS 版の出力との byte 差分」なので、**不一致が出たときに Rust 側が正しい**
場合がありうる。判定手順:

1. TS 差分が `IDENTICAL` → そのまま次へ
2. 差分が出たページは **doc-gen4 の参照木** (`lean-projects/.lake/build/doc`) と突き合わせる。
   doc-gen4 は本物の md4c を使っているので、**Rust が doc-gen4 に一致して TS が外れている
   なら TS のサブセットの限界**であって Rust の欠陥ではない
3. その場合は既知の乖離としてこの節に記録し、最終判定は `coverage.ts` で行う。
   **TS に合わせて md4c を歪めない** — それは移設ではなく劣化の移植

**M1-d3 で実際に 1 回発火し、手順 3 で終わった**【実測 2026-08-11】 — `render-compare.sh` は
**431/432 で `IDENTICAL` を出さない**が、差分 1 ページは登録済みの乖離だけで、`coverage.ts` の
出力は **TS 版とパス・日付以外バイト一致** (§1 の既知値を再現)。
→ **M1 のゲートは「`IDENTICAL`」ではなく「この手順で決着したこと」で通っている。**
`render-compare.sh` には例外リストを入れていない【判断】 — 例外を持つ比較器は 2 件目の乖離を
黙って飲む。差分集合をピン留めするのは Rust 側のテスト (`tests/pages.rs`、集合ごと assert)。

**登録済みの乖離は 2 つ**【いずれも実測 2026-08-11。どちらも Rust が doc-gen4 側】:

1. **CommonMark サブセット** — 同じ 4,987 入力 (実 docstring 4,858 + 手書き 129) を TS の
   `renderDocString` と doc-gen4 の `docStringToHtml` に通すと **TS が外れるのは 41 件、
   うち実 docstring は 1 件だけ** (`InformationTheory.Shannon.TimeBandLimiting.Count` の
   module doc 1 = バッククォートの入れ子が壊れた code span)。残り 40 件は TS が「この対象には
   出現しないので実装しない」と宣言した機能 (`render.ts:1091-1096`: table / task list / image /
   hard break / entity / permissive autolink / reference link / strikethrough / backslash escape /
   CRLF / NUL)。オラクル `crates/lean-doc-md/tests/ts_docstring.rs`
2. **autolink の `inLink`** — doc-gen4 の `renderText` は `<a>` の中の code span を素通しする
   (`DocString.lean:264`) が、`render.ts:1622` は無条件に autolink する。**リンクを引かない
   状態では両者は同一バイト**なので 1 の比較では見えない。**実 docstring 4,858 件中 0 件**
   なのでゲート A のバイトは動かない。オラクル `crates/lean-doc-render/tests/docgen4_linked.rs`

#### 移設元から掘り出した落とし穴 — §7 の表に無いもの

1. **`linkIndexBytes` は UTF-16 code unit 数であってバイト数ではない** (8,494,819 ≠ 実ファイル
   8,508,273 B)【実測】。`metadata().len()` から再現しようとすると静かにずれる
2. **`knownModules` は 3 つの供給源の和集合** (`render.ts:2051-2052, 2079`: IR のモジュール名 ∪
   `known` の値 ∪ `.lidx` の `@` 節)。**`LinkIndex` 単体で autolink を解決すると取りこぼす**
   → M1-c で `NameIndexBuilder::build` が `.lidx` を**引数で要求する**形にして構造的に塞いだ。
   取りこぼしは「リンクが 1 本消える」形で出るのでテストが弱いと通る (mutation で確認済み)
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
- **M3**: モジュール一覧は**ソースの glob** で作る。`.lake/build` を走査すると孤児 olean 659 個を拾う【実測】
- **M5**: olean の内容ハッシュには**ソースの絶対パスが 429/432 モジュールで埋まっている**【実測】ので、
  CI と開発機で IR キャッシュを共有するなら鍵をパス非依存にする

---

## 6. ファイル別内訳

**移設対象は約 4,250 行 + シェル約 1,100 行**【調査で実測】。
(approach.md §5.6 の旧勘定 3,898 行は `prune-pages` 138 と `build-link-index` 213 を落としていた。修正済み。)

### 移設する (TS/シェル → Rust)

| パス | 行 | 役割 | M |
|---|---:|---|---|
| `stage7d/render.ts` | 2,227 | IR → モジュールページ HTML | M1 |
| `stage7d/build-link-index.ts` | 213 | 依存写像 (`.lidx` / `.json`) の構築 | M1 → M5 で供給側を差し替え |
| `stage7h/global.ts` | 492 | 全域成果物 6 本 + 全域写像 delta (`contentHash` キャッシュ版) | M2 |
| `stage5/ledger.ts` | 422 | `detect` — olean ハッシュ台帳の build/check、鍵の比較 | M3 |
| `stage5/ownership.ts` | 219 | `ownership` (L3-1) — 名前の所有権差分 | M3 |
| `stage5/merge-ir.ts` | 307 | `merge` — 部分 IR の畳み込み・削除・deps 再計算 | M3 |
| `stage5/impact.ts` | 231 | `impact` (L3-2) — changed set → 再生成集合 | M3 |
| `stage5/prune-pages.ts` | 138 | `prune` — 削除モジュールのページ・孤児・空ディレクトリ削除 | M3 |
| `stage7h/incremental.sh` | 441 | 増分パイプライン本体 (7 段) | M3 |
| `stage7h/run.sh` の `render()` 部 | — | フル生成の実体。**独立スクリプトは存在しない** | M3 |
| `stage7g/extract-once.sh` | 87 | 単発 / 常駐への抽出要求 + events → timings | M4 |
| `stage7g/serve-ctl.sh` | 185 | 常駐の start/request/stop/info (FIFO + holder) | M4 |

### 移動する (Lean のまま)

`stage7d/Extract.lean` (2,784) と `stage7d/build.sh` (31)。後者の **`leanc -rdynamic`** は load-bearing。
**`Extract.lean:1766` に旧セッションの scratchpad 絶対パスが `defaultIrDir` として焼かれている**【実測】 —
常に上書きされているので実害は出ていないが、**製品ツリーに移す時点で消す**。
同種のハードコードが `build-link-index.ts:42-45` と `incremental.sh:106` / `run.sh:55`
(既定 `SOURCE_URL` に 40 桁 rev 直書き) にもある。

### 製品外に残す / 破棄する

- **残す**: `stage4c/coverage.ts` (740、オラクル。**Deno のまま**)、`stage7h/oracle.sh` (157 → Rust の
  統合テストへ。7 状態 base/rerun/modified/removed/added/restored/**stale-state** を持つ)、
  診断チェッカ群 (`check-spans` / `md-diff` / `autolink-check` / `member-check` / `instance-check` / `rev-check`)
- **破棄**: `stage7e/Parse.lean` (783) + `decl-diff.ts` (253) — プレビューを持たない決定 (approach.md §9)。
  `stage4c/compare.ts` / `bytes.ts` — `coverage.ts` の前世代。
  `stage8/source-url.js` (67) — 決定 3 で採らない。**experiments には残す** (選択肢の保存)

### パイプラインの段の順序 — Rust CLI が守る制約【すべて実測】

1. **`ownership` は `merge` より前** — merge が上書きしてしまう「旧 IR の所有者」が要る
2. **`global` は `impact` より前** — 全域写像 delta (`global-set.txt`) が L3-2 の入力
3. **抽出 → ownership → merge はラウンド**。`--max-rounds` 既定 5、超過は exit 5
4. **`--render-all` (renderKey 変化) は `--mode` を上書きして `all` に落とす**
5. **空の再生成集合は render をスキップする** — 呼び手側の `if [ ${#ONLY[@]} -eq 0 ]` が唯一の砦 (→ §5)
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

---

## 7. Rust 側の構成

### crate 分割 (案)

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

- **doc-gen4 は md4c の HTML レンダラを使っていない** (`DocGen4/Output/DocString.lean:202-393`) —
  `MD4Lean.parse` で AST を取り `renderBlock` / `renderText` で**自前**に HTML を組む。Rust 側も
  同じ形。**フラグ**は `MD_DIALECT_GITHUB | MD_FLAG_LATEXMATHSPANS | MD_FLAG_NOHTML`
  (`DocString.lean:393`)
- **vendor するのは `md4c.c` / `md4c.h` だけでよい** (LICENSE 同梱) — `entity.c` は
  `md4c-html.c` 専用で、doc-gen4 は entity を**生のまま通す** (`DocString.lean:211`) ので
  実体表が要らない。`cc` crate でビルドし push API のコールバック 5 本を自前で書く
- **AST は MD4Lean の wrapper と 1:1 ではない** — **リスト項目の暗黙 `P` だけは wrapper が
  足している** (`wrapper.c:47-77`。md4c は `MD_BLOCK_LI` 直下にテキストとブロックを混ぜて流すが
  Lean の `Li` はブロックしか持てない)。doc-gen4 の `renderLi` はブロックを回すので**これは効く**
  → **Rust 側は wrapper の写経で作る** (`crates/lean-doc-md/src/parse.rs`)
- **ヘッダのレイアウトは C コンパイラに答え合わせさせる** — `csrc/layout_probe.c` が `sizeof` /
  `_Alignof` / `offsetof` / 全 enum 値 / 全フラグを吐き `tests/abi.rs` が 123 項目を突き合わせる。
  **間違ったレイアウトがたまたまリンクする**のがこの crate の最大の失敗様式なので機械で確認する
- **MD4Lean は 2 つの入力で死ぬ**: fenced code block 中の NUL は **SIGSEGV** (wrapper が
  スカラを `Array String` に押し込む、`wrapper.c:558`)、本文行の無い GFM テーブルは **SIGABRT**
  (`wrapper.c:389` の assert、md4c は本文 0 行なら `MD_BLOCK_TBODY` を出さない)。どちらも Lean 側が
  未定義動作なのでバイト一致のしようがなく、**Rust は落ちない方に倒した** (U+FFFD 置換 = CommonMark と
  `DocString.lean:208` に一致 / 空 body)。対象パッケージの docstring には**どちらも出現しない**
  【実測。4,858 件を通して確認】のでゲート A のバイトには影響しない

**FFI が消すのは自前 CommonMark サブセット (`render.ts:1079-1672` の 594 行 = レンダラの 27%) だけ**
【実測】 — **autolink 解決系 (`render.ts:925-1077` の 153 行) は残る**。後者は `nameToLink` /
`isNameLit` (Lean の `decodeNameLit` の移植) / `isLetterLike` / `autoLinkInline` / `headingId` /
`extendLink` で、**md4c の外側 = doc-gen4 の `Output/DocString.lean` の移植**なので書き直すしかない。
つまり **approach.md §5.6 の「Rust なら本物のパーサに置き換えられる」は 594 行に効き、153 行には効かない。**

### M1 の結果 — オラクルの所在【すべて実測 2026-08-11】

移設元はすべて `stage7d/render.ts`。**オラクルは 2 種類しかない** — 「TS の関数を切り出したもの」と
「doc-gen4 / MD4Lean 本体を対象環境で実走させたもの」。どちらも生成器がコミットされているので、
committed fixture (対象パッケージが無い機械でも `cargo test` が通る分) と全件の両方を再現できる。

| 段 | 移設先 | オラクル | 母数と結果 |
|---|---|---|---|
| **M1-c 前半** md4c FFI + AST | `lean-doc-md/src/parse.rs` | **MD4Lean 本体** の AST を JSON でダンプ (`tests/oracle/gen-md4lean-expected.ts --full`) | 実 docstring 重複除去 4,858 + 手書き 83 + 方言 9 → **4,947 / 4,947 一致** (MD4Lean が落ちる 3 件は除外)。fixture 533 件 / 468 KB |
| **M1-c 後半** AST → HTML | `lean-doc-md` | **doc-gen4 `docStringToHtml`** (`tests/oracle/dump-html.lean` + `gen-docgen4-expected.ts`) | **4,987 / 4,987 バイト一致**。fixture 327 件 / 226 KB |
| **M1-c 最終段** autolink 解決 | `lean-doc-render/src/autolink.rs` | **`render.ts` の `renderDocString`** に本物の IR + 本物の `.lidx` (8,508,273 B) を食わせた出力 | 実 docstring **4,857 / 4,857 一致** (除外 1 = 既知乖離、新規 0)。**副**: 参照ページ 432 枚の docstring 区間 4,909 箇所 / アンカー 5,498 本 (`tests/ref_pages.rs`) → 差分 1 = 同じ乖離。**referee**: doc-gen4 本体に `name2ModIdx` / `moduleNames` を詰めて実走 (`dump-html-linked.lean`) → **139 / 139** |
| **M1-d1** コード片 | `lean-doc-render/src/code.rs` | `render.ts` の `Renderer.fragment` | IR のスパン付きフラグメント **55,514 / 55,514 バイト一致** (`hasAnchor` 込み)。fixture 225 件 |
| **M1-d2** 宣言ページの部品 | `src/decl.rs` / `src/frame.rs` | 同上 | `declHeader` **4,750 / 4,750**、`declHtml` **4,560 / 4,560** (= 4,750 − `suppressed` 190)、frame **432 / 432**。fixture 187 + 29 |
| **M1-d3** ページ描画・主ループ・CLI | `src/page.rs` / `src/site.rs` / `lean-doc/src/main.rs` | 合成 IR に対する `render.ts` の実走 (`tests/oracle/gen-pages-expected.ts`) | **432 ページ中 431 が TS 版とバイト一致** (差分 1 = §5 の既知乖離)。ゲート A は §1 |

**doc-gen4 をオラクルにできたのは環境が要らないから** — `nameToLink?` が読むのは
`SiteContext.result` だけなので、**空の `AnalyzerResult` を渡すと全ルックアップが外れ**、
Rust 側の `NoLinks` と同じ挙動になる。referee ではその `AnalyzerResult` に `name2ModIdx` と
`moduleNames` の 2 フィールドだけ詰めれば第 1〜3 分岐が動く (`currentName := none` で第 4 分岐を
構造的に無効化)。**両仕様が一致する範囲**に絞れば doc-gen4 が審判になる — **これで `inLink` の
乖離が出た** (→ §5)。**referee は「プロトタイプが常に正しい」を仮定しないために作った。**

**autolink だけは合わせる相手が doc-gen4 ではなく `render.ts`**【判断】 — 両者の `nameToLink` は
別物 (doc-gen4 は `isPrivateName` + 自動生成 eliminator の親フォールバック + `sameEnd`、
`render.ts` は `PRIVATE_PREFIX` 前方一致 + `known`/`linkIndex` + `knownModules` + 接尾一致) で、
**439/439 を出しているのは後者**。**副オラクル (参照ページ) が要るのは「入力が両側で同じ」を
破るため** — 主オラクルの `moduleDeclNames` (接尾一致が舐める並び) だけは関数を切り出せず
`pageHtml` から式を持ち上げているので、持ち上げが間違っていれば**両側が同じ誤った入力で
一致してしまう**。参照ページは `render.ts` 全体の出力なのでそこだけは独立している。

コーパスの数字【実測】: `known` **5,296** / `.lidx` **258,760** 宣言 / `knownModules` **6,158**
(= IR の 432 ∪ `.lidx` の `@` 6,115 ∪ `known` の値) / コード片のアンカー **120,868** 本 /
`getRoot` は **5 通り** (fixture では 3 通り振る — root はリンクのバイトに入るので 1 通りだと
root を無視する実装が通る) / `suppressed` **190** (`render.ts:2042` のコメントの 186 は古い)。

**Unicode 一般カテゴリの表は UnicodeBasic から吐かせた**【実測】 — doc-gen4 は `UnicodeBasic` に
訊く。Rust の crate を持ってくると**別の UCD 版**になり、V8 の `\p{P}\p{Z}\p{C}` と UnicodeBasic は
**4,802 コードポイントで食い違う**。`dump-gc.lean` が同じビルドから範囲表を吐き `src/gc.rs`
(839 + 742 範囲) を生成する。**この crate で唯一の生成コード**で `gen-gc-table.ts --check` が
陳腐化を検出する。

**crate 境界を 2 つ動かした**【判断】 — (1) `escapeHtml` は `lean-doc-md` に置き
`lean_doc_render::escape_html` は再エクスポート。docstring レンダラも `Html.escape` を使うので、
**実装を 1 つにするなら依存の下側に置くしかない**。(2) `nameToLink?` の**第 1 分岐 (ソースパス)
だけは注入点の外** — `Foo/Bar.lean` は索引を引かず root だけで解ける (`DocString.lean:41-42`) ので、
注入点の向こうに置くと **4,987 件中 131 件が外れる**【実測】。

**移さなかったもの**【判断】 — `render.ts:2120` の flatten probe (V8 の rope flattening を誘発する
プローブで、**ソースに実際に NUL バイトが埋まっている**: byte offset 80,955)、`--limit` (線形性計測用) /
`--out` (JSONL。どの段も読まない)、`FragCounters` / `sink` (バイトに届かず、分岐構造の第 2 の定義に
なって本体とずれる)。**分岐被覆は出力側で判定する** (下記)。

#### 全件バイト一致は分岐被覆の証明ではない — M1 で得た方法論【実測。M2 以降にそのまま効く】

**実データが到達しない分岐が、どの段でも無視できない割合ある**【実測。各段の `branchTotals`】:

| 段 | 到達しない分岐 | 中身 |
|---|---:|---|
| **M1-d1** | **10 中 3** | `constSpansUnlinkable` / `constSpansViaParent` / `constSpansNameNotInRefs` が 0 = `findLinkableParent` を丸ごと落としても全件比較は緑 |
| **M1-d2** | **41 中 9** | `class` / `inductive` / `class_inductive` の宣言も `mk` 以外の ctor も ctor 無し structure も継承フィールドの `id` 分岐も field の暗黙束縛も import 0 のモジュールも実 IR に無い |
| **M1-d3** | **21 中 8** | import 0 / 要エスケープのモジュール名 / `_private.` 宣言 / **module doc と宣言の位置衝突** (= ページ順のタイブレークが効く唯一の形) / `--only` の 3 形 / `.lidx` 無し |

**mutation で裏を取ると、全件バイト比較が緑のまま通る変異が各段にある**【実測。変異はすべて
少なくとも 1 つのテストで落ちる】 — d1 は 5 件中 2 (`findLinkableParent` 削除 / kind 2 の
`hasAnchor` 抑制)、d2 は 6 件中 4 (`isDirect` 反転 / `structure_ext` 潰し / `inductive` の extra
削除 / `containedNames` の `>=` strict 化)、d3 は 6 件中 5 (例: `suppressed` をモジュール単位に
落としても実 IR の 194 メンバは全部が親と同じモジュールなのでバイトが動かない)。
**止めているのは手書きケースだけ。**

→ **M2 以降も同じ形を取る**: 到達しない分岐を `branchTotals` で数え、**全部を手書きケースで埋め**、
`the_curated_cases_cover_what_the_package_does_not` 相当のテストが「テストがこの事実に依存して
いる」ことを明示的に守る。**「全件一致したから移設できた」と書かない。**

### M2-a の結果 — 全域成果物 6 本【すべて実測 2026-08-11】

移設元は `stage7h/global.ts` の from-scratch 経路 (`--state` 無し = stage 5 の `build`)、移設先は
`crates/lean-doc-global`、ハーネスは `tools/global-{reference,compare}.sh`。**6 本すべて TS 版と
バイト一致** — bmp 1,216,017 / name-map 602,729 / navbar 57,949 / tactics 243 / references.bib 0 /
references.html 186 = **1,877,124 B**。**オラクルは 2 本立て**: 6 成果物は `global.ts build` を
**プログラムとして実走**、`ModuleFacts` (= `tokens`。M2-b の delta 用で**バイトに届かない**) は
`factsOf` を**行範囲で切り出し**、実 IR 432 モジュール分 (21,825 トークン / 818,331 B) を照合。

**分岐は 37 中 14 が実データに無い**【実測】 — U1 が効く唯一の形 (BMP 外の名前) は**宣言 4,750・
依存名 533・モジュール 432 のどれにも 0 件**で、7 箇所の `.sort()` を UTF-8 順にしても実 1.9 MB は
1 バイトも動かない。止めるのは合成 IR のケースだけ。**第 2 のオラクル (doc-gen4) は M2 では
使えない**【実測】 — 同名 4 本のうち一致するのは `references.bib` (0 B) だけで、`declaration-data.bmp`
は**別物**: 依存閉包 258,760 宣言 / `instancesFor` 有り `dependencies` 無し / `kind` が `def`・`ctor`。

**移設中に 1 箇所だけプロトタイプと違う答えを選んだ**【判断 → V6】 — `autolinkTokens` の分割は
プロトタイプが **V8 の `\p{Z}\p{C}`**、Rust が**レンダラと同じ UnicodeBasic** (`lean_doc_md::gc`)。
**両者は 4,803 コードポイントで食い違う**【実測】。**6 成果物のバイトには届かない** (トークンは
M2-b の delta にしか出ない) ので M2-a のゲートは動かないが、**delta は M3 の再生成集合の入力**なので
**取りこぼすと古いページが残り、誰も報告しない**。今の根拠は「実 docstring 3,394 件で
トークン列が完全一致」【実測】だけ。→ **V6 で検証する。**

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
| **`index.json` の書き出し** | `merge-ir.ts:243-250` はスプレッドで既存キー順 (= ソート順) を**偶然**維持している | `BTreeMap` で**明示的に**再現する |
| **`contentHash`** | Lean の `String.hash` (`lean_string_hash`、64bit) の 16 桁 hex (`Extract.lean:1786-1788`) | **Rust 側は再計算しない設計を推奨** — 抽出器が Lean のままなので**読むだけ**で足りる。再計算する設計にすると `lean_string_hash` の移植が必要になる |
| **台帳のハッシュ** | `crypto.subtle.digest("SHA-256")` / `--algorithm lake` は Lake の `<file>.olean.hash` を**読むだけ** | `sha2` crate で同値。`lake` 経路は文字列読み取りのみ |
| **equation の長さ制限** | `[...s].length < 200` = **code point 数** (`render.ts:1683`) | `chars().count()`。バイト長でも UTF-16 長でもない |
| **浮動小数** | **生成バイトに float は入らない** (`toFixed` はレポートと timings のみ) | 影響なし |

**Deno 固有 API はほぼ無い**【実測 — 移設対象 8 ファイルの `Deno.*` を全列挙した】。
`writeTextFile` / `readTextFile` / `exit` / `args` / `mkdir` などは `std::fs` + `std::env` に 1:1。
置き換えが要るのは 3 つだけ: `crypto.subtle.digest` → `sha2`、`CompressionStream("gzip")`
(**gzip サイズの計測にしか使っていないので製品では不要**)、`performance.now()` → `Instant`。

### 既に byte 一致していない箇所が 1 つある【実測】

`merge-ir.ts:222-227` が書く `deps/*.json` は**挿入順**、Lean 版 (from-scratch) は**ソート順**。
実物を比べると `Mathlib.json` は**同じ 23,137 B で内容が違う** (トップレベルのキー順と名前の順)。
**サイトのバイトには届かないので現状は許容されている** (`merge-ir.ts:41-44` がそう書いている)。

→ **Rust 版は「Lean と同じソート順で書く」ようにすればこの差を消せる。消すか放置かを M3 で決める。**
消すと「増分 IR と from-scratch IR が完全に byte 一致する」というより強い不変条件が手に入る。

---

## 8. 実装中に実測すること (検証項目)

| # | 問い | いつ | 否定されたら |
|---|---|---|---|
| V1 | 環境走査で作った依存写像が `declaration-data.bmp` 由来 (258,760 宣言) と同じ結果を出すか | M5 | M5 の設計をやり直す。公開サイト経路が使えない以上、代替は「依存の olean から作る」しかない |
| V2 | `contentHash` キャッシュを残る IR 全読み 5 回に当てたときの取り分 | M3 の後 | 取り分が小さければ入れない (構造は残す) |
| V3 | Rust 版のフル生成・増分の実時間 (TS 版と**同じセッション・同じ暖機状態**で) | M1〜M3 の各ゲート後 | — (数字を取るだけ。**速度は移設の目的ではない**) |
| V4 | 公開サイトがバージョン別アーカイブを持たないという**仮定** | M5 | 持つなら公開サイト経路が復活する |
| V5 | 第 2 の対象で `lake build` と同じジョブに置いたときの CI 時間 | M6 | — |
| V6 | `autolinkTokens` の分割を UnicodeBasic に替えた件 (→ §7 M2-a) が**再生成集合を過少にしないか**。V8 と食い違う 4,803 コードポイントが code span 内に出たとき、Rust が**トークンを取りこぼす**側に倒れないか | **M2-b** (delta を作る段) と **M5** (第 2 の対象) | **両方の集合の和で分割する** — トークンは delta の**フィルタ**なので、多い側に倒すと過剰再生成 (性能の損)、少ない側に倒すと**古いページが黙って残る** (正しさの損)。過剰側を採る |

**V3 の書き方に注意** — CLAUDE.md「倍率は分母を明示する」。移設で速くなった分があっても、
それは**言語ではなく「やらなくてよい仕事をやめた」分**である可能性が高い (approach.md の
「Rust だから速い」と書かない規則)。**同じ仕事をしているか**を確認してから比較する。

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
