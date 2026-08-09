---
name: handoff
description: 現在のセッションの状態を `.claude/handoff.md` に書き出し、次セッションで `/carryon` から再開できるようにする。ユーザーが「セッションリセット」「次のセッション用にまとめて」「ハンドオフ書いて」「/handoff」と言ったときに起動する。今やっている作業の続きを cold な未来の自分が拾えるよう、状態 + 次の一手 + 読むべきファイル を簡潔にまとめる。
---

# handoff: セッション間の引き継ぎを書く

`.claude/handoff.md` を **上書き** で書く。次セッションで `/carryon` した未来の自分が「冷えた状態でも作業を再開できる」ことが目的。

## やること

1. **状態スナップショットを集める** (並列で OK):
   - `git status` (uncommitted の有無、branch)
   - `git log --oneline -5` (直近のコミット)
   - 現在の Task list (TaskList tool で取得)
2. **docs の hygiene チェック** (CLAUDE.md「docs の衛生」):
   - `wc -l docs/*.md` — 600 行を超えた plan があれば handoff 前に `/compact-plan <path>`。
   - `docs/approach.md` と `docs/verification-log.md` の食い違いを解消する。**検証結果が SoT**、計画側の予測を結果に合わせて直す (逆をやらない)。
   - **計測数値の出所が切れていないか** — docs に書いた数字のうち、対応する `benchmarks/results/*.jsonl` が消えている / 再生成できないものがあれば、その場で「実測」ラベルを外すか根拠を復元する。cold な次セッションは数字を無条件に信じる。
3. **`.claude/handoff.md` を以下の形式で書く** (`Write` で上書き)
4. ユーザーには「ハンドオフ書いた」と一言だけ。長い要約は不要 (内容はファイルにある)

## handoff.md の形式

```markdown
# Handoff — <YYYY-MM-DD HH:MM>

## State

- Branch: <branch>
- Uncommitted: <"clean" or short list>
- Active phase / 作業中の文脈: <一文>
- 計測環境: <計装が当たったままか / olean 暖機の有無 / 前回計測からの変化。計測を触っていないなら省略>

## Relay control（relay 連鎖のときだけ）

relay スキルが連鎖を駆動しているときのみ入れる（relay が内容を渡す）。通常の handoff では省略。

- Mode: ON | DONE | PAUSED | ABORTED
- Goal: <完遂すべきゴール>
- Leg: N / cap K
- Predecessor: <session 名 or none>   # 後続が起動確認後に kill
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: <成果 / commit hash>

## Tasks

(TaskList の出力を pending/in_progress のみ抽出。完了済みは除く)

- #N [status] subject — description (もしあれば)

## Where we are

直近で何が終わって、何が未解決か。2〜4 文。**事実だけ**書く (推測・所感は次のセクション)。

## Next step

次セッションの最初の一手。具体的に書く:
- どのファイルを開くか
- どの検証段階のどの数字を取りに行くか / どの bash を打つか
- 既知の戦略があれば一文で

## Files to read first

未来の自分がまず開くべきファイルを優先順で 3〜5 個。各 1 行コメント:

- `path/to/file.md` — なぜ最初に読むべきか
- ...

## Load-bearing context

Next step を実行するとき効く学びを選んで書く。**「次の一手に効くか」で取捨** — 効かない懐古は書かない。ファイルからは拾えない事項に限る。例:

- 空振りした調査 (grep 空振り / 存在しなかった API) — 再調査の無駄を防ぐ。クエリも添える
- 決定したけどまだコードにも docs にも反映していない方針
- 試して却下したアプローチ + 理由 (再度トライ防止)
- 当たりをつけた仮説
- 計測で踏んだ落とし穴 (プロセス並列とメモリ、cold cache、ログの取り違え等)

なければこのセクションごと省略する。
```

## 書くときの原則

- **冗長にしない**。次セッションは `.claude/handoff.md` を Read するだけ — そこに全部入っているべきだが、長すぎると逆に読まれない。全体で 50 行以内が目安
- **過去のセッションのまとめは書かない**。「今日やったこと」ではなく「次に何をするか」。ただし next step に効く学びは Load-bearing context に選んで残す (懐古ではなく次の燃料として)
- **ファイルから読める情報は重複させない**。plan の中身を貼らない、計測結果は数字ではなくログのパスで参照する
- **絶対パスではなくプロジェクト相対パス**で書く (移植性のため)。例外は計測対象リポジトリのパスで、これは別リポジトリなので絶対パスで良い
- ユーザーへの返答は 1〜2 行。書き出した事実 + ファイルパスだけ

## トリガー

ユーザーが以下を発話したとき必ず起動:

- 「セッションリセット」「リセットします」
- 「次のセッション用に」「ハンドオフ」
- 「引き継ぎ書いて」「まとめて終わる」
- `/handoff`

`/carryon` の対になるスキル。
