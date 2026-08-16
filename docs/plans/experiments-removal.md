# `experiments/` の撤去

**決定 2026-08-16、ユーザー判断** — `experiments/` を **全部消す** (棚卸しで出た 3 案のうち「案 C」)。
凍結された使い捨てプロトタイプ 27 ディレクトリ / tracked 2.5 MB / 164 ファイルを HEAD から落とす。

## 1. Context — 何が消えて、何が困るか

`experiments/` は「凍結された過去」ではなく、**いまも実行系から参照されている**【実測 2026-08-16、棚卸し】。

- **`cargo test --workspace` は `experiments/` を必要としない。** 参照は doc コメントと
  文字列リテラル (`"lean-doc/experiments/stage4b"` = 台帳の `irGenerator` の**値**) だけ。
  外部木を見るテストは全部 env-gate で skip する
- **27 ディレクトリのうち実行系が触るのは 7 つ** — stage4b / 4c / 5 / 5e / 7d / 7g / 7h。
  残り 20 (tracked 約 1.5 MB) は誰も実行しない
- 7 つが持つ「消せない機能」は 3 種類:

| | 機能 | 実体 |
|---|---|---|
| (a) | フィクスチャ生成器の `--check` | `stage7d/render.ts` / `stage7h/global.ts` / `stage5/*.ts` を `gen-*.ts` 9 本が叩く |
| (b) | 受け入れオラクル | `stage4c/coverage.ts` (doc-gen4 に対する byte 再現率) |
| (c) | 環境操作スクリプト | `stage5e/setup-clone.sh` / `stage7g/extract-once.sh`。**採点ロジックはゼロ** |

### この決定が撤回するもの

案 C は既存の 2 つの記述と衝突する。**衝突する側を書き換えるのが今回の作業に含まれる。**

1. **`implementation-plan.md:44`「`coverage.ts` と `tools/*-gate.sh` は消さない」** —
   `coverage.ts` は消す。`tools/*-gate.sh` は残す (付け替えて生かす)
2. **`implementation-plan.md:40`「ゲート A の再定義はユーザーが後日行う」** —
   ゲート A は **M8 で完全に終わっている** (`milestone-log.md:690`: doc-gen4 参照木は
   「バイトのオラクル」から「内容のオラクル」へ格下げ済み)。再定義はしない
3. **CLAUDE.md / README の「`experiments/` は 1 バイトも変更しない」** — 削除はこの規則の変更

### 撤回しないもの

- **99.5062% は書き換えない。** M6 時点の到達点として残す (`implementation-plan.md:42`)
- **フィクスチャ 8 本は消さない。** 生成器を失って「再生成手段を持たない凍結値」になるが、
  値そのものは回帰テストとして生き続ける

## 2. Approach

**「消す前に、消えるものが持っていた機能を 3 通りに処分する」** — 削除は最後の 1 コミット。

| 処分 | 対象 | やり方 |
|---|---|---|
| **(i) 付け替え** | 製品側に等価物があるもの | CI 3 本 → `extractor/`。`incremental-reference.sh --extractor proto` → `product` を既定に |
| **(ii) 削除** | 等価物が無く、役割が終わっているもの | `render/global-reference.sh` とその compare、`--impl ts` 経路、`clone-gate.sh` のゲート 2、`gen-*.ts` 7 本 |
| **(iii) 移設** | 採点ロジックを持たない道具 | `stage5e/{setup-clone,rebuild-own}.sh` → `tools/` |

**数字の出所は tag で辿れるようにする** — 削除直前の commit に `experiments-frozen` を打ち、
docs の `experiments/...` 参照を「tag `experiments-frozen` の `experiments/...`」に書き換える。
**これをやらないと verification-log の 44 行が宙に浮く。**

処分の順序は **(i)(ii)(iii) → docs → 削除 → 検証**。逆にすると壊れた中間状態が commit される。

### なぜ「Rust に移植して採点器を作り直す」をやらないか

README / 実装計画の原則: **「同じ言語・同じ設計で書き直すと両方同じ間違いをする経路ができる」**。
だから TS の採点器を Rust に移植しても採点器にならない。案 C は
**「採点器を作り直す」ではなく「採点をやめる」** — ゲート A が終わっている以上、
残っていたのは回帰の網であって、その網は commit 済フィクスチャ 8 本が既に持っている。

## 3. ファイル別内訳

### (i) 付け替え

| ファイル | やること |
|---|---|
| `.github/workflows/ci-import-modules.yml` | `experiments/stage4b/build.sh` → `extractor/build.sh`、`stage4b/build/extract` → `extractor/build/extract` |
| `.github/workflows/ci-import-prefetch.yml` | 同上 |
| `.github/workflows/ci-import-prefetch-narrow.yml` | 同上 |
| `benchmarks/tools/measure-residency.sh:49` | `stage1/run.sh` 参照の始末 |
| `tools/incremental-reference.sh` | `--extractor proto` を削除、既定を `product` へ。`--impl ts` 削除 |
| `tools/clone-gate.sh` | `EXTRACTOR` を `extractor/build/extract` へ。ゲート 2 (TS 対 Rust) を削除 |
| `tools/build-gate.sh` | `SETUP_CLONE` を `tools/setup-clone.sh` へ |

