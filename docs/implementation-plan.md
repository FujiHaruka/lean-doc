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
| **M3** | **増分 4 本 + パイプライン**を Rust へ | 本物の移動と本物の削除を `lake build` ごと回して**フルビルドと byte 一致**。**M3-a (`detect`) / M3-b (`ownership` + `merge`) / M3-c (`impact` + `prune`) / M3-d1 (`site`) / M3-d2 (`incremental` + `modules`) / M3-d2b (`merge --modules`) / M3-d3 (差分ハーネス) は通過済み** → §7。残りは **M3-d4** (クローンでのゲート) |
| **M4** | 抽出器を製品ツリーへ + **常駐の自動起動・停止**を Rust から配線 + CLI の形を確定 | **1 コマンド**で対象リポジトリのサイトが出て 439/439 |
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
`render()` 部 (3 行) → M3-d1、`incremental.sh` (441) + `run.sh` の `modlist()` (5 行) → M3-d2。

| 未移設のパス | 行 | 役割 | M |
|---|---:|---|---|
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
「doc-gen4 / MD4Lean 本体を対象環境で実走させたもの」。オラクルは `crates/lean-doc-{md,render}/tests/`、
生成器は各 `tests/oracle/` (コミット済みなので committed fixture と全件の両方を再現できる)。
母数と結果: md4c AST **4,947 / 4,947**、AST → HTML **4,987 / 4,987**、autolink **4,857 / 4,857**
(+ referee 139 / 139、参照ページ 432 枚の docstring 区間 4,909 箇所 / アンカー 5,498 本)、
コード片 **55,514 / 55,514**、`declHeader` **4,750 / 4,750** / `declHtml` **4,560 / 4,560** /
frame **432 / 432**、ページ **431 / 432** (差分 1 = §5 の登録済み乖離)。
**doc-gen4 をオラクルにできたのは環境が要らないから** — `nameToLink?` が読むのは `SiteContext.result`
だけなので、**空の `AnalyzerResult` を渡すと全ルックアップが外れ** Rust の `NoLinks` と一致し、referee
では `name2ModIdx` / `moduleNames` の 2 フィールドだけ詰めれば第 1〜3 分岐が動く。**両仕様が一致する
範囲**に絞れば doc-gen4 が審判になり、**これで `inLink` の乖離が出た** (→ §5)。**referee は
「プロトタイプが常に正しい」を仮定しないために作った** (M5 でも同じ手を使う)。ただし **autolink だけは
合わせる相手が `render.ts`**【判断】 — `nameToLink` は両者別物で **439/439 を出しているのは後者**。
**副オラクル (参照ページ) が要るのは「入力が両側で同じ」を破るため** — 主オラクルの `moduleDeclNames`
だけは関数を切り出せず `pageHtml` から式を持ち上げており、間違っていれば**両側が同じ誤った入力で
一致してしまう**。コーパスの数字【実測】: `known` **5,296** / `.lidx` **258,760** 宣言 /
`knownModules` **6,158** / コード片のアンカー **120,868** 本 / `getRoot` **5 通り** / `suppressed` **190**。
**Unicode 一般カテゴリの表は UnicodeBasic から吐かせた**【実測】 — Rust の crate を持ってくると
**別の UCD 版**になり V8 の `\p{P}\p{Z}\p{C}` と **4,802 コードポイントで食い違う**。`dump-gc.lean` が
同じビルドから範囲表を吐き `src/gc.rs` を生成する (`gen-gc-table.ts --check` が陳腐化を検出)。
**crate 境界は 2 つ動かした**【判断】 — `escapeHtml` は `lean-doc-md` に置いて再エクスポート、
`nameToLink?` の**第 1 分岐 (ソースパス) だけは注入点の外** (root だけで解けるので、向こうに置くと
**4,987 件中 131 件が外れる**【実測】)。

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

