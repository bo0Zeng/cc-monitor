# F-E2 — resume idle 就地复用 e2e（#75/#76）

> Linux 自环（aya/xvfb + loopback SSH + F-E1 fixture）。分支 account-ux。
> 边界矩阵见 MASTERPLAN「★功能边界矩阵 › F-E2」。红线：daemon 零改·不 push/bump·无 emoji。

## 目标
端到端验 resume 就地复用：远端 archived/idle-tmux 会话 resume 时**复用原会话名 `cc-<sid8>`、不产 `cc-<sid8>-N` 孤儿**（治 #76），且账号注入正确的 `CLAUDE_CONFIG_DIR`（治 #75）。跨进程整链，单测碰不到。

## DoD（边界，逐条实跑断言，如实回报到哪一层）
- [ ] 远端 archived + idle-tmux(灰) → Resume（tmux）：argv.log 里命令含 resume + **复用 `cc-<sid8>` 名（无 `-N`）**；`tmux has-session` 孤儿数=0；复活清灰（`[e2e] tab-state ... archived→live` / tmuxIdle 0）。
- [ ] 远端 archived（无 tmux）→ Resume（直连）：新 session，argv `CLAUDE_CONFIG_DIR` = 目标账号目录。
- [ ] 带账号 pin「用账号 X resume」：argv `CLAUDE_CONFIG_DIR` = X 的目录（走 account-restart.ts 现成 opts.confirm 范式，DEV 自动接受）。
- [ ] 不带 pin：默认全局当前账号目录（#75 主因）。
- [ ] 本地 archived Resume：archived→live 复活。
- [ ] 边界：重复 resume 幂等 / tmux 已消失回退 / 会话仍 live 时守卫不误动。
- [ ] 门禁：tsc 0 + vitest 不回归；不破 f40-suite；daemon 零改。

## Fixture（复用 F-E1，别重造）
`e2e/fake-claude`（记 argv+env→argv.log、写 pidfile、追 jsonl、sleep）、`e2e/gen-idle-tmux.sh`（`tmux new -d -s cc-<x8>` + `@ccm_sid`）、`e2e/daemon-wrapper.sh`（`CLAUDE_CONFIG_DIR=/tmp/e2e-remote-claude` 隔离）、loopback SSH 到本机。多账号边界：造 2 个隔离 `CLAUDE_CONFIG_DIR`（模拟两账号）。

## 已知难点（如实报到哪一层）
GUI 触发 resume 在 headless Linux 是难点：XTEST 键盘进不了 webview（f40 前车），右键菜单→选项需鼠标 geometry click（仿 f40）触发；实在够不着可加一个 DEV 中键/probe 触发 resume，或退**命令级/argv 级**验证（直接驱 resume 路径断言 argv.log + 无孤儿）。**分层如实回报：全链 GUI / 命令级 / 退到哪层都要说清，别假装全链。**

## 步骤
1. sync worktree 到 account-ux；读本文件 + MASTERPLAN F-E2 矩阵。
2. 复用/扩 F-E1 fixture（加多账号目录）。
3. 建 `e2e/resume-suite.sh`（仿 graylight-suite.sh），逐边界跑 + 断言 argv.log / `tmux has-session` 孤儿数 / `[e2e] tab-state`。
4. 实跑（Linux 自环），回盘核实真结果。
5. 门禁 tsc+vitest 绿。

## 审计结果 / 签收
（D 代码审计 / E 工程审计 / F 主计划更新 —— 待填）
