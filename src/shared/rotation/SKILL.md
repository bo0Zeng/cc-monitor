---
name: rotation
description: cc-monitor 的账号轮换怎么拼、怎么写策略。轮换 ＝ 引擎（事实 ＋ 机械动作）＋ 加载的策略（全部偏好，Rhai 脚本，声明参数、算优先级）＋ 有名字的配置（一份策略的参数值）；会话打开配置的副本。讲后端有哪些积木和命令、策略文件的接口与共用小函数 `lib`、怎么试跑和推演、怎么装进加载路径、怎么写成配置给会话打开。不存策略、不跑策略程序。用在——「写一份轮换策略」「按某某办法换号」「改轮换配置」「这个会话为什么换到这个号」「试跑 / 推演一下」「装一份策略」。
---

# 账号轮换：积木 · 策略 · 配置

一句话：**界面上能调的每一样，都是此刻这份策略的参数。** 引擎只管事实和机械动作；偏好（到多少换、时段、备用、延迟、都到上限后停不停、换不换回）一样都不归引擎。

本 skill 不存策略、不替用户存策略副本、不常驻跑任何东西；动 `rotation.json` 只经下面的命令。

## 1. 它是什么

| 层 | 有什么 |
|---|---|
| 引擎（后端） | 事实：每号每窗口用量 · 重置 · 被拒 · 用满 · 超额在兜 · 要重新登录 · 在用号的用速 · 别的会话钉在哪。机械动作：拼上下文 · 沙箱里跑策略（出错照内置策略 1 排）· 叠手动调整 · 从头往下挑 · 热换 · 没号可挑就停发 · 推演 · 存策略的备忘 |
| 策略（`.rhai` 文件） | `meta()` 声明参数；`priority(ctx)` 回每号的优先级与为什么。不碰文件网络、没有钟；不回「换到谁」 |
| 配置（有名字） | 用哪份策略 · 参与的号（每行写全这份策略的每号参数）· 整份参数 · 新加的号缺省 |
| 会话 | 打开的副本 `config: {id, rev, copy?}` · 在用的号 · 手动调整 · 备忘 · 换号记录 |

一判：拼 `ctx` → 调 `priority(ctx)` → 取 `@memo` 存回会话 → 排（`p` 大的先 · 同分按号 id · `()` 最后、按 `back` 早的先）→ 叠手动调整（热换的号最前，拖过的号放回原位）→ 从头往下取第一个「事实上能用 · 接得上 · `p` 不是 `()`」的号 → 不是在用的号就换；一个都没有就停发。

## 2. 读此刻的事实

都经 `~/.cc-monitor/bin/ccm -- --<命令>`，参数从 stdin 读一段 JSON；给人看加 `--text`，时刻按 `--tz`（看的人就在这台 ⇒ `--tz local`）。

- `--quota-read`：每号每窗口用量与重置时刻 · 登录 · 被拒。
- `--rotation-session-read {sids}`：每个会话的 `rows`（有效优先级的先后，第一行在用；每行 `why` 是写好的那一句，`skip` ＝ 此刻不挑）· 打开的配置 · 描述 · `blocked`（停发：最早谁几点回来）· `strategyError`（策略出错，此刻照内置策略 1 排）· `needs`（轮换文件读不懂：需手动，引擎照上一份读懂的接着用）。
- `--rotation-plan {sid | config | draft, view: 12h | 24h}`：推演，往后怎么换（此刻之后的段都是估的 `est`）。

## 3. 配置

形状：

```json
{"name": "默认配置 2", "strategy": "2",
 "accounts": [
   {"start": true, "params": {"line": 90, "slots": [], "mode": "normal"}},
   {"rest": true,  "params": {"line": 90, "slots": [], "mode": "normal"}},
   {"id": "lab",   "params": {"line": 99, "slots": [{"at": "17:00-02:00", "line": 80}], "mode": "normal"}}
 ],
 "params": {"grab_min": 30, "grab_left": 30, "skip": 2, "tired_7d": 10, "delay": 0, "all_out": "stop"},
 "acctInit": {"line": 90, "slots": [], "mode": "normal"}}
```

- `{"start": true}` ＝ 起会话的号；`{"rest": true}` ＝ 账号库里其余的订阅号（没在别的行点名的），都照这一行；点名的号单写一行。
- 每行写全这份策略声明的每号参数；`acctInit` 只在加新号时抄过去。描述是后端写的（`text`），别自己拼。
- 命令：`--rotation-configs-read` · `--rotation-config-save {id?, ifRev?, config, force?}`（参数照声明逐格校验，错的格回 `refused{errors:[{cell, code, said}]}`；`ifRev` 对不上 ⇒ `conflict`）· `--rotation-config-rename` · `--rotation-config-delete`（有会话开着 · 是本机默认 ⇒ 不删）· `--rotation-default-set {id}`。

