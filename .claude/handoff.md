# Handoff — 2026-08-12 (16)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 5 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画 + 未決 3 件 + M0 + **M1-a** + **M1-b** + バイト差分オラクル。`26df736` → `4fdc8d4`
  - r2: **M1-c 完遂** (md4c FFI + AST / AST → HTML / autolink)。`a605256` → `2a8cebe`
  - r3: **M1 完遂** (d1/d2/d3) + docs 圧縮 + **M2-a**。`418938d` → `5965298`
  - r4: **M2 完遂** (M2-b1 キャッシュ + delta、V6 是正、ゲート後半 438/437) + **M3-a**。
    `f811724` → `0e90382` の 5 コミット

## State

- Branch: main / **clean** / push 済み (`0e90382` が HEAD)
- `cargo test --workspace --no-fail-fast` **210 本** / clippy 警告 0 / fmt 緑
- **M2 は完遂しゲートを両方通過** — 6 成果物 1,877,124 B バイト一致、**サイト 438 中 437 一致**
  (差分 1 = M1 で登録済みの CommonMark 乖離)
- **M3-a 完了** — `detect` (台帳)。TS 版と 78 ファイル一致
- 計測対象 `/Users/haruka/dev/lean-projects` は clean、doc-gen4 の計装は APPLIED。
  **参照木 736 MB 健在** (作り直すと約 9 時間)。この leg でも Lean はビルドしていない
- 揮発 fixture 健在: `/private/tmp/lean-doc-relay/` の `w7h/{base-ir,base-state,base-ledger.json}` /
  `m1/ref-pages` / `m2/{ref-global,gate/ref-site}` / `w7c/linkindex/link-index.lidx`

## Tasks

- #1 [completed] M2
- #2 [in_progress] **M3** — **M3-a 完了**。残りは **M3-b / M3-c / M3-d**
- #3〜#5 [pending] M4〜M6

## Where we are

M2 は完遂。M3 は約 1,760 行あるので **4 分割**した:

| | 中身 | 状態 |
|---|---|---|
| **M3-a** | `detect` (`stage5/ledger.ts` 422) | **完了** (`0452ef5`) |
| **M3-b** | `ownership` (219) + `merge` (`merge-ir.ts` 307) | 次 |
| **M3-c** | `impact` (231) + `prune` (`prune-pages.ts` 138) | |
| **M3-d** | パイプライン (`incremental.sh` 441 + `run.sh` の `render()`) + **M3 のゲート** | |

## Next step

**まず `/compact-plan`** — `docs/implementation-plan.md` が **617 行**で CLAUDE.md の
600 行の閾値を超えている。M1 完了段と M2-a の検証経緯は畳んでよい (git が持つ)。
**仮説と、それを否定する条件は残す**。

その後 **M3-b を subagent に dispatch**。移設元は `experiments/stage5/{ownership,merge-ir}.ts`。
**`merge-ir.ts:222-227` の既知の非一致**を消すか放置するかを決める段でもある (→ 計画 §7 末尾) —
消すと「増分 IR と from-scratch IR が完全に byte 一致する」という強い不変条件が手に入る。

## Files to read first

- `docs/implementation-plan.md` — §5 (M3 の穴)、§6 (段の順序 6 制約 / データ形式)、§7 の
  **M2-b / M3-a の結果**と末尾の「既に byte 一致していない箇所が 1 つある」
- `crates/lean-doc-incr/src/{ledger,detect}.rs` — M3-a。M3-b が隣に付く
- `crates/lean-doc-incr/tests/ledger.rs` — **分岐台帳と手書きケースの家の作法**
- `tools/ledger-{reference,compare}.sh` — **12 シナリオを 1 箇所で定義して両実装に回す**形

## Load-bearing context

- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と `lean-projects` の無傷確認 +
  **成果物を自分で作り直してバイト比較** + **mutation を 1 件スポット再現** + **母数を独立に再計算**。
  r4 では 3 体すべての報告が検算に耐えた
- **`--source-url` をシェル変数で渡すとき引用符を剥がす** — r4 で `"..."` ごと渡して
  432 ページ全部が差分になった。剥がすと差分 1 に戻る
- **`w7h/base-pages` は参照に使えない** — `run.sh` の `render()` が `--link-index` を渡していない。
  ページの参照は `m1/ref-pages`、サイト全体の参照は `m2/gate/ref-site` (438)
- **「全件バイト一致」は分岐被覆の証明ではない**。M2-b は mutation 8 件中 **0 件**、M3-a は 7 件中
  **1 件**しか全件比較で捕まらなかった。**分岐発火回数を先に測ってから**一致を主張する
- **隣のパッケージは安い**【M3-a で実証】 — 対象が構造的に持たない形 (module system の 3 ファイル olean)
  を Mathlib 8 モジュールで埋めたら、バイトで捕まる変異が 1 → 2 件に増えた。M3-b でも使える
- **鍵文字列の置換は鍵名でアンカーする** — `extractKey.irGenerator` は `extractor` と同一文字列だが
  **更新してはいけない**方 (→ 計画 §6)
- **`cargo test --workspace` は `--no-fail-fast` を付ける**
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている** (署名不正で SIGKILL)。JS は **deno**。`diff` は zsh で
  `colordiff` にエイリアス (未インストール) なのでスクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**
- **`lean-projects` に書かない** — `lake build` / `lake update` を走らせない
- **`coverage.ts:512` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード**
- **`serde_json` の `preserve_order` は必須依存**。**`sha2` の `features = ["asm"]` も必須**
  (無いと同じ仕事が 2.86 倍遅く、プロトタイプとの比較が成立しない)
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない**。この防御を緩めない
