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
| F08 | 质量门禁（eslint/prettier/stylelint/覆盖率棘轮/mock 卫生）+ `TMUX_LS_FMT` 双写点 CI 断言 + daemon 只读机器护栏 | I8 I7 | 🟡 F08a 完成（两护栏=cargo 测）；F08b 前端 lint/coverage 待做 |
| F09 | 测试补齐（main.ts 盲区可测纯函数 + vendor code-picture-core 进 cargo test + e2e 冒烟进 CI）| I8/G3 | ⬜ 待做 |
| F10 | README 中英修版本/删悬空/补账号 + RELEASING/CONTRIBUTING checklist 补 README 两条 | I2 I3 | ⬜ 待做 |
| F11 | 文档漂移（ARCHITECTURE 账号子系统 + STATE-MATRIX 4命令 + INVARIANTS 上移 color-scheme + 子README + 索引 + actions 数）| I7docs/G2 | ⬜ 待做 |
| F12 | `remote-section.ts` 数据层抽 `remote-config.ts`（治分层倒挂）| I6/G3 | ⬜ 待做 |
| F13 | 脊柱拆分（tabs 抽 AccountBadgeController；评估 ssh_source）| I6 | ⬜ 待做（最高风险，撞到停）|
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
| `src/settings/remote-section.ts` 数据层 | F12,F13 | 纯数据迁 `remote-config.ts` | ⬜ F12 |
| `.github/workflows/ci.yml` + `package.json` | F08,F09,F13 | 终态 job：rust/frontend(+lint+coverage)/daemon/vendor-crate/e2e-smoke + 双写点断言 | 🟡 F08a：双写点断言 + daemon 只读护栏落成 **cargo 测**（跑既有 rust/daemon job，比 CI 步骤更稳）；F08b 补 lint/coverage npm+CI |
| 文档簇 + BACKLOG | F10,F11 | 不漂移；BACKLOG 打删除线 | ⬜ F10/F11 |

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
- **13 — F06 定性 + F08a 红线机器护栏**：F06 无 aya-代码（#43 机制已修+已测、残留真机）。**F08a**（低风险主线程自审 + 双变异验证）：`TMUX_LS_FMT` 双写点断言（src-tauri tmux.rs，`include_str!` daemon 源、锚定 const 定义行、双向）+ **daemon 只读机器护栏**（remote-daemon-proto/readonly_guard.rs，`#[cfg(test)]` 内 strip cfg(test) 块后断言生产代码无 FS 变更）。**关键决策**：两护栏落成 **cargo 测**而非 CI YAML 步骤——跑在既有 rust/daemon job 且本地即验、无 YAML 脆弱性（比账本原措辞更优）。红线 I7（daemon 只读，只加只读测试）/I8（TMUX_LS_FMT 只断言不改）机器化守护到位。src-tauri 365 / daemon 全绿。F08b（前端 eslint/stylelint/coverage，warn-only 基线不追清零；prettier 不做避 churn）待下轮。
