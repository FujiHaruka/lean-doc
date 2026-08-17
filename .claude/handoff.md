# Handoff — 2026-08-18 (配布: composite action / Rust Release / extractor プリビルドの可否)

## State

- Branch: main / clean / push 済み。tag `v0.1.0` は `117e928`
- **計画の SoT は [`docs/plans/distribution.md`](../docs/plans/distribution.md)** — 段の内訳・
  検証項目 X1〜X5・撤退ラインはすべてそこ。**この handoff に書き写さない**
- `cargo test --workspace` 355 passed / 0 failed / 21 ignored、CI 3 ジョブ緑 (leg 1 起点)
- **ディスク 52 Gi 空き**【実測 2026-08-18】。`/private/tmp/lean-doc-relay` は無い

## Relay control

- Mode: ON
- Goal: **配布を完遂する** — D1 (extractor の一意性を測る) → D2 (Rust バイナリの Release) →
  D3 (composite action) → D4 (extractor プリビルド、D1 の結果次第)。
  **「toolchain だけで一意に決まる」の検証と Lean プリビルド配布の可否検証を含む**【ユーザー明示】。
  **ディスク容量に気をつける**【ユーザー明示 → 計画 §4】
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: (進行中)

## 次の一手

**D1 の X1 から。** 手元 (macOS arm64) で `e2e/micro` / `e2e/micro-dep` / `lean-projects` の
3 環境で `extract` をビルドし、byte 比較する。3 つとも toolchain は `leanprover/lean4:v4.31.0`
で**依存セットだけが違う** (Lean core のみ / path 依存あり / Mathlib 全体) — これが
`tools/ci-build.sh` の既存の実測 (「別パッケージで byte 一致、ただし依存セットは互いのコピー」)
の穴を塞ぐ組み合わせ。

**作業領域は `/private/tmp/lean-doc-relay/dist/`、各検証の終わりに消す** (171 MB × 3)。

## Files to read first

- `docs/plans/distribution.md` — **計画の SoT**
- `tools/ci-build.sh` の見出し「WHEN THE EXTRACTOR IS BUILT」 — 既存の byte 一致の実測と、
  それが弱い証拠である理由が本人の言葉で書いてある
- `extractor/build.sh` — 2 段ビルドと `-rdynamic` の理由
- `.github/workflows/ci.yml` の `e2e-micro` ジョブ — Linux 側の検証を足す先
- `README.md` — 配布まわりの現在の記述 (「no released binary and no `cargo install` yet」)
