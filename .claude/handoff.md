# Handoff — 2026-08-16 (21)

## Relay control
- Mode: **DONE**
- Goal: **UI 完全刷新 (M8)** — doc-gen4 模倣をやめ、IR はそのままで HTML / CSS / JS を自前にし、
  information-theory の GitHub Pages を新 UI で再ホストする
- Leg: 1 / cap 8   # 1 leg で完遂
- Predecessor: none
- Stop-on: **completion**
- Progress ledger:
  - r1: **M8 完遂** — 計画 `fc607ef` / プロトタイプ `690657b` / M8-a+b `c641097` /
    publish スクリプト `c73b387` / M8-d `80b8c81` / docs `aa596c7`〜`610c088` /
    UI-2 `5cc5078` / M8-e `19158be` / ゲート結果 `34e2170`

## 到達点 — M8 は 5 段とも通っている【すべて実測 2026-08-16】

**公開済み: <https://fujiharuka.github.io/information-theory/>** (gh-pages `aae92b9`、rev `c4f6af29`)

| ゲート | 結果 |
|---|---|
| **UI-1 自己完結** | **通過** — 432 ファイルに外部の `<script src>` / `<link href>` が **0 本**。CDN 4 本 (Lato / JuliaMono / polyfill / MathJax) が消えた |
| **UI-2 リンク** | **通過** — 内部リンク **31,265 本で 404 が 0** (直前は 160) |
| **UI-3 モバイル** | **未判定** — CSS はそう書いたが**実ブラウザで描画を見ていない** |
| **UI-4 再ホスト** | **通過** — 新 UI の 11 パスが全部 200、**doc-gen4 の資産 16 本が全部 404** |

`lean-doc build` **24.51 s / 422 モジュール / 4,394 宣言**【実測、warm】。
成果物は 1,877,124 → **1,201,009 B (−36%)**。gzip 後: CSS 6,159 / JS 6,001 / 検索索引 49,193 B。

**ゲート A (doc-gen4 との byte 再現) はここで完全に終わった。** 99.5062% は M6 時点の到達点として残置。

## State

- Branch: main / clean / **push 済み** (HTTPS 経由。下記)
- 品質: `cargo test --workspace --no-fail-fast` **355 passed / 2 failed**、clippy 0、fmt 緑
- **赤 2 件は環境要因で、直してはいけない**:
  `packages::every_root_matches_doc_gen4s_own_blob_urls` と
  `ledger::the_corpus_matches_the_prototype`。計測対象が**このセッション中に** `ca4fd931` →
  `c4f6af29` へ進み、`lakefile.toml` → `lakefile.lean`、**依存 15 → 9**、モジュール 432 → 422 に
  変わったため。**変更を stash して HEAD で走らせても同じ 2 件が落ちることを確認済み。**
  → **テストを弱めない。対象側が落ち着いてから母数を取り直す。**
- `experiments/` は 1 バイトも動いていない
- 計測対象 `/Users/haruka/dev/lean-projects` は**無傷** (未追跡 5 件はユーザー自身のベンチログ、
  HEAD 不動、doc 参照木 6,080 ページ健在)

## 次にやるなら

1. **実ブラウザで見る (UI-3)** — 375 px の見え方 / フォントのフォールバック (**非 ASCII 178 種**) /
   ダークモードの実際の色 / スクリーンリーダー。**どれも未確認**で、README 未検証 14〜17 に書いてある
2. **赤 2 件の母数を取り直す** — 対象の v1 配布準備が落ち着いてから
3. **`prune --ir` の代償が変わった** — 孤児掃除が消すものが「誰も読まない `navbar.html`」から
   **「サイトの入口 4 本」**になった。パイプラインは `--ir` を渡さない設計なので現状は無害
4. **`lean-doc serve`** — `file://` でツリーが出ない (決定 4 で**捨てた**) のはこれで解ける

## Load-bearing context (次に触る人が踏む罠)

- **ssh (port 22) はこの機材から通らない**【実測】。push は HTTPS + `gh` の credential helper を
  `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='!gh auth git-credential'`
  で**その 1 コマンドの間だけ**指す。**git の設定を汚さないこと**
- **資産の SoT は `crates/lean-doc-render/assets/`。** `design/preview/*.html` はそれを相対参照する
  だけで、**CSS / JS の 2 本目を持たない**。プレビューの単体ファイル化は `design/preview/bundle*.py`
- **`assets.rs` のテストが「emit する class ⊆ style.css のセレクタ」を縛っている。**
  class 名を変えると**ブラウザではなく `cargo test` が落ちる** — これは意図した設計
- **`lean-doc site` は資産を書かない。`build` だけが書く** — `site` = render + global という
  構造的主張 (`tests/site.rs`) を守るため。**手で木を作るときは `cp assets/* <site>/` が要る**
- **`--lib` は `lakefile.lean` のプロジェクトで必須。** CLI は「Lean コードであってデータではない」
  と言って止まる (推測しない設計)
- **`instancesFor` は IR の `instTypes` を読むだけ** — 計画の「型に現れる全定数」という仮説は
  **59 件中 0 件しか一致せず否定された**。規則は「クラス適用の明示引数それぞれの先頭定数」で、
  印字済みトークンからは復元できない。`STATE_DERIVATION` は v2
- **docstring のソースパスは known module への接尾辞マッチで解決する。曖昧なら黙ってリンクしない** —
  この world には 2 つ以上のモジュールに属する接尾辞が **479 種 (2.6%)** ある
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている**。JS は **deno**。`diff` はスクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -A` を使わない**

## Files to read first

- `docs/plans/ui-redesign.md` (250) — M8 の**技術前提と決定 1〜6**、ゲート結果、§8 の成果物表
- `docs/milestone-log.md` (820) — M8 の結果 (母数の移動 / `instancesFor` / UI-2 / 再ホスト)
- `crates/lean-doc-render/assets/{style.css,app.js}` — UI の実体。**ここが SoT**
- `crates/lean-doc-render/src/{frame,page,decl}.rs` — HTML の構造。`design/preview/module.html` と対
- `tools/publish-pages.sh` — gh-pages への公開 (既定は dry run)
- `benchmarks/tools/check-dead-links.py` — UI-2 の測り方
