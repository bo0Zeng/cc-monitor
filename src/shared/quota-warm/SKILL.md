---
name: quota-warm
description: 让这台机器上各 Claude 订阅号的 5 小时额度窗口尽早开始计时——窗口没在计时（已过期、或从没见过数）就用那个号发一句最短的话，之后按最早的重置时刻自己醒来再看。也写了 AI 自己管额度时能直接调的那几条 cc-monitor 后端命令（读额度账 · 读轮换 · 现在就换号）。触发场景——「让各号的 5h 窗口早点开始」「没在计时就发一句」「看看各号几点重置」「quota-warm」「帮我管额度 / 轮换顺序」。
---

# quota-warm：没在计时就发一句

5 小时窗口从这个号发出第一句话才开始计；没人发，窗口就不开、也没有重置时刻。早点开窗，窗口就早点到点重置，用的时候额度更可能是满的。

**要求**：这台装了 cc-monitor，它的后端与中转在跑（额度账由中转记、账号来自它的账号库）。cc-monitor 不装、不起、不认这个 skill；它只用 cc-monitor 已有的三样基本模块（见「依赖」）。

## 它怎么判（每个号一行）

读 `ccm -- --quota-read`（这台中转记下的额度账）里每个号的 `warm`：判定在 cc-monitor 核心（`act` 发 `send` / 等 `wait` · `at` 醒点 · `text` 为什么），这里只照它发、照它睡。核心的判法：

| 那个号此刻 | 做什么 |
|---|---|
| 5h 窗口在计时（重置时刻在将来） | 不动，醒在重置时刻 ＋ 60 秒 |
| 5h 窗口已过期 · 没有 5h 窗口 · 这台从没见过它的数 | 用这个号发一句，让窗口开始计时 |
| 被拒（5h 或 7d 用满） · 7d 用到 100% | 等到它重置 ＋ 60 秒 |
| 要登录（`需登录`）· 按量号缺 key（`需 key`） | 记一行，1 小时后再看 |
| 额度账读不懂（不是 JSON · 读不出） | 一个都不发，15 分钟后再看；那一句照实说哪一份读不懂、为什么 |
| 账号库里没有这个号 | 跳过 |

发那一句的回包带回额度账里那个号此刻的 5h 重置时刻：在将来 ⇒ 成，醒在它 ＋ 60 秒；没带回 / 报错 / 超时（后端那一发 120 秒）⇒ 记「没成」，15 分钟后再试。读不到额度账 ⇒ 15 分钟后再试。

🔴 **没见过数 ＝ 不知道，不等于窗口没在计时。** 额度账只记经过这台中转的回包；别处（别的机器、没走中转的 claude）用过这个号，这台看不到。想知道只能发一句——而**计时中的窗口不会因此被推后**（实测：已在计时时回包给的重置时刻还是原来那个），所以「没见过就发」不亏。

**读重置时刻从来不用发。** 中转经手的每个回包都记进额度账，`--quota-read` 随时读得到。要发的只有一件事：让一个**没在计时**的窗口开始计时。中转从没见过的号，此刻是什么样只有两条路知道——发一句，或者真用它一次。

## 发法

发那一句只住 cc-monitor 后端一处（轮换策略交回 `warm` 时后端也从这里发）：`ccm -- --quota-warm`，stdin `{"agent": "claude", "account": "<号>"}`。
回包 `{agent, account, sent, at, resets_at}`：`sent` 为假 ＝ 同一个进程里 15 分钟内发过，照那一次答；`resets_at` ＝ 额度账里那个号此刻的 5h 重置时刻（没读到 ⇒ `null`）。
后端照适配层登记的那一形起本二进制当 ccm 跑一发 `claude -p`，走这台的中转（回包头由中转记进额度账）：

- haiku · 不给工具 · 不起 MCP · 只读项目级设置 · 关掉思考（`MAX_THINKING_TOKENS=0`）· 不发非必要的请求 · 不落会话记录 · 系统提示词换成一句。
- 工作目录 `~/.cc-monitor/autostart/`：cc-monitor 后端在历史页与会话列表里藏掉工作目录是它的会话。
- `--no-rotation`：这一发不轮换 —— 「新会话用」设成会换号的那一份时，不带它这一发会被换到别的号、预热白做。
- 号由 `--account <名>` 点名；环境照后端进程的（`PATH` 换成起会话那个 shell 的那一份）。

**花多少**（实测一发，claude 2.1.289）：系统提示词换成一句 ⇒ 输入约 870 token（claude 自带、去不掉的那一段）· 输出 4 token。不换系统提示词约 6.8k token：缓存热时按一成价算，可隔几小时才发一次时缓存早已过期、要按写缓存价付 ⇒ 换掉更省。

## 用法

