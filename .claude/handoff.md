# Handoff — 2026-08-22 (残件掃き R1〜R5)

## State

- Branch: **`main`** / clean / **`7ac9538`** まで push 済み
- **`docs/plans/residual-sweep.md` は全項目が決着した** — R1〜R4 が入り、R5 は保留、
  R3 の仕様判断は「解決させる」で実装済【いずれもユーザー判断】
- `cargo test --workspace` = **36 バイナリ / 437 passed / 0 failed / 21 ignored**
- `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo doc` (RUSTDOCFLAGS=-D warnings) / `tools/provenance-gate.sh` /
  `tools/corpus-gate.sh --verify-list` すべて緑
- 作業領域 `/private/tmp/lean-doc-relay` は **34 MB** まで掃除済み
  (Mathlib の IR 550 MB × 2 とサイト 1.1 GB は計測後に消した)
- 計測対象 `/Users/haruka/dev/lean-projects` は**無傷** — 計装 APPLIED、
  olean 数も計測前と同じ (mathlib 8229 / 対象 1090)、untracked も `docs/doc-gen-bench/` のみ

## Relay control

- Mode: **DONE** (この連鎖は `/carryon` の単独再開であって relay ではない)

## このセッションで入ったもの

| | 内容 | commit |
|---|---|---|
| 計画 | `docs/plans/residual-sweep.md` を起票 | `2470781` |
| **R1** | prototype の 7 キーを `PROTOTYPE_FACT_KEYS` に集約し、構造の主張を機材ゼロ依存のテストへ | `275766d` |
| **R2** | NOTICE の網羅を導出で検査。抜けていた **13 件**を載せた | `458a5aa` |
| **R3** | 版固定できる依存 + ギュメ付きモジュールの実物を作り `tools/pinned-dep-gate.sh` に | `106faff` |
| 修正 | rustdoc: `#[cfg(test)]` へは intra-doc link を張れない | `370740e` |
| **R5** | 保留の判断と、覆われている範囲の書き直し | `cbe203f` `c9fcc66` |
| **R4** | Mathlib 全体 8,169 モジュールを実測、小 RAM は外挿 | `285f56b` |
| 決定 | 未検証の残り 1 件はやらない | `1eebd45` |
| **R3 判断** | `.lidx` の綴りを解決させ、`RENDERER_ID` を v4 に上げた | `7a904dd` |

数字はすべて `benchmarks/results/residual-sweep-2026-08-22.txt` (§1〜§5)。

### 効くようになったもの

- **`litedoc4_global::PROTOTYPE_FACT_KEYS`** — prototype の 7 キーの唯一の定義。
  転写 2 本 (`tests/global.rs::Facts` / `tests/state_and_delta.rs`) は**両方これを検査する**
- **`tools/provenance-gate.sh` の後半** — `release.yml` の matrix から対象を読み、
  `cargo tree -e normal` の closure と NOTICE を**両方向**で突き合わせる。例外リスト無し
- **`tools/pinned-dep-gate.sh`** — `e2e/micro-dep` を git リポジトリ化して
  版固定できる依存に差し替える。ネットワーク不要 (git の `insteadOf`)。CI の `e2e-micro` job に配線済み

## 残っている判断 — 無し

## 決着済 — 蒸し返さない

- **`.lidx` の綴りは解決させる**【決定 2026-08-22、ユーザー判断】(`7a904dd`)。
  実装は `NameIndex::module_for_unescaped` (非エスケープ綴り → モジュール、**曖昧なら答えない**)。
  **`is_name_lit` は緩めていない** — doc-gen4 の転写で、あらゆる code span の入口だから。
  足したのは `is_name_lit` が**偽の側**で、そこは無条件に `None` を返していた
  = **バイト不動**。`RENDERER_ID` は **v3 → v4**

- **未検証の残り 1 件 (小 RAM Linux ランナーでの実走) はやらない**
  【決定 2026-08-22、ユーザー判断】(`1eebd45`)。**測れないからではなく、外挿で結論が出ていて
  実走しても動かないから**。**「未検証 0 件」と書かない** — 閉じたのであって測ってはいない。
  **再検討の入口は前提 2** (olean 5.7 GB の全部が要るとは限らない。Linux の residency は
  62.4%、n=1 → 3.3 + 3.5 = 6.8 GB なら 7 GB に収まりうる)
- **R5 (他人のリポジトリから `uses:`)** は**保留のまま**【ユーザー判断】。
  **安い道 (既存リポジトリを checkout して workspace を他人の木にする) も取らない**と決めた

## Next step

**`docs/plans/residual-sweep.md` は全項目が決着した。** R1〜R4 は入り、
R5 と未検証の残り 1 件は「やらない」、R3 の仕様判断は「解決させる」で実装済。
残っているのは計画自体を閉じること (§4 の撤退ラインを読み直す) か、別の作業に移ること。

## Files to read first

- `docs/plans/residual-sweep.md` — §2 Approach が「主張は、それを決めるものと同じ場所に置く」。
  各 R に「結果」が入っている
- `benchmarks/results/residual-sweep-2026-08-22.txt` — §1〜§5。**落とし方の表**が各節にある
- `e2e/README.md` の「その実物を作った」節 — R3 の測定結果と、`file://` を使って
  ハーネスが自分で dead link を作った件

## Load-bearing context

- **`e2e/micro-dep` は 2 つの配線で使われる** — `e2e/micro` からは path (版固定できない側)、
  `tools/pinned-dep-gate.sh` からは git (版固定できる側)。**同じフィクスチャであることが比較の要**。
  片方だけ変えない
- **`tools/pinned-dep-gate.sh` の `insteadOf` は必須** — `file://` を manifest に入れると
  `site-gate.sh` がそれを内部リンクと判定して **dead link 5 本**を出す。
  **製品の失敗と見分けがつかない**
- **NOTICE の導出セクションは手で編集しない** — ゲートが両方向で見ている。
  依存が変わったら NOTICE に行を足す (deny.toml を広げても notice は満たされない)
- **Mathlib のモジュール一覧を olean から作らない** — 旧版の残留が 54 件混ざる。ソース木から作る
- **`NameIndex::known_modules` は綴りの混合物** — IR が `«Dep-Aux».Basic` を、
  `.lidx` の `@` 節が同じモジュールの `Dep-Aux.Basic` を入れる。
  **「その集合に居るか」を「解決経路が到達するか」の代わりに使わない**
  (実際にこれで写像が空になった → `benchmarks/results/residual-sweep-2026-08-22.txt` §5)
- **CLAUDE.md の罠は全部そのまま効いている** — `mise exec --` 越し、パイプで終了コードを見ない、
  `pgrep -f 'litedoc4 watch'` を先に見る、`rg -r` を束ねない。
  **今回新しく踏んだのは「push 前に `cargo doc` を回していなかった」** (CI を 1 回赤くした)