移設元は `stage7h/global.ts`、移設先は `crates/lean-doc-global`、ハーネスは
`tools/global-{reference,compare}.sh`。**6 成果物すべて TS 版とバイト一致 (計 1,877,124 B)**。
M2-b で差し替えたのは `facts_for` 1 箇所だけで、M2-a が置いた継ぎ目が意図どおり効いた。
**`global-state.json` もプロトタイプとバイト一致**させた (Rust **841,947 B** / TS 841,949 B、差は
`derivation` の 1 文字列 **26 B ↔ 24 B** のみ)。`ModuleFacts::tokens` は 6 成果物のどのバイトにも
届かないので**全件比較では原理的に見えなかった**が、state を合わせたことで **432 モジュール分の facts
(imports / tactics / decls / instances / tokens) が丸ごとバイト照合された**。`STATE_DERIVATION` は §6 の
`extractKey` / `renderKey` と同じ規律で**敢えてプロトタイプと違う文字列** — TS が書いた state は
Rust から見て異物であるべき。**7 状態オラクル** (base / rerun / modified / removed / added / restored /
**stale-state**) は Rust の統合テストに移し、合成 IR と実 IR の両方で回す。実 IR の hit/miss は
0/432 → 432/0 → 431/1 → 430/0 → 430/1 → 429/3 → 0/432 で、**全 14 ステップで 6 成果物がバイト一致**。
**delta もプロトタイプと突き合わせた** — 参照 `name-map.json` に「移動 3 + before だけ 2 + after だけ 1」を
仕込んで両実装に食わせ、`changedNames` **6** / `affected` **14** が一致し **`--print-set` はバイト一致**
(610 B / 14 行)。変化ゼロなら**両者とも 0 B** (改行なし) — この空ファイルは M3 で `--only-from` に
そのまま渡る形なので、レンダラ側の「空集合と未指定を型で区別する」(→ §5) と対になっている。
**分岐と mutation は上表** (6 成果物の全件バイト比較で捕まった変異は 0 件)。**第 2 のオラクル (doc-gen4) は
M2 では使えない**【実測】 — 同名 4 本のうち一致するのは `references.bib` (0 B) だけで、`declaration-data.bmp`
は**別物** (依存閉包 258,760 宣言 / `instancesFor` 有り `dependencies` 無し / `kind` が `def`・`ctor`)。
**M2 のゲート = サイト 438 中 437 が byte 一致**【実測 2026-08-12】 — `lean-doc render` と
`lean-doc global` を**同じ木に出して**プロトタイプの木と比較 (参照 = `m1/ref-pages` 432 +
`m2/ref-global` 6)。**差分 1 = §5 の登録済み乖離のみ**で、両コマンドの書き込みは 1 ファイルも衝突しない。
**`w7h/base-pages` は参照に使えない**【実測】 — `run.sh` の `render()` が `--link-index` を渡していない
(→ 決定 4) ので、製品の既定とは別物。

### M3-a の結果 — `detect` (olean ハッシュ台帳)【すべて実測 2026-08-12】

`stage5/ledger.ts` → `lean-doc-incr/src/{ledger,detect}.rs`、CLI `lean-doc ledger build|check|touch`、
ハーネス `tools/ledger-{reference,compare}.sh`。**12 シナリオを 1 箇所で定義して両実装に回す**【判断、以降の
段でも同じ】 — 一致すべきはファイルではなく**問いへの答え**なので、問いを両側で書くと誰も意図していない
2 つを比べることになる。**78 ファイルすべて一致**: 台帳 8 本は**意図的に変えた 2 文字列 (計 15 B) だけ**が
違い (鍵名でアンカーして置換すれば 1 バイト差なし)、`check` の出力 36 本は全部バイト一致、`--concurrency 8`
の台帳は `--concurrency 1` と完全一致 = **バイトはスケジューリングに依存しない**。
**隣のパッケージは「対象が持たない形」を安く供給する**【判断】 — 対象の 432 モジュールは `.olean` 1 本ずつで
**module system の 3 ファイル形が構造的に無い**ため、Mathlib 8 モジュール (24 ファイル / 10,323,184 B) を
参照集合に足して埋めた。
**速度**【実測 2026-08-12。Apple M1 / 16 GB / 並列度 1 / 432 モジュール 237,909,832 B / 同一セッション /
各 8 回。生ログ `benchmarks/results/m3a-ledger-*.jsonl`】 — warm 中央値 Rust **0.1424 s** / TS **0.2468 s**、
cold 0.2883 / 0.3797 s。**分母は同一の仕事**だが**言語の速さの話にはできない** — 内側の `hashSeconds` は
両者ともハードウェア SHA-256 に届いていて、差の出所は読み取り経路 (peak RSS 147 MB 対 8.2 MB) と
プロセス起動 (0.005 s 対 0.037 s)。

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

