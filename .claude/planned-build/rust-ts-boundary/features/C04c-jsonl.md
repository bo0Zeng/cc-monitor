# C04c — JSONL 边界：`JsonlLine` / `JsonlBatch`（**Phase B 修订了原计划的处置**）

> 主计划：`../MASTERPLAN.md` · 前置：C01/C05/C02/C03/C04a/C04b 均已闭环
> **本文件记录一次对原计划的修订**（planned-build 铁律 4：计划 ≠ 现实就停下改计划）

## 1. 原计划怎么写的，为什么改

**原处置**（C02 §2 末 + C04 §2）：生成两个 payload，`message` 字段用**逃生口**
`#[ts(type = "import('../cards').JsonlRecord")]` 指向 TS 侧手写的 `JsonlRecord`，
并把「当场暴露的 4 处 `cards/index.ts` 缺口如实登记不顺手修」。

**为什么这个处置不对**：C02 Phase D 审计的 I4 已经把方向订正过一次——
这个边界上 **Rust 的 `JsonlRecord` 就是线定义**（wire == `serde_json::to_string(JsonlRecord)`），
TS 那份是**更窄的手抄**。逃生口指向手抄版 ⇒ **生成物指向病灶**，
等于把「手写镜像静默漂移」这个本工作区正在治的病，用一条 `#[ts(type)]` 固化下来。

**为什么原计划以为做不到**：`events.ts:13-16` 当时写着
「用生成物替换它是**一次渲染层重构**，不是一次类型迁移 ⇒ 延后」。

**实测把这个判断推翻了**（本轮 Phase B 的实验，可复现）：
把 `cards/index.ts` 的 `JsonlRecord` 换成 re-export 生成物，`tsc` 一共 **6 个错**：

| 类别 | 条数 | 说明 |
|---|---|---|
| 机械错 | **4** | 3 个是 `export type {…} from` 不把名字带进本地作用域（**C02 栽过两次，这次第三次**）+ 1 个「手写 `ApiMessage` 未使用」 |
| 真错 | **2** | 都是 `string \| null` vs `string \| undefined`，且**都能在消费侧修** |

⇒ 不是渲染层重构，是**两处边界类型放宽**。且**一行 `tabs.ts` 都不用碰**
（`tabs.ts:818` 那个错的修法在 `turn-notify.ts`，不在 `tabs.ts`）。

## 2. 承重数字（本轮实测，含一处对我自己的订正）

| 项 | 实测 | 备注 |
|---|---|---|
| Rust `JsonlRecord` variant | **12** | **我第一遍报的 11 是错的**：正则只匹配带 `#[serde(rename)]` 的，漏了 `#[serde(other)] Unknown`（unit variant） |
| TS 手抄版 variant | **8** | 缺 `permission-mode` / `last-prompt` / `file-history-snapshot` / `Unknown` |
| 要一起派生的 Rust 类型 | **4** | `JsonlRecord` · `ApiMessage` · `Usage` · `ForkedFrom`。**`ContentBlock` 刻意不派生**（下文） |
| 新增生成物 | **6** | 上面 4 个 + `JsonlLinePayload` + `JsonlBatchPayload` ⇒ 生成物 15 → **21** |
| `serde_json::Value` 字段 | **6 处**，可达 **3 处** | 只给可达的 3 处配 `ts(type = "unknown")`（不是 `any`：逼前端先做形状守卫，与 §18 宽容 schema 的读法一致） |

### 为什么不派生 `ContentBlock`

Rust 的 `ApiMessage.content` 是 `serde_json::Value` —— **压根没引用 `ContentBlock`**，
`grep` 全仓除定义与一句 doc 注释外零引用 ⇒ 它在线上**不可达**。
生成它就是「为假想消费者建抽象」（同 C03 跳过 `SftpStat` 的先例）。

**TS 侧的 `ContentBlock` 留手写**：它是对 `content: unknown` 的**解释模型**，
属于前端侧的意图类型，同账本第 4 行「IR 是前端的意图模型，别把它拖过边界」。

**顺带登记不修**：Rust 的 `pub enum ContentBlock` 疑似**死类型**（除定义外零引用）。

### 为什么保留 `| { "type": "Unknown" }`

`Unknown` 是 `#[serde(other)]` 的 unit variant，而 `messages.rs:317` 的 `is_displayable()`
白名单里**没有它** ⇒ 它实际不上线。生成物在这一格比现实**更宽**。
**刻意不加 `#[ts(skip)]`**：更宽是 fail-safe 方向（消费方必须处理一个不会发生的分支，代价为零），
更窄则要求我证明「它永不上线」——那个证明要横跨 32k 行 Rust，我不做那个证明。

## 3. 手抄版被删时暴露的三处静默漂移（都是**真发现**）

