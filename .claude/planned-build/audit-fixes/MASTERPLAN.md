# 主计划 / MASTERPLAN — audit-fixes（rev 11 全面制定：bug 全清 + 测试/文档/重构）

> 单一事实来源。动因：`项目审阅报告-2026-07-25.md`（full-audit）+ `issue-bug根因报告-2026-07-25.md`（8 issue 根因）。
> 本版把跨多轮的全部决策收敛成一份连贯计划（旧增量见 §7 变更记录）。

## 0. 目标 / 范围 / 红线
- **总体目标**：修完所有 OPEN bug（会话/生命周期 cluster + full-audit 的 I4/I5），补 TS/CSS 测试与门禁，修文档漂移，低风险重构；让 `account-ux` 分支可干净合入。做完每个 bug 的**代码能确证部分**即关其 GitHub issue。
- **范围内**：F03 剩余（3.2 灰灯 / 3.4 标题+绑定）· F04 · F05 · F06 · F07（bug）+ F08–F13（测试/文档/重构）。
- **红线（每轮核对）**：
  1. **daemon（`remote-daemon-proto/`）零行为改动** —— 只准加只读测试/门禁；灰灯/标题都只用 daemon 现成的 `TmuxSessions` 帧（已带 `@ccm_sid`+`pane_current_command`）。
  2. **不改 `TMUX_LS_FMT` 双写点**（`src-tauri/src/tmux.rs` ↔ `remote-daemon-proto/src/watcher.rs`）任一侧列。
  3. **不碰用户 `~/.bashrc`** —— F14（zcc/bcc 迁移 + wrapper 去轮询）单列、用户自跑，我只给脚本/清单。
  4. 不改 `cc-<sid8>` 会话名协议既有语义；**不 push / 不发版 / 不 bump 版本**；孤儿**不自动 kill**（仅手动 F05）。
  5. **不新增 cc-monitor 侧轮询**（用户 2026-07-25 拍板"不要轮询"）——状态判定尽量"收 daemon 推帧/事件即算"；唯一周期扫描 = daemon 内部 `tmux ls`（红线外、既有、物理上绕不开）。
- **成功标准**：8 open bug 全处理（修/明确转真机/转 F14）+ 对应 issue 关；tsc 0 / npm test 全绿 / cargo test（含 vendor）全绿 / build ✓ / 覆盖率门禁过；每个 bug 修复有回归测 + 变异验证。

