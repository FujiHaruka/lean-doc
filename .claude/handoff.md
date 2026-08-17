# Handoff — 2026-08-18 (配布: composite action / Rust Release / extractor プリビルドの可否)

## State

- Branch: main / push 済み。**tag `v0.1.1`** (打ち直した — 最初の tag は publish が
  `gh` を git 木の外で叩いて落ちたため。Release が無い状態だったので番号は再利用した)
- **計画の SoT は [`docs/plans/distribution.md`](../docs/plans/distribution.md)**、
  数字の SoT は [`benchmarks/results/extractor-uniqueness-2026-08-18.txt`](../benchmarks/results/extractor-uniqueness-2026-08-18.txt)
- `cargo test --workspace` 緑、`provenance-gate` ok (27 claims)
- **ディスク 51 Gi 空き**。作業領域 `/private/tmp/lean-doc-relay/dist` は掃除済み (ログのみ)

## Relay control

- Mode: ON
- Goal: **配布を完遂する** — D1 (extractor の一意性) → D2 (Rust Release) → D3 (action) →
  D4 (プリビルドの可否)。**ディスクに気をつける**【ユーザー明示】
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: **D1 完遂** (X1/X3/X4/X5/X6 すべて実測)、**D2** (`release.yml` + tag v0.1.1)、
    **D3** (`action.yml` + `ci-action.yml` 4 ジョブ緑)、**D4 は「配れるが配らない」判定**。
    途中で **main が 4 コミット赤かったのを修復** (`b91df79`)。
    主な commit: `048c517` `93ae676` `09648ac` `dd8df60` `c3294dd` ほか

## 次の一手

**残っているのは 1 つだけ** — action が **Release からバイナリを取る経路**が一度も実走
していない (ここまでは常に cargo build にフォールバックしていた)。`ci-action.yml` に
`uses: FujiHaruka/lean-doc@v0.1.1` の job を足して `workflow_dispatch` で 1 回回す。
それが緑なら配布は完遂。

## この leg で出たもの

**「測っていないのに緑」が今日 4 回出た** (CLAUDE.md「品質ゲート」に一般形を追記済み):

1. `provenance-gate` が日本語の「派生物」を探していて、README 英語化で **4 コミット赤**
2. `extractor-mismatch.sh` が **extract を起動できていないのに「拒否された」と緑**
3. 同スクリプトが CI で、**lake の package 解決の失敗を toolchain 不一致の答えと誤読**
4. action の増分ゲートが **`pagesRendered` を推測した名前で探し、何も検査せず緑**

4 を厳密にしたら **action の実欠陥が出た**: 同じ job で 2 回呼ぶと 2 回目の
`actions/cache` restore が 1 回目の出力を**古い commit の状態で上書き**していた。
product は無傷 (手元で 2 回走らせると `pagesRendered: 0`)。

## Files to read first

- `docs/plans/distribution.md` — 計画の SoT。D4 の判定 (配れるが配らない) の根拠
- `benchmarks/results/extractor-uniqueness-2026-08-18.txt` — X1/X3〜X6 の数字全部
- `action.yml` の `keys` ステップ — `hashFiles()` が workspace 外を読めない件と
  sentinel による cache ガード
- `.github/workflows/release.yml` — アセット名に版を入れない理由 (`latest/download`)
