# STATUS — 账号体验重做

## 当前阶段
Phase A **已审批** + 三开放设计点已拍板 + 两执行决策：**F5 走 vendor 内嵌**、**全自动 planned-build loop 一口气跑**（连续 B→G，只在真阻塞/计划≠现实/需新决策停）。
**F5 完成签收（B→F 全过）**：vendor 内嵌 + deploy/check IPC + 前端检测→一键部署。D 审计无阻塞（I1 install 死锁 + S1/S2/S4/S5 已修复验），E 与 F3 正交无耦合债。门禁 tsc0/vitest600/cargo check0/cargo test368。commit 落盘。**下一个：F1**（切号入口：状态栏 chip 全局下拉 + tab 右键 per-session「此会话切到 X」；「当前工作账号」文案统一「当前账号」）。

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
