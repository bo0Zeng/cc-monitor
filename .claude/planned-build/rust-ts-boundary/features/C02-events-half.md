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

**为什么 `JsonlRecord` 必须延后** —— **理由已按 Phase D 审计 I4 订正，见下方「订正」块**：

1. ~~**Rust 侧那个 enum 自认是有损的。**~~ **这条我写错了方向，已撤。**
2. **闭包触底到 `serde_json::Value`**（`ApiMessage.content`，`messages.rs` 里有 6 处 `Value`），
   而 TS 侧的 `JsonlRecord` 住在 **`src/cards/index.ts`（1187 行）**——
   那是前端卡片渲染的承重模型，其价值恰在 Rust 侧没有的 `ContentBlock` 判别联合。
   **整体替换它是一次前端渲染层的重构，不是一次类型迁移。** 这条成立。
3. **（审计补上的真正卡点）`seq: u64` 要先有 C03 的大整数策略。**
   `ts-rs` 默认把它映射成 `seq: bigint`，而手写版是 `seq: number`（wire 是 JSON 文本
   ⇒ `JSON.parse` 永不产 BigInt，C01 已就 `size_bytes` 立过这条）。
   要正确表达它得先用 `#[ts(type = "number")]`，而**那是 C03**，C02「明确不做」里写着不碰。

> ### 订正：理由 ① 是错的（C02 Phase D 审计 I4，2026-07-29）
>
> 我原先写「Rust 的 `JsonlRecord` 自认有损 ⇒ 让它当源等于把 TS 侧收窄」。
> **`history.rs:628` 那句原文我引得准确，但它说的「有损」是相对磁盘上的 jsonl 文件**——
> 而 `jsonl-line` 事件的 wire 内容**就是** `serde_json::to_string(JsonlRecord)`。
> **在那条边界上，那个 enum 不是「有损的模型」，它就是 wire 的定义。** 方向反了。
>
> **审计实测的事实与我的判断相反**：手写的 TS 侧**比 wire 更窄**——
> `cards/index.ts::JsonlRecord` 只有 8 个变体而 Rust enum 有 11 个
> （缺 `permission-mode` / `last-prompt` / `file-history-snapshot`）；
> `user`/`assistant` 变体缺 `isSidechain`（Rust 侧 `is_sidechain: bool`，`serde(default)`
> 无 skip ⇒ **每行都在 wire 上**），以致 `src/turn-notify.ts:27` 不得不**再手抄一份局部
> interface** 才拿得到它。
> **⇒ 做掉它会加宽 TS 侧、当场暴露这 4 处缺口，不会收窄。**
>
> 那 4 处缺口是**今天就存在的 latent 缺口**，不是本轮引入的 —— **单独登记**，见下。
>
> **审计还实测跑通了一条更便宜的正确做法**（变异 X，已还原）：不替换 `cards/index.ts`，
> 只在 `message` 字段上用逃生口指回它 ——
> `#[cfg_attr(test, ts(type = "import('../cards').JsonlRecord"))]` + `origin` 配 `ts(optional)`，
> `tsc --noEmit` 0 错、`origin?: string` 与手写版逐字一致。**这条路写进 C03/C04 的选项里。**
>
> **为什么这条订正比那个阻塞更该记**：写错的理由会让下一个人以为它卡在一个架构决定上，
> 而实际卡点是 C03 的一个具体属性。**结论（延后）没变，但依据换了。**

⇒ **C02 覆盖 8 个 payload + `TaskEntry`**：`SessionEnded` · `SessionIdle` · `SessionStarted` ·
`RemoteSessionAdded` · `RemoteHealth` · `SessionActivity` · `FrontendReady` · `TasksUpdate`。
**`JsonlLine` / `JsonlBatch` 登记延后**，等有人先决定「Rust 的 `JsonlRecord` 要不要变成
无损模型」——那是一个独立的产品/架构决定，不该被一次类型迁移顺手做掉。

**顺带记一条 C01 那条通用守卫的第一次真实收益**：`JsonlLinePayload.origin` 也带
`skip_serializing_if`（`bridge.rs:78`），`TaskEntry` 有 2 处。它们全都会被那条扫描要求配
`ts(optional)` —— **这条守卫是 C02 开工前就已经在替我数活了**。

