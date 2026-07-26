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
- [~] **步骤 2（灰灯，甲-evented 事件驱动）Phase B 设计已定（聚焦设计 agent + aya 实测承重假设）**：
  ### 机制定案（候选 ii：emitter 收 removed 时查 tmux 快照；已证可行、无阻断硬伤）
  - **(a) idle 在哪算 = emitter**（`lib.rs:590-606` removed 臂）。每个 removed sid：`find_tmux_origin_for_sid(sid)` → Some(origin)→**IDLE**（`mark_idle` + emit `SESSION_IDLE` + **不 forget** 绑定）；None→**ARCHIVED**（`clear_idle` + forget + `SESSION_ENDED`，原逻辑）。落 emitter 因红线①（ssh_source removed/flush 臂只 send、唯一 emit=emitter）+ 红线②（现 reconcile `retain(announced_live)` 已不认识退出的 sid）。
  - **★判据 = command-agnostic（有据偏离 ledger 的「command≠claude」）**：`TmuxSessions` 帧最长 8s 陈旧，退出瞬间 command 可能仍是 claude → 卡 command≠claude 会正常退出高频误判 archived。改用：「claude 没了」用 **daemon-removed（权威边沿）**判、「tmux 在」用 **@ccm_sid present in 帧** 判（claude 退出后 wrapper watcher 停写但不 unset，@ccm_sid 恒 present——aya 已实测 `ccm-wrapper.sh` + session 级 option + 登录 shell 存活）。**代码注释 + §24 显式写清此取舍。**
  - **(b) 覆盖/去抖**：emitter 单线程 FIFO 消费，同一 sid removed 顺序处理，idle/archived 互斥，无并发竞争。daemon-removed 是 idle **唯一触发边沿**；tmux 帧**永不产 idle**（只收割）。
  - **(c) idle→archived 产出者 = 收帧驱动收割器**（`ssh_source.rs:2257` TmuxSessions 臂，**替掉已删的 8s poller** = 甲-evented）：存 raw 后 `tracked = announced.keys() ∪ snapshot_idle_for_origin(host)`；`reconcile_step(&state, &tracked, &backend, THRESHOLD)` → retire → `session_changes.send(removed)` → emitter 重判（缺 backend→None→archived）。**reconcile_step 零改**，只把 announced 换成 announced∪idle。灰延迟 ≈ 8s×2 ≈ 16s（同旧 poller 量级）。
  - **(d) SessionChange 加字段？= 不加**。候选 ii 只需 `Vec<String> removed`；新增独立全局账本 `REMOTE_IDLE`(origin→idle sids)，`session_map.rs:33` 一字不动、9 构造点免回填。
  - **(e) 复活竞态**：idle 边沿单一（只 daemon-removed 在 emitter 产），帧只收割永不产 idle → 陈旧 @ccm_sid 不复活 archived。archived 后 IDLE 已清、sid 离 announced → 收割器 tracked 不含它。
  ### 逐文件改动（file:line，实现照此）
  - **bridge.rs**：`SESSION_IDLE="session-idle"` 常量 + `SessionIdlePayload{session_id}`。
  - **ssh_source.rs**：`REMOTE_IDLE` 静态(origin→HashSet<sid>) + `mark_idle/clear_idle/snapshot_idle_for_origin/snapshot_idle_by_origin`（**唯一写者=emitter**）；纯函数 `tmux_origin_for_sid(by_origin, sid)`(逐 origin parse_tmux_ls 找 @ccm_sid==sid) + `find_tmux_origin_for_sid` 包装；stream_loop `loop` 前 `let mut reconcile_state`；TmuxSessions 臂加收割块；断连 run() 把 `snapshot_idle_for_origin`(只读) 并入 removed；删 `snapshot_announced_by_origin`。
  - **tmux_reconcile.rs**：删 `POLL_INTERVAL`+`run_tmux_reconcile_poller`(8s)；保留 `reconcile_step`/`ReconcileState`/`RETIRE_MISS_THRESHOLD`(assert≥2)+纯函数测；模块头改「收帧驱动」。
  - **lib.rs**：emitter added 臂 `clear_idle`；removed 臂改 (a) 分流；删 poller spawn `:664-666`；F5 对账 `:783-790` 排除 idle sid + 重 emit SESSION_IDLE。
  - **前端 tabs.ts**：`Tab.tmuxIdle:boolean`（**不改 TabStatus 枚举**）+ `pendingTmuxIdle` + `markTmuxIdle` + archiveTab/updateActivity/reviveTab/ensureTab 复活处清灰 + `updateTabButton` toggle `tmux-idle` class(且 status!==archived)。**events.ts** `session-idle` 进**同一 queue**与行同序 + `onSessionIdle`。**main.ts** `onSessionIdle:(sid)=>tabs.markTmuxIdle(sid)`。**styles.css** `.tab.tmux-idle .live-dot{灰}`。
  - **INVARIANTS §24**：补 F03.2 段（idle 是 remote_active 之外第三态、REMOTE_IDLE 唯一写者=emitter、idle 边沿单一、F5 idle 对称）。
  ### §24 不变量保全：remote_active 唯一写者/写点零新增；idle 只写 REMOTE_IDLE(唯一写者 emitter)；收割器/断连/daemonless 只经 remote_tx send removed，不直写 remote_active。
  ### 实现拆分：**F03.2a=Rust 后端**(bridge/ssh_source/tmux_reconcile/lib，cargo 可验)先 → **F03.2b=前端**(tabs/events/main/css)→ 合并全视角 D 审计。
  - [ ] F03.2a 实现（下一轮）· [ ] F03.2b 实现 · [ ] 全视角 D 审计
- [x] **步骤 3（attach-into-idle）**：抽 `findIdleTmux(sessions,sid)`（@ccm_sid 命中 + command≠claude，与 findClaudeTmux 互斥）；F03.1 就地复用改用它（去重）；attach 菜单同步(缓存命中)+异步(resolveAttachMenuItem)两路在无活 claude 但有 idle 时提供「Attach（空 tmux …）」。回归测（findIdleTmux 5 例 + DOM idle-attach 1 例）+ 变异验证。**§4 重排下先于步骤 2 做（不依赖灰灯机制）**。
- [~] **步骤 4（rbind 标题 + 拉起即绑，#74/#41）= 甲′ + 丙**：
  - [x] **4a 甲′（远端，aya 已验）**：`createRunAttach` create 分支（ccmSid 存在时）非阻断加 `set-titles on` + `set-titles-string ccm-rbind-#{@ccm_sid}`（**裸值无双引号**——launch.rs 拒双引号）。从 @ccm_sid 派生外层标题、claude 覆盖不了、无轮询。aya 实测：claude 抢 pane_title 后外层标题仍渲染 `ccm-rbind-<sid>`。测试更新 session-backend.test + remote-launch.test 精确串。
  - [ ] **4b 丙（本地 Windows，你真机验）**：`launch_remote_terminal` 加 sid 参 → wt `-w new --title ccm-rbind-<sid> --suppressApplicationTitle` → spawn 后 `RemoteHwndCache.try_bind_with_retry` 前向登记 + shrink `ON_DEMAND_BIND_ATTEMPTS`。Windows 侧 aya 验不了 → 真机验证再关 #74/#41。

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
