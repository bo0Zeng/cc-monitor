# C02 — 事件半边先迁完

> 主计划：`../MASTERPLAN.md`（用户已批；选型见 §8：`ts-rs` v12）
> 前置：C01 已闭环（`20c9dd7`，变异 A/B 双向验收通过）

## 1. 为什么先迁事件半边

主计划 §0.0 的判断：事件半边的纪律比命令半边好，改动面最小、信噪比最高。
**Phase B 实测确认了「纪律好」这一点，但把范围修正了四处**（见 §2）——
其中两处是主计划写错了机制，两处是我对 TS 侧形态的估计过于乐观。

## 2. 实测盘点（2026-07-29 Phase B）

### `src-tauri/src/bridge.rs`

| 项 | 实测 |
|---|---|
| 事件名常量 | **10 个** `pub const`：`jsonl-line` · `jsonl-batch` · `session-ended` · `task-update` · `session-activity` · `session-started` · `session-idle` · `remote-session-added` · `remote-health` · `snapshot-inflight` |
| payload struct | 文件里 **11 个**，但**属于事件半边的只有 10 个**：`JsonlLine` · `JsonlBatch` · `SessionEnded` · `SessionIdle` · `SessionStarted` · `RemoteSessionAdded` · **`FrontendReady`（`Deserialize`，方向相反）** · `RemoteHealth` · `SessionActivity` · `TasksUpdate`。第 11 个 `ActiveSession` **是命令返回类型，归 C04**（见下方 ①） |
| `rename_all = "camelCase"` | **只有 3 个**有：`JsonlBatch` · `RemoteHealth` · `TasksUpdate`。**另外 8 个没有** |

### TS 侧的对应物

| Rust payload | TS 侧在哪 | 备注 |
|---|---|---|
| `JsonlLine` / `SessionEnded` / `SessionIdle` / `SessionStarted` / `TasksUpdate` / `SessionActivity` | `src/events.ts`，**6 个手写 `export interface`** | 主要替换目标 |
| `JsonlBatch` / `RemoteSessionAdded` | `src/events.ts`，**内联字面量类型**（`:396` / `:465`） | **最危险的一种形态**，见下方 ② |
| `RemoteHealth` | `src/remote-health.ts` | 不在 `events.ts`，**主计划共享面 1 只写了 `events.ts`，漏了这个文件** |
| `FrontendReady` | **TS 侧从来没有过类型**（`main.ts:769` 直接 `emit("frontend-ready", { prioritySid })`） | 生成它是**净新增能力**，不是替换 |
| `ActiveSession` | 不属事件半边 —— 命令返回类型 | **移出 C02，归 C04** |
| `snapshot-inflight` | 该事件**没有专属 payload struct** | 事件名守卫仍覆盖它；但别断言「每个事件名都有一个类型」 |

### 步骤 1 实测（read-only，趁 C01 审计在跑时做）——**又改了两处范围**

**① `ActiveSessionPayload` 根本不是事件 payload，是命令返回类型 ⇒ 移出 C02、归 C04。**
`bridge.rs:167` 自己的 doc comment 就写着「`list_active_sessions` IPC 返回项」，
而 `lib.rs:1507` 的签名是 `-> Vec<bridge::ActiveSessionPayload>`。
它只是**住在 `bridge.rs` 里**，不属于事件半边。
⇒ **C02 覆盖 10 个 payload，不是 11 个。**

**② TS 侧的手写形态有四种，不是一种。** 这决定了「替换」的工作量与风险：

