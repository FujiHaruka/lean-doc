# サイト側 JS の TypeScript 化 — ソースと成果物を分ける

`crates/litedoc4-render/assets/app.js` (917 行, 32,173 B) を **TypeScript のモジュール群**に
書き直し、**vite でビルドして 1 本の minify 済 ES module に畳む**。型は strict、
lint / format は biome、テストは vitest。

対象は**サイトが読む JS だけ**。`tools/*.ts` と `benchmarks/tools/*.ts` は Deno で走る
計測・ゲート用スクリプトで、成果物に入らない (→ 決定 5)。

---

## 1. いま何が無いか

| | 現状 |
|---|---|
| ソース | `assets/app.js` **917 行 1 ファイル**、素の ES module |
| 型 | 無し。TypeScript も JSDoc 型注釈も `@ts-check` も無し (`@param` / `@type` は 0 件) |
| lint / format | 無し。`package.json` も `tsconfig.json` も無し |
| ビルド | 無し。`include_str!("../assets/app.js")` でバイナリに焼き、そのまま配る |
| minify | 無し (gzip は GitHub Pages 側が掛ける前提 → `docs/milestone-log.md` §M8) |
| テスト | **実ブラウザのゲートだけ** (`tools/browser-gate.sh`) |

**静的検査が 1 つも無い**のが本体の欠陥。`app.js` に構文エラーを入れても
`cargo test --workspace` は緑のまま通り、CI のブラウザゲートまで落ちてこない。
「壊れていることが分かるのが最後」は、このリポジトリが他のどの層でも避けている形。

副次的に 2 つ:

- **サイズ予算をほぼ食い潰している**。`the_assets_stay_within_their_budget` の上限
  32,768 B に対し **32,173 B、残り 595 B**【実測 2026-08-19】。予算は
  「今日のバイト数ではなく丸い数字を上に置く」設計で、検索 v2 の
  18,424 → 32,173 B でその余白が消えた。次に何か足すと「読まずに上限を上げる」方に倒れる。
- **テーマの保存キーが 2 言語に重複している**。`frame.rs` の `THEME_BOOT`
  (`<head>` に埋まる同期スクリプト) と `app.js` の `THEME_KEY` が
  どちらも `"litedoc4-theme"` を字面で持ち、片方だけ直せる。

---

## 2. Approach

**「配るもの」と「書くもの」を分ける。** 今は同一ファイルなので、読みやすさと
配布サイズが正面から衝突している (コメントが 3 割を占め、それが全読者にダウンロードされる)。

```
crates/litedoc4-render/
  web/          ← 書くもの: TS のソース、テスト、package.json / tsconfig / vite / biome
  assets/       ← 配るもの (手書きのまま): style.css / favicon.svg
  build.rs      ← vite を呼ぶ。app.js は OUT_DIR に出る (git に入らない)
```

`build.rs` が `npm run build` を回し、vite が **`$OUT_DIR/app.js`** を書く。
`assets.rs` は `include_str!("../assets/app.js")` を
`include_str!(concat!(env!("OUT_DIR"), "/app.js"))` に差し替える。
**生成物はリポジトリに入らない。**

この形を選ぶ理由が Approach の中身:

1. **生成物をコミットしない。** commit された生成物は必ず腐り、腐っていないことを
   見るためのゲートが要る — その全部が消える。対価は **node が Rust のビルド依存になる**
   ことだが、**このワークスペースは `publish = false`** で crates.io に出しておらず、
   配布は `release.yml` がビルドする musl バイナリなので、**利用者は node を一切払わない**。
   払うのは「ソースから `cargo build` する人」= 開発者と CI だけで、そこには node がある。
   `docs/implementation-plan.md` の「Rust 側は `cargo build` で完結する」は
   **この決定で更新する** (予測と結果が食い違ったら結果が SoT — ここでは方針の変更なので、
   計画側を直す)。
2. **判断は 1 箇所に集める。** 「node があれば作る、無ければコミット済のを使う」という
   二経路は取らない — CLAUDE.md が言う「同じ問いに答える経路が 2 本あると片方だけ直る」。
   `build.rs` は node が無ければ**落ちる**。落ちたときのメッセージが仕事を全部する。
3. **テストはブラウザを要らない層に集中させる。** 索引のデコード・採点・絞り込み・
   ツリーの入れ子は純粋関数で、`app.js` の行数の過半を占め、かつ**今まで一度も
   単体で検査されたことがない**。ブラウザゲートは残す (置き換えではなく下に足す)。
