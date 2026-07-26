# 状态 / STATUS — 恢复入口（每次先读这里）

> 工作区 `audit-fixes`。分支 `account-ux`。跨轮靠此文件，不靠记忆。主计划见 MASTERPLAN.md（rev 11）。

## 当前
- **阶段**：C 实现推进（F07 完成 + commit）→ 下一个 **F03.2 + F06**（灰灯事件驱动 + #60，最高风险）
- **下一个功能**：**F03.2 灰灯（甲-evented 事件驱动）+ F06（#60/#43 真机验证 + #43 父子拉不起来残留）合并** —— 最高风险，带全视角 D 审计
  - F03.2 灰灯 = 甲-evented：收 daemon TmuxSessions 帧即算 idle/live/archived、删 8s 定时器、零轮询。Phase B 先开设计 agent 论证 emitter/§24 协调，实现后全视角并行 D 审计。**机制已定=甲-evented，别停下问；只在撞真状态机冲突/计划≠现实时停。**
  - F06：#43「父子拉不起来」残留（代码，若可复现）；#60/#43/#63-attach 的 F74* 真机验证项归纳（用户侧）。
- **之后**：F08→F09→F10→F11→F12→F13 → Phase G。

## 已完成（commit）
- F01 账号安全(75594ff/3221f26) · F02 kill 白名单(e389410) · F03.1 idle 复用(537077b) · F03.3 attach-idle(5fd77b8) · F03.4a 甲′(85f1a0d) · F04(5494293) · F05(14dff16)。
- **已关 issue**：#71 #42 #67 #46。剩余 open bug=[41,43,60,63,72,74,75,76] 全在计划。

## 已定决策（勿再问）
- **F03.4 = 甲′(已完，aya 验) + 丙(延 Windows 真机批次——aya 无 cross-target 编译都验不了)**。#74 主体已被甲′修；#74/#41 留真机。
- **F03.2 灰灯 = 甲-evented**（收 daemon `TmuxSessions` 帧即算、删 8s 定时器、cc-monitor 侧零轮询）。高风险 → Phase B 先设计 agent 论证 + 实现后全视角 D 审计。
- **无轮询原则**：cc-monitor 侧不新增轮询；唯一周期=daemon 内部 tmux ls（红线外既有）；wrapper 轮询归 F14。
- **F04 keepalive = 非-bug**（-NoExit 已保留窗口）；孤儿**仅手动**(F05)。

## 自动模式
- **全自动 + 高停顿门槛（用户「不要再停loop了」）**：只在真·阻塞 / 计划≠现实无法自解 / 同一步失败≥2 / 确实必须用户拍板处停；其余（含已定机制的 F03.2、常规实现）自主推进、不逐步确认。
- **节奏**：ScheduleWakeup 60s。做完每个 bug 关代码能确证的 issue（需真机/daemon 版本留开）。

## 红线（每轮核对）
daemon 零行为改动 · 不改 TMUX_LS_FMT 双写点 · 不碰 ~/.bashrc(F14 用户自跑) · 不改 cc-<sid8> 语义 · 不 push/发版/bump · 孤儿仅手动 · **cc-monitor 侧不新增轮询**。

## 真机/你侧待办（代码完也得你验再关）
丙(F03.4b) · #74/#41 · #60/#43(真机复现) · #75(resume 真拉起) · #60②/#63-attach(**须远端重装 ccm 助手写 @ccm_sid**) · #63 torn-tail(daemon-bound，本轮不修) · F14(.bashrc 迁移 + wrapper 去轮询)。

## 基线
tsc 0 / npm test 586 / cargo 353(Rust 自 F02 未动) / build ✓。
