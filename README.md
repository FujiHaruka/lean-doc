# lean-doc

**Mathlib に依存する Lean パッケージのための、高速ドキュメント生成基盤。**
自パッケージのモジュールだけを短時間でドキュメント化し、依存ライブラリ (Mathlib) は
**再生成せず外部参照にする** — 飛び先は**そのパッケージの版固定 GitHub ソース**
(`…/mathlib4/blob/<rev>/Mathlib/Order/Basic.lean#L67-L67`)。出力は静的 HTML。

抽出器は **Lean** (`extractor/`)、その外側 — IR の消費・HTML レンダリング・増分・
全域成果物・依存写像 — は **Rust** (`crates/`)。

> **これは doc-gen4 の再実装であって、独立実装ではない。**
> レンダリングと抽出の判断は **doc-gen4** (Apache-2.0, © 2021 Henrik Böving) を**読んで**
> 書き直したもので、**出力の byte 一致を受け入れオラクルにして**開発した (M6 で 99.5062%)。
> 抽出器には逐字コピーも残っている。**lean-doc 全体を doc-gen4 の派生物として扱い**、
> 同じ Apache-2.0 で配布する。以下の速度比較は**同じ設計の別実装どうしの比較ではなく、
> やる仕事を減らした実装との比較**。
> → [`NOTICE`](NOTICE) / [`docs/provenance.md`](docs/provenance.md)

```sh
lean-doc build --root <あなたのパッケージ> --out <出力先> --extractor-bin <抽出器>
```