### 登记给 C04：`SessionActivityPayload` 还有第二条消费路径没硬化（审计 I2）

`src/tabs.ts:1629-1632` 仍是**内联字面量**：
`invoke<{ session_id: string; status: string | null; waiting_for: string | null }[]>("list_session_activity")`,
而 `src-tauri/src/lib.rs:1489-1491` 的签名是 `-> Vec<bridge::SessionActivityPayload>`
——**就是 C02 刚生成的那个类型**。

审计变异实证：给 `waiting_for` 加 `#[serde(rename = "waitingFor")]` → `tsc` **只红 1 条**
（`main.ts:712`，事件路径），`tabs.ts:1632`（F5 快照路径）**不红**，运行时 `a.waiting_for` 变 `undefined`。

**归属上它是命令 ⇒ 落在 C02「明确不做：不碰命令半边」里，不算越界。**
但它说明一件事：**「共享面」的正确粒度是「类型」而不是「半边」**——
同一个 struct 同时被事件与命令消费，只硬化事件那一路，留下的正是 C02 自己命名为
「最危险」的那种形态（内联字面量：没有名字，漂移时没有任何东西会红）。

类型已经生成好了，改这一行零风险 ⇒ **C04 第一批做掉**。
另：C02 §2 ② 把内联字面量数成 2 个（`events.ts:396`/`:465`），**那个数漏了**——
至少还有 `tabs.ts:1632` 与 `main.ts:744`（`ActiveSessionPayload`，已登记归 C04）。

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
- [x] **生成物忠实复现混合的线上格式** —— **数字按交付集重述**（审计 I5：原文写「3 个 camelCase、
      8 个 snake_case」，那是 `bridge.rs` **11 个 struct** 的统计，不是**交付的 9 个生成物**的，
      按原措辞无法逐项核）。交付集实测：**camelCase 容器 3 个**（`RemoteHealthPayload` /
      `TasksUpdatePayload` / `TaskEntry`）+ **snake_case 5 个** + **字段级 rename 1 个**
      （`FrontendReadyPayload`）。审计另用序列化探针对 9 个类型各构造 all-Some/all-None 两版
      逐字节对拍，**没核漏**，含「`RemoteSessionAddedPayload` 与被删的内联字面量字段名/顺序/
      可空性逐一相同」这条。 ：3 个 camelCase、8 个 snake_case，**一个都不改**
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
2. ~~10 个~~ **8 个** struct + `TaskEntry` 加派生（`FrontendReady` 只有 `Deserialize` —— **先确认 `ts-rs` 对纯 `Deserialize`
   的结构照样生成**；若不生成，那是一个要单独处置的发现，不是绕过理由）。
3. **逐个核对生成物字段名与线上格式一致**（交付集：3 camelCase 容器 / 5 snake_case / 1 字段级 rename）。
   任何一个对不上就停下——那说明 `ts-rs` 对某个 serde 形态的处理与我预期不符。
4. 替换 `events.ts` 的 ~~6 个~~ **5 个**（`JsonlLinePayload` 留手写）+ `remote-health.ts` 的 1 个
   + `tasks-panel.ts` 的 `TaskEntry`。
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

### Phase D 审计闭环（2026-07-29，89 次工具调用 / 13 组变异 / 全部还原）

审计报 **1 阻塞 + 6 重要 + 5 建议**。**逐条独立复现后的处置**：

#### 阻塞 B1（成立，已修）：`skip_serializing_if` 扫描的 400 字符窗口会被**隔壁字段**喂饱

我先读代码算了一遍窗口距离，再用变异实测复现：只删 `description` 那行 `ts(optional)`
⇒ 编译过（547 绿）· 生成物退化成 `description?: string | null`
（**正是守卫注释里点名要防的那个「过度宽松」形态**）· **守卫 6/6 全绿 · `tsc` 0 错**
· C05 的 diff 门禁也不红（源与生成物是一致的）。**整条门禁栈静默。**

