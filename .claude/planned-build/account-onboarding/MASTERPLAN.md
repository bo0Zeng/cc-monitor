# MASTERPLAN — 账号体验重做（新用户一站式、无歧义）

分支 account-ux。目标：把多账号从「power-user + 假设已装 cc-acct-iso + 13 个外露概念」重做成「新用户能直接上手、概念只剩『账号名 + 切换』、一站式无歧义」。北极星 = CCSwitcher 的 UX（概念最小、当前号常显、切回免重登、加号内联触发登录）+ 保留 cc-acct-iso 的隔离底子（比 cc-switch 强，能并发不互踢）。

## 0. 现状事实（三路调研勘定，带证据）

- **起会话机制本身是对的**：`export CLAUDE_CONFIG_DIR='<dir>'; unset <嵌套>; claude`（`src/remote-launch.ts:76-82,227-247`）。resume 同前缀（:99-189）。账号解析 `withAccount`/`resolveFollowAccount` 粘性：会话 lastAccount → 当前账号 → 基座（`src/accounts.ts:167-179,446-494`）。**底层不动。**
- **`cc` 与账号无关**：`cc`=`/usr/bin/cc`（C 编译器）；cc-monitor 装的本机 PS `cc` 函数只做本地监控绑定、不设 CLAUDE_CONFIG_DIR（`profile_installer.rs:157-166`）。不存在 per-account launcher。
- **断点 1（最致命）**：cc-monitor **不部署 cc-acct-iso**（daemon 是 `sftp.rs:365 deploy_remote_daemon` 一键推的，cc-acct-iso 无等价路径），完全假设远端已装 + manifest 已建（`accounts-section.ts:171-173`）。没装 → 面板按钮弹终端 command not found，零兜底。
- **断点 2**：纯终端起号无一站式入口（`remote-launch` 只在 GUI 内注入；终端要用户自己 export 或自配 cc-acct-iso rc，`<name>cc` 在用户 bashrc 里仅是半接线注释、`zcc` 当前未定义）。
- **13 个外露概念**（高摩擦：configDir 手贴、manifest 心智、cc-acct-iso 六子命令 + 手改 rc、mismatch 破坏性对齐、基座 vs 当前账号、verify/sync 实现名词上按钮）。
- **usage**：daemon 已有 `--usage`（每会话一行 camelCase JSON，`usage.rs:491`）；plan 额度（5h/周窗口 %）唯一路 = 渲染 `/usage` + capture-pane（issue #73）。per-account 需在各账号 CLAUDE_CONFIG_DIR 下取。

## 1. 北极星与总原则

1. 用户心智只留 **「账号名 + 当前高亮 + 点一下切」**；manifest/configDir/skill/verify/sync/mismatch/基座 全部藏进幕后或排障区。
2. **当前号永远无歧义可见**，且「选中态 ≠ 已激活」不混。
3. **加号 = 内联触发登录**（逼近一键，诚实天花板：弹终端自动跑 add 并落进 `/login`，因 CC 登录本就交互式，无法全 headless）。
4. **切回旧号免重登**（已有凭据快照，保留）。
5. **新机零门槛**：检测 + 一键部署 cc-acct-iso（补齐断点 1）。
6. **诚实标「何时生效」**：切号只影响新会话 / 在跑会话不变（撤掉破坏性「对齐」到命令面板）。
7. 红线：**不碰 ~/.bashrc**（rc 片段只生成+复制、绝不代写）、不新增轮询、不用 emoji、daemon 起会话机制零改（读侧 accounts.rs 不动）、git commit 无 Co-Authored-By。

## 2. 概念收敛（13 → 3）

| 保留（用户可见） | 藏进幕后/排障 | 删/降级 |
|---|---|---|
| 账号名 + 头像色 | configDir（仅「复制起号命令」内部用） | mismatch 徽章 ⚠k/⇄（→命令面板） |
| 当前账号（★，常显） | manifest 路径（→排障折叠） | 「对齐/批量对齐」主 UI |
| 登录状态（已登录/去登录） | verify/sync（→排障折叠） | 「当前工作账号 vs 默认」双名（统一叫「当前账号」） |
| 额度用量（新） | cc-acct-iso 子命令（→排障/部署内部） | 基座话术简化为「未指定」 |

## 3. ★共享面账本（防补丁，定最终形态）

