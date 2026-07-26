# 状态 / STATUS — 恢复入口（每次先读这里）

> 工作区 `audit-fixes`。分支 `account-ux`。跨轮靠此文件，不靠记忆。主计划见 MASTERPLAN.md（rev 11）。

## 当前
- **阶段**：**F09 测试补齐完成**（B→F 过，低风险自审）→ 下一步 **F10/F11 文档**
  - F09（本轮）：① code-picture-core 25 测进 CI（rust job 加 `-p`，不动 vendor）；② e2e 脚本健康冒烟（新 e2e-smoke ubuntu job：shellcheck --severity=error + py_compile；真 e2e 需 Xvfb+app+xdotool→大投入低 ROI，待 v2/真机）；③ main.ts:989 内联 basename 盲区去重到已测 `sftp/paths.basename`（panorama 第三份留 F12）。tsc0/npm595/code-picture-core25/build✓。
  - **覆盖率地板未变**（F09 补 Rust 测 + 路由到已覆盖代码，vitest 覆盖不涨）→ 收紧 F08b 地板仍待更多 TS 侧补测。
  - **F10/F11（下轮，文档）**：F10 README 中英修版本/删悬空/补账号 + RELEASING/CONTRIBUTING；F11 文档漂移（ARCHITECTURE 账号子系统 + STATE-MATRIX 4命令 + INVARIANTS color-scheme + 子 README + 索引 + actions 数）。
- **F08 质量门禁**：F08a 红线护栏 + F08b lint/coverage 全完成。**F03.2（灰灯）**：全链闭环。F06 无 aya-代码。
  - 提交链：a-core(0934e7d)→a-wire(0451065)→b 前端(d00703c)→**D 审计修(a487d2c)**；文档 F 回看（INVARIANTS §24bis + MASTERPLAN rev12 + features/03 签收）待本轮 commit。
  - **D 审计（3 并行 agent）零阻塞**：§24 单写者/全红线/机制符合度确认。修了：① ever_bound×idle 卡灰竞态（reconcile_step 加 pre_bound 播种）② 远端复活不清灰（ensureTab 主清灰）③ emitter 分流零测（classify_removed 纯枚举）④ grid 灰点 DOM 无测。4 处均变异验证。
  - **残留记档**（非阻塞、真机/后续）：TOCTOU 短命会话误归档=非回归、session-activity 误清极窄竞态=自愈、带外杀端到端变灰+RETIRE_MISS_THRESHOLD 标定=真机。
- **F06 Phase B 结论（无 aya-代码可做）**：#43 机制（父恒绿/分裂父子）已在本分支修好三处**且已测**——backend `scan_dir` 归并（`scan_dir_same_sid_interactive_wins_over_bg` + `_same_kind_newer_wins`，覆盖 kind_rank/newer_than）+ frontend interactive 升格/降格（tabs.vitest:793/811）。真残留「父子拉不起来 / Ctrl-X 未合并」= **须真机复现**（aya 驱动不了 Windows GUI 起会话流），归你侧待办。F06 无新实现，不走 D/E。
- **下一步 = F08**（质量门禁：eslint/prettier/stylelint/覆盖率棘轮 + `TMUX_LS_FMT` 双写点 CI 断言 + daemon 只读护栏——真代码工作）→ F09-F13 → Phase G。
- **F03.2 灰灯设计（features/03 步骤2，勿再问机制）**：候选 ii（emitter 收 removed 时 `find_tmux_origin_for_sid` 内联判 idle）+ 收帧驱动收割器复用 reconcile_step（删 8s poller=零轮询）+ **command-agnostic 判据**（claude 死用 daemon-removed、tmux 在用 @ccm_sid present，不信 ≤8s 陈旧 command）+ 独立 `REMOTE_IDLE` 账本(唯一写者=emitter,SessionChange 不加字段)。§24 逐条保全已论证。
  - **F03.2a-core 完成**（零行为改动、cargo 绿）：bridge.rs SESSION_IDLE 常量+SessionIdlePayload；ssh_source REMOTE_IDLE 账本 + mark/clear/snapshot_idle_* + `tmux_origin_for_sid` 纯函数（command-agnostic）+ find_tmux_origin_for_sid 包装 + 5 Rust 测。**尚无人调=临时,行为不变。**
  - **F03.2a-wire（下一轮，Rust 行为改动）**：lib.rs emitter removed 臂改 `find_tmux_origin_for_sid` 分流(Some→mark_idle+emit SESSION_IDLE+不 forget;None→clear_idle+forget+SESSION_ENDED)、added 臂 clear_idle、删 poller spawn、F5 排除 idle+重发；ssh_source stream_loop TmuxSessions 臂加收帧收割器(reconcile_state+tracked=announced∪idle+reconcile_step→send removed)、断连并 idle、删 snapshot_announced_by_origin；tmux_reconcile 删 POLL_INTERVAL+poller 保 reconcile_step。cargo fmt/test 绿。
  - **F03.2b（再下轮，前端）**：tabs `tmuxIdle` + markTmuxIdle + 清灰生命周期 + `tmux-idle` class；events.ts session-idle 同 queue；main.ts wire；styles.css 灰点。tsc/vitest。
  - **合并全视角 D 审计**（正确性/§24 单写者不变量/计划符合度）后签收。
  - F06：#43「父子拉不起来」残留（代码可复现则修，否则归真机）；#60/#43/#63-attach F74* 真机验证归"你侧待办"；#60① 靠本步事件驱动改善、真机验。
- **之后**：F08→F09→F10→F11→F12→F13 → Phase G。

## 已完成（commit）
- F01 账号安全(75594ff/3221f26) · F02 kill 白名单(e389410) · F03.1 idle 复用(537077b) · F03.3 attach-idle(5fd77b8) · F03.4a 甲′(85f1a0d) · F04(5494293) · F05(14dff16) · F07(8aac094)。
- **F03.2 灰灯 idle-tmux 三态（全链过 D+E+F）**：a-core(0934e7d)+a-wire(0451065)+b(d00703c)+D审计修(a487d2c)。甲-evented 零轮询、command-agnostic、REMOTE_IDLE 正交单写、pre_bound 消卡灰、ensureTab 主清灰、classify_removed 纯枚举、grid 同源灰点；INVARIANTS §24bis。
- **F08 质量门禁**：F08a 红线护栏（TMUX_LS_FMT 双写点断言 + daemon 只读护栏，cargo 测，027ae89）+ F08b 前端 eslint/stylelint(advisory)+coverage 地板（58172f8）。
- **F09 测试补齐**：code-picture-core 25 测进 CI + e2e 脚本冒烟 + main.ts basename 去重到已测。
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
tsc 0 / npm test 595 / cargo(src-tauri) 365 / daemon cargo 全绿 / build ✓（F08a 后：src-tauri +1=TMUX_LS_FMT 双写点断言；daemon +1=只读护栏）。
