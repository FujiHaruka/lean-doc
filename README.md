# lean-doc

**Mathlib に依存する Lean パッケージのための、高速ドキュメント生成基盤。**
自パッケージのモジュールだけを短時間でドキュメント化し、依存ライブラリ (Mathlib) は
**再生成せず外部参照にする**。出力は doc-gen4 と同じ形の静的 HTML。

抽出器は **Lean** (`extractor/`)、その外側 — IR の消費・HTML レンダリング・増分・
全域成果物・依存写像 — は **Rust** (`crates/`)。

```sh
lean-doc build --root <あなたのパッケージ> --out <出力先> --extractor-bin <抽出器>
```

**現在: v0.1。** ゲート A (doc-gen4 出力との byte 再現) とゲート B
(第 2 の対象で 1 コマンドが通る) は通っている。**通っていないもの・測っていないものは
[未検証項目](#未検証項目) に全部書く。**

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
③**依存ライブラリを再生成せず、名前 → モジュールの写像だけ持って上流サイトへリンクする**、
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
| rev だけ変更 (Lean 起動 0 回) | **0.65 s** (433 ページ) / **0.87 s** (432 ページ) | 【実測】2 つは別の木で、ページ数が違う |

**cold と warm を混ぜて読まないこと。** olean は mmap で読むので、同じ仕事が
page cache の状態で 2 倍以上動く (環境ロード単体では warm 2.5 s ↔ cold 13 s【実測】)。

### byte 再現率 — これは**受け入れオラクル**であって製品目標ではない

同じ IR から作ったページを doc-gen4 の出力と領域単位で突き合わせた値
(`experiments/stage4c/coverage.ts`、Deno、製品外)。**再測定 2026-08-15、`lean-doc build`
の出力に対して**【すべて実測】:

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
- **静的資産 (`style.css` / `search.js` など) はこの母数の外** (→ [出力](#出力に何が入っていて何が入っていないか))。

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
| 依存写像 (`.lidx`) | **抽出器が import 済みの環境を走査して作る** — 上流サイトの配布物に依存しない |
| フル生成か増分か | `--out` に何が居るか (`lean-doc-build.json` マーカー) |

**`--out` は必須で既定なし。** 素直な既定 `<root>/.lake/build/doc` は doc-gen4 の出力木
なので、そこを既定にするのはデータ消失。`--root` の中の `--out` は exit 3、マーカーの無い
非空ディレクトリも exit 3 (**このコマンドが作ったと確認できる木しか消さない**)。

`--full` で全部作り直し。段ごとのサブコマンド (`modules` / `extract` / `site` / `render` /
`global` / `incremental` / `ledger` / `merge` / `ownership` / `impact` / `prune`) も
表に出ている — 引数なしで `lean-doc` を実行すると usage が出る。

---

## CI に置く

**`lake build` と同じジョブに置くこと。** これが第一の打ち手で、理由は Lean の速度ではなく
I/O — 同じ import が **同じジョブなら 2.61 s、別ジョブなら 20〜89 s (8〜34 倍)**
【実測、Linux ランナー → [`docs/approach.md`](docs/approach.md) §3】。決めているのは
olean が page cache に残っているかどうかで、`actions/cache` では代わりにならない
(復元されるのはディスク上のバイトで、新しいランナーの page cache は空)。

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
**差分 0**【実測】。**CI ランナーは page cache が cold 側なので、この数字は下限**
(cold がどれだけ効くかは §3 の 2.61 s / 20〜89 s が示す範囲)。
抽出器のビルドは別測定で **14.90 s wall / 10.07 s user / peak RSS 1.52 GB**【実測、warm】
— キャッシュする理由は大きさではなく、**パッケージのコミットでは変わらない**こと。

キャッシュは 3 つ、いずれも**同じジョブの中で復元して使う** (workflow のコメントに理由):
`~/.cache/mathlib` (鍵: `lake-manifest.json`)、`extractor/build` (鍵: `lean-toolchain` +
`Extract.lean` のハッシュ)、cargo + `target` (鍵: `Cargo.lock`)。

---

## 出力に何が入っていて、何が入っていないか

`<out>/site` = **モジュールページ N 本 + 全域成果物 6 本**:
`navbar.html` / `references.html` / `references.bib` / `tactics.html` /
`declarations/declaration-data.bmp` / `declarations/name-map.json`。
対象 1 では 432 + 6 = **438 ファイル**、第 2 の対象では 13 + 6 = **19 ファイル**【実測】。

**静的資産 (`style.css` / `search.js` / `jump-src.js` / `expand-nav.js` など) は生成しない。**
ページはそれらを相対パスで参照するので、**そのまま置いても見た目は付かない** —
doc-gen4 の資産を横に置く必要がある。byte 再現率の母数もこの外。**v0.1 の穴として明示する。**

---

## 未検証項目

**済んだことにしない。** ここに書いてあるものは v0.1 では測っていない / 通していない。

1. **実在の公開パッケージでの実走** — ゲート B の第 2 の対象は**合成に限る**と決めた
   (同じ Mathlib rev に固定するため)。合成の対象は境界値を**狙って**入れられる代わりに、
   **こちらが知らない形では驚かせてくれない。**
2. **GitHub Actions 実走** — workflow YAML は **YAML として構文検証しただけ**
   (Ruby の Psych でパースし、ジョブ 1 本 / ステップ 8 本を確認)。actions のバージョンと
   入力・キャッシュ鍵・elan のインストールは**実行していない**。**中身の
   `tools/ci-build.sh` はローカルで実走している。**
3. **`lake exe cache get`** — ネットワークが要るので**一度も実行していない**。
   `--cache-get` を渡した経路は未実行。
4. **`«…»` を含む依存側モジュールの `.lidx` 綴り差** — `.lidx` はモジュール名を
   **非エスケープ**で書く。href は同じパスに解決するので出力バイトには出ないが、
   **ルックアップ鍵としては別物**なので、そういうモジュールを docstring から名指しすると
   解決が外れうる。**それを名指しする docstring を持つ対象で測っていない。**
5. **「同名宣言を複数モジュールが持つ形」** — 対象 1 には 25 件あるが、第 2 の対象で
   **再現できなかった** (equation lemma を強制しても blacklist で IR に出ない)。**測れていない。**
6. **CI ランナーが cold 側かどうか** — 「同じジョブ 2.61 s / 別ジョブ 20〜89 s」は
   Linux ランナーでの実測だが、**この製品構成での CI 実走はない**。
7. **Lean のバージョン差** — 動作を確認したのは **v4.31.0 のみ**
   (Mathlib `fabf563a7c95`)。あらゆるバージョンへの後方互換は**意図的にやらない**。
8. **fenced code block 中の NUL** — MD4Lean が SIGSEGV する入力なので**合わせる相手が
   存在しない**。lean-doc は落ちないほうに倒しているが、出力には U+FFFD に加えて
   **生の NUL が 1 バイト残る**【実測】。
9. **プロトタイプ (TS) との登録済みの乖離 3 件** — CommonMark サブセット / autolink の
   `inLink` / heading id の分割表。**3 件とも lean-doc のほうが doc-gen4 と一致する側**で、
   対象 1 のバイトは動かない【実測】。ただし根拠は「**対象 1 に出現しない**」なので
   **別の対象では出うる** (heading id は第 2 の対象で実際に `id=` と `href=#` に出た)。
   詳細と母数は [`docs/implementation-plan.md`](docs/implementation-plan.md) §5。
10. **プレビューモード・テーマ・検索の依存横断** は**持たない**と決めている
    (→ [`docs/approach.md`](docs/approach.md) §9)。

---

## 文書

| | |
|---|---|
| [`docs/approach.md`](docs/approach.md) | アプローチの **SoT**。なぜこの形なのか、何を意図的にやらないか |
| [`docs/implementation-plan.md`](docs/implementation-plan.md) | 実装の **SoT**。ゲート / 移設の順序 / 各段の実測 |
| [`docs/verification-log.md`](docs/verification-log.md) | **数字の SoT**。予測と食い違ったらこちらが正 |
| [`benchmarks/`](benchmarks/) | 実測レポート・計装パッチ・生ログ。**数字の出所** |

**docs に書く数字はすべて 実測 / 外挿 / 仮定 / 理論値 のラベルを持つ** — これは
このプロジェクトの成果物が数字だから (→ `CLAUDE.md`「計測の誠実性」)。

## リポジトリの構成

```
crates/          製品コード (Rust)。ir / md / render / global / incr / CLI
extractor/       抽出器 (Lean)。build.sh が対象の lake env を借りてビルドする
tools/           ハーネスとゲート。ci-build.sh もここ
experiments/     検証段階ごとの使い捨てプロトタイプ。2026-08-11 に凍結 (読むだけ)
benchmarks/      計測レポート・計装パッチ・生ログ
docs/            上の 3 文書
```

`experiments/` を凍結しているのは、**verification-log がそこを数字の出所として
指している**ため — 書き換えると過去の実測が再現できなくなる。
受け入れオラクル `experiments/stage4c/coverage.ts` だけは今も回す (**Deno のまま製品外**。
Rust 版を採点する側なので、同じ言語で書き直すと「両方同じ間違いをする」経路ができる)。

## 計測条件

機材 **Apple M1 / 8 コア / 16 GB**、macOS。Lean・Mathlib・doc-gen4 いずれも **v4.31.0**
(Mathlib `fabf563a7c95`)。**計測対象は常に同じ Lean プロジェクトに固定**
(`InformationTheory`、432 モジュール、Mathlib 全体に依存) — **比較は同一ワークロード上でのみ
意味を持つ**ため。第 2 の対象は `tools/make-target2.sh` が同じ Mathlib rev で合成する
13 モジュール 2 ライブラリのパッケージ。