| 共享面 | 现状 | 最终形态 | 涉及功能 |
|---|---|---|---|
| `src/settings/accounts-section.ts`（522 行面板） | 卡片+维护区+向导混一坨，manifest/configDir 外露 | 卡片列表（名/色/登录态/用量）+ 加号 + `<details>排障>`（verify/sync/manifest/部署）；未装态 → 一键部署 | F3/F4/F5/F7 |
| `src/tabs.ts` 右键菜单 + 徽章 | 徽章带 ⇄ 对齐；无切号入口 | 右键「账号」子菜单（列账号/★当前/切号）；徽章保留「信息才显」但去掉 ⇄ | F1/F2 |
| `src/account-chip.ts` 状态栏 chip | 带 selectDefault 操作菜单 + ⚠k 对齐计数 | **只读**：显示当前账号 + 用量摘要；点击 → 打开设置；去掉操作菜单与 ⚠k | F1/F2/F7 |
| `src/account-commands.ts` 命令面板 | 已有 align-active/align-all | 保留对齐（唯一入口）+ 新增「切换当前账号」「起某号终端」 | F2/F6 |
| `src/accounts.ts` store | detectAccountMismatch/alignable/badge | 逻辑保留（命令面板仍用）；「当前工作账号」对外文案统一「当前账号」 | F1/F2 |
| `src-tauri` 新 IPC | 有 deploy_remote_daemon | 新增 `deploy_remote_acct_iso`（sftp 推 skill + 装，仿 daemon）；`account_usage`（取 per-account 用量） | F5/F7 |

## 4. 功能清单 + 顺序 + DoD

顺序按「被依赖先做 / 断点优先 / 低耦合并行」。

### F5 — cc-acct-iso 一键部署 + 存在性检测（**先做，断点 1，其余功能的地基**）
- 新 IPC `deploy_remote_acct_iso`（`src-tauri/src/sftp.rs`，仿 `deploy_remote_daemon`）：sftp 推 `~/.claude/skills/cc-acct-iso/`（脚本 + lib.sh + install.sh）到远端，跑 `cc-acct-iso-install.sh`（软链到 `~/.local/bin`，**不碰 rc**）。
- 面板未启用态先探测「远端有没有 cc-acct-iso」：无 → 显「一键部署」按钮（而非直接给 init 命令让它 command not found）；有但无 manifest → 走现有 init 向导。
- **DoD**：全新远端（无 cc-acct-iso）点一次「部署」→ 命令可用 → 再走 init 向导成功；tsc0/vitest 绿；部署路径过 `is_safe_remote_*` 守卫；无 rc 改动。

### F1 — 全局/局部两个切号入口（★用户确认的干净拆分）
- **全局切当前账号 → 状态栏 chip 极简下拉（CCSwitcher 式）**：chip 点开 = 小账号列表，★=当前，点一下 = 设为当前账号（全局，改 config.json defaultName，影响以后新会话）。chip **保留操作**（非只读），但收敛成"极简下拉切号 + 用量摘要"，去掉 ⚠k 对齐计数。
- **单会话切号（局部）→ tab 右键「此会话切到账号 X」**：per-session，= 用账号 X 重启此 tab 的会话（复用 account-restart，吸收旧 align 的直觉版）。**tab 右键不做全局切号**。
- 「当前工作账号」全部文案 → 「当前账号」。
- **DoD**：chip 下拉可全局切号并高亮当前；tab 右键「此会话切到 X」重启对齐单会话；两入口语义不混（全局 vs 局部）；切号 toast 诚实标影响范围；vitest 覆盖 chip 下拉切号 + tab 右键 per-session 切号。

### F2 — 撤对齐主 UI，降级命令面板
- 移除 tab 徽章 ⇄、chip ⚠k 计数、批量对齐主 UI。**单会话对齐由 F1 的 tab 右键「此会话切到 X」承接**（更直觉）；**批量对齐 `alignAll` 仅留命令面板**（`account-commands.ts` 已有，保留）。
- 主 UI 用一句诚实提示替代（「切号只影响新会话；在跑的会话要换号 → tab 右键『此会话切到 X』，或命令面板批量对齐」）。
- `account-restart.ts` 逻辑保留（tab 右键 per-session + 命令面板批量 复用）。
- **DoD**：主界面无 mismatch 概念外露（无 ⚠k/⇄）；命令面板批量对齐仍可用；vitest 调整（删徽章 ⇄ 断言、留命令面板断言）。

### F3 — 账号面板砍成卡片 + 排障折叠
- accounts-section.ts 重构：卡片列表（头像/名/登录态/用量），加号表单保留；manifest 路径 + verify + sync + 部署 全收进 `<details>排障（默认折叠）>`。
- 去掉 configDir 长路径常驻显示（移进卡片「···」或排障；保留「复制路径」但降权）。
- **DoD**：稳态面板只见「卡片 + 加号」；排障折叠内含 verify/sync/manifest/部署；vitest 覆盖折叠默认态 + 卡片渲染。