## 1. 功能清单
| ID | 功能 | 对应 bug/审计 | 状态 |
|----|------|--------------|------|
| F01 | follow-resume 账号安全 + resume 选账号（pin 现读磁盘 + 基座逃生口）| B1 + #75(部分) | ✅ 完成 |
| F02 | `kill_remote_tmux` 补 `is_ccm_tmux_name` 白名单 | I1 | ✅ 完成 |
| F03.1 | idle-tmux 就地复用 resume（复用原名不产孤儿）| #76 根因 | ✅ 完成 |
| F03.3 | attach-into-idle（`findIdleTmux` + 两路 attach 菜单）| #76 | ✅ 完成 |
| **F03.2** | **idle-tmux 灰灯（三态，事件驱动）** | #76 显示 + #60 邻域 | ✅ 完成（全链 D+E+F，见 rev12） |
| **F03.4** | **rbind 标题 + 拉起即绑**：甲′ ✅（远端从 @ccm_sid 派生标题，aya 验通、无轮询——**主体修 #74**）；**丙 延期 Windows 真机批次**（aya 无 Windows cross-target，cfg(windows) 核心连编译都验不了 → 不盲提交；等有 Windows 环境边写边编边验）| #74 #41 #72残留 | 🟡 甲′完成/丙延期 |
| F04 | 统一直连管线（keepalive **核实为非-bug**：-NoExit 已保留窗口）+ tmux 后端基座逃生口（两后端对称） | #75 直连腿 | ✅ 完成 |
| F05 | 手动清理真孤儿（仅 `cc-*` 无对应 tab，过 `is_ccm_tmux_name`）| #76 残留 | ✅ 完成（14dff16） |
| F06 | #43「父子拉不起来」残留（代码）+ #60/#43/#63attach 的 F74* 真机验证 | #43 #60 #63(部分) | 🟡 无 aya-代码：机制(父恒绿/分裂)已修+已测；残留(拉不起来/Ctrl-X)真机 |
| F07 | 刷新竞态(I4 序号门) + 多远端缓存(I5 切号清全 origin) + resumeTab onUnselectable | I4 I5 | ✅ 完成 |
| F08 | 质量门禁（eslint/prettier/stylelint/覆盖率棘轮/mock 卫生）+ `TMUX_LS_FMT` 双写点 CI 断言 + daemon 只读机器护栏 | I8 I7 | ✅ 完成（F08a 两护栏=cargo 测 + F08b eslint/stylelint advisory + coverage 地板；prettier 不做避 churn） |
| F09 | 测试补齐（main.ts 盲区可测纯函数 + vendor code-picture-core 进 cargo test + e2e 冒烟进 CI）| I8/G3 | ✅ 完成（code-picture-core 25 测进 CI + e2e 脚本健康冒烟 + main.ts basename 去重到已测；真 e2e 待 v2/真机） |
| F10 | README 中英修版本/删悬空/补账号 + RELEASING/CONTRIBUTING checklist 补 README 两条 | I2 I3 | ✅ 完成（7bf412a：版本 3.2.0 + CI 四 job 同步 + 多账号小节 + RELEASING 链接） |
| F11 | 文档漂移（ARCHITECTURE 账号子系统 + STATE-MATRIX 4命令 + INVARIANTS 上移 color-scheme + 子README + 索引 + actions 数）| I7docs/G2 | ✅ 完成：ARCHITECTURE 账号子系统 + 双子 README + INVARIANTS §32 暗色事实 + README action 26→28/加G + planned-build 索引；**STATE-MATRIX §2 = 审计过标不动**（账号命令 stateless）；**移草案不做**（草案已实现=历史设计文档非 proposal） |
| F12 | `remote-section.ts` 数据层抽 `remote-config.ts`（治分层倒挂）| I6/G3 | ✅ 完成：数据层→`src/remote-config.ts`（180 行）、8 importer 迁移、行为等价（tsc0/npm595/build0）、无环；remote-section 1801→1640 |
| F13 | 脊柱拆分（tabs 抽 AccountBadgeController；评估 ssh_source）| I6 | ❌ **评估后不做（dispose）**：拆分由「具体架构病」证成非行数；两 god file 拆分负收益 + 引入 §24/可见性/测试分区风险 → 维持现状。唯一真架构病 F12 已修。评估留档 `spine-split/MASTERPLAN.md` |
| ~~F14~~ | ~~.bashrc zcc/bcc 迁移 + wrapper 去轮询（inotify 事件驱动版）~~ | — | **用户自跑** |

**已关 issue**：#71 #42 #67 #46（v3.2.0 已修，full-audit 复核后关）。**剩余 open bug** = [41,43,60,63,72,74,75,76] 全在计划。

