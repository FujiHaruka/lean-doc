# stage 8 — rev 非埋め込みの残る未検証点をブラウザで閉じる

`approach.md` §7 の段階 8。**判断基準は計測前に commit する** (段階 6b と同じ規律)。

## 何が残っているか — 1 点だけ

段階 7e-rev が §8 の未検証 2 点のうち 1 つを閉じ、もう 1 つを 2 つに割った
(→ `benchmarks/results/stage7e-rev-summary.txt` B-3):

| | 状態 |
|---|---|
| 点 A: オラクルは注入後なら成立するか | **閉じた** (99.5062%、全木 byte 一致) |
| B-3 (a) **配信時に文字列置換で注入する版** の順序制約 | **閉じた — 制約は無い**【実測】 |
| B-3 (b) **JS で実行時に注入する版** の順序制約 | **未検証。ここだけが段階 8** |

**§7 の段階 8 の書き方には分母の混同がある** (この段階で訂正する):
「同期スクリプトが 4,992 か所の `href` を書き換えるコスト」の 4,992 は
**木全体 = 配信時置換版の分母**。ブラウザで 1 ページを開くときに書き換えるのは
そのページの分だけで、**p50 9 / p90 24 / max 73 / min 1、合計 4,992 / 432 ファイル**【実測】。
**JS 注入版のコストは「木全体 4,992」ではなく「1 ページ最大 73」で測る。**

## 供給元の制約 — rev をページのバイトに戻してはいけない

注入する rev は**ページ外**から来なければならない。ページに
`<script>const SOURCE_URL="…"</script>` を埋めると rev がバイトに戻り、
この決定 (§5.6) の目的そのもの — コミットごとに 432 ページが無効にならないこと — が消える。
よって供給元は**全ページ共通の外部 1 ファイル** (`source-url.js`) とし、
rev の更新はそのファイルだけの書き換えにする。

## 構成 (4 つ)

`jump-src.js` は `<head>` の `<script type="module">` (defer 相当) で、
DOMContentLoaded ハンドラを登録し、**`?jump=src` 付き & hash 非空のときだけ**
`#<declId>` 下の `.gh_link a` の `getAttribute("href")` を `location.replace()` に渡す。
既存ページの `<head>` には同期のインライン `<script>` が既に 2 本ある
(`SITE_ROOT` / `MODULE_NAME`) ので、そこに 1 本足す形が製品での実装形になる。

| # | 注入の置き方 | 予測 |
|---|---|---|
| **B0** | 注入なし (`pages-ph` そのまま) | 壊れる (baseline) |
| **M1** | `<script type="module" src="source-url.js">` を **`jump-src.js` より前**に置き、即時に書き換える (defer 実行時点で DOM は完成済み) | 通る |
| **M2** | `<script src="source-url.js">` (**同期**、`jump-src.js` より前) で DOMContentLoaded リスナを登録し、その中で書き換える (同一イベントのリスナは登録順に発火) | 通る |
| **M3** | 注入を DOMContentLoaded リスナ内で行い、**`jump-src.js` より後**に登録する | **壊れる** |

**M3 は negative control で、この段階の要**。M3 が通ってしまうなら
「順序制約がある」という B-3 (b) の言明自体が誤りで、その場合は言明のほうを訂正する。
なお「module script を `jump-src.js` より後に置いて即時書き換え」は M3 ではない
(defer の実行は文書順だが DOMContentLoaded 発火より前なので間に合う)。**壊れるのは
「リスナ内で注入し、かつ登録が後」の場合だけ** — この区別を実測で確定させる。

## 予測 (計測前に固定)

| # | 予測 |
|---|---|
| V1 | **B0** は同一オリジンの `…/%7B%7BSOURCE_URL%7D%7D/…` へ遷移し **404** になる (B-2 の DOM シム結論のブラウザ裏取り) |
| V2 | **M1** は具体 rev の `https://github.com/…/blob/<40 hex>/…#L44-L52` へ遷移する |
| V3 | **M2** も同じ URL へ遷移する |
| V4 | **M3** は B0 と同じ壊れ方をする (= 順序制約は実在する) |
| V5 | 書き換えの実行時間は **max ページ (73 か所) でも 1 ms のオーダーに収まる** |
| V6 | `?jump=src` 無しで開くと、どの構成でも `location.replace` は走らない (壊れ方は `?jump=src` のディープリンクに限られる) |
| V7 | `source-url.js` の追加往復は**同期版 (M2) では parse をブロックする**。全ページ共通なので 2 ページ目以降はキャッシュに載る |

**V5 の測り方**: 壁時計 1 点ではなく、書き換え本数の違うページ (min 1 / p50 9 / max 73) で測る。
**V1〜V4 の判定は `location.replace` に渡った URL** であって「見た目」ではない。

## 手段

Chrome (`/Applications/Google Chrome.app`) を headless + `--remote-debugging-port` で起動し、
CDP を deno の WebSocket から直接叩く (npm/node は壊れているので使わない)。
木はローカル HTTP で配信する — **`file://` で測らない** (module script の origin 挙動が違う)。
**GitHub への実リクエストは `Fetch` ドメインでブロックして URL だけ記録する** —
外部へ実際に出さない。404 の確認はローカルサーバの応答で行う。

