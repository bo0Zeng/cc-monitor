---
name: quota-warm
description: 让这台机器上各 Claude 订阅号的 5 小时额度窗口尽早开始计时——窗口没在计时（已过期、或从没见过数）就用那个号发一句最短的话，之后按最早的重置时刻自己醒来再看。也写了 AI 自己管额度时能直接调的那几条 cc-monitor 后端命令（读额度账 · 读写轮换 · 现在就换号）。触发场景——「让各号的 5h 窗口早点开始」「没在计时就发一句」「看看各号几点重置」「quota-warm」「帮我管额度 / 轮换顺序」。
---

# quota-warm：没在计时就发一句

5 小时窗口从这个号发出第一句话才开始计；没人发，窗口就不开、也没有重置时刻。早点开窗，窗口就早点到点重置，用的时候额度更可能是满的。

**要求**：这台装了 cc-monitor，它的后端与中转在跑（额度账由中转记、账号来自它的账号库）。cc-monitor 不装、不起、不认这个 skill；它只用 cc-monitor 已有的三样基本模块（见「依赖」）。

## 它怎么判（每个号一行）

读 `ccm -- --quota-read`（这台中转记下的额度账）里每个号的 5h 重置时刻：

| 那个号此刻 | 做什么 |
|---|---|
| 5h 窗口在计时（重置时刻在将来） | 不动，醒在重置时刻 ＋ 60 秒 |
| 5h 窗口已过期 · 没有 5h 窗口 · 这台从没见过它的数 | 用这个号发一句，让窗口开始计时 |
| 被拒（5h 或 7d 用满） · 7d 用到 100% | 等到它重置 ＋ 60 秒 |
| 登录失效 | 记一行，1 小时后再看 |
| 账号库里没有这个号 | 跳过 |

发完再读一次额度账：出了新数且窗口在计时 ⇒ 成；没出新数 / 回包说出错 / 超时（120 秒）⇒ 记「没成」，15 分钟后再试。读不到额度账 ⇒ 15 分钟后再试。

🔴 **没见过数 ＝ 不知道，不等于窗口没在计时。** 额度账只记经过这台中转的回包；别处（别的机器、没走中转的 claude）用过这个号，这台看不到。想知道只能发一句——而**计时中的窗口不会因此被推后**（实测：已在计时时回包给的重置时刻还是原来那个），所以「没见过就发」不亏。

**读重置时刻从来不用发。** 中转经手的每个回包都记进额度账，`--quota-read` 随时读得到。要发的只有一件事：让一个**没在计时**的窗口开始计时。中转从没见过的号，此刻是什么样只有两条路知道——发一句，或者真用它一次。

## 发法

官方 `claude -p` 经 ccm 起、走这台的中转（回包头由中转记进额度账）：

```text
ccm -p --model haiku --tools "" --no-session-persistence --setting-sources project --strict-mcp-config \
    --output-format json --system-prompt '只回复字母a' a -- --account-dir ~/.cc-monitor/accounts/<号> --cwd ~/.cc-monitor/autostart
```

- 不给工具 · 不起 MCP · 不跑用户级钩子与插件 · 关掉思考（`MAX_THINKING_TOKENS=0`）· 不落会话记录。
- 工作目录 `~/.cc-monitor/autostart/`：cc-monitor 后端在历史页与会话列表里藏掉工作目录是它的会话。
- 交给 ccm 的环境去掉了 `CCM_*` · `CLAUDE_CODE_*` · `ANTHROPIC_*` · `CLAUDE_CONFIG_DIR` · `TMUX*`（免得从某个 claude 会话里继承来、发到别的号或别的中转上）。

**花多少**（实测一发，claude 2.1.289）：系统提示词换成一句 ⇒ 输入约 870 token（claude 自带、去不掉的那一段）· 输出 4 token。不换系统提示词约 6.8k token：缓存热时按一成价算，可隔几小时才发一次时缓存早已过期、要按写缓存价付 ⇒ 换掉更省。

## 用法

```text
scripts/quota-warm status      只读：每个号一行，该发的标「⇒ 该发」
scripts/quota-warm once        跑一趟（该发的发）后退出
scripts/quota-warm run         先跑一趟，之后按最早的醒点自己醒来再跑，一直跑
```

