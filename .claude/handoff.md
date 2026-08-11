# Handoff — 2026-08-11 (14)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 3 / cap 8
- Predecessor: leandoc-v01-r2
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画を新規作成 + 未決 3 件を決定 + M0 + **M1-a** + **M1-b** + バイト差分オラクル。
    `26df736` → `4fdc8d4` の 8 コミット
  - r2: **M1-c を完遂** (md4c FFI + AST / AST → HTML / autolink 解決)。
    `a605256` → `2a8cebe` の 4 コミット

## State

- Branch: main / **clean** / push 済み (`2a8cebe` が HEAD)
- `cargo test` 103 本 / `clippy --workspace --all-targets -D warnings` / `fmt --check` が全部緑
- 計測対象 `/Users/haruka/dev/lean-projects` は clean、doc-gen4 の計装は APPLIED のまま。
  参照ページ 432 枚 (`/private/tmp/lean-doc-relay/m1/ref-pages`)、base IR
  (`.../w7h/base-ir`)、doc-gen4 参照木 736 MB は健在。**この leg では Lean をビルドしていない**

## Tasks

- #2 [in_progress] **M1** — M1-a/b/c 完了。**残りは M1-d だけ**
- #1 [pending] `approach.md` を 600 行以下に (**641 行**)。`implementation-plan.md` も **568 行**で
  M1-d の追記で超える → **M1 のゲートを通した後に `/compact-plan` で両方**
- #3〜#7 [pending] M2〜M6 (依存順に blockedBy 済み)

## Where we are

**M1 は 4 分割の 3 つまで終わった。** docstring 層は 3 段のオラクルで固まっている —
doc-gen4 本体 (`docStringToHtml` を対象環境で実走、4,987/4,987) / TS 版 (`render.ts` に本物の
`.lidx` を食わせる、実 docstring 4,857/4,857) / 参照ページ 432 枚 (アンカー 5,498 本)。
**残る乖離は既知の 1 件だけ** (壊れた入れ子バッククォートの code span。**md4c 側が正**)。

## Next step

**M1-d を subagent に dispatch する。** `render.ts` の残り (宣言・署名・equations・members・
instances・出力・主ループ) を `lean-doc-render` へ。**ゲートは `tools/render-compare.sh` が
`IDENTICAL` を出すこと**、その後 `coverage.ts` でゲート A を採点 (**rev 置換後の木に対して**)。

M1-d の落とし穴は計画 §5 に集約済み。特に **`suppressed` は全モジュール横断の集合**
(`render.ts:2043-2048`) / **`--only` は空集合と未指定を型で区別** (空 = 何も描かない) /
**`Member.isDirect` の既定値が TS と Rust で逆** / **`decl_header` は `div.decl_type` を入れ子に持つ**。

## Files to read first

- `docs/implementation-plan.md` — §5 の M1 分割表と落とし穴、§7 の「Rust の既定が壊す箇所」(U1/U2)
- `crates/lean-doc-render/src/lib.rs` — 既にある部品の境界
- `crates/lean-doc-md/src/html.rs` — docstring 側の入口 (`LinkResolver` 注入点)
- `tools/render-compare.sh` — 内側ループのオラクル

## Load-bearing context

- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と `lean-projects` の無傷確認 +
  **mutation を 1 件スポット再現**。r2 では subagent の報告に**母数の取り違えが 1 件**あった
  (分子と母数が別コーパス) — **数字は母数まで検算する**
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **`/private/tmp/lean-doc-relay/` の全件 fixture は揮発する**。消えたら
  `crates/*/tests/oracle/gen-*.ts --full <path>` で再生成 (対象リポジトリが要る)。
  リポジトリに入っているのは数百件に絞った fixture (各 1 MB 未満) という流儀
- **npm/node は壊れている** (署名不正で SIGKILL)。JS は **deno**。`diff` は zsh で
  `colordiff` にエイリアス (未インストール) なのでスクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**
- **doc-gen4 の参照木 736 MB / 6,080 ページを消さない** (作り直すと約 9 時間)
- **`coverage.ts:512` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード** —
  `--source-url` に 40 hex 以外を渡すと**採点が静かに下がる**
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない**。この防御を緩めない
- **`serde_json` の `preserve_order` は必須依存** — `indexmap` が「未使用に見える」と削られるとバイトが動く
