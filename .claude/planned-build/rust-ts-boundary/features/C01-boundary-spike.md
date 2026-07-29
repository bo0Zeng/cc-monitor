# C01 — 样板：一条命令走通全链（`ts-rs` v12）

> 主计划：`../MASTERPLAN.md`（用户 2026-07-29 已批；技术选型订正见其 §8）
> **本功能的验收不是「跑通了」，是「变异会红」。**

## 1. 目标（一句话）

用**一个**命令证明「Rust 是源、TS 是产物」这条链**真的有牙**：
改 Rust，`npx tsc --noEmit` 必须报错。今天不会。

## 2. 选中的命令：`get_data_paths`

| 判据 | 为什么它满足 |
|---|---|
| 有 `camelCase` 重命名 | `DataPathInfo`（`data_paths.rs:23`）与 `DataPathsResponse`（`:41`）**都带** `#[serde(rename_all = "camelCase")]`。**这是选它的首要理由**——`ts-rs` 认 serde 属性是选它而非 `typeshare` 的主要依据，必须在样板里就验 |
| 嵌套结构 | `DataPathsResponse` 内含 `DataPathInfo` ⇒ 验嵌套导出 |
| 主流返回形态 | `Result<DataPathsResponse, String>`（本仓几乎所有命令都是这个） |
| TS 消费点少且正是要替掉的形态 | **恰好一个**：`src/settings/data-section.ts:87` `await invoke<DataPathsResponse>("get_data_paths")` —— 手写类型参数，一改 Rust 就静默失配 |
| 不在冲突区 | 不是 `accounts.ts`（归 account-zero）/ `sftp/paths.ts`（归 C03）/ `launch-plan.ts`（本区不碰） |
| 低风险 | 只读探测（`collect` + `spawn_blocking`），无写盘、无远端、无 tmux |

## 3. DoD（可验证，逐条勾）

- [ ] `ts-rs` v12.0.1 进 `src-tauri/Cargo.toml`（**dev 侧亦可**——导出走 `cargo test`，见步骤 3）
- [ ] `DataPathInfo` / `DataPathsResponse` 派生 `TS`，生成物落 `src/generated/`
- [ ] 生成物里的字段名是 **camelCase**（不是 snake_case）——**这条单独断言**
- [ ] `src/settings/data-section.ts` 改成 import 生成的类型，**不再手写类型参数**
- [ ] **变异 A**：删掉 `DataPathInfo` 的一个字段 → 重新生成 → `npx tsc --noEmit` **报错**
- [ ] **变异 B**：把某字段改名（Rust 侧）→ 重新生成 → `tsc` **报错**
- [ ] **反向自检**：不做任何变异时 `tsc` **0 错**（否则说明是别的原因在红）
- [ ] `Result<_, String>` 的**调用形状不变**（仍是 `await` + `try/catch` 拿字符串）
      —— 若 `ts-rs` 迫使形状变，按主计划 §6 开放问题 1：加一层保持 throw 语义的包装
- [ ] 生成物有 `// @generated` 头 + 「不许手改」说明；**进 git**
- [ ] 全门禁绿且**数字不降**：cargo **536** · code-picture-core **25** · npm **814/53 files** ·
      clippy 0 · tsc 0 · npm audit rc=0 · shellcheck 0 · exec-bit rc=0
- [ ] **8 套真机套件 152 条全绿**（本工作区硬判据：纯类型层改动 ⇒ 行为逐字节不变）

**明确不做**（防范围蔓延）：不碰其余 118 个命令 · 不碰 127 个其余 struct ·
不做 CI 门禁（那是 C05）· 不动大整数（那是 C03）· 不建手写 `invoke` 包装层
（C01 只证明类型链有牙；包装层归 C04）

## 4. 与主计划对接

- **共享面 6（`ci.yml`）本功能不碰** —— `gate-integrity` 优先，C05 才追加。
- **共享面 4（IR 类型）不碰。**
- 新增共享面：**`src/generated/`**（本功能建立，C02/C03/C04 都往里加）。
  登记进主计划 §3 —— 最终形态：一个目录、`// @generated`、`linguist-generated`、进 git、
  **任何手改都应被 C05 的门禁抓到**。

## 5. 逐条实现步骤

1. **先摸清 `ts-rs` v12 的导出机制**（它靠 `cargo test` 产出，路径由
   `TS_RS_EXPORT_DIR` 或 `#[ts(export_to = "…")]` 控制）。**验证点**：能把两个 struct
   导到 `src/generated/` 且文件名可控。
2. **`cargo add ts-rs`**（先只加，不派生），确认 `cargo build` + `cargo test` 仍 536 绿。
   **验证点**：加依赖本身零影响。
3. **给两个 struct 加 `#[derive(TS)]` + `#[ts(export)]`**，跑导出。
   **验证点**：`src/generated/` 出现文件，且字段名是 camelCase。**camelCase 不对就停下**——
   那说明 `ts-rs` 没认 serde 属性，选型前提就没了（回主计划 §8 重议）。
4. **改 `data-section.ts:87`** 用生成的类型。**验证点**：`tsc` 0 错。
5. **变异 A + B**，各自先 `git diff` 确认落位、再确认 `cargo build` 过，**然后**判 `tsc` 的色。
   **验证点**：两次都红，且报错信息指向 `data-section.ts`。**不红就停下**——
   本功能的全部意义就是这一步。
6. **还原变异**，跑全门禁 + 8 套真机套件。
7. **Phase D 审计**（低风险 ⇒ 1 个综合 agent 或 `/code-review`）：
   重点问「生成物是否真的被消费」「有没有引入运行时行为变化」。
8. **Phase E/F**：把 `src/generated/` 登记进主计划 §3；更新 STATUS；commit。

## 6. 测试策略

- **主要判据是变异**（步骤 5），不是新增断言数。
- 加一条**长期守卫**：断言 `src/generated/` 下的文件含 `@generated` 且
  `data-section.ts` 不再出现 `invoke<DataPathsResponse>` 这种手写类型参数形态。
  **守卫范围只覆盖这一个文件**——不许写成「全仓不许有 `invoke<`」，
  那会假红（其余 118 个命令还没迁），**而假红的守卫会被人关掉**（本会话栽过三次）。
- **不写源码文本扫描当行为测试**：`tsc` 报不报错本身就是行为判据，这里天然不需要代理指标。

## 7. 代码审计结果（Phase D）

（待填）

## 8. 工程审计结果（Phase E）

（待填）

## 9. 签收

- [ ] 通过代码审计
- [ ] 通过工程审计
- [ ] 主计划已据此更新
