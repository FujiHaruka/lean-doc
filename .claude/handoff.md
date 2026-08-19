# Handoff — 2026-08-19 (機能スイープ: doc-gen4 の要望 8 件)

## State

- Branch: main / clean / `561db4c` まで
- Active phase: `docs/plans/feature-sweep.md` の **束 A から着手**
- 計測環境: 触っていない (計装・olean 暖機とも前回のまま)

## Relay control

- Mode: ON
- Goal: `docs/plans/feature-sweep.md` の 8 項目を束 A → B → C で完遂。各項目は
  「ゲート/テスト・撤退ライン・実測ログ」の 3 点が揃うまで終わりにしない。
  新しいゲートは必ず一度落としてから通す。完了条件は計画 §5 の 6 項目すべて。
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 計画 `561db4c` / 数式クレート確定 `?` / **A-1 本体 `baab197`**
    (依存リンクを検証つきで docs へ。既定は不変、テスト 394 緑、clippy 緑)。
    残り: A-1 のゲート + 残り経路 (`incremental` / `ledger` / `links`) + 実測ログ

## Where we are

計画は `docs/plans/feature-sweep.md` (322 行、`561db4c`)。調査の出所は doc-gen4 の
issue 108 件 + PR。依存リンクの腐敗率は実測済み
(`benchmarks/results/deps-link-rot-2026-08-19.txt`)。

**h (`git@` URL) は既に実装済みだったので範囲外**にした (`build.rs:1069 github_path`)。

## Next step

A-1 の残り (ゲート `tools/deps-docs-gate.sh` / `incremental`・`ledger`・`links` への
`--deps-docs-map` / 実測ログ) を終えたら **A-2 (g) watch** へ。

## Files to read first

- `docs/plans/feature-sweep.md` — この作業の SoT。§3 Approach と §6 決定
- `benchmarks/results/deps-link-rot-2026-08-19.txt` — A-1 の根拠になる数字
- `crates/litedoc4-render/src/external.rs` — 依存リンクの値型。「第 3 の状態」の議論
- `crates/litedoc4-render/src/autolink.rs` の `NameIndex::link_to` — リンク 3 形の唯一の判断箇所
