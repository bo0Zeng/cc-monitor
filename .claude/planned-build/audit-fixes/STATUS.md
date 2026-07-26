# 状态 / STATUS — 恢复入口（每次先读这里）

> 工作区 `audit-fixes`。分支 `account-ux`。跨轮靠此文件,不靠记忆。

- **当前阶段**：C 实现推进（F05 完成 + commit）→ 下一个 F07
- **当前功能**：下一个 = **F07 刷新竞态(I4)+多远端缓存(I5)+onUnselectable**
- **当前步骤**：F05 完成（findOrphanTmux/isCcmTmuxName 纯函数 + cleanupOrphanTmux + chip「🧹 清理孤儿会话」入口，手动/不自动，复用 F02 白名单）。
- **⚡ 停顿门槛调高（用户 2026-07-25「不要再停loop了」）**：只在真·阻塞 / 计划≠现实无法自解 / 同一步失败≥2 / 确实推断不出且必须用户拍的地方停；其余(含已定机制的 F03.2、常规实现)自主推进,不再逐步确认。
- **F03.4 已定 = 甲′ + 丙**：甲′=远端 `set-titles-string ccm-rbind-#{@ccm_sid}`（**裸值无双引号**，session-backend createRunAttach 非阻断两行，aya 已验无轮询）；丙=本地 `wt -w new --title ccm-rbind-<sid> --suppressApplicationTitle` + spawn 后 `RemoteHwndCache.try_bind_with_retry` 前向登记（launch.rs/IPC加sid/bind.rs复用/TS传sid）+ shrink 重试循环。**丙 Windows 侧 aya 验不了 → 你真机验再关 #74/#41**；甲′ aya 全验。
- **F03.2 已定 = 甲-evented**（事件驱动）：收 daemon `TmuxSessions` 帧即算 idle/live/archived、**删 8s 定时器、cc-monitor 侧零轮询**；高风险 → Phase B 先设计 agent 论证 + 实现后全视角 D 审计。
- **无轮询原则**（用户拍板）：cc-monitor 侧不新增轮询;唯一周期扫描=daemon 内部 tmux ls(红线外既有);wrapper 的 __ccm_rbind 轮询归 F14(用户自跑,给事件驱动版)。
- **loop 执行顺序（§4 重排）**：F03.3 → F03.4 → F04 → F05 → F07 → **F03.2+F06 合并**（F03.2 灰灯机制在其联合 Phase B 二选一定，高风险、会停下 present 给用户）→ F08→F09→F10→F11→F12→F13 → Phase G
- **计划审计记录（3 轮独立）**：①Plan agent rev06→3 阻塞+3 重要→rev07。②独立复审 rev07→抓到真阻塞(F03.2-A idle 产出与 reconcile announced_live 门控矛盾)+4 重要→rev08。③独立复审 rev08→再抓 2 阻塞(账本把候选(i)专属成本当定死前置 / idle→archived 正向无产出者)+订正→**rev09 定稿**。三轮除 F03.2-A 账本外全过(完备/红线/归属/排序/可关性 ✓)。
- **已完成功能**：F01 账号安全(75594ff/3221f26)；F02 kill 白名单(e389410)；**F03 步骤1** idle 就地复用(537077b)。**已关 issue #71/#42/#67/#46**（v3.2.0 已修）
- **下一个功能**：F03.2 灰灯(A)→3.3 attach-idle→3.4 rbind 标题+直连/新会话补@ccm_sid→F04→F05→F06→F07→F08-13
- **阻塞 / 待用户确认**：灰灯已定 A。计划审计 agent 在跑，回来若有阻塞级发现则先改计划。
- **最近一次计划回看**：2026-07-25（主计划 rev 06 重制）
- **自动模式（/loop）**：**全自动** —— bug 优先(F03.2→…→F07)，再工程(F08-13)；做完每个 bug 关其 issue；只在阻塞/计划≠现实/同一步失败≥2/新决策/全部完成时停
- **本轮 loop 目标**：下轮 F03.3 attach-into-idle 走 B→F（菜单让 attach 进 idle cc-<sid8> 空 tmux；on-demand 查、不碰 live-state 链路，低风险）+ commit
- **loop 停止条件**：未命中（计划已定稿 rev09、3 轮独立复审过）。后续停点：F03.2+F06 联合 Phase B 的灰灯机制决策(i/ii)、真机验证项、F13 拆分冲突
- **基线**：cargo fmt 0 / 353 Rust 测 / tsc 0 / npm test 573 / build ✓
- **剩余 open bug**：[41,43,60,63,72,74,75,76] 全在计划；[71,42,67,46] 已关
- **loop 节奏**：ScheduleWakeup 60s（用户要求）
- **红线（每轮核对）**：daemon 零行为改动（只用现成 p1p 能力）· 不改 TMUX_LS_FMT 双写点 · 不碰 ~/.bashrc（F14 用户自跑）· 不改 cc-<sid8> 语义 · 不 push/发版/bump · 孤儿仅手动回收
- **feature 拆分记**：F01 三步——①pin 现读(本轮) ②基座可选 ③resume 账号下拉。picker 组件是 F03/F04 共享面。
- **备注**：全自动档下 loop 可自过「不碰新共享面」的功能计划；碰账本外新共享面或 §6 开放问题（如 F03 退出即收 vs 保留）则停下问用户。