## 4. 改一个会话

- `--rotation-session-edit {sid, set: [{acct?, k, v}], add?, drop?, strategy?}`：改这个会话打开的那份的副本。`acct` 缺 ＝ 整份参数；`@start` · `@rest` · `@init`（新加的号缺省）· 号 id（`rest` 底下的号 ⇒ 给它单写一行）。改回原样 ⇒ 副本自己没了。
- `--rotation-session-save {sid, ifRev, as?: {name?}, force?}`：写回（撞版本 ⇒ `conflict`，两边各改了哪几格）或另存为（新 id，本会话改开它）。
- `--rotation-session-revert {sid}` · `--rotation-session-open {sid, config, discard?}`（有没存的改动 ⇒ 先回 `dirty`）。
- 改了一个会话，别的会话不受影响；要它们也照这样，就保存。

## 5. 写策略

**语言**：Rhai（手册 https://rhai.rs/book/）。能用：变量 · `if` · `for` · 函数 · 数组 · 表 · 闭包 · `?.` · `??`。

**四个函数**：

| 函数 | 回 |
|---|---|
| `meta()` | `#{format: 1, name?, ordered?, params: [声明…], acct_params: [声明…]}` |
| `texts()` | `#{zh: #{键: 句}, en: #{…}}`：`label` · `hint` · `describe` · `why` 用到的每个键两种语言都要有；`choice` 的每个选项要有 `<label>_<选项>`（`<label>_<选项>_h` 可选，是灰字） |
| `describe(params, accounts)` | 片段数组：`"键"` 或 `#{k: "键", 参数…}`；常接 `lib::describe_common(params, accounts)` |
| `priority(ctx)` | `#{号: #{p: 数 \| (), why: "键" \| #{k, …}, back?: 时刻}, "@memo"?: 表}` |

**参数声明** `#{k, type, init, label, of?, min?, max?, hint?}`，`type` 七种，界面照它生成格：

| `type` | 值 | 格 |
|---|---|---|
| `bool` | `true` · `false` | 勾选框 |
| `minutes` · `points` · `percent` | 数 · `()` | 数字格 ＋ 单位；`init` 是 `()` 的可空着 |
| `choice` | `of` 里的一个 | 单选（多了下拉） |
| `line` | 数 · `#{"5h": n, "7d": n}` · `()` | 用到 n % · 用满才换 · 5h、7d 分开 |
| `slots` | `[#{at: "HH:MM-HH:MM", line: n}]` | 时段表，`0` ＝ 这段不用 |

`ordered: true` ⇒ 配置里号的先后是这份策略的参数（`ctx.accounts` 按写下的先后；不 `ordered` 的按号 id）。

**约定的参数名**（`lib` 与合图认得，同名必须同类型）：每号 `line`（`line`）· `slots`（`slots`）；整份 `delay`（`minutes`）· `all_out`（`choice` `["stop", "go"]`）。用别的名字也能跑，只是共用小函数和图上的到线段认不得。

**`ctx`**（只有事实 ＋ 参数值）：`now`（unix 秒）· `clock`（配置时区的本地钟，一天里第几分钟）· `current`（在用的号）· `stint: {since, used}`（换进在用号以来卡人那一窗用了几点）· `rate`（在用号最近 30 分的涨速，点/分；没根据 ⇒ `()`）· `memo`（上一判回的 `@memo`）· `params` · `accounts: [{id, kind: "sub" | "api", ok, why_not, back, w5h: {open, used, resets}, w7d, others, params}]`。
`ok` 只是事实（没被拒 · 没用满 · 不在超额上 · 登录着），**线要自己判** ⇒ 用 `lib::usable`。

**共用小函数 `lib`**（随引擎走，只读，`--rotation-strategies-read` 的 `lib.source` 是源码）：

