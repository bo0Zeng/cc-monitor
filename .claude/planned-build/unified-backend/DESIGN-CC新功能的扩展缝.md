# 设计稿 B · CC 新功能的扩展缝（「后面 Claude Code 加的功能怎么加进去」）

- 产出：2026-08-02。用户中途点名：「思考一下后面的 claude code 加的功能（大概率就是 / 命令）怎么加进去。」
- 性质：专职设计 agent 的只读调研 + 设计；**语料实测全部只读** `~/.claude/projects/`，未改任何文件
- 主线程复核见 §9

## §0 一句话

**cc-monitor 对 CC 的耦合分九个面，其中「新记录类型」这一面已经是宽容降级的（F63），
且远端 daemon 对它完全透明 —— 这两条是本仓最大的结构红利。
真缺口不在「扛不住变化」，在「变化发生了没人知道」，以及「daemon 需要新动作时没有能力协商」。**

## §1 语料实测（2026-08-02 本机全量，只读）

| 项 | 值 |
|---|---|
| 会话文件 | **1,904** 个 jsonl / **1,876 MB** |
| 记录总数 | ≈**471,700** |
| 非法 JSON 行 | **1** 条 |
| 记录 `type` 种类 | **20** 种 |
| cc-monitor **认识**的 | 10 种 |
| cc-monitor **不认识**的 | **10 种 / 27,696 条 / 5.87%** |
| 未知类型的 `uuid`/`parentUuid` | **全 0**（10 种逐种核过） |
| 已知 type 缺必填字段的行 | **0** |
| slash 命令名种类 | **32** |

未知类型逐条：`mode` 20498 · `file-history-delta` 2562 · `agent-name` 2312 · `pr-link` 2177 ·
`relocated` 108 · `worktree-state` 19 · **`started` 6** · **`result` 6** · **`fork-context-ref` 5** ·
`frame-link` 3。

> ### ★ 本设计最要紧的一条实测
>
> `doc/INVARIANTS.md §18.1`（F63，2026-07-16）记的是「7 个未知 type / 8,774 条 / 157,385 行」。
> **今天是 10 个 / 27,696 条 / 471,700 行** —— CC 在 17 天里加了 3 个新记录类型
> （`started` / `result` / `fork-context-ref`），**而仓里没有任何东西知道这件事**
> （`parser.rs` 逐字写着「刻意不对 unknown-type 分支 warn」—— 那个决定是对的，20,498 条 `mode` 会刷屏）。

### 1.2 一个新目录形状，今天已经坏了一处

`started`/`result` 全部来自：

```
projects/<enc-cwd>/<sid>/subagents/workflows/wf_<id>/
    agent-<hash>.jsonl        ×6
    agent-<hash>.meta.json    ×6   {"agentType":"workflow-subagent","spawnDepth":1}
    journal.jsonl                  {"type":"started"|"result", key, agentId, result}
```

本机 **7 个**这种目录。而 `subagent.rs::list_meta_matches` 是**非递归 `read_dir`** +
**`description` 精确相等**匹配，而 `workflow-subagent` 的 meta **根本没有 `description` 字段**
—— 两个独立原因各自足以不命中 ⇒ 用户展开这类 Task 卡看到「加载失败：no subagent matches description=…」。

**这不是假想用例，是今天的缺陷。**

## §2 九个耦合面 + 三条判据

| # | 面 | 今天的形态 | 失效模式 |
|---|---|---|---|
| 1 | jsonl 新 `type` | **宽容 + 抢救** | 不显示、不告警、**不崩** |
| 2 | 已知 type 的字段变化 | 宽容 schema + **per-line warn** | 今天 0 条；触发即 **0→全量刷屏** |
| 3 | 记录里的枚举值 | **两种相反处置并存，都是刻意的** | 见下 |
| 4 | 目录/文件布局 | 顶层有适配层 ✅ / `subagents/` 内部硬编码 ❌ | §1.2 已实证坏 |
| 5 | slash 命令（jsonl 侧） | **已经完全数据驱动** | 零改动扛住 32 个命令名 |
| 6 | CC CLI flag | 数据驱动注册表 + 透传缝 | 加 flag = 注册一项 |
| 7 | CC 配置文件 | 宽容读 + 窄硬编码 | 见 N7 |
| 8 | CC 落盘格式（我们写） | **逐字段仿制，无真机对拍** | **最贵**：改了没有任何信号 |
| 9 | daemon 侧 | 记录**完全透明**；语义解释有耦合 | 见 §4 |