| 形态 | 哪些 | 风险 |
|---|---|---|
| **具名 `export interface`**（6 个） | `JsonlLine` · `SessionEnded` · `SessionIdle` · `SessionStarted` · `TasksUpdate` · `SessionActivity`（全在 `src/events.ts`） | 最好办，直接换 import |
| **具名类型，但在另一个文件** | `RemoteHealth`（`src/remote-health.ts`） | 主计划原先漏了这个文件 |
| **内联字面量类型**（2 个） | `sub<{ chunkIndex; chunkTotal; payloads }>("jsonl-batch", …)`（`events.ts:396`）· `sub<{ … cwd; name }>("remote-session-added", …)`（`events.ts:465`） | **最危险的一种**——它没有名字，漂移时没有任何东西会红，人也很难在 review 里看见 |
| **完全无类型** | `frontend-ready`：`main.ts:769` `void emit("frontend-ready", { prioritySid: lastActive })`，TS 侧**从来没有过这个类型** | 生成它是**净新增能力**，不是替换 |

**`FrontendReadyPayload` 还是唯一用「字段级 `rename`」的**（`#[serde(rename = "prioritySid")]`
在 `priority_sid` 上，而不是容器级 `rename_all`）。C01 的变异 B 已经实证 `ts-rs` 认这个形态。

**③ `snapshot-inflight` 这个事件没有专属 payload struct**（11 个 struct 里没有它）。
事件名守卫仍覆盖全部 10 个名字；但「每个事件名都有一个 payload 类型」这条**不成立**，
别把它写成断言。

### 两处需要修正主计划的地方

**① 共享面 1 说「事件名常量也从 `bridge.rs:11-51` 生成」—— 做不到。**
`ts-rs` 生成**类型**，不生成 `const` 字面量。
→ **改为**：事件名保持手写，由**钉死 10 个名字的结构性守卫**对拍
（从 `bridge.rs` 源里抠出 `pub const X: &str = "y";` 的 10 对，与 TS 侧的字面量比对；
计数自检用 `== 10`）。形状照 `config_surface.rs::every_host_declaration_is_pinned`。

**② 共享面 1 只写了 `src/events.ts`，漏了 `src/remote-health.ts`。**
→ 补进主计划共享面表。

### Phase C 步骤 1 实测：**范围再改一次** —— `JsonlLine`/`JsonlBatch` 延后

加派生之前先查了 10 个 payload 的字段类型，撞到**两条传递依赖**——`ts-rs` 会要求
被引用的类型也派生 `TS`：

| payload | 传递依赖 | 处置 |
|---|---|---|
| `TasksUpdatePayload.tasks` | `Vec<crate::tasks::TaskEntry>` | **一起做**。`TaskEntry` 是 7 个简单字段 + `rename_all="camelCase"` + 2 处 `skip_serializing_if`（正好会被 C01 那条通用守卫要求配 `ts(optional)`） |
| `JsonlLinePayload.message` | `crate::messages::JsonlRecord` | **延后，不在 C02 做** |

**为什么 `JsonlRecord` 必须延后**（两条独立理由，任一条都够）：

1. **Rust 侧那个 enum 自认是有损的。** `history.rs:628` 原文：
   「**用 `serde_json::Value` 原样搬运**（不走**有损的** `JsonlRecord` enum，避免丢 gitBranch/…）」
   —— 仓库自己在另一条路上**绕开**了它。让一个自认有损的模型去当 TS 类型的**源**，
   等于把 TS 侧收窄成那个损失。这跟 C01 那条 `kind` 被放宽是**同一个病的反面**，
   但后果大得多。
2. **闭包触底到 `serde_json::Value`**（`ApiMessage.content`，`messages.rs` 里有 6 处 `Value`），
   而 TS 侧的 `JsonlRecord` 住在 **`src/cards/index.ts`（1187 行）**——
   那是前端卡片渲染的承重模型。**用生成物替换它是一次前端渲染层的重构，不是一次类型迁移。**

⇒ **C02 覆盖 8 个 payload + `TaskEntry`**：`SessionEnded` · `SessionIdle` · `SessionStarted` ·
`RemoteSessionAdded` · `RemoteHealth` · `SessionActivity` · `FrontendReady` · `TasksUpdate`。
**`JsonlLine` / `JsonlBatch` 登记延后**，等有人先决定「Rust 的 `JsonlRecord` 要不要变成
无损模型」——那是一个独立的产品/架构决定，不该被一次类型迁移顺手做掉。

