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
    **A-1 完了 `7ef9488`** (push 済み) — ゲート `tools/deps-docs-gate.sh` が両枝を通り、
    `incremental`/`ledger`/`links` に `--deps-docs-map`、実測ログ
    `benchmarks/results/deps-docs-2026-08-19.txt`。
    527 名中 524 が docs、3 がソース (doc-gen4 が公開しない再帰子)。
  - r1 続き: **A-2 完了 `19ffb9f`** + 整形 `09a39d4` (CI が `cargo fmt --all --check` で
    2 回赤くなった — 検証手順に fmt が無かった。計画 §5 を ci.yml が SoT に書き換え)。
    **束 A 完了。** watch は warm 5.5-6.2 s / cold 13.7 s、86-89% が Lean の環境ロード。
    **常駐 extractor はパスをまたげないことが実測で判明** — 計画の前提が誤りだった。
  - r1 続き: **B-0 (自動生成宣言の親子判定) 調査中** → `docs/plans/b0-generated-decls.md`

## Where we are

計画は `docs/plans/feature-sweep.md` (322 行、`561db4c`)。調査の出所は doc-gen4 の
issue 108 件 + PR。依存リンクの腐敗率は実測済み
(`benchmarks/results/deps-link-rot-2026-08-19.txt`)。

**h (`git@` URL) は既に実装済みだったので範囲外**にした (`build.rs:1069 github_path`)。

## Next step

**B-0 の結論を読んで束 B の schema 内容を確定** → c (sorry) → e (属性) → f (自動生成)。
**IR schema 4 → 5 は 1 回で上げる。**

## Files to read first

- `docs/plans/feature-sweep.md` — この作業の SoT。§3 Approach と §6 決定
- `benchmarks/results/deps-link-rot-2026-08-19.txt` — A-1 の根拠になる数字
- `crates/litedoc4-render/src/external.rs` — 依存リンクの値型。「第 3 の状態」の議論
- `crates/litedoc4-render/src/autolink.rs` の `NameIndex::link_to` — リンク 3 形の唯一の判断箇所
