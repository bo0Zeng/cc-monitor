# 主计划 / MASTERPLAN — audit-fixes（full-audit 修复 + issue bug + 统一起会话标准）

> 单一事实来源。动因：`项目审阅报告-2026-07-25.md`（4 视角 full-audit）+ `issue-bug根因报告-2026-07-25.md`（#76/#75/#74/#72/#63/#60/#43/#41 三 agent 根因）。
> 用户 2026-07-25 批准：全自动做 F01–F13；F14（碰 ~/.bashrc）用户自跑、不进 loop。

## 0. 目标与范围
- **总体目标**：修完 full-audit 的阻塞1+重要8、根因清楚的 issue bug，并**把所有"起会话"收敛成一致管线**（tmux 一套、直连一套，各自内部一致，不同调用点不再起不同名字/走不同管线）。
- **范围内**：F01–F13（见 §1）。
- **范围外（红线）**：
  - **daemon（`remote-daemon-proto/`）零行为改动**——#60① 所需 reconcile+`TmuxSessions` 帧已内嵌 p1p，只用其现成能力；回收/kill 走 src-tauri。**不改 `TMUX_LS_FMT` 双写点任一侧列**。
  - **不碰用户 `~/.bashrc`**——F14（zcc/bcc 迁移整合）单列、用户自跑、我只给脚本/清单。
  - 不改 `cc-<sid8>` 会话名协议的既有语义（只做"补齐 producer 缺口 + 统一"）。
  - **不 push / 不发版 / 不 bump 版本**，除非用户拍板。
  - 孤儿回收**不自动**（用户 2026-07-25 拍板）——只做手动按钮（F05）。
- **整体成功标准**：tsc 0；`npm test` 全绿；`cargo test`（monitor + vendor crate）全绿；`npm run build` ✓；覆盖率门禁通过；B1 有回归测+变异验证；README/ARCHITECTURE 与代码不漂移；所有 tmux 起点命名+@ccm_sid+identity 一致。

## 1. 功能清单
> 状态：待规划 / 规划中 / 实现中 / 审计中 / 完成