**CI 3 本は付け替えても未検証** — stage4b は IR schema 3、`extractor/` は schema 4 で
**抽出後の解析の量が違う**。CLI の形 (`extract <modules.txt> <out.jsonl>`) は同一なので動くはずだが、
**測る対象が変わる**。ワークフロー冒頭にその旨を書き、記録済みの数字と新しい数字を
並べないよう警告する。3 本とも `workflow_dispatch` 専用なので push/PR では落ちない。

### (ii) 削除

| ファイル | 理由 |
|---|---|
| `tools/render-reference.sh` / `tools/render-compare.sh` | TS 専用。M1 のゲートは通過済で参照木は `/private/tmp` にしかない |
| `tools/global-reference.sh` / `tools/global-compare.sh` | 同上 (M2) |
| `tools/{merge,impact,ledger}-reference.sh` の `--impl ts` 経路 | フィクスチャは commit 済。`--impl rust` は残す |
| `crates/lean-doc-render/tests/gen-ts-expected.ts` | `stage7d/render.ts` を叩く |
| `crates/lean-doc-render/tests/oracle/gen-{pages,page-parts,autolink,fragment,docgen4-linked}-expected.ts` | 同上 (`docgen4-linked` は要確認 — doc-gen4 側なら残す) |
| `crates/lean-doc-md/tests/oracle/gen-ts-docstring-expected.ts` | 同上 |
| `crates/lean-doc-global/tests/oracle/gen-{global,delta}-expected.ts` | `stage7h/global.ts` を叩く |
| `crates/lean-doc-incr/tests/oracle/gen-{ledger,merge,impact}-expected.ts` | `experiments/` を直接は読まないが `--impl ts` の出力を読む → 一緒に死ぬ |

**残す生成器**: `crates/lean-doc-md/tests/oracle/gen-{docgen4,md4lean}-expected.ts` —
オラクルは doc-gen4 / MD4Lean であって `experiments/` ではない。

### (iii) 移設

| from | to |
|---|---|
| `experiments/stage5e/setup-clone.sh` | `tools/setup-clone.sh` |
| `experiments/stage5e/rebuild-own.sh` | `tools/rebuild-own.sh` |

`stage7g/extract-once.sh` は**移設しない** — あれは stage7d の抽出器を回すための道具で、
製品側 (`--extractor product|resident`) に等価物がある。

### docs

| ファイル | 参照行数 | やること |
|---|---:|---|
| `docs/verification-log.md` | 44 | `experiments/...` → tag 参照。**数字は 1 つも書き換えない** |
| `docs/milestone-log.md` | 5 | 同上。M4-a のゲートが指す `stage7d/build/extract` は**元から gitignored** なので、tag でも辿れないことを明記 |
| `docs/implementation-plan.md` | 6 | §1 の枠 (ゲート A) を書き換え。§2 の「受け入れオラクルは製品外に残す」を撤回 |
| `docs/approach.md` | 1 | tag 参照へ |
| `docs/provenance.md` §8 | 1 | 「`experiments/` を配布物に含めた場合」の前提を確定 (**含まれない、HEAD から消えた**) |
| `README.md` | 3 箇所 | 構成表から `experiments/` を落とす。`coverage.ts`「今も回す」を削除 |
| `CLAUDE.md` | 数箇所 | 「`experiments/` は変更しない」規則を削除 |
| `crates/lean-doc-{md,render}/tests/data/PROVENANCE.md` | 各 1 | 「生成器は tag `experiments-frozen` にある。**再生成手段は HEAD に無い**」 |

## 4. ゲート (この作業の完了条件)

| | 条件 |
|---|---|
| G1 | `rg -n 'experiments/' --glob '!*.md' .` が **0 件** |
| G2 | `cargo test --workspace --no-fail-fast` が **355 passed / 2 failed** (赤 2 件は既知の環境要因。**増えていない**こと) |
| G3 | `cargo clippy --workspace --all-targets` 0 warning、`cargo fmt --check` 緑 |
| G4 | md 中の `experiments/` 参照が**すべて** tag `experiments-frozen` を伴う |
| G5 | `git tag -l experiments-frozen` が削除直前の commit を指す |
| G6 | `tools/` の残ったハーネスが `--help` / 引数検証まで到達する (実走は不要) |

## 5. やらないこと

- **履歴の書き換えはしない。** `filter-repo` を使えば `experiments/` は履歴からも消えるが、
  全 commit のハッシュが変わり、docs が指している commit hash が全部無効になる。
  **「公開しない」を満たすのはリポジトリを private に保つことであって、削除ではない**
  (この事実はユーザーに提示済み)
- **フィクスチャの値は触らない。** 生成器を消しても値は凍結値として生き続ける
- **99.5062% を含む既存の数字は 1 つも書き換えない**