**面 3 的细节（就是「排他式契约」那条）**：`status`/`subtype`/`operation` 宽容；
**`kind` 与 `stop_reason` 排他**（前者决定「算不算可交互会话」，后者决定「这一轮结束了」）。

### 三条判据（别一刀切）

- **Q1「看不懂时什么都不做」安全吗？** 少一张卡 = 安全 → 宽容；会误导、会驱动破坏性动作、会计错钱 → 排他。
- **Q2 这个值是「描述」还是「授权」？** 描述（灯色、命令名）→ 宽容；授权（`kind` 决定建不建 Tab / 能不能被 kill，
  `stop_reason` 决定发不发通知 / 放不放行 send-keys）→ 排他。
- **Q3 错的那一侧代价对称吗？** 对称/可撤销 → 宽容；不可逆 → 排他。

用它校验既有设计：`kind` 排他 ✅、`status` 宽容 ✅、`stop_reason` 排他 ✅、记录 `type` 宽容 ✅ ——
**四条都对，一条不改。**

> **但两者有一个共同的、今天缺失的配套义务：宽容 ≠ 无声，排他 ≠ 无声。**
> 「看不懂就降级」和「不在白名单就隐藏」都是**正确的行为**，但都必须留下一条可查的记录 ——
> 否则「CC 变了」这件事本身不可观测。今天两边都是全静音。

## §3 推荐（六件，按性价比排）

**R1 · 未知面记账（核心，零行为变化）** —— 在四个降级点各加一次有界计数（进程内 `AtomicU64`，
上限 64 键，溢出记 `<overflow>`），经既有的 `config_surface` 诊断面暴露成一张只读表：
未知记录 `type` + 条数 + 首见样例 · 未登记的 pidfile `kind` · 未登记的 `status` ·
hello 里不认识的 `capabilities`/`emits`/`commands` token。
**为什么第一件**：把「CC 变了」从不可观测变成看一眼就知道。**为什么不是 warn**：实测 20,498 条 `mode`。

**R2 · warn 节流 + 必填字段清单显式化** —— 那条 per-line warn 今天 0 条，而这正是危险所在：
失效模式是**二值的**（一旦 CC 改了某个已知 type 的必填字段，从 0 跳到全量；一个 37MB 会话 = 几万条 warn）。

**R3 · subagent 发现层递归 + 匹配分档**（修今天已坏的）：递归扫 `<sid>/subagents/**`（有界深度 3）·
匹配分三档（`description` 精确 → `agentId` 精确 → `timestamp` 最近）**并把档位说出来**。
⚠ **诚实登记**：workflow 子 agent 的 meta 没有 `description`，第一档必然落空；
能做到的上限是「列出这个 workflow 下的 N 个 agent 让用户选」，**做不到自动定位**。别为此发明启发式。

**R4 · `/branch` 落盘格式的真机对拍** —— `branch_matches_native_fork_shape` 钉的是**手写合成夹具**，
CC 改了原生 `/branch` 的形状它照样绿。苗头已经有了：新出现的 `fork-context-ref`
说明 CC 侧长出了**第二套分叉记账**，与我们仿的 `forkedFrom` 不是同一套。
做法（廉价版，**刻意不进 CI**）：`#[ignore]` 真机对拍 + `RELEASING.md` 一行手工步骤。

**R5 · hello 加第五轴 `subcommands`**（daemon 侧唯一的真缺口，见 §4）。

**R6 · 四张表（`SUBCOMMANDS`/`COMMANDS`/`EMITS`/`CAPABILITIES`）↔ `BUILD_ID` 的机检**（20 行）——
`main.rs` 的 p1r/p1t/p1u 三段注释逐字写着「加子命令必须 bump BUILD_ID」这一课，而 p1u 那次**仍然漏了**。

## §4 daemon 这一侧：能力协商够不够

分三档，**只有第三档不够**：

- **(a) 只是新记录类型 → daemon 零改。** `watcher::process_jsonl` 逐行 raw 转发，**daemon 不解释 `type`**。
  ⇒ **这条要写进 INVARIANTS**：它今天是**被动成立**的，没有任何护栏钉住。
  哪天有人为了某个功能在 daemon 侧加一句「只转发已知 type」，这条红利就没了。