### M3-b の結果 — `ownership` + `merge`【すべて実測 2026-08-12】

`stage5/{ownership.ts,merge-ir.ts}` → `lean-doc-incr/src/{ownership,merge}.rs`、CLI
`lean-doc ownership|merge` (+ `merge --verify A --against B`)、ハーネス `tools/merge-{reference,compare}.sh`
は **1 ラウンド = 抽出 → ownership → merge なので 2 段を 1 本で回す**。編集は**対象リポジトリではなく IR に
注入する** (fixture は base IR 自身から組んだ部分抽出木、`contentHash` は捏造 = 等値比較しかされない)。
**母数 4,051 ファイル / 一致 3,986 / REORDERED 65 / 差分 0 / 欠落 0**。REORDERED は下記の意図した乖離のみ
(`deps/*.json` 48 + `index.json` 17)。**比較器に例外リストは入れていない**【判断、以降の段でも同じ】 —
代わりに「同じ mapping で違うキー順」を**ファイル名を持たない規則**で分類する。in-process 側
(`tests/merge.rs`) は**モジュールファイル 3,890 本を「コピー元のバイト」と直接照合**する。
**隣のパッケージの手はここでは使えない**【実測】 — 必要な「IR の `refs` に BMP 外の名前」は対象 IR に 0 件
(依存閉包の `.lidx` には 37 行)、かつ**依存パッケージの IR 木が存在しない**ため。手書きケースのまま。
**ラウンドと `--max-rounds` / exit 5 は M3-d に残す** — ループは ownership と merge の間に抽出器が要るので
段の中に置けない (M3-a が glob を `detect` の担当外としたのと同じ扱い)。**`--modules` はここでは再現しな
かった**【実測】 — `merge-ir.ts:29,40` が usage に出しながら `opt("--modules")` を一度も読まない**未実装の
フラグ**だったため。**M3-d2b でこちらの設計として足した** (→ M3-d2b の節)。

#### 「既に byte 一致していない箇所」の決着 — **消した**【実測 2026-08-12】

`merge-ir.ts:222-227` の `deps/*.json` は**挿入順**、Lean 版 (from-scratch) は `Json.mkObj` の**ソート順**
だった。**Rust 版は Lean 順で書く** — `deps/*.json` も `index.json` の `dependencyMaps` も**キーは
`Json.mkObj` のアルファベット順、配列はコードポイント順** (`Extract.lean:2050-2057`)。ここだけプロトタイプ
(UTF-16 `.sort()`) と分かれる。**推測ではなく実物** (`w7h/base-ir/deps/*.json`) を開いて決めた
(`merge-ir.ts:44` の「Lean の HashMap 由来なので外では再現できない」は**事実として誤り**だった【実測】。
**移設元のコメントを根拠に使うときは実物で裏を取る**)。
**結果**: **何も変わらない再抽出を合流した木は from-scratch の木と 436/436 バイト一致**。入れないと合流後の
index は from-scratch と同じ 88,541 B・同じデータなのにバイトが違うままだった。**merge が書くものはもう
1 つもプロトタイプの順序ではない。** **サイトのバイトには届かない**【実測】 — キー順だけが違う 2 本の IR を
`render` + `global` に通して **438/438 バイト一致 (31,617,612 B)**。理屈 (レンダラは `.lidx` を読む /
`global.ts:319` が読み直しに `Object.keys(deps).sort()` を噛ませる) もあるが、**生成バイトの主張なので実測
した**。合流後 IR から作ったサイトは **M2 ゲートの参照木と 438 中 437 一致** (差分 1 = §5 の登録済み乖離)。
**乖離は `STATE_DERIVATION` / V6 と同じ規律で登録する** — `tests/merge.rs` が差分集合 34 件 (deps 25 +
index 9) を `assert_eq!` で固定し、**プロトタイプと違うことと、独立に書いた Lean 順 writer と同じことの
両方**を assert する。片方だけだと「壊れた」と「意図どおり」が区別できない。

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

