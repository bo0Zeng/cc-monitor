# planned-build 工作区索引

`planned-build` skill 的持久产物按**工作区**分子目录，每区一套 `{MASTERPLAN,STATUS}.md` + `features/`。
恢复某区先读其 `STATUS.md`。本索引由 audit-fixes F11 建，列各区一句话状态。

> **2026-07-29 Phase G 全面订正。** 此前本表**缺 integrate-toolchain**（当时唯一在动的工作区），
> 另有 7 行失准，且「以各区 STATUS 为准」这句免责**当时救不了**——被指向的两份 STATUS 自己
> 也落后实际 3-4 个功能（同轮已修）。**所以：这句免责只在被指向的文件确实新鲜时才成立。**

| 工作区 | 主题 | 状态（约） |
|---|---|---|
| **unify-launch/** | 统一整个软件的会话启动架构（15 套实现 → 一条路径；层所有权 + LaunchPlan IR + 账号/身份/终端调和） | **已收官**：F01-F11 + R 段 9/9 + B 段 B01-B04 + L2 全部交付并 push。含 `INVENTORY.md`（34 个符号锚点 2026-07-29 实测全部命中） |
| **account-onboarding/** | 账号体验重做（新用户一站式无歧义） | F5(9d3a7e6)/F1(7680a43) 已交付；余下**已被 unify-launch 接管**（其 MASTERPLAN-v2 经四视角 full-audit 判定不可执行，审计结论仍在本区 `AUDIT-v2-FINDINGS.md`） |
| **audit-fixes/** | full-audit + open issue bug 全修 + 测试/门禁/文档/重构（`account-ux` 分支可干净合入） | **已收官**（F01-F12 + Phase G 均完成；F13 脊柱拆分单独拆出 → spine-split/） |
| **spine-split/** | 脊柱拆分评估：`tabs.ts` / `ssh_source.rs` 是否分解为小模块（**行数别照抄本表**：2026-07-29 实测 3309 / 4847，本区文档记的 3178 / ~4512 都已过期，且以行数立论本身违反那条判据） | **关闭——评估后决定不拆**（判据=具体架构病非行数；两文件拆分负收益+引入 §24/可见性风险；唯一真架构病 F12 已在 audit-fixes 修）。未动代码。**Phase G 代码工程视角独立复核认同**：`ssh_source.rs` fan-in 14 但只暴露 6 个符号，是内聚 SSH facade；唯一异味是 `shell_quote` 放错模块（BACKLOG E31） |
| **auto-e2e/** | 给真机功能（灰灯/resume/attach/换号/账号）补 e2e 埋点 + Windows 全自动测试 harness | **Phase C（主计划已批、F-E0/F-E1 灰灯已并入主线）；E2-E5 有完整计划、零落地记录、无收官语——读起来像在飞行中，实际最后改动早于两个活跃区 3 天** |
| **account-ux/** | 多账号 UX（切号菜单 / 按会话选号 / 换号优雅重启 / app 内部署向导，#68/#69） | 已交付。**那次发版实际是 v3.3.0**（本表原写 v3.2.0；`package.json`/`Cargo.toml`/`tauri.conf.json`/`CHANGELOG` 都是 3.3.0，只有 `README.md` 还写 3.2.0 → BACKLOG E30） |
| **account-isolation/** | 多账号隔离又同步内核（`cc-acct-iso`：各 `CLAUDE_CONFIG_DIR` + symlink 共享） | 已交付（v3.2.0） |
| **bugfix-sweep/** | 会话/生命周期一批 bug 清扫 | 归档（成果已并入主线） |
| **daemon-codex/** · **codex-phase2/** | daemon 侧适配 codex（非 CC agent）分阶段 | **DG3-DG6 已交付**，其后**由用户暂停**（暂停只写在 `daemon-codex/` 里，`codex-phase2/STATUS.md` 的「Phase A 完成，指向 F1」落后自己正文 6 个功能） |
| **tmux-daemon-reconcile/** | tmux 存活对账（带外杀 tmux → 变灰，#60-A / F74c） | 已交付（reconcile_step + 收帧收割器，见 INVARIANTS §24/§24bis）。**注意：该目录无 `STATUS.md`/`MASTERPLAN.md`，`PLAN.md` 从不声明完成——「已交付」只存在于本索引里，不可回溯** |
| **integrate-toolchain/** | 工具链整合：受管工具注册表 + 配置面审计 + 待贴文本统一 + 设置面板 IA（**P 段**） | **已收官**：T01/T02/T03/T04/T07 交付并审计闭环；T05/T09 移出本区；T06（code-picture）**用户已搁置**；T08 未开工。Phase G 终账 = `PHASE-G-DRAFT.md` + 仓根 `项目审阅报告-PhaseG-2026-07-29.md` |

| **account-zero/** | 把「基座」变成受管的「账号 0」（吸收破坏隔离的那个状态，而不是把它定义成违规） | **Phase A 已落盘、用户 2026-07-29 已批准**；等 Z01 的功能计划。Z01 纯增量零风险；Z02 要碰 `tabs.ts`（红线待松） |
| **rust-ts-boundary/** | Rust↔TS 边界从人工纪律改成生成物（`tauri-specta`）+ 门禁 | **Phase A 已落盘，等审批**。路线图 ①，是 ③④ 的地基；也是「要不要用 Rust GUI 重写前端」那个问题的便宜答案 |
| **gate-integrity/** | 门禁不许在零断言下报绿（真机套件断言地板 + vendored bash 进 shellcheck + 6 套 e2e 进 CI） | **Phase A 已落盘，等审批**。路线图 ③，规模小但保护其余全部工作。**G-B 是 `account-zero` Z01 的前置** |
| **local-as-remote/** | 本地 = 不走 ssh 的远端（含 Linux 平台）。落地 `doc/INVARIANTS.md` **§40** | **Phase A 已落盘，等审批**。路线图 ④。L5 平价对账可先做；L0 是唯一可能推翻方向的一步（WebKitGTK） |

> 注：状态摘要仅导航用；权威状态恒以各区 `STATUS.md` 为准。新开工作区时在此追加一行。
