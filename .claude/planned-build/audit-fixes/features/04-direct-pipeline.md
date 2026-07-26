# F04 — 统一直连管线 + keepalive（#75 直连腿）

## 核实结论（先核实再动手）
- **keepalive 是"非 bug"**：直连 resume 走 `wt.exe → powershell -NoExit → & ssh -t … -- 'bash -lic …'`。`launch.rs:163` 的 **`-NoExit`** 让 ssh 退出后 **PowerShell 窗口保留在提示符**（注释：「命令退出后窗口保留（可读错误）」）→ claude 非零退出**不闪退**、错误留在 scrollback 可见。根因报告 Agent B 的"闪退"结论**未计入 -NoExit，是误诊**。→ **不加 keepalive**（不修不存在的 bug）。
- **#75 直连腿的真因 = 错账号注入**，已由 **F01**（基座逃生口 + pin 现读）处理；tmux create-gate 空 shell 由 **F03.1**（idle 就地复用）处理。

## DoD
- [x] 核实直连失败行为（-NoExit 保留窗口）→ 明确 keepalive 不需要。
- [x] **tmux 后端基座逃生口**（与直连对称，两后端一致 —— 决策2）：`resumeTabTmux` 加 `useBase`（两处 withAccount follow 都 `useBase ? undefined : 现读pin`）；归档远端 tab 菜单加「用基座 resume（tmux，不隔离）」+ 直连项重标「用基座 resume（直连，不隔离）」。回归测 + 变异验证。
- [x] 两后端选择已存在（归档远端 tab 菜单「Resume（直连）」+「Resume（tmux）」，A4/F52 现成）。

## 不做
- 不加 keepalive（-NoExit 已兜）。
- 不把直连路由进 tmux（直连=无 tmux=天然无身份；要身份走 tmux 版）。
- per-account × tmux 组合不做（菜单爆炸；基座逃生口才是 #75 所需）。

## #75 处置
- 账号/生命周期因已修：F01（直连基座+pin 现读）+ F03.1（idle 复用）+ F04（tmux 基座）。keepalive 非-bug。
- **#75 留开**：resume 真拉起来是真机现象，代码侧账号安全已完 → 真机验证后与 F01 合并关。

## 审计
- **D**（低风险主线程自审）：useBase additive、对称直连版；两处 follow 一致改（replace_all）；菜单加项不改既有；无 daemon/双写点/bashrc/轮询。tsc 0 / npm test 580 / build ✓。
- **E**：两后端基座对称，账本「起会话入口 UI」最终形态更齐；主计划自洽。

## 签收
- [x] 过 D + E
- [x] 主计划已更新
- [x] F04 完成