- **(b) 需要 daemon 解释新语义** → additive wire 字段 + `skip_serializing_if` + `emits` 声明。**够。**
- **(c) 需要 daemon 执行一个新动作** → **不够**。

### 缺口①：一次性子命令没有能力协商

hello 今天四轴（`capabilities` 流 flag / `emits` 帧 / `commands` 入方向命令 / `kinds` agent），
**唯独没有「我认识哪些一次性子命令」**。后果：monitor 发 `--foo`，旧 daemon 不认 ⇒ 忽略 + 进流模式
⇒ 吐 hello 然后开始推行，调用方拿到一坨 jsonl 而不是查询结果。
`remote_branch.rs` 为此手写了 **hello marker 嗅探** —— **每加一条子命令就要重写一遍**，
而且它自己的注释记着两份判据曾经不一致。

⇒ 加第五轴 `subcommands`（additive、源 = `main.rs::SUBCOMMANDS`、同款测试钉住）。
**不能塞进 `capabilities`**：§26 的护栏是「声明 ⟹ 会剥离对应流 flag」，一次性子命令没有 flag 可剥。
**⚠ 它不解决存量**（旧 daemon 不发这个字段），marker 嗅探不能删，只能从「每条各写一份」收敛成「一份共享 fallback」。

### 缺口②：BUILD_ID bump 靠记性 → R6

## §5 两个用例走查

### 用例甲：CC 新增 `/foo`，产生 `type:"foo_result"`

**不改一行的结果：静默不显示。不崩、不刷 warn、不误折叠、不误计费、不误计数。**
（daemon 原样转发 → `parser` salvage 成 `Unrecognized` → `is_displayable` true →
`cards/index.ts` 落 `default:` skip → 不进搜索索引 → 不计费。）

要「看见」它，按需求分档：

| 想要 | 改几处 |
|---|---|
| 知道它存在（诊断） | **0 处**（R1 那张表自动列出来） |
| 渲染成一张卡 | **3 处**（`messages.rs` 加变体 · `is_displayable` · `cards/index.ts` 加 case） |
| 还要进分支链 | +1（`branching.ts` 白名单） |
| 还要可搜 | +1（`search.rs` 一臂） |
| **远端也一样** | **0 处**（daemon 透明） |

**历史成本样本**（不是估计）：`ai-title→custom-title` 6 文件 · `queue-operation` 7 · pidfile `status` 8 ·
`kind:"bg"` 6+20（两仓）· F63 抢救 5 · F62 原生 `forkedFrom` 8。
⇒ **本设计要保住的性质：默认 0 处、按需 3-5 处、远端恒 0 处。**

### 用例乙：CC 新增一个「要在会话里执行 + 要落盘」的能力

基线 = `--fork-session` 实测（daemon 侧 10 文件 / monitor 侧生产约 10）。daemon 侧九步，
其中**最易漏的是「`SUBCOMMANDS` 加了但 dispatch 臂没加」**（p1t 那个真 bug：实现完整但 match 漏列
⇒ 落 `_` 臂 ⇒ `unknown argument` exit 2，而 monitor 真的在发那条命令）。
旧 daemon 的用户今天看到的是一条手写的诚实降级提示（每条新命令都要重写一遍）；
R5 之后 monitor **在点按钮之前**就知道 ⇒ 置灰 + 一键重装，且对每一条未来子命令自动成立。

## §6 代价与反对意见

- **对 R1**：「这是给未来做的抽象」—— **不成立**，它记的是**已经发生的事**（10 个未知 type / 27,696 条，
  其中 3 个是 F63 记档之后新出现的）。真代价是热路径（限制在 salvage 分支，5.87% 的行，那里本来就已经在做第二遍 parse）+
  多一个只读 IPC（`parity_ledger` 要登记，天然 `Both`）。
- **对 R2**：「今天 0 条，在修不存在的 bug」—— 部分成立。反驳：失效模式是二值的。**若只能做一件，先做 R1。**
- **对 R3**：放宽匹配会挂错 ⇒ 档位严格有序且**必须在 UI 上说出来**；上限诚实（做不到自动定位）。
- **对 R4**：「不进 CI 等于没有」—— **接受这个批评**。反驳：CI 里没有活的 CC（`e2e/fake-claude` 是个 sleep shim），
  为了自动化往 CI 塞一个真 CC，引入的维护面比它防的漂移更大。