4. **TS 側にエンコーダを書かない。** テストのオラクルは **Rust の encoder が焼いた
   バイト列**をフィクスチャとしてコミットしたもの。TS でエンコーダを書いて
   TS のデコーダを試すのは、CLAUDE.md が禁じている
   「オラクルを同じ言語・同じ設計で書き直す」そのもの (両方が同じ間違いをする)。
5. **挙動同値が条件。** この作業は**リファクタであって仕様変更ではない**。
   既存の `tools/browser-gate.sh` / `tools/search-gate.sh` / `tools/e2e-micro.sh` が
   緑のままであることが受け入れ条件で、赤くなったら TS 側を直す (ゲートを緩めない)。

---

## 3. 決定

**決定 1 — 生成物はコミットしない。`build.rs` が vite を呼び、`OUT_DIR` に出す**
【決定 2026-08-19、ユーザー判断】。
`assets/app.js` は git から**消す**。`include_str!` の引数だけが `OUT_DIR` を指すように変わる。
却下した案は 2 つ: 生成物をコミットして鮮度をゲートで見る案 (ゲートが 1 本増え、
かつ「コミットし忘れ」という新しい失敗モードを作る)、site 書き出し時にバンドルする案
(実行時依存になり、`litedoc4 build` が node を要る)。

対価と、それが安い理由:

| | |
|---|---|
| 対価 | **node が Rust のビルド依存になる**。`cargo build` は node と `web/node_modules` を要る |
| 誰が払うか | **ソースからビルドする人だけ** — 開発者と CI |
| 誰が払わないか | **litedoc4 の利用者**。`publish = false` で crates.io に出しておらず、配布は `release.yml` の musl バイナリ【実測: `.github/workflows/release.yml`】 |
| 初回のみ | `node_modules` が無ければ `build.rs` が `npm ci` を回す (ネットワークが要る) |

**決定 2 — 出力は ES module 1 本、sourcemap 無し。**
`<head>` は `<script type="module">` で読んでいるので `format: "es"`。
sourcemap を site に置くと `ASSETS` が 3 → 4 本になり、`prune.rs` /
`publish-pages.sh` / `target2-gate.sh` / `build-gate.sh` が持つ「静的資産は 3 本」の
記述が全部動く。**読者が払うバイトでもある。** ソースは公開リポジトリにあるので、
デバッグしたい人はそれを読める。

**決定 3 — minify は vite 既定の esbuild。**
terser は圧縮率で数 % 勝つが、決定 1 で生成物をコミットする以上
**出力の決定性が要件**になる。esbuild は vite に同梱で版が lockfile に落ちる。

**決定 4 — node は mise で固定する** (`mise.toml`, `node = "24.19.0"` = 現行 LTS)。
この機材では PATH 上の `node` が 2023 年の pkg 版で **SIGKILL される** (→ CLAUDE.md
「この機材の罠」)。CI は `actions/setup-node` に同じ版を渡す。

**決定 5 — biome の適用範囲は `crates/litedoc4-render/web/` に限る (初手)。**
`tools/*.ts` と `benchmarks/tools/*.ts` は **Deno スクリプト**で、`npm:` / `jsr:` 指定子を
持ち、成果物には入らない。まとめて format すると本題と無関係な巨大 diff が出る。
**P3 で差分の大きさを実測してから判断する** — 「やらない」ではなく「順序」の決定。

**決定 6 — `THEME_BOOT` も TS から生成する (P4)。**
`<head>` に埋まる同期スクリプトで、遅延する `app.js` には移せない (テーマの
ちらつきを消すのが仕事)。**site の 4 本目のファイルにはしない** — TS から
別エントリとして minify し、その文字列を Rust が `include_str!` して
`<script>…</script>` の中に**埋め込む**。これで §1 の重複が 1 箇所に集まる。

---

## 4. モジュール分割

917 行を 15 本に割る。境界は**現在のコメント見出しがすでに引いている**線に沿わせた
(`// ---- theme`, `// ---- search` …)。

