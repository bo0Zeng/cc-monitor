# F-E4 — 孤儿 tmux 清理 e2e（F05）+ 可注入 confirm seam

> Linux 自环（复用 F-E1/E2/E3 fixture）。分支 account-ux。
> 边界矩阵见 MASTERPLAN「★功能边界矩阵 › F-E4」。红线：daemon 零改·不 push/bump·无 emoji·孤儿仅手动。

## 目标
端到端验孤儿清理：无 tab 的 `cc-*` tmux 会话被 scan 计入并（确认后）真删，且**不误伤**非 `cc-*` 用户会话与 `<project>_cc`（cc-bus 资产）。附建唯一生产码 seam。

## ★唯一生产码改动 = 可注入 confirm seam（行为等价）
`cleanupOrphanTmux`（tabs.ts:2577 附近）/ `killRemoteTmux`（tabs.ts:2554 附近）现用裸 `window.confirm`（headless 卡死）。**加 `opts.confirm` 注入**，对齐 `account-restart.ts:84` 已有范式（`const confirmFn = opts.confirm ?? ((m)=>window.confirm(m))`）：
- 默认不传 → `window.confirm`（**默认交互零变化**，行为等价）。
- 测试/DEV 注入 `()=>true` / `()=>false`。
这是本轮**唯一**动生产码处；改完 tsc+vitest 必须绿、f40 不破。主线程 D 审计重点核**行为等价**（默认路径未变）。

## DoD（边界，逐条实跑断言）
- [ ] 无 tab 的 `cc-*`（带 @ccm_sid）→ `findOrphanTmux` 计入；`cleanupOrphanTmux(confirm=()=>true)` → `tmux has-session` 消失（真删）。
- [ ] 非 `cc-*` 用户会话（如 `mywork`）→ **不列孤儿、不删**。
- [ ] `<project>_cc`（cc-bus 资产，如 `KVM_cc`）→ **不列孤儿、不删**（`isCcmTmuxName` 只认 `cc-<8hex>`，不认 `*_cc`——断言此）。
- [ ] confirm 接受(`()=>true`) 真删 vs 拒绝(`()=>false`) no-op（`has-session` 仍在）。
- [ ] 零孤儿 → no-op、计数=0；混合场景 → 计数只数真孤儿。
- [ ] **UX 审计 #2 接点（固化现状，非修 bug）**：造一个**正跑 claude（command=claude）的 `cc-*` 会话、其 sid 不在 tabs** → 断言 `findOrphanTmux` **当前是否**把它列为孤儿。按 #2 预期=**会**（误列活会话）。如实断言当前行为 + 在 README/报告标注「此断言固化 UX 审计 #2 的已知缺口：活 claude 会话被误列孤儿，修不修待用户定」。**不在本轮改 findOrphanTmux 判据**（那是修 bug，超测试授权）。
- [ ] 门禁：tsc 0 + vitest 不回归（seam 加了也绿）；f40 不破。

## Fixture（复用 + 扩）
`gen-idle-tmux.sh`（造 `cc-<x8>` idle）；再造：一个 `cc-<x8>` 跑 `fake-claude`（command=claude，模拟活会话）、一个非 cc 名会话（`tmux new -d -s mywork`）、一个 `*_cc`（`tmux new -d -s proj_cc`）。scan/cleanup 走真 `findOrphanTmux` + `cleanupOrphanTmux`（import 真源，注入 confirm）。

## 诚实层级
`findOrphanTmux`/`cleanupOrphanTmux` 是 TabManager 方法（DOM/jsdom 是天花板）+ 真 `tmux` 交互（命令级）。GUI 触发（账号菜单「清理孤儿会话…」）Linux 不可达（同前）。逐边界标层级。

## 步骤
1. sync worktree 到 account-ux；读本文件 + MASTERPLAN F-E4 矩阵 + ux-audit-2026-07-26.md #2。
2. 加 confirm seam（生产码，行为等价）→ tsc+vitest 绿。
3. 扩 fixture + 建 `e2e/orphan-suite.sh`（命令级 + 真 tmux）+ 必要时 vitest 覆盖 seam。
4. 逐边界实跑，回盘核实真结果（tmux has-session 真删/不误伤、confirm 接受/拒绝、#2 现状）。

## 审计结果 / 签收
（D 代码审计[**重点：confirm seam 行为等价**] / E 工程审计 / F 主计划更新 —— 待填）
