# F10+F11 — 文档修版本/漂移

> 纯文档、低风险。轻量流程（摸底→交叉核对实况→改→自审）。分两步：**F10=README（本轮）** → **F11=doc/ 子系统漂移（下轮）**。

## 背景（摸底交叉核对）
- **版本漂移（I2）**：README.md/.en.md 头写 `v3.0.0`，实际 package.json/tauri.conf/Cargo 全 **3.2.0**（memory：v3.2.0 已发版）。
- **CI/测试描述漂移**：README 写「CI 三 job + 308 后端 + 69 daemon + 308 DOM」，实际 F08/F09 后=**4 job**（rust〔含 `-p code-picture-core`〕/frontend〔+eslint/stylelint/coverage〕/daemon/e2e-smoke）、src-tauri ~365、vitest 595、code-picture-core 25。
- **账号功能缺失（I3）**：README 无「多账号（#68/#69）」——v3.2.0 头号功能未提。
- **RELEASING 链接**：README.md 有，README.en.md **缺**。
- 悬空链接：READMEs 无悬空 .md 链接（已 grep 核）。

## DoD
### F10（README，本轮）
- [x] 版本 `v3.0.0`→`v3.2.0`：README.md 头(5)+脚(267)、README.en.md 头(5)。核：无残留 3.0.0。
- [x] CI/测试描述同步：三 job→四 job + eslint/stylelint 顾问 + 覆盖率地板 + `-p code-picture-core` + e2e 冒烟；计数刷新（365/595/25）。两语言。
- [x] 补「多账号（#68/#69）」功能小节（隔离+共享/账号组/按会话选号/优雅退出/部署向导/daemon 版本注）。两语言，内容对齐 CHANGELOG v3.2.0。
- [x] README.en.md docs 表补 RELEASING 链接（.md 已有）。
- [x] 核：无悬空链接、版本处处 3.2.0。**未 bump 版本**（只文档匹配既有 3.2.0，红线守）。

### F11（doc/ 漂移，本轮完成）
- [x] **ARCHITECTURE.md 补账号子系统**（0→11 提及）：backend §2 树加 `accounts.rs`；frontend §2 树加账号层（account-chip/commands/restart/color + acct-deploy）；§5 加「账号子系统：隔离又同步（A2–A6）」小节（模型/只读边界/前端族/withAccount vs restart 分离 + 失败语义）。
- [x] **src/README + src-tauri/README 补账号**：src 模块分工加 account-*.ts 行；src-tauri IPC 清单加 3 账号只读命令行（标注 stateless）。
- [x] **INVARIANTS.md §32**：仓库级事实「本仓只有暗色主题」（`color-scheme: dark`、无 `prefers-color-scheme`、TOKENS 固定暗色调色板；**实际 TOKENS=15 项非审计说的 11**），从 account-ux MASTERPLAN 上移沉淀。
- [x] **README 快捷键**：action 数 **26→28**（精确重数 actions.ts ACTIONS：28 个 id）；加 **G**（打开/关闭代码全景，`app.toggle-panorama` default KeyG，原缺表）；未绑数「2」→「6」（含 **Acct** 账号切号/对齐——align 破坏性故意 default:null）。
- [x] **.claude/planned-build/README.md** 建工作区索引（7 区一句话状态）。
- [~] **doc/ 两份草案移 `proposals/` = 不做**（审计建议不成立）：`账号用量-usage抓取方案` 已实现（`usage_query.rs`/usage-pivot）、`远端支持方案-agent查看器与代码全景图` 已实现（SSH 远端 + vendor code-picture）——是**已落地的历史设计文档**、非未建 proposal；移入「proposals/」会误标 + 断 `account-isolation/STATUS.md` 引用。记档不动。
- **STATE-MATRIX §2 = 非问题（审计过标，未动）**：§2 明确「只收签名含 `State<...>` 的 IPC 命令」，账号命令**全 stateless**（`list_remote_accounts(origin: String)` 等）→ 按 §2 自身定义正确排除。

## 不做什么
- **不 bump 版本、不发版**（红线）——只把 README 版本改到与既有 3.2.0 一致。
- 不重写 README 版本历史大 blob（只改版本号/计数/CI 描述/last-release 指针 + 加账号小节）。
- 不动代码。

## 审计结果（F10）
- **D（低风险主线程自审 + 文档交叉核对）**：版本据 package.json/tauri.conf/Cargo（皆 3.2.0）；账号内容据 CHANGELOG [3.2.0]；CI job 数据 ci.yml（rust/frontend/daemon/e2e-smoke=4）；链接全解析、无悬空。无代码改动。
- **E**：F10 doc-only 无耦合；F11 续修 doc/ 子系统。主计划自洽。

## 签收
- [x] **F10（README）过 D+E+F**（低风险自审）：版本 3.2.0 一致 + CI 四 job/门禁同步 + 多账号小节 + RELEASING 链接；无悬空、未 bump。
- [x] **F11（doc/ 漂移）过 D+E+F**（低风险自审 + 文档交叉核对代码）：ARCHITECTURE 账号子系统（0→11）+ 双子 README 补账号 + INVARIANTS §32 暗色主题事实 + README action 26→28/加 G/未绑 2→6 + planned-build 索引 README。**修正审计数字**（TOKENS 15≠11、action 28≠26/30）。**⑤ 移草案不做**（草案已实现=历史设计文档非 proposal）+ **STATE-MATRIX §2 不动**（账号命令 stateless）。全无悬空链接、无代码改动。