**顺带记一条 C01 那条通用守卫的第一次真实收益**：`JsonlLinePayload.origin` 也带
`skip_serializing_if`（`bridge.rs:78`），`TaskEntry` 有 2 处。它们全都会被那条扫描要求配
`ts(optional)` —— **这条守卫是 C02 开工前就已经在替我数活了**。

## 3. 一条硬性范围约束：**绝不"统一"线上格式**

实测线上格式是混的——3 个事件的 payload 字段是 camelCase，8 个是 snake_case。
**手写 TS 今天是准的**（没 `rename_all` 的那些在 TS 侧确实写 `session_id`/`waiting_for`，
有 `rename_all` 的 `TasksUpdatePayload` 写 `sessionId`）——纪律守住了，这里没有既存 bug。

**生成物会忠实复现这个不一致，这是正确行为。**
给那 8 个补 `rename_all` 会**改变线上字段名** = 行为变化 = 违反本工作区
「纯类型层改动、行为逐字节不变」的硬判据。若将来要统一，那是**另一个功能**，
要有自己的 DoD 与迁移方案（两侧同时改 + 版本协商）。**C02 不做，登记。**

## 4. DoD

- [x] **8 个** payload struct + `TaskEntry` 派生 `TS` + 导到 `src/generated/`
      （`ActiveSessionPayload` 归 C04；`JsonlLine`/`JsonlBatch` 延后，理由见 §2 末）
      （含 `FrontendReadyPayload`——它是 `Deserialize`，TS 侧构造它时同样需要类型）
- [x] `src/events.ts` 的 5 个手写 `interface` 换成 import + re-export
      （`JsonlLinePayload` 留手写——它依赖延后的 `JsonlRecord`）
- [x] `src/remote-health.ts` 的 `RemoteHealthPayload` 已替换；**顺带 `src/tasks-panel.ts` 的手写 `TaskEntry` 也替换**（与生成物完全同形，留两份就是漂移风险）
- [x] `events.ts` remote-session-added 那处内联字面量换成生成物
      （`:396` jsonl-batch 那处随 `JsonlBatch` 一起延后）
- [x] **`frontend-ready` 的 TS 侧首次拿到类型**（今天是无类型的内联对象 `{ prioritySid }`）
- [x] **生成物忠实复现混合的线上格式**（逐个核过：`session_id`/`waiting_for` snake · `sessionId`/`prioritySid` camel）—— ：3 个 camelCase、8 个 snake_case，**一个都不改**
- [x] **10 个事件名由结构性守卫钉死**（`bridge.rs` 的 const ↔ TS 字面量），计数自检 `== 10`
- [x] **变异验收**：① 删 `SessionStartedPayload.kind`（连带 `lib.rs:467` 构造点）→ 编译过（547 绿）→ 生成物少了字段 → `tsc` 红指向 `events.ts:444`；
      ② 改掉一个事件名常量 → 那条守卫红。两次都先 diff 确认落位 + 确认编译得过才判色
- [x] 反向自检：不变异时 `tsc` 0 错、守卫 6 绿
- [x] 生成目录期望从 2 项扩到 **11 项**，`skip_serializing_if` 计数自检 `toBe(1)` → `toBe(3)`
- [x] 全门禁绿且数字不降：cargo **547**（+9 导出测试）· code-picture-core 25 · npm **820/54**（+1 事件名守卫）· clippy 0 · tsc 0 · fmt 干净 · **C05 那条新门禁 rc=0** · npm audit rc=0 · shellcheck 0 · exec-bit rc=0
- [x] **8 套真机套件全绿、条数与基线一致**（26/44/12/15/13/-/14/7）· 默认 socket 4 会话逐字未变 · `git diff -- e2e/` 0 行

**明确不做**：不统一线上格式（§3）· 不碰命令半边（C04）· 不碰大整数策略（C03）·
不做 CI 门禁（C05）· 不碰 IR 类型 · 不碰 `src/accounts.ts`（归 `account-zero`）

