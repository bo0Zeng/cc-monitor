# C02 — 事件半边先迁完

> 主计划：`../MASTERPLAN.md`（用户已批；选型见 §8：`ts-rs` v12）
> 前置：C01 已闭环（`20c9dd7`，变异 A/B 双向验收通过）

## 1. 为什么先迁事件半边

主计划 §0.0 的判断：事件半边的纪律比命令半边好，改动面最小、信噪比最高。
**Phase B 实测确认了这一点，但也修正了两处**（见 §2）。

## 2. 实测盘点（2026-07-29 Phase B）

### `src-tauri/src/bridge.rs`

| 项 | 实测 |
|---|---|
| 事件名常量 | **10 个** `pub const`：`jsonl-line` · `jsonl-batch` · `session-ended` · `task-update` · `session-activity` · `session-started` · `session-idle` · `remote-session-added` · `remote-health` · `snapshot-inflight` |
| payload struct | **11 个**（比事件名多一个）：`JsonlLine` · `JsonlBatch` · `SessionEnded` · `SessionIdle` · `SessionStarted` · `RemoteSessionAdded` · **`FrontendReady`（`Deserialize`，方向相反）** · `ActiveSession` · `RemoteHealth` · `SessionActivity` · `TasksUpdate` |
| `rename_all = "camelCase"` | **只有 3 个**有：`JsonlBatch` · `RemoteHealth` · `TasksUpdate`。**另外 8 个没有** |

### TS 侧的对应物

| Rust payload | TS 侧在哪 | 备注 |
|---|---|---|
| `JsonlLine` / `SessionEnded` / `SessionIdle` / `SessionStarted` / `TasksUpdate` / `SessionActivity` | `src/events.ts`，**6 个手写 `export interface`** | 主要替换目标 |
| `JsonlBatch` / `RemoteSessionAdded` | `src/events.ts` + `src/main.ts` | 需查是内联还是复用 |
| `RemoteHealth` | `src/remote-health.ts` | 不在 `events.ts`，**主计划共享面 1 只写了 `events.ts`，漏了这个文件** |
| `ActiveSession` / `FrontendReady` / `SnapshotInflight` | **TS 侧无对应类型** | 生成它们仍有价值（边界的一部分），但没有手写版可替换 |

### 两处需要修正主计划的地方

**① 共享面 1 说「事件名常量也从 `bridge.rs:11-51` 生成」—— 做不到。**
`ts-rs` 生成**类型**，不生成 `const` 字面量。
→ **改为**：事件名保持手写，由**钉死 10 个名字的结构性守卫**对拍
（从 `bridge.rs` 源里抠出 `pub const X: &str = "y";` 的 10 对，与 TS 侧的字面量比对；
计数自检用 `== 10`）。形状照 `config_surface.rs::every_host_declaration_is_pinned`。

**② 共享面 1 只写了 `src/events.ts`，漏了 `src/remote-health.ts`。**
→ 补进主计划共享面表。

## 3. 一条硬性范围约束：**绝不"统一"线上格式**

实测线上格式是混的——3 个事件的 payload 字段是 camelCase，8 个是 snake_case。
**手写 TS 今天是准的**（没 `rename_all` 的那些在 TS 侧确实写 `session_id`/`waiting_for`，
有 `rename_all` 的 `TasksUpdatePayload` 写 `sessionId`）——纪律守住了，这里没有既存 bug。

**生成物会忠实复现这个不一致，这是正确行为。**
给那 8 个补 `rename_all` 会**改变线上字段名** = 行为变化 = 违反本工作区
「纯类型层改动、行为逐字节不变」的硬判据。若将来要统一，那是**另一个功能**，
要有自己的 DoD 与迁移方案（两侧同时改 + 版本协商）。**C02 不做，登记。**

## 4. DoD

- [ ] 11 个 payload struct 全部派生 `TS` + 导到 `src/generated/`
      （含 `FrontendReadyPayload`——它是 `Deserialize`，TS 侧构造它时同样需要类型）
- [ ] `src/events.ts` 的 6 个手写 `interface` 删除，改成 import 生成物
- [ ] `src/remote-health.ts` 的 `RemoteHealth` 手写类型同样替换
- [ ] `JsonlBatch` / `RemoteSessionAdded` 在 `events.ts`/`main.ts` 的表达查清并替换
- [ ] **生成物忠实复现混合的线上格式**：3 个 camelCase、8 个 snake_case，**一个都不改**
- [ ] **10 个事件名由结构性守卫钉死**（`bridge.rs` 的 const ↔ TS 字面量），计数自检 `== 10`
- [ ] **变异验收**：① 删掉某个 payload 的一个字段 → `tsc` 报错；
      ② 改掉一个事件名常量 → 那条守卫红。两次都先 diff 确认落位 + 确认编译得过才判色
- [ ] 反向自检：不变异时 `tsc` 0 错、守卫绿
- [ ] `src/generated/` 的守卫计数从 `toBe(2)` 更新到实际值（C01 留的那条会红一次，**这是设计**）
- [ ] 全门禁绿且数字不降（基线：cargo 538 · npm 819/54 · clippy 0 · tsc 0）
- [ ] **8 套真机套件全绿、条数与基线一致**（行为逐字节不变）

**明确不做**：不统一线上格式（§3）· 不碰命令半边（C04）· 不碰大整数策略（C03）·
不做 CI 门禁（C05）· 不碰 IR 类型 · 不碰 `src/accounts.ts`（归 `account-zero`）

## 5. 实现步骤

1. **先查清 `JsonlBatch`/`RemoteSessionAdded` 在 `events.ts`/`main.ts` 里到底是手写 interface
   还是内联字面量类型**——决定它们算不算"替换目标"。
2. 11 个 struct 加派生（`FrontendReady` 用 `#[ts(export)]` 但注意它只有 `Deserialize`，
   确认 `ts-rs` 对纯 `Deserialize` 的结构是否照样生成）。
3. **逐个核对生成物字段名与线上格式一致**（3 camelCase / 8 snake_case）。
   任何一个对不上就停下——那说明 `ts-rs` 对某个 serde 形态的处理与我预期不符。
4. 替换 `events.ts` 的 6 个 + `remote-health.ts` 的 1 个。
5. 写事件名钉死守卫（10 对，计数自检 `== 10`）。
6. 更新 C01 那条生成物计数守卫（`toBe(2)` → 新值）。
7. 变异 ① ②，各自先 diff + 确认编译，再判色。
8. 全门禁 + 8 套真机套件。
9. Phase D 审计（低风险档，1 agent）→ Phase E/F → commit。

## 6. 测试策略

- 主判据是变异（步骤 7）。
- 结构性守卫两条：事件名钉死（新增）· 生成物存在且被消费（扩 C01 那条）。
- **守卫范围**：事件名守卫覆盖**全部 10 个**（这里可以全覆盖，因为事件半边这次一次迁完，
  不像命令半边还有 118 个没迁）。这与 C01 那条「只覆盖一个文件」的收窄**不矛盾**——
  范围该等于**性质**的范围，而这次性质就是全覆盖。

## 7. 代码审计结果（Phase D）

（待填）

## 8. 工程审计结果（Phase E）

（待填）

## 9. 签收

- [ ] 通过代码审计
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含 §2 那两处订正）
