# experiments — 凍結 (2026-08-11)

検証段階 1〜8 の使い捨てプロトタイプ。**製品ツリー (`crates/`) ができた時点で凍結した。**

## 凍結の意味

- **変更しない。** ここの数字は `docs/verification-log.md` が出所として指しているので、
  書き換えると過去の実測が再現できなくなる
- **移設元としてのみ読む。** 何がどこにあり、どれが最新版かは
  [`docs/implementation-plan.md`](../docs/implementation-plan.md) §6 の表が持つ
- **新しい検証段階が要るなら、従来どおり新しいディレクトリを足す** (既存を壊さない)

## 凍結時点で「最新版」だったもの

| 成果物 | パス |
|---|---|
| 抽出器 (Lean) | `stage7d/Extract.lean` |
| レンダラ | `stage7d/render.ts` |
| 依存写像の構築 | `stage7d/build-link-index.ts` |
| 全域成果物 | `stage7h/global.ts` |
| 増分 5 本 | `stage5/{ledger,ownership,merge-ir,impact,prune-pages}.ts` |
| パイプライン | `stage7h/{run,incremental}.sh`、`stage7g/{extract-once,serve-ctl}.sh` |
| 受け入れオラクル | `stage4c/coverage.ts` (**製品外に残す。Deno のまま**) |
| 増分の状態オラクル | `stage7h/oracle.sh` (7 状態) |

同名ファイルが複数の stage にあるのは写しで、**どれが最新かは上の表と実装計画 §6 が SoT**。
