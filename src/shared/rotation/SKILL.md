---
name: rotation
description: cc-monitor 的账号轮换（第五版）怎么用、怎么写策略。后端给积木（每个号的事实 · 热换 · 停发 · 什么时候叫策略来判 · 推演 · 出错兜底）；「该用哪个号、什么时候换」全在策略里：一份普通的 Rhai 文件，头里是名字 · 一行灰字 · 自己的设置和设置的声明，代码里一个 `decide(ctx)`。讲命令、策略文件的接口与积木库 `lib`、怎么试跑和推演、怎么装。不存策略、不常驻跑东西。用在——「写一份轮换策略」「按某某办法换号」「改轮换的设置」「这个会话为什么换到这个号」「试跑 / 推演一下」「装一份策略」。
---

# 账号轮换：积木 · 策略

一句话：**后端没有任何换号的想法。** 上限、软硬、启用延迟、回切、都到上限后停还是接着用，都是策略文件里的设置，由策略自己的代码解释（随包三份都调积木库 `lib`）。

本 skill 不存策略、不替用户存草稿、不常驻跑任何东西；动 `rotation.json` 与策略目录只经下面的命令。命令都经 `~/.cc-monitor/bin/ccm -- --<命令>`，参数从 stdin 读一段 JSON，stdout 一行 JSON；形状全表见 `src/doc/IPC-COMMANDS.md`。

## 1. 后端给什么

- **事实**（`--rotation-facts`）：每个号两窗（5 小时 · 7 天）的用量 · 重置时刻 · 用速（额度账里量出来的，量不到就空）· 被拒到几点 · 登录 · 超额。
- **一判**：策略交回「此刻照什么先后用号」，后端取第一个事实上发得出去的号；跟此刻用的不同就热换；一个都没有就停发（回那一家的「用满」回包）到策略说的时刻。
- **什么时候判**：只在节点上判，结论缓存到下一个节点：被拒 · 重置 · 外来的事实变了 · 设置改了 · 策略约的时刻（`wake.at`）· 策略约的读数（`wake.when`）。节点之间一发都不叫策略。
- **用户拖出来的顺序**：到策略说该换的那一刻先用它（策略此刻不列的跳过）。
- **出错兜底**：策略编不过 · 跑出错 · 超时 · 回的形状不对 ⇒ 此刻的号发得出去就照用，否则照设置里号列表的先后取第一个发得出去的；界面上提示「策略出错」带行号。

## 2. 命令

- 读：`--rotation-strategies`（策略目录：每份的名字 · 灰字 · 版本 `rev` · 读不懂的原因 · 哪几个活会话在用 · 新会话用哪份）· `--rotation-view {target: {sid} | {strategy, settings?}, hours?}`（一个会话或一份策略此刻的设置 · 声明 · 每个号的事实 · 这一判 · 推演 · 提示）。
- 写：`--rotation-settings {target: {sid} | {strategy}, settings, ifRev?}`（整份设置，照声明校验；会话 ⇒ 存成草稿）· `--rotation-save {target, settings?, ifRev, as?: {name}}`（只重写文件头，代码一个字不动；`as` ⇒ 另存一份、会话改用它）· `--rotation-revert {sid}` · `--rotation-use {sid, strategy, discard?}` · `--rotation-for-new {strategy}` · `--rotation-temp {sid, op: {place: {account, at}} | "clear"}` · `--rotation-use-now {sid, account}`。
- 策略文件：`--rotation-strategy-add {source, name?}` · `--rotation-strategy-copy {id, name}` · `--rotation-strategy-rename {id, name, ifRev}` · `--rotation-strategy-delete {id}`（有活会话在用 · 是新会话用的那份 ⇒ 不删）。
- 试跑：`--rotation-try {source, settings?, start?, current?, temp?, hours?, useNow?}`：拿一份原文照此刻的事实判一次、往后推演，不写盘不换号。
- 算术：`--rotation-day-paint {segs, from, to, v}`：24 小时条上刷一段（`v: null` 删那一截）。

## 3. 策略文件

`~/.cc-monitor/strategies/<id>.rhai`，文件名去 `.rhai` 就是 id。开头一段块注释是 JSON 头，后面是代码：

```rhai
/*--- rotation
{ "format": 5, "name": "夜里省着用", "line": "夜里只用到一半", "tz": "Asia/Shanghai",
  "settings": { "caps": {"5h": [{"from": 0, "to": 420, "v": 50}]}, "capKind": "soft", "accounts": [{"start": true}, {"id": "lab"}] },
  "form": [ {"key": "caps", "kind": "daySegments", "scope": "each", "inherit": true, "label": {"zh": "用量上限"}, "windows": ["5h", "7d"], "default": 100} ] }
---*/
fn decide(ctx) { lib::decide_with(ctx, #{}) }
```

- `settings`：这份策略自己的设置，形状它自己定；号列表固定在 `settings.accounts`（`{start: true}` ＝ 开会话用的号）。
- `form`：设置怎么画、怎么校验。格的种类：`percent` · `minutes` · `toggle` · `choice` · `daySegments`；`scope: all` 画在总体设置，`each` 画在每个号下面（`inherit: true` ＝ 号没另设就跟总体）。`label` · `help` 用平常话写，不用第二人称，不写讲结构的字。
- `ctx`：`now` · `clock`（策略时区的本地分钟）· `start` · `current` · `settings` · `memo`（上一判交的，原样带回）· `accounts[{id, pos, kind, sendable, used, resets, refusedUntil, rate}]`。没有文件 · 网络 · 钟 · 随机数。
- `decide(ctx)` 交回 `#{order: [号…], why: #{号: "一句"}, hold_until?, wake: #{at?, when: [#{acct, w, used_at_least}]}, memo?}`。

**积木库 `lib`**（只算不决定）：`cap_at` · `next_edge` · `left` · `rank` · `decide_with(ctx, scores)`（上限 · 软硬 · 启用延迟 · 回切 · 都到上限后停发，照随包那一套设置；`scores` 给每个号一个分，大的先，同分照号列表）。随包三份：照列表先后 · 先用快重置的 · 先用剩得多的（多一格「多出几个点才换」）。

## 4. 怎么装、怎么试

1. 写好原文，`--rotation-try {source}` 看这一判和推演对不对；
2. `--rotation-strategy-add {source}` 装进策略目录（读不懂 ⇒ `bad_strategy` 带行号）；
3. 会话改用它：`--rotation-use {sid, strategy}`；新会话都用：`--rotation-for-new {strategy}`。
