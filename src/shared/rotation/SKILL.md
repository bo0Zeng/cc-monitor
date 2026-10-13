---
name: rotation
description: cc-monitor 的账号轮换（第五版）怎么用、怎么写策略。后端给积木（每个号的事实 · 热换 · 停发 · 什么时候叫策略来判 · 推演 · 策略出错时照号列表）；「该用哪个号、什么时候换」全在策略里：一份普通的 Rhai 文件，头里是名字 · 一行灰字 · 自己的设置和设置的声明，代码里一个 `decide(ctx)`。讲命令、策略文件的接口与积木库 `lib`、怎么试跑和推演、怎么装。不存策略、不常驻跑东西。用在——「写一份轮换策略」「按某某办法换号」「改轮换的设置」「这个会话为什么换到这个号」「试跑 / 推演一下」「装一份策略」。
---

# 账号轮换：积木 · 策略

一句话：**后端没有任何换号的想法。** 号牌分排、上限、软硬、启用延迟，都是策略文件里的设置，由策略自己的代码解释（多半调积木库 `lib` 的 `by_rows`）。没有随包策略：策略文件都是用户自己的（自己写的 · 让 AI 照这份 skill 写的 · 升级时从旧配置换出来的）；没选策略的会话不轮换，只用开会话的号。

本 skill 不存策略、不替用户存草稿、不常驻跑任何东西；动 `rotation.json` 与策略目录只经下面的命令。命令都经 `~/.cc-monitor/bin/ccm -- --<命令>`，参数从 stdin 读一段 JSON，stdout 一行 JSON；形状全表见 `src/doc/IPC-COMMANDS.md`。

## 1. 后端给什么

- **事实**（`--rotation-facts`）：每个号两窗（5 小时 · 7 天）的用量 · 重置时刻 · 用速（额度账里量出来的，量不到就空）· 被拒到几点 · 登录 · 超额。
- **一判**：策略交回「此刻照什么先后用号」，后端取第一个事实上发得出去的号；跟此刻用的不同就热换；一个都没有就停发（回那一家的「用满」回包）到策略说的时刻。
- **什么时候判**：只在节点上判，结论缓存到下一个节点：被拒 · 重置 · 外来的事实变了 · 设置改了 · 策略约的时刻（`wake.at`）· 策略约的读数（`wake.when`）。节点之间一发都不叫策略。
- **用户拖出来的顺序**：到策略说该换的那一刻先用它（策略此刻不列的跳过）。
- **策略出错时**：策略编不过 · 跑出错 · 超时 · 回的形状不对 ⇒ 此刻的号发得出去就照用，否则照设置里号列表的先后取第一个发得出去的；界面上提示「策略出错」带行号。

## 2. 命令

- 读：`--rotation-strategies`（策略目录：每份的名字 · 灰字 · 版本 `rev` · 读不懂的原因 · 哪几个活会话在用 · 新会话用哪份）· `--rotation-view {target: {sid} | {strategy, settings?}, hours?}`（一个会话或一份策略此刻的设置 · 声明 · 每个号的事实 · 这一判 · 推演 · 提示）。
- 写：`--rotation-settings {target: {sid} | {strategy}, settings, ifRev?}`（整份设置，照声明校验；会话 ⇒ 存成草稿）· `--rotation-save {target, settings?, ifRev, as?: {name}}`（只重写文件头，代码一个字不动；`as` ⇒ 另存一份、会话改用它）· `--rotation-revert {sid}` · `--rotation-use {sid, strategy | null, discard?}`（`null` ⇒ 这个会话不再轮换）· `--rotation-for-new {strategy | null}`（`null` ⇒ 新会话不轮换，这是默认） · `--rotation-temp {sid, op: {place: {account, at}} | "clear"}` · `--rotation-use-now {sid, account}`。
- 策略文件：`--rotation-strategy-add {source, name?}` · `--rotation-strategy-copy {id, name}` · `--rotation-strategy-rename {id, name, ifRev}` · `--rotation-strategy-delete {id}`（有活会话在用 · 是新会话用的那份 ⇒ 不删）。
- 试跑：`--rotation-try {source, settings?, start?, current?, temp?, hours?, useNow?}`：拿一份原文照此刻的事实判一次、往后推演，不写盘不换号。
- 算术：`--rotation-day-paint {segs, from, to, v}`：24 小时条上刷一段（`v: null` 删那一截）。