**現在: v0.1.0** — **2026-08-17 に tag `v0.1.0` で締めた**。締めたのは
**完了条件だった 2 つのゲートが決着したから**であって、下の[未検証項目](#未検証項目)が
片付いたからではない (**17 件はそのまま残っている**)。

| | |
|---|---|
| **ゲート B** = v0.1 の完了条件 (対象リポジトリ**以外**で 1 コマンドが通り、増分が効き、CI に置ける) | **通過**【実測 2026-08-17、`tools/target2-gate.sh all` = all checks passed → [`benchmarks/results/g3-gate-b-2026-08-17.txt`](benchmarks/results/g3-gate-b-2026-08-17.txt)】。ただし第 2 の対象は**合成に限る**と決めてあるので、主張は「合成の対象で通った」まで |
| **ゲート A** = doc-gen4 出力との byte 再現 | **終了。再定義しない**【決定 2026-08-16】。M6 時点で 99.5062% を出したあと、M7 で依存リンクを版固定ソースに切り替えてバイトでは一致しなくなり、M8 で UI を自前にした時点で doc-gen4 は「バイトのオラクル」ではなくなった |
| **代わりに置いたもの** | **外部オラクルを要らない 3 種** (自己整合性 / 不変量 / Lean 自身) → 下の[品質ゲート](#品質ゲート--何をもって壊れていないとするか) |
| **緑の定義** | `cargo test --workspace` **346 passed / 0 failed / 21 ignored** + clippy `-D warnings` + `fmt --check`、**CI が push ごとに判定**【実測 2026-08-17】 |

**「v0.1」は「完成」ではなく「ここまでを通した」という線。**
**通っていないもの・測っていないものは [未検証項目](#未検証項目) に全部書く。**

---

## 何をやめたから短いのか

**「Rust だから速い」ではない。** 差の大半は**やらなくてよい仕事をやめた**ことによる。
doc-gen4 (v4.31.0) を計装して 432 モジュールのドキュメント生成を実測したところ、
コストの大半はドキュメントを作る仕事ではなかった【すべて実測 →
[`benchmarks/doc-gen4-report.md`](benchmarks/doc-gen4-report.md)】:

| 実測 | |
|---:|---|
| **85.0%** | 抽出フェーズが環境ロードに費やす割合 (モジュールごとに 1 プロセス起こすため) |
| **0.032%** | 走査した定数のうち実際に処理されたもの (8.9 億件中 28 万件) |
| **0%** | HTML 生成フェーズの増分性 (無変更でも毎回全再生成) |

やめたのは 4 つ — ①**1 プロセスでまとめて抽出する**、②**走査をやめて索引を引く**、
③**依存ライブラリを再生成せず、名前 → モジュール → 版固定ソースの写像だけ持ってリンクする**、
④**HTML 生成に Lean を要らなくする** (IR から生成し、変更が無ければ再生成もしない)。

外側を Rust で書いたのは速度のためではない — **外側の速度差は言語ではなく
「IR を何回全部読むか」で決まる**【実測 → [`docs/verification-log.md`](docs/verification-log.md)】。

---

## 数字

**倍率は分母が違えば別の主張。混ぜて引用しない。**
ラベルは 実測 (ログがある) / 外挿 (実測の一部から伸ばした) / 仮定 / 理論値。

### 現行 doc-gen4 との比較 — 3 つとも別の主張

| 比較 | 現行 doc-gen4 | lean-doc | 倍率 | ラベル |
|---|---:|---:|---:|---|
| **(a) 自パッケージ 432 モジュールの抽出 + IR 永続化** | 1,076 s | **14.08 s** | **76×** | 両方【実測】warm、どちらも 1 スレッド |
| **(b) ゼロからサイトを構築 (依存込み)** | 約 32,600 s = 9.1 時間 | **14.97 s** | **約 2,180×** | 分子【外挿】/ 分母【実測】warm |
| **(c) 同じ 432 ページの HTML 生成だけ** | 3.17 s | **0.885 s** | **3.6×** | 分子【外挿】/ 分母【実測】warm |

- **(b) の分子は完走していないビルドからの外挿** — 8,600 モジュールのうち 3,590 (42%) を
  抽出した時点で打ち切り、そこまでの実測 13,611 s を全体に伸ばした値。
  **2,180 倍の大半は実装の巧拙ではなく作業範囲の差** (依存を外部参照にしたぶん)。
- **(c) の分子は外挿** — doc-gen4 の 6,072 モジュールでの実測をモジュール数で比例縮小した値。
- **1 モジュール変更の倍率は書かない**【判断】 — doc-gen4 の増分 5.21 s【実測】は
  **HTML を再生成していない** (stale) 上に `lake build` を含み、こちらは HTML を更新して
  `lake build` を含まない。**同じ仕事ではない。**

### `lean-doc build` 自身の実測 (対象: 432 モジュール、Apple M1 / 16 GB / `--jobs 4`)

| 何を | 時間 | 条件 |
|---|---:|---|
| フル生成 (依存写像を外から渡した構成) | cold **21.53 s** / warm **9.79 s** | 【実測】同じ仕事が page cache で 2 倍動く |
| フル生成 (**写像も自分で作る** = 既定の 1 コマンド) | **29.50 s** | 【実測 2026-08-15】page cache は cold 寄り。同じ構成の warm は未測定 |
| 何も変えずにもう 1 回 | **0.30〜0.40 s** | 【実測】サイトは 1 バイトも動かない (438 行の sha256 マニフェストが完全一致) |
| 本物のモジュール移動 + `lake build` (35 抽出 / 36 ページ) | **5.80 s** | 【実測】結果は編集後ソースからのフル生成と **439/439 バイト一致** |
| **既存モジュールに 1 宣言足す + `lake build`** (1 抽出 / **1 ページ**) | warm **3.96〜6.22 s、中央値 4.35 s** (壁時計、n=6) | 【実測 2026-08-17】**同じバイナリでこの幅** — 動いているのは Lean の環境ロード (2.4〜3.8 s、既知の床) で、**壁時計はこの対象では閾値にできない**。同じ編集は 2026-08-17 の 4 段の前には **422 ページ全部を再描画**していて内側時計 6.24 s だった → [`docs/plans/reextract-count.md`](docs/plans/reextract-count.md) §6 |
| rev だけ変更 (Lean 起動 0 回) | **0.65 s** (433 ページ) / **0.87 s** (432 ページ) | 【実測】2 つは別の木で、ページ数が違う |

**cold と warm を混ぜて読まないこと。** olean は mmap で読むので、同じ仕事が
page cache の状態で 2 倍以上動く (環境ロード単体では warm 2.5 s ↔ cold 13 s【実測】)。

### byte 再現率 — これは**受け入れオラクル**であって製品目標ではない

同じ IR から作ったページを doc-gen4 の出力と領域単位で突き合わせた値。採点器は Deno の
`experiments/stage4c/coverage.ts` で、**tag `experiments-frozen` にある** (HEAD には無い →
[構成](#リポジトリの構成))。**再測定 2026-08-15、`lean-doc build` の出力に対して**【すべて実測】:

| | |
|---:|---|
| **99.5062%** | 再現できたバイト 21,919,956 / 22,028,728 |
| **304 / 348** | byte 完全一致したページ |
| **108,772 B** | 再現できなかったバイト。**全部 rev** (それ以外の食い違いは 0 バイト) |

- **母数は doc-gen4 が実際にディスクに出した 348 ページ。** フルビルドを 42% で打ち切った
  ため、現行の 432 モジュールに対して 84 ページ足りない。
- **残り 0.494% はオラクル側の天井** — 参照ツリー自身が 2 つの git revision を持つ。
- **採点する木の `--source-url` は参照木と同じ rev でなければならない**【実測】 —
  別の rev を渡すと `gh_link` / `nav_gh` が全部外れて **96.4%** になる (差は rev だけ)。
- **静的資産 (`style.css` / `app.js` など) はこの母数の外** (→ [出力](#出力に何が入っていて何が入っていないか))。
- **この数字は M6 時点の到達点であって、いまの出力に対する主張ではない。** M8 で UI を
  自前にしたので、**doc-gen4 とはもうバイトで一致しない (意図的に)**。書き換えずに残してあるのは、
  M1〜M6 の結果ログと食い違わせないため → [`docs/milestone-log.md`](docs/milestone-log.md) の M8。
- **この採点はもう回していない。** doc-gen4 互換を追うのをやめた時点で役割が終わったので、
  採点器ごと HEAD から撤去した (2026-08-16)。**再開するなら tag から復元する。**

---

## インストール

### 要るもの

- **Rust** (`rust-toolchain.toml` が stable を指定。rustup が入っていれば自動で解決する)
- **C コンパイラ** — CommonMark は md4c を vendor して FFI でリンクしている (移植ではない)
- **elan / lake**、および **`lake build` が通る対象パッケージ** —
  抽出器は対象の toolchain を借りてビルドするため

### 手順

```sh
git clone <this repository> && cd lean-doc

# 1. Rust 側は cargo で完結する -> target/release/lean-doc
cargo build --release

# 2. 抽出器 (Lean) は対象パッケージの環境を借りてビルドする -> extractor/build/extract
TARGET_REPO=/path/to/your-package extractor/build.sh
```

**lean-doc 側に toolchain も lakefile も Mathlib も置かない** — 環境は `lake env` で
対象から借りる。抽出器のバイナリは 171 MB あり、**対象の toolchain に対して作られる**ので、
toolchain が変わったらビルドし直す (だから配布もできないし、既定のパスも持たせていない)。

---

## 使い方

```sh
./target/release/lean-doc build \
  --root /path/to/your-package \
  --out  /path/to/docs \
  --extractor-bin ./extractor/build/extract \
  --jobs 4
```

サイトは **`<out>/site`**。同じ `--out` にもう一度回すと**増分**になる
(`<out>/{ir,state,work}` と `<out>/ledger.json` がその状態)。

**コマンドが自分で決めるもの** — 渡す必要はない:

| | 出所 |
|---|---|
| `--lib` | `lakefile.toml` の `[[lean_lib]]`。読めない形 (`lakefile.lean` など) は**推測せず exit 3** |
| モジュール一覧 | ソースの glob (`.lake/build` を走査しない — 孤児 olean を拾うため) |
| `--source-url` | git の HEAD + origin remote。**github.com のみ** (`/blob/<rev>/` は GitHub の形) |
| 依存写像 (`.lidx`) | **抽出器が import 済みの環境を走査して作る** — 上流サイトの配布物に依存しない。宣言ごとの**ソース行範囲**もここに入る |
| 依存リンクの飛び先 | `lake-manifest.json` の url + 40 桁 rev、Lean core だけ `lean --githash`。**すべてオフライン** — ビルド時にネットワークへ出ない |
| フル生成か増分か | `--out` に何が居るか (`lean-doc-build.json` マーカー) |

**`--out` は必須で既定なし。** 素直な既定 `<root>/.lake/build/doc` は doc-gen4 の出力木
なので、そこを既定にするのはデータ消失。`--root` の中の `--out` は exit 3、マーカーの無い
非空ディレクトリも exit 3 (**このコマンドが作ったと確認できる木しか消さない**)。

`--full` で全部作り直し。段ごとのサブコマンド (`modules` / `extract` / `site` / `render` /
`global` / `incremental` / `ledger` / `merge` / `ownership` / `impact` / `prune`) も
表に出ている — 引数なしで `lean-doc` を実行すると usage が出る。

---

## CI に置く

**`lake build` と同じジョブに置くこと。** 理由は Lean の速度ではなく I/O で、決めているのは
olean が page cache に残っているかどうか。**罰則の大きさはランナーの RAM で決まる**
【実測、Linux ランナー n=5 → [`docs/approach.md`](docs/approach.md) §3】:

| | 環境ロード |
|---|---|
| 同じジョブ / 別ジョブ (4 コア / 15.6 GiB、2026-08-16) | 2.4〜2.6 s / 2.5〜2.9 s = **1.08〜1.58 倍** |
| 同じランナーで page cache を落とす (陽性対照) | **13.5〜22.3 s = 5.2〜11.9 倍** |
| 別ジョブ (2 コア / 7.75 GiB、2026-08-10) | 20〜89 s = 8〜34 倍 |

**別ジョブが cold になるとは限らない** — 別ジョブも `lake exe cache get` と
キャッシュ復元で olean を**自分で書く**ので、RAM が足りればそのまま page cache に残る。
同じジョブに置くのは、**その比を問わなくて済ませる**ため。

- **[`tools/ci-build.sh`](tools/ci-build.sh)** — コマンドの実体。
  `lake exe cache get` (任意) → `lake build` → 抽出器 → cargo → `lean-doc build` を
  1 ジョブで順に回し、**段ごとの実時間**を出す。
- **[`.github/workflow-templates/lean-doc-docs.yml`](.github/workflow-templates/lean-doc-docs.yml)**
  — その薄いラッパー。**あなたのパッケージの** `.github/workflows/docs.yml` にコピーして使う。

**この形にしたのは検証できるようにするため** — workflow YAML に直接コマンドを書くと
「誰も実行したことのないファイル」になる。実体をシェルスクリプトに置けば、
**CI が走らせるのと同じコマンドをローカルで実際に実行できる**。

**実測 (V5) — 第 2 の対象 (13 モジュール 2 ライブラリ、Mathlib 依存) で
`tools/ci-build.sh` を端から端まで実走**【すべて実測 2026-08-15。Apple M1 / 16 GB /
`--jobs 4` / macOS。**GitHub Actions ではなくローカル**、page cache は warm 寄り】:

| 段 | clean (パッケージの `.lake/build` を消して 13 モジュール再ビルド、**n=4**) | 何も変えずもう 1 回 (**n=5**) |
|---|---:|---:|
| `lake exe cache get` | **未実行** (ネットワーク) | 未実行 |
| `lake build` | 3.879 / 3.974 / 3.982 / **8.146** | 1.071〜1.083 (replay) |
| 抽出器のビルド | cache hit (**別測定で 14.90 s**) | cache hit |
| `cargo build` | cache hit | cache hit |
| **`lean-doc build`** | 1.764 / 1.783 / 2.061 / **2.091** | **0.058〜0.062** (0 再抽出 / 0 ページ) |
| **合計** | 5.887 / 5.897 / 6.080 / **10.361** | **1.274〜1.286** |

**1 回目 (太字) だけ倍近い** — このセッションで最初の Lean 起動で、差は page cache であって
実装ではない【実測】。**1 回だけ測った数字を信じないこと**: 別条件 (パッケージは再ビルド
せず `--out` だけ新規) では `lean-doc build` が 3.875 s も出ている。

出たサイト 19 ファイルは、同じ IR からの `lean-doc site` と `/usr/bin/diff -r` で
**差分 0**【実測】。**CI ランナーでの同じコマンドの実測は `docs/verification-log.md`
「CI 軸 (2026-08-16)」にある** — 4 コア / 15.6 GiB で `lean-doc build` は **11.5〜20.7 s**
(422 モジュール、n=5)。**「ランナーは cold 側だからこれは下限」は成り立たなかった**
(page cache に載るため)。
抽出器のビルドは別測定で **14.90 s wall / 10.07 s user / peak RSS 1.52 GB**【実測、warm】
— キャッシュする理由は大きさではなく、**パッケージのコミットでは変わらない**こと。

キャッシュは 3 つ、いずれも**同じジョブの中で復元して使う** (workflow のコメントに理由):
`~/.cache/mathlib` (鍵: `lake-manifest.json`)、`extractor/build` (鍵: `lean-toolchain` +
`Extract.lean` のハッシュ)、cargo + `target` (鍵: `Cargo.lock`)。

---

## 出力に何が入っていて、何が入っていないか

`<out>/site` = **モジュールページ N 本 + 全域成果物 7 本 + 静的資産 3 本**。
対象 1 では 432 + 7 + 3 = **442 ファイル**【実測 2026-08-16】。

| 全域成果物 | |
|---|---|
| `index.html` | サイトの入口。全モジュールの一覧でもある |
| `404.html` | GitHub Pages の custom 404。似た名前を提案する |
| `search.html` | 検索結果のページ |
| `foundational_types.html` | `Sort` / `Type` / `Prop` — ソース中に無い型の説明 |
| `modules.json` | モジュールツリーと Imported by (全ページで読む) |
| `search-index.json` | 検索と instances (**必要になってから**読む) |
| `declarations/name-map.json` | 増分パイプラインが `--before` で読む。UI は使わない |

| 静的資産 | |
|---|---|
| `style.css` / `app.js` / `favicon.svg` | **バイナリに埋め込んであり、毎ビルド無条件に書き出す** |

**この木は自己完結している** — 空のディレクトリに置いて開けば、**外部ホストへのリクエスト 0 本**で
描画される【実測 2026-08-16。生成した木に外部の `<script src>` / `<link href>` が 0 本】。
実際に置いたもの: **<https://fujiharuka.github.io/information-theory/>**
(422 モジュール、`lean-doc build` 1 コマンド **24.51 s**【実測、warm】)。
doc-gen4 が読んでいた CDN 4 本 (Lato / JuliaMono / polyfill / MathJax) は無い。
**外部を指すのは `<a href>` だけ**で、それは依存パッケージのソースへの版固定リンク (M7)。

**M8 で母数が動いた**: byte 再現の母数は 432 + 6 = 438 (M6) → 432 + 7 = **439**。
静的資産 3 本は**以前も今も母数の外**。doc-gen4 の JS のためだけにあった 5 本
(`declaration-data.bmp` / `navbar.html` / `tactics.html` / `references.{bib,html}`) は、
**その JS を捨てたので読み手ごと消えた** → [`docs/milestone-log.md`](docs/milestone-log.md) の M8。

---

## 未検証項目

**済んだことにしない。** ここに書いてあるものは v0.1 では測っていない / 通していない。**14 件**。
**下の[「意図的に持たないもの」3 件](#意図的に持たないもの--未検証ではなく決定)とは分けてある** —
あちらは測っていないのではなく、持たないと決めたもの。

1. **実在の公開パッケージでの実走** — ゲート B の第 2 の対象は**合成に限る**と決めた
   (同じ Mathlib rev に固定するため)【決定 2026-08-15】。合成の対象は境界値を**狙って**
   入れられる代わりに、**こちらが知らない形では驚かせてくれない。**
   **v0.1 を締めた (2026-08-17) 時点でこの制約は役目を終えている** — 今は「v0.1 の範囲外」
   ではなく**単に未実施**で、下の 4 / 5 / 9 / 12 はこれ 1 本で同時に当たりうる。
2. **GitHub Actions 実走** — **テンプレートの手順は実走して緑になった**
   【実測 2026-08-16、[run 31955883894](https://github.com/FujiHaruka/lean-doc/actions/runs/31955883894)】。
   actions のバージョンと入力・3 つのキャッシュ鍵・elan のインストール・`ci-build.sh` の
   1 行がランナー上で通っている (13 モジュール / 23 ファイル / `docs` 2.510 秒)。
   **残っているのは `push:` トリガと、利用者リポジトリ自身の checkout の 2 つだけ**
   (検証では対象パッケージをジョブ内で生成しているため)。
3. **`lake exe cache get`** — **実走した** (同 run。`--cache-get` を渡した経路も通っている)。
   ただし**ローカルの計測では依存が既にあるので通っていない**経路のまま。
4. **`«…»` を含む依存側モジュールの `.lidx` 綴り差** — **綴り差は実在し、解決も実際に外れた**
   【実測 2026-08-17 → [`e2e/README.md`](e2e/README.md)】。`.lidx` は `Dep-Aux.Basic` と
   **非エスケープ**、IR と import リストは `«Dep-Aux».Basic`。docstring が同じモジュールを
   3 通りに綴ると、**`.lidx` と同じ非エスケープ綴りだけが解決しない**
   (IR の綴りと source path 記法は解決する)。
   **残っているのはここから**: この差が**出力に出る**のは、**版固定できる依存**が
   ギュメ付きモジュールを持つときだけ。フィクスチャの依存は path なので
   3 綴りとも「リンクを張らない」に落ちて差が消える。**その実物でまだ測っていない。**
5. **「同名宣言を複数モジュールが持つ形」** — 対象 1 には **25 件**ある
   【実測 → [`docs/verification-log.md`](docs/verification-log.md)。索引方式 8,849 と
   走査方式 8,824 の差】が、第 2 の対象で**再現できなかった**
   (equation lemma を強制しても blacklist で IR に出ない)。**測れていない。**
6. **CI ランナーが cold 側かどうか** — **測った。cold 側ではなかった**【実測 2026-08-16、
   n=5、[run 31956756819](https://github.com/FujiHaruka/lean-doc/actions/runs/31956756819)】。
   別ジョブでも major fault は 3 桁以下で、環境ロードは同じジョブの 1.08〜1.58 倍にしかならない。
   **未検証として残るのはこの先**: 測ったのは **1 ワークロード** (422 モジュール /
   peak RSS 約 4.0 GB / 15.6 GiB のランナー) だけで、**より大きいパッケージや RAM の
   小さいランナーでは cold 側に戻りうる** (同じランナーで page cache を落とすと 5.2〜11.9 倍)。
7. **Lean のバージョン差** — 動作を確認したのは **v4.31.0 のみ**
   (Mathlib `fabf563a7c95`)。あらゆるバージョンへの後方互換は**意図的にやらない**。
8. **fenced code block 中の NUL** — MD4Lean が SIGSEGV する入力なので**合わせる相手が
   存在しない**。lean-doc は落ちないほうに倒しているが、出力には U+FFFD に加えて
   **生の NUL が 1 バイト残る**【実測】。
9. **プロトタイプ (TS) との登録済みの乖離 3 件** (**プロトタイプは HEAD に無い** —
   tag `experiments-frozen` から読む → [撤去したプロトタイプ](#撤去したプロトタイプ--tag-experiments-frozen))
   — CommonMark サブセット / autolink の
   `inLink` / heading id の分割表。**3 件とも lean-doc のほうが doc-gen4 と一致する側**で、
   対象 1 のバイトは動かない【実測】。ただし根拠は「**対象 1 に出現しない**」なので
   **別の対象では出うる** (heading id は第 2 の対象で実際に `id=` と `href=#` に出た)。
   詳細と母数は [`docs/implementation-plan.md`](docs/implementation-plan.md) §5。
10. **依存 root の 39 件中 27 件にオフラインのオラクルが無い**【実測 →
    [`docs/milestone-log.md`](docs/milestone-log.md) M7-b】 — doc-gen4 の参照木は対象の
    import closure しかドキュメント化していないので、`Archive` / `Counterexamples` / `Cli` /
    `MD4Lean` / `UnicodeBasic` などは**照合相手が無い**。HTTP のサンプルだけが手段。
11. **`--root` 無しの `ledger check` は `build` と違う `renderKey` を出す** — 依存リンクの
    写像が key に入ったため。**全ページ再生成になるだけで誤りは出ない**が、手で段を回す人は踏む。
12. **等幅フォントのフォールバック (M8 決定 2)** — Web フォントを読まないので、本文に出る
    **非 ASCII 178 種**【実測】が読める字形になるかは**環境依存**。添字 (₁ ᵢ ᵐ ⁿ) と
    double-struck (ℝ ℕ ℤ) を持たない等幅フォントでは字幅が崩れる。
    **macOS 以外では見ていない。** 崩れるなら JuliaMono をサブセットして vendor する。
    **ブラウザゲートはここを見ない** (下の 13) — 見るのはレイアウトであって字形ではない。
13. **実ブラウザで見ていないのは「見た目そのもの」。** **動作は見ている** —
    [`tools/browser-gate.sh`](tools/browser-gate.sh) が CI で本物の Chrome を回し、
    **9 検査すべて緑・375 px の overflow 0 px**【実測 2026-08-16 →
    [`docs/plans/quality-gates.md`](docs/plans/quality-gates.md) Q8。`ui-redesign.md` の
    **UI-3「未判定」はこれで決着した**】。**ゲートが見ないのは 3 つ**:
    **フォントの字形** (上の 12 と同じ穴)、**ダークモードの実際の色**
    (見ているのは `data-theme` が動くことだけで、色ではない)、**スクリーンリーダー**。
    CSS を読んで正しいはずだ、以上のことが言えないのはこの 3 つに縮んだ。
14. **md4c FFI の fuzz と `cargo-deny` を回していない** — 品質ゲート計画の Q6 と Q9 の残り
    (→ [`docs/plans/quality-gates.md`](docs/plans/quality-gates.md) §5)。CommonMark は
    **vendor した C を FFI で呼んでいる**のに、**落ちる入力を探す手段がゲートに無い**。
    既知の 2 入力 (fenced code 中の NUL / 本文行の無い GFM テーブル) は回帰テストにあるが、
    **それは見つけたものであって探した結果ではない。**
    ワークスペース全体の mutation (**1,602 mutant、未実施**【実測 → 同 Q10。1 個約 3 秒 =
    約 80 分】) は**ゲートにしないと決めてある**ので、これは積み残しではなく判断。

### 意図的に持たないもの — 未検証ではなく決定

**上の 14 件と混ぜない。** これは測っていないのではなく、**持たないと決めた**もの。
消さずに残すのは、**何が起きたら判断が変わるか**が計画の一部だから。

| 決めたこと | 中身 | 判断を変える条件 |
|---|---|---|
| **プレビューモードと、検索の依存横断は持たない** | → [`docs/approach.md`](docs/approach.md) §9。**テーマは M8 で持った** — 検索も自パッケージの 4,750 宣言については持つ。**依存パッケージの宣言は検索できない** | 依存を横断して引きたい要求が実際に出たとき |
| **数式は組版しない** (M8 決定 3) | 対象 348 ページ中 TeX があるのは **1 ページ、`$` 12 個**【実測】なので、MathJax も KaTeX も積んでいない。`$$…$$` は**文字列として読めるだけ** | 数式の多いパッケージを対象にするとき |
| **`file://` ではモジュールツリーを出さない** (M8 決定 4) | `fetch` が同一オリジンで止まる。**捨てた**のであって気づいていないのではない。本文は読めるし、`<noscript>` が `index.html` へ導く | 配布先が GitHub Pages (HTTP) でなくなるとき |

---

## 文書

| | |
|---|---|
| [`docs/approach.md`](docs/approach.md) | アプローチの **SoT**。なぜこの形なのか、何を意図的にやらないか |
| [`docs/implementation-plan.md`](docs/implementation-plan.md) | 実装の **SoT**。ゲート / 移設の順序 / Rust 側の構成と制約 |
| [`docs/milestone-log.md`](docs/milestone-log.md) | 上の**結果**。M1〜M7 の各段で何を通し、どの数字が出たか |
| [`docs/verification-log.md`](docs/verification-log.md) | **数字の SoT**。予測と食い違ったらこちらが正 |
| [`docs/provenance.md`](docs/provenance.md) | **由来判定の SoT**。どのコードが doc-gen4 / 第三者由来か、そこから出る義務は何か |
| [`benchmarks/`](benchmarks/) | 実測レポート・計装パッチ・生ログ。**数字の出所** |

**docs に書く数字はすべて 実測 / 外挿 / 仮定 / 理論値 のラベルを持つ** — これは
このプロジェクトの成果物が数字だから (→ `CLAUDE.md`「計測の誠実性」)。

## 品質ゲート — 何をもって「壊れていない」とするか

**doc-gen4 との byte 再現ゲートは M8 で終わった。** UI を自前にした時点で、
正しい出力を知っている第三者がいなくなったから。**消えたのは外部オラクルであって
テストの網ではない**が、代わりを置かないと残ったテストは自分が何を守っているか言えなくなる。

代わりに置いたのは**外部を要らない 3 種類の判定**:
**(1) 自己整合性** (出力は自分の中で閉じているか) /
**(2) 不変量** (別経路が同じ答えを出すか) / **(3) Lean 自身を上流の事実として使う**。

| | 何を見るか | どこで |
|---|---|---|
| `cargo test --workspace` | **機材ゼロ依存**のテスト。ここが「緑」の定義 | CI (push ごと) |
| [`tools/e2e-micro.sh`](tools/e2e-micro.sh) | **本物の Lean → 本物の抽出器 → site**。1 コマンド / 冪等 (2 回目は 0 抽出 0 描画) / 決定性 (別ディレクトリへのフル生成がバイト一致) / `--jobs` 不変 | CI |
| [`tools/site-gate.sh`](tools/site-gate.sh) | 内部リンクの 404 = 0、外部ホストへのリソース読み込み = 0、**検索索引とページが双方向で一致** | CI |
| [`tools/browser-gate.sh`](tools/browser-gate.sh) | 実ブラウザ: コンソールエラー 0 / ツリー / 検索 / instances / テーマ / **375 px で横スクロールなし** / JS 無効で本文が読める | CI |
| [`tools/provenance-gate.sh`](tools/provenance-gate.sh) | 帰属表示が実在するか (27 claims)。Apache-2.0 §4 は public 化で発動している | CI |
| [`tools/corpus-gate.sh`](tools/corpus-gate.sh) | **計測対象を要する 24 本**。機材が要るのでテストではなくゲート | 手動 |

**「テスト」と「ゲート」を分けているのは、境界を CI の境界に一致させるため** —
テストは自分の入力を持ち、ゲートは機材・対象・toolchain を要る。**corpus 依存のものが
`cargo test` から「静かに skip」されていた間、そのうち 7 本はフィクスチャが機材から
消えているのに緑を返していた**【実測 2026-08-16】。`#[ignore]` に変えて初めて分かった。

e2e フィクスチャ [`e2e/micro`](e2e/) が**対象パッケージの持たない宣言の形**
(`class` / `inductive` / `class inductive` / 非 `mk` constructor / 継承 field /
BMP 外の識別子 / `scoped notation`) を構成として持つのは、**オラクルの入力に無い形は
何バイト一致しても見えない**から。実際、最初に通した時点で **inductive の constructor が
ページに 1 つも描かれていない**という欠陥が出た。計画と結果は
[`docs/plans/quality-gates.md`](docs/plans/quality-gates.md)。

## リポジトリの構成

```
crates/          製品コード (Rust)。ir / md / render / global / incr / CLI
extractor/       抽出器 (Lean)。build.sh が対象の lake env を借りてビルドする
e2e/micro/       e2e フィクスチャ (Lean、Mathlib 非依存)。対象が持たない形を持つ
tools/           ハーネスとゲート。ci-build.sh もここ
benchmarks/      計測レポート・計装パッチ・生ログ・検査ツール
docs/            上の 5 文書 + docs/plans/
NOTICE           第三者コードの帰属。doc-gen4 / md4c / MD4Lean / UnicodeBasic / V8
```

### 撤去したプロトタイプ — tag `experiments-frozen`

検証段階 1〜8 の使い捨てプロトタイプ (`experiments/`、TS + シェル、27 ディレクトリ / 164 ファイル)
は Rust への移設が M1〜M8 で終わった時点で **HEAD から撤去した (2026-08-16)**。
**履歴には残っている** — `experiments/` が完全な状態の最後の commit (`a15addc`) に
tag **`experiments-frozen`** が打ってある:

```
git show experiments-frozen:experiments/stage7d/render.ts   # レンダラのプロトタイプ
git show experiments-frozen:experiments/stage4c/coverage.ts # 受け入れオラクル
git log  experiments-frozen -- experiments/
```

- **docs が数字の出所として指す `experiments/...` は、すべてこの tag を伴う。**
- **消えたのは採点器であって数字ではない。** commit 済フィクスチャ
  (`crates/*/tests/data/*-expected.json`) は凍結値として残っていて `cargo test` は無傷。
  ただし**再生成手段は HEAD に無い** — 作り直すには tag から生成器を復元する。
- 採点器を Rust に書き直さなかったのは、**同じ言語・同じ設計で書き直すと「両方同じ間違いを
  する」経路ができる**ため。採点器は作り直すのではなく、役割が終わった時点で畳んだ。

## 計測条件

機材 **Apple M1 / 8 コア / 16 GB**、macOS。Lean・Mathlib・doc-gen4 いずれも **v4.31.0**
(Mathlib `fabf563a7c95`)。**計測対象は常に同じ Lean プロジェクトに固定**
(`InformationTheory`、432 モジュール、Mathlib 全体に依存) — **比較は同一ワークロード上でのみ
意味を持つ**ため。第 2 の対象は `tools/make-target2.sh` が同じ Mathlib rev で合成する
13 モジュール 2 ライブラリのパッケージ。

## ライセンス

**Apache License 2.0** ([`LICENSE`](LICENSE))。

一部のファイルは **doc-gen4 (Apache-2.0, © 2021 Henrik Böving) の派生物**で、
そのことは各ファイルに書いてある。第三者コードの帰属は [`NOTICE`](NOTICE) に、
どのファイルが何に由来するかの内訳は [`docs/provenance.md`](docs/provenance.md) にある。
**lean-doc は doc-gen4 をリンクしない** — 抽出器は `import Lean` だけ、Rust 側は Lean に
一切依存しない。
