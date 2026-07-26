# 状态 / STATUS — 恢复入口（每次先读这里）

> 工作区 `audit-fixes`。分支 `account-ux`。跨轮靠此文件，不靠记忆。主计划见 MASTERPLAN.md（rev 11）。

## 当前
- **阶段**：F03.2 **Phase B 设计已定并 commit** → 下一步 **F03.2a 实现（Rust 后端）**
- **F03.2 灰灯设计（features/03 步骤2，勿再问机制）**：候选 ii（emitter 收 removed 时 `find_tmux_origin_for_sid` 内联判 idle）+ 收帧驱动收割器复用 reconcile_step（删 8s poller=零轮询）+ **command-agnostic 判据**（claude 死用 daemon-removed、tmux 在用 @ccm_sid present，不信 ≤8s 陈旧 command）+ 独立 `REMOTE_IDLE` 账本(唯一写者=emitter,SessionChange 不加字段)。§24 逐条保全已论证。
  - **F03.2a-core 完成**（零行为改动、cargo 绿）：bridge.rs SESSION_IDLE 常量+SessionIdlePayload；ssh_source REMOTE_IDLE 账本 + mark/clear/snapshot_idle_* + `tmux_origin_for_sid` 纯函数（command-agnostic）+ find_tmux_origin_for_sid 包装 + 5 Rust 测。**尚无人调=临时,行为不变。**
  - **F03.2a-wire（下一轮，Rust 行为改动）**：lib.rs emitter removed 臂改 `find_tmux_origin_for_sid` 分流(Some→mark_idle+emit SESSION_IDLE+不 forget;None→clear_idle+forget+SESSION_ENDED)、added 臂 clear_idle、删 poller spawn、F5 排除 idle+重发；ssh_source stream_loop TmuxSessions 臂加收帧收割器(reconcile_state+tracked=announced∪idle+reconcile_step→send removed)、断连并 idle、删 snapshot_announced_by_origin；tmux_reconcile 删 POLL_INTERVAL+poller 保 reconcile_step。cargo fmt/test 绿。
  - **F03.2b（再下轮，前端）**：tabs `tmuxIdle` + markTmuxIdle + 清灰生命周期 + `tmux-idle` class；events.ts session-idle 同 queue；main.ts wire；styles.css 灰点。tsc/vitest。
  - **合并全视角 D 审计**（正确性/§24 单写者不变量/计划符合度）后签收。
  - F06：#43「父子拉不起来」残留（代码可复现则修，否则归真机）；#60/#43/#63-attach F74* 真机验证归"你侧待办"；#60① 靠本步事件驱动改善、真机验。
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
