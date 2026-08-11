# Handoff — 2026-08-11 (15)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 4 / cap 8
- Predecessor: leandoc-v01-r3
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画 + 未決 3 件 + M0 + **M1-a** + **M1-b** + バイト差分オラクル。`26df736` → `4fdc8d4`
  - r2: **M1-c 完遂** (md4c FFI + AST / AST → HTML / autolink)。`a605256` → `2a8cebe`
  - r3: **M1 完遂** (d1/d2/d3) + docs 圧縮 + **M2-a**。`418938d` → `5965298` の 5 コミット

## State

- Branch: main / **clean** / push 済み (`5965298` が HEAD)
- `cargo test --workspace --no-fail-fast` **188 本** / clippy / fmt 緑
- **M1 は完遂しゲート A も通過** — 432 ページ中 431 が TS 版とバイト一致 (差分 1 = 登録済みの
  CommonMark 乖離)、`coverage.ts` の出力は **TS 版とパス・日付以外バイト一致** (99.5% / 304-348)
- 計測対象 `/Users/haruka/dev/lean-projects` は clean、doc-gen4 の計装は APPLIED のまま。
  **doc-gen4 の参照木 736 MB は健在** (作り直すと約 9 時間)。この leg では Lean をビルドしていない
- 揮発 fixture: `/private/tmp/lean-doc-relay/` の `w7h/base-ir` / `m1/ref-pages` /
  `w7c/linkindex/link-index.lidx` / `m2/verify-ref` は健在

## Tasks

- #3 [in_progress] **M2** — **M2-a 完了**。残りは **M2-b**
- #4〜#7 [pending] M3〜M6 (依存順に blockedBy 済み)

## Where we are

M2 は 2 分割の前半が終わった。**6 成果物 (bmp / name-map / navbar / tactics / references×2、
計 1,877,124 B) は TS 版とバイト一致**し、`lean-doc global --ir --out` で出る。
ハーネスは `tools/global-{reference,compare}.sh`。**残りは M2-b** = `contentHash` キャッシュ層
(`ModuleFacts` / `STATE_VERSION` / `STATE_DERIVATION` / `global-state.json`) と
**全域写像 delta** (`--before` / `--print-set` / `--delta-json` → `global-set.txt`)。
キャッシュの継ぎ目は `lean-doc-global/src/site.rs` の `facts_for` 1 箇所に寄せてある。

## Next step

**M2-b を subagent に dispatch する。** 移設元は `experiments/stage7h/global.ts` の
`ModuleFacts` 周り (136-230) と delta (365-420)。**オラクルは `stage7h/oracle.sh` の 7 状態**
(base / rerun / modified / removed / added / restored / **stale-state**) — 7 番目が
「導出規則が変わったら鍵の文字列を変えて全ミスさせる」規律をテストしている (計画 §3)。
その後 **M2 のゲートの後半「サイト 439 も byte 一致」** を render + global を 1 つの木に出して確認する。

## Files to read first

- `docs/implementation-plan.md` — §3 (キャッシュのバージョン鍵)、§5 の M2 の穴、§7 の M2-a の結果と **V6**
- `crates/lean-doc-global/src/site.rs` — `facts_for` = M2-b が中身を差し替える 1 点
- `experiments/stage7h/global.ts` — 移設元。`--state` 有りの経路が M2-b
- `experiments/stage7h/oracle.sh` — 7 状態のオラクル。Rust の統合テストに移す

## Load-bearing context

- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と `lean-projects` の無傷確認 +
  **成果物を自分で作り直してバイト比較** + **mutation を 1 件スポット再現** + **母数を独立に再計算**。
  r3 では 4 体すべての報告が検算に耐えた (母数取り違えゼロ)
- **`cargo test --workspace` は `--no-fail-fast` を付ける** — 付けないと最初に落ちたテスト
  バイナリで止まり、「どのテストが変異を捕まえたか」を読み違える
- **「全件バイト一致」は分岐被覆の証明ではない** (計画 §7 に常設ルール化済み)。M1/M2-a を通して
  **実データが到達しない分岐は 15〜40%**、mutation 25 件中 18 件は**全件比較が緑のまま**通った。
  M2-b でも**分岐発火回数を先に測ってから**一致を主張する
- **V6 が M2-b で発火する** — `autolinkTokens` の分割を UnicodeBasic に替えてあり、V8 と
  4,803 コードポイント食い違う。トークンは**delta にしか出ない**が delta は M3 の再生成集合の入力。
  **取りこぼすと古いページが黙って残る**。過剰側 (和集合) に倒すのが撤退策
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている** (署名不正で SIGKILL)。JS は **deno**。`diff` は zsh で
  `colordiff` にエイリアス (未インストール) なのでスクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**
- **`coverage.ts:512` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード** —
  40 hex 以外を `--source-url` に渡すと**採点が静かに下がる**
- **`serde_json` の `preserve_order` は必須依存** — `indexmap` が「未使用に見える」と削られるとバイトが動く
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない**。この防御を緩めない