| ファイル | 中身 | 元 |
|---|---|---|
| `src/types.ts` | `ModuleEntry` / `ModulesFile` / `InstancesFile` / `SearchIndex` / `Narrow` / `SearchData` | (新規) |
| `src/site.ts` | `body` / `ROOT` / `MODULE` / `url()` | 冒頭 |
| `src/data.ts` | 3 ファイルの取得と一度きりのメモ化、`searchData()` | fetching |
| `src/index-format.ts` | `readIndex` / `nameAt` / `kindAt` / `moduleAt` / `utf16Length` / スクラッチ緩衝 / `findNames` | the index format |
| `src/score.ts` | `scoreBytes` / `rank` / `NARROW_MAX` | search (採点) |
| `src/search.ts` | `search` / `searchNarrowed` | search (走査) |
| `src/theme.ts` | `initTheme` | theme |
| `src/drawer.ts` | `initDrawer` | drawer |
| `src/tree.ts` | `nest` / `treeHtml` / `initTree` | module tree |
| `src/imported-by.ts` | `initImportedBy` / `countBadge` | imported by |
| `src/instances.ts` | `initInstances` / `declItem` | instances |
| `src/result-item.ts` | `resultItem` (ドロップダウン / 検索ページ / 404 の共用) | search |
| `src/search-box.ts` | `initSearch` | search |
| `src/search-page.ts` | `initSearchPage` | search page |
| `src/not-found.ts` | `initNotFound` | not found |
| `src/sundry.ts` | `jumpToSource` / `openForPrint` | sundry |
| `src/main.ts` | 起動順だけ (`initSearchPage` が `initSearch` より先、の理由コメントを保つ) | boot |

**コメントは落とさない。** 現在の `app.js` のコメントは「なぜこの形なのか」を
書いていて (`<details>`/`<summary>` を使わない理由、astral 文字が 2 単位である理由、
narrow を捨てる閾値の根拠)、これが消えると次に触る人が同じ罠を踏む。
**minify が読者向けに落とすので、ソース側で削る理由が無くなった**のが今回の利得。

### 型で拾いたいもの

`strict` に加えて **`noUncheckedIndexedAccess`** を入れる。この木の主役は
`bytes[at]` のようなバイト走査なので、添字アクセスが `T | undefined` になるのが効く。
ただし**ホットループ (`search` / `nameAt`) では実測でコストが出る形になったら
`!` ではなく局所的な非 null 表明ヘルパで受ける** — 「型のために遅くする」は本末転倒。

---

## 5. テスト (vitest)

**ブラウザを要らない層**を単体で押さえる。オラクルは決定 4 の通り Rust 由来。

| テスト | オラクル |
|---|---|
| `readIndex` がヘッダと 2 つの小表を読む | Rust の `encode` が焼いた `search-index.bin` フィクスチャ |
| `nameAt` が全件を復元する | 同フィクスチャに対応する名前一覧 (JSON、Rust が書く) |
| `findNames` が全件を 1 走査で見つける | 同上 |
| `utf16Length` が astral を 2 と数える | `String.prototype.length` (JS 自身) — U1 の再発防止 |
| `scoreBytes` の 3 段階と順序 | 凍結した期待値 (旧 JSON 採点器と一致することは `search-gate.sh` が見ている) |
| `search` と `searchNarrowed` が同じ答えを返す | **互いに** — 1 文字ずつ打った結果と、毎回全走査した結果の一致 |
| `nest` が `A` と `A.B` の共存を壊さない | 手書き |

**フィクスチャの作り方**: `crates/litedoc4-render/web/test/fixtures/` に
`search-index.bin` と `names.json` を置き、**Rust 側のテストが同じ入力から
再生成して一致を主張する** (フィクスチャが黙って腐らないように)。
`#[ignore]` にはしない — 入力は Rust のソースに書かれた文字列なので機材ゼロ依存。

DOM が要るもの (`nest` の描画、`initTheme` の localStorage) は
`environment: "happy-dom"` で回す。**ブラウザゲートの代わりではない** —
happy-dom が答えるのは「この関数が期待の DOM を作るか」だけで、
「375 px で読めるか」「検索が実際に返るか」は今まで通り実 Chrome の仕事。

---

## 6. ゲートと CI

`tools/assets-gate.sh` を足す。**機材 (node) を要るのでテストではなくゲート**
(CLAUDE.md の境界定義そのまま)。中身は 1 本道:

```
npm ci  →  biome ci  →  tsc --noEmit  →  vitest run  →  vite build
```

**鮮度の検査は要らない** — 決定 1 で生成物が git に無いので、腐りようがない。
ゲートが見るのは「TS が型を通り、lint を通り、テストが緑で、バンドルが建つ」ことだけ。
`vite build` の段は `cargo build` も走らせているが、ここで**単独でも**回すのは、
**落ちた段が TS なのか Rust なのかを 1 行で言える**ようにするため。

