# extractor — lean-doc の抽出器 (Lean のまま)

対象パッケージの olean を `importModules` で読み、モジュール単位の IR (schema 4) を書く。
**これだけが Lean で、外側 (IR 消費・レンダリング・増分・検索索引) は Rust**
(`docs/implementation-plan.md` §5.6)。抽出器が Lean なのは速度の話ではなく、
**対象の Lean 環境の中でしか動けない**から — delaborator も `getEqnsFor?` も
`findDocString?` も Lean のプロセスの中にしか無い。

移動元は `experiments/stage7d/Extract.lean` (2,784 行) と `experiments/stage7d/build.sh` (31 行)。
M4-a で**移設ではなく移動**した (Lean のまま)。`experiments/` は凍結なので向こうは 1 バイトも
触っていない — `docs/verification-log.md` の数字はあちらのバイナリで取られている。

| ファイル | 行 | |
|---|---:|---|
| `Extract.lean` | 2,954 | 抽出器本体。IR schema 4 + `--link-index` (M5-a) |
| `build.sh` | 39 | `lake env lean` → `lake env leanc -rdynamic` の 2 段 |
| `build/` | — | 生成物 (171 MB のバイナリ + 2.7 MB の C)。**gitignored** |

## ビルドと実行

```sh
TARGET_REPO=/path/to/lean-project extractor/build.sh   # -> extractor/build/extract
lean-doc extract --modules <list> --ir-dir <dir> --timings <file> \
  --extractor-bin extractor/build/extract --target /path/to/lean-project --jobs 4
```

`--link-index` はまだ `lean-doc extract` から渡せない (製品側の配線は M5-b)。直に叩く形:

```sh
cd /path/to/lean-project && lake env /path/to/lean-doc/extractor/build/extract \
  modules.txt events.jsonl --skip-analyze --link-index link-index.lidx
```

lean-doc 側に toolchain も lakefile も Mathlib も置かない (CLAUDE.md)。環境は
`lake env` で対象から借りるので、**対象が `lake build` 済みであることがビルドの前提**。
バイナリは対象の toolchain に対して作られるため、**別の対象には作り直しが要る**。

直接叩く形 (`extract <modules.txt> <events.jsonl> [options]`) は `Extract.lean` の
ヘッダにある。製品の呼び手は `lean-doc extract` (M4-b) で、その先は
`lean-doc incremental --extractor` (M3-d2)。

## M5-a で足したもの — `--link-index <path>`

依存クロージャの「名前 → モジュール」写像 (`.lidx`) を、**抽出のために読んだ環境から**
書き出す経路 (`writeLinkIndex`)。レンダラが docstring の autolink を解決する入力で、
これまでは doc-gen4 のサイトの `declarations/declaration-data.bmp` から作っていた
(`experiments/stage7d/build-link-index.ts`) が、その経路は上流の公開サイトが**同じ Lean /
Mathlib** であることを前提にしていて、この対象では成り立たない (計画 §4、実測)。

**IR は 1 バイトも動かない** — `--link-index` を足した状態でフラグ一式
(`--equations --refs --write-ir --tagged-code --jobs 4`) を回し、M4-a のゲートの参照 IR と
`diff -r` して**436/436 バイト一致**【実測 2026-08-15】。

## 移動で挙動を変えた点 — 全部

**IR のバイトは 1 つも動かない。**変えたのはコマンドラインの表面だけで、内訳は次の 5 件。
確認は `diff experiments/stage7d/Extract.lean extractor/Extract.lean` — 移動直後は
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
**M4-c**。`lean-doc extract` は `--serve*` を名指しで断る。

## ゲート — 凍結バイナリとの IR バイト一致

同じモジュール一覧 (`lean-doc modules --root … --lib InformationTheory`、432 件、
UTF-16 code unit 順) を**両側に同じファイルで**渡し、同じフラグ
(`--equations --refs --write-ir --tagged-code --jobs 4 --ir-dir <dir>`) で走らせて、
IR 木を全ファイル `cmp` する。

一覧を共有するのは、**抽出器が渡されたリスト順をそのまま `index.json` に書く**から
【実測、計画 §7 M3-d2】 — 別々に作った一覧だと中身と無関係に落ちる。

| | |
|---|---|
| 参照 | `experiments/stage7d/build/extract` (凍結。**実行するだけ、再ビルドしない**) |
| 候補 | `extractor/build/extract` |
| 結果 | **436/436 バイト一致** (432 モジュール + `index.json` + `deps/*.json` 3)【実測 2026-08-15】 |

`diff -r` も `IDENTICAL`。差分 0 / 欠落 0 / 余分 0。
再実行は `extractor/build.sh` してから両側を上のフラグで回すだけ。
