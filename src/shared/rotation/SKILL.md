---
name: rotation
description: cc-monitor 的账号轮换（第五版）怎么用、怎么写策略。后端给积木（每个号的事实 · 热换 · 停发 · 什么时候叫策略来判 · 推演 · 策略出错时照号列表）；「该用哪个号、什么时候换」全在策略里：一份普通的 Rhai 文件，文件名是标题，开头的注释是说明，接着是头（自己的设置和设置的声明），代码里一个 `decide(ctx)`。讲命令、策略文件的接口与积木库 `lib`、怎么试跑和推演、怎么装。不存策略、不常驻跑东西。用在——「写一份轮换策略」「按某某办法换号」「改轮换的设置」「这个会话为什么换到这个号」「试跑 / 推演一下」「装一份策略」。
---

# 账号轮换：积木 · 策略

一句话：**后端没有任何换号的想法。** 权重、权重容差、强制切换阈值、用量上限、耗尽后启用、启用延迟、单次用量上限，都是策略文件里的设置，由策略自己的代码解释（积木库 `lib` 只给算上限 · 余量 · 排先后这类小函数）。没有随包策略：策略文件都是用户自己的（自己写的 · 让 AI 照这份 skill 写的 · 升级时从旧配置换出来的）；没选策略的会话不轮换，只用开会话的号。

本 skill 不存策略、不替用户存草稿、不常驻跑任何东西；动 `rotation.json` 与策略目录只经下面的命令。命令都经 `~/.cc-monitor/bin/ccm -- --<命令>`，参数从 stdin 读一段 JSON，stdout 一行 JSON；形状全表见 `src/doc/IPC-COMMANDS.md`。

## 1. 后端给什么

- **事实**（`--rotation-facts`）：每个号两窗（5 小时 · 7 天）的用量 · 重置时刻 · 用量涨得多快（额度账里量出来的，量不到就空）· 被拒到几点 · 登录 · 超额。
- **一判**：策略交回「此刻照什么先后用号」，后端取第一个事实上发得出去的号；跟此刻用的不同就热换；一个都没有就停发（回那一家的「用满」回包）到策略说的时刻。
- **什么时候判**：只在节点上判，结论缓存到下一个节点：被拒 · 重置 · 外来的事实变了 · 设置改了 · 策略约的时刻（`wake.at`）· 策略约的读数（`wake.when`）。节点之间一发都不叫策略。
- **用户拖出来的顺序**：到策略说该换的那一刻先用它（策略此刻不列的跳过）。
- **策略出错时**：策略编不过 · 跑出错 · 超时 · 回的形状不对 ⇒ 此刻的号发得出去就照用，否则照设置里号列表的先后取第一个发得出去的；界面上提示「策略出错」带行号。

## 2. 命令

- 读：`--rotation-strategies`（策略目录：每份的 `id`（＝ 文件名 ＝ 标题）· 说明 `description`（文件开头的注释）· 版本 `rev` · 读不懂的原因 · 哪几个活会话在用 · 新会话用哪份）· `--rotation-view {target: {sid} | {strategy, settings?}, hours?}`（一个会话或一份策略此刻的设置 · 声明 · 每个号的事实 · 这一判 · 推演 · 提示）。
- 写：`--rotation-settings {target: {sid} | {strategy}, settings, ifRev?}`（整份设置，照声明校验；会话 ⇒ 存成草稿）· `--rotation-save {target, settings?, ifRev, as?: {name}}`（只重写头那一块，说明和代码一个字不动；`as` ⇒ 另存成文件名 `name`、会话改用它）· `--rotation-revert {sid}` · `--rotation-use {sid, strategy | null, discard?}`（`null` ⇒ 这个会话不再轮换）· `--rotation-for-new {strategy | null}`（`null` ⇒ 新会话不轮换，这是默认） · `--rotation-temp {sid, op: {place: {account, at}} | "clear"}` · `--rotation-use-now {sid, account}`。
- 策略文件：`--rotation-strategy-add {source, name}`（存成 `name.rhai`）· `--rotation-strategy-copy {id, name}`（另起一个文件名，内容不改）· `--rotation-strategy-rename {id, name, ifRev}`（换文件名，指它的会话与「新会话用」跟着改）· `--rotation-strategy-delete {id}`（有活会话在用 · 是新会话用的那份 ⇒ 不删）。名字：1–32 字，字母数字（含中文）与 ` -_·()（）+`；同名已有 ⇒ `bad_args`。
- 试跑：`--rotation-try {source, settings?, start?, current?, temp?, hours?, useNow?}`：拿一份原文照此刻的事实判一次、往后推演，不写盘不换号。
- 算术：`--rotation-day-paint {segs, from, to, v}`：24 小时条上刷一段（`v: null` 删那一截）。

## 3. 策略文件

`~/.cc-monitor/strategies/<标题>.rhai`：**标题就是文件名**（去 `.rhai`，也是 id）。文件分三段：

