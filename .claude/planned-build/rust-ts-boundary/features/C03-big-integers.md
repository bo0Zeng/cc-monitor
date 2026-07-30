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
- **仓内先例**：`usage.rs` 的 **`UsageTotals`**（`:21-29`）四个 `u64` 跨边界，
  TS 侧 `views/usage-pivot.ts` 声明 `number` 并直接做算术；**全仓无 BigInt**。

  > **两处更正**（Phase B 复核时发现，C01 审计报告的转述不准、我先前沿用了）：
  > ① 那个 struct 叫 **`UsageTotals`**，不是 `TokenUsage`；
  > ② 它是 4 个 `u64` **加 1 个 `u32`**（`msgs`），不是纯 4 个字段。
  > 另：它同时带 `Deserialize`——daemon `--usage` 回传的行也反序列化成它
  > （`remote_history::aggregate_remote_usage_all`）。那是 Rust↔Rust 的另一条边界、
  > 不影响 TS 侧决定；加派生前已确认不影响 `Deserialize`
  > （C01 审计逐字节验过 `derive(TS)` 不触碰 `Serialize`/`Deserialize`，复用该结论）。
- **C01 已处置一处**：`DataPathInfo.size_bytes` 加了 `#[cfg_attr(test, ts(type = "number"))]`
  + 上限论证。

**已确认的静默有损点**（Phase G 代码工程视角报的那处，我已核）：
`sftp_pool.rs:33-42` 的 `SftpEntry.size: u64` ↔ `src/sftp/paths.ts:6-13` 的 `size: number`。
同文件还有 `SftpStat.size: u64`（`:47-51`）与 `TransferProgress` 的两个 `u64`（`:254/256`）。
⇒ **最终处置**：`SftpEntry` 与 `TransferProgress` 收进本功能，
**`SftpStat` 跳过**（理由见 §7 那张表：TS 侧裸 `invoke` 无类型参数、字段没人用）。

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

- [x] **步骤 1 全仓盘点已做**（跑的是 grep，不是印象）：列出**所有**带 `ts_rs::TS` 派生的 struct 里的
      `u64`/`i64` 字段。**实测结果见 §7**（与本条 Phase B 时的预估不同：
      已派生集合里只有 1 个大整数字段且已处置；`JsonlBatchPayload.chunk_*` **是 `u32` 不是 `u64`**
      ——Phase B 时我把它写进候选，那是错的）。**这一步跑的是 grep，不是印象**
- [x] `SftpEntry.size` 加 `#[cfg_attr(test, ts(type = "number"))]` + 上限论证
- [x] `sftp_pool.rs` 的 `SftpEntry` + `TransferProgress` 加 `ts-rs` 派生，`src/sftp/paths.ts` 的手写 `SftpEntry`
      改成 import 生成物（**这是唯一已确认的静默有损点，本功能要真把它闭掉**）
- [x] `usage.rs::`**`UsageTotals`** 一并处置（4 个 `u64`，仓内先例，TS 侧已当 number 用）
- [x] **新增守卫**：`u64`/`i64` 字段必须配 `#[ts(type = …)]`，计数自检用等号
- [x] **变异验收**：① 去掉某个字段的 `ts(type = "number")` → 生成物变 `bigint` → `tsc` 红
      **且**新守卫红；② 新加一个带 `u64` 的字段而不配属性 → 新守卫红。
      两次都先 diff 确认落位 + 确认编译得过再判色
- [x] 反向自检：不变异时 7 绿 + tsc 0
- [x] 更新 `TS_DERIVING_SOURCES`（3→5 个文件）与生成目录期望（11→14）（**会红一次，那是设计**）
- [x] 全门禁绿且数字不降（基线：cargo 547 · npm 820/54 · clippy 0 · tsc 0 · C05 门禁 rc=0）
- [x] 8 套真机套件全绿、条数与基线一致（26/44/12/15/13/-/14/7）· 默认 socket 4 会话逐字未变 · `git diff -- e2e/` 0 行

**明确不做**：不把任何字段改成 `string`（那要两侧一起改 + 独立立项）·
不碰 `JsonlLine`/`JsonlBatch`（随 C02 一起延后）· 不碰命令半边的其余部分（C04）·
不改 `ci.yml`

## 5. 实现步骤

1. **全仓盘点**（必须 grep，不凭印象）。产出一张「字段 → 量纲 → 上限论证」的表。
2. `sftp_pool.rs` 的 `SftpEntry` + `TransferProgress` 加派生 + 三个字段加 `ts(type = "number")` + 论证注释
   （**不是** `SftpEntry` + `SftpStat`——后者跳过，见 §7）。
3. `src/sftp/paths.ts` 的手写 `SftpEntry` 改 import + re-export（注意 C02 踩过两次的坑：
   **只写 `export type {…} from` 不会把名字带进本地作用域**）。
4. `usage.rs::`**`UsageTotals`** 同样处理，核对 `views/usage-pivot.ts` 的手写版是否同形
   （**已核：逐字段同形**）。
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

**低风险档，不开审计 agent。** 依据：本功能是**同一个已被两轮审计打磨过的形状的第三次应用**
（C01 建守卫、C02 审计把它从「400 字符窗口」修成「同字段属性块」+「以 `pub struct` 为锚收属性」），
攻击面就是「新守卫会不会红」，而那已由变异**双重**证明（守卫红 + `tsc` 独立红）。

### 步骤 1 的盘点结果与我 Phase B 的预期不同，值得单独记

