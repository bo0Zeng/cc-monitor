# C03 — 大整数的显式处置

> 主计划：`../MASTERPLAN.md`（选型见 §8：`ts-rs` v12）
> 前置：C01（`20c9dd7`+`8abd489`）· C05（`f7dd23c`）· C02（`682d5a5`）均已落地
> **必须在 C04 之前**：否则 C04 会把 127 个 struct 里所有 `u64` 机械地生成成
> `ts-rs` 的默认值 `bigint`，**把一个已知错误批量固化**。

## 1. 事实（C01 已实证，此处复用，不重新论证）

**`ts-rs` 默认把 `u64`/`i64` 映射成 `bigint`，而那对 Tauri 的命令 IPC 是错的：**

- **原理级证据**：`tauri-2.11.2/src/ipc/mod.rs:181-183` 的 `impl<T: Serialize> IpcResponse for T`
  走 `serde_json::to_string(&self)` ⇒ 线上是 **JSON 文本**，而 `JSON.parse`
  **永不产出 BigInt** ⇒ 命令返回值**在原理上不可能**以 BigInt 到达 TS 侧。
- **仓内先例**：`usage.rs` 的 `TokenUsage` 四个 `u64` 跨边界，
  TS 侧 `views/usage-pivot.ts` 声明 `number` 并直接做算术；**全仓无 BigInt**。
- **C01 已处置一处**：`DataPathInfo.size_bytes` 加了 `#[cfg_attr(test, ts(type = "number"))]`
  + 上限论证。

**已确认的静默有损点**（Phase G 代码工程视角报的那处，我已核）：
`sftp_pool.rs:33-42` 的 `SftpEntry.size: u64` ↔ `src/sftp/paths.ts:6-13` 的 `size: number`。
同文件还有 `SftpStat.size: u64`（`:47-51`）。

## 2. 策略：默认 `number`，但**必须是显式带论证的决定**

这个应用里跨边界的 `u64`/`i64` **全都是量纲**（字节数 / 毫秒时间戳 / 计数），
而它们**全都远低于 2^53-1**：

| 量纲 | f64 安全整数上限对应的实际值 | 结论 |
|---|---|---|
| 字节数（文件大小） | 2^53-1 ≈ **8 PB** | `number` 安全 |
| 毫秒时间戳 | 2^53-1 ms ≈ **28.5 万年** | `number` 安全 |
| token 计数 / 行号 / 序号 | 远低于 2^53 | `number` 安全 |

⇒ **策略不是「有些走 bigint」，而是：默认 `number`，但绝不许是 `ts-rs` 的默认值。**

**两者的区别是全部要点**：
- `ts-rs` 默认 → `bigint` → **与运行时不一致，类型在撒谎**；
- 显式 `#[ts(type = "number")]` + 一行上限论证 → **与运行时一致，且下一个人看得见为什么**。

**什么时候该走 `string`**（本轮预计一个都用不上，但规则要写下来）：
若某字段是**不透明标识符**（不做算术、可能超 2^53、比较需精确），
那它就不该是 `u64` 而该是 `String`——**在 Rust 侧改类型，不在边界上打补丁**。
若确实必须是 `u64` 且可能超界，用 `#[ts(type = "string")]` + `#[serde(with = ...)]` 两侧一起改，
**那是行为变化，要独立立项**。

## 3. 机制：一条守卫，形状照 `skip_serializing_if` 那条

C01 建、C02 扩的那条通用扫描已经证明这个形状好用（C02 时它**开工前就替我数出了活**）。
本功能加同族的第二条：

> **扫 `TS_DERIVING_SOURCES` 里所有含 `ts_rs::TS` 派生的 struct，
> 每一个 `u64`/`i64`/`Option<u64>`/`Option<i64>` 字段都必须配 `#[ts(type = "…")]`。**
> 计数自检用 `toBe(<实测数>)`，加了新的必须红一次。

