# F-E3 — 换号重启编排 e2e（#68/#69）

> Linux 自环（复用 F-E2/F-E1 fixture + resume-cmd-driver 范式）。分支 account-ux。
> 边界矩阵见 MASTERPLAN「★功能边界矩阵 › F-E3」。红线：daemon 零改·不 push/bump·无 emoji。

## 目标
端到端验优雅换号重启编排：`compact → exit → kill → resume(新账号)` 序列正确，且 resume 落**新账号的 `CLAUDE_CONFIG_DIR`**；批量对齐 idle/busy 分流 + 可注入 confirm。

## 诚实层级（沿用 F-E2 结论，别谎报）
GUI 触发在 Linux 结构性不可达（`launch_powershell_window` 仅 Windows→回退剪贴板）。天花板 = **命令级**（import 真源 `src/account-restart.ts` 的 `restartWithAccount` + `src/tabs.ts` 编排入口，注入 `opts.confirm`，断言命令序列/argv/账号解析）+ **daemon-frame**（exit→kill→re-add 帧）。**逐边界如实标层级，够不着的写清。**

## DoD（边界，逐条实跑断言）
- [ ] restart 账号 X + compactFirst=true：jsonl 追 compact 记录驱动 → 序列 `compact→exit→kill→resume`；resume argv `CLAUDE_CONFIG_DIR` = X 目录（非旧号）。
- [ ] restart 无 compact：直接 `exit→kill→resume`（无 compact 等待）。
- [ ] 检测到 mismatch（活会话账号 ≠ origin 当前号）→ restart 后 mismatch 清零（`detectAccountMismatch` 前后对比）。
- [ ] 批量对齐 `alignAllToCurrentAccount`：idle 会话 vs busy 会话分流；busy 走 confirm（注入 `()=>true` 放行 / `()=>false` 拦下各测一次）。
- [ ] 取消 confirm（`opts.confirm=()=>false`）→ no-op，不 kill 不 resume，argv.log 无新行。
- [ ] 失败语义边界：kill 失败**中止不续 resume**（account-restart.ts:152-161）；resume 未起来不记账不报成功（:170-178）——各造一次断言。
- [ ] 门禁：tsc 0 + vitest 不回归；不破 f40-suite；零 src/daemon 改动（本轮不改生产码；confirm 注入用**测试内**注入，不改 tabs.ts 裸 confirm——那属 F-E4 seam）。

## Fixture（复用，别重造）
F-E2 的 `e2e/fake-claude`（记 argv+env、追 jsonl；**加：能追 compact 记录**驱动 compact 探测）、`gen-idle-tmux.sh`、`daemon-wrapper.sh`、2 个隔离 `CLAUDE_CONFIG_DIR`（模拟两账号）。仿 `e2e/resume-cmd-driver.ts` 建 `e2e/restart-cmd-driver.ts`（import 真 `restartWithAccount`/编排，打印序列+argv）。

## 与 UX 审计的接点（固化为回归护栏，非修 bug）
审计 #3：对齐/换号成功后 ~10s mismatch 假阳性（`sessionAccountsByS` 只 10s 轮询刷新、对齐后没立即重探）→ **F-E3 可加一条断言钉住"restart 后 mismatch 源何时刷新"的当前行为**，把这个已知边界固化（修不修由用户定，测试只如实记录现状）。

## 步骤
1. sync worktree 到 account-ux（`git merge --ff-only account-ux`）；读本文件 + MASTERPLAN F-E3 矩阵。
2. 扩 fixture（compact 记录）+ 建 restart-cmd-driver.ts。
3. 建 `e2e/restart-suite.sh`（命令级）+ 必要时 `e2e/restart-daemon-frames.sh`（帧级），逐边界断言。
4. 实跑，回盘核实真结果；tsc+vitest 绿。

## 审计结果 / 签收
（D 代码审计 / E 工程审计 / F 主计划更新 —— 待填）