| # | 漂移 | 方向 | 后果 |
|---|---|---|---|
| **①** | TS 给 `queue-operation` 声称了一个 Rust 根本没有的 `timestamp?: string` | 手写版**更宽** | 线上永远没有它，读到的**恒为 `undefined`**。Rust 测试 `messages.rs:349` 甚至喂了 `"timestamp":"t"` 进去，enum 直接丢弃 |
| **②** | 手抄的 `Usage` 把 `cache_creation_input_tokens` / `cache_read_input_tokens` 标成 optional | 手写版**更宽** | Rust 只有 `#[serde(default)]`、**没有** `skip_serializing_if` ⇒ 线上恒有。类型声称「可能缺」而实际不缺 |
| **③** | variant 数 8 vs 12 | 手写版**更窄** | 三个真实 variant 在 TS 侧不存在；`isSidechain` 缺失正是 `turn-notify.ts:22` 那个本地最小接口的由来 |

**都不是我"顺手修"的**——它们是**换成生成物后自动消失**的。这比「如实登记不修」强。

## 4. 一处运行时早就对、类型一直在说谎的教科书例子

`src/cards/api-error.ts:70-72` 本来就写着：

> `typeof` 而非 `!== undefined`：serde 把 `Option::None` 序列化成显式 null，
> null 会穿过 undefined 判定渲染出"重试 null/null"。

**运行时早就知道线上是 `null` 并用 `typeof` 防着，是类型签名 `retryAttempt?: number` 在说谎。**
中间靠**一条注释**解释这个差异。接上生成物后 `tsc` 立刻把谎揭出来
⇒ 把签名放宽成 `?: number | null`，让类型说代码本来就防的那件事。

同一形状另外三处：`buildApiErrorCard.status` · `cardHeader.model` · `extractBranchRecord` 的三个字段。
**`extractBranchRecord` 只放宽入参，不放宽 `BranchRecord`**——那是前端自己的分支图模型
（账本第 4 行），线上的 `null` 在**入口处**归一化成 `undefined`，不往里传染。

## 5. 守卫：C04c 挖出并修掉两个**真缺口**

### 缺口 1：两条通用性质扫描**只扫 `pub struct`，不扫 `pub enum`**

字段正则还要求 `pub ` 前缀，而 **enum variant 的字段没有 `pub`**。
后果：`JsonlRecord::System.duration_ms: Option<u64>` 生成出 `durationMs: bigint | null`，
**守卫一声不吭**——我是盯生成物发现的，不是它逼我的。
修法：锚点改 `^pub (struct|enum)`，字段正则 `pub ` 改可选。
性质是「**任何**跨边界的大整数字段都要显式表态」，与它住在 struct 还是 enum variant 里无关。

### 缺口 2：字段层的属性窗口**顺序敏感** ⇒ 假红

原窗口是 `slice(skip_serializing_if 那行, 字段行)` —— 只看得见写在 serde 属性**之后**的属性。
我给 `origin` 写成 `#[cfg_attr(test, ts(optional))]` 在前、`#[serde(...)]` 在后
（与 `data_paths.rs` 的 serde-在前 **语义完全相同**），守卫**假红**，
而生成物明明已经是 `origin?: string`。
**C02 审计的 S2 只修了 struct 层的顺序敏感，字段层这个窗口漏了。**
修法：与 u64 那条同形——从字段声明行**往上**收属性块 ⇒ 顺序无关。已用双向变异验证。

### 新增一条守卫：`Option<大整数>` 配 `ts(type)` 时不许丢 `| null`

**本轮我自己踩的坑**：`ts(type = …)` 覆盖**整个**类型、不只是 `Option` 内层。
`duration_ms` 配 `"number"` 生成出 `durationMs: number` —— 丢了 `| null`，
而该字段**没有** `skip_serializing_if` ⇒ None 序列化成 `null`。两种诚实形态：

- 有 `skip_serializing_if` ⇒ 线上**省略** ⇒ `ts(optional, type = "number")` ⇒ `x?: number`
- 无 `skip_serializing_if` ⇒ 线上是 **null** ⇒ `ts(type = "number | null")` ⇒ `x: number | null`

等号计数 `toBe(2)`（`data_paths.rs::size_bytes` 走 optional 分支 · `messages.rs::duration_ms` 走 null 分支）。

### 守卫计数的变化（每个都必须红一次再更新，这是设计的）

| 断言 | 旧 | 新 | 新增的来源 |
|---|---|---|---|
| 生成物清单 | 15 | **21** | C04c 的 6 个 |
| `skip_serializing_if` ⇒ `ts(optional)` | 3 | **5** | `origin` + `stop_reason` |
| u64/i64 ⇒ `ts(type)` | 8 | **10** | `seq` + `duration_ms` |
| `Option<大整数>` 不许丢 null | — | **2** | 新断言 |

## 6. DoD

