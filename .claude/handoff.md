# Handoff — 2026-08-22 (leg 2)

## State

- Branch: **`bundle-c`** / clean / `25f03fc` まで push 済み。**PR #2 が open**
  (https://github.com/FujiHaruka/litedoc4/pull/2)。`main` は `d0538b4` のまま触っていない
- Active phase: `docs/plans/feature-sweep.md` の **束 C**。**C-1 / C-2 / C-3 完了、C-4 だけ残っている**
- CI (`32534654072`): `test` ジョブだけ赤。**内訳は凍結フィクスチャ 3 本で、これは C-4 まで想定どおり**。
  `e2e` (Linux、実抽出器 + ブラウザゲート) と `supply chain` は緑
- 計測環境: 対象 IR は `/private/tmp/lean-doc-relay/mathml/ir` (422 モジュール、2026-08-22 抽出)。
  olean は暖まっている。`/private/tmp/lean-doc-relay` に e2e-c1/c2/c3 と mathml が残っている (数 GB ではない)

## なぜブランチなのか

束 C は**描画が動く唯一の束**で、C-4 の再凍結まで `cargo test` が赤い。
main を赤くしないためにブランチにした【判断 2026-08-22】。C-4 が終わって緑になったら main へ。

## Relay control

- Mode: ON
- Goal: `docs/plans/feature-sweep.md` の 8 項目を束 A → B → C で完遂。各項目は
  「ゲート/テスト・撤退ライン・実測ログ」の 3 点が揃うまで終わりにしない。
  新しいゲートは必ず一度落としてから通す。完了条件は計画 §5 の 6 項目すべて。
- Leg: 2 / cap 8
- Predecessor: leg 1 (束 A・B)
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 計画 `561db4c` / **束 A** = A-1 `baab197`+`7ef9488`、A-2 `19ffb9f` /
    **B-0** `419061c` / **束 B** = B-1 `8318e6c`、B-2 `4e79978`、B-3 `82c049a` /
    版 0.2.0 `e591562`
  - r2: **束 C** = C-1 `aaba189`、C-2 設計 `6222052` + 実装 `e291147`、C-3 `25f03fc`。
    **C-4 だけが残っている。** ユーザー判断を 1 件仰いだ (数式クレートを math-core に差し替え)

## Next step — C-4 フィクスチャの再凍結

計画 §3【決定 1】の 3 手順 + 残 2 件。**この段は戻しにくい** (計画 §7)。
**差分レビューで説明できないものが 1 つでもあれば止める。**

### 4-1. `LITEDOC4_BLESS=1` の再生成手段を HEAD に置く

落ちている 3 本と、**それぞれ扱いが違う**。ここが C-4 の設計上の要点:

| フィクスチャ | 出所 | 生成器 | 扱い |
|---|---|---|---|
| `litedoc4-md/tests/data/docgen4-expected.json` | **doc-gen4 の出力** | **HEAD に在る** (`tests/oracle/gen-docgen4-expected.ts` + `dump-html.lean`) | **こちらの出力で上書きしない** |
| `litedoc4-md/tests/data/ts-docstring-expected.json` | 凍結プロトタイプ | 無い (`experiments-frozen`) | **BLESS でこちらの出力に切り替える** |
| `litedoc4-render/tests/data/page-parts-expected.json` | 凍結プロトタイプ | 無い (同上) | **BLESS でこちらの出力に切り替える** |

**`docgen4-expected.json` を上書きしない理由**【判断 2026-08-22、要ユーザー確認】:
計画 §3 手順 3 は `docgen4-linked-expected.json` について
「これは doc-gen4 の出力が期待値なので… こちらの出力で上書きしない」と書いている。
**同じ理由が `docgen4-expected.json` にもそのまま当てはまる** — 生成器が HEAD に在り、
**327 件中 322 件は今も doc-gen4 と byte 一致している**。上書きすると
「dialect が動いていない」という生きたオラクルを 322 件ぶん捨てることになる。

代わりに **declared divergence** にする。**この木には既に前例がある** —
`crates/litedoc4-md/tests/ts_docstring.rs:183` の
`the_disagreements_are_the_subsets_declared_gaps`。形も同じで、
**「一致してしまった」も失敗として報告する**両側検査なので「例外リストで黙って飲む」に
ならない。数式 5 件を名前で列挙し、件数も主張する。テスト名は
`every_case_matches_doc_gen4` → **乖離があることが名前から分かるものに変える**。

乖離している 5 件 (`cargo test -p litedoc4-md --test docgen4` が印字する):
`curated: inline math` / `curated: display math` / `curated: math with markdown inside` /
`html: escapes in math` / `html: table, markup in cells`。
`ts-docstring-expected.json` の 1 件は表のセル内の `$e$`。

### 4-2. 差分を全件レビューする

変わったファイルごとに「どの項目のどの変更で変わったか」を 1 行。**今分かっている全部**:

- `page-parts-expected.json` 187 件全部 → **C-2**。全宣言の末尾に
  `<details class="extra" data-fill="used-by" data-name="…"><summary>Used by</summary><ul></ul></details>` が付いた。
  **他の差は無いはず** — C-1 の数式はこの fixture の入力に無く、C-3 は題名だけで宣言ブロックに触らない
- `ts-docstring-expected.json` 1 件 → **C-1**。表のセル内 `$e$` が `<math>` になった
- `docgen4-expected.json` 5 件 → **C-1**。上記のとおり上書きしない

### 4-3. `PROVENANCE.md` を 2 つ書き換える

- `crates/litedoc4-render/tests/data/PROVENANCE.md` — `page-parts-expected.json` の役割が
  「プロトタイプの出力」から「自分の出力に対する回帰基準 (2026-08-22 以降)」に変わる。
  **この文書自身が「オラクルを失ったならテスト名にそう書け」と指示している**ので従う
- `crates/litedoc4-md/tests/data/PROVENANCE.md` — `ts-docstring-expected.json` を同上。
  `docgen4-expected.json` は**役割は変わらない**が、5 件の宣言済み乖離を書く

### 4-4. `MIN_SCHEMA_VERSION` を 4 → 5

`crates/litedoc4-ir/src/reader.rs:35`。**B-1 が保留し、reader.rs のコメントが
「C-4 でやること」と名指ししている。**

**引っかかる点**: 2 つの fixture が **schema 4 の IR を入力として埋め込んでいる**
(エスケープされた JSON 文字列):

    crates/litedoc4-global/tests/data/global-expected.json   `"ir":{"deps/Dep.json":"{\"schemaVersion\":4,…`
    crates/litedoc4-render/tests/data/pages-expected.json     `…\"schemaVersion\":4}"`

床を上げるとこの**入力**が弾かれる。入力は BLESS では再生成されない (期待値ではない)。
**方針案**: 一度きりの機械的な移行 (埋め込み IR の `schemaVersion` を 4 → 5 にする
スクリプトを回して差分を記録する)。「テストを通すために期待値を手で書き換える」のとは
別の行為だが、**PROVENANCE の「JSON を手で編集しない」に触れるので、やり方を先に決めること**。

### 4-5. 完了条件の残り (計画 §5)

- **条件 5**: 対象 422 モジュールで **full / 1 宣言追加 / no-op を測り直す**。
  **束 C は遅くしている** (C-2 が +5.3%)。`benchmarks/results/` に記録し、
  **遅くなった理由を書く (隠さない)**
- **条件 6**: README を**最終状態だけ**で更新する。差分・過程・経緯を書かない。
  **数式が入ったので「No」の 1 つ目が消える。** `Used by` と `litedoc4.toml` も
  利用者向けに書く必要がある (`litedoc4.toml` は利用者が触るファイル)
- 条件 1〜4 (CI が回すもの全部 / 既存ゲート / 新ゲートは一度落とす / 走った本数) は
  **C-4 の後にもう一度全部回す**

## Files to read first

- `docs/plans/feature-sweep.md` — SoT。§3【決定 1】/ §4 C-1〜C-4 の「結果」節 / §5 完了条件 / §9
- `crates/litedoc4-md/tests/ts_docstring.rs:177-209` — **declared divergence の前例**。C-4 はこれに倣う
- `crates/litedoc4-render/tests/data/PROVENANCE.md` — 「JSON を編集するな、生成器を復元するか、
  自分の出力に対する回帰テストに置き換えてテスト名にそう書け」
- `benchmarks/results/{mathml,usedby,config}-2026-08-22.txt` — C-1〜C-3 の数字と、
  ゲートを落とした記録

## Load-bearing context

- **束 C は `RENDERER_ID` を v2 → v3 に上げた** (`litedoc4-incr/src/ledger.rs`)。
  IR schema は 5 のまま、ワークスペースの版は 0.2.0 のまま。
- **ユーザー判断が 1 件出ている**: 数式クレートを `pulldown-latex` → **`math-core`**。
  理由は `pulldown-latex` が `$a < b$` を `<mo><</mo>` と書くこと (Mathlib の 2.9%)。
  代償は依存 30 → 49 crate。`deny.toml` の「deliberately small」はこの日書き換えた。
  **NOTICE が MIT 単独の推移的依存を網羅していない**ことが分かり、
  `docs/provenance.md` §8 に未検証項目として起票してある。
- **`the_state_file_is_the_prototypes_bytes` が固定する 861,999 B は C-2 で動いた。**
  432 モジュールの corpus がこの機材に無く測り直せていない。テストは `#[ignore]` で
  fixture 不在なら byte 比較の手前で落ちるので嘘はついていない。テスト本体にもその旨を書いた。
- **`e2e/micro` に `litedoc4.toml` と `docs/index.md` を置いた** — 何も設定していない
  パッケージでは 4 経路が自明に一致してしまい、config-gate が「通るだけのゲート」になるため。
- 計測用バイナリ: `/private/tmp/lean-doc-relay/mathml/litedoc4-before` (C-1 以前) と
  `litedoc4-after` (C-1 まで) を残してある。C-4 の測り直しで使える。
- **CLAUDE.md の罠は全部そのまま効いている** — `mise exec --` 越しに呼ぶ、パイプで終了コードを
  見ない、`pgrep -f 'litedoc4 watch'` を先に見る、ブラウザゲートの後に
  `pkill -f check-site-browser.ts`。
