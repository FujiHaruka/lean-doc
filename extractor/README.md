# extractor — litedoc4 の抽出器 (Lean のまま)

対象パッケージの olean を `importModules` で読み、モジュール単位の IR (schema 5) を書く。
**これだけが Lean で、外側 (IR 消費・レンダリング・増分・検索索引) は Rust**
(`docs/implementation-plan.md` §5.6)。抽出器が Lean なのは速度の話ではなく、
**対象の Lean 環境の中でしか動けない**から — delaborator も `getEqnsFor?` も
`findDocString?` も Lean のプロセスの中にしか無い。

移動元は `experiments/stage7d/Extract.lean` (2,784 行) と `experiments/stage7d/build.sh` (31 行)。
M4-a で**移設ではなく移動**した (Lean のまま)。移動中 `experiments/` は凍結していたので
向こうは 1 バイトも触っていない — `docs/verification-log.md` の数字はあちらのバイナリで取られている。
**`experiments/` は 2026-08-16 に HEAD から撤去した** (tag `experiments-frozen`)。

| ファイル | 行 | |
|---|---:|---|
| `Extract.lean` | 2,954 | 抽出器本体。IR schema 5 + `--link-index` (M5-a) |
| `build.sh` | 39 | `lake env lean` → `lake env leanc -rdynamic` の 2 段 |
| `build/` | — | 生成物 (171 MB のバイナリ + 2.7 MB の C)。**gitignored** |

## ビルドと実行

```sh
TARGET_REPO=/path/to/lean-project extractor/build.sh   # -> extractor/build/extract
litedoc4 extract --modules <list> --ir-dir <dir> --timings <file> \
  --extractor-bin extractor/build/extract --target /path/to/lean-project --jobs 4
```

`--link-index` は **`litedoc4 extract --link-index <p>` から渡せる** (製品側の配線は M5-b 済み)。
抽出を伴わずに写像だけ作る / 計測するときは直に叩く形:

```sh
cd /path/to/lean-project && lake env /path/to/litedoc4/extractor/build/extract \
  modules.txt events.jsonl --skip-analyze --link-index link-index.lidx
```

litedoc4 側に toolchain も lakefile も Mathlib も置かない (CLAUDE.md)。環境は
`lake env` で対象から借りるので、**対象が `lake build` 済みであることがビルドの前提**。
バイナリは対象の toolchain に対して作られるため、**別の対象には作り直しが要る**。

直接叩く形 (`extract <modules.txt> <events.jsonl> [options]`) は `Extract.lean` の
ヘッダにある。製品の呼び手は `litedoc4 extract` (M4-b) で、その先は
`litedoc4 incremental --extractor` (M3-d2)。

## M5-a で足したもの — `--link-index <path>`

依存クロージャの「名前 → モジュール」写像 (`.lidx`) を、**抽出のために読んだ環境から**
書き出す経路 (`writeLinkIndex`)。レンダラが docstring の autolink を解決する入力で、
これまでは doc-gen4 のサイトの `declarations/declaration-data.bmp` から作っていた
(`experiments/stage7d/build-link-index.ts`) が、その経路は上流の公開サイトが**同じ Lean /
Mathlib** であることを前提にしていて、この対象では成り立たない (計画 §4、実測)。

**IR は 1 バイトも動かない** — `--link-index` を足した状態でフラグ一式
(`--equations --refs --write-ir --tagged-code --jobs 4`) を回し、M4-a のゲートの参照 IR と
`diff -r` して**436/436 バイト一致**【実測 2026-08-15】。

## 段 C / 段 D で足したもの — `--link-index-omit` と `--link-index-key`

どちらも `--link-index` と組でのみ意味を持つ (単独で渡すと usage エラー)。
設計と数字は `docs/plans/reextract-count.md` §6、実測は
`benchmarks/results/lidx-own-half-2026-08-17.txt` と `g3-stage-*-2026-08-17.txt`。

| フラグ | 何をするか |
|---|---|
| `--link-index-omit <modules.txt>` | **段 C**。ここに名前がある モジュールの**宣言群を書かない** (`@` 節には残す)。レンダラは自パッケージの名前を IR 由来の索引で先に解決するので、この 3.6% は読まれない — **落としてもサイトは 429/429 バイト一致**。落とす理由は速度ではなく、**そこだけが 1 モジュール編集で動く**ため (動くと `renderKey` が動き全ページ再描画になる) |
| `--link-index-key <token>` | **段 D**。地図の隣に `<path>.key` を置き、次回**トークン一致 + `#lidx2` マーカー一致 + `@` 節が現環境と一致**なら**走査ごと飛ばす**。トークンは呼び手が作る不透明文字列で、抽出器から見えないもの (依存 olean の同一性 = `extractKey`、omit 一覧の中身) を担う |

製品側 (`litedoc4 build` / `incremental --serve`) は**両方を自動で渡す**ので、
利用者がこのフラグを意識することはない (`crates/litedoc4/src/resident.rs`)。
手で叩くときだけ意味がある。

## M7-a で変えたもの — `.lidx` の 1 行に**ソース行範囲**が乗る

依存へのリンクを版固定の GitHub blob URL (`…/Mathlib/Order/Basic.lean#L67-L67`) にするための
入力 (計画 §M7)。マーカーが `#lidx1` → **`#lidx2`**、宣言の行が
`\t<name>` から `\t<name>\t<line>\t<endLine>` になる。値は `findDeclarationRanges?` の
`range.pos.line` / `range.endPos.line` で、IR の `line`/`endLine` と同じ 2 フィールド。

