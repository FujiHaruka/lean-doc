# Handoff — 2026-08-18 08:42

## State

- Branch: main / **clean** / push 済み (HEAD `27e6496`)。**tag `v0.1.3`**、Release 3 本 (v0.1.1〜v0.1.3)
- Active phase: **配布は完遂**。次のゴールは未定 (ユーザー待ち)
- CI は 4 ワークフロー全緑 — `ci.yml` (3 ジョブ) / `ci-action.yml` (action の自己テスト 5 ジョブ) /
  `release.yml` (tag で走る) / `ci-extractor-portability.yml` (`workflow_dispatch`、測定)
- 計測環境: 手元の `extractor/build/extract` は **micro-dep でビルドしたもの** (どの環境で
  作っても byte 同一なので区別不要)。`lean-projects` は無傷、`/private/tmp/lean-doc-relay` は掃除済み。
  **ディスク 51 Gi 空き**

## Relay control

- Mode: DONE
- Goal: 配布を完遂する (D1 検証 → D2 Release → D3 action → D4 プリビルドの可否)
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: **D1〜D4 完遂**。D4 は「配れると確認した上で配らない」判定。main の赤 (4 コミット) も修復

## Tasks

なし (Task list は使っていない)。

## Where we are

配布経路が 3 つできた — action (`uses: FujiHaruka/lean-doc@v0.1.3`)、Release のバイナリ
(Linux musl 静的 / macOS arm)、ソースからのビルド。extractor のプリビルドは**技術的には
可能**と実測で確認した上で、**226 MB vs 16 秒のビルド**という理由で配らないと決めた。
この作業で欠陥が **11 件**出たが、**product は 0 件**で全部が配布経路と検査自身だった。

## Next step

**ユーザーの指示待ち。** 手を動かすなら候補は 3 つ:

1. **Marketplace 掲載** — 要件 (public / ルートの `action.yml` / `branding`) は満たしている。
   ただし **Web UI の操作なのでユーザーの手作業**。こちらからはできない
2. **`.github/workflow-templates/lean-doc-docs.yml` の去就** — starter workflow は org の
   `.github` リポジトリ専用で、**配布経路として一度も機能していない**。action が役目を
   引き継いだので消す候補。**消す判断はユーザーのもの**
3. **他人のリポジトリから action を使う** — self-test の呼び出し側はこのリポジトリ自身なので、
   これはまだ一度も起きていない (`implementation-plan.md` §1 の未検証項目)

## Files to read first

- `docs/plans/distribution.md` — 計画の SoT。冒頭に結果表と欠陥 11 件の内訳 (241 行)
- `benchmarks/results/extractor-uniqueness-2026-08-18.txt` — X1/X3〜X6 の数字と §5 の判定
- `action.yml` — 3 つのハマりどころが全部コメントに入っている
- `.github/workflows/ci-action.yml` — 何を検査していて、なぜ 5 つに分かれているか

## Load-bearing context

- **`docs/approach.md` が 613 行**で「600 行を超えたら `/compact-plan`」の閾値を超えている。
  今回の作業では 1 行も触っていないので**手を付けなかった**。触るなら独立した作業として
- **`GITHUB_ACTION_REF` は composite action の `run` 段では空**【実測】。context 式
  `github.action_ref` には来るので env で渡す。同様に **`hashFiles()` は workspace の外を
  読めない** — action 自身の `Cargo.lock` はハッシュできない
- **`actions/cache` は同じ job で 2 回目の restore が先行ステップの出力を上書きする**【実測】。
  action は sentinel で初回だけ restore する形にしてある。**同じ形は他の action にもある**
- **`macos-13` (Intel) runner は 15 分待っても起動しなかった**【実測 2026-08-18】。
  Intel macOS 向けを出すなら、まず runner が取れるかを確かめてから
