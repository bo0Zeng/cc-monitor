# STATUS — 账号体验重做

## ⛔ 本工作区已被接管（2026-07-27）
范围从「账号 UX」再次扩大为「**统一整个软件的会话启动架构**」，且 `MASTERPLAN-v2.md` 经四视角
full-audit 判定**不可直接执行**（结论见本目录 `AUDIT-v2-FINDINGS.md`，仍是权威引用源）。
**新工作区 = `../unify-launch/`**（`MASTERPLAN.md` + `INVENTORY.md` + `STATUS.md`）。
本区已交付并保留：F5 一键部署(9d3a7e6)、F1 切号入口(7680a43)。余下 F2-F7 并入 unify-launch 的 F09/F10。
**下面的内容是接管前的历史记录，不再更新。**

## ⚠ 重大转向（2026-07-27，用户暂停 loop 后）
用户把任务从「账号 UX 清理」升级为「**统一整个软件的会话启动机制 + 双账号集成 + 架构做干净**」。红线全解（见 REDIRECTION-v2 R6）。诊断（枚举 agent a6c54efe）：4 套 builder + 2 套不兼容账号模型 + @ccm_sid 分叉 = restart/resume 经常失败的根因。**loop 暂停**，正在做架构 design fan-out（2 Plan agent：a1c53887 启动路径统一 / ab0ebd3e 双账号集成+终端同步）→ 综合出主计划 v2 → 用户审批 → 重启 loop 从 U0（启动统一）开干。新方向见 `REDIRECTION-v2.md` + `UNIFIED-PLAN.md`。原 F2-F7 顺序作废，按 UNIFIED-PLAN §三 的 U0→U1→U2→U3→F2′→F3→F4→F7 重排。

## 当前阶段
Phase A **已审批** + 三开放设计点已拍板 + 两执行决策：**F5 走 vendor 内嵌**、**全自动 planned-build loop 一口气跑**（连续 B→G，只在真阻塞/计划≠现实/需新决策停）。
**F5 完成签收（B→F 全过）**：vendor 内嵌 + deploy/check IPC + 前端检测→一键部署。D 审计无阻塞（I1 install 死锁 + S1/S2/S4/S5 已修复验），E 与 F3 正交无耦合债。门禁 tsc0/vitest600/cargo check0/cargo test368。commit 落盘。**F1 完成签收（B→F 全过）**：chip 去 ⚠k 成纯全局切换器 + tab 右键 relabel「把此会话切到账号 X」+ 全局 rename 当前账号。D 审计无阻塞（I1 文案一致/S3 补正路覆盖等已修）。门禁 tsc0/vitest598。commit 落盘。
**下一个：F2**（撤对齐主 UI：删 tab 徽章 ⇄ + mismatch 主 UI + **清 F1 移交的 countAccountMismatches 死代码**；保留命令面板 alignAll/alignableSids；主 UI 换诚实提示）。相关：tabs.ts(⇄/updateAccountBadge/countAccountMismatches)、tabs.vitest。
（已 commit：F5 9d3a7e6、F1 待本轮 commit。）

## 恢复入口
读本文件 + MASTERPLAN.md。功能顺序：F5（部署地基）→ F1（切号入口）→ F2（撤对齐）→ F3（面板砍卡片）→ F4（加号一键）→ F6（终端起号）→ F7（用量）。

## 已完成
- Phase A：三路调研（tmux 控制模式已开 issue #82 / cc-switch UX 标杆 / 账号起号机制勘查）勘定现状；MASTERPLAN 落盘（F1-F7 + 共享面账本 + 概念收敛 13→3）。
- 用户三决策：对齐→命令面板 / verify·sync→排障折叠 / 先出计划审批。

## 下一个
用户审批主计划 + 三个开放设计点（F1 切号语义 / F7 用量深度 / chip 去留）→ Phase B 写 features/F5。

## 阻塞
无（等审批）。

## 自动模式
半自动：主计划 + 每个功能计划过用户；审批门禁与阻塞停 loop。

## loop 停止条件
撞审批门禁 / 计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G。

## 红线
不碰 ~/.bashrc（rc 只生成+复制）· 不新增轮询 · 不用 emoji · daemon 起会话机制零改 · git commit 无 Co-Authored-By · 底层 remote-launch CLAUDE_CONFIG_DIR 注入机制不动。
