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

### F11（doc/ 漂移，下轮）
- [ ] ARCHITECTURE 补账号子系统；STATE-MATRIX 4 命令；INVARIANTS color-scheme 上移；子 README（src/scripts/src-tauri/remote-daemon-proto）核漂移；文档索引；CI actions 数（doc 里描述 job 数/门禁与 ci.yml 对齐）。

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
