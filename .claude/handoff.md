# Handoff — 2026-08-18 (配布を完遂 / relay leg 1 で DONE)

## State

- Branch: main / clean / push 済み。**tag `v0.1.3`** (Release 3 本: v0.1.1 / v0.1.2 / v0.1.3)
- **配布経路が 3 つできた**:
  - **action** — `uses: FujiHaruka/lean-doc@v0.1.3` (利用側は 5 行)
  - **Release のバイナリ** — `x86_64-unknown-linux-musl` (静的) / `aarch64-apple-darwin`
  - ソースからのビルド (従来どおり)
- CI は 4 ワークフロー: `ci.yml` / `ci-action.yml` (action の自己テスト 5 ジョブ) /
  `release.yml` (tag で走る) / `ci-extractor-portability.yml` (`workflow_dispatch`、測定)
- **ディスク 51 Gi 空き**【実測】。作業領域は掃除済み

## Relay control

- Mode: DONE
- Goal: 配布を完遂する (D1 検証 → D2 Release → D3 action → D4 プリビルドの可否)
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: **D1〜D4 すべて完遂**。D4 は「**配れると確認した上で配らない**」判定。
    途中で main の赤 (4 コミット) を修復。commit は `048c517`〜`v0.1.3` の 15 本ほど

## この leg で出たもの

**欠陥 11 件。product は 0 件で、全部が配布経路か検査自身**
(内訳の表は [`docs/plans/distribution.md`](../docs/plans/distribution.md) 冒頭)。
**最初から緑だったものは 1 つも無い。**

**extractor のプリビルド配布は「可能だが割に合わない」**【実測】 — toolchain だけで決まり
(依存 0〜15 個で byte 一致)、可搬で (`ELAN_HOME` を変えても動く)、不一致では
`incompatible header` で落ちる。**それでも 226 MB を配って 16 秒のビルドを消すのは合わない**。
将来やるときの手順は `ci-extractor-portability.yml` が持っている。

## 次に手をつけるなら

**ユーザーの指示待ち**。候補:

- **Marketplace 掲載** — `uses:` は掲載なしで動いているが、掲載すると検索で見つかる。
  **Web UI の操作 (Release 編集画面のチェックボックス) が要るのでユーザーの手作業**。
  要件 (public / ルートの `action.yml` / `branding`) はすべて満たしている
- **他人のリポジトリから使われる**のはまだ一度も起きていない (self-test の呼び出し側は
  このリポジトリ自身) → `implementation-plan.md` §1 の未検証項目に残してある
- **Mathlib 依存の実在パッケージでの実走** (前 leg からの持ち越し)
- 可動 tag `v0` を作るか — 今は作っていない (README が「tag に pin しろ」と書いているため)

## Files to read first

- `docs/plans/distribution.md` — **計画の SoT**。冒頭に結果表と欠陥 11 件の内訳
- `benchmarks/results/extractor-uniqueness-2026-08-18.txt` — X1/X3〜X6 の数字と §5 の判定
- `action.yml` — `hashFiles()` が workspace 外を読めない件、cache の sentinel、
  Release 取得を「ref とツリーの版が一致するときだけ」にした理由
- `.github/workflows/ci-action.yml` — 何を検査していて、なぜ 5 つに分かれているか