## 2. 架构概览
- **三层**：TS 前端 `src/` ↔ Tauri Rust `src-tauri/` ↔ 只读 daemon `remote-daemon-proto/`（本族零行为改动）。
- **账号解析瀑布（勿破）**：`resume 显式选号 > 会话 lastAccount(pin 现读磁盘) > 全局当前工作账号 > 基座`。
- **会话三态（用户拍板）**：`live(claude 跑,活动灯) / idle-tmux(tmux 在但 command≠claude,灰灯) / archived(tmux 没了,灰掉)`。
- **live-state 单写者（INVARIANTS §24）**：`remote_active` 唯一写者 = remote-session-emitter（`lib.rs:590-606` 是归档唯一执行点），收 `SessionChange` 通道（daemon 事件 / 断连 flush / tmux 帧对账）。
- **tmux 状态来源**：daemon 内部周期 `tmux ls` → 推 `TmuxSessions` 帧给 cc-monitor（`watcher.rs`）；cc-monitor **不再 SSH 查**（B2 起读 `snapshot_tmux_by_origin`）。**灰灯据此做成"收帧即算"的事件驱动，不加 cc-monitor 侧轮询**（见 §3 F03.2）。
- **窗口绑定（↗ 拉前）**：本地 PS 会话已是"拉起即绑"（PS 上报 PID→HWND）；远端曾退化成"事后扫可覆写标题"→ #74/#41。F03.4 把远端也拉回"拉起即绑 + 不可覆写标题"（见 §3 F03.4）。

## 3. ★共享面账本（rev 11，含已定最终形态）
| 共享面 | 涉及功能 | 最终形态 | 状态 |
|--------|----------|----------|------|
| `src/tabs.ts` pin 读取 | F01,F13 | `readSessionPin` 现读磁盘，三处 resume 一致 | ✅ F01 |
| `src/tabs.ts` `findIdleTmux`/`findClaudeTmux`（@ccm_sid 精确匹配） | F03.1,F03.3,F03.2,F06 | idle 判据（@ccm_sid 命中+command≠claude）单一函数，就地复用/attach-idle/灰灯/#60 共用 | ✅ F03.1/3.3 落 findIdleTmux |
| `src/remote-launch.ts`+`session-backend.ts` 载荷/命名/身份 | F03,F04 | create/reuse 共用 `buildResumePayload`；reuse 走 `runInExistingAttach`；**F03.4-甲′**：createRunAttach create 分支非阻断加 `(set-option -t <t> set-titles on||true) && (set-option -t <t> set-titles-string ccm-rbind-#{@ccm_sid}||true)`（**裸值无双引号**——launch.rs 拒双引号；从 @ccm_sid 派生、claude 覆盖不了、无轮询，aya 已验） | 🟡 F03.1 落 create/reuse；F03.4 加甲′ |
| **`src-tauri/src/launch.rs` + `bind.rs`（RemoteHwndCache）+ IPC** | **F03.4-丙** | **拉起即绑**：`launch_remote_terminal` 加 `sid` 参 → wt 起 `-w new new-tab --title ccm-rbind-<sid> --suppressApplicationTitle`（本地钉标题、claude 覆盖不了）→ spawn 后立即 `RemoteHwndCache.try_bind_with_retry`（现成，零新增）前向登记 sid→HWND；`ON_DEMAND_BIND_ATTEMPTS` 40→个位、eager 9s 扫可删（拉起即绑无需等四跳）。**Windows 侧行为 aya 验不了 → 用户真机验证再关 #74/#41** | ⬜ F03.4；老 WT/Plan B 回退退化现行为不阻断 |
| **`SessionChange`(session_map.rs) + emitter(§24) + tmux 帧收帧处(`ssh_source.rs` stream_loop) + tabs 渲染** | **F03.2** | **灰灯 = 事件驱动（甲-evented）**：把 live/idle/archived 判定从 8s 定时器**挪进"收到 daemon `TmuxSessions` 帧"处**（帧到即算），**删 cc-monitor 侧 `POLL_INTERVAL` 定时器**。idle=@ccm_sid 命中+command≠claude；archived=连续 N 帧不见（retire 阈值 ≥2 映射为连续帧）；live→idle 借 daemon SessionRemoved 事件 + 帧内 command。emitter 处理 idle**不进 remote_active**、发新前端事件；tabs 灰灯 class（不改 TabStatus 枚举）；**同轮更 INVARIANTS §24**。边界：daemonless 不发帧→恒 archived 不进 idle | 🟡 **F03.2 Phase B 设计定**（聚焦设计 agent + aya 实测）：机制=**候选 ii**（emitter 收 removed 时 `find_tmux_origin_for_sid` 内联判 idle；**SessionChange 不加字段**）+ **收帧驱动收割器复用 reconcile_step**（删 8s poller=零轮询）+ **command-agnostic 判据**（claude 死用 daemon-removed、tmux 在用 @ccm_sid present——**超越本行上文的「command≠claude」**：帧 ≤8s 陈旧、退出瞬间 command 可能仍 claude 会误判 archived，注释+§24 记明）。详见 features/03 步骤2。拆 F03.2a(Rust,cargo 可验)+F03.2b(前端)→合并全视角 D 审计 |
| `src-tauri/src/tmux.rs` | F02,F05 | kill 与 send-keys 对称过 `is_ccm_tmux_name`；F05 清孤儿复用同校验 | ✅ F02 |
| `src/main.ts` refreshSessionAccounts | F07,F09 | in-flight 序号门 + 可测纯函数 | ⬜ F07 |
| `src/settings/remote-section.ts` 数据层 | F12,F13 | 纯数据迁 `remote-config.ts` | ✅ F12：数据层已迁 `src/remote-config.ts`（8 importer 依赖数据模块非 UI 文件）；F13 不再碰 |
| `.github/workflows/ci.yml` + `package.json` | F08,F09,F13 | 终态 job：rust/frontend(+lint+coverage)/daemon/vendor-crate/e2e-smoke + 双写点断言 | ✅ F08+F09 落成全终态：双写点断言+daemon 只读护栏=cargo 测；frontend eslint/stylelint(advisory)+coverage 地板；rust job 加 `-p code-picture-core`（vendor 25 测）；新 e2e-smoke job（shellcheck --severity=error + py_compile） |
| 文档簇 + BACKLOG | F10,F11 | 不漂移；BACKLOG 打删除线 | ✅ F10（README 版本/CI/账号/RELEASING）+ F11（ARCHITECTURE 账号子系统/双子 README/INVARIANTS §32/action 数/索引）；无悬空 |

