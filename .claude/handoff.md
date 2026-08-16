# Handoff — 2026-08-16 (品質ゲート整備 完了・public 化済み)

## Relay control
- Mode: DONE
- Goal: 品質ゲート整備 (P0〜P2 + e2e 3 階層) を完遂 + リポジトリを public 化して GitHub Actions を有効にする
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: public 化 + ゲート 9 本 + CI 3 ジョブ + 実欠陥 1 件の修正。**24 commit** (`25731f2..215f5a6`)、全部 push 済み

## State

- Branch: main / **clean** / **push 済み**
- **リポジトリは public** (2026-08-16 に変更【決定、ユーザー判断】 — Actions を無料枠で回すため)
- 品質: `cargo test --workspace` **346 passed / 0 failed / 24 ignored**、clippy 0、fmt 緑【実測】
- **CI が push ごとに 3 ジョブを回して success** (test / supply-chain / e2e、**1m26〜29s**)

## What was done

計画は [`docs/plans/quality-gates.md`](../docs/plans/quality-gates.md)、結果は
[`docs/milestone-log.md`](../docs/milestone-log.md)「品質ゲート整備の結果」。

**doc-gen4 互換 (byte 再現) の代わりに置いたのは、外部オラクルを要らない 3 種**:
自己整合性 / 不変量 / Lean 自身。ゲートは 9 本:

| | |
|---|---|
| `cargo test --workspace` | 機材ゼロ依存。**緑の定義** |
| `tools/e2e-micro.sh` | **本物の Lean → 抽出器 → site**。5 ゲート (1 コマンド / 冪等 / 決定性 / `--jobs` 不変 / 仕事量) |
| `tools/site-gate.sh` | 404 = 0 / 外部リソース = 0 / 索引 ⟷ ページ双方向 |
| `tools/browser-gate.sh` | 実ブラウザ 9 検査。**UI-3 (375 px) が「未判定」から決着** |
| `tools/provenance-gate.sh` | 帰属表示 27 claims |
| `tools/corpus-gate.sh` | 対象を要する 24 本 (手動)。`--verify-list` は CI |
| `cargo-deny` | licenses / advisories / sources |
| fuzz corpus | `lean-doc-md`。MD4Lean が死ぬ 2 入力で落ちない |
| サイズ予算 | 静的資産 3 本 (`assets.rs` のテスト) |

**見つかったもの 5 件** (すべて milestone-log に記録):
1. **レンダラの実欠陥** — inductive / class_inductive の constructor がページに 1 つも描かれていなかった (修正済み + 回帰テスト)
2. **corpus 依存 24 本のうち 7 本が「フィクスチャが消えているのに緑」**
3. **ゲート自身のバグ 2 件** (自己比較・出力順依存)
4. **docs の主張 3 つが誤り** (IR 全読み「5 回」/ 集約の主張 / `pages_rendered`) → 直した
5. mutation: 今回の diff 9/9、`decl.rs` 74 中 1 missed → 塞いで 74/74

## Next step

**このスコープの残作業は無い。** 次に着手するなら:

1. **V2 (`contentHash` キャッシュ)** — **測る道具はできた** (`work.irReads`)。
   ただし **`ownership` は N+1 読みで差分側はキャッシュで消せない**ので、
   「5 回 → 1 回」の見積もりは楽観 (→ approach.md §5.6)
2. **ワークスペース全体の mutation** — **1,602 mutant / 約 80 分**。ゲートにはしない
3. **E2 (合成の第 2 の対象、`tools/target2-gate.sh`)** — Mathlib が要るので CI には載せない。
   **いま動くかは未確認** (`build-gate.sh` は gate1-site が判定不能だったので止めた)
4. 赤 2 件 (`packages::every_root_...` / `ledger::the_corpus_...`) は**対象が動いた事実の報告**。
   `cargo test` からは ignored なので緑に影響しない。母数を取り直すのは対象側が落ち着いてから

## Load-bearing context

- **`e2e/micro` の宣言を消さない。** 1 つ 1 つが「対象が持たない形」を担当している
  (`class` / `inductive` / `class inductive` / 非 `mk` ctor / 継承 field / implicit binder /
  astral 識別子 / scoped notation)。**Mathlib を足さない** — 足した瞬間 CI で回らなくなる
- **`tools/corpus-tests.txt` の `## frozen` 3 本は走らせても永久に panic する**
  (`--full` 録画の生成器が tag `experiments-frozen` の中)。ゲートは実行対象から外し、
  **外したことを毎回出力する**
- **新しいゲートは必ず一度落としてから通す。** 作った当日に「何をしても通るゲート」を 2 件作った
- **`benchmarks/tools/measure-ledger.sh` は起動時に tracked な生ログを切り詰める**【実測】。
  動作確認のつもりで実走しない
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
  **その 1 コマンドの間だけ**効く。git の設定を汚さない
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**

## Files to read first

- `docs/plans/quality-gates.md` — §3 Approach (なぜこの 3 種か) と §5 の結果表
- `CLAUDE.md`「品質ゲート」 — 運用の規律 (テストとゲートの境界 / skip で緑を返さない / 壁時計を使わない)
- `e2e/README.md` — フィクスチャが何を担当しているか
- `docs/milestone-log.md`「品質ゲート整備の結果」 — 数字と見つかった 5 件
