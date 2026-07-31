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
| **account-zero/** | 把「基座」变成受管的「账号 0」（吸收破坏隔离的那个状态，而不是把它定义成违规）+ **2026-07-30 扩范围：原生身份组成的单点声明 + 版本钉/漂移检测 + 物理迁移能力** | **Phase A 已批（07-29）、07-30 按用户追加需求修订落地**。8 个功能（Z01-Z08）**零代码**。**Z08 迁移能力是 P0**——它同时是 BACKLOG **E36「API key 路线乙」**的前置。**整个 cc-acct-iso 半区的硬前置是 G-B**。Z02 仍卡 `tabs.ts` 红线；动 `~/.claude/skills/` 与 z/b 真账号目录的授权**已给**，但用户说「先不要改，我在用 claude code」⇒ 动真账号那步等发话 |
| **rust-ts-boundary/** | Rust↔TS 边界从人工纪律改成生成物（`tauri-specta`）+ 门禁 | **Phase A 已落盘，等审批**。路线图 ①，是 ③④ 的地基；也是「要不要用 Rust GUI 重写前端」那个问题的便宜答案 |
| **gate-integrity/** | 门禁不许在零断言下报绿（真机套件断言地板 + vendored bash 进 shellcheck + 6 套 e2e 进 CI） | **主计划已批（07-29）；G-B 已交付签收（07-30）**。**G-B 解除了 `account-zero` cc-acct-iso 半区（Z01/Z04/Z06/Z08）的「没有网不能改那个工具」**。余 G-A（八套断言地板）· G-C（6 套 e2e 进 CI） |
| **settings-ia/** | 设置面板信息架构重做：作用域为轴（应用 / 机器 / 改动足迹）· 本机是机器列表第一行 · 三处部署首次同屏 · cc-bus 驾驶舱移出设置 | **✅ 全区收官（2026-07-31）**：11 个功能 S0-S10 全部交付并各自一个 commit（S9 是 Phase G 核账当场逮出漏做的）。共享面账本 7 条里 5 条达最终形态，**2 条如实标黄未改绿**（`panel.ts`「每页各自一个模块」没做但不追加拆分；`collapsible-group` 展开回调没加 —— 落地后一个消费者都没出现，是当初的预判失误而非欠账）。Phase G 审计另开 **E59-E63**。**S1 持久化改 per-host patch 是硬前置**（现在 `writeRemoteConfig` 整表覆盖，拆页面会静默抹掉未挂载的机器）。顺带修 S0：daemon tmux 快照陈旧回归（用户实测「branch 后原 tab 永久灰点」）|
| **branch-anywhere/** | 任意对话节点分叉，**两条都活着**：远端也能 branch · 不走 CC 的 `/branch`（那是同一 pidfile 原地换 sid）而是复制 jsonl + 另起一个 · 继承原会话参数（账号/tmux/cwd） · 实时会话里也有入口 · 废弃分支保留路口但呈现区分 | **Phase A 已批准（2026-07-31）**。7 个功能 G0-G6。**实证结论：我们的落盘格式与官方 `/branch` 一致**（两份原生 fork 的算法指纹跳过 562/52 条旁支 ⇒ 排除线性切片 ⇒ 官方与我们同为祖先回溯）⇒ **不改落盘行为**，G0 只把结论钉成测试。**不引官方 SDK**：它是 Python 而我们零 Python 依赖、daemon 是 musl 静态二进制，且它与 TUI 落盘不一致。**G2 要收窄 `readonly_guard`（撞红线，待批）**，同时解掉 E50；本区取代 E52 |
| **local-as-remote/** | 本地 = 不走 ssh 的远端（含 Linux 平台）。落地 `doc/INVARIANTS.md` **§40** | **✅ 全区收官（2026-07-30）**：L5 平价对账表（121 命令 / 50 能力 / **20 不对称逐条带理由**）· L0 可构建半（三个 WebKitGTK 依赖本来就在、app 二进制 rc=0 ⇒ **计划最担心的「WebKitGTK 很痛」没发生**）· L1（POSIX 本地送法：同一 payload 只少 ssh 那一跳，**验收判据已机器化**）· **L2 原方案三条全否**（撞 §36 铁律 / 撞 R07 审计决定 / 收益是事实错误）⇒ 改做跨语言漂移点守卫 · L3a 本机账号枚举（**对账表真的结清一条**）· L4 两个 Linux CI/release job。**L3b 未做**（硬依赖 `account-zero`）。~~**Phase A 已落盘，等审批**。路线图 ④。L5 平价对账可先做；L0 是唯一可能推翻方向的一步（WebKitGTK） |
| **zero-poll-liveness/** | 判活从轮询改成内核事件（tmux hook + pidfd + inotify）。承 BACKLOG **E34** + 用户 2026-07-30「daemon 是能改的，要性能最佳且不要轮询」 | **✅ 全区交付签收（P0-P7 八个功能全部完成，2026-07-30）**（`4e7b100`/`81b22b8`/`03daf6c`/`de57453`/`5290768`/`7127357`/`3ccb6d6`/`d9f464c`/`64c2477`/`67653e2`/`75ee6f4`/`66134cc`）。**daemon 里 A/B 两条轮询都已删除，生产段零定时器**（`no_timer_guard.rs` 钉住，四条变异成立）。**四路事件实测**：强杀会话进程 → `session_removed` **~18ms**（原 ≤2s）· `kill-server` → 零会话帧 **27ms** · server 复活 **153ms** · 跨 cgroup SIGKILL **30ms** · **「多个中杀一个」→ 正向死亡帧 126ms（对照组：拆掉 hook 5042ms）**，原为 8s×2 ≈ 16s。P1 销掉 `INVARIANTS:408` 那条真 bug；`pidfd` 让 **PID 复用在机制上不存在**（唯一一条正确性改进）。wire 两处 additive、**不 bump `PROTO_VERSION`**；`BUILD_ID` → `p1r-event-liveness`。文档见 `doc/INVARIANTS.md` **§41**，**BACKLOG E34 已结案**（含对其原措辞三处订正） |


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
| 6 | **G-B** vendored bash 进 shellcheck + `run-tests.sh` 进 CI —— **✅ 完成签收（2026-07-30）**：shellcheck 覆盖 32→**36** 文件 + 覆盖面地板；`run-tests.sh` 首次运行 **171/171 全绿**、进 CI 带条数地板。**第一个发现是账本自己那个 `scripts/**` pattern 会恒红**（不开 globstar 时把 `scripts/test` 目录喂给 shellcheck）。⇒ **account-zero cc-acct-iso 半区（Z01/Z04/Z06/Z08）的网建好了** | gate-integrity | 否 |
| 6b | **Z08** `isolate` 迁移能力 —— **✅ 完成签收（2026-07-30）**：新 `ISOLATE` 动词（copy-then-unlink + CAS + 自检 + 回滚）· **`cmd_sync` 从 `RM` 改成私有化（修一个真实的数据丢失：此前加隔离项再 sync 会把每个账号那个文件直接删掉）** · `cmd_add` 认隔离集 · lockstep 完成。测试 171→**197**。**`share <item>` 排第二半**（反方向不丢数据）；**`migrate` 命令名被 `sync` 吸收**。⇒ **E36 API key 路线乙的技术前置就位** | account-zero | 否 |
| 6c | **Z06** 原生身份组成单点声明 —— **✅ 完成签收（2026-07-30）**：`NATIVE_IDENTITY` 表 + 四投影，`ISOLATE_SET`/`LEGACY_HOME_ITEMS`/`chmod 600` 从它派生（逐字相同）· **跨语言双写点守卫**钉住 daemon 那处独立 `loggedIn` 判定。**复测纠正了「6 处」**：`accounts.rs:49` 只是注释，真重复的只有两条且跨语言。**顺带浮出两处不一致并修掉**（`init`/`sync` 都只 chmod `.credentials.json`）。测试 197→**215**。`mcp.rs` 那条双写点未钉（第二半） | account-zero | 否 |
| 6d | **Z07** 版本钉 + 漂移检测 —— **✅ 完成签收（2026-07-30），BACKLOG E37 已销**：只读版本探测（解析 launcher 路径，**零执行 claude**）+ manifest additive `claudeVersionPinned` + `verify` 四条检测（**D1b 致命**：secret 泄漏进共享库，零误报，顺带覆盖此前漏查的 `.claude.json`。⚠ 「= 静默串号」那句理由**是错的，Z01 已订正**：隔离项从不被 symlink 出去）。测试 215→**231**。**成功标准 8 的达成范围已如实限定**（懒创建的项让「缺席」不可判定 ⇒ 只提示）。**给 Z01 留了硬提醒：账号 0 的 config dir 就是共享库 ⇒ D1b 对它必须有例外** | account-zero | 否 |
| 7 | **Z01** 账号 0 登记 + 可见 —— **✅ 完成签收（2026-07-30）**：账号 0 写时合成、**追加数组末尾**、`configDir` 键**省略**（空串是禁用拼法）· `verify` 改判（判据换成声明的 **`root` 字段**，**顺带订正了 Z07 那句「会被自动 symlink = 静默串号」的事实错误**）· `run 0`/`which`/`shellinit 0cc` 走 **`env -u`** · daemon `Option<String>` + 裸起会话归属账号 0 + 新动词 `--account-trust-zero` + 能力标记 `accountZeroAware` · monitor **`degraded_notice` 绝不静默降级**（旧 daemon / 旧 cc-acct-iso 分开说）。测试 231→**268** / daemon 141→**149** / monitor 611→**618** / vitest 837→**847**。**账号 0 暂不可选**（要 unset 注入形态，卡 `tabs.ts`）已登记 | account-zero | 否 |
| 8 | **Z04** 守卫 —— **✅ 完成签收（2026-07-30）**：计划的三条**逐条实测后全部订正**（「禁显式路径」工具禁不了 ⇒ 改成三个入口都点名「**这不是账号 0**」；「verify 新增检查」Z01 已以 vfail 落地 ⇒ 本轮补的是它的**盲区**；「删账号 0 特判」**早就有两道守卫**、只是零断言 ⇒ 补网）。**真正的洞**：纯 in-place 库里 `.claude.json` 的**真分裂完全不可见**（泛泛的模式警告不查实况 + Z01 那条 vfail 整条 skip + 「私有实体」还报绿灯）⇒ 新增「两份同时存在」检测，点名两个路径 + **凭据却是同一份**。变异 B 反而证出共享库有**两道**独立守卫。测试 268→**294** | account-zero | 否 |
| 9 | **Z02** 「未选账号」消失 —— **⚠ 部分交付（2026-07-30）**：交付了 `--base` 的**跨语言契约守卫**（monitor 发 `--base` ↔ `shared/ccm` 两处 `unset CLAUDE_CONFIG_DIR`，此前**全仓无人钉**，漂了就是「以为起账号 0、实际烧默认账号」的静默串号）。**★ 顺带订正 Z01 的一句错记录**：「launch-plan 今天只会 export」**是错的**，unset 注入形态两条渲染路各有一份；真正卡的是**选择链路**。**UI 三态化仍卡 `tabs.ts` 红线**（实测 20 处不是 ~14；`views/history.ts` 实测 0 处，计划列错），恢复步骤见 `features/Z02-PARTIAL.md` §6。**文案刻意不先改**（菜单还不能选账号 0 时改文案 = 把「没选」标成「账号 0」） | account-zero | **是：`tabs.ts` 红线（余下部分）** |
| 10 | **Z03** 账号 0 接既有能力 —— **⚠ 部分交付（2026-07-30）**：**(a) 用量探针支持账号 0 已做**（载荷 `unset CLAUDE_CONFIG_DIR; …`，**fail-closed 绝不退化成裸载荷**——裸载荷会被远端 rc 那句 `export CLAUDE_CONFIG_DIR=<默认>` 劫走、UI 却标成账号 0 的用量 = 静默串号）· 顺手把 `unset CLAUDE_CONFIG_DIR; ` 提成 `UNSET_CONFIG_DIR_PREFIX` 消掉一个正在长出来的双写点 · 换掉 Z01 的两处占位 · 订正 Rust 侧两处过时注释 · 订正计划行号（拒绝点是 `:70` 不是 `:74`）。**(b) 按会话切号切到账号 0 卡 `tabs.ts` 红线**，接在 `Z02-PARTIAL.md` §6 第 7 步。vitest 855→**860** | account-zero | **是：`tabs.ts` 红线（(b) 半）** |
| 11 | **Z05** rc 片段一键生成 —— **✅ 完成签收（2026-07-30）**：**片段从远端 `shellinit` 抓，不在 TS 里重写**（单一来源留在 bash，零新增双写点）· **fail-closed**：BEGIN/END 围栏都要在，半截片段绝不放行（贴进 rc 会让登录 shell 报错），两种失败给不同的可执行诊断 · 复用 T03 的 `buildPasteBlock`，**绝不代写 `~/.bashrc`** · 新增跨语言围栏守卫。**★ 撞出 T03 守卫的结构性盲区**：`accounts-section.ts` 现在族 A/族 B **两族都属**，二分覆盖不了 ⇒ 新增**族 AB** 并把判据从「有没有 `writeText`」换成**处数恰好等于已登记数**。四道既有守卫按设计触发（命令数 119→**120** ×3 + 待贴块族），顺带订正一处陈旧的测试标题。vitest 860→**866** / cargo 618→**620** | account-zero | 否 |
| 12 | **G-A** 八套真机套件断言地板 —— **✅ 完成签收（2026-07-30）**：新增 `e2e/assert-pass-floor.sh`（fail-closed：套件非零退出 / 抓不到 `合计 PASS=` / 低于地板，三种都红），8 步 CI 全部接上，**地板全是本地真跑的实测值**（26/44/12/15/13/21/14/7 = **152**）· **覆盖面地板双层**（调用行数 ≥8 + 逐对校验「套件名+地板值」，后者防地板被抹成 0）· **顺手消掉一个漂过两次的双写点**（步骤名里的「N 项」去掉，留地板一处）· `cc-spawn-uplift` 补上 `PASS=` 打印，**并揪出一条绕开 `chk` 的手搓判定**（输出 21 行而计数只到 20）。**★ 验收判据当场成立**：删一条断言 ⇒ 裸跑 rc=0（今天的 CI 会绿）/ 带地板 rc=1。shellcheck 36→**37** | gate-integrity | 否 |
| 13 | **G-C** 6 套 e2e 进 CI —— **✅ 完成签收（2026-07-30）· 并销掉 BACKLOG E41**：6 套统一加 `unset TMUX` + 短 `TMUX_TMPDIR`（**零调用点改动**，84 处裸 `tmux` 一个没改）。**★ E41 的归因订正**：病因不是「缺 `-L`」那个表面特征，而是**从 tmux 会话里跑时继承了 `$TMUX`** —— 只设 `TMUX_TMPDIR` 不 `unset TMUX` 时会话照样落默认 socket（实测踩到并当场清理）；另 socket 路径有 108 字节上限，`TMUX_TMPDIR` 必须短。**5 套进 CI 自带地板**（24/17/5/5/7），第 6 套 `graylight-suite` 拿到隔离但不进 CI（全链级要跑起 GUI app，证据：daemon 级兄弟同隔离下 5/5 绿）。**绕开了主计划记的「唯一技术不确定性」**（daemon 在 `e2e-tmux-rust` 就地编，不跨 job 传产物）。带地板套件 8→**13** / 152→**210** 条 | gate-integrity | 否 |
| ~~14~~ | **L5** 平价对账表 + 门禁（独立，可提前） | local-as-remote | 否 | **✅ 已交付（2026-07-30 全区收官）** |
| ~~15~~ | **L0** Linux 可构建可跑（**唯一可能推翻方向的一步**） | local-as-remote | 否 | **⚠ 只交付「可构建」那半**（三个 WebKitGTK 依赖本来就在、app 二进制 rc=0 ⇒ 那个「痛点」没出现，但只覆盖一台机器**不外推**）；**「起 app」待授权** |
| ~~16~~ | **L1** POSIX 本地 = 不走 ssh 的远端 | local-as-remote | 否 | **✅ 已交付（2026-07-30 全区收官）** |
| ~~17~~ | **L2** Windows 本地进 IR | local-as-remote | 否 | **✅ 已交付（2026-07-30 全区收官）** |
| ~~18~~ | **L3a** 本地账号枚举（只读，Rust 读 manifest） | local-as-remote | 否 | **⚠ 只交付枚举那半**（对账表 `accounts.list` 已结清）；**账号注入 / per-account model / UI 入口都还没有** |
| ~~19~~ | **L4** Linux 打包进 CI/release | local-as-remote | 否 | **✅ 已交付**（CI build job 会真跑；**release 的 `build-linux` 从未真跑过** —— 只在 tag 触发。AppImage 待后续） |
| **20** | **L3b** 本地账号管理（写） | local-as-remote | 依赖 account-zero 全部落地 | **未做** —— 本区自陈硬依赖 `account-zero` 的账号模型定稿；在一个正在变形的模型上做平台移植 = 反复批评过的形状 |
| **21** | ~~**E34 事件驱动的 tmux 存活信号**~~（用户点名「把轮询杀掉」）—— 升格为独立工作区、拆成 P0-P7 八个功能，**✅ 八个全部交付签收（2026-07-30）**。实测 ~18ms / 27ms / 153ms / 30ms / **126ms**（末者有对照组 5042ms）；两条轮询都已删、生产段零定时器；文档 `doc/INVARIANTS.md` §41，**E34 已结案** | **zero-poll-liveness** | **已给**：P4（装 hook 到活着的 tmux server）用户 2026-07-30 已授权；其余不需要。**原表两处已订正**：① 不再需要改 `shared/ccm` 本体（hook 由 **daemon** 装——只有 daemon 有「server 重启」这个时机）② §24 单写者不再是开放问题（所有新信号都汇进既有 `SessionChange{removed}` → emitter，零新写点） |

**为什么 C05 从 #4 提到 #2**（C01 的 Phase D 审计 I1 实测）：CI 的 `rust` job（跑 `cargo test`）
与 `frontend` job（跑 `tsc`）是**两次独立 checkout**——重新生成的产物在前者里被丢掉，
后者对着**已提交的**生成物做类型检查。审计变异实测：serde-rename 一个字段而**不重新生成**
⇒ `cargo` 绿、`tsc` 绿、5 条守卫绿、`npm` 819 全绿，**而生产里 UI 标签会变空**。
也就是说「改了 Rust 忘了重新生成」这个洞**让每一道现有门禁都保持绿色**。
C01 已加 `npm run check:types`（同一棵树里 regen → tsc → `git diff --exit-code`）作为可脚本化的
兜底，但**它还没进 CI**。若先做 C02/C03/C04，这个洞会被复制到 127 个 struct。

**为什么 G-B 插在整个 account-zero cc-acct-iso 半区之前**（2026-07-30 扩写）：Z01/Z04/**Z06/Z08**
都要改 `vendor/cc-acct-iso/scripts/`，而那 1348 行今天在 shellcheck 门禁之外、它自己的 424 行
测试从没跑过。**没有网不能改那个工具。** 而 Z06（把散在 6 处的身份组成收成单点声明）与
Z08（新增 `isolate`/`share`/`migrate`）是**结构性**改动，比 Z01 的「加一个 manifest 条目」大得多
⇒ 这条前置对它们比对 Z01 更硬。

**授权现状（2026-07-30 更新）**：动 `~/.claude/skills/cc-acct-iso/` 的授权**已给** ⇒ #6b-#8 不再被
授权挡住（只被 **G-B** 这个技术前置挡）。仍未松的只有 **`tabs.ts` 红线**（挡 #9 Z02、以及
rust-ts-boundary 的 C04b/C04d 批 8）。**另有一条时序约束**：动 `z`/`b` **真实账号目录**那一步
（Z08 的迁移落地、E36 写 key）用户说「先不要改，我现在在用 claude code」⇒ **等用户发话**；
但 Z08 的**能力本身**（代码 + 隔离 socket 上的验收）不受这条限制，可以先做完。
跳过的项在本表标注原因，并在收尾汇报里如实列出。
**绝不为了「跑完」而自行放宽用户设的红线或改用户家目录里的文件。**

**loop 停止条件**（任一命中即停，交回用户）：
撞到需要新决策的阻塞 · 同一步 ≥2 次失败 · 门禁红且非在途变异 · 全部完成（→ Phase G）

---

## ✅ 2026-07-30：四区全部走完，**Phase G 已做**

报告在仓根 **`PHASE-G-REPORT.md`**。三件最该看的：
1. **订正了 10 组计划断言**（多数是计划写下时就错、或被别的功能解决了而没同步）——「开工前复测」连续 23 轮每轮都抓到不符。
2. **主计划终账又抓到 8 处过时**（zero-poll P4-P7 / gate-integrity G-A·G-C / account-zero Z01 已交付却仍标未开工；外加 `local-as-remote` 的 L2 行被改成了 5 列）——**已全部修**。
3. **交了一半的逐条标清**（L0 起 app / L2 的 Windows 152 条 / L3a 的注入+UI / L4 的 release job 从未真跑 / zero-poll 待重部署），**Phase D 多 agent 审计与 `/full-audit` 是全区欠账**（常驻指令不开 agent）。

---

> 注：状态摘要仅导航用；权威状态恒以各区 `STATUS.md` 为准。新开工作区时在此追加一行。