```text
scripts/quota-warm status      只读：每个号一行，该发的标「⇒ 该发」
scripts/quota-warm once        跑一趟（该发的发）后退出
scripts/quota-warm run         先跑一趟，之后按最早的醒点自己醒来再跑，一直跑
```

| 选项 | 缺省 | 说明 |
|---|---|---|
| `--accounts work,personal` | 额度账的候选（`--quota-read` 的 `candidates`：出过数的号 ＋ 账号库里没出过数的号） | 看哪几个号（逗号分隔，账号库里的名字） |
| `--dry-run` | 关 | 只判、不发（日志写「该发（--dry-run，没发）」） |

- `once` / `run` 持一把锁 `~/.cc-monitor/quota-warm.lock`（里面是 pid）：同一时刻只跑一份，第二份直接退出并说锁在哪。`status` 不拿锁。
- `run` 长睡分段（每段至多 10 分钟）醒来按真实时刻重算，机器休眠醒来不会睡过头；下一次至少隔 30 秒。没有要看的号 ⇒ 退出。
- 想让它一直在：自己挑方式起 `run`（tmux 窗口、用户级服务……）。cc-monitor 不代装。

## 怎么读

`status` 每行 `<号>  <说明>`，说明是核心写好的那一句（`warm.text`），这里照抄：`5h 40% ↻12:28`（在计时，几点重置）· `被拒（5h）↻…` · `7d 用满 ↻…` · `需登录` · `需 key` · `无采样 · 发一句才知道  ⇒ 该发` · `5h 窗口已过期  ⇒ 该发` · `无 5h 窗口  ⇒ 该发` · `读取 <路径> 失败 · 内容无法解析 · 不发`。时刻按这台的本地钟写，不是今天的带上 `MM-DD`。

日志 `~/.cc-monitor/logs/quota-warm.log`（同时打到 stdout），一行一件：

```text
2026-10-05 11:28:17 开始（run）：work, personal, team
2026-10-05 11:28:17 work: 5h 40% ↻12:28
2026-10-05 11:28:17 personal: 无采样 · 发一句才知道 ⇒ 发了 ⇒ ↻16:28
2026-10-05 11:28:17 team: 5h 窗口已过期 ⇒ 没成（failed：…）⇒ 11:43 再试
2026-10-05 11:28:17 下次 11:43
```

「没成」常见原因：这台中转没在跑（cc-monitor 后端没起）⇒ 回包没经中转、额度账没新数（`resets_at` 为空）；号要重新登录；ccm 起不了 claude（`failed` 那一句与原话）· 超时。

## 依赖（cc-monitor 的基本模块）

- 这台的额度账（`--quota-read`，见下面的命令段）：不给 `--accounts` 时号单也从这里来（`candidates`），脚本不另扫账号库。
- 后端用某个号发那一句（`--quota-warm`，见下面的命令段）：经 ccm `--account <名>` 起一次 `claude -p`，走这台的中转；`--no-rotation` 让这一发不轮换。
- 后端藏掉工作目录是 `~/.cc-monitor/autostart/` 的会话（历史页 · 会话列表 · 流上的会话宣告）。

## AI 自己管额度：cc-monitor 的后端命令

都经 `~/.cc-monitor/bin/ccm -- --<命令>`：要参数的从 stdin 读一段 JSON（＝ 参数对象），stdout 一行 JSON。轮换（策略文件 · 会话用哪份 · 拖出来的顺序 · 推演）另一份 skill `rotation` 讲全，这里只选最常用的两条。

<!-- 命令：quota-read quota-warm rotation-view rotation-use-now（生成物：照 `ccm -- --commands` 现打，勿手改；重打见 `tests/backend/stream/catalog_tests.rs`） -->
- `ccm -- --quota-read`：这台的额度账。不收参数
- `ccm -- --quota-warm`：给一个号开窗。参数 `QuotaWarmReq`
- `ccm -- --rotation-view`：一个会话或一份策略此刻的轮换。参数 `RotationViewReq`
- `ccm -- --rotation-use-now`：现在用这个号。参数 `RotationUseNowReq`

```ts
type AgentKind = string;
type QuotaWarmReq = { 
/**
 * 哪一家（线上的 kind）。
 */
agent: AgentKind, 
/**
 * 账号库里的号（配置目录末段；账号 0 是 `0`）。
 */
account: string, };
type RotTarget = { sid: string, } | { strategy: string, };
type RotationUseNowReq = { sid: string, account: string, };
type RotationViewReq = { target: RotTarget, 
/**
 * 设置页那一形往后推几小时（1–48，缺 ⇒ 12）；会话那一形不画推演，不看它。
 */
hours?: number, };
```
<!-- /命令 -->

额度账每号带写好的几行 `rows` 与开窗那一判 `warm`；给人看加 `--text`（所有命令通用：回包里写好的格拼成字）；回包里的「几点」按这台的钟写（看的人在别台 ⇒ `--tz <IANA 名>`）。