```
有 ts_rs::TS 派生的文件（盘点时）：data_paths.rs · bridge.rs · tasks.rs
这 3 个文件里的 u64/i64：
  data_paths.rs:87  size_bytes: Option<u64>   ← C01 已处置
  bridge.rs:84      seq: u64                  ← JsonlLinePayload，未派生（延后）
  tasks.rs          无
```

**即：已派生的集合里只有 1 个大整数字段，而它已经处置过了。** 新守卫若只覆盖这一格，
今天会数到 1 并**平凡通过** ⇒ **C03 的价值几乎全是前瞻性的**（在 C04 派生 127 个 struct
之前把规则立好）。所以本功能给它找了**三个真实应用面**，让规则有内容可守，
而不是立一条今天空转的规则。

### 三个应用面的选取，以及**跳过 `SftpStat` 的理由**

| 目标 | u64 | 手写镜像 | 收 |
|---|---|---|---|
| `sftp_pool.rs::SftpEntry.size` | 1 | `src/sftp/paths.ts:6-13`（`size: number`） | **收**——**这是 Phase G 报的那个唯一已确认的静默有损点** |
| `sftp_pool.rs::TransferProgress` | 2 | `src/sftp/panel.ts:31-35`，且 `:339` 真在做算术 | **收**——同一类静默有损点 |
| `usage.rs::UsageTotals` | 4 | `src/views/usage-pivot.ts:11-17`，逐字段同形 | **收**——4 个字段让守卫的计数有内容 |
| `sftp_pool.rs::SftpStat.size` | 1 | **无** | **跳过**：TS 侧是裸 `invoke("sftp_stat", …)` **无类型参数**（`panel.ts:293/395`），返回值是 `unknown`、字段没人用 ⇒ **生成它就是「为假想消费者建抽象」** |

**这是一次刻意的、窄的越界**：这三个都是**命令半边**的类型，而共享面协议说命令半边归 C04。
理由是**策略需要 ≥2 个真实应用面才成为规则而不是特例**，且 `sftp_pool` 是本功能的动因缺陷。
C04 继承其余。

### 上限论证按量纲**分开算**，不套用同一条

- 字节数（`SftpEntry.size` / `TransferProgress`）：f64 安全整数上限 2^53-1 ≈ **8 PB**。
- token 计数（`UsageTotals` 四个字段）：**单独算的**——2^53-1 ≈ **9×10^15 tokens**，
  按每天 10^7 tokens 算是 **9 亿天**。
  `usage.rs` 头注那句「u64 防大历史累加溢出」是**刻意选择**，所以不能套用字节数那条。

### 两处我自己踩的坑，都记下来

**① 我用裸 `grep -c bigint src/generated/*.ts` 得到「3 个文件含 bigint」，一度以为收窄失效。**
实际全在 **JSDoc 散文**里——`ts-rs` 把 Rust 的 `///` doc comment 搬进 JSDoc，而那些注释
正是在解释「不许回落到 bigint」。**剥注释后类型体里 0 个。**
⇒ **这是 C03 Phase B 那条「不能写『全仓生成物不含 bigint』断言」的实证**：
我自己就用裸 grep 复现了那个假阳性。守卫必须打在源上。

**② 新守卫第一版的正则 `/ts\(type\s*=/` 当场假红**，因为实际属性写法是
`ts(optional, type = "number")`——`type` 不紧跟 `ts(`。
更要紧的是：**报错信息里的省略号把属性行藏住了**（显示成一串空行），
我一度以为是「向上收属性」的循环有问题、差点去改对的那一半。
⇒ **别从被截断的断言消息里推断原因。** 已写进守卫注释。

## 8. 工程审计结果（Phase E）

- **主计划自洽**：C03 未引入新耦合。`TS_DERIVING_SOURCES` 3 → 5 个文件、生成目录 11 → 14。
- **给 C04 的三条硬交接**：
  1. **`JsonlLinePayload.seq: u64` 现在有策略可用了** ⇒ `JsonlLine`/`JsonlBatch` 那个延后的
     真正卡点已解除（C02 审计 I4 订正过：它卡的不是架构决定，是这条）。
     加上审计实测跑通的逃生口（`#[ts(type = "import('../cards').JsonlRecord")]`），C04 可以做掉它。
  2. **`SftpStat` 与命令半边其余部分归 C04**，且 C04 要面对一个新问题：
     像 `SftpStat` 这样**返回值没人用类型**的命令，是「补上类型」还是「保持裸 invoke」？
     C03 的答案是后者（不为假想消费者生成），但 C04 面对 118 个命令时需要一条成文规则。
  3. **守卫的两条扫描（`skip_serializing_if` / 大整数）现在是 C04 要复制 127 次的形状**，
     两条都已被变异验证过，且都打在源上。
- **技术债，如实登记**：`sftp_pool.rs` 还有 `SftpStat.size` 一个 u64 未纳入守卫覆盖
  （因为该 struct 未派生）。**这不是漏洞**——守卫的性质是「已派生的 struct 里每个大整数
  都要显式表态」，未派生的不在性质范围内。但 C04 派生它时守卫会红一次，那是设计。

## 9. 签收

- [x] **通过代码审计** —— 低风险档（同一形状的第三次应用，前两次已被审计打磨），
      判据是变异**双重**验证（守卫红 + `tsc` 独立红）。§7 记了我自己踩的两个坑。
- [x] **通过工程审计** —— 见 §8，含三条给 C04 的硬交接与一条如实登记的覆盖边界。
- [x] **主计划已据此更新** —— §1 功能表 · §3 共享面 2 · 变更记录 07。