- [x] `JsonlLinePayload` / `JsonlBatchPayload` 生成，`events.ts` 消费生成物
- [x] `seq: u64` 按 C03 策略 + **按量纲**算上限（per-file 行号：2^53-1 行 ≈ 每行 1KB 时 9 EB 单文件）
- [x] `duration_ms: u64` 同样按量纲单独算（**时长 ms**：2^53-1 ms ≈ 28.5 万年）
- [x] `message` **不用逃生口**，直接生成 `JsonlRecord`（修订理由见 §1）
- [x] 4 处 `cards/index.ts` 缺口 → **换成生成物后自动消失**，不是「登记不修」
- [x] 变异验收（见 §7）
- [x] 全门禁绿且数字不降；8 套真机 152 条逐个不变

**明确不做**：不碰 `tabs.ts`（红线；实测也不需要）· 不派生 `ContentBlock`（不可达）·
不给 `Unknown` 加 `#[ts(skip)]`（更宽是 fail-safe）· 不修 Rust 那个疑似死的 `ContentBlock`（登记）

## 7. 变异验收（每条先 diff 确认落位、**再确认编译得过**，然后才判色）

| # | 变异 | diff | 编译 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A1** | Rust 侧把 `apiErrorStatus` 的 serde rename 改名 | ✔ | `cargo` rc=0 | `tsc` 精确报 `Property 'apiErrorStatus' does not exist … apiErrorStatusRENAMED: number \| null` | **成立**。证明整条链在 **enum variant 字段**上也通 |
| **A2** | 真删 `api_error_status` 字段 | ✔ | **rc=101，两轮都没编译过** | — | **作废，不算进验收**。见下方 |
| **B** | 删掉生成物清单里的 `JsonlRecord.ts` | ✔ | n/a | 守卫红并列出差异 | 成立 |
| **C** | 把 `duration_ms` 的 `\| null` 去掉 | ✔ | `cargo` rc=0 | 守卫红：「是 Option 且无 skip_serializing_if ⇒ 线上会是 null，但 ts(type) 里没有 `\| null`」 | 成立 |
| **D** | `origin` 的属性顺序换成 serde-在前 | ✔ | rc=0 | 守卫**仍绿** | 顺序无关性成立 |
| **E** | 删掉 `origin` 的 `ts(optional)` | ✔ | rc=0 | 守卫红并点名字段 | 仍有牙 |

### A2 为什么作废——**同一个陷阱的新变体，这次抓到了**

第一轮：删字段 → `cargo` rc=101（`codex_record.rs:342` 与 `messages.rs:451` 有构造/匹配点）
⇒ 那次的 `tsc rc=0` 是**无效结果**（C01 栽过的老陷阱）。
第二轮：把那两处也删掉 → **仍然 rc=101**，而 `tsc` 报的是 **`apiErrorStatusRENAMED`**
—— 它在读 **A1 遗留的过期生成物**（A2 的 cargo 失败 ⇒ `gen:types` 没重跑）。

**新教训（比原来那条更细）**：变异链上生成物是**中间产物**。
变异没编译过时，不仅「tsc 沉默」是假信号，**「tsc 变红」也可能是假信号**
——它可能在对着上一次变异的残留生成物报错。
判色前要确认的不只是「编译过了」，还有「**生成物是这次变异产的**」。
A1 测的是同一条性质且构造上必然编译，所以验收由 A1 承担，A2 如实作废。

## 8. 工程审计结果（Phase E）

**账本第 1 行（events 一族）**：C02 交付 9 个、C04b 加 1 个，C04c 再加 2 个 payload
+ 4 个传递依赖 ⇒ 该行的「最终形态：手抄镜像全部由生成物取代」**在事件半边基本达成**。

**账本要不要新增行**：不需要。`messages.rs` 是账本第 1 行那条数据流的**源头**，
不是一个新的共享面——`JsonlRecord` 只有一条消费路径（events → cards 渲染）。

**对后续的影响**：
- **C04d** 少了 6 个待生成 struct（63 → 57）；`cards/index.ts` 已经不含跨边界手写类型。
- 两条通用性质扫描现在覆盖 enum ⇒ C04d 往派生集里加任何 enum-heavy 文件时都自动受保护。
  **这一条是 C04c 给 C04d 的真正遗产**，比那 6 个生成物更值钱。

**遗留登记（不修）**：
1. Rust `pub enum ContentBlock` 疑似死类型（除定义与一句 doc 注释外零引用）。
2. 生成物比线上更宽一格（`Unknown` variant），刻意为之，理由见 §2。
3. `JsonlRecord` 的 `Unknown` 与 `Unrecognized` 两个 variant 语义相近但不同
   （前者 `#[serde(other)]` 兜未知 tag，后者是我们自造的抢救信封）——命名容易混，登记不改名
   （改名会同时动 Rust 与 TS 的判据字符串，属于行为改动）。

## 9. 签收

- [x] 通过代码审计（主线程 6 条变异，其中 1 条如实作废并记下新教训）
- [x] 通过工程审计（账本无需新增行；两条守卫缺口已修，是给 C04d 的遗产）
- [x] 主计划已据此更新（生成物 15 → 21；三条守卫计数；变更记录 10）
