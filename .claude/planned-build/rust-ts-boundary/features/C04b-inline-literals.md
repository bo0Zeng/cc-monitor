# C04b — 两处已登记的内联字面量

> 主计划：`../MASTERPLAN.md` · 前置：C01/C05/C02/C03/C04a 均已闭环签收
> 拆分论证：`C04-command-half.md` §2

## 1. 两处目标（本轮实测，不照抄）

| # | 位置 | 内联字面量 | 对应的 Rust 源 | 生成物 | 能不能做 |
|---|---|---|---|---|---|
| **①** | `src/main.ts:744-746` | `{ session_id: string; cwd: string; kind: string \| null; name: string \| null }[]` | `bridge.rs:180` `ActiveSessionPayload`（命令 `list_active_sessions` 的返回项） | **待生成** | **能** |
| **②** | `src/tabs.ts:1631-1633` | `{ session_id: string; status: string \| null; waiting_for: string \| null }[]` | `lib.rs:1489` `fn list_session_activity() -> Vec<bridge::SessionActivityPayload>` | **C02 已生成** | **不能——卡「不碰 `tabs.ts`」红线** |

### 关于 ② 的一条实测收紧

计划原文只说「`tabs.ts:1632` 的 `SessionActivityPayload`」。本轮核实后可以说得更硬：
`list_session_activity` 的返回类型**就是** `Vec<bridge::SessionActivityPayload>`，
而 `src/generated/SessionActivityPayload.ts` 已经存在、字段**逐字节一致**
（`session_id: string, status: string | null, waiting_for: string | null`）。

⇒ **② 是一行改动**：把内联字面量换成 `import type { SessionActivityPayload }` + `SessionActivityPayload[]`。
不需要新派生、不需要重新生成、不需要改守卫。**它 100% 卡在红线上，零技术障碍。**
这一条写清楚是为了让红线决策有具体代价：松开红线换来的是一行。

## 2. 一条 snake_case 的观察（不是问题，是生成的理由）

`ActiveSessionPayload` 与 `SessionActivityPayload` **都没有** `#[serde(rename_all = "camelCase")]`，
所以它们在线上是 **snake_case**（`session_id`），与 `DataPathInfo`（camelCase）不一致。

**不许「顺手统一」**——那是行为改动（本工作区每个 commit 的硬判据是**行为逐字节不变**），
而且会同时打断 `main.ts` 和 `tabs.ts` 的读取。生成物必须**忠实于线上契约**，
而不是忠实于某种风格偏好。

**这反而是生成它的理由**：手写镜像可以静默漂成 camelCase 而无人发现；生成物不会。
守卫的 camelCase 断言只钉那两个确实是 camelCase 的文件（范围恰好等于性质范围，C01 纪律），
所以加一个 snake_case 生成物不会假红——这一条本轮已实测确认。

## 3. DoD

- [x] `bridge.rs` 的 `ActiveSessionPayload` 加 `ts_rs::TS` 派生 + 导出
- [x] `main.ts` 的内联字面量换成 `ActiveSessionPayload[]`
- [x] 守卫的生成物清单 14 → **15**（`toEqual` 等号，不是 `>=`）
- [x] **变异验收**：删/改 `ActiveSessionPayload` 的一个字段 → `tsc` 报错（证明真被消费）；
      先 diff 确认落位 + 确认 Rust 编译得过再判色
- [x] 全门禁绿且数字不降；8 套真机 152 条逐个不变（行为逐字节不变）
- [x] ② **如实标注「已跳过，等授权」**，不假装达成

**明确不做**：不碰 `tabs.ts`（红线）· **不把 `list_active_sessions` 加进 C04a 的包装层**
——`main.ts` 有 9 个 `invoke` 调用点，只迁 1 个既不让「29」降一格、又让同一个文件里
两条路并存（正是账本第 7 行约束 ④ 要防的）。整文件迁移是 C04d 的事。

## 4. 代码审计结果（Phase D）

**强度：低风险**（一个 struct 派生 + 一处类型替换 + 一条守卫计数，无逻辑改动）⇒
按 planned-build 强度裁剪，用主线程变异验收 + 全门禁代替多 agent 并行审计。

- **变异 A**（删 `ActiveSessionPayload.cwd`）：Rust 侧同时改掉 `lib.rs` 的构造点使其编译得过
  （**C01 栽过一次**：变异没编译过时 `tsc` 什么都不说，那种「绿」是无效结果）→
  `tsc` 报错指向 `main.ts` 的 `s.cwd`。**成立。**
- **变异 B**（生成物清单从 15 改回 14）→ 守卫 `toEqual` 红并列出差异。**成立。**

## 5. 工程审计结果（Phase E）

**共享面**：动的是账本第 1 行（`main.ts` 属 events 一族）。第 1 行的最终形态是
「手抄镜像全部由生成物取代」，本功能朝它走了一格，没打补丁。**账本无需新增行。**

**对后续的影响**：C04d 迁 `main.ts` 整文件时，这处调用点已经是 `invoke<生成类型>` 形态，
只需换成包装层调用，不必再决定「类型从哪来」。

**遗留**：② 一行改动等 `tabs.ts` 红线授权。已同时记进 `STATUS.md` 阻塞项与主计划 §0.1 成功标准 4。

## 6. 签收

- [x] 通过代码审计（低风险档：变异 A/B 双向验收 + 全门禁）
- [x] 通过工程审计（账本第 1 行朝最终形态走，无新共享面）
- [x] 主计划已据此更新（生成物 14 → 15；C04b 状态；变更记录 09）