## 4. 依赖与顺序（bug 优先）
- **bug 段**：F03.4 → F04 → F05 → F07 → **F03.2 + F06（合并，共享 tmux 帧/live-state 面）** → 剩余真机验证项。
  - F03.4 先（甲′ aya 可验、丙 你真机验，修 #74/#41 结构因）；F04 复用 F01 账号选择；F05 复用 F02 白名单；F07 独立小改；**F03.2 灰灯最高风险 + 与 F06(#60) 同改 live-state 面 → 合并最后做、机制反复评估**。
- **工程段**：F08 → F09（门禁先于补测）→ F10 → F11（文档零码风险）→ F12 → F13（重构最后，撞到停）。
- 每个 bug 完成 → 代码能确证的部分 `gh issue close`；需真机/daemon 版本的留开并注明。

## 5. 横切约定
- **回归纪律**：每个 bug 先写复现失败测试 → 修 → 变异验证（改坏看测试是否红）。
- **无轮询原则**：cc-monitor 侧不新增轮询；状态判定收 daemon 推帧/事件即算；标题/绑定用"拉起即绑 + 不可覆写标题"（无 OSC 重刷轮询）。唯一周期扫描 = daemon 内部 `tmux ls`（红线外、既有）。
- **审计强度**：**F03.2（动 emitter/§24 单写者，最高风险）→ Phase B 先开设计 agent 论证机制 + 实现后全视角并行 D 审计**；F03.4/F04/F05/F07 中风险 → 2–3 agent 或聚焦审。
- **测试约定（quality-gates，F08 落地）**：vitest(jsdom)+tsx；eslint(flat)+prettier；stylelint；@vitest/coverage-v8 分支覆盖率棘轮，账号/会话核心模块目标 85%；变异手动/核心模块；CI 云端 GitHub Actions。lint 棘轮基线不追一次清零。
- **门禁纪律**：结果重定向到文件 + Read/grep 核实 + pipefail；build 才抓 CSS 错。