**このゲートは必ず一度落としてから通す** (CLAUDE.md: ゲートは自分では自分を検査しない)。
落とし方は 4 通りとも決めてある — 型エラー / lint 違反 / 落ちるテスト / 構文エラーを
1 つずつ入れて、**それぞれ別の段で落ちる**ことを確認する
(「どの段でも同じように落ちる」ゲートは段を分けている意味が無い)。

CI (`ci.yml`) には `actions/setup-node` + このゲートを 1 段足す。
**Rust の段より前に置く** — node の段は数十秒で、TS が壊れているのに
Rust を 10 分回すのは無駄。

### 巻き添えの範囲 — 数えた

`cargo` を回すのは **13 本中 8 本**【実測】: `ci.yml` (13 箇所) / `ci-lake.yml` /
`release.yml` / `ci-leak.yml` / `ci-extractor-portability.yml` / `ci-placement.yml` /
`ci-browser-windows.yml` / `ci-action.yml`。
**加えて `action.yml`** — 利用者が使う composite action で、リリース済のバイナリが
無いとき (`@main` 参照、未リリースのプラットフォーム) に **`cargo build` へ落ちる**【実測】。
つまり決定 1 の「利用者は node を払わない」には**この 1 経路だけ穴がある**。

穴が塞がっている根拠は 1 つ:
**GitHub-hosted runner には Node が最初から入っている**【**仮定** — 未検証。
`runs-on` は 13 本すべて ubuntu / macos / windows-latest で、container 指定は
1 件も無い【実測】ので、runner image の同梱物がそのまま使える *はず*】。
**この仮定は P3 で CI を実際に回して潰す** — 潰れるまで「たぶん大丈夫」と読まない。
外れていた場合の手当ては、8 本 + `action.yml` の cargo 経路に
`actions/setup-node` を足すこと (作業量は分かっていて、判断が要らない)。

---

## 7. 撤退ライン

**倍率では引かない** (2026-08-19 の反省: 検索 v2 の撤退ラインを倍率で引いて
「4.9 倍だから撤退」という無意味な判断に至りかけた)。引くのは挙動と手順の線:

- **挙動同値が崩れたら撤退。** `browser-gate.sh` / `search-gate.sh` / `e2e-micro.sh` の
  どれかが TS 化のあとで赤くなり、**それが TS 側のバグではなく方式の限界**だと分かったら、
  その時点で止めてユーザーに返す。ゲートを緩めて通すことはしない。
- **`cargo build` が node 以外を要るようになったら撤退。** 決定 1 が受け入れたのは
  node ただ 1 つで、「ついでに」何かが増えるのは受け入れていない。実測は
  **`node_modules` を消し、ネットワークを切って `cargo build`** — 落ちるのは正しいが、
  **落ち方が「`npm ci` を回せ」と読めること**が条件 (無言の `include_str!` エラーは不可)。
- **既存ワークフローの巻き添えが数えられなくなったら止める。** cargo を回す
  ワークフローすべてに `actions/setup-node` が要る。P3 で**全数を数えてから**足す
  (数えずに ci.yml だけ直すと、残りが「node が無い」で赤くなる)。
- サイズは**線ではなく観測**として記録する。minify 後の `assets/app.js` のバイト数と
  gzip 後を `docs/milestone-log.md` に足し、予算 (現 32 KiB) を実測に合わせて引き直す。
  **予算を下げるのは「余白が戻った」の裏返しなので、下げた根拠を Cargo doc コメントに残す。**

---

## 8. 段取り

| | 中身 | 完了の判定 |
|---|---|---|
| **P0** | `web/` の足場 — mise / package.json / tsconfig (strict) / biome.json (スペース) / vite.config.ts / vitest 設定 | `npm run build` が空の `main.ts` から `assets/app.js` を作る |
| **P1** | 917 行を §4 の 16 本に移す。**挙動は 1 行も変えない** | `vite build` が通り、`browser-gate.sh` が緑 |
| **P2** | §5 のテストと Rust 側のフィクスチャ再生成テスト | `vitest run` が緑、Rust 側も緑 |
| **P3** | `tools/assets-gate.sh` + CI。**一度落としてから通す**。決定 5 (biome の範囲) を実測して判断 | CI が緑、ゲートを意図的に落とせることを確認済 |
| **P4** | `THEME_BOOT` を TS 化 (決定 6) | `frame.rs` の同期スクリプトが `include_str!` 由来になり、テーマキーの重複が消える |

**P4 は任意。** P0〜P3 が入った時点でこの計画の目的
(型・lint・テスト・ビルド・minify) は満たされている。
