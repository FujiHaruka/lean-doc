# Lake パッケージ化 — lean-doc を dev 依存として `require` させる

**状態**: **着手 (2026-08-18)**。ユーザー指示により L1 → L2 を通しで完遂する。

配布 (`docs/plans/distribution.md`) が扱ったのは **CI (GitHub Actions) の経路だけ**だった。
ローカルで使う利用者には、いまも「lean-doc を clone して 2 つのバイナリのパスを自分で管理する」
しか無い (README §Running it locally)。この計画はそこに **`require` という経路**を足す。

**前提はすべて実測で確定済み** → [`../../benchmarks/results/lake-package-probe-2026-08-18.txt`](../../benchmarks/results/lake-package-probe-2026-08-18.txt)。
推測で lakefile を書かない、が起点。

---

## 1. Context — いま利用者が背負っているもの

```bash
TARGET_REPO=/path/to/your-package extractor/build.sh   # -> extractor/build/extract
lean-doc build --root … --lib … --out … --extractor-bin ./extractor/build/extract
```

`--extractor-bin` に**既定値は無い** (`crates/lean-doc/src/main.rs:195`) — 「対象の toolchain に
対して建てるので、焼き込んだパスは 1 台でしか正しくない」という**正しい理由**による。
結果として利用者が 2 つのバイナリの所在を管理している。

**Lake 経路が構造的に埋めるもの**が 3 つある:

| | いま | `require` 後 |
|---|---|---|
| extractor の所在 | 利用者が `build.sh` を叩き、パスを渡す | **Lake が建てて、script が場所を知っている** |
| toolchain の一致 | `lake env` で対象の環境を借りて担保 | **依存として建つので構造的に一致** |
| `--lib` | `lakefile.lean` のプロジェクトでは**手で書く**必要がある (`lakefile.rs` が名前で拒否 — 正直に読むには Lake で elaborate するしかないため) | **script は Lake が elaborate した後に走る**ので Lake に聞ける |

3 つ目が効く。Mathlib も doc-gen4 も `lakefile.lean` なので、いま `--lib` の手書きは
**主要な利用者ほど必要になる**。`action.yml:70` と `ci-placement.yml:61` が同じ穴を
input で塞いでいるのは、Rust 側が原理的に踏み込まないと決めた領域だから。

---

## 2. Approach

> **配布物を増やすのではなく、「どのバイナリをどこから得るか」の判断を lakefile 1 箇所に集める。
> extractor は Lake に建てさせ、Rust は Release から取る。**

**なぜこの分担か。** 2 つのバイナリの性質は真逆で、その真逆さが GHA とローカルで**逆向きに効く**:

| | `extract` (Lean, 226 MB) | `lean-doc` (Rust, 2.5 MB) |
|---|---|---|
| 誰が建てられるか | **利用者は建てられる** (elan を持っている) | **建てられない** (rustup を持っているとは限らない) |
| 版の縛り | 対象の toolchain に厳密に縛られる | どこでも同じものが動く |
| 結論 | **Lake に建てさせる** | **Release から取る** |

`distribution.md` D4 が「extractor を配らない」と決めた根拠 (226 MB / 16 秒 / toolchain × OS の
マトリクス) は、この経路では**問い自体が消える** — 各利用者の Lake が建てるので、配る必要が無い。

**判断は 1 箇所に集める** (CLAUDE.md「欠陥を直すとき」)。バイナリの入手順序を決める場所は
lakefile の中の 1 関数にし、`action.yml` が既に払った授業料 —— **ref だけで版を選ぶと `@main` が
古いバイナリを掴む** —— の一般形をそこに移す: **要求された rev のツリーの `Cargo.toml` が
版の SoT**。

**新しい単一障害点を作らない。** ダウンロードは常に「取れなければ建てる / 明確に落ちる」。
L1 の時点で `lean-doc` が見つからない場合の経路を先に作り、L2 はその手前に 1 段足すだけにする。

**段は独立して価値がある。** L1 だけでも「`--lib` と `--extractor-bin` を書かなくてよくなる」が
手に入る。L2 が倒れても L1 は残る。

---

## 3. 確定した前提【すべて実測 2026-08-18】

計画の形を決めたのはこの 5 つ。出所は
[probe レポート](../../benchmarks/results/lake-package-probe-2026-08-18.txt)。

