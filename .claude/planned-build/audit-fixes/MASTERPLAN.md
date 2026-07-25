# 主计划 / MASTERPLAN — audit-fixes（rev 08 定稿：bug 全清 + 测试/文档/重构）

> 单一事实来源。动因：`项目审阅报告-2026-07-25.md`（full-audit）+ `issue-bug根因报告-2026-07-25.md`（8 issue 根因）。
> 用户 2026-07-25 批准：全自动做完剩余 bug + 测试/文档/重构；灰灯走 **A（后端 reconcile 出信号）**；做完的 issue 关闭；F14（碰 ~/.bashrc）用户自跑。

## 0. 目标与范围
- **总体目标**：把所有 OPEN bug 修完（会话/生命周期 cluster + full-audit 的 I4/I5），并补测试门禁 + 修文档漂移 + 低风险重构，让 `account-ux` 分支可干净合入。
- **范围内**：F03 剩余（步骤 2-4）· F04 · F05 · F06 · F07（bug）+ F08-F13（测试/文档/重构）。做完每个 bug 关对应 GitHub issue。
- **范围外（红线，每轮核对）**：
  - **daemon（`remote-daemon-proto/`）零行为改动** —— 灰灯走 A 也只用 daemon 现成的 `TmuxSessions` 帧（已带 `pane_current_command`）；reconcile/emitter 改在 **src-tauri**。**不改 `TMUX_LS_FMT` 双写点任一侧列**。
  - **不碰用户 `~/.bashrc`** —— F14（zcc/bcc 迁移）单列、用户自跑、我只给脚本/清单。
  - 不改 `cc-<sid8>` 会话名协议既有语义；不 push / 不发版 / 不 bump；孤儿**不自动 kill**（F05 仅手动）。
- **整体成功标准**：8 个 open bug 全修（或明确转 F14/真机）+ 对应 issue 关闭；tsc 0 / npm test 全绿 / cargo test（含 vendor）全绿 / build ✓ / 覆盖率门禁过；每个 bug 修复有回归测 + 变异验证。

## 1. 功能清单（rev 06 现状）
> 状态：完成 / 实现中 / 待规划

| ID | 功能 | 对应 bug/审计 | 状态 | 关 issue |
|----|------|--------------|------|--------|
| F01 | follow-resume 账号安全 + resume 选账号 | B1 + #75(部分) | ✅ 完成 | #75 待全修后关 |
| F02 | kill_remote_tmux 白名单 | I1 | ✅ 完成 | （无 issue，审计项）|
| F03 | 统一 tmux 管线 + 三态生命周期 | #76 #75 #74 #72 #41 | 🟡 步骤1完成 | #76 **只关命名/回收/复用部分**；其「无 @ccm_sid→attach 管不了」子句 = ccm 重装真机前置（同 #60②），不随代码关 |
| ├ 3.1 | idle-tmux 就地复用 resume | #76 根因 | ✅ 完成 | — |
| ├ 3.2 | **idle-tmux 灰灯（走 A：reconcile 发 idle 信号）** | #76 显示 / #60 邻域 | ⬜ **下一个（高风险）** | — |
| ├ 3.3 | attach-into-idle（菜单 attach 进空 tmux） | #76 | ⬜ 待做 | — |
| └ 3.4 | 建序列直设 `ccm-rbind-<sid>` 标题（tmux 路径，#74/#41 结构因）。**直连无 tmux=天然无 @ccm_sid（非可补 bug，路由决策归 F04）；新会话 sid 建时未知（靠 wrapper 回填）** | #74 #41 #72残留 | ⬜ 待做 | 代码完 + **真机验证**后关 #74/#41；#72 tmux 已修、直连/新会话属天然限制 |
| F04 | 统一直连管线 + keepalive（失败不闪退）+ **决定直连是否路由进 tmux（决定 #72 直连腿能否有身份）**| #75 直连腿 | ⬜ 待做 | 代码完(keepalive 单测)+ **真机验证窗口不再闪退**后，与 F01 合并关 #75 |
| F05 | 手动清理真孤儿（**仅 `cc-*` 无对应 tab 的**，过 `is_ccm_tmux_name`）。`<project>_cc`=cc-bus 资产、不归 cc-monitor 清（见 #76「cc-bus 退纯通讯」future）| #76 残留 | ⬜ 待做 | — |
| F06 | 绑定族：#43「父子拉不起来」残留（代码）+ #60/#43/#63attach 的 F74* **真机验证**。**#63 尾消息(torn-tail)=daemon-bound（根因在 `remote-daemon-proto/src/watcher.rs` `read_new_lines` 扣尾行），红线内不可修 → 转发版/真机，本轮不做不关** | #43 #60 #63(部分) | ⬜ 待做 | 真机验证后关 #60/#43；#63 只关①/attach，torn-tail 留 |
| F07 | 刷新竞态(I4) + 多远端缓存(I5) + onUnselectable | I4 I5 | ⬜ 待做 | （审计项）|
| F08 | 质量门禁（eslint/prettier/stylelint/覆盖率棘轮/mock 卫生）+ **`TMUX_LS_FMT` 双写点逐字相等 CI 断言** + **daemon 只读机器化护栏（I7 结构面，非仅文档）** | I8 I7 | ⬜ 待做 | — |
| F09 | 测试补齐（main.ts 盲区 + vendor crate + e2e CI）| I8/G3 | ⬜ 待做 | — |
| F10 | README + 发版根因 checklist | I2 I3 | ⬜ 待做 | — |
| F11 | 文档漂移（ARCHITECTURE/STATE-MATRIX/INVARIANTS/子README/索引）| I7docs/G2 | ⬜ 待做 | — |
| F12 | remote-section 数据层抽 remote-config.ts | I6/G3 分层倒挂 | ⬜ 待做 | — |
| F13 | 脊柱拆分（tabs AccountBadgeController；评估 ssh_source）| I6 | ⬜ 待做 | — |
| ~~F14~~ | ~~.bashrc zcc/bcc 迁移~~ | — | **用户自跑** | — |

