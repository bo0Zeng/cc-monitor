# F08 — 质量门禁 / quality-gates

> 分两子轮：**F08a=红线机器护栏（Rust 测，本轮）** → **F08b=前端 lint/覆盖率工具（下轮）**。
> 依据主计划 §3 账本 I7（daemon 只读/零改）、I8（TMUX_LS_FMT 双写点）、CI 共享面终态行。

## 背景（现状摸底）
- CI 已有 `.github/workflows/ci.yml`：rust(fmt --check 强制 / clippy warn-only / cargo test) · frontend(npm audit prod / tsc / npm test / vite build) · daemon(Linux fmt/clippy/test)。
- **无** eslint/prettier/stylelint；vitest 无覆盖率（未装 @vitest/coverage-v8）。
- `TMUX_LS_FMT` 逐字双写：`src-tauri/src/tmux.rs:22` ↔ `remote-daemon-proto/src/watcher.rs:69`（分属两独立 crate，无法共享 const）。
- daemon 生产代码经核**已是只读**：唯一 `.write_all` 是 `main.rs:324` 写 **stdout**（wire 协议，非 FS）；所有 `fs::write/create_dir/remove_*` 都在各文件 `#[cfg(test)]` 测试块内（temp 夹具）。

## DoD（分步）
### F08a（本轮）——红线机器护栏（纯 Rust 测，跑在既有 cargo test job，零新增 CI YAML）
- [x] **步骤 1（TMUX_LS_FMT 双写点断言）**：`tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`——`include_str!` daemon watcher.rs，把本 crate `TMUX_LS_FMT` 真 TAB 折回 `\t`、**锚定 `const TMUX_LS_FMT: &str = "…"` 定义行**断言（非裸字面量，避注释假阴性）。**双向**：改 monitor 或 daemon 任一侧忘同步→编译期红。变异验证过（改本 const 加一列→红、还原→绿）。
- [x] **步骤 2（daemon 只读机器护栏）**：`remote-daemon-proto/src/readonly_guard.rs`（整体 `#[cfg(test)]`，生产构建为空）——`read_dir(CARGO_MANIFEST_DIR/src)` 遍历 `*.rs`，`strip_cfg_test`（花括号配平剥所有 cfg(test) 块，因 main.rs 测试块在文件中部）后断言生产代码不含 FS 变更（`fs::write`/`create_dir`/`remove_*`/`rename`/`copy`/`hard_link`/`soft_link`/`File::create`/`File::options`/`OpenOptions`）。stdout `write_all` 非 `fs::`、放行。跳过护栏文件自身（其模式数组含这些子串）。变异验证过（生产 fn 插 `fs::write`→红、还原→绿）。**已知局限**（记档于源）：括号配平不识别字符串内花括号，偏保守=假阳性 fail-closed。
- [x] **验证**：src-tauri（365 测）+ remote-daemon-proto 全绿 + fmt --check 0 + daemon 非测试 build 0；两护栏各自变异验证。

### F08b（下轮）——前端 lint/覆盖率
- [ ] **步骤 3（eslint flat，warn-only 基线）**：`eslint.config.js`(flat) + typescript-eslint；`npm run lint`；CI 加步骤但 **warn-only / --max-warnings 基线**（同 clippy 不强制，**不追一次清零**）。
- [ ] **步骤 4（stylelint）**：`styles.css` 单文件；`stylelint-config-standard`；基线化（现有告警设 baseline、不强制清零）。
- [ ] **步骤 5（覆盖率 reporting + 温和地板）**：装 `@vitest/coverage-v8`；`npm run test:dom -- --coverage`；**不设 85% 全局**（现实：`*.test.ts` 的 tsx node 测不计入 vitest 覆盖，只计 `*.vitest.ts`），改设「当前值地板棘轮」+ 核心 DOM 模块 per-file 目标。CI 出报告。

## 不做什么（防蔓延）
- **prettier 不做（或仅 advisory）**：全量重排会对既有刻意风格造成巨大 churn，违「不追一次清零」；风格靠 review 保持。若加也只 `--check` 不写。
- **不动 daemon 行为**：只加只读测试（红线 I7 明确允许）。
- **不改 TMUX_LS_FMT 任一侧**（红线 I8）——本功能只**加断言**锁住它，不动格式串。
- 不把 lint 一次性清零、不为过 lint 大改既有代码。
- 不 push/发版/bump。

## 与主计划对接（共享面）
- `.github/workflows/ci.yml` + `package.json`（账本行「终态 job：rust/frontend(+lint+coverage)/daemon + 双写点断言」）：F08a 用 Rust 测落「双写点断言 + daemon 只读护栏」两项（进既有 cargo test job，不加新 YAML）；F08b 加 lint/coverage npm 脚本 + CI 步骤。**朝账本终态实现，不打补丁。**
- I7（daemon 只读）：护栏是其机器化守护。I8（TMUX_LS_FMT）：断言是其机器化守护。

## 审计结果
- **代码审计(D)：F08a（低风险主线程自审——纯 test-only 增量、零生产行为改动、已变异验证）**：
  - *正确性*：两护栏均变异验证（drift→红 / 生产 fs::write→红）。硬化：TMUX_LS_FMT 断言从「裸字面量 contains」改为「锚定 const 定义行」消除注释假阴性；daemon 括号配平局限记档（偏保守=假阳性 fail-closed、不塞词法器）。
  - *计划符合度*：严格 = 步骤 1+2，纯 Rust 测、零 CI YAML 改动。
  - *架构/红线*：daemon 只加 `#[cfg(test)]` 只读测试（红线 I7 明确允许「只准加只读测试/门禁」）；TMUX_LS_FMT 只**断言**不改（红线 I8）；无 bashrc/发版/轮询。跨 crate `include_str!` 是**有意的**编译期耦合——把双写点契约显式化（daemon 源移位则 monitor 测编译失败，正是要的信号）。
- **工程审计(E)**：两护栏落成 **cargo 测**（跑在既有 rust + daemon CI job **且** 本地），比账本原「CI 步骤断言」措辞**更优**（无 YAML 脆弱性、本地即验）。无耦合债；主计划自洽。账本 CI 行终态微调（F08a 部分以 cargo 测落地，F08b 补 lint/coverage 的 npm+CI 步骤）。

## 签收
- [x] **F08a（两护栏）过 D+E+F**（低风险主线程自审 + 双变异验证）：TMUX_LS_FMT 双写点断言（锚定 const 行）+ daemon 只读护栏（strip cfg(test) + FS 变更模式）。src-tauri 365 / daemon 全绿 / fmt 0 / build 0。红线 I7/I8 机器化守护到位。
- [ ] F08b（前端 lint/coverage）过 D+E+F
