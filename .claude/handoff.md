# Handoff — 2026-08-11 (13)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 2 / cap 8
- Predecessor: none (leg 1 はユーザーの元セッションで tmux 名を持たない → **kill しない**)
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画を新規作成 + 未決 3 件を決定 + M0 (Rust ワークスペース) + **M1-a (IR リーダ)**
    + **M1-b (下回り)** + バイト差分オラクル構築。`26df736` → `4fdc8d4` の 8 コミット

## State

- Branch: main / **clean** / 全部コミット済み (`4fdc8d4` が HEAD)
- **検証フェーズは終わっている。いまは実装フェーズ** — `experiments/` (凍結) から
  Rust の `crates/` へ移設中
- **Rust 1.97.1 が入った** (このセッションで rustup 導入、PATH は `~/.zprofile` / `~/.profile`)。
  `cargo build` / `test` / `clippy --workspace --all-targets` / `fmt --check` が全部緑
- 計測対象 `/Users/haruka/dev/lean-projects` は **clean**、doc-gen4 の計装は APPLIED のまま。
  **この leg では Lean を 1 度も走らせていない**

## Tasks

- #3 [in_progress] **M1: レンダラを Rust に移設し 439/439 byte 一致**
  — **M1-a (IR リーダ) と M1-b (下回り) は完了**。残るのは **M1-c** と **M1-d**
- #4〜#8 [pending] M2〜M6 (依存順に blockedBy 済み)
- #1 [pending] approach.md を 600 行以下に (いま **641 行**)。`/compact-plan` の仕事

## Where we are

**M1 は 4 分割してあり (計画 §5 の表)、前半 2 つが終わった。**

| | 中身 | 状態 |
|---|---|---|
| M1-a | IR リーダ (schema 4、UTF-16 スパン) | ✅ `4eb53e7` |
| M1-b | 下回り (escape / quote / order / ws / `.lidx`) | ✅ `4fdc8d4` |
| **M1-c** | **md4c FFI + docstring の AST → HTML + autolink 解決 (153 行)** | **次はここ** |
| **M1-d** | **ページ描画と主ループ** | M1-c の後 |

**オラクルは 2 段になっている。**

1. **内側ループ** = `render.ts` の出力とのバイト差分。**参照は既に固定済み**
   (`/private/tmp/lean-doc-relay/m1/ref-pages`、432 ページ、`--link-index` 込み)。
   `tools/render-reference.sh` で再生成、`tools/render-compare.sh` で比較。
   **`render.ts` が決定的であることは実測済み** (2 回生成して 432/432 一致)
2. **ゲート A** = `coverage.ts`。**実走確認済み** — 参照ページで **99.5% / 304 of 348 ページ
   byte 一致 / 不足 108,772 B は全部 rev**

**md4c は準備済み** — `crates/lean-doc-md/vendor/md4c/` に 0.5.2 を vendor し、`build.rs` で
コンパイル・リンクまで通してテストで固定した (`5e8d0df`)。**残るのは `MD_PARSER` の
コールバック表を書いて AST を組むところ**。

## Next step

**M1-c を subagent に dispatch する。** スコープ:

1. `MD_PARSER` の構造体レイアウトと enum を `vendor/md4c/md4c.h` から起こす
   (**M1-b の下ごしらえは意図的にレイアウトを書いていない** — 間違ったレイアウトが
   たまたまリンクするのが最悪なので)
2. フラグは **`MD_DIALECT_GITHUB | MD_FLAG_LATEXMATHSPANS | MD_FLAG_NOHTML`**【実測】
3. AST → HTML は **doc-gen4 の `DocGen4/Output/DocString.lean:202-393` の移植**
   (md4c の HTML レンダラは使わない)。entity は **生のまま通す** (`:211`)
4. autolink 解決 153 行 (`render.ts:925-1077`) を移す — **`nameToLink` / `isNameLit`
   (Lean の `decodeNameLit` の移植) / `isLetterLike` / `autoLinkInline` / `headingId` / `extendLink`**

**検証は M1-b と同じ流儀を要求すること** — 期待値を `render.ts` から `deno` で生成し、
Rust 側と突き合わせ、**わざと壊して落ちることまで確認**する (mutation check)。
M1-b の `crates/lean-doc-render/tests/gen-ts-expected.ts` と `differential.rs` が手本。

その後 **M1-d** (ページ描画と主ループ) → `tools/render-compare.sh` が `IDENTICAL` を出すまで回す。

## Files to read first

- `docs/implementation-plan.md` — **実装の SoT**。§5 の M1 分割表と
  **「M1-b が元コードから掘り出した落とし穴」7 項目**は M1-c で必ず踏むので先に読む
- `crates/lean-doc-render/src/lib.rs` と `crates/lean-doc-ir/src/lib.rs` — 既にある部品の境界
- `crates/lean-doc-md/vendor/md4c/PROVENANCE.md` — 何を vendor して何を敢えて外したか
- `CLAUDE.md`「計測の誠実性」「オーケストレーション」

## Load-bearing context

- **doc-gen4 の参照木 736 MB / 6,080 ページが `lean-projects/.lake/build/doc` に健在。
  作り直すと約 9 時間。消さないこと。**
- **`experiments/` は凍結**。1 バイトも変更しない (数字の出所として verification-log が指している)
- **subagent には「コミットするな」と指示する** — 統合と commit はオーケストレータが持つ
- **同時に走らせる subagent は 1 体まで** (同時実行は使用量上限に当たってセッションごと止まる)
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない** — `&text[a..b]` を
  コンパイルエラーにするため。この防御を緩めない
- **`serde_json` の `preserve_order` は必須依存** (§7)。`indexmap` が「未使用に見える」と
  削られるとバイトが動く
- **npm/node は壊れている** (署名不正で SIGKILL)。JS が要るときは **deno**。
  `diff` は zsh で `colordiff` にエイリアス (未インストール) なので**スクリプトでは `/usr/bin/diff`**
- **`git add -f` を使わない** — `.gitignore` を無効化して 163 MB を push しかけた
- **`coverage.ts:512` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード。**
  `--source-url` にタグ名やブランチ名を渡すと**採点が静かに下がる**
