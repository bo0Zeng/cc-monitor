# U7-5 · 自洽夹具扫查 + 分类 ③ 判定

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：低（只加一条护栏 + 做判定）

## ① 自洽夹具扫查：查过了，**其余三处没有**

U7-4 的失效模式是「测试的写侧与生产的读侧用同一个 `const`」。按主计划新补的横切约定，
把 `usage-core` / `branch-core` / `inbound` 都扫了一遍，**用变异验、不靠读代码**：

| 目标 | 常量 | 变异 | 结果 |
|---|---|---|---|
| `usage-core` | **零个常量** —— 契约是 JSON 字段名，内核 `get("x")` 与夹具 `"x":` 各写各的字面量 | 内核改 `output_tokens` → `output_tokenz` | 3 条红 ✅ |
| 同上 | | 内核改 `requestId` → `reqId` | 3 条红 ✅ |
| `branch-core` | **零个常量** | —— | 无可查 |
| `inbound` | `MAX_LINE_BYTES` / `REPLY_CHANNEL_CAPACITY` / `COMMANDS` | 见下 | —— |

**结论：`usage-core` 与 `branch-core` 没有这个问题。** 如实说明，不为凑一条修复而改东西。

⚠ 一处**较弱但不改**：`inbound` 的内存/超限测试用 `MAX_LINE_BYTES` 自己算洪水大小
（`"p".repeat(MAX_LINE_BYTES)`），常量变则洪水一起变 ⇒ 测不出那个**数值**变化。
但它不是跨边界的契约名（不上线、monitor 不认它），是本地调参值 —— **判定不改**，登记于此。

## ② 顺带补上的一个真空白：入方向命令名没有钉到文档

扫 `COMMANDS` 时发现：`hello_commands_match_the_dispatch_table` 钉的是
`COMMANDS ↔ dispatch 分派臂` —— **两边都在代码里，文档不在其中**。

实测三步：

| 变异 | 谁红 |
|---|---|
| 只改 `COMMANDS`（不改分派臂） | `hello_commands_match_the_dispatch_table` |
| `COMMANDS` + 分派臂**同时**改 | 只剩 `ping_replies_ok` 这条**行为测试** |
| 上述再加上行为测试里的命令名（= 一次彻底的重命名） | **什么都不红**，文档里的 `ping` 成了一个不存在的命令 |

客户端是照文档发命令的：文档说 `ping`、daemon 只认 `heartbeat` ⇒ `unknown_command`，
而两边各自看都「对」。

⇒ 补 `every_inbound_command_appears_in_the_protocol_doc`（与 wire 字段、`--子命令` 同一档）。
变异复验：彻底重命名 ⇒ 红 `["heartbeat"]`。

## ③ §0.1 分类 ③ 的判定：**不按合流排，而且描述已过期一半**

计划让我先判定「判活 / tmux 观测」该不该合。实测两处都与 §0.1 的描述不同：

### 判活

§0.1 说「两个恒定分支互相取反：本机非 Windows 恒 `false`、daemon 非 Linux 恒 `true`」——
**后半已过期**：U4a 把 daemon 的非 Linux 分支从 `true` 换成了 `unimplemented!()`
（那次的原话：「`true` 是一个没人会发现的谎」）。

**前半仍然成立，而且后果比「恒 false」这个说法严重得多。** 实测调用链：

```
spawn_watcher(projects_dir, active_filter, …)      lib.rs:441
  → active_filter(sid) = map.is_session_active(sid) lib.rs:413
    → is_process_alive(pid, proc_start)             session_map.rs:198
      → #[cfg(not(windows))] false                  session_map.rs:517
```

⇒ **Linux / macOS 上，本机 watcher 的活跃过滤器拒绝每一个会话 —— 一行本机 jsonl 都不会 emit。**
另外每 2s 的心跳收割器（`!is_process_alive(...)`，**无平台门**）会把会话表清空。

也就是说：**cc-monitor 的本机读面在 Linux/macOS 上整体不工作**，那两个平台上它只能当远端监视器用。
这与仓库的 Windows-first 定位自洽（`bind.rs` 整个 `cfg(windows)`、PowerShell 集成、`ccm` 只装远端），
**但它没有出现在任何面向用户的文档里**（`ARCHITECTURE.md` / `README.md` 都没说）。

⇒ **U7d 的性质要改写**：不是「合并两个残桩」这种清理，是
**「本机读面在两个平台上不存在」这件事本身**。这决定它的优先级与验收方式。

### tmux 观测

§0.1 把它列成「本机 `tmux.rs` ↔ 远端 `watcher.rs` + hook」。实测 **`tmux.rs` 根本不是本机实现**：
它的公开入口是 `list_remote_tmux(origin)`，**开一条 SSH exec 去列远端的 tmux**。

所以那不是「本地 vs 远端」的平台残桩对，是**同一份远端数据的旧轮询路径 vs 新推送路径**
（B2 的 `tmux_sessions` 帧正是为替掉 8s 轮询加的）。⇒ 真问题是「**旧路径退役了没有**」，
是一次**退役**，不是「写新实现」。

⇒ 两处都**不该按前几件「抽共享 crate」的模式排**，而且**彼此性质也不同**，不该并成一件。

## 交付

一条新护栏 + 上述判定落进主计划。**没有为了凑修复而改代码。**

## 签收

- [x] 过代码审计（D）—— 本轮全是测量与一条护栏，变异复验见上表
- [x] 过工程审计（E）—— 判定已改写 U7d / U7b 的性质，见主计划
- [x] 主计划已更新（F）
