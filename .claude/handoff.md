# Handoff — 2026-08-12 (17)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 6 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画 + 未決 3 件 + M0 + **M1-a** + **M1-b** + バイト差分オラクル。`26df736` → `4fdc8d4`
  - r2: **M1-c 完遂** (md4c FFI + AST / AST → HTML / autolink)。`a605256` → `2a8cebe`
  - r3: **M1 完遂** (d1/d2/d3) + docs 圧縮 + **M2-a**。`418938d` → `5965298`
  - r4: **M2 完遂** + **M3-a**。`f811724` → `0e90382`
  - r5: plan 圧縮 (617→449) + **M3-b** + **M3-c**。`3e1e239` / `adf0380` / `5086353`

## State

- Branch: main / **clean** / push 済み (`5086353` が HEAD)
- `cargo test --workspace --no-fail-fast` **216 本** / clippy 警告 0 / fmt 緑
- **M3 は a/b/c 通過、残りは M3-d のみ** (パイプライン + M3 のゲート)
- 計測対象 `/Users/haruka/dev/lean-projects` は clean、doc-gen4 の計装は APPLIED。
  **参照木 736 MB 健在**。この leg でも Lean はビルドしていない
- 揮発 fixture 健在: `/private/tmp/lean-doc-relay/` の `w7h/{base-ir,base-state,base-ledger.json}` /
  `m1/ref-pages` / `m2/{ref-global,gate/ref-site}` / `w7c/linkindex/link-index.lidx`

## Tasks

- #1 [in_progress] **M3** — 残りは **M3-d** のみ
- #2〜#4 [pending] M4〜M6

## Next step

**M3-d を着手する前に、ゲートの実行形を決める** — これが最初の一手。計画のゲートは
「本物の移動と本物の削除を `lake build` ごと回してフルビルドと byte 一致」だが、
**この leg まで `lean-projects` では `lake build` を一度も走らせていない**。

推奨: **リーフモジュールを 1 つ選んでソースを編集 → `lake build` (増分。`lake update` は絶対に走らせない)
→ 増分パイプライン → 同じ最終状態からフル生成 → byte 比較 → ソースを元に戻す**。
`.lake/build/doc` (736 MB / 約 9 時間) には触れない。commit しない。
リーフを選ぶのは、依存の多いモジュール (例 `InformationTheory.Shannon.Bridge` は 49 モジュールが参照)
だと再ビルドが重いため。**この判断を最初に確定してから dispatch すること。**

その後 **M3-d を subagent に dispatch**。移設元は `stage7h/incremental.sh` (441) と
`stage7h/run.sh` の `render()` 部。**先に §7「M3-d が引き取る負債」5 件を読ませる**。

## Files to read first

- `docs/implementation-plan.md` — §7 の **「M3-d が引き取る負債」5 件** (M3-c で判明した結線の穴)、
  §6「段の順序 6 制約」、§5 の M3 / M3-d の穴
- `crates/lean-doc-incr/src/{ledger,detect,ownership,merge,impact,prune}.rs` — M3-d が並べる 6 段
- `crates/lean-doc/src/main.rs` — CLI の現在形 (`render` / `global` / `ledger` / `ownership` /
  `merge` / `impact` / `prune`)
- `tools/{ledger,merge,impact}-{reference,compare}.sh` — **シナリオを 1 箇所で定義して両実装に回す**形

## Load-bearing context

- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と `lean-projects` の無傷確認 +
  **成果物を自分で作り直してバイト比較** + **mutation を 1 件スポット再現** + **母数を独立に再計算**。
  r5 では 2 体の報告が全部検算に耐えた (母数 4,051 と 3,631 を独立に再現)
- **`--source-url` をシェル変数で渡すとき引用符を剥がす** — `"..."` ごと渡すと 432 ページ全部が差分になる
- **`w7h/base-pages` は参照に使えない** — `run.sh` の `render()` が `--link-index` を渡していない。
  ページの参照は `m1/ref-pages`、サイト全体の参照は `m2/gate/ref-site` (438)。
  **M3-d はこの罠を 3 度目に踏む位置にいる** — 移設時に既定引数として型で固定して構造的に閉じる
- **「全件バイト一致」は分岐被覆の証明ではない**。全件比較をすり抜ける mutation は M2-b 8/8、M3-a 6/7、
  M3-b 4/8、M3-c 6/13。**分岐発火回数を先に測ってから**一致を主張する
- **M3-b で増分 IR は from-scratch IR とバイト一致するようになった** (何も変わらない再抽出で 436/436)。
  merge が書くものはもう 1 つもプロトタイプの順序ではない → **M3-d のゲートは IR 層でも判定できる**
- **移設元のコメントを根拠にしない** — `merge-ir.ts:44` の「Lean の外では再現できない」は事実として誤りだった
- **`cargo test --workspace` は `--no-fail-fast` を付ける**
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている**。JS は **deno**。`diff` は zsh で `colordiff` エイリアス (未インストール) なので
  スクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**
- **`serde_json` の `preserve_order` と `sha2` の `features = ["asm"]` は必須依存**
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない**。この防御を緩めない