## 5. 实现步骤

1. ~~查清 `JsonlBatch`/`RemoteSessionAdded` 的形态~~ **已完成**（见 §2 ②：两者都是**内联字面量类型**，
   位于 `events.ts:396` / `:465`）。
2. **10 个** struct 加派生（`FrontendReady` 只有 `Deserialize` —— **先确认 `ts-rs` 对纯 `Deserialize`
   的结构照样生成**；若不生成，那是一个要单独处置的发现，不是绕过理由）。
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

**对抗性审计已发出（1 个综合 agent），结果待收。** 以下是**我自己**在 Phase C 撞到并处置的，
交给审计独立复核（**别采信**）：

### 一、范围在实测中改了三次（11 → 10 → 8 + TaskEntry）

- **11 → 10**：`ActiveSessionPayload` 不是事件 payload，是 `list_active_sessions` 的返回类型
  （`bridge.rs` 自己的 doc comment 就这么写）⇒ 归 C04。
- **10 → 8**：加派生前先查字段类型，撞到两条传递依赖。`TaskEntry` 简单 ⇒ 一起做；
  **`JsonlRecord` 延后**，两条独立理由（Rust 侧自认有损 + 闭包触底到 `serde_json::Value`
  且 TS 侧那个类型是 1187 行的渲染承重模型）。

**这三次都是「加派生之前先数清字段类型」换来的**，不是做到一半才发现。

### 二、一条我先说错、随后被实测纠正的判断

我一度认为 C01 那条「每处 `skip_serializing_if` 都要配 `ts(optional)`」的规则**太严**
——因为 `TaskEntry` 那两个字段带 `serde(default)`，`ts-rs` 的
`maybe_omitted && has_default` 兜底**自动**加了 `?`，看起来不需要显式属性。

**实测证否**：兜底产出 `description?: string | null`（可缺席**且**可为 null），
而 `skip_serializing_if` 意味着运行时**永不为 null**（缺席就是缺席）⇒ 那个 `| null` 过度宽松，
**且与手写版 `tasks-panel.ts` 的 `description?: string` 不一致，`tsc` 当场报错**。
加显式 `ts(optional)` 后产出 `description?: string`，与运行时一致。
⇒ **规则不该放宽，C01 那条守卫是对的。** 已把这段推理写进守卫注释。

### 三、两处 TS 侧的 import/re-export 坑（同一个）

`events.ts` 与 `tasks-panel.ts` 都踩了：**只写 `export type { … } from "…"` 不会把名字带进
本地作用域**，而两个文件内部都在用那些名字（8 处 / 4 处）。必须 `import type` + 单独 `export type {…}`。
第一次改完 `tsc` 报 8 条 + 4 条，据此修正。

### 四、变异验收（两条，各自先 diff 确认落位 + 确认编译得过才判色）

- **变异 1**：删 `SessionStartedPayload.kind`。**第一次编译不过**（`lib.rs:467` 有构造点用它）
  ——而那一刻 `tsc` 输出为空。**如果按那个空输出判色会得出「链路没牙」的错误结论。**
  补齐构造点后：编译过（547 绿）→ 生成物确认少了字段 → `tsc` 红指向 `events.ts:444`。
  这是本会话「编译失败不等于测试有牙」的**第六次**，每次都是靠先查编译才没误判。
- **变异 2**：改 `SESSION_IDLE` 常量的字面量。**事件名守卫红，而 `tsc` 绿**
  ⇒ 证明这条守卫覆盖的是一个**别的门禁都抓不到**的洞。
  （一个 `const &str` 字面量改动不可能编译失败，且该守卫读 Rust 源不读生成物，
  所以判色有效；但我那条查编译的 grep 正则写坏了没真跑成，如实记。）

## 8. 工程审计结果（Phase E）

（待填）

## 9. 签收

- [ ] 通过代码审计
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含 §2 那两处订正）