## 6. 风险与真机/外部依赖
- **F03.2 最高风险**：动 `SessionChange`（本地+远端共用）/emitter/§24 单写者，在 #60/#43/#63 历史 bug 高发区。事件驱动改法要处理"收帧即算"与归档的覆盖/去抖、"刚 archived 又被 idle 复活"竞态。撞状态机冲突/计划≠现实 → 停 loop。
- **F03.4-丙 延期 Windows 真机批次**：aya 无 Windows cross-target（只 `x86_64-unknown-linux-gnu`）→ 丙 的 cfg(windows) 核心（wt `--title/--suppressApplicationTitle/-w new` + spawn + 前向登记）**连编译都验不了**，不在 loop 里盲提交；等有 Windows 环境边写边编边验。甲′ 已把 **#74 主体修好**（cc-monitor resume 会话现有可靠标题）；丙 增量 = 消 #41 四跳时序 + 去扫描。#74/#41 留开待 Windows 验。
- **#63 尾消息 torn-tail = daemon-bound**：根因在 `remote-daemon-proto/src/watcher.rs read_new_lines` 扣尾行，修它破红线 → 转发版/真机，本轮 F06 不碰不关。
- **真机/外部前置**（代码改完也不能确证、须用户动作再关）：#74/#41（标题四跳/wt 行为）、#60/#43（真机复现）、#60②/#63-attach（**用户远端重装 ccm 助手写 @ccm_sid** 硬前置）、#75 直连腿（真机看窗口不闪退）。
- F13 ssh_source.rs(4512) 拆分高危 → 可能降级为只做 tabs controller 抽取。