| ID | 功能 | 一句话 | 状态 | 依赖 | 优先级 |
|----|------|--------|------|------|--------|
| F01 | follow-resume 账号安全 + resume 选账号 | pin 现读磁盘(修 B1);resume 入口加账号下拉(含「基座/不隔离」),默认全局当前账号,显式重钉(修 #75) | **完成** | — | 🔴P0 |
| F02 | kill 白名单 | `kill_remote_tmux` 补 `is_ccm_tmux_name`(与 send-keys 对称)+订正假注释(修 I1) | **完成** | — | 🔴P0 |
| F03 | 统一 tmux 管线 + 三态生命周期 | 三态(live/**idle-tmux**/archived);idle 就地复用 resume(治 #76)+灰灯+attach-idle+建序列设 rbind 标题(#74/#41)。用户拍板 B 保留(不自动 kill) | **实现中**(步骤1/4 完成) | — | 🟠P1 |
| F04 | 统一直连起会话管线 | 所有直连一致 + keepalive(失败不闪退) + 每入口提供 tmux/直连后端选择(修 #75 直连腿) | 待规划 | F01 | 🟠P1 |
| F05 | 手动清理孤儿 | 「清理孤儿会话」按钮:点了才扫 cc-* + 列确认无 claude + 二次确认 kill;不自动(缓解 #76) | 待规划 | F02 | 🟠P1 |
| F06 | 绑定族验证 + 残留 | 代码层确认 #60②/#63/#43/#72-tmux 的 F74* 完整并补测;新修 #63 尾消息 torn-tail + #43 父子拉不起来 | 待规划 | — | 🟠P1 |
| F07 | 刷新竞态 + 缓存 | refreshSessionAccounts 加序号门(I4);切号失效全 origin 缓存(I5);resumeTab 补 onUnselectable | 待规划 | — | 🟠P1 |
| F08 | 质量门禁基建 | eslint+prettier+stylelint+vitest 分支覆盖率(棘轮)+mock 卫生,接 CI(I8);不追一次清零 | 待规划 | — | 🔵P1 |
| F09 | 测试补齐 | 补 main.ts 等盲区可测纯函数;vendor code-picture-core 进 cargo test;e2e 冒烟进 CI | 待规划 | F08 | 🔵P2 |
| F10 | README + 发版根因 | README 中英修版本/删悬空/补账号;RELEASING+CONTRIBUTING checklist 补 README 两条(I2+I3) | 待规划 | — | 🔵P1 |
| F11 | 文档漂移批量 | ARCHITECTURE 账号子系统 + STATE-MATRIX 4命令 + INVARIANTS 上移 color-scheme + 子README + proposals 归位 + 索引 + actions 数 | 待规划 | — | 🔵P2 |
| F12 | remote-section 数据层抽取 | readRemoteConfig 等纯数据抽 src/remote-config.ts,治分层倒挂 | 待规划 | — | ⚪P2 |
| F13 | 脊柱拆分 | tabs 抽 AccountBadgeController;(评估)ssh_source/lib.rs setup 分模块——最高风险最后 | 待规划 | F01,F07,F12 | ⚪P3 |
| ~~F14~~ | ~~.bashrc 账号命令迁移~~ | zcc/bcc 迁 CLAUDE_CONFIG_DIR 隔离 + 干净 identity 启动器 | **用户自跑** | — | ⬜ |

## 2. 架构概览
- **三层**：TS 前端 `src/` ↔ Tauri Rust `src-tauri/` ↔ 只读 daemon `remote-daemon-proto/`（本族零行为改动）。
- **账号解析瀑布（勿破）**：`resume 显式选号 > 会话 lastAccount(pin) > 全局当前工作账号 > 基座(无可选账号时)`。pin 真相源 = 本机 `history-metadata.json`，**必须现读**（`invoke("list_last_accounts")`）。
- **起会话两后端**：tmux（可 attach/管理，走 `session-backend.ts` createRunAttach）+ 直连（`ssh -t` 裸跑，不进 tmux）。目标：**每后端一条管线、所有调用点共用**。
- **身份契约**：`@ccm_sid`（tmux session option，认会话）+ `ccm-rbind-<sid>`（窗口标题，↗ 拉前绑 HWND）。目标：建会话序列里**直接设两者**，不依赖交互 `__ccm_rbind`。

## 3. ★共享面账本
| 共享面 | 涉及功能 | 最终形态 | 状态 |
|--------|----------|----------|------|
| `src/tabs.ts` pin 读取 | F01,F13 | pin 走新私有方法 `readSessionPin(sid)`（现读 `list_last_accounts`），resumeTab/resumeTabTmux 共用；史 history.ts 已现读，三处一致 | F01 引入 helper |
| resume/起会话入口 UI（后端+账号选择） | F01,F03,F04 | 每入口统一「后端(tmux/直连) + 账号(含基座)」选择组件；F01 先落 resume 版，F03/F04 铺到新建/重启并复用 | F01 落 resume |
| `src/remote-launch.ts` + `session-backend.ts` 载荷/命名/身份 | F03,F04 | tmux：统一命名 + create 序列设 @ccm_sid + rbind 标题 + `; exit` 收尾；直连：一致 + keepalive。命名/身份/回收单一实现 | F03/F04 |
| `src/agent-profile.ts` launcher | F03 | 评估默认 launcher 是否仍裸 claude（identity 改由 create 序列设，不再依赖 ccm 交互） | F03 |
| `src-tauri/src/tmux.rs` | F02,F05 | `kill_remote_tmux` 与 `tmux_send_keys` 对称过 `is_ccm_tmux_name`；F05 清理孤儿复用同校验 | F02 先补 |
| `src/main.ts` refreshSessionAccounts | F07,F09 | 加 in-flight 递增序号门；核心判定抽可测纯函数 | F07 |
| `src/settings/remote-section.ts` 数据层 | F12,F13 | 纯数据迁 `src/remote-config.ts`，7 下游 import 改指向 | F12 |
| `.github/workflows/ci.yml` | F08,F09,F13 | 终态 job：rust / frontend(+lint+coverage) / daemon / vendor-crate-test / e2e-smoke | 逐功能加 |
| `package.json` | F08,F09 | 统一 test 入口 + lint/format/coverage 脚本 | F08 |
| 文档簇 + BACKLOG | F10,F11 | 文档不漂移;BACKLOG 对应项打删除线 | F10/F11 |

## 4. 依赖与顺序
- 依赖：F04→F01（复用账号选择组件）；F05→F02（复用白名单）；F09→F08；F13→F01/F07/F12。
- 顺序理由：正确性(F01→F02)先解合入门槛 → 统一管线(F03→F04→F05)是核心诉求且改动面大早做 → 绑定族验证(F06)+竞态(F07)收账号尾 → 测试门禁(F08→F09)让后续在门禁下落地 → 文档(F10→F11)零代码风险 → 重构(F12→F13)最高风险最后、撞到就停。

## 5. 横切关注点与约定
- **回归纪律**：F01/F02/F06 每个 bug 先写复现失败测试再修。
- **测试约定（quality-gates，F08 落地）**：TS lint=eslint(flat)+prettier；CSS=stylelint；覆盖率=@vitest/coverage-v8 看分支、棘轮(不许比上次低)、账号核心模块目标 85%；变异只手动/核心模块；CI=云端 GitHub Actions(已有 ci.yml)。lint 引入用棘轮基线,既有违规记基线不阻断,只挡新增。
- **门禁纪律**：结果重定向到文件 + Read/grep 核实 + pipefail，别信内联回显；build 才抓得到 CSS 语法错。
- **红线每轮核对**（见 §0 范围外）。

## 6. 风险与开放问题
- F03/F04 改载荷/命名/身份是核心手术，波及所有起会话入口 + 有安全转义约束（launch.rs 拒双引号等）→ 中高风险，撞到"计划≠现实"停 loop。
- F03「claude 退出即收会话(`; exit`)」与「保持会话可 re-attach」有张力——Phase B 定清：默认退出即收(治孤儿) vs 保留(可 attach)。开放问题，到 F03 拍。
- F08 lint 在存量码可能爆量 → 棘轮基线缓解；仍停摆则停 loop 问用户降规则集。
- F13 ssh_source.rs(4512) 拆分高危 → 可能降级为只做 tabs controller 抽取。
- 已修绑定族(#60/#63/#43)的**真机验证**需远端装 ccm 助手 + daemon 在跑 = 用户机器动作，我只能代码层核实。

## 7. 变更记录
- 01 — 初版 — 主规划（基于 full-audit 报告，9 功能）
- 02 — 重排 — 并入 issue bug 根因（#76/#75/#72 等）+ 用户四决策（resume 选账号/两后端都保留各自一致/孤儿仅手动/identity 建时设不进 tmux 敲 ccm）→ 扩为 F01–F13 + F14 用户自跑；账本加「起会话入口 UI」「载荷命名身份」「pin readSessionPin」三共享面
- 03 — F01 完成 — 发现 per-account「用账号 X resume」picker A4/A5 已存在（步骤3 大半现成）；步骤2 补「用基座 resume」逃生口即闭合 #75 的选号面。**tmux 版 base + 历史页 base 项显式转 F04**（每入口后端×账号矩阵一处做，防两处各做）→ 账本「resume/起会话入口 UI」最终形态记明由 F04 收口
- 04 — F02 完成 — kill 守卫与 send-keys 对称（cc-* 放行含 cwd 回退自建会话、非 cc- 拒），修 I1 误杀。**下一站 F03 触及 §6 开放问题「claude 退出即收 vs 保留可 attach」→ loop 停下交用户拍板后再实现**
- 05 — F03 重塑 + 步骤1 完成 — **用户拍板 B 保留**（杀的是 tmux 不是 claude 会话,不自动 kill）+ 加**第三态 idle-tmux**（灰灯）+ idle 就地复用 resume/attach。§6 开放问题据此关闭（不再"退出即收"）。账本「载荷/命名/身份」最终形态更新：create 与 reuse 共用 `buildResumePayload`，reuse 走新 `runInExistingAttach`（无 new-session）。**F05 手动清理**范围缩为"无对应 tab 的真孤儿"（idle-tmux 现在是复用而非孤儿）。步骤1（idle 就地复用 resume）已交付，剩灰灯/attach-idle/rbind 标题（步骤2-4）