## この段階が触らないもの

`experiments/stage1`〜`stage7h` は **1 行も変えない**。`stage4c/coverage.ts` も呼ばない
(この段階は byte を測らない — 点 A は 7e-rev で閉じている)。
Lean は 1 度も走らせない。`pages-ph` は**写して**使い、原本を書き換えない。

---

# 何をやったか (2026-08-11)

数字と計測条件の SoT は `benchmarks/results/stage8-summary.txt`。ここは要旨だけ。
**上の予測の節は残してある。外れた分もそのまま残している。**

## 実装

| ファイル | 役割 |
|---|---|
| `source-url.js` | rev の**唯一の供給元**。全ページ共通の外部 1 ファイル。`.gh_link a` / `.gh_nav_link a` の `getAttribute("href")` の `{{SOURCE_URL}}` を具体 rev に差し替える。スケジューリングはタグの `data-schedule` で決まる (`auto` = parse 完了済みなら即時 / それ以外は DOMContentLoaded、`listener` = 常に DOMContentLoaded) |
| `build-sites.ts` | `pages-ph` を `cp -R` で写し、`<head>` の jump-src.js タグの前後に `<script>` を 1 本足して 5 構成を作る |
| `serve.ts` | ローカル HTTP 配信 + 全リクエストの JSONL ログ。`--delay-source-url` / `--cache-control` で V7 の条件を振る |
| `cdp.ts` | CDP クライアント全体。deno の WebSocket 直叩き (npm/node は使えない) |
| `probe.ts` | ハーネス。`--suite {hook,nav,cost,v7,cache}` |
| `run.sh` | driver。これ 1 本で全部再現する |

構成は表のとおり B0/M1/M2/M3。**m3b を 1 つ足した** — README 本文が
「module を jump-src.js より後に置いて即時書き換えは M3 ではない」と名指ししている
ケースで、言明のままにせず測るため。M3 の壊れが「置き場所」ではなく
「リスナ登録順」に由来することを分離する。

## 結果

| # | 予測 | 結果 |
|---|---|---|
| V1 | B0 は `%7B%7B…` へ遷移して 404 | **成立**。サーバ側ログに 404 |
| V2 | M1 は具体 rev の GitHub URL へ | **成立**。`#L44-L52` まで一致 |
| V3 | M2 も同じ URL へ | **成立**。M1 と同一文字列 |
| V4 | M3 は B0 と同じ壊れ方 (順序制約は実在) | **成立**。m3b は通る |
| V5 | max ページでも 1 ms のオーダー | **成立。ただし予測の前提が 2 つ外れた** (下記) |
| V6 | `?jump=src` 無しなら `location.replace` は走らない | **成立**。25 回中 0 件 |
| V7 | 同期版は parse をブロック / 2 ページ目以降キャッシュ | **前半は成立。後半は無条件には成立しない** (下記) |

判定は `Page.frameRequestedNavigation(reason=scriptInitiated)` の URL。
`location.replace` を JS で包む案は**使えない**ことを実測した —
`replace` は Location の own property で configurable=false、
`Object.defineProperty` は `TypeError: Cannot redefine property: replace`。

## 予測と食い違った点

1. **V5「max ページ (73 か所)」という前提が誤り。** コストは書き換え本数に比例せず、
   **要素数に対して単調**で本数に対しては単調でない (9 本 0.109 ms > 10 本 0.094 ms、
   47 本 0.66 ms > 73 本 0.33 ms)。木で最も高いのは本数最大のページではなく
   **バイト最大のページ** (47 本 / 555,904 B / 13,562 要素) で **0.80 ms**。
   1 ms のオーダーには収まるが、**大きく下回るのではなく 1 ms の直下**。
   要素数と本数の寄与は 5 点では**分離できていない**。

2. **V7 後半「全ページ共通なので 2 ページ目以降はキャッシュに載る」は無条件には成立しない。**
   鮮度ヘッダが無ければ Chrome は 10 回のナビゲーションで 10 回取りに来る。
   `Cache-Control: max-age=3600` を付けると 1 回になる。
   → **配信側の要件**として残る (かつ rev 更新時に古い `source-url.js` を
   掴ませない設計が要る)。

3. **予測に無かった副作用**: module 版 (M1/M3) は parse をブロックしないが、
   **DOMContentLoaded は同期版と同じだけ遅れる**。
   「module にすれば往復のコストが消える」わけではない。

4. **構成間の差は「タグの置き方だけ」ではない。** 表の定義が M1 を「即時」、
   M2/M3 を「リスナ内」としている以上、`data-schedule` 属性も違う。
   表の定義に従い、README 本文の「置き方だけ」という要約のほうを訂正する。

## B-3 (b) の最終形

  **注入が DOM の `.gh_link a[href]` を書き換え終わる時点が、`jump-src.js` が
    登録した DOMContentLoaded ハンドラの実行より前でなければならない。**

満たし方は 3 通りあり、**すべてブラウザで実測した**:
`module/defer で即時書き換え` (M1、m3b — タグの位置は前でも後でもよい)、
`同期スクリプトで DOMContentLoaded を先に登録` (M2)。
外すのは「リスナ内で注入し、かつ登録が jump-src.js より後」のときだけ (M3)。