**已交付关闭的 issue**：#71 #42 #67 #46（v3.2.0 bugfix-sweep 已修，本轮 full-audit 复核后关闭）。

## 2. 架构概览
- **三层**：TS 前端 `src/` ↔ Tauri Rust `src-tauri/` ↔ 只读 daemon `remote-daemon-proto/`（零行为改动）。
- **账号解析瀑布（勿破）**：`resume 显式选号 > 会话 lastAccount(pin 现读磁盘) > 全局当前工作账号 > 基座`。
- **会话三态（用户拍板）**：`live(claude 跑,绿/黄/红) / idle-tmux(tmux 在但 command≠claude,灰灯) / archived(tmux 没了,灰掉)`。
- **live-state 单写者（INVARIANTS §24）**：`remote_active` 唯一写者 = remote-session-emitter，收 `SessionChange` 通道（daemon 事件 / 断连 flush / **tmux_reconcile poller**）。**F03.2-A 在此加第三类"idle"信号**（高风险区）。

## 3. ★共享面账本（rev 09）
| 共享面 | 涉及功能 | 最终形态 | 状态 |
|--------|----------|----------|------|
| `src/tabs.ts` pin 读取 | F01,F13 | `readSessionPin` 现读磁盘，三处 resume 一致 | ✅ F01 落 |
| `src/remote-launch.ts`+`session-backend.ts` 载荷/命名/身份 | F03,F04 | create 与 reuse 共用 `buildResumePayload`；reuse 走 `runInExistingAttach`；F03.4 建序列设 rbind 标题（tmux 路径）；直连无 tmux=天然无身份、新会话 sid 建时未知 → 路由决策归 F04 | 🟡 F03.1 落 create/reuse；F03.4 补 rbind 标题 |
| **`SessionChange`(session_map.rs，本地+远端共用、无 Default) + emitter(§24 单写者，**归档唯一执行点 `lib.rs:590-606`**) + 前端 session 事件 + tabs 渲染** | **F03.2,F06** | **已定 final form**：idle = remote_active **之外**的第三态；emitter 不进 remote_active、发新前端事件；tabs 灰灯 class（不改 TabStatus 枚举）；F03.2 同轮更 §24。<br>**归档唯一落点 = emitter**（`lib.rs:590-606` `for sid in change.removed → emit SESSION_ENDED`）；ssh_source 的 removed 臂（**`:2231`=daemon-removed=唯一 live→idle 触发点** / `:1789`=断连 flush）**只往通道 send、不 emit** ⇒ 「判 idle vs archived」**架构上只能在 emitter 落**。<br>**★以下留 F03.2 Phase B 定（不在主计划硬写，高风险 + 全 D 审计）——两候选都要改 emitter removed 臂**：<br>&nbsp;(1) **idle 产出**：**(i)** 独立 tmux 扫描(不受 announced_live 门控)+ emitter **抑制/推迟** daemon-removed 归档；或 **(ii)** emitter 收 removed 时**同步查 tmux 快照**内联判 idle/archived。<br>&nbsp;(2) **idle→archived 正向产出者（必答）**：候选(ii) **原生无此信号**（sid 已离 announced_live，daemon 与 reconcile 都不再发）→ 不补则「卡灰关不掉」新 bug（#60 区）。谁产？（补扫描 / 扩 reconcile）<br>&nbsp;(3) 「刚 archived 被 idle 复活」反向竞态 + §24 单写者去抖。<br>&nbsp;(4) `SessionChange` **是否加 idle 字段**：**仅候选(i)/隐式方案需要**（要回填全 **9** 个构造点 session_map.rs:295/392、tmux_reconcile.rs:144、ssh_source.rs:1789/2182/2210/2231/2521/2549）；**候选(ii) 不需要加字段**（判断内联 emitter）。<br>**边界**：daemonless/陈旧 daemon 不发 TmuxSessions 帧（tmux_reconcile.rs:99-107、tmux_raw_registry 不填 :980-982）→ 恒 archived、**不进 idle**；`ssh_source.rs:2549`=`archive_daemonless`，**是这条 archived、非 live→idle 源**。 | ⬜ F03.2 落（**最高风险，机制先在其 Phase B 定 + 全视角 D 审计**）|
| `src-tauri/src/tmux_reconcile.rs` | F03.2,F06 | F03.2 在此加 idle 产出（需携带 `command`，非现在只收 sid，:127）；F06 在此标定 #60 常量（`POLL_INTERVAL` 可调；`RETIRE_MISS_THRESHOLD` 有编译期下限 `assert!(>=2)` :38，只能调到 ≥2）+ 更 10 个单测。两功能同改一文件，改前对齐 | ⬜ F03.2/F06 |
| `src/tabs.ts` `findClaudeTmux`(:242-254 @ccm_sid 精确匹配) | F03.2,F03.3,F06 | idle 判据（F03.2）、attach-idle（F03.3）、#60②/#63-attach 真机验证（F06）都碰这条核心匹配。改前查这三处对齐、勿各行其是 | ⬜ |
| `src-tauri/src/tmux.rs` | F02,F05 | kill 与 send-keys 对称过 `is_ccm_tmux_name`；F05 清理孤儿复用同校验 | ✅ F02 落 |
| `src/main.ts` refreshSessionAccounts | F07,F09 | in-flight 序号门 + 可测纯函数 | ⬜ F07 |
| `src/settings/remote-section.ts` 数据层 | F12,F13 | 纯数据迁 `remote-config.ts` | ⬜ F12 |
| `.github/workflows/ci.yml` + `package.json` | F08,F09,F13 | 终态 job：rust/frontend(+lint+coverage)/daemon/vendor-crate/e2e-smoke | ⬜ F08 |
| 文档簇 + BACKLOG | F10,F11 | 不漂移；BACKLOG 打删除线 | ⬜ F10/F11 |

