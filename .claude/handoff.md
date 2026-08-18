# Handoff — 2026-08-18 (3)

## State

- Branch: main / **clean** / push 済み。CI と `lake package (self-test)` は
  **両方緑** (run 32149030757 / 32149030748)
- Active phase: **なし。ユーザーの指示待ち**
- 前 leg で残した「次にやること」のうち **1 (Lean v4.33 対応) と 4 (approach.md の圧縮) を完遂**。
  **4 は要約ではなく分割で**やった【ユーザー判断】
- 手元: `/private/tmp/lean-doc-relay/lean433` は**削除済み** (1.1 GB あった)。
  elan の **v4.32.2 / v4.33.0 は残してある** (既定は v4.31.0)。各 2.6 GB だが、
  **3 版でのビルドを再現する唯一の機材**なので消していない

## Where we are

**残っている未検証項目は 3 件のまま** (他人のリポジトリから `uses:` される /
ギュメ付きモジュールを持つ「版固定できる依存」/ より大きいパッケージ・RAM の小さいランナー)。
**U5 が増やした既知の欠陥 1 件は閉じた。**

| | |
|---|---|
| **Lean v4.33 対応** | **3 版 (v4.31.0 / v4.32.2 / v4.33.0) で建つ**。直し方は版分岐ではなく、**列挙を Lean 自身の `ReducibilityStatus.toAttrString` に委ねた**。古い版の IR は **436/436 byte 一致** (分岐は 75 宣言で実際に踏む)。v4.33 との差は **4 宣言の `implicit_reducible` → `instance_reducible` だけ** |
| **approach.md** | 613 行 → **3 分割** (`approach.md` 226 / `approach-pillars.md` 288 §5 / `approach-performance.md` 149 §6)。**節番号は振り直していない** — 番号は docs と凍結ログから引かれている。`approach.md` 末尾に対応表 |

## Next step

**ユーザーの指示待ち。** 手を動かすなら:

1. **`failures` を誰も判定に使っていない** — 抽出器は宣言ごとのエラーを
   `failures (N)` と印字して**終了コード 0 で終わる**。v4.33 対応の assert 探針で
   **75 宣言が IR から消えても 0 だった**【実測】。CLAUDE.md の
   「出力と終了コードが食い違う形はゲートを嘘にする」の未処理分。
   **ただし「N>0 で落とす」は仕様判断** — 正当に失敗する宣言があるかを先に測る
2. **残り 3 件のどれか** — 一番効くのは「他人のリポジトリから使われる」で、
   **別リポジトリの作成が要る【ユーザー判断】**
3. **可動 `v0` tag** — `docs/plans/distribution.md` §152、まだ作っていない (local / remote とも未作成を確認)
4. **Mathlib 依存パッケージを v4.32 / v4.33 で建てる** — 今回測ったのは Mathlib 非依存の
   micro だけ。432 モジュールは **v4.31.0 でしか走らせていない**

## Files to read first

- **`benchmarks/results/lean-433-fix-2026-08-18.txt` — 今回の SoT (直した回)**。
  見つけた回は `lean-version-2026-08-18.txt`
- `docs/plans/unverified-sweep.md` §0.1 — 直し方の 4 択と、なぜ上 3 つが駄目か
- `extractor/Extract.lean` の `getCustomAttrs` — 変更箇所は 1 つだけ
- `docs/approach.md` 末尾 — §5・§6 の対応表

## Load-bearing context

- **版をまたぐ式は推測で書けない**【実測】 — `String.drop` はここでは `String.Slice` を返し、
  `dropRight` も `String.mk` も deprecated。**3 版で実際にコンパイルして選んだ** (5 候補中 2 つが通った)
- **`| _ => pure ()` は「建つ」が「正しい」ではない**。今回それで塞いでいたら、
  v4.33 で **4 宣言の属性が黙って消えていた**。U5 の時点でそう書いてあったのを実際に確認した
- **否定対照を必ず取る**: 同じ v4.33 の複製・同じコマンドで**ソースだけ修正前に戻すと落ちる**。
  「直したから建つようになった」はこれで初めて言える
- **assert のコストは 1 行では言えない** — 火は噴くが**プロセスは 0 で終わる** (上の Next step 1)。
  コメントにその旨を書いてある。「throw する」とだけ書くと嘘になる
- **docs を分割するときは節番号を振り直さない**。`verification-log.md` の
  「直した文書: approach.md §5.5」のような**日付つきの記録**は書き換えない (歴史の改竄になる) —
  代わりに元ファイルに対応表を置いて解決させる