移設元は `stage7h/run.sh:78-80` の `render()` **3 行** (`render.ts` → `global.ts build` を同じ IR・同じ
出力先に。**独立したスクリプトは存在しない**)、移設先 `lean-doc site`、ハーネス `tools/site-compare.sh`。
**母数 438 / 一致 437 / 差分 1 / 欠落 0 / 余分 0。差分 1 は §5 の登録済み乖離のみ** (`…/Count.html`)。
**合成が何も足していない**ことも確かめた — `render` + `global` を別プロセスで回した木、M2 ゲートが
2 コマンドで作った木、どちらとも **438/438 一致**。
**決定 4 を既定ではなくフラグの形で塞いだ**【判断】 — `--link-index` / `--no-link-index` の**どちらか必須**
(拒否文言は `render` と定数を共有)。**ガードが効いていることを実測した** — 同じ IR・同じ `--source-url` で
`.lidx` の有無だけを振ると **438 中 150 が差分** (= 432 ページ中 150。全域 6 本は不動)。**移設元はこれを
渡していない**ので、そのまま写せば 3 度目を踏んでいた。`site` が呼ぶ側の実在フラグ (`--only` /
`--only-from` / `--before` / `--print-set` / `--delta-json`) は「unknown argument」にせず**名指しで断る**
【判断】 — 呼び手に要る答えは「綴り間違い」ではなく「なぜここに無いか」。
**フル生成と増分で 2 段の順序が逆なのは矛盾ではない**【実測】 — 制約 2 (§6) は**全域 delta が再生成集合の
半分だから**であって成果物の依存ではなく、delta の無いフル生成では順序は自由 (両段の書き込みは 1 ファイルも
衝突しない)。`run.sh` は render → global、`incremental.sh` は global (step 6) → render (step 7)。**`site()` の
docstring に明記した** (M3-d2 が「render が先」と読まないように)。

### M3-d2 の結果 — `incremental` + `modules` (並べる人)【すべて実測 2026-08-12】

`stage7h/incremental.sh` + `run.sh:82-86` の `modlist()` → `crates/lean-doc/src/pipeline.rs` (bin crate。
render + global + incr の全部に依存するのはここだけ)、CLI `lean-doc incremental|modules`。**段はすべて
ライブラリ呼び出しで、外部プロセスは抽出器だけ**。
**抽出器の継ぎ目** — `--extractor <program>` は**必須で既定値なし**【判断】。起動形は `<program>
[<extractor-arg>…] --modules <round-in> --ir-dir <dir> --timings <file>` = `stage7g/extract-once.sh:24,47`
の必須引数そのもの。役目は 2 つ: ①**M4 の境界** (既定値を持たせると製品が凍結済み `experiments/` に依存
する)、②**Lean 無しでパイプラインをテストできる形** (テストは焼いた部分 IR 木を配る偽抽出器を渡す。これが
無いと全テストが Lean 依存になり事実上テストされない)。`--jobs` は受け付けない (§6 制約 6。
`--extractor-arg --jobs --extractor-arg 4` で通る)。
**ゲート = `oracle.sh` の 7 状態をパイプライン全体に対して回し、各状態でフル生成 (`lean-doc site`) と
byte 比較**【実測】。合成パッケージ (6〜7 モジュール) を source / olean / IR の 3 層で持ち、olean と IR を
**独立に**動かせる形にした (宣言の移動で referrer の olean が動かない = L3-1 が要る理由を fixture が含む)。
**サイトは 7 状態すべて全件一致** (12/12・12/12・12/12・11/11・12/12・12/12・12/12)。
**IR は M3-d2 の時点で 55 中 52 一致**【実測】 — 3 状態 (added / restored / stale-state) が `index.json` だけ
差分。`merge` が base index に無いモジュールを**末尾に push** し、from-scratch はモジュール一覧の順に並べる
ので、**モジュールが増えた木は同じエントリを違う順で持つ**。**サイトのバイトには届かない**ので気づかれない
形の乖離で、M3-b の 436/436 はモジュールの増減が無い合流だったため見えなかった。→ **M3-d2b で塞いだ
(55/55)**。下記。