**行範囲が取れない宣言は 1 フィールドのまま残す** — 落とすとリンクごと消えるが、
範囲が無いだけならアンカーが落ちるだけ (doc-gen4 の `gh_nav_link` と同じ形)。
読む側 (`crates/litedoc4-render/src/link_index.rs`) は `#lidx1` も読み続ける。

| | 前 | 後 |
|---|---:|---:|
| `.lidx` バイト | 8,465,776 | **10,464,171** (+23.6%) |
| 宣言数 / うち行範囲あり | 255,975 / — | 255,975 / **255,975 (100%)** |
| `linkIndex` フェーズ warm 中央値 | 0.927 s | **1.203 s** (+29.8%) |
| `linkIndex` フェーズ cold 中央値 | 1.504 s | **1.770 s** (+17.7%) |

【実測 2026-08-16、n=5 ずつ → `benchmarks/results/m7a-summary.txt`】。
参照木由来のオラクル 241,553 件に対し、`.lidx` から組んだ URL は
**235,185 件を突き合わせて不一致 0**【実測、母数と残り 3 バケットは同ファイル】。

## 移動で挙動を変えた点 — 全部

**IR のバイトは 1 つも動かない。**変えたのはコマンドラインの表面だけで、内訳は次の 5 件。
確認は `diff <(git show experiments-frozen:experiments/stage7d/Extract.lean) extractor/Extract.lean`
(`experiments/` は撤去済なので tag 経由で読む) — 移動直後は
**11 hunk / 86 行**ですべて下の 5 件のいずれかだった。**現在は 17 hunk / 218 行** で、
増えた 6 hunk / 133 行は上の `--link-index` (M5-a)。

1. **`defaultIrDir` を削除した**。旧セッションの scratchpad 絶対パス
   (`/private/tmp/claude-502/…/2dbcb565-…/scratchpad/ir-tagged`) が焼かれていた。
   呼び手が常に `--ir-dir` を渡すので発火したことは無く、だからこそ footgun —
   `--write-ir` でフラグを忘れた実行が**誰も指定していない場所に数 MB 書いて成功を報告する**。
   製品ツリーではそのパスは他人の機械に存在すらしない。
2. **`--ir-dir` を `--write-ir` の必須引数にした**。欠けていたら**引数解析の時点で**
   exit 1 (`parseArgs` の `check`)。使う場所 (IR 書き出し) は 20 秒の抽出の最後なので、
   そこで落とすと全部払ってから usage エラーが届く。
3. **`IR_DIR` 環境変数を読むのをやめた**。フラグの値がコマンドラインの外から来る経路で、
   別実行の export が残っていると**コマンドラインが完全に見える実行の IR が黙って逸れる**。
   1 と同じ穴の入口違い。読む側 (`benchmarks/tools/read-ir.ts` / `html-inventory.py` の
   `IR_DIR`) は無関係で、そのまま。
4. **`resolveIrDir` → `getIrDir`** に改名。「3 つの供給源から解決する」関数ではなくなったので。
   `--ir-dir` が無ければ `IO.userError`。2 があるので実際には到達しないが、
   「実際には」を検査可能にするために残してある。
5. **ヘッダと usage 文字列**を製品ツリーの文脈に書き直した (`--ir-dir` の必須化を含む)。

`build.sh` は**形を変えていない** — `env.sh` への相対パスが 1 階層上がった
(`../../benchmarks/tools/env.sh` → `../benchmarks/tools/env.sh`) だけ。
特に **`leanc -rdynamic` は load-bearing**: `importModules (loadExts := true)` が
Lean インタプリタでモジュール初期化子を走らせ、実行中の実行ファイルからシンボルを解決する
(Lake の `supportInterpreter := true`)。外すと
"Could not find native implementation of external declaration" で死ぬ。

**まだ変えていないもの**: `--serve` (常駐) はバイナリに残っているが、製品側から配線するのは
**M4-c**。`litedoc4 extract` は `--serve*` を名指しで断る。

## ゲート — 凍結バイナリとの IR バイト一致

同じモジュール一覧 (`litedoc4 modules --root … --lib InformationTheory`、432 件、
UTF-16 code unit 順) を**両側に同じファイルで**渡し、同じフラグ
(`--equations --refs --write-ir --tagged-code --jobs 4 --ir-dir <dir>`) で走らせて、
IR 木を全ファイル `cmp` する。

一覧を共有するのは、**抽出器が渡されたリスト順をそのまま `index.json` に書く**から
【実測、`docs/milestone-log.md` の M3-d2】 — 別々に作った一覧だと中身と無関係に落ちる。

| | |
|---|---|
| 参照 | `experiments/stage7d/build/extract` (凍結。**実行するだけ、再ビルドしない**) |
| 候補 | `extractor/build/extract` |
| 結果 | **436/436 バイト一致** (432 モジュール + `index.json` + `deps/*.json` 3)【実測 2026-08-15】 |

**このゲートはもう回せない。** 参照側のバイナリは `.gitignore` 対象で
tag `experiments-frozen` にも入っておらず (ソースの `Extract.lean` は入っている)、
撤去とは無関係に **M4-a の時点からローカルファイル 1 個に依存していた**。
**その 171 MB は 2026-08-16 に消した。** 回すには tag から
`experiments/stage7d/Extract.lean` を取り出し、当時と同じ toolchain で
ビルドし直すところから要る。**上の 436/436 は書き換えない** — 2026-08-15 に
実際に出た結果であることは変わらない。**再現手段が無い実測**として読むこと。

`diff -r` も `IDENTICAL`。差分 0 / 欠落 0 / 余分 0。
再実行は `extractor/build.sh` してから両側を上のフラグで回すだけ。