## 4. 依赖与顺序（bug 优先）
- **bug 段（先）**：F03.3 → F03.4 → F04 → F05 → F07 → **F03.2 + F06（合并，共享 `tmux_reconcile`/live-state 面）**。**调整理由**（据 3 次复审）：F03.2 灰灯需 Phase B 机制决策（最高风险、会停 loop）且其功能核（idle 复用）F03.1 已交付 → 先做不依赖它的清晰项（F03.3 attach-idle / F03.4 rbind / F04 直连 / F05 清孤儿 / F07 竞态），把 F03.2 与 F06（#60 同区、同改 tmux_reconcile）**合并到最后一起做、机制在其联合 Phase B 定**（避免两次动 live-state 脆区）。
  - **F03.4↔F04 衔接**（审计 重要-5）：F03.4 只做 tmux 路径的 rbind 标题（身份），**不碰直连**；「直连是否路由进 tmux（决定它能否有身份）」的决策**归 F04**，避免直连面被改两遍。F03.4 实现前先确认不越界到直连。
- **工程段（后）**：F08 → F09（门禁先于补测）→ F10 → F11（文档零码风险）→ F12 → F13（重构最高风险最后，撞到停）。
- 每个 bug 功能完成 → 若其 issue 可完全关闭则 `gh issue close`（部分修的不关，留残留）。

## 5. 横切关注点与约定
- **回归纪律**：每个 bug 先写复现失败测试再修 + 变异验证（改坏看测试是否变红）。
- **审计强度**：F03.2（动 live-state 单写者，高风险）→ **Phase D 全视角并行 agent**；F04/F05/F06/F07 中风险 → 2-3 agent 或聚焦审。
- **测试约定（quality-gates，F08 落地）**：vitest(jsdom)+tsx；eslint(flat)+prettier；stylelint；@vitest/coverage-v8 分支覆盖率棘轮，账号/会话核心模块目标 85%；变异手动/核心模块；CI 云端 GitHub Actions。lint 棘轮基线不追一次清零。
- **门禁纪律**：结果重定向到文件 + Read/grep 核实 + pipefail；build 才抓 CSS 错。

