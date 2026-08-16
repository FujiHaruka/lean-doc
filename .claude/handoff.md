# Handoff — 2026-08-16 (experiments/ 撤去 完了、push だけ残)

## State

- Branch: main / **clean** / **push できていない** (下記 Blocker)
- ローカル commit 5 本が未 push: `a15addc` → `c179373` → `c013122` → `0dcb3a7` → `34a8861`
- **tag `experiments-frozen` は push 済み** (`a15addc` を指す)
- 品質: `cargo test --workspace --no-fail-fast` **355 passed / 2 failed**、clippy 0、fmt 緑【実測】
  **赤 2 件は環境要因で直してはいけない** — 計測対象の依存が 15 → 9 に変わったため
  (`packages::tests::every_root_matches_doc_gen4s_own_blob_urls` と `ledger::the_corpus_matches_the_prototype`)
- 計測環境: doc-gen4 の計装は当たったまま (`apply-instrumentation.sh --check` → `APPLIED (v4.31.0)`)【実測】

## Blocker — push に `workflow` scope が要る

```
! [remote rejected] main -> main (refusing to allow an OAuth App to create or update
  workflow `.github/workflows/ci-import-modules.yml` without `workflow` scope)
```

`gh auth status` → scopes は `admin:public_key, gist, read:org, repo`。**`workflow` が無い。**
撤去コミットが `.github/workflows/ci-import-*.yml` 3 本を触っているため弾かれる。

**ユーザーに実行してもらう** (ブラウザが開く対話フロー):

```
gh auth refresh -h github.com -s workflow
```

そのあと:

```
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.helper \
  GIT_CONFIG_VALUE_0='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
```

**ssh (port 22) はこの機材から通らない**【実測】ので上記の 1 コマンド限定の credential helper を使う。
git の設定を汚さないこと。

## Where we are

`experiments/` の撤去は**完了**。ユーザー判断は「案 C: 全部消す」(棚卸しで出た 3 案のうち最大)。
計画は `docs/plans/experiments-removal.md`。tracked **160 ファイルを HEAD から削除**し、
**4 本を移設**した (`tools/{setup-clone,rebuild-own}.sh`、`benchmarks/tools/{merge-timing,summarize}.py`)。

処分は 3 通り:
- **付け替え** — CI 3 本 → `extractor/`、`incremental-reference.sh`/`clone-gate.sh` → 製品側 (`lean-doc extract`)
- **削除** — `render/global-reference.sh`、`--impl ts` 経路、`gen-*.ts` 12 本、`clone-gate.sh` のゲート 2
- **移設** — 採点ロジックを持たない道具 4 本

**この撤去が撤回した既存の決定 2 つ** (計画 §1 に明記):
1. `implementation-plan.md:44`「`coverage.ts` と `tools/*-gate.sh` は消さない」→ `coverage.ts` は消した
2. `implementation-plan.md:40`「ゲート A の再定義はユーザーが後日行う」→ **再定義しない**で確定
   (根拠: M8 で doc-gen4 参照木は「バイトのオラクル」から「内容のオラクル」へ格下げ済み)

## Next step

1. **上の Blocker を解消して push** (これだけが残作業)
2. push 後、`docs/plans/experiments-removal.md` は役目を終える。残すなら「完了」と状態を書く

## 未決 — ユーザーに判断を仰ぐこと

- **`experiments/*/build/` が on-disk に 1.7 GB 残っている** (untracked / gitignored、10 ディレクトリ)。
  `git rm` は tracked ファイルしか消さないので残った。**あえて消していない** — この中の
  `experiments/stage7d/build/extract` は `milestone-log.md:308` の M4-a ゲート
  (「凍結バイナリとの 436/436 バイト一致」) が参照する唯一の実体で、**tag にも入っていない**
  (`.gitignore` 対象だったため)。消すと、そのゲートは tag から `Extract.lean` を取り出して
  ビルドし直さないと回せなくなる。**消してよいか要確認。**

## Files to read first

- `docs/plans/experiments-removal.md` — 今回の計画。§4 のゲートと §「G1 が 0 にならない理由」
- `CLAUDE.md`「撤去したプロトタイプ — tag `experiments-frozen`」 — 規則の新しい形
- `README.md`「撤去したプロトタイプ」 — 公開面の説明
- `crates/lean-doc-{md,render}/tests/data/PROVENANCE.md` — フィクスチャが「再生成手段の無い凍結値」になった旨

## Load-bearing context

- **`benchmarks/tools/measure-ledger.sh` は起動時に `benchmarks/results/m3a-ledger-*.jsonl` を
  切り詰める**【実測。今セッションで踏んで `git checkout` で復元した】。
  tracked な生ログなので、**動作確認のつもりで実走してはいけない。**
- **残ってよい `experiments/` の文字列は 4 種**: 生ログ / IR の `generator` 値 / 由来コメント /
  過去形の説明。**0 件にしてはいけない** (計画 §4 の「G1 が 0 にならない理由」)。
  特に `extractor/Extract.lean:2313` の `"lean-doc/experiments/stage4b"` は**値**で、
  書き換えるとフィクスチャ 8 本が全部落ちる
- **CI 3 本は付け替えたが未検証**。stage4b (schema 3) → `extractor/` (schema 4) で
  **測る対象が変わっている**。各ワークフロー冒頭にその警告を書いた。
  **記録済みの数字と新しい数字を並べないこと。**
- **履歴の書き換えはしない。** public にすれば tag 経由で `experiments/` は見える。
  「見せない」を満たすのは private に保つことであって削除ではない (ユーザーに提示済みの前提)
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