1. **`lean-toolchain` は置かない。** 置けるが、**置くと `lake update` が利用者の
   `lean-toolchain` を書き換えうる** (依存側の版が高いとき。しかも elan の入手失敗より前に
   書き換わる)。低いときは警告すら出ずに黙殺される。**置かなければ Lake は何も言わず root の
   toolchain を使う。** CLAUDE.md の「lean-doc 側に toolchain を置かない」は守れるだけでなく、
   **守るのが正解**だった。
2. **`lake-manifest.json` (`packages: []`) は置く。** 無いと利用者に毎回 warning が出る。
   lean-doc 自身では `lake update` を走らせられない (toolchain が無いので elan が解決できない)
   ので手書きになる。
3. **`lakefile.lean` にする。** `script` は `lakefile.toml` では書けず、**`[[script]]` は
   エラーも警告も無く黙殺される**。`lakefile.toml` は置かない (両方あると .lean が勝つが、
   info 1 行しか出ない = 気づけない)。
4. **`lean_exe extract where supportInterpreter := true` で建つ。**
   バイナリは build.sh のものと byte 一致**しない** (+308,032 B。package シンボル prefix と
   `-O3 -DNDEBUG`) が、**書く IR は 5 通りの起動すべてで byte 一致**。
5. **root からの起動は `Lake.exe`。** script の env に `LEAN_PATH` は入っていないので
   `getAugmentedEnv` を通す。**root の `lake build` が先に要る** (`lake exe` は root の lib を
   建てない)。

### この計画が変える主張の置き場所

`extractor-uniqueness-2026-08-18.txt` §6 は自分でこう限界を書いていた:

> **「起動できる」と「正しい IR を書く」は別**。ここで比べたのはバイナリの byte であって出力ではない

Lake 経路はバイナリの byte を**必ず動かす**ので、この限界を放置できない。
**オラクルを「バイナリの SHA 一致」から「IR の byte 一致」に移す。** これは M8 で
「doc-gen4 互換の byte 再現は追わない」と決めたのと同じ形の判断で、**追えないから諦めるのではなく、
主張したいことが最初から出力の同一性だったから**。

---

## 4. L1 — lakefile を置き、extractor を Lake に載せる

### L1-a. パッケージの骨格

| ファイル | 中身 |
|---|---|
| `lakefile.lean` | `package «lean-doc»` / `lean_exe extract` / `script docs` |
| `lake-manifest.json` | `{"version": …, "packages": []}` を手書き (前提 2) |
| `lean-toolchain` | **置かない** (前提 1) |
| `.gitignore` | ルートの `/.lake/` を追加 (path require で 171 MB がここに落ちる) |

`lean_exe extract` の `root` は `Extract`、パッケージの `srcDir` は `extractor`。
Lean のソースは `extractor/` にしか無いのでこれで足りる。

### L1-b. `script docs` が組み立てるもの

```
lean-doc build --root <workspace root> --lib <Lake から取った lean_lib>...
               --out <利用者が指定> --extractor-bin <Lake が建てた extract>
               --lake <lake> [--jobs N] [--source-url …]
```

- **`--lib` は Lake から取る** — `(← getWorkspace).root.leanLibs`。これが §1 の 3 つ目。
- **`--extractor-bin` は `Lake.exe` で建ててからパスを渡す**。ただし `Lake.exe` は
  「建てて起動する」ので、**建てるだけ**の経路が要る (→ 未決 D1)。
- **root の `lake build` が先に要る** (前提 5)。script がこれを保証する。
- `--out` は **利用者が指定**。既定値は置かない (→ 未決 D2)。

### L1-c. `lean-doc` (Rust) の見つけ方 — L1 の版

順序を 1 箇所 (`resolveLeanDoc`) に集め、**L2 はこの列に 1 段足すだけ**にする:

| 順 | 出所 | L1 |
|---:|---|---|
| 1 | `LEAN_DOC_BIN` 環境変数 | ✅ |
| 2 | バージョン固定のキャッシュ | (L2) |
| 3 | Release からダウンロード | (L2) |
| 4 | `PATH` の `lean-doc` | ✅ (版を印字して警告) |
| 5 | `cargo build` (cargo があれば) | ✅ |
| 6 | **明確なエラー** — 何を探して何が無かったかを列挙し、README の該当節を指す | ✅ |

**PATH をダウンロードより後ろに置く**のは版ずれのため — PATH の `lean-doc` は IR スキーマが
古いかもしれない。L1 の時点では 2/3 が空なので PATH が事実上の主経路になるが、**順序は
最初から最終形にしておく** (後から差し込むと「判断が 2 箇所」になる)。

### L1-d. ゲート