```rhai
// 换号时按权重排序，权重相差在权重容差以内算同权，同权按账号列表顺序。
// 在用账号未达用量上限时，重置早于它、且权重超出它强制切换阈值的账号立即切入。

/*--- rotation
{ "format": 5, "tz": "Asia/Shanghai",
  "settings": { "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]}, "close": 25, "force": 25,
                "accounts": [{"start": true}, {"id": "lab"}, {"id": "team", "fallback": true, "delay": 30}] },
  "form": [ {"key": "caps", "kind": "daySegments", "scope": "each", "inherit": true, "label": {"zh": "用量上限"},
             "help": {"zh": "账号各窗口用量的上限，可按时段分段"}, "windows": ["5h", "7d"], "default": 100},
            {"key": "close", "kind": "percent", "scope": "all", "label": {"zh": "权重容差"}, "default": 0},
            {"key": "force", "kind": "percent", "scope": "all", "nullable": {"zh": "不强制切换"}, "label": {"zh": "强制切换阈值"}, "default": null},
            {"key": "fallback", "kind": "toggle", "scope": "each", "label": {"zh": "耗尽后启用"}, "default": false},
            {"key": "delay", "kind": "minutes", "scope": "each", "label": {"zh": "启用延迟"}, "default": 0} ] }
---*/

fn weight(a, left, mins) { if mins == () { 0.0 } else { 300.0 / mins } }
fn decide(ctx) { … }
```

- **说明**：文件最开头连着的 `//` 行（到第一行不是 `//` 的为止；行首的 `// ` 去掉，空的 `//` 行分段）。界面只显示它、不改它。只讲怎么排、怎么换，**不写设置的值**（值在设置里，界面一改就旧了）。
- **头**（`/*--- rotation … ---*/`）：`format` · `tz` · `settings` · `form`。头里**没有** `name` · `line`（写了 ⇒ 读不懂、形状不对）。后端存设置只重写这一块，说明和代码一个字节不动。
- `settings`：这份策略自己的设置，形状它自己定；号列表固定在 `settings.accounts`（`{start: true}` ＝ 开会话用的号），**号列表的先后就是默认优先级**（权重同权时照它）。
- `form`：设置怎么画、怎么校验。格的种类：`percent` · `minutes` · `toggle` · `choice` · `daySegments`；`scope: all` 画在总体设置，`each` 画在每个号下面（`inherit: true` ＝ 号没另设就跟总体）。`label` 用短的正式名字（用量上限 · 权重容差 · 强制切换阈值 · 耗尽后启用 · 启用延迟 · 单次用量上限），定义写在 `help` 里；不用第二人称，不写讲结构的字。
- `ctx`：`now` · `clock`（策略时区的本地分钟）· `start` · `current` · `settings` · `memo`（上一判交的，原样带回）· `accounts[{id, pos, kind, sendable, used, resets, refusedUntil, rate}]`。没有文件 · 网络 · 钟 · 随机数。
- `decide(ctx)` 交回 `#{order: [号…], why: #{号: "一句"}, hold_until?, wake: #{at?, when: [#{acct, w, used_at_least}]}, memo?}`。后端照 `order` 第一个事实上发得出去的号用：**在用的号排第一就是不换**。

**换谁、什么时候换**（常见的写法，全在策略自己的代码里）：
- 用满了换谁：每个号算一个权重，从高到低；较高的超出较低的不到权重容差就算同权，照号列表先后。
- 强制切换阈值：在用的号还没到用量上限时，只有重置比它早、权重又超出它这一比例的号才直接换过去（后重置的号剩的不会先作废，不必抢；这样也不会来回换）。不设 ⇒ 用满才换。
- 耗尽后启用：其余的号都没额度时才用它，其余的号一有额度就换走；启用延迟 ＝ 其余的号都没额度满这么久才用；单次用量上限 ＝ 每次换过来 5 小时用量涨到这么多就停用，直到其余的号恢复。起点和记账放在 `memo` 里。
- 权重会随时间、随用量变：开着强制切换阈值时，算出它几时超过在用的号，用 `wake.at`（时刻）· `wake.when`（读数）约下次来问；后端只在这些节点上判。
- 一个都没有：`order` 给空 ⇒ 停发到 `hold_until`。

**积木库 `lib`**（只算、不决定，跟哪种权重都无关）：
- `cap_at(segs, clock)` · `next_edge(segs, clock)` · `segs_of(s, r, w)`：分段上限、下一条边界、号另设还是跟总体；
- `cap_of(s, r, w, lift, clock)` · `left_of(s, r, a, lift, clock)` · `has_quota(s, r, a, lift, clock)`：此刻上限、余量（两窗取小）、有没有额度；`lift` ＝ 放开软上限（`capKind: soft` 的号当 100）；
- `rank(ids, pri, close)`：按权重排先后，`close`% 以内同权、照 `ids` 先后；
- `delay_start(prev, holds, now)` · `delay_over(start, minutes, now)`：启用延迟的起点（存进 `memo`）、到点没有；
- `hold_from(s, rows, accts, ids, now, clock, extra)`：一个都没有时停到几点。

## 4. 怎么装、怎么试

1. 写好原文，`--rotation-try {source}` 看这一判和推演对不对；
2. `--rotation-strategy-add {source, name}` 装进策略目录（文件名 ＝ 标题）（读不懂 ⇒ `bad_strategy` 带行号）；
3. 会话改用它：`--rotation-use {sid, strategy}`；新会话都用：`--rotation-for-new {strategy}`。