## 6. 风险与开放问题
- **#63 尾消息 torn-tail = daemon-bound，本轮不可修**（审计 阻塞-1）：根因在 `remote-daemon-proto/src/watcher.rs` `read_new_lines` 扣尾行，修它破「daemon 零改动」红线 → 转发版时随 daemon bump / 真机做，本轮 F06 **明确不碰不关**。
- **F03.2-A 高风险**：动 `SessionChange`/emitter/单写者(§24) + 前端新事件 + tabs 渲染，在 #60/#43/#63 历史 bug 高发区；且 `SessionChange` **本地+远端共用**（加字段要回填本地构造点）。撞到状态机冲突/计划≠现实 → 停 loop。真机验证需远端 daemon(≥p1p)+ccm 助手在跑（用户机器动作）。
- **`TMUX_LS_FMT` 双写点是 F03.2 的 load-bearing 依赖**（审计 重要-6）：区分 live/idle 全靠它的 `pane_current_command` 列；任一侧改列静默丢行 → idle 判据全废。F08 加双写点逐字相等 CI 断言机器化守它。
- F04「直连 keepalive」形态（`; exec bash` vs `|| read`）实现时定，低风险。
- F06 #60/#43 残留 + #74/#41 + **#75 直连腿**都需真机确认（代码层修 + 单测，真机验证是用户动作）→ 代码完成后转真机再关。**#60②/#63-attach 关闭硬前置 = 用户远端重装 ccm 助手写 @ccm_sid**（issue 根因报告:41,63）——非纯代码能关。
- F13 ssh_source.rs(4512) 拆分高危 → 可能降级为只做 tabs controller 抽取。

## 7. 变更记录
- 01 — 初版（full-audit 9 功能）
- 02 — 并入 issue bug 根因 + 用户四决策 → F01–F13 + F14 自跑
- 03 — F01 完成（pin 现读 + 基座逃生口；per-account picker 已现成；tmux/history base 转 F04）
- 04 — F02 完成（kill 白名单对称）
- 05 — F03 重塑三态 + 步骤1（idle 就地复用）完成；§6 开放问题关闭=保留
- 06 — **重制（用户"重新制定全局计划并审计"）**：灰灯定 **A（reconcile 出 idle 信号）**；bug 优先重排（F03.2→…→F07 先，F08-13 后）；账本加「SessionChange/emitter/session 事件/tabs」高风险共享面；**关 #71/#42/#67/#46**（已修）；剩余 open bug=[41,43,60,63,72,74,75,76] 全在计划；加"做完关 issue"约定
- 07 — **据第一次计划审计(Plan agent)定稿**：①#63 torn-tail 重分类=daemon-bound、本轮红线内不修不关（F06 只做 #43 残留 + 真机验证）②F05 只清 `cc-*`、`<project>_cc` 归 cc-bus 不清（解 is_ccm_tmux_name 矛盾）③F03.4 只做 tmux rbind 标题、直连身份决策归 F04（解依赖倒置）④账本 F03.2-A 写可执行契约⑤F08 加 TMUX_LS_FMT 双写点 CI 断言 + I7 daemon 只读机器护栏⑥#74/#41 代码完转真机再关
- 08 — **据第二次独立复审(Plan agent)修正**：⑦**F03.2-A 契约的 idle 产出机制错**（reconcile 的 `announced_live` retain 使它在 daemon removed 后已不认识该 sid，不能产 idle）→ 改成「final form 定死 + 产出机制留 F03.2 Phase B 二选一(独立扫描 / emitter 收 removed 时查快照)」，不在主计划硬写⑧`SessionChange` 回填清单修正为真实构造点、点明 ssh_source removed/flush 臂是真难点⑨账本补两个共享面(tmux_reconcile.rs、tabs.ts findClaudeTmux)⑩idle 对 daemonless 静默退化标边界⑪#75 直连腿标真机再关⑫#60②/#63-attach 关闭硬前置=用户重装 ccm 助手
- 09 — **据第三次独立复审(Plan agent)定稿**：⑬F03.2-A 账本重写——把「加 SessionChange 字段 + ssh_source 臂判 idle」从"定死"降级为**仅候选(i)需要**（候选(ii) 判断内联 emitter `lib.rs:590-606`=归档唯一执行点、**不需加字段**）⑭**idle→archived 正向产出者**列入 Phase B 必答（候选(ii)原生无→防"卡灰关不掉"）⑮点明**两候选都要改 emitter removed 臂**⑯订正 `:2549`=daemonless archive 非 live→idle 源、构造点"8"→"9"⑰#76 只关命名/回收部分、attach 子句带 ccm 重装前置⑱F06 `RETIRE_MISS_THRESHOLD` 编译期下限 ≥2。三次独立复审除 F03.2-A 账本外全过（完备/红线/归属/排序/可关性 ✓）；F03.2 机制归其 Phase B（loop 到那停下 present）。**据以起 loop**
