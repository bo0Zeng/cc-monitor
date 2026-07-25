# F03 — 统一 tmux 管线 + 三态会话生命周期

> 用户 2026-07-25 批准的**三态模型**（在 live/archived 外加 idle-tmux）。治 #76 根因（孤儿堆积）+ #75 一条
> （create-gate 短路把用户 attach 进没起 claude 的空 shell）+ 填「claude 退了但 tmux 还在」的显示空白。

## 三态模型（用户拍板）
| 态 | 定义 | 灯 | tab |
|---|---|---|---|
| live | claude 在跑 | 绿/黄/红(活动) | 正常 |
| **idle-tmux(新)** | tmux `cc-<sid8>` 在(@ccm_sid=X)但 command≠claude(空 shell) | **灰灯** | 正常(不灰掉,靠灯区分) |
| archived | tmux 也没了 | 灭 | 灰掉(现状) |
转移：`live →(claude 退,tmux 留)→ idle-tmux →(tmux 也没)→ archived`；`idle-tmux →(就地 resume)→ live`。

## 用户问的两点（已核实）
- **id 能保留**：tmux 会话名 `cc-<sid8>` + `@ccm_sid` option 是 session 级，claude 退出后照样在 → cc-monitor 认得出是 sid X 的空 tmux。
- **还能拉起**：往空 tmux send-keys `claude --resume <sid>` 就地复活（JSONL 没删），复用同名 = 不产孤儿。

## DoD（分步）
- [x] **步骤 1（idle-tmux 就地复用 resume）**：resumeTabTmux 在 ①live-attach 与 ②new-session 之间加 ①.5——@ccm_sid 精确命中但 command≠claude → 复用原名 send-keys resume（`runRemoteResumeIntoExistingTmux`），不 pickFreshTmuxName 新起。回归测 + 变异验证。
- [ ] **步骤 2（灰灯 UI）**：把 idle-tmux 态接到活动灯的一个灰状态（经 reconcile / tmux 存活信号）。
- [x] **步骤 3（attach-into-idle）**：抽 `findIdleTmux(sessions,sid)`（@ccm_sid 命中 + command≠claude，与 findClaudeTmux 互斥）；F03.1 就地复用改用它（去重）；attach 菜单同步(缓存命中)+异步(resolveAttachMenuItem)两路在无活 claude 但有 idle 时提供「Attach（空 tmux …）」。回归测（findIdleTmux 5 例 + DOM idle-attach 1 例）+ 变异验证。**§4 重排下先于步骤 2 做（不依赖灰灯机制）**。
- [ ] **步骤 4（identity 建时设 rbind 标题，#74/#41 结构因）**：create 序列直接写 `ccm-rbind-<sid>` 标题（不依赖交互 __ccm_rbind）。

## 不做（防蔓延）
- 不自动 kill 空 tmux（用户拍板：不自动回收；真孤儿靠 F05 手动）。
- 不改 daemon（idle 检测用现成 TMUX_LS_FMT 的 @ccm_sid+command）；不改 TMUX_LS_FMT 双写点。
- 不改 create-gate 新会话路径的载荷（保持逐字节，避免回归）。

## 与主计划对接
- 共享面「remote-launch.ts + session-backend.ts 载荷/命名/身份」→ 落 `buildResumePayload`（单一载荷源）、`buildResumeIntoExistingTmuxCmd`、`SessionBackend.runInExistingAttach`。账本最终形态：一套 tmux 载荷/命名逻辑，create 与 reuse 共用 payload。
- 共享面「resume/起会话入口 UI」→ 步骤 3 的 attach-idle 与 F04 的后端×账号矩阵协调。

## 实现步骤
1. ✅ session-backend.ts 加 `runInExistingAttach`（send-keys+attach，无 new-session）；remote-launch.ts 抽 `buildResumePayload` + 加 `buildResumeIntoExistingTmuxCmd`（基座前置 `unset CLAUDE_CONFIG_DIR` 清残留 env）；remote-launch-run.ts 加 `runRemoteResumeIntoExistingTmux`（boolean+剪贴板回退）；tabs.ts resumeTabTmux 加 ①.5 idle 分支。
2. ⏳ 灰灯 UI（reconcile → 活动灯灰状态）。
3. ⏳ attach-into-idle。
4. ⏳ rbind 标题建时设。

## 测试策略
- 纯函数（tsx remote-launch.test.ts）：buildResumeIntoExistingTmuxCmd 基座/账号/复用传入名/非法拒。
- DOM（tabs.vitest）：resumeTabTmux 三分支（idle→复用 / live→attach / 无→新起）+ 变异（破坏 idle 检测→红）。
- 回归纪律：先证 #76 复用行为，变异锚点删 idle 分支→回落新起→红。

## 审计结果
- **代码审计(D)**（中风险主线程自审 + LSP 级核对）：新载荷经 posixQuote/name 校验、复用 `buildResumePayload` 单一源（create 版逐字节不变，remote-launch.test.ts 既有断言未破）；idle 判据仅 @ccm_sid 精确命中（不按 cwd 猜，免撞漂移会话）；基座复用前置 unset CLAUDE_CONFIG_DIR 防旧账号残留（#75 复用变体）；无 daemon/双写点/bashrc 触碰。
- **工程审计(E)**：`buildResumePayload` 收敛载荷防 create/reuse 漂移（账本一致）；runInExistingAttach 走后端座（不硬编码 tmux）；主计划自洽；tsc 0 / npm test 573 / build ✓。

## 签收
- [x] 步骤 1（idle 就地复用）过 D+E（rev05）
- [x] **步骤 3（attach-into-idle）过 D+E**（低风险主线程自审）：`findIdleTmux` 纯函数、只按 @ccm_sid 精确命中（不按 cwd 猜）；两路 attach（同步缓存 + 异步）一致；F03.1 内联去重复用同一函数（账本"单一判据"最终形态）；空 tmux 不提供 preview/kill（无 claude 画面）；无 daemon/双写点/bashrc 触碰。tsc 0 / npm test 579 / build ✓。
- [x] 主计划已更新（rev 09；§4 重排 F03.3 先于 F03.2）
- [ ] 步骤 2（灰灯，机制留 Phase B）/ 步骤 4（rbind 标题）未做