**为什么这条比「断言生成物里没有 bigint」更好**：后者只在**已生成**的类型上成立，
而新增一个带 `u64` 的 struct 时，前者会在**加派生的那一刻**就红，
后者要等到有人恰好去看生成物。**守卫要打在源上，不是打在产物上。**

**一条不能写的断言**：「全仓生成物不含 `bigint`」——将来若真有字段需要 `bigint`
（两侧一起改的那种），这条会假红。**假红的守卫会被人关掉。**

## 4. DoD

- [ ] **步骤 1 先做全仓盘点**：列出**所有**带 `ts_rs::TS` 派生的 struct 里的
      `u64`/`i64` 字段（当前已知：`data_paths.rs` 1 处已处置 · `sftp_pool.rs` 2 处 ·
      `bridge.rs` 的 `JsonlLinePayload.seq`/`JsonlBatchPayload.chunk_*` 属**延后**的那两个 struct）。
      **这一步必须跑 grep**，不能凭印象
- [ ] `SftpEntry.size` / `SftpStat.size` 加 `#[cfg_attr(test, ts(type = "number"))]` + 上限论证
- [ ] `sftp_pool.rs` 的两个 struct 加 `ts-rs` 派生，`src/sftp/paths.ts` 的手写 `SftpEntry`
      改成 import 生成物（**这是唯一已确认的静默有损点，本功能要真把它闭掉**）
- [ ] `usage.rs::TokenUsage` 一并处置（4 个 `u64`，仓内先例，TS 侧已当 number 用）
- [ ] **新增守卫**：`u64`/`i64` 字段必须配 `#[ts(type = …)]`，计数自检用等号
- [ ] **变异验收**：① 去掉某个字段的 `ts(type = "number")` → 生成物变 `bigint` → `tsc` 红
      **且**新守卫红；② 新加一个带 `u64` 的字段而不配属性 → 新守卫红。
      两次都先 diff 确认落位 + 确认编译得过再判色
- [ ] 反向自检：不变异时全绿
- [ ] 更新 `TS_DERIVING_SOURCES` 与生成目录期望（**会红一次，那是设计**）
- [ ] 全门禁绿且数字不降（基线：cargo 547 · npm 820/54 · clippy 0 · tsc 0 · C05 门禁 rc=0）
- [ ] 8 套真机套件全绿、条数与基线一致

**明确不做**：不把任何字段改成 `string`（那要两侧一起改 + 独立立项）·
不碰 `JsonlLine`/`JsonlBatch`（随 C02 一起延后）· 不碰命令半边的其余部分（C04）·
不改 `ci.yml`

## 5. 实现步骤

1. **全仓盘点**（必须 grep，不凭印象）。产出一张「字段 → 量纲 → 上限论证」的表。
2. `sftp_pool.rs` 两个 struct 加派生 + 两个 `size` 加 `ts(type = "number")` + 论证注释。
3. `src/sftp/paths.ts` 的手写 `SftpEntry` 改 import + re-export（注意 C02 踩过两次的坑：
   **只写 `export type {…} from` 不会把名字带进本地作用域**）。
4. `usage.rs::TokenUsage` 同样处理，核对 `views/usage-pivot.ts` 的手写版是否同形。
5. 写新守卫 + 更新两处期望。
6. 变异 ① ②。
7. 全门禁 + 8 套真机套件。
8. Phase D 审计 → E/F → commit。

## 6. 测试策略

- 主判据是变异（步骤 6）。
- 守卫打在**源**上（Rust 属性）而不是产物上（生成物里有没有 bigint），理由见 §3。
- **不断言注释里的论证文本**（C01 审计 S2 的教训：散文断言一重排就假红）。
  论证留给人读，机器只查属性在不在。

## 7. 代码审计结果（Phase D）

（待填）

## 8. 工程审计结果（Phase E）

（待填）

## 9. 签收

- [ ] 通过代码审计
- [ ] 通过工程审计
- [ ] 主计划已据此更新