## 3. 策略文件

`~/.cc-monitor/strategies/<id>.rhai`，文件名去 `.rhai` 就是 id。开头一段块注释是 JSON 头，后面是代码：

```rhai
/*--- rotation
{ "format": 5, "name": "夜里省着用", "line": "夜里只用到一半", "tz": "Asia/Shanghai",
  "settings": { "caps": {"5h": [{"from": 0, "to": 420, "v": 50}]}, "capKind": "soft",
                "accounts": [{"id": "lab"}, {"start": true, "row": 1}, {"id": "team", "row": 1}] },
  "form": [ {"key": "row", "kind": "row", "scope": "each", "label": {"zh": "排"}, "help": {"zh": "同一排里用满一个才换下一个；上面一排的号一有额度就换上去"}},
            {"key": "caps", "kind": "daySegments", "scope": "each", "inherit": true, "label": {"zh": "用量上限"}, "windows": ["5h", "7d"], "default": 100} ] }
---*/
fn decide(ctx) { lib::by_rows(ctx, #{}) }
```

- `settings`：这份策略自己的设置，形状它自己定；号列表固定在 `settings.accounts`（`{start: true}` ＝ 开会话用的号）。
- `form`：设置怎么画、怎么校验。格的种类：`percent` · `minutes` · `toggle` · `choice` · `daySegments` · `row`（号牌分排：第几排，0 最上，沿号列表不减；有这一格界面才画分排）；`scope: all` 画在总体设置，`each` 画在每个号下面（`inherit: true` ＝ 号没另设就跟总体）。`label` · `help` 用平常话写，不用第二人称，不写讲结构的字。
- `ctx`：`now` · `clock`（策略时区的本地分钟）· `start` · `current` · `settings` · `memo`（上一判交的，原样带回）· `accounts[{id, pos, kind, sendable, used, resets, refusedUntil, rate}]`。没有文件 · 网络 · 钟 · 随机数。
- `decide(ctx)` 交回 `#{order: [号…], why: #{号: "一句"}, hold_until?, wake: #{at?, when: [#{acct, w, used_at_least}]}, memo?}`。

**积木库 `lib`**：`cap_at` · `next_edge` · `left_of` · `has_quota` · `rank` · `by_rows(ctx, scores)`。`by_rows` 就是号牌分排那一条：
- 同一排里不抢：在用的号用满了（或事实上发不出去）才换，换到这一排里排第一的、又有额度的号；
- 上面一排的号一有额度，就换上去；下面一排只在上面所有排都没额度时才用；
- 「有额度」＝ 事实上发得出去，且没到它此刻的用量上限（`caps` 分时段，号没另设跟总体）；
- 启用延迟（`delay`）：上面的排全都没额度满这么多分钟，才换到它；
- 所有号都到上限：软上限（`capKind: soft`）的号接着用到用满，硬上限的不超过；一个都没有就停发到最早恢复。
`scores` 只管同一排里谁先：给每个号一个分，大的先，同分照号牌左右（不给 ⇒ 照号牌左右）。它认的设置：`caps` · `capKind`（总体与每号）· `accounts[{start | id, row?, caps?, capKind?, delay?}]`。自己写的策略不调它也行，但要记得：后端照 `order` 第一个用，在用的号排第一就是不换。

## 4. 怎么装、怎么试

1. 写好原文，`--rotation-try {source}` 看这一判和推演对不对；
2. `--rotation-strategy-add {source}` 装进策略目录（读不懂 ⇒ `bad_strategy` 带行号）；
3. 会话改用它：`--rotation-use {sid, strategy}`；新会话都用：`--rotation-for-new {strategy}`。
