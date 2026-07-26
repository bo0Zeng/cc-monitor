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

### F11（doc/ 漂移，下轮）——已摸底，精确清单如下
- [ ] **ARCHITECTURE.md 补账号子系统**（现 0 处 account 提及）：account=一个 CLAUDE_CONFIG_DIR；lib.rs 注册 4 命令（见下）；前端 account-*.ts + account-restart.ts（换号优雅退出重编排）；daemon accounts_query.rs 纯只读。需读账号代码流写一段像样的子系统说明。
- [ ] **src/README.md + src-tauri/README.md 补账号**（各 0 处提及）：account-*.ts 前端族 + accounts.rs 后端命令。
- [ ] **INVARIANTS.md 上移仓库级事实**：从 `.claude/planned-build/account-ux/MASTERPLAN.md:122` 移入——本仓**无浅色主题**（`color-scheme: dark`、无 `prefers-color-scheme`）、`theme.ts` TOKENS 11 token。
- [ ] **README 快捷键**：action 数「26」需按 `src/keybindings/` registry 精确重数（初步 grep ~23 `{id:`，含未绑 toggle 口径不定——**实现时精确核**再改）；快捷键表补 `Acct`/`G`（若确有）。
- [ ] **doc/ 两份设计草案移 `proposals/`**：`doc/账号用量-usage抓取方案.md` + `doc/远端支持方案-agent查看器与代码全景图.md`（未写码草案）→ 建 `doc/proposals/` 移入 + 修引用。
- [ ] **.claude/planned-build/ 加索引 README**（列各工作区状态）。
- **STATE-MATRIX §2 = 非问题（审计过标）**：§2 明确「只收签名含 `State<...>` 的 IPC 命令」，4 个账号命令**全无 State**（`list_remote_accounts(origin: String)` 等 stateless）→ 按 §2 自身定义**正确排除**、无需登记。F11 不动 §2；在此记档缘由。

## 不做什么
- **不 bump 版本、不发版**（红线）——只把 README 版本改到与既有 3.2.0 一致。
- 不重写 README 版本历史大 blob（只改版本号/计数/CI 描述/last-release 指针 + 加账号小节）。
- 不动代码。

## 审计结果（F10）
- **D（低风险主线程自审 + 文档交叉核对）**：版本据 package.json/tauri.conf/Cargo（皆 3.2.0）；账号内容据 CHANGELOG [3.2.0]；CI job 数据 ci.yml（rust/frontend/daemon/e2e-smoke=4）；链接全解析、无悬空。无代码改动。
- **E**：F10 doc-only 无耦合；F11 续修 doc/ 子系统。主计划自洽。

## 签收
- [x] **F10（README）过 D+E+F**（低风险自审）：版本 3.2.0 一致 + CI 四 job/门禁同步 + 多账号小节 + RELEASING 链接；无悬空、未 bump。
- [ ] F11（doc/ 漂移）过 D+E+F