## 7. 变更记录
- 01–05 — 初版→并入 issue 根因+四决策→F01/F02/F03.1 完成、F03 重塑三态（详见 git 历史）
- 06 — 重制：灰灯定方案 A；bug 优先重排；关 #71/#42/#67/#46
- 07 — 第一次计划审计：#63 torn-tail=daemon-bound / F05 只清 cc-* / F03.4 直连归 F04 / F08 加双写点断言+I7护栏
- 08 — 第二次独立复审：F03.2-A idle 产出机制与 reconcile 门控矛盾 → 机制留 Phase B
- 09 — 第三次独立复审：账本把候选(i)专属成本当定死前置 / idle→archived 正向无产出者 → 账本重写
- 10 — 用户拍板：F03.2=甲（高风险→反复评估+审计）；F03.4 开 3 agent 调研更优解
- **11 — 全面制定（本版）**：三 agent 调研收敛 → **F03.4 = 甲′（远端 set-titles-string 从 @ccm_sid 派生，裸值无双引号，aya 已验无轮询）+ 丙（本地 wt --title + suppressApplicationTitle + 拉起即绑 forward-register，根治 #74+#41，Windows 真机验）**；**F03.2 = 甲-evented（收 daemon 帧即算、删 8s 定时器，cc-monitor 侧零轮询）**；确立**无轮询原则**（§5）；F03.4 shrink bind 重试循环；主体全面重写为连贯计划
- **12 — F03.2（灰灯）实现 + 全视角 D 审计闭环**：a-core(0934e7d)→a-wire(0451065)→b 前端(d00703c)→3 并行 D agent（Rust/§24·计划红线·前端）**零阻塞**→D 审计修(a487d2c)。**共享面账本落最终形态**：`tabs.ts`(Tab.tmuxIdle 正交布尔、非 TabStatus 枚举；updateTabButton toggle)、`events.ts`(session-idle 入同 queue，镜像 ended)、`main.ts`(wire) 均按既有 archived/activity 模式实现、**无补丁叠加**。**新增独立面**：`REMOTE_IDLE` 账本（唯一写者=emitter，与 remote_active 正交）+ `reconcile_step` 加 `pre_bound`（消卡灰竞态）+ `classify_removed` 纯枚举。grid-monitor 灰点=同源一致性延伸（防 tab-bar/grid 灯不一致的后续补丁）。INVARIANTS §24bis 补 + F74c 悬空引用修。**工程审计(E)结论**：F03.2 自洽、§24 不破、无拖累后续功能的耦合/技术债（F06/F08-F13 与之正交）；主计划仍自洽。残留均记档（TOCTOU 短命会话误归档=非回归、真机标定项）。cargo 363 / tsc 0 / vitest 595 / build ✓
- **20 — Phase G 最终验收（收官）**：端到端全绿（tsc0/vitest595/src-tauri365/cpc25/daemon125/build✓/CI 4job）；/full-audit 4 并行 agent + 交叉对比 **0 阻塞**（重要项=本轮文档半更新残留，已修 + 1 daemon-bound 已知残留记档）。**Phase G 修**：README 中英数字统一/删悬空 [未发布] 指针/README.en 补 28·G·Ctrl+K·6-unbound/doc 索引补 remote-daemon-proto+修 e2e 行/CONTRIBUTING·RELEASING·DEVELOPMENT 同步/ARCHITECTURE §2 补 Codex 适配/vitest.config 注释改对门禁语义/两审阅报告移入 audit-fixes/**§24bis 单写者机器护栏（cargo 测 remote_idle_single_writer_guard，变异验证过）**/INVARIANTS §24bis 记单写者机器化+空 backend 残留。报告=项目审阅报告-PhaseG-2026-07-26.md。**audit-fixes 计划闭环。**
- **19 — F13 评估→停交回用户**：`tabs.ts`(3178) 账号族边界摸底——**核心纠缠点 `alignableCurrent`** 同读三状态域（账号 5 map / tab 状态 / 重启执行态 restartingSids+onLine compact 回调）、被徽章渲染+不一致查询+重启执行三类共用。可拆部分（账号态+徽章视图，单向 Controller）净值有限（移 ~80 行、增 ~6 getter），而徽章「信息才显」逻辑微妙、自动重构回归风险落最高风险文件。命中主计划 §6「F13 撞到停」+ loop「拆不干净→交回用户，别硬拆」→ **停 loop，未动代码**。ssh_source(4512) 更高危、本轮不拆（记档）。交回用户选①接受现状②交互式拆③状态袋。
- **18 — F12 remote-config 抽层（治分层倒挂）**：把 config 数据层从 1801 行 UI 文件 `settings/remote-section.ts` **逐字节**抽到新 `src/remote-config.ts`（180 行，仅依赖 config、无 UI/DOM、无环）：类型 RemoteHostConfig/RemoteConfig + CRUD read/write/resolveRemoteConfig + coerceHost/findHostByOrigin/sftpEligibleHosts/parseAddressLines/HOST_DEFAULTS。**8 个非 UI importer**（tabs/account-chip/cards/main/port-forward + 摸底漏经 tsc 兜出的 accounts-section/sftp-panel）改依赖数据模块；UI 类逻辑零改（describeStage/sameHost/sameRemote/dirty-check 留 remote-section）。中风险主线程自审 + tsc/595 测/build 三重网证行为等价。remote-section 1801→1640。F13 脊柱拆分不再碰此数据层。
- **17 — F11 doc/ 子系统漂移修**：ARCHITECTURE 补账号子系统（0→11 提及：backend/frontend §2 树 + §5「隔离又同步 A2-A6」小节，含 withAccount vs restart 分离裁定 + 失败语义）；src/README + src-tauri/README 补账号（前端族 + 3 只读命令）；INVARIANTS §32 沉淀「本仓只有暗色主题（color-scheme:dark、无 prefers-color-scheme、TOKENS 15）」；README action 26→28 + 加 G(panorama) + 未绑 2→6（含 Acct）；建 .claude/planned-build/README 索引（7 区）。**修正审计数字**（TOKENS 15≠11、action 28≠26/30）。**⑤ 移草案不做**（两草案已实现=历史设计文档、非未建 proposal、移动会误标+断引用）；**STATE-MATRIX §2 不动**（账号命令 stateless、§2 只收 State 消费者=审计过标）。无悬空链接、无代码改动。
- **16 — F10 README 文档修（+F11 摸底）**：README 中英 v3.0.0→3.2.0（头/脚，无残留；**未 bump 版本**只文档匹配既有 3.2.0）+ CI「三 job」→「四 job」+ eslint/stylelint/coverage/vendor-crate/e2e-smoke 门禁描述同步 + 补「多账号（#68/#69）」功能小节（对齐 CHANGELOG v3.2.0）+ README.en 补 RELEASING 链接；无悬空链接。F11（doc/ 子系统漂移）已摸底出精确清单（features/10-docs F11 段）——**发现 STATE-MATRIX §2「4 账号命令未登记」是审计过标**：账号命令全 stateless、§2 只收 State 消费者→正确排除、不动。低风险主线程自审。
- **15 — F09 测试补齐**：① vendor `code-picture-core`（path 依赖非 workspace 成员，`--all` 测不到其 25 测）→ CI rust job 加 `cargo test -p code-picture-core`（不动 vendor 源，红线守）；② e2e 真跑需 Xvfb+tauri dev+xdotool（大 GUI runner、app 仅 Windows→低 ROI）→ 改**脚本健康冒烟** e2e-smoke ubuntu job（shellcheck --severity=error 忽略 style 噪音 + py_compile），真 e2e 记档待 v2/真机；③ main.ts:989 内联 basename 盲区 → 复用**已测** `sftp/paths.basename`（行为等价、纯 leaf 无环；panorama 第三份留 F12）。低风险主线程自审。tsc0/npm595/code-picture-core25/shellcheck+py0。
- **14 — F08b 前端 lint/coverage 门禁**：装 eslint@9(flat)+typescript-eslint@8 + stylelint@17 + @vitest/coverage-v8@4。**eslint/stylelint 顾问式**（CI `|| true` 不阻断，同 clippy；基线 lint 7/css 57、不追清零、不 --fix 避 churn；`_`-约定对齐是配置正确性非改代码）；**覆盖率地板棘轮**（S40/B34/F36/L41，当前值下方 ~2-3%，阻断但只挡明显回归；**不设 85% 全局**因 tsx `*.test.ts` 不计入 vitest 覆盖）。**prettier 不做**（避 styles/ts 全量 churn，风格已一致靠 review）。npm 脚本 lint/lint:css/coverage + ci.yml frontend job 三步。**F08 完结**。低风险主线程自审。tsc0/npm595/build✓/coverage floor✓。
- **13 — F06 定性 + F08a 红线机器护栏**：F06 无 aya-代码（#43 机制已修+已测、残留真机）。**F08a**（低风险主线程自审 + 双变异验证）：`TMUX_LS_FMT` 双写点断言（src-tauri tmux.rs，`include_str!` daemon 源、锚定 const 定义行、双向）+ **daemon 只读机器护栏**（remote-daemon-proto/readonly_guard.rs，`#[cfg(test)]` 内 strip cfg(test) 块后断言生产代码无 FS 变更）。**关键决策**：两护栏落成 **cargo 测**而非 CI YAML 步骤——跑在既有 rust/daemon job 且本地即验、无 YAML 脆弱性（比账本原措辞更优）。红线 I7（daemon 只读，只加只读测试）/I8（TMUX_LS_FMT 只断言不改）机器化守护到位。src-tauri 365 / daemon 全绿。F08b（前端 eslint/stylelint/coverage，warn-only 基线不追清零；prettier 不做避 churn）待下轮。
