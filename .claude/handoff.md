# Handoff — 2026-08-17 (G1: CI 軸を実測で閉じる / leg 1 → leg 2)

## Relay control
- Mode: ON
- Goal: **G1「CI 軸を実測で閉じる — 『配置が第一の打ち手』をランナー実体で裏付ける」**。
  達成後、**朝 10:00 JST 前ならゴールを自分で再設定して自走を続ける**【ユーザー指示】
- Leg: 2 / cap 8
- Predecessor: none (leg 1 はユーザーの元セッション。tmux 名を持たないので kill しない)
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: ゴール設定のみ。**実作業ゼロ**【ユーザー指示「次セッションで実作業を始めて」】

## State

- Branch: main / **clean** / push 済み
- 直前の relay (品質ゲート整備) は **DONE**。そのスコープの残作業は無い
- 開始時刻 **2026-08-17 00:0x JST**。**朝 10:00 JST が自走の終了目安**
- `cargo test --workspace` 346 passed / 0 failed / 24 ignored、CI 3 ジョブ緑【前 leg 実測】

## なぜ G1 か

`approach.md` §3 / §8 は「cold な環境ロードへの**第一の打ち手は配置**(doc 生成を `lake build` と
同じジョブに置く)」と書き、根拠は Linux 実測の `importModules` **2.61 s (同じジョブ) ↔ 20〜89 s
(別ジョブ) = 8〜34 倍**。**しかしこれは `importModules` 単体の値**で、テンプレ全体を通した
ランナー実体の数字が無い。

- `tools/ci-build.sh` (295) と `.github/workflow-templates/lean-doc-docs.yml` (135) は書かれているが、
  **テンプレは一度も Actions で走っていない** — ファイル冒頭に `THIS FILE HAS NEVER BEEN RUN 【未検証】`
  と自分で明記してある。未検証なのは actions のバージョンと入力 / 3 つのキャッシュ鍵 / elan / `lake exe cache get`
- V5 の実測 (clean 5.887〜10.361 s、無変更 1.274〜1.286 s) は **Apple M1 ローカル warm**。
  docs 自身が「**CI ランナーは cold 側なのでこれは下限**」と書いている
- **リポジトリを public にした理由が「Actions を無料枠で回すこと」**(CLAUDE.md)。条件は整っている

## 7 + 1 段

| # | やること | 判定 |
|---|---|---|
| 1 | **public 化の残り** — `lean-doc-docs.yml:71-79` の `token: ${{ secrets.LEAN_DOC_TOKEN }}` は 2026-08-16 の public 化で不要。コメントも「While this repository is private」のまま | 利用者が持っていない secret を要求しない |
| 2 | **`tools/make-target2.sh` の Linux 経路** — 現在 `.lake/packages` を **APFS clonefile** で借りる macOS 専用。Actions では `lake exe cache get` で取る | Linux で target2 が建つ |
| 3 | **計測用ワークフロー** `.github/workflows/ci-placement.yml` (**`workflow_dispatch` のみ**) — **A: 同じジョブ / B: 分割ジョブ** の A/B。n≧5、`/usr/bin/time -v` で CPU 時間・major fault・peak RSS。先例は既存の `ci-import-*.yml` 4 本 | 一度落としてから通す |
| 4 | **実走** → ランナー実体の数字。**cold 側は `ubuntu-latest` でも 2 実体に割れる**(readahead とストレージ差、未検証、n=2) ので**実体を記録する** | 生ログが `benchmarks/results/` に落ちる |
| 5 | **approach.md §8 (c)** 「**完全版の抽出器でも同じ差が出るか**」に答える (macOS では作業集合が 16 GiB に収まらず測れなかった項目) | §8 の行が実測に変わる |
| 6 | 数字を commit + **同じコミットで docs を更新** — approach.md §3/§8、implementation-plan.md V5、milestone-log.md、README の未検証 10 件 | 引用元と数字が食い違わない |
| 7 | 可能ならテンプレ自体を実走させ `THIS FILE HAS NEVER BEEN RUN` を消す | 消せないなら**理由を書いて残す** |
| 8 | (小・独立) `approach.md` §9「Lean フロントエンドの再実装」に**段階 7e / 段階 2 への参照**を足す — 根拠が別の節にあるのに §9 から辿れない | 1 行で足りる |

## Next step

**段 1 と段 8 は独立で小さいので先に片付けて 1 commit。** その後、段 2 (`make-target2.sh` の
Linux 経路) の調査 → 段 3 のワークフロー作成へ。段 3 を書いたら**必ず一度落としてから**通す。

## Load-bearing context

- **Actions を回すのは lean-doc 自身のリポジトリだけ。外部への新規リポジトリ作成はしない**
  【ユーザーに宣言済み】 — target2 はジョブ内で `make-target2.sh` が生成する
- **`workflow_dispatch` で明示起動する。push トリガにしない** — 毎コミット数 GB を落とすことになる
- `gh run watch` / `gh run view` で結果を取る。**foreground の `sleep` は使わない**
  (背景 Bash か Monitor)
- **計測の誠実性** — 1 回だけ測った数字を信じない / cold と warm を混ぜない /
  実測・外挿・仮定・理論値のラベルを落とさない / 倍率は分母を明示する
- **ゲートは自分では自分を検査しない。** 新しいワークフローは**必ず一度落として**から通す
  (作った当日に「何をしても通るゲート」を 2 件作った実績がある)
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
  **その 1 コマンドの間だけ**効く。git の設定を汚さない
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
- **`e2e/micro` の宣言を消さない / Mathlib を足さない** — 足した瞬間 CI で回らなくなる
- **`benchmarks/tools/measure-ledger.sh` は起動時に tracked な生ログを切り詰める**【実測】。
  動作確認のつもりで実走しない

## Files to read first

- `.github/workflow-templates/lean-doc-docs.yml` — テンプレ。冒頭に未検証の宣言 (段 1 / 段 7)
- `tools/ci-build.sh` — CI ジョブの実体 295 行。`lake build` → `lean-doc build` の順序が主張の本体
- `.github/workflows/ci-import-modules.yml` — **計測用ワークフローの先例** (`workflow_dispatch`)
- `tools/make-target2.sh` — 第 2 の対象の生成器 (段 2。APFS clonefile 依存は line 81 付近)
- `docs/approach.md` §3 / §8 — 配置の主張と、残る 3 項目 (a) 先読みを絞る (b) IR キャッシュ (c) 完全版
- `docs/milestone-log.md`「M6 の結果」 — V5 の数字とテンプレの位置づけ

## G1 の後 (朝 10:00 JST 前なら次ゴールを自分で設定する)

候補 (leg が自分で選ぶ。**選んだ理由を handoff に書く**):
- **宣言単位の再解析キャッシュ** — 意味解析を速くするのではなく**回数を減らす**筋。
  approach.md §6.1 末尾が「次に狙うなら」と名指ししていて未着手
- **approach.md §8 (a) 先読みの集合を絞る** — ランナー実体で答えが割れるので期待値は上がるが保証は無い。
  配置が効いている限り優先度は低い
- **E2 (`tools/target2-gate.sh`) がいま動くかの確認** — 前 leg の Next step に残っている