**`tools/lake-package-gate.sh`** — 機材と toolchain を要るので `tools/` (CLAUDE.md
「テストとゲートを分ける」)。フィクスチャは **`e2e/consumer/`** を新設する:
lean-doc を path で `require` する最小パッケージ。`e2e/micro` には触らない
(あちらの母数と不変量を動かさないため)。

検査するもの:

| | 何を見るか | 落ちたら何が壊れたか |
|---|---|---|
| 1 | `lake script list` に `lean-doc/docs` が出る | 配線が無い |
| 2 | `lake run docs -- --out <tmp>` がサイトを書く | script が組み立てる引数が違う |
| 3 | そのサイトが `tools/site-gate.sh` で緑 | 出力が壊れている |
| 4 | **Lake が建てた extract と `build.sh` の extract が同じ IR を書く** (`diff -r`) | **2 つのビルド経路が食い違った** |
| 5 | `--lib` を渡していないのに正しい lib が選ばれた | Lake からの取得が壊れた |

**4 が本体。** これが前提 4 を毎回検証し続ける唯一の場所になる。

**このゲートは必ず一度落としてから通す** (CLAUDE.md)。

> **結果**【実測 2026-08-18】: 5 項目すべてを個別に落として確認済み。
> **計画の予測が 1 つ外れた** — 「Lake 側の extract を `build.sh` 側と差し替えると 4 は
> *落ちない*」と書いていたが、**落ちる**。ゲートが**先に 2 つのバイナリの digest を比較し、
> 一致を失敗として扱う**ため (probe §2 で「構造的に +308,032 B 違う」と実測済みなので、
> 一致は「同じものを 2 回比べている」を意味する)。**これは計画より強い**: 差し替えは
> 「4 が何も見ていない状態」なので、通ってはいけなかった。
> 4 が本当に IR を見ていることは**別の壊し方**で確認した — `build.sh` 側を `--no-attrs` を
> 足す wrapper に差し替えると `index.json` から差分が出て落ちる。
>
> **呼び出し側が独立に 1 回落とした**: `LEAN_DOC=/bin/echo` で 5 項目中 4 つが FAIL、
> exit 1。item 2 のメッセージが `lake run docs exited 0 but wrote no site` で、
> **終了コードを答えと読んでいない**ことも確認できた。

**被検査範囲の穴 (記録)**【実測 2026-08-18】: このゲートは `LEAN_DOC_BIN` を自分で固定するので、
**`resolveLeanDoc` の段 4 (PATH) と段 5 (cargo build) は一度も走らない**。L2 が足す段 2/3 は
**この固定をしないゲートを別に持つ**こと (§5 L2-f)。「走らせた」と「見ている」は別 (CLAUDE.md)。

### L1-e. 実配布経路の検査 (CI)

path require は**実配布経路ではない**。利用者が書くのは git require:

```lean
require «lean-doc» from git "https://github.com/FujiHaruka/lean-doc" @ "v0.1.4"
```

D3 が `uses: FujiHaruka/lean-doc@main` を実走させて初めて「配布経路が動く」と言えたのと同じ。
**`.github/workflows/ci-lake.yml`** を足し、push された sha を git require して同じゲートを回す。

### L1-f. docs

- README §Running it locally に `require` の節を足す (既存のコピペ手順は**消さない** —
  Lake を使わない利用者の経路として残す)。
- `tools/ci-build.sh:64` の「lean-doc has no toolchain, no lakefile」が**嘘になる**ので直す
  (toolchain は依然として無い。lakefile だけが増える)。
- `docs/approach.md` の「lean-doc 側に toolchain も lakefile も置かない」に相当する記述と
  CLAUDE.md を、**「toolchain は置かない / lakefile は置く」**に更新。

---

## 5. L2 — Release からバイナリを取る

L1-c の表の 2 と 3 を埋める。

### L2-a. 版の決め方 — action が払った授業料の一般形

**要求された rev のツリーの `Cargo.toml` が SoT。** `__dir__ ++ "/Cargo.toml"` の
`version` を読み、`releases/download/v<version>/` から取る。

これは `action.yml` が実測で学んだこと —— **ref が tag かどうかだけで判定すると `@main` が
古い Release のバイナリを掴む** —— の同型。`@main` で require した利用者は、
main のツリーの版に対応する Release が**在るときだけ**取れて、無ければ次の段に落ちる。

### L2-b. プラットフォーム判定と、アセットが無い場合

Release にあるのは **2 ターゲットだけ**【実測】:
`lean-doc-x86_64-unknown-linux-musl.tar.gz` / `lean-doc-aarch64-apple-darwin.tar.gz`。
アセット名に版は入らない。

