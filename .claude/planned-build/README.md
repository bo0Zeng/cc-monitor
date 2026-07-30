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
| **zero-poll-liveness/** | 判活从轮询改成内核事件（tmux hook + pidfd + inotify）。承 BACKLOG **E34**（用户点名「把轮询杀掉」）+ 用户 2026-07-30 追加「daemon 是能改的，要性能最佳且不要轮询」 | **Phase A 已落盘，等审批**。**范围比 E34 登记的大一格**：daemon 里是 **A/B 两条**轮询（2s 判活 tick + 8s `tmux ls`），E34 只盯了后者。P1 顺带销掉 `INVARIANTS:408` 那条预先登记、卡在「daemon 零改」上的真 bug |


---

## 当前在跑：四区连续执行顺序（2026-07-29 用户授权全自动）

> **用户原话**：「我要全自动把这些需求跑完」+ 顺序按 ①rust-ts-boundary ②account-zero
> ③gate-integrity ④local-as-remote。下表是把该顺序与**技术依赖**、**外部授权**合并后的实际序列。
> **loop 每轮先读本表，再读对应区的 `STATUS.md`。**

| # | 功能 | 区 | 需要外部授权？ |
|---|---|---|---|
| 1 | **C01** 边界样板（一条命令走通，**变异验收**：删一个 Rust 字段 tsc 必须报错） | rust-ts-boundary | 否 |
| 2 | **C05** 门禁（生成物必须最新）—— **2026-07-29 由 #4 提到这里**，理由见下 | rust-ts-boundary | 否 |
| 3 | **C02** 事件半边（已是单一枢纽，改动面最小） | rust-ts-boundary | 否 |
| 4 | **C03** 大整数策略（必须在 C04 之前，否则把已知数据损失批量固化） | rust-ts-boundary | 否 |
| 5 | **C04a** 类型化 invoke 包装层 + 钉死 119 个命令（机制先建）—— **完成** (`588cf5d`，Phase D 3 阻塞+6 重要+11 建议全处置) | rust-ts-boundary | 否 |
| 5b | **C04b** 两处内联字面量 —— **`main.ts:744` 已完成** (`285bbae`，生成物 14→15，变异 A 是成功标准 1 在命令返回类型上首次成立) · **`tabs.ts:1632` 已跳过，等授权**（实测**一行改动**：类型 C02 已生成、字段逐字节一致，零技术障碍） | rust-ts-boundary | **`tabs.ts` 红线** |
| 5c | **C04c** JSONL 边界 —— **完成**：Phase B 修订处置，**直接生成 `JsonlRecord`** 而非用逃生口指向手抄版（生成物 15→21；三处静默漂移自动消失；守卫补上「不扫 enum」与「字段层属性顺序敏感」两个真缺口） | rust-ts-boundary | 否 |
| 5d | **C04d** 按模块分批迁移 —— **完成**（八批 11 个 commit：`f44cb57`…`4696505`）。`import invoke` 的生产文件 **29 → 3** = 主计划成功标准 4 达成；**119 个命令全部静态可见**（盲区归零）；生成物 → 67。两批等授权：4b `accounts.ts`（等 Z02）· 8 `tabs.ts`（等红线） | rust-ts-boundary | 部分卡红线 |
| 6 | **G-B** vendored bash 进 shellcheck + `run-tests.sh` 进 CI | gate-integrity | 否 |
| 7 | **Z01** 账号 0 登记 + 可见 | account-zero | **是：动 `~/.claude/skills/cc-acct-iso/`** |
| 8 | **Z04** 守卫 | account-zero | **是：同上** |
| 9 | **Z02** 「未选账号」消失 | account-zero | **是：`tabs.ts` 红线** |
| 10 | **Z03** 账号 0 接既有能力 | account-zero | 是（承 Z01/Z02） |
| 11 | **Z05** rc 片段一键生成（独立，可提前） | account-zero | 否 |
| 12 | **G-A** 八套真机套件断言地板 | gate-integrity | 否 |
| 13 | **G-C** 6 套 e2e 进 CI | gate-integrity | 否 |
| 14 | **L5** 平价对账表 + 门禁（独立，可提前） | local-as-remote | 否 |
| 15 | **L0** Linux 可构建可跑（**唯一可能推翻方向的一步**） | local-as-remote | 否 |
| 16 | **L1** POSIX 本地 = 不走 ssh 的远端 | local-as-remote | 否 |
| 17 | **L2** Windows 本地进 IR | local-as-remote | 否 |
| 18 | **L3a** 本地账号枚举（只读，Rust 读 manifest） | local-as-remote | 否 |
| 19 | **L4** Linux 打包进 CI/release | local-as-remote | 否 |
| 20 | **L3b** 本地账号管理（写） | local-as-remote | 依赖 account-zero 全部落地 |
| **21** | **E34 事件驱动的 tmux 存活信号**（用户点名「把轮询杀掉」）—— 已升格为独立工作区，**Phase A 已落盘等审批**，拆成 P0-P7 八个功能 | **zero-poll-liveness** | **部分**：P4（装 hook 到活着的 tmux server）要授权；其余不要。**原表两处已订正**：① 不再需要改 `shared/ccm` 本体（hook 由 **daemon** 装——只有 daemon 有「server 重启」这个时机）② §24 单写者不再是开放问题（所有新信号都汇进既有 `SessionChange{removed}` → emitter，零新写点） |

**为什么 C05 从 #4 提到 #2**（C01 的 Phase D 审计 I1 实测）：CI 的 `rust` job（跑 `cargo test`）
与 `frontend` job（跑 `tsc`）是**两次独立 checkout**——重新生成的产物在前者里被丢掉，
后者对着**已提交的**生成物做类型检查。审计变异实测：serde-rename 一个字段而**不重新生成**
⇒ `cargo` 绿、`tsc` 绿、5 条守卫绿、`npm` 819 全绿，**而生产里 UI 标签会变空**。
也就是说「改了 Rust 忘了重新生成」这个洞**让每一道现有门禁都保持绿色**。
C01 已加 `npm run check:types`（同一棵树里 regen → tsc → `git diff --exit-code`）作为可脚本化的
兜底，但**它还没进 CI**。若先做 C02/C03/C04，这个洞会被复制到 127 个 struct。

**为什么 G-B 插在 Z01 之前**：Z01 要改 `vendor/cc-acct-iso/scripts/`，而那 1348 行今天在
shellcheck 门禁之外、它自己的 424 行测试从没跑过。**没有网不能改那个工具。**

**未获授权时的行为（loop 不许在这儿空转）**：跑到 #7 若两条授权都还没有，**跳过 #7-#10，
继续 #11 起**；跳过的项在本表标注「已跳过，等授权」，并在收尾汇报里如实列出。
**绝不为了「跑完」而自行放宽用户设的红线或改用户家目录里的文件。**

**loop 停止条件**（任一命中即停，交回用户）：
撞到需要新决策的阻塞 · 同一步 ≥2 次失败 · 门禁红且非在途变异 · 全部完成（→ Phase G）

---

> 注：状态摘要仅导航用；权威状态恒以各区 `STATUS.md` 为准。新开工作区时在此追加一行。
