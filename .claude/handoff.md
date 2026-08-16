# Handoff — 2026-08-16 16:05

## State

- Branch: main / **clean** / push 済み (`7825916`)
- Active phase: **`experiments/` のクリーンアップ**。そのために **採点器 (reference oracle) 自体を見直す**
- 品質ベースライン: `cargo test --workspace --no-fail-fast` **355 passed / 2 failed**、clippy 0、fmt 緑、
  抽出器の Lean ビルドも通る (`extractor/build.sh` EXIT=0)。
  **赤 2 件は環境要因で直してはいけない** — 計測対象の依存が 15 → 9 に変わったため
  (`packages::tests::every_root_matches_doc_gen4s_own_blob_urls` と `the_corpus_matches_the_prototype`)
- 計測環境: 計測対象 `/Users/haruka/dev/lean-projects` は無傷。**doc-gen4 の計装は当たったまま**
  (`benchmarks/tools/apply-instrumentation.sh --check` → `APPLIED (v4.31.0)`)【実測】

## Tasks

なし (ライセンス対応は完了、3 コミット push 済み)。

## Where we are

ライセンス対応は決着した — 全体を doc-gen4 の派生物として扱い Apache-2.0 に統一、
`LICENSE` / `NOTICE` / `docs/provenance.md` を新設し、逐字コピー 6 箇所と移設 9 ファイルに帰属を書いた。
残った唯一の未決が **`experiments/` を公開するか** で、**ユーザーの決定は「公開しない、クリーンアップする」**。
ただし調査の結果、`experiments/` は凍結された過去ではなく **いまも回っている採点側** だと判明した。

## Next step

**採点器の見直しから入る。`experiments/` の削除はその後。** 順に:

1. **採点器の棚卸し** — `experiments/` に依存している実行系は次の 9 本【実測】:
   `tools/{render,global,merge,impact,ledger,incremental}-reference.sh`、`tools/{build,clone}-gate.sh`、
   `.github/workflows/ci-import-prefetch-narrow.yml` (`experiments/stage4b/build.sh` を直接叩く)。
   参照先は `stage7d/render.ts` / `stage7h/{global.ts,incremental.sh}` /
   `stage5/{ownership,merge-ir,impact,prune-pages,ledger}.ts` / `stage5e/setup-clone.sh` / `stage7g/extract-once.sh`。
   出発点: `rg -n 'experiments/' crates/ tools/ extractor/ .github/ --glob '!*.md'`
2. **各採点器を「まだ意味があるか」で仕分ける** — 判断軸は
   **「Rust 版を独立に採点できるか」**。同じ言語・同じ設計で書き直すと
   「両方同じ間違いをする」経路ができるので、単純な Rust 移植は採点器にならない
   (README の `stage4c/coverage.ts` の記述がこの原則の出所)
3. **フィクスチャで代替できるものを見分ける** — `crates/*/tests/data/*-expected.json` は
   既に凍結プロトタイプの出力を固定したもの。**採点器が「フィクスチャ生成器」でしかないなら、
   フィクスチャさえ残れば `experiments/` 側は消せる**。逆に実行時に走らせているものは消せない
4. **削除の実行と、壊れる参照の始末** — docs (`verification-log` / `milestone-log` / `approach`) から
   `experiments/` を指している数字の出所を、フィクスチャ or benchmarks/results に付け替える
5. **`docs/provenance.md` §8 の前提を確定させる** — いま「`experiments/` は配布物に入らない前提」と
   書いてある。決着したら書き換える

## Files to read first

- `docs/provenance.md` §8 — `experiments/` を棚卸しの対象外にしている前提が書いてある。まずこれ
- `experiments/README.md` — 何が凍結されていて、なぜかの一次資料
- `tools/clone-gate.sh` (`:146-151`) — 採点器を 6 本まとめて参照している最大の依存元
- `README.md` の「リポジトリの構成」節 — `stage4c/coverage.ts` を今も回す理由が書いてある
- `crates/lean-doc-{md,render}/tests/data/PROVENANCE.md` — どのフィクスチャが何の出力かの一覧 (2026-08-16 新設)

## Load-bearing context

- **削除は目的を達成しない**【重要】 — HEAD から消しても **git 履歴に残る**ので、
  リポジトリを public にした時点で `experiments/` は見える。隠すには `filter-repo` で
  履歴を書き換えるしかなく、全コミットのハッシュが変わる。
  **「公開しない」を本当に満たすなら、リポジトリを private のままにして公開は部分的に出す方が確実。**
  ユーザーはこの事実を提示された上で「クリーンアップする」と決めた
- **法的な理由では消さなくてよい** — 全体が Apache-2.0 + `NOTICE` 済なので、
  `experiments/` を公開するコストは記述 1 本。**削除の理由は方針であって法務ではない**
- CLAUDE.md と README には **「`experiments/` は 1 バイトも変更しない」** と書いてある。
  削除はこの規則の変更にあたるので、**両方を同じコミットで直す**
- `experiments/stage4c/coverage.ts` は受け入れオラクルとして**今も回している** (README に明記)
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
- **ssh (port 22) はこの機材から通らない**【実測】。push は
  `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='!gh auth git-credential' git push https://github.com/FujiHaruka/lean-doc.git main:main`
  で**その 1 コマンドの間だけ** credential helper を指す。git の設定を汚さないこと