**Intel macOS と arm Linux は意図的に存在しない** (D2 —— runner が取れず、
「誰も実行していないバイナリを配る」ことになるため)。したがって
**「自分のプラットフォームのアセットが無い」は正常な経路**であり、黙って落ちてはいけない。
何が無かったかを印字して次の段 (PATH → cargo) に進む。

arch の取り方は `System.Platform` に無い可能性が高い → `uname -m` を `IO.Process.output` で
読む (→ 未決 D3)。

### L2-c. checksum は必須

同じ Release の `checksums.txt` を取り、SHA-256 を照合する。**照合できないものは使わない。**
これは選択ではない: ビルドツールがネットワークから実行ファイルを取ってきて黙って走らせる、
という形にしないための最低条件。

### L2-d. キャッシュの置き場所

**`.lake` の中に置かない** — `lake update` で消える (CLAUDE.md で踏んだ事情)。
`${XDG_CACHE_HOME:-~/.cache}/lean-doc/v<version>/<target>/lean-doc`。
版とターゲットでディレクトリを分けるので、複数プロジェクト・複数版が共存できる。

### L2-e. 黙ってネットワークを叩かない

- ダウンロードする前に **URL とサイズを 1 行印字**する。
- **`LEAN_DOC_NO_DOWNLOAD=1`** で段 3 を飛ばす (オフライン / 方針で禁止している環境)。
- `LEAN_DOC_BIN` は最優先のまま (段 1)。

### L2-f. ゲート

**ダウンロード経路を実際に走らせる。** これが無いと L2 は「書いたが一度も動いていないコード」に
なる —— このプロジェクトが繰り返し捕まえてきた失敗の形。

| | 何を見るか |
|---|---|
| 1 | キャッシュを空にした状態で `lake run docs` が **Release からバイナリを取り、サイトを書く** |
| 2 | 2 回目は**ダウンロードせず**キャッシュを使う (印字の有無ではなく、ネットワークを切って走ること) |
| 3 | **checksum を意図的に壊すと失敗する** (壊したものを使わない) |
| 4 | `LEAN_DOC_NO_DOWNLOAD=1` で段 3 を飛ばし、次の段に落ちる |
| 5 | アセットの無いターゲットを騙って、**黙らずに次の段へ落ちる** |

3 と 5 は**壊して落とす**検査なので、ゲート自身の自己検査を兼ねる。

---

## 6. 未決 (実装中に測って決める)

| | 問い | 決め方 |
|---|---|---|
| **D1** | script から「建てるだけ」をどう書くか (`Lake.exe` は建てて起動する) | `Workspace.runBuild` を試す。駄目なら `lake build` を subprocess |
| **D2** | `--out` の既定値。`main.rs:134` は「`--out` は `--root` の中に置けない」と書いている | まず**本当に拒否されるか**を確かめる。拒否されるなら既定を置かず必須にする |
| **D3** | Lean から arch を取れるか | `System.Platform` を見て、無ければ `uname -m` |
| **D4** | `lean_lib` が 0 個 / 複数のとき何を渡すか | `target2` は 2 つ持つ (`target2-gate.sh`)。全部渡す想定で確認 |

---

## 7. やらないこと

- **lean-doc 自身を `lake build` できるようにはしない。** toolchain を持たない以上、
  自分のディレクトリでは lake が動かない【実測】。**必ず root ワークスペース側から叩く。**
- **`lakefile.toml` は置かない** (前提 3)。
- **利用者の `lakefile` を `.lean` に変えさせない** — `crates/lean-doc/src/lakefile.rs` が
  読むのは `.toml` で、そこは変えない。`--lib` を Lake から取るのは script の仕事。
- **Windows** — README で未対応を宣言済み。lakefile は Windows で `require` されたら
  素直に落ちる。
- **extractor のプリビルド配布** — D4 の判定は動かない。この経路では問い自体が消える。

---

## 8. 撤退ライン

- **L1-d の 4 (IR 一致) が落ちる** → Lake 経路は採らない。`build.sh` を残し、
  この文書に「なぜ駄目だったか」を書いて終わり。前提 4 が偽だったということなので、
  probe レポートの §2 も同じコミットで直す。
- **git require が CI で緑にできない** → `lakefile.lean` を出さない。半分動く配布経路を
  README に書くのは、いまのコピペより悪い (D3 と同じ撤退ライン)。
- **L2 のダウンロードが checksum 照合まで通らない** → L2 を捨てて L1-c の段 4/5 で運用する。
  L1 は残る。
