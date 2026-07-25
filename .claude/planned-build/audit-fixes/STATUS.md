# 状态 / STATUS — 恢复入口（每次先读这里）

> 工作区 `audit-fixes`。分支 `account-ux`。跨轮靠此文件,不靠记忆。

- **当前阶段**：F 回看完成（F01 全部签收 + commit）→ 下轮进 F02
- **当前功能**：F01 已完成 ✅（步骤 1/2/3 全签收）
- **当前步骤**：—（下一功能 F02）
- **已完成功能**：F01 follow-resume 账号安全（B1 pin 现读 + 基座逃生口 + per-account picker 复用；tmux/history base 一致性转 F04）
- **下一个功能**：**F02 kill 白名单**（`kill_remote_tmux` 补 `is_ccm_tmux_name`，修 I1）
- **阻塞 / 待用户确认**：无（用户已批准全自动 F01–F13）
- **最近一次计划回看**：2026-07-25（F01 完成，主计划 rev 03）
- **自动模式（/loop）**：**全自动** —— 连续 B→G，只在阻塞/计划≠现实/同一步失败≥2/F03 开放问题/全部完成时停
- **本轮 loop 目标**：下轮 F02 走 B→F —— 后端 Rust 小改 + 回归测，低风险
- **loop 停止条件**：未命中
- **基线**：tsc 0 / npm test 570 / build ✓（F01 完成后）
- **loop 节奏**：ScheduleWakeup 60s（用户要求）
- **红线（每轮核对）**：daemon 零行为改动（只用现成 p1p 能力）· 不改 TMUX_LS_FMT 双写点 · 不碰 ~/.bashrc（F14 用户自跑）· 不改 cc-<sid8> 语义 · 不 push/发版/bump · 孤儿仅手动回收
- **feature 拆分记**：F01 三步——①pin 现读(本轮) ②基座可选 ③resume 账号下拉。picker 组件是 F03/F04 共享面。
- **备注**：全自动档下 loop 可自过「不碰新共享面」的功能计划；碰账本外新共享面或 §6 开放问题（如 F03 退出即收 vs 保留）则停下问用户。
