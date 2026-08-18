# `lean-doc` → `litedoc4` 改名の記録 (2026-08-18)

**決定【ユーザー判断】**: GitHub リポジトリ・crate・CLI・Lake パッケージ名のすべてを
`lean-doc` から **`litedoc4`** に改名する。

この文書は**何を置換しなかったか**の SoT。旧名が残っている箇所を「消し忘れ」と読んで
直すと、凍結値・過去の実測・外部に実在する資産名が壊れる。

## 1. なぜ `litedoc` ではなく `litedoc4` か

`litedoc` は**実測で衝突していた**【実測 2026-08-18】:

| 衝突 | 中身 |
|---|---|
| [LiteyukiStudio/litedoc](https://github.com/LiteyukiStudio/litedoc) (9★) | **同カテゴリ** — Python モジュールの API markdown 生成器。**PyPI の `litedoc` と CLI 名を占有** |
| [nssalian/litedoc](https://github.com/nssalian/litedoc) | **Rust** — crates.io に `litedoc-core` / `litedoc-cli` を publish 済 (2026-02-06)。こちらが作る `litedoc4-ir` / `-render` と同じ付け方 |
| [0xovo/LiteDoc](https://github.com/0xovo/LiteDoc) (153★) | ブラウザ PDF→Markdown コンバータ。**検索一位を取る** |
| npm `litedoc` | 2014 年の死んだパッケージだが名前は占有 |
| GitHub 組織 `LiteDoc` | 実在 |

`litedoc4` は **crates.io / npm / PyPI / GitHub 検索すべて空き**【実測 2026-08-18】。
`doc-gen4` / `mathlib4` / `lean4` と同じく **4 が付くのが Lean 界隈の慣習**なので、
Lean 4 向けだと名前が言う。**弱点**: Lean 5 が出たら古びる (mathlib4 と同じ)。

検討して落とした `zeolite` は **3 レジストリすべて占有**だった【実測】。

## 2. 置換の規模

置換規則は 6 つ: `lean-doc`→`litedoc4` / `lean_doc`→`litedoc4` / `LEAN_DOC`→`LITEDOC4` /
`leandoc`→`litedoc4` / `LeanDoc`→`Litedoc4` / `leanDoc`→`litedoc4`。

- **183 ファイル**を書き換え、**805 ファイルは保護対象として一切触らなかった**
- crate ディレクトリ 6 つを `git mv`、`.github/workflow-templates/lean-doc-docs.yml` も改名

`Lean Doc` (2 件) は `open Lean DocGen4` という **Lean の名前空間**なので規則に入れていない。

## 3. 旧名を残した 5 種 — 直さないこと

| # | 対象 | 規模 | 残す理由 |
|---|---|---|---|
| 1 | `benchmarks/results/**` | 361 ファイル | **生ログ**。過去の実測を書き換えない (CLAUDE.md「計測の誠実性」) |
| 2 | `crates/*/tests/data/**` | 14 ファイル | **凍結フィクスチャ**。生成時のパスが焼かれていて、**再生成手段は HEAD に無い** (tag `experiments-frozen` から復元が要る)。`PROVENANCE.md` の `git show experiments-frozen:crates/lean-doc-*/…` は**タグ内の実在パス**で、改名すると解決しなくなる |
| 3 | 文字列 `lean-doc-relay` | 32 ファイル | ゲートの作業領域 `/private/tmp/lean-doc-relay/<段>`。**凍結フィクスチャに生成時のパスとして入っていて、テストの `DEFAULT_IR` がそれと一致している必要がある** |
| 4 | `lean-doc/experiments/stage4b` / `stage4c` | 6 ファイル | **プロトタイプが IR の `generator` に書いていた実在の識別子**。`ledger.rs` の `assert_ne!` は「今の ID がこれと違う」ことを検査するもので、書き換えると**実在しない文字列と比べる無意味な検査**になる。`extractor/Extract.lean` は今もこの値を書く (移植版は「ディスク上の木を書いたもの」だと主張しない) |
| 5 | v0.1.3 の資産名 `lean-doc-aarch64-apple-darwin.tar.gz` 等 | `docs/plans/lake-package.md` | **既存タグ v0.1.0〜v0.1.3 に実在する名前**。L2 の実走記録 (960,891 B、sha256 `589d2b7e…`) は改名前のもの |

**4 は一度誤って置換し、復元した。** 一括置換が「これからの識別子」と「外部に実在する物を
指す固有名詞」を区別しないことの実例で、`docs/plans/experiments-removal.md` が
「書き換えるとフィクスチャが全部落ちる」と**明記していたのに踏んだ**。

## 4. 改名が動かしたもの — 帰結

**出力・状態に名前が埋まっていた 3 箇所が変わった。** どれも意図した挙動だが、
利用者から見ると 1 回だけ挙動が変わる:

| 定数 | 旧 → 新 | 帰結 |
|---|---|---|
| `DIGEST_MARKER` (`litedoc4-render/src/external.rs`) | `lean-doc external-links v1` → `litedoc4 …` | digest が動く |
| `EXTRACTOR_ID` (`litedoc4-incr/src/ledger.rs`) | `lean-doc extractor v1` → `litedoc4 …` | **既存 ledger が失効 → 次回フル再抽出** |
| `RENDERER_ID` | `lean-doc renderer v2` → `litedoc4 …` | **同 → 次回フル再レンダリング** |
| `localStorage` キー (`frame.rs`) | `lean-doc-theme` → `litedoc4-theme` | 既存サイト閲覧者のテーマ設定が 1 度リセット |

**据え置きも選べたが変えた** — ID は「どのツールが書いたか」を言うものなので、
別名のツールが書いた台帳を自分のものとして信じるほうが危ない。

`external.rs` の digest 期待値は **`shasum -a 256` を外部オラクルとして計算し直した**。
**旧マーカーで旧期待値が完全に再現することを対照として先に確認**してから新値を採った
(コードの出力をコピーすると、正準形が壊れても通るテストになる)。

## 5. 未解決 — 改名後の最初のリリースまでの過渡状態

1. **`lakefile.lean` の段 3 (Release からの取得) は現版では機能しない** — このツリーの
   `Cargo.toml` は 0.1.3 で、**v0.1.3 の Release に `litedoc4-*.tar.gz` は無い**
   (あるのは `lean-doc-*`)。段 4 (PATH) / 段 5 (cargo build) に落ちる
2. **README の `curl … /releases/latest/download/litedoc4-x86_64-…` は今は 404**
3. **`uses: FujiHaruka/litedoc4@v0.1.3` は動くが旧名時代の action** — タグの中身は
   自己完結していて旧資産を掴むため実害は無いが、名前は食い違っている
4. **`tools/lake-download-gate.sh` (L2) は走らせられない** — 新名の資産が存在しないため。
   **改名後のリリースを打つまで L2 は未検証に戻っている**
5. 既存の URL は **GitHub のリポジトリ改名リダイレクト頼み**

→ **次のリリース (v0.1.4) を打った時点で 1〜4 を解消し、L2 を走らせ直すこと。**

## 6. 改名後に実際に通したもの【実測 2026-08-18】

| | 結果 |
|---|---|
| `cargo test --workspace` | **35 スイート緑 / FAILED 0**。落ちた 1 件は digest 期待値で、上記 §4 の手順で更新 |
| `cargo fmt --check` / `clippy -D warnings` | 緑 |
| `cargo doc --document-private-items` (`-D warnings`) | 緑 |
| `cargo machete` / `cargo deny check` | 緑 |
| `tools/corpus-gate.sh --verify-list` | **ok (21 tests)** |
| `tools/provenance-gate.sh` | **ok (27 claims)** |
| `tools/e2e-micro.sh` | **ok** — 改名後の CLI が Lean パッケージからサイトを構築 |
| `tools/lake-package-gate.sh` | **5/5 ok** — **`require «litedoc4»` と `lake script list` の `litedoc4/docs` が動く** |

未走: `tools/lake-download-gate.sh` (§5-4)、browser gate、CI 実走。