- **对 R5**：**它不消掉 marker 嗅探**（存量旧 daemon 不发这个字段），别把它当成能立刻删的理由。
- **对 R6**：「不 bump 也不一定出事」—— **出事过两次，逐字有档**。

## §7 不做什么

- **N1 不做「记录类型注册表 / 插件式渲染器」。** 一个新记录类型要同时回答四个不同子系统的问题
  （上不上 wire / 进不进分支链 / 长什么样 / 进不进搜索），而这四个的**默认值互不相同**。
  今天那 3-5 处改动**每一处都是一个真决定**，压成一处只是把决定藏起来变成含糊默认值。
  **重开条件**：出现第 3 个需要**同时**打开这四个开关的新类型，且三次改动的 diff 高度雷同。
- **N2 不把 `kind` / `stop_reason` 改成宽容**（授权型判据，有事故背书）。要加的只是 R1 的记账。
- **N3 不把 `AGENT_PROFILE` / `adapter` 参数化成「CC 版本表」**（没有可靠版本信号；宽容 schema 是版本无关的替代品）。
  **重开条件**：出现一次「同一字段在新旧版本里语义相反」。今天零实例。
- **N4 不做 `/branch` 对拍进 CI。**
- **N5 不为 slash 命令建白名单**（今天已完全数据驱动，加白名单是负收益）。
  ⚠ 但要订正一条**被实测证伪的注释**：`cards/slash.ts` 写着「`/clear`、`/help`、`/model` 等 CLI-only 命令
  不会写 JSONL，物理上识别不到」。实测：`/model` **70 条**、`/context` 11、`/login` 4、`/doctor` 3、
  `/ide` 3、`/exit` 3 都在 jsonl 里；只有 `/clear`（0）和 `/help`（0）是对的。
- **N6 不动 daemon 的记录透明性去「优化」**（别加「只转发已知 type」）。**建议把这句写进 INVARIANTS** ——
  它今天是被动成立的、无护栏。
- **N7 不为 `hooks_diag` 硬编码的两个钩子事件做数据驱动**（那是「cc-bus 自己装了哪两个钩子」的诊断，
  不是「CC 有哪些钩子事件」的枚举）。

## §8 落地

| 件 | 落点 | 理由 |
|---|---|---|
| **R1 + R2** | **新开 `U-CC1 数据面漂移记账`** | 与 U 序列完全正交、零行为变化、**无前置依赖，越早越好**（装上之后才开始有数据）|
| **R3** | **新开 / 挂 BACKLOG bugfix** | 今天**已坏**的缺陷，不该等 U 序列 |
| **R6** | 挂 **U10** | 20 行，挡已犯两次的错；不依赖 R5 |
| **R5** | 挂 **U10** | U10 必然要加新子命令 + 入方向命令，是这条协商轴的**第一个真消费者** |
| **R4** | 挂 **U11** | U11 本来就要动 `--fork-session` / `cc-spawn` |
| 文档三条 | ②③ 随 R1/R5；① 挂 **U13/U14** | ① INVARIANTS 加「daemon 对 jsonl 记录类型透明」；② §18.1 更新实测数字；③ 订正 `cards/slash.ts` 那条注释 |

**顺序**：R3（已坏、独立）→ R1 → R2 → R6 → R5（等 U10 的真消费者）→ R4（随 U11）。

## §9 主线程复核

- **两条「今天就坏/就错」的实测，我认为最有价值，且都不在 U8a-2b 的范围里**：
  ① `subagent.rs` 对 `subagents/workflows/wf_*/` 已经加载失败；② `cards/slash.ts` 那条注释被语料证伪。
  两条都**新开件**，不塞进本轮 —— 本轮是控制面，混进读面缺陷会让 commit 讲不清一件事。
- **「CC 17 天加了 3 个记录类型而我们不知道」** 是本设计的核心论据，它把 R1 从「给未来做的抽象」
  变成「记录已经发生的事」。我采纳这个论证。
- **N1 我特别赞同**：本仓已有「拆由具体架构病证成」的原则，R1 的价值恰恰在于它**不改行为**、只产生信号，
  等信号攒够了再谈抽象。
- **未验证项**照实登记，没有当成事实写进任何代码注释。