| 选项 | 缺省 | 说明 |
|---|---|---|
| `--accounts work,personal` | 账号库里所有已登录的号 | 看哪几个号（逗号分隔，账号库里的名字，即 `~/.cc-monitor/accounts/<名>`） |
| `--system '…'` | `只回复字母a` | 换上的系统提示词 |
| `--text '…'` | `a` | 发的那一句 |
| `--model …` | `haiku` | 用哪个模型 |
| `--dry-run` | 关 | 只判、不发（日志写「该发（--dry-run，没发）」） |

- `once` / `run` 持一把锁 `~/.cc-monitor/quota-warm.lock`（里面是 pid）：同一时刻只跑一份，第二份直接退出并说锁在哪。`status` 不拿锁。
- `run` 长睡分段（每段至多 10 分钟）醒来按真实时刻重算，机器休眠醒来不会睡过头；下一次至少隔 30 秒。没有要看的号 ⇒ 退出。
- 想让它一直在：自己挑方式起 `run`（tmux 窗口、用户级服务……）。cc-monitor 不替你装。

## 怎么读

`status` 每行 `<号>  <说明>`：`5h 40% ↻12:28`（在计时，几点重置）· `被拒（5h）↻…` · `7d 用满 ↻…` · `登录：needsLogin` · `没见过数（只有发一句才知道）  ⇒ 该发` · `5h 窗口已过期  ⇒ 该发`。时刻是这台的本地钟，不是今天的带上 `MM-DD`。

日志 `~/.cc-monitor/logs/quota-warm.log`（同时打到 stdout），一行一件：

```text
2026-10-05 11:28:17 开始（run）：work, personal, team
2026-10-05 11:28:17 work: 5h 40% ↻12:28
2026-10-05 11:28:17 personal: 没见过数（只有发一句才知道） ⇒ 发了（in 870 · cache 0 · out 4）⇒ 5h 1% ↻16:28
2026-10-05 11:28:17 team: 5h 窗口已过期 ⇒ 发了没成（…；额度账没有新数）⇒ 11:43 再试
2026-10-05 11:28:17 下次 11:43
```

「发了没成」常见原因：这台中转没在跑（cc-monitor 后端没起）⇒ 回包没经中转、额度账没新数；号要重新登录；ccm 起不了 claude（日志里是「退出码 N，回包读不懂」或「超时」）。

## 依赖（cc-monitor 的基本模块）

- `ccm -- --quota-read`：读这台的额度账（只读，不读 stdin）。
- ccm `--account-dir <目录>` 用某个号起一次 `claude -p`，走这台的中转。
- 后端藏掉工作目录是 `~/.cc-monitor/autostart/` 的会话（历史页 · 会话列表 · 流上的会话宣告）。

## AI 自己管额度：cc-monitor 的后端命令

都经 `~/.cc-monitor/bin/ccm -- --<命令>`；要参数的从 stdin 读一段 JSON（＝ 参数对象），stdout 一行 JSON。形状与字段全表见仓里 `src/doc/IPC-PROTOCOL.md` 的「额度账与账号轮换」一节及其下各小节。

- `--quota-read`：这台额度账（每个号的状态 · `5h` / `7d` 用量与重置时刻 · 登录 · 从没出过数的号）。
- `--rotation-rules-read`：这台的轮换规则表（每条：名字 · 顺序 · 勾上哪几个 · 满了才换还是到 N% 就换 · 封顶与时段 · 最多等几分钟 · 谁在用）；`defaultRule` ＝ 本机默认指向哪条。
- `--rotation-rule-save`：新建（不给 `id`）或整份改一条（给 `id` ＋ 读到的 `rev` 作 `ifRev`；别处先改过 ⇒ `{state:"conflict"}`、不写）；动态改顺序 ＝ 读额度账、自己排好、整份写回默认那条。`--rotation-rule-rename` · `--rotation-rule-delete` · `--rotation-default-set` 同族。
- `--rotation-session-read` / `--rotation-session-set`：一批会话的轮换与「账号」格（卡住的会话最早什么时候能续：`blocked.earliest.at`）。
- `--rotation-switch`：现在就换（`hot` 不重启 · `restart` 重开）。
