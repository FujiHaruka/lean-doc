# Handoff — 2026-08-17 (未検証項目 4 段を完遂 / relay leg 1 で DONE)

## State

- Branch: main / **clean** / push 済み (HEAD `3eae364`)。tag `v0.1.0` は `117e928`
- `cargo test --workspace` **355 passed / 0 failed / 21 ignored**、
  `cargo fmt --check` / `clippy -D warnings` 緑、`tools/corpus-gate.sh --verify-list` ok (21)、
  `tools/provenance-gate.sh` ok (27 claims)
- **CI は 3 ジョブ緑** (`test` / `e2e (real extractor, no Mathlib)` / `supply chain`)
- **README「未検証項目」は 18 → 15 → 13 件**。**15 → 13 は実際に潰した 2 件で、
  どちらも「未検証」ではなく壊れていた**
- **ディスクは 56 Gi 空き**。`/private/tmp/lean-doc-relay` は**削除済み**

## Relay control

- Mode: DONE
- Goal: README「未検証項目」15 件のうち 4 件を、段 1〜4 の順で潰す
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r0: **棚卸しのみ** (`a3aeb31`)。4 段の実作業はゼロ
  - r1: **段 1〜4 すべて完了**。`2d70d5b` (段 2) / `19a4fc6` (段 1) / `80e1eb8` + `16be789` (段 3) /
    `2c9b198` + `3eae364` (段 4)

## この leg で出たもの — 4 段の結果

| 段 | 項目 | 結果 |
|---|---|---|
| **1** | #15 md4c fuzz + `cargo-deny` | **両方すでに動いていた**。docs が古かった。探索は新規に実走 |
| **2** | #10 + #4 | **#10 は未検証ではなく壊れていた**。#4 は綴り差が実在すると判明 |
| **3** | #13 等幅フォント | **2 OS で欠け 0**。UI-V1 (JuliaMono vendor) は発火しない |
| **4** | #1 実在の公開パッケージ | **`batteries` で 3 件出た**。うち 2 件は product の欠陥 |

**この leg の教訓は 1 つに集約する**: **「未検証」と書いてあるものは、測ると壊れている。**
4 段のうち **3 段で欠陥が出た**。残り 13 件を「たぶん大丈夫」と読まないこと。

### 段 1 — 探索は回した。ゲートは元からあった

- **`cargo-deny` は CI ジョブ `supply-chain` で既に緑だった** (commit `76d17e0`、2026-08-16)。
  README #15 と `quality-gates.md` Q9 が**1 日古いまま**だった
- **fuzz corpus ゲートも既に毎 push 走っていた** (`crates/lean-doc-md/tests/fuzz_corpus.rs`)
- **新しくやったのは探索** — `fuzz/` (cargo-fuzz、nightly、ワークスペース外) で
  **8,939,197 execs / crash 0**【実測】。うち 1 本は**空 corpus** から回して
  **MD4Lean を殺す 2 入力を自力で再発見** (QV6 成立)
- **最大の発見は「何を検査していなかったか」**: `RUSTFLAGS` は Rust にしか届かないので、
  **`CFLAGS` を渡さない fuzz は md4c を一切見ていない**。cov 1023 → 2487 で確認【実測】

### 段 2 — 版固定できない依存へのリンクは 404 していた

- `e2e/micro-dep/` を **path で require** した初回に `site-gate` が **DEAD internal links 3**
- 3 経路とも別物 (import リスト / docstring の名前参照 / 署名中の定数リンク)
- 直した形: **版固定できない依存にはリンクを張らない** (名前はテキスト)
- **#4 の実測**: `.lidx` は非エスケープ、IR はエスケープ済み。docstring の 3 綴りのうち
  **`.lidx` と同じ非エスケープ綴りだけ解決しない**。ただし**出力に出るのは版固定できる依存の
  ときだけ**なので #4 は残っている (README #3)

### 段 3 — 持っていた Linux 機材を使っていなかった

- ブラウザゲートに**検査 8** を追加。**文字集合は対象由来の 178 種**を固定
  (`benchmarks/tools/mono-charset.json`、`mono-charset.py` で再生成)
- **macOS も `ubuntu-latest` も字形の欠け 0**。off-width は 34 / 22 だが**決定 2 が許容済み**
- **等幅スタック単独では 8〜9 割しか描いていない**ことも数字で出た

### 段 4 — batteries は 3 つ驚かせてきた

対象: `leanprover-community/batteries` @ `fa08db58…` (**Mathlib 非依存**、176 モジュール / 3,030 宣言)。
数字は `benchmarks/results/batteries-2026-08-17.txt`。

1. **build が止まった** — `class LawfulLTCmp … extends Std.OrientedCmp` の継承 field を
   置けず、ページを 1 枚も書かずに終了。→ 解決を `.lidx` まで落とした
2. **死にリンク 10 本** — `[[lean_lib]]` が 3 つあるのに `--lib` は 1 つ分しか書かない。
   → 「**このランがページを書かないモジュールにはリンクを張らない**」に一般化 (段 2 と同じ穴)
3. **ゲート側の偽陽性 2 件** — `check-site-closure.py` が `id` 属性を unescape していなかった

最終状態: **site-gate ok / DEAD 0 / browser-gate 10 検査すべて緑 (180 ページ)**。

## 次に手をつけるなら

**ユーザーの指示待ち**。この leg のゴールは完遂したので、次は新しいゴールが要る。候補:

- **README 未検証項目の残り 13 件**。「測ると壊れている」実績が 3/4 なので、
  効きそうなのは **#1 (`push:` トリガと利用者リポジトリの checkout)** と
  **#9 (依存 root 27 件にオラクルが無い)**
- **Mathlib 依存の実在パッケージでの実走** — batteries は Mathlib 非依存なので、
  ゲート B の「合成に限る」制約はまだ実物で外れていない
- **性能の次の一手は構造変更**で、この計画の外 (→ `implementation-plan.md` §1 末尾)

## Files to read first

- `README.md` §未検証項目 — **13 件の現況**
- `benchmarks/results/batteries-2026-08-17.txt` — 段 4 の数字と出たもの全部
- `docs/implementation-plan.md` §M7「訂正 —『マップに root が無い ⇒ 相対ページリンク』は
  死にリンクだった」 — **同じ穴を 1 日で 2 回踏んだ記録**
- `e2e/README.md` — フィクスチャの担当表と「path 依存を足して出たもの」
- `fuzz/README.md` — 探索の回し方。**`CFLAGS` が無いと md4c を見ない**
- `docs/plans/quality-gates.md` Q6 / Q8 — ゲートの現況

## 踏んだ地雷

**恒久的なものは `CLAUDE.md` に移した** — この leg で出た一般論は「計測の誠実性」
(未検証を「たぶん大丈夫」と読まない / docs の「未」は腐る)、「品質ゲート」
(被検査範囲を数字で確かめる / 両方向同時の差分は比較器を疑う)、**新設「欠陥を直すとき」**
(一般形に引き上げたか毎回問う)、**新設「この機材の罠」**に入っている。
**ハンドオフに書き写さないこと。**

この leg 固有の状態:

- **`/private/tmp/lean-doc-relay` は削除済み**。次に計測するときは作り直す
- **`fuzz/corpus` と `fuzz/target` は gitignored** (合計 140 MB ほど手元に残っている)。
  消しても `fuzz/README.md` の手順で作り直せる
