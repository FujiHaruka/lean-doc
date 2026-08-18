# Handoff — 2026-08-18

## State

- Branch: main / HEAD は**改名コミット** (直前は `70f48b1`)
- Active phase: **2026-08-18 に `lean-doc` → `litedoc4` へ改名した**【ユーザー判断】 —
  リポジトリ名・crate・CLI・Lake パッケージ名のすべて。**記録の SoT は
  `docs/plans/rename.md`** (旧名を意図的に残した 5 種と、改名が生んだ過渡状態)。
  その前の Lake パッケージ化は完遂済み
- CI は **5 ワークフロー** — `ci.yml` (3 ジョブ) / `ci-action.yml` (5 ジョブ) / `release.yml` (tag) /
  `ci-extractor-portability.yml` (`workflow_dispatch`) / **`ci-lake.yml` (4 ジョブ、新規)**
- 手元の状態: `.lake/` 178 MB (Lake が建てた extract) と `e2e/micro/.lake/` 176 MB
  (build.sh の extract) が残っている。**どちらも gitignored かつ再生成可能**。
  `lean-projects` は無傷、**ディスク 51 Gi 空き**

## Relay control

- Mode: DONE
- Goal: Lake パッケージ化を第 1 段 (lakefile + extractor を Lake に載せる) から
  第 2 段 (Release からの Rust バイナリ取得) まで完遂する
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: **L1・L2 とも完遂**。計画 `2b4c03f` → L1 `ce0f32d` → フィクスチャ修正 `bf4c6b6` →
    L2 `202011b`。PR #1 で CI 実走してから main へ。欠陥 3 件、**product は 0 件**

### 前 relay (完了、参考)

- Goal: 配布を完遂する (D1 検証 → D2 Release → D3 action → D4 プリビルドの可否) — **DONE**

## Where we are

利用者が書くのはこれだけ:

```lean
require «litedoc4» from git "https://github.com/FujiHaruka/litedoc4" @ "main"
```
```
lake run docs -- --out ../mypkg-docs
```

**利用者が用意するものは無い** — extractor は Lake が利用者の toolchain で建て、Rust バイナリは
Release から取る (checksum 照合必須)。**`--lib` の手書きが要らなくなった**のが `lakefile.lean` の
パッケージ (Mathlib / doc-gen4) にとっての本題。

## Next step

**ユーザーの指示待ち。** 手を動かすなら候補は 3 つ:

1. **tag を打つか — 改名で優先度が上がった**。`v0.1.3` 以前のツリーには `lakefile.lean` が
   無いので既存 tag では `require` が解決できない、に加えて、**改名で 3 つ壊れたままになっている**:
   README の `curl` が **404** (最新 Release の資産は `lean-doc-*.tar.gz`)、`lakefile.lean` の
   段 3 が資産を見つけられない、**L2 が未検証に戻った** (`tools/lake-download-gate.sh` を
   走らせられない)。`Cargo.toml` を 0.1.4 に上げて tag を打てば 3 つとも解ける。
   **これは outward-facing なのでユーザーの判断** → `docs/plans/rename.md` §5
2. **`resolveLitedoc4` 段 5 (`cargo build`) の成功経路** — 10 項目のゲートで唯一残った穴。
   両ゲートが失敗する `cargo` shim を置いているため一度も走っていない
   (置かないと「落ちるべき項目」が数分の release ビルドの末に別の理由で緑になる)
3. **`docs/approach.md` が 613 行**で `/compact-plan` の閾値超え。前 relay から持ち越し

## Files to read first

- **`docs/plans/rename.md` — 改名の記録。§3 が「直してはいけない旧名 5 種」、§5 が過渡状態**
- `docs/plans/lake-package.md` — 計画の SoT。冒頭に結果表、§3 に確定した前提 5 つ、§6 に未決の答え
- `benchmarks/results/lake-package-probe-2026-08-18.txt` — Lake の挙動の実測。**§1 が toolchain の話**
- `lakefile.lean` — `resolveLitedoc4` の入手順序 6 段が判断の 1 箇所
- `tools/lake-download-gate.sh` の冒頭 — **なぜ `LITEDOC4_BIN` を設定してはいけないか**

## Load-bearing context

- **`lean-toolchain` を litedoc4 に置かない**【実測】。依存側が root より高い版を持つと
  `lake update` が**利用者の `lean-toolchain` を書き換える** (elan の入手失敗より前に)。
  低いと警告すら出ない。**CLAUDE.md の制約は弱まるどころか強化された**
- **オラクルは IR の byte 一致**。Lake ビルドは package シンボル prefix と `-O3` で
  バイナリを +308,032 B 動かすので、**バイナリの SHA 一致は原理的に追えないし追う必要も無い**
- **`lake run` は `--` を剥がさない**【実測】。script 側で先頭の `--` を落としている
- **script の env に `LEAN_PATH` は入っていない**【実測】。ただし `litedoc4 build` は
  `--lake` から自分で `lake env` を張るので、script が張る必要は無い
- **`lakefile.toml` の `[[script]]` はエラーも警告も無く黙殺される**【実測】。
  だから litedoc4 側は `lakefile.lean`。**利用者側の lakefile は `.toml` のままでよい**
- **`trap … EXIT` の最後のコマンドの終了コードがスクリプトの終了コードになる** —
  `tools/e2e-micro.sh` がこれで「ok」と印字して exit 1 していた。CLAUDE.md「この機材の罠」に追加済み
- **依存として入った litedoc4 自身が consumer の package リストに載り**、
  `no top-level .lean file, so no module root could be resolved` という note が毎回出る。
  エラーでも実害でも無いが**全利用者の出力に出る**。消すならパッケージ形を変えることになる
