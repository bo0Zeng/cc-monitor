# U7b · tmux 两条读路的判定 + 防轮询回潮

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：低（一条结构护栏 + 判定；零生产代码改动）

## Phase B：**旧轮询早就退役了** —— U7-5 的「旧 vs 新」框架也要修一次

U7-5 判定它是「同一份远端数据的旧轮询 vs 新推送 ⇒ 真问题是旧路径退役了没有」。
复核实测，**那个框架仍不够准**：

| 实测 | 出处 |
|---|---|
| 8s 轮询**已经没了** | `ssh_source.rs:1006` 注释逐字写着推送账本「**替掉每 8s 新建 SSH 的 `list_remote_tmux` 轮询**（B2 治远端 sshd 日志刷屏）」 |
| 前端**没有**任何 `setInterval` 轮 tmux | 全仓 grep 零命中 |
| 对账路径**零次**调 exec | `grep -c list_remote_tmux tmux_reconcile.rs` = **0**，它由 `TmuxSessions` 帧驱动 |
| 但 `list_remote_tmux` **远非死代码** | `tabs.ts` **6 处**生产调用 + `fork-flow.ts` 1 处 |

⇒ **不是「旧路径没退役」，是「退役的是轮询，保留的是按需查」。** 两者不是同一件事。

## 判定：两条路**刻意并存**，不合

| 路径 | 谁在用 | 为什么不能没有 |
|---|---|---|
| **推送账本**（`tmux_sessions` 帧 → `REMOTE_TMUX_RAW`） | 后台**对账**（判 idle / 归档） | B2 加它正是为了替掉轮询 |
| **按需 exec**（`list_remote_tmux`） | `tabs.ts` 的 6 个**决策点**（接 / 起 / 回退） | 账本**只在该 origin 有 daemon 流连着时才有数据**；未连、daemonless、刚启动都为空。决策点要的是**当下这一刻**的权威读，不是推送节奏上的快照 |

硬合只能二选一：**要么对账退回轮询（撤销 B2），要么决策点在没有 daemon 时失去数据。**
——与 U7-3 判 `is_safe_config_dir` / `norm_dir` 「刻意不合」同一形状。

已知的混用坑另有守卫：`superseded_always_archives_even_when_tmux_snapshot_still_shows_the_sid`
（`/branch` 原地换 sid 时那份快照**恒错**）。

## 交付：一条防回潮的结构护栏

`the_reconcile_path_never_execs_its_own_tmux_listing` —— **对账路径的生产段里不许出现
`list_remote_tmux`**。防的是最自然的那个补丁：「对账拿不到数据时顺手 exec 一下」，
它会把 B2 治好的事（每 8s 一条新 SSH、远端 sshd 日志刷屏）原样带回来。

变异复验：在对账路径插一行提到那个函数 ⇒ 红「对账路径开始自己 exec `tmux ls` 了 —— 那是轮询回潮」。

## 顺带交付 U7d 的文档那一半

U7-5 实测出「**本机读面在 Linux/macOS 上整体不工作**」而**任何面向用户的文档都没写**。
本轮补进 `doc/ARCHITECTURE.md`（新一节，含完整调用链）+ `README.md` / `README.en.md` 顶部提示。

⚠ 起初我在 README 里写了「给 `localhost` 配一条远端即可」当变通 —— **那是未经验证的断言**，
已按纪律 7 改成「理论上应当可行，但**没实测过**，不作为方案写在这里」。

**U7d 的另一半（要不要给 `is_process_alive` 写 `/proc` 实现）是功能决定，不在本件。**

## 签收

- [x] 过代码审计（D）—— 一条护栏 + 变异复验
- [x] 过工程审计（E）—— 判定写入主计划；U7b 由「退役」改判「刻意并存 + 防回潮」
- [x] 主计划已更新（F）
