# Handoff — 2026-08-22 (leg 2 の終わり)

## State

- Branch: **`main`** / clean / `bd5bbe0` (PR #2 のマージコミット) まで push 済み
- **CI は main で緑** (`32540918723` completed success)
- **`docs/plans/feature-sweep.md` の 8 項目は全部入った。束 A・B・C 完了。**
  計画 §5 の完了条件 6 項目すべて満たしてある (根拠は
  `benchmarks/results/bundle-c-2026-08-22.txt` §3)
- `cargo test --workspace` は **36 バイナリ緑 / 失敗 0**
- 作業領域: `/private/tmp/lean-doc-relay` は 34 MB まで掃除済み。
  残してあるのは `mathml/ir` (対象 422 モジュールの IR、2026-08-22 抽出) と
  `mathml/litedoc4-before` / `litedoc4-after` (C-1 前後の比較用バイナリ)。
  **stray な `litedoc4 watch` は無い**

## Relay control

- Mode: **DONE**
- Goal: **達成した。** `docs/plans/feature-sweep.md` の 8 項目を束 A → B → C で完遂。
- Leg: 2 / cap 8
- Stop-on: **completion で終端した**

**この連鎖は 2 度切れている。記録として残す**【2026-08-22】:

- **leg 1 が後続を一度も起こさなかった。** `dabd2de` から `d0538b4` まで 10 commit のあいだ
  `Leg: 1` のままで、`relay-launch-leg.sh` が走った痕跡も tmux セッションも無い。
  束 A と束 B は 1 leg でやり切られている。
- **leg 2 (このセッション) は `/relay` ではなく `/carryon` で起こされ**、
  `## Relay control` の `Mode: ON` をデータとして読んで `Skill(relay)` を呼ばなかった。
  結果、C-4 の直前で **carryon 停止条件 #3 (context 圧迫) をそのまま適用して停止した** —
  relay では**終端してはいけない唯一の条件**で、refresh (自分で次 leg を起こす) に
  読み替えるべきだった。ユーザーが手で再開させて C-4 が入った。

**直したのは carryon 側**【ユーザー判断 2026-08-22】 — `.claude/skills/carryon/SKILL.md`
の step 2 に「`## Relay control` があり `Mode: ON` なら先に `Skill(relay)` を呼ぶ」を足した。
`/relay` で起こされたかどうかに依らず override が載る。
- Progress ledger:
  - r1: **束 A** = A-1 `baab197`+`7ef9488`、A-2 `19ffb9f` / **B-0** `419061c` /
    **束 B** = B-1 `8318e6c`、B-2 `4e79978`、B-3 `82c049a` / 版 0.2.0 `e591562`
  - r2: **束 C** = C-1 `aaba189`、C-2 `6222052`+`e291147`、C-3 `25f03fc`、C-4 `5101d15`。
    PR #2 でマージ (`bd5bbe0`)。**ユーザー判断を 3 件仰いだ** (下記)

## このセッションで入ったもの

| | 内容 | ログ |
|---|---|---|
| C-1 | 数式をビルド時に MathML へ焼く。閲覧側に JS も webfont も足さない | `benchmarks/results/mathml-2026-08-22.txt` |
| C-2 | 宣言単位の逆引き `Used by`。`instances` と同じ空 details + `declarations/used-by.json` | `usedby-2026-08-22.txt` |
| C-3 | `litedoc4.toml` で題名と index ページを設定 | `config-2026-08-22.txt` |
| C-4 | フィクスチャ再凍結 + `MIN_SCHEMA_VERSION` 4 → 5 | `bundle-c-2026-08-22.txt` |

### ユーザー判断 3 件 (計画に反映済み)

1. **数式クレートは `math-core`** (計画の決定 4 は `pulldown-latex` だった)。
   `pulldown-latex` が `$a < b$` を `<mo><</mo>` と書くため — Mathlib の 2,123 span 中 61 件。
   代償は依存 **30 → 49 crate**。`deny.toml` の「deliberately small」はこの日書き換えた。
2. **`docgen4-expected.json` も決定 1 どおりこちらの出力で再凍結**
   (「宣言済み乖離 5 件」として doc-gen4 オラクルを残す案は採らなかった)。
   **失ったもの**: `cargo test` は「CommonMark の dialect が動いていない」を主張しなくなった。
   オラクル (`tests/oracle/gen-docgen4-expected.ts`) は回せるままで、両 `PROVENANCE.md` に書いた。
3. **`MIN_SCHEMA_VERSION` を 5 に上げた。** 埋め込み IR 62 ファイルを一度きりの文字列置換で移行。

### 恒久的に効くようになったもの

- **`LITEDOC4_BLESS=1`** — `docgen4-expected.json` と `page-parts-expected.json` の再生成手段。
  変えた case を全部印字し、編集前の round-trip を検査し、**冪等であることを主張する**。
  `page_parts` の bless は **`header` が動いていたら書かずに落ちる**。
- **`RENDERER_ID` v3** — 描画バイトが動いたら上げる互換トークン。上げ忘れると増分ビルドが
  「0 pages rendered」で成功に見えたまま古いバイトを残す。
- **新しいゲート 5 本** — e2e GATE 10 / 11 (`tools/usedby-gate.sh`) / 12 (`tools/config-gate.sh`)、
  ブラウザ検査 4b / 8b。**どれも一度落としてから通した記録がレポートにある**。

## Next step — 残件は 1 つだけ

**`crates/litedoc4-global/tests/state_and_delta.rs` の
`the_state_file_is_the_prototypes_bytes` が固定する 861,999 B が C-2 で動いた。**

- C-2 が `ModuleFacts::refs` を足したので state が増えた (422 モジュールの IR では
  838,324 → 1,310,764 B)。固定値は **432 モジュールの corpus** で測ったもの
- **測り直せていない** — `/private/tmp/lean-doc-relay/w7h/base-ir` はこの機材に無く、
  対象パッケージ自体ももう 432 モジュールではない
- **黙って嘘をついてはいない**: テストは `#[ignore]` で、fixture が無いと
  `LITEDOC4_PROTOTYPE_STATE` の読み込みで落ちる (byte 比較の手前)。
  テスト本体にその旨をコメントで書いてある

潰し方は 2 通りあり、どちらもユーザー判断を要する:
(a) 432 モジュールの corpus と prototype state を復元して測り直す、
(b) このテストを今の機材で回せる形に作り直す (プロトタイプとの byte 比較をやめる)。

**それ以外に「次にやること」は決まっていない。** `docs/implementation-plan.md` §1 末尾の
未検証 3 件と `docs/provenance.md` §8 の未検証項目
(**NOTICE が MIT 単独の推移的依存を網羅していない** — 依存が 49 crate に増えた直後なので
特に) が候補。

## Files to read first

- `docs/plans/feature-sweep.md` — §3.5 が束 C の決着、§4 の各項目に「結果」節がある
- `benchmarks/results/bundle-c-2026-08-22.txt` — 再凍結の全件レビューと、
  対象での full / 1 宣言追加 / no-op の整数
- `crates/litedoc4-md/tests/data/PROVENANCE.md` と
  `crates/litedoc4-render/tests/data/PROVENANCE.md` — 何がどちらの出力なのか、
  何を失ったのか。**次にフィクスチャを触る人はここから読む**

## Load-bearing context

- **`e2e/micro` に `litedoc4.toml` と `docs/index.md` がある** — 何も設定していない
  パッケージでは 4 経路が自明に一致してしまい、config-gate が「通るだけのゲート」になるため。
  **「消し忘れ」ではない。**
- **`e2e/micro/Micro/Math.lean` の `\colim` を「動く命令」に直さない** — それが入力。
- **CLAUDE.md の罠は全部そのまま効いている** — `mise exec --` 越しに呼ぶ、
  パイプで終了コードを見ない、`pgrep -f 'litedoc4 watch'` を先に見る、
  ブラウザゲートの後に `pkill -f check-site-browser.ts`、`rg -r` を束ねない。