### F4 — 加号一键化
- 加号流：名（+ 可选快照）→ 一步弹终端自动 `cc-acct-iso add … --apply` 完直接落 `/login`（现在是 add 完要用户再找登录入口）。
- 无快照路径：内联说明「弹出的终端里 /login 即可」；有快照：免重登直接可用。
- **DoD**：加号 ≤2 步到「终端里 /login」；快照路径免重登；danger 二次确认保留；vitest 覆盖命令构造。

### F6 — 终端起号理顺 + `cc` 澄清（**不碰 rc**）
- 每张账号卡片给「复制『在终端起此号』命令」→ 复制 `CLAUDE_CONFIG_DIR='<dir>' claude`（一站式终端入口，零心智）。
- 「装 shell 快捷方式」入口（排障区）：一键复制 `cc-acct-iso shellinit` 产出的 rc 片段（`export` + `<name>cc` 函数）+ 明确「贴进 ~/.bashrc，工具不代写」。修正现状半接线：让 `<name>cc` 真能用。
- 文案/文档澄清「`cc` 是编译器、与账号无关；起 Claude 用 `claude` / `<name>cc`」。
- **DoD**：卡片一键拿到可直接粘的起号命令；rc 片段可复制且正确；无自动改 rc；文档更新。

### F7 — 账号额度用量集成（★用户确认：一步到位做 plan 窗口%）
- 新 IPC `account_usage`：per-account 取 **plan 额度窗口%**（5h/周窗口剩余%+重置倒计时）——走 issue #73 路径 = 在各账号 `CLAUDE_CONFIG_DIR` 下渲染 `/usage` + capture-pane 解析。daemon `--usage` 现成 JSON 作为「本工具会话累计」补充同显。
- 卡片显每号用量摘要（窗口%/重置倒计时/累计），CCSwitcher 式；chip 显当前号窗口% 摘要。
- **不新增轮询**：/usage capture 按需触发（打开面板/手动刷新），不加独立定时器；capture 走一次性隐藏会话，用完即清（诚实标：这是较重操作）。
- **DoD**：每张卡片显 plan 窗口 %（拿不到诚实留白 + 说明为何）；chip 显当前号窗口%；capture 会话不残留、不新增轮询；关联/推进 issue #73/#52。
- **风险标注**：/usage capture-pane 是 F7 里唯一"重"的一环（要起隐藏 claude 会话、渲染、抓屏、解析文本），格式随 CC 版本可能漂移 → Phase B 要先做一次真机 spike 确认可解析，再定实现。

## 5. 测试约定
沿用现有门禁：tsc 0、vitest 全绿（每功能新增/调整用例）、`cargo test`（F5/F7 动 Rust）、prod build。纯函数（命令构造、用量解析、账号模型）单测优先。每功能 Phase C 收尾跑门禁并回盘核实（pipefail，别信内联回显）。

## 6. 开放设计点 —— 已由用户拍板（2026-07-27）
1. **F1 切号语义** → **全局/局部拆开**：全局切当前账号走**状态栏 chip 极简下拉**（CCSwitcher 式）；单会话切号走 **tab 右键「此会话切到 X」**（per-session 重启）。tab 右键不做全局。
2. **F7 用量深度** → **一步到位做 plan 额度窗口%**（/usage capture-pane，触 issue #73）+ daemon 累计补充。
3. **chip 去留** → **保留极简下拉切号**（全局入口），非只读；去掉 ⚠k 对齐计数。

## 变更记录
- 2026-07-27 建 Phase A 主计划（三路调研勘定现状 + 用户三决策：对齐降级命令面板 / verify·sync 收排障 / 先审批）。
- 2026-07-27 用户拍板三开放设计点：F1 全局(chip 下拉)/局部(tab 右键)拆开、F7 一步到位窗口%、chip 保留极简下拉。主计划**已审批**，进 Phase B（F5 起）。
- 2026-07-27 **F5 完成签收**（B→F）：vendor 内嵌 cc-acct-iso + deploy_remote_acct_iso/check_remote_acct_iso IPC + 前端检测→一键部署。D 审计两 agent 无阻塞、同报 I1（install 静默失败死锁）已修 + S1/S2/S4/S5 修。门禁 tsc0/vitest600/cargo check0/cargo test368。账本「新 IPC」项落最终形态，与 F3 正交。commit 落盘（9d3a7e6）。下一个：F1。
- 2026-07-27 **F1 完成签收**（B→F）：chip 去 ⚠k 成纯全局切换器（CCSwitcher 式下拉）+ tab 右键 relabel「把此会话切到账号 X」per-session 切号 + 全局 rename 当前工作账号→当前账号（13 文件）。D 审计无阻塞（I1 文案一致/S2 注释/S3 补正路覆盖/S4 平行 已修）。门禁 tsc0/vitest598。**移交 F2**：`countAccountMismatches` 成死代码（随 chip ⚠k 删除），F2 撤 mismatch 主 UI 时一并清。commit 落盘。下一个：F2。