**負債 8 件の決着**: ①②③ **構造で消した** — 2 つの再生成集合の合流は**メモリ上の和集合**
(`ImpactRun::summary` と `GlobalSummary::delta`) で、ファイルを経由しないので「無いファイル」という状態が
存在しない。3 本 (`global-set.txt` / `impact-set.txt` / `render-set.txt`) は診断として同名で残すが**読み戻さない**。
④ **型で閉じた** — `prune_removed(pages, remove, json)` が唯一の呼び口で、IR 木を名指す引数が無い。
⑤ **`lean-doc modules --root --lib` を新設**。⑥ ラウンド開始前に `--work` へ退避。
⑦ **`incremental` でだけ 40 桁 hex を検査** (外れたら exit 2 + 何が壊れるか)。`render` / `site` の緩さは不変【判断】。
⑧ フル生成 (`site --state`) が書き、増分が毎回更新する。docstring に前提として明記。

**プロトタイプと意図的に違えた点**: `--l3-1` / `--global` / `--count-reads` / `--module` / `--serve*` /
`--jobs` は受け付けず**名指しで断る**。`--mode` の未知値は**起動時に exit 2** (プロトタイプは選択対象が
空だと exit 0 で何もしない)。抽出器の失敗は**exit 4** (子の終了コードは文言に。5 は「収束しなかった」専用)。
`modules` の順序は**プロトタイプの `sort` と違う**【判断・実測 2026-08-12】 — `sort` は呼び手のロケールで
照合し、**同じ 432 件・同じ集合だが 163 行が別の位置に来る** (`LC_ALL=C` = code point 順と比べて。原因は
ロケール照合が `.` と大小文字を無視すること。`diff` のハンク数 22 はブロック移動を 1 と数えるので**移動量を
大幅に過小評価する** — 分母は「同じ添字で食い違う行数 / 432」で取ること)。**U1 の UTF-16 順**に固定した
(`LC_ALL=C sort` と 432/432 一致)。ロケール依存の順序は機械が変われば再現しない。
**この順序は 2 つの成果物のバイトを作る**【実測 2026-08-12】 — ①**台帳の `modules` 配列順** (同じ 432
モジュールで 2 通りの順のリストから台帳を作ると **120,103 B 同士でバイトが違う**)、②**IR の `index.json` の
`modules` 配列順** — **抽出器は渡されたリスト順をそのまま保つ** (`w7h/base-ir/index.json` はプロトタイプの
ロケール順と完全一致、code point 順とは 163 行違う)。**サイトのバイトには届かない** (`check` は再抽出集合を、
`impact` は選択をそれぞれソートする)。→ **M3-d4 のゲートは増分側と from-scratch 側に必ず同じモジュール
一覧を渡すこと。** 違うリストを渡すと、パイプラインとは無関係に `index.json` だけで落ちる。
**`--ledger` は読むだけで書き戻さない** (プロトタイプも同じ。`run.sh:167` が毎回 seed し直す) — 連続実行の
あいだ誰が台帳を更新するかは **M4**。
**逃げた mutation 2 件はどちらも等価変異**【実測】 — `--exclude` を落としても 2 巡目は lost/gained が空
(2 巡目のモジュールは olean が動いていないので宣言名が base と同じ)、削除リストを毎ラウンド渡しても両段が
「base index に居ないもの」を落とす。**「捕まらないこと」を assert で固定**した (3 巡目が可能になったら落ちる)。

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

#### M3-d3 が踏んでいない経路 — M3-d4 が要る理由【実測】

**L3-1 と L3-2 は 7 シナリオすべてで空の答えしか返していない。** `ledger touch` は台帳のエントリを
無効化するだけで **olean の内容を変えない**ので、再抽出しても IR のバイトが動かない (`irChanged` は
`self-one` / `importers-hub` / `referrers-two` で **0**)。実測値は **`staleFound` 0 / `globalStale` 0 が 7/7**。
`changedNames` が非空だったのは `removed-one` / `added-one` の 2 件 (どちらも 2 名) だが、**`affected` はそこでも 0**。
→ **ラウンドが 2 巡する経路も、2 つの再生成集合の union が両方非空になる経路も、M3-d3 では 1 度も踏んでいない。**
両実装が一致したのは「同じ空集合を出した」ことを含む。**これを踏むのが M3-d4 (クローンでの本物の移動)。**

---

## 8. 実装中に実測すること (検証項目)

| # | 問い | いつ | 否定されたら |
|---|---|---|---|
| V1 | 環境走査で作った依存写像が `declaration-data.bmp` 由来 (258,760 宣言) と同じ結果を出すか | M5 | M5 の設計をやり直す。公開サイト経路が使えない以上、代替は「依存の olean から作る」しかない |
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