| 函数 | 回 |
|---|---|
| `usable(a, ctx)` | `#{ok, why, back, left}`：事实 ＋ 这个号的 `line` · `slots`；`why` 是 `@refused` · `@full` · `@overage` · `@login` · `@line` · `@off` |
| `checks(ctx)` | `#{号: usable(…)}` |
| `line_now(a, ctx)` · `left(a, ctx)` | 此刻对它生效的线 · 离线还剩几点 |
| `in_slot(at, clock)` · `slot_now(a, ctx)` | 时段判断，跨零点照算 |
| `delay(ctx, out, mins)` | 换号前延迟：有号排到在用的上面，连着 `mins` 分才让它上去；备忘写进 `out["@memo"].delay` |
| `keep_going(ctx, out)` · `all_out(ctx, out)` | 都到上限后接着用：一个肯挑的号都没有时，把事实上还能用的号排上，在用的先 |
| `describe_common(p, accounts)` | 描述里的号数 · 线 · 时段 · 延迟 · 都到上限后 |

**最常见的骨架**：

```rhai
fn priority(ctx) {
  let p = ctx.params;
  let ck = lib::checks(ctx);
  let out = #{};
  for a in ctx.accounts {
    let u = ck[a.id];
    if !u.ok { out[a.id] = #{ p: (), why: u.why, back: u.back }; continue; }
    out[a.id] = #{ p: 1000 - a.w5h.used.to_int(), why: #{ k: "w_rest", n: a.w5h.used } };
  }
  if ctx.current in out && out[ctx.current].p != () {
    out[ctx.current] = #{ p: 9000, why: "w_keep" };   // 还能用就不换
  }
  out = lib::delay(ctx, out, p.delay);
  lib::all_out(ctx, out)
}
```

- `p` 怎么给：数大的先，`()` ＝ 此刻不挑。分档（9000 / 5000 / 3000 …）避免交叠。「在用的还能用就不换」＝ 给它高档；「先等」＝ 给 `()` 带 `back`。
- 要记到下一判的：写 `out["@memo"]`，下一判从 `ctx.memo` 读（引擎替策略存，策略本身不写任何东西）。
- 字：`why` 写键 ＋ 参数；参数名 `at` · `*_at` 是时刻（引擎按看的那一台的时区写），`acct` 是号，`then` 是接着的一句，别的数字取整写出。
- 坑：`default` · `use` 是保留字（声明里缺省值的键叫 `init`）；闭包里别同时引用 `ctx` 又遍历 `ctx.accounts`（报 Data race；先取到局部变量，或把 `ctx` 当参数传给函数）；整数没有 `.round()`；不确定的格先判 `== ()`。
- 上限：一判 20 万次操作 · 调用深度 16 · 字符串 4 KiB · 数组 1024 · 表 256 · 墙钟 50 ms；一份文件至多 64 KiB。超了 / 出错 / 回的形状不对 ⇒ 这一判照内置策略 1 排，面板照实说哪一行。
- 范例：内置三份（`--rotation-strategies-read` 的 `source`）—— 1：照写下的先后 ＋ 用法（普通 / 优先 / 备用）＋ 延迟 ＋ 用备用号前先等；2：按重置早晚先用；3：快到期的先用、在用的快用完先用完。

## 6. 试跑

- `--rotation-strategy-try {source | id, config | draft | sid}`：按此刻的事实跑一次，回每号的 `p` · 为什么 · `back` · 会挑谁 · 备忘 · 用了几次操作；不写盘不换号。`source` 是现编的全文（不用先装）。
- `--rotation-plan {draft, source, view}`：照草稿和现编的策略往后推演。
- 验一个例子：照用户说的那几步各造一份草稿（或换一个会话）跑，看第一行与每行的为什么是不是用户要的。

## 7. 装进加载路径

写 `~/.cc-monitor/strategies/<id>.rhai`，或 `--rotation-strategy-add {id, source}`（读得懂才写；与内置同 id 不收）。`--rotation-strategies-read` 看读没读懂：读不懂的带 `error: {line, col, msg, text}`。删：`--rotation-strategy-delete {id}`（有配置在用 · 内置的删不了）。

## 8. 写成一份配置给会话打开

`--rotation-config-save {config: {name, strategy: "<id>", accounts, params, acctInit}}` → `--rotation-session-open {sid, config: <回的 id>}`；所有新会话都用 ⇒ `--rotation-default-set {id}`（已开着的会话不跟着换）。

## 9. 手动调整

`--rotation-session-set {sid, place: {account, at}}`（`at = 0` ＝ 现在就换过去，不出任何提示）· `{sid, refresh: true}`（清掉手动调整与备忘，照策略重算）· `--rotation-switch`。跟会话走、不进配置、不算改配置。

## 10. 不做的

- 不替用户存策略副本；不常驻跑任何东西；动 `rotation.json` 只经命令。
- 界面与手机不跑策略，只照后端回的显示。