**审计那个洞察比指控本身更值得记**：**这条性质在 C01 时是真的**——那时只有 1 处
`skip_serializing_if`，窗口内无处可抄。**是 C02 把它扩到第 2 处相邻同构字段的那一刻失效的，
而没人碰过守卫的逻辑。** 而且 C02 顺手拆掉了第二道防线：我 commit message 写
「与手写版 `tasks-panel.ts` 不一致、tsc 当场报错」——那句在我观察的那一刻是真的，
但 C02 正是把那个手写版删掉的那次改动。**两件事合起来才造成这个洞。**

→ **修法**：窗口从「前视 400 字符」收到**同一字段的属性块**（`skip_serializing_if` 到该字段
自己的 `pub ` 声明之间）。**变异验证**：同一个变异现在红，且报文精确到字段名。

#### 顺带治了 S2（切块对属性顺序敏感），因为 C04 要复制这个形状 127 次

按 `#[derive` 边界切块时，若有人把 `cfg_attr` 写在 `#[derive]` **之前**，
struct 体会落进不含 `ts_rs::TS` 的块而被跳过。→ 改成**以 `pub struct` 为锚往上收全部属性**，
与顺序无关。**变异验证**：按非惯用顺序给 `JsonlLinePayload` 补派生且不配 `ts(optional)` → 红。

**改这条时我的新逻辑当场被自己的计数自检抓到一个 bug**：往上收属性时只跳 `#[` 行，
而 `code()` 把注释行变成**空行**（不是删掉），于是在 `data_paths.rs`（属性与 `pub struct`
之间有一整段解释注释）处停住 ⇒ 整个 struct 被跳过 ⇒ `toBe(3)` 报「实得 2」。
**那条计数自检不是装饰。**

#### 重要 I1（成立，已修）：常量正则漏掉名字带数字的常量

`[A-Z_]+` 遇到 `SESSION_IDLE2:` 只吃到 `SESSION_IDLE`，随后 `:` 匹配 `2` 失败 ⇒ 整条被跳过
⇒「加事件必须红一次」的承诺对这一类新增不成立。→ `[A-Z0-9_]+`。**变异验证**：加一个
带数字的常量 → 现在红（实得 11）。本仓命名习惯（Batch7-F24 / SS-F / F03.2 / issue #32）
让带数字的常量名相当现实。

#### 重要 I3（成立，已修）：`frontend-ready` 的名字不被任何东西钉死

而 **C02 恰好是「给它首次上类型」的那一次**。核实：`lib.rs:753` `app.listen("frontend-ready")`
↔ `main.ts:775` `emit(...)`，两侧都是裸字面量，`bridge.rs::events` 里**没有**对应常量
⇒ 新守卫的 10 个名字不含它。**给它上了类型却把名字漏在门禁外。**
→ 加 `pub const FRONTEND_READY`、`lib.rs` 改用常量、守卫计数 10 → **11**。
加常量零行为变化（同一字面量，只是有了名字）。

**附带一条也核了**：守卫的 `tsFiles` 白名单里的 `main.ts`，原先那 10 个字面量**一个都没有**
（逐个 grep 确认）⇒ 它只扩大了搜索面、没换来覆盖。**修掉 I3 之后它才真正有用**
（`main.ts` 现在含 `"frontend-ready"`）。

#### 重要 I4（成立，**这条比阻塞更该记**）：我写错了一条延后理由

见 §2 的「订正」块。摘要：我写「Rust 的 `JsonlRecord` 自认有损 ⇒ 让它当源等于把 TS 侧收窄」
——**方向反了**。那句 `history.rs:628` 的原文我引得准确，但它说的「有损」是相对**磁盘上的
jsonl 文件**，而 `jsonl-line` 事件的 wire 内容**就是** `serde_json::to_string(JsonlRecord)`
⇒ 在那条边界上它**就是 wire 的定义**。审计实测手写的 TS 侧**反而更窄**
（8 变体 vs 11、缺 `isSidechain` 以致 `turn-notify.ts:27` 再手抄一份局部 interface）
⇒ 做掉它会**加宽** TS 侧、当场暴露 4 处 latent 缺口。真正卡点是 `seq: u64` 要先有 C03。
**结论（延后）没变，依据换了。** 那 4 处缺口另行登记。

