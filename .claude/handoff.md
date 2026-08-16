# Handoff — 2026-08-16 21:00

## State

- Branch: main / **clean** / push 済み (`332c697`)
- Active phase: **M8 (UI 刷新) 完遂**。<https://fujiharuka.github.io/information-theory/> が新 UI で公開済み
- 品質: `cargo test --workspace --no-fail-fast` **355 passed / 2 failed**、clippy 0、fmt 緑。
  **赤 2 件は環境要因で直してはいけない** — 計測対象が `lakefile.toml` → `lakefile.lean`・
  依存 15 → 9 に変わったため。変更を stash して HEAD で走らせても同じ 2 件が落ちることを確認済み
- 計測環境: 計測対象 `/Users/haruka/dev/lean-projects` は**無傷** (HEAD `c4f6af29`、
  doc-gen4 の参照木 6,080 ページ健在)。ただし**別作業で動いている最中**

## Tasks

なし (M8 の 7 件はすべて完了)。

## Where we are

M8 で doc-gen4 の HTML / CSS / JS の模倣をやめ、IR はそのままで UI を自前にした。
静的資産 3 本 (`style.css` / `app.js` / `favicon.svg`) は**新規に書いたもの**で、doc-gen4 の
ファイルをコピーしてはいない。一方で**レンダラのロジックは M1〜M7 で doc-gen4 から移設したもの**で、
各 `src/*.rs` の doc comment に `Ported from` / `DocGen4/Output/*.lean` の参照が大量に残っている。
**lean-doc リポジトリに LICENSE ファイルが無い。**

## Next step

**doc-gen4 由来のスタイル / スクリプトの棚卸しと、ライセンス上の義務の確認。** 順に:

1. **由来の棚卸し** — 「コピー」「移設 (読んで書き直した)」「設計の踏襲」「無関係」の 4 段階で分類する。
   出発点は `rg -n 'Ported from|DocGen4|doc-gen4' crates/*/src/*.rs` (分布: `packages.rs` 22、
   `decl.rs` 12、`md/html.rs` 12、`md/lib.rs` 8、`autolink.rs` 7…)。
   **`crates/lean-doc-render/assets/` の 3 本は M8 で新規に書いた** — `git log --follow` で確認できる。
2. **生成物に何が残るか** — 公開サイトは public なので、ここが実際に配布しているもの。
   1 ページに `class="fn"` 84 / `class="name"` 11 / `break_within` 2 / `markdown-heading` 3 /
   `hover-link` 3【実測】。**class 名と HTML の構造は doc-gen4 の設計そのまま**
   (`code.rs` は M8 でも変えていない)。
3. **ライセンスの確認** — doc-gen4 は **Apache License 2.0**
   (`/Users/haruka/dev/lean-projects/.lake/packages/doc-gen4/LICENSE`)。Lean 4 と Mathlib も同じ。
   Apache 2.0 の義務は「ライセンス本文の同梱 / 変更の告知 / NOTICE の保持」。
   **どこまでが「派生物」か**を、上の 1〜2 の分類に照らして判断する。
4. **vendor しているもの** — `crates/lean-doc-md/vendor/md4c/` に `LICENSE.md` と
   `PROVENANCE.md` が**既にある**。ここは前例として先に読む価値がある (同じ作法を doc-gen4 にも
   適用すればよいのかもしれない)。
5. **結論を落とす先** — lean-doc の `LICENSE` / `NOTICE` を作るかどうか、
   生成サイトのフッタに表記が要るかどうか。**要らないなら「要らない」と根拠付きで書く。**

## Files to read first

- `crates/lean-doc-md/vendor/md4c/PROVENANCE.md` — **vendor した第三者コードの扱いの前例**。まずこれ
- `docs/implementation-plan.md` §6「ファイル別内訳」 — 何が移設で何が移動かの一覧
- `crates/lean-doc-render/src/decl.rs` / `frame.rs` の冒頭 — 移設の記述の典型。
  `frame.rs` は M8 で書き換えた側、`decl.rs` は「決定は doc-gen4 のもの」と明記している側
- `crates/lean-doc-render/assets/style.css` — 自前で書いた側。冒頭に設計の根拠がある
- `docs/plans/ui-redesign.md` — M8 で何をやめ何を作ったか (§8 の表)

## Load-bearing context

- **「コピーした」ものは無いはず**だが、**CSS のテクニックは 2 つ意図的に踏襲した** —
  `.fn` のハンギングインデント (`text-indent: -1ex; padding-left: 1ex`) と
  `.break_within` の `word-break` の組。どちらも `style.css` にコメントで出典を書いてある。
  「アイデアの踏襲」と「表現の複製」の線引きがここに出る
- **`experiments/` は凍結** (数字の出所)。doc-gen4 を読んで書いた TS が入っているが、
  **1 バイトも変更しない**。棚卸しの対象には入るが、修正の対象には入らない
- **`benchmarks/doc-gen4-instrumentation.patch`** は doc-gen4 のソースへのパッチ =
  **doc-gen4 の著作物の差分**。ここは他と性質が違うので分けて扱う
- lean-doc リポジトリは **private**、生成サイトは **public**。義務が発生するとしたら後者が先
- **ssh (port 22) はこの機材から通らない**【実測】。push は HTTPS + `gh` の credential helper を
  `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='!gh auth git-credential'`
  で**その 1 コマンドの間だけ**指す。git の設定を汚さないこと
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