#### 重要 I2（成立，登记给 C04）· I5（成立，已修）· I6（成立，已修）

- **I2**：`SessionActivityPayload` 还有第二条消费路径（`tabs.ts:1632` 内联字面量，命令路径）
  没硬化。归属上落在「明确不做：不碰命令半边」里**不算越界**，但它说明
  **「共享面」的正确粒度是「类型」而不是「半边」**。已登记进 §2，C04 第一批做掉。
- **I5**：DoD 那条 3/8 是 `bridge.rs` 11 个 struct 的统计而非交付集的、§5 三处步骤没跟上
  范围订正、MASTERPLAN 三处 + 变更记录缺 C02 ⇒ 全部改准。
- **I6**：`tasks.rs` 注释把生成结果写成 `active_form?: string`，实际是 `activeForm?: string`
  ——**那段的整个论点就是「字段名由 serde 决定」，写错等于自相矛盾**。已改。

#### 审计独立复核确认「做对了」的（摘要）

用**序列化探针**对 9 个类型各构造 all-Some/all-None 两版逐字节对拍，确认「绝不统一线上格式」
**没核漏**（含「`RemoteSessionAddedPayload` 与被删的内联字面量字段名/顺序/可空性逐一相同」）·
`ts(optional)` 的推理**双向实证** · `FrontendReadyPayload` 纯 `Deserialize` 照样生成，
且生成的**必需**字段让「首次上类型」真有牙（加一个必需字段 → `tsc` 红 `TS2741 @ main.ts:774`）·
**对外 API 逐字节不变是在编译产物层验的**（`esbuild` 转译 HEAD 与 HEAD~1 对比：两个文件
逐字节相同、`events.ts` 只多两行注释、三文件产物里无 `generated` 字样 ⇒ 生成物从未进 bundle）·
无运行时行为变化（五路取证）· 事件名钉死那组**不是安慰剂**（六种变异都杀得掉，`code()` 两侧都剥）。

#### 以下是我自己在 Phase C 撞到并处置的（审计已独立复核）

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

## 8. 工程审计结果（Phase E）

- **主计划自洽**：C02 未引入新耦合。共享面 1 已扩到 4 个文件并写明交付 9 个类型。
- **给 C03/C04 的三条硬交接**：
  1. **`JsonlLine`/`JsonlBatch` 卡在 C03 的 `seq: u64`**（不是卡在架构决定上——见 §2 订正块）。
     审计实测跑通了一条更便宜的路：`#[ts(type = "import('../cards').JsonlRecord")]` 逃生口，
     不替换 `cards/index.ts` 也能生成，`tsc` 0 错。**写进 C03/C04 的选项。**
  2. **C04 第一批做掉 `tabs.ts:1632`**（I2）。教训是**「共享面」的粒度该是类型不是半边**。
  3. **守卫的切块与窗口逻辑现在是 C04 要复制 127 次的形状**，所以 B1/S2 在这一轮治掉
     而不是留给 C04——那时治要改 127 处的上下文。
- **技术债，如实登记**：`cards/index.ts::JsonlRecord` 比 wire 窄 4 处
  （缺 3 个变体 + `isSidechain`），**今天就存在、不是本轮引入**；`turn-notify.ts:27`
  为此手抄了一份局部 interface。**做 `JsonlRecord` 时会当场暴露它们。**
- **`eslint` 基线是 rc=1（7 条 error）**，全在 C02 未碰的文件、`src/generated/` 零告警
  ——**不是本轮引入的**，记一笔免得后人误判。

## 9. 签收

- [x] **通过代码审计** —— Phase D 闭环：1 阻塞 + 6 重要 + 5 建议，逐条独立复现后处置。
      阻塞与三条修复各配一次变异验证；**其中一条（I4）是对我自己写下的推理的订正，
      比那个阻塞更该记**。
- [x] **通过工程审计** —— 见 §8，含三条给 C03/C04 的硬交接与两条如实登记的技术债。
- [x] **主计划已据此更新** —— §1 功能表状态与目标 · §3 共享面 1 · 变更记录 06。
