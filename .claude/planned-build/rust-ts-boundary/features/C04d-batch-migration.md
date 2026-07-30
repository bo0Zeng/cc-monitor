# C04d — 按模块分批迁移（rust-ts-boundary 的最后一个功能）

> 主计划：`../MASTERPLAN.md` · 前置：C01/C05/C02/C03/C04a/C04b/C04c 均已闭环
> **本功能是多批次的**：每批一个或一组模块、一个 commit、一次全门禁。

## 1. 承重数字（本轮实测，含两处对上一轮的订正）

| 项 | 实测 | 上一轮记的 | 说明 |
|---|---|---|---|
| 已派生 `ts_rs::TS` 的类型 | **21** | 21 | 与生成物文件数一致 |
| 仍未派生的 `pub struct` + `Serialize` | **53** | 「57」 | **订正**：57 是我上一轮的估算（77 − 21 + 口径校正），实测重数是 53 |
| 仍未派生的 `pub enum` + `Serialize` | **10** | 未记 | **新数**：enum 也跨边界（C04c 已让守卫覆盖 enum），要一并计入 ⇒ 未派生总数 **63** |
| `invoke` 调用点 | **143** | 143 | 一致（生产 + 剥注释 + 按调用表达式） |
| 直接 `import { invoke }` 的生产文件 | **29** | 29 | 一致，有等号守卫盯着 |

**按文件的调用点分布**（29 个文件，从少到多）：11 个文件各 1 处 · 2 个各 2 处 ·
4 个各 3 处 · 2 个各 4-5 处 · 然后 `cc-bus-section` 6 · `cc_integration` 7 ·
`main.ts` 9 · `remote-section` 9 · `mcp-section` 10 · `sftp/panel` 12 ·
`views/history` 14 · **`tabs.ts` 15** · **`panorama/api.ts` 21**。

## 2. 批次划分（先易后难，每批独立可验收）

**排序原则不是「文件大小」而是「这一批会不会引入新的判断」**：
先把不需要任何新 Rust 派生的迁完（验证批次流程本身），再逐批处理需要生成类型的。

| 批 | 范围 | 需要新 Rust 派生？ | 29 → | 状态 |
|---|---|---|---|---|
| **1** | 6 个文件 / 5 条包装层条目，**零新派生** | 否 | **23** | **完成** |
| **2** | 需要生成类型的 1-调用点文件（`account-usage` 内联字面量 · `cards/subagent` · `ccm-probe` · `settings/config-surface-section`） | 是（**7 个**：4 个命令返回类型 + 3 个传递依赖，含 1 个内部标记枚举） | **19** | **完成** |
| **3** | 2-3 调用点一组（`config` · `views/usage-view` · `views/port-forward` · `settings/accounts-section` · `settings/cc-bus-hooks-section`） | 是（**8 个**，含 2 个内部标记枚举） | **14** | **完成** |
| **4** | `account-restart` 4 · `settings/diagnostics-section` 5 | 是（**4 个**，含 3 个大整数、两个量纲） | **12** | **完成** |
| **4b** | **`accounts.ts` 4** | 是（`accounts.rs` 6 个类型） | 11 | **已跳过——被跨工作区冲突协议挡住**（详见 §3d） |
| **5a** | `settings/cc-bus-section` 6 · `settings/cc_integration` 7 | 是（**10 个**，含 3 个**非 pub**） | **10** | **完成** |
| **5b** | `main.ts` 9 · `settings/mcp-section` 10 | 是（**1 个**；`main.ts` 零新派生） | **8** | **完成** |
| **5c** | `settings/remote-section` **9**（不是 8，见 §3g） | 是（**6 个**） | **7** | **完成** |
| 6 | `sftp/panel` · `views/history` · `views/session-viewer`（**含 3 个动态派发口**） | 是 | 3 | 待做 |
| **7** | `panorama/api.ts` 21（最大单文件） | **只 1 个**（其余 10 个受 vendor 铁律阻塞，见 §3k） | **3**（=最终形态） | **完成 · 成功标准 4 达成** |
| — | **`tabs.ts`（15 处）** | — | — | **已跳过，等授权** |

**最终形态**：`import { invoke }` 只剩 `src/ipc/commands.ts`（那 1 个）
+ **`tabs.ts`（等授权）** + **一个动态派发逃生口**。这正是主计划 §0.1 成功标准 4 改写后的形态。

### ★ 3 个「动态派发口」——查实后**不需要逃生口**（批 6a 定稿，推翻原计划）

原计划要在 `commands.ts` 里另导出一个 `invokeDynamic(name, args)` + `DYNAMIC_ONLY` 白名单。
**批 6a 查实三处的实际形状后，这个设计不该做：**

| 位置 | 实际形状 | 是不是真动态 |
|---|---|---|
| `views/session-viewer.ts:211` | `const ipc = origin ? "stream_read_remote_session" : "stream_read_session_jsonl"` | **不是**——两个**字面量**之间的三元 |
| `views/history.ts:489` | `const ipc = proj.origin ? "stream_remote_history_sessions" : "stream_history_sessions_in_project"` | **不是**——同上 |
| `sftp/panel.ts:485` | `doWrite(cmd: string, args)` 转发 helper | **不是**——**调用方传的全是字面量** |

⇒ 「动态」只在于名字从一个**封闭、静态可知的集合**里选。
**为一件其实是静态的事加一个 `string` 键的后门，方向是错的**——那等于亲手造一个
守卫扫不到的洞，而这个工作区整轮都在治「守卫扫不到」。

**新处置：三处都改成静态调用，`invokeDynamic` 不做。**
连带后果（都是好的）：
1. C04a 记的「7 个命令 TS 静态看不见」这个**已知盲区会整体消失**
   ⇒ TS 字面量命令名从 **112 → 119**（批 6a 已到 114），`DYNAMIC_ONLY` 最终 `toEqual([])`。
2. **最终形态从 4 变 3**：1 包装层 + `tabs.ts`（等授权）+ `accounts.ts`（等 Z02）
   ——**没有动态派发口这一项**。主计划 §0.1 成功标准 4 要按这个改。
3. 每条命令拿到**自己精确的签名**，而不是共用一个超集 args（见下）。

## 3. 批次 1 明细（本 commit 交付）

**选它的理由**：这 6 个文件的 5 个命令，Rust 返回类型全部落在「不需要新派生」的桶里
⇒ 能把**批次流程本身**（包装层加条目 · 换调用点 · 三条等号守卫同时动 · 全门禁 + 真机）
验证一遍，而不必同时判断类型生成的对错。

| 文件 | 命令 | Rust 返回 | §5 桶 | 包装层签名 |
|---|---|---|---|---|
| `e2e-probe.ts` | `frontend_perf_log` | 无返回（`()`） | ① | `(args: { lines: string }) => Promise<void>` |
| `events.ts` | `frontend_perf_log` | 同上 | ① | **同一条目**（一个命令两个调用方） |
| `error-toast.ts` | `open_log_file` | `Result<(), String>` | ① | `() => Promise<void>` |
| `remote-launch-run.ts` | `launch_remote_terminal` | `Result<(), String>` | ① | `(args: { origin: string; remoteCmd: string }) => Promise<void>` |
| `views/pane-preview.ts` | `capture_remote_pane` | `Result<String, String>` | ③ 但是**原始类型** | `(args: { origin: string; target: string }) => Promise<string>` |
| `tasks-panel.ts` | `get_session_tasks` | `Result<Vec<TaskEntry>, String>` | ③ | `(args: { sessionId: string }) => Promise<TaskEntry[]>`（`TaskEntry` C02 已生成） |

**一条值得记的细节**：`frontend_perf_log` 有**两个**调用方（`e2e-probe.ts` 与 `events.ts`），
这正好验证了包装层的价值——原来两处各自手写命令名，现在共用一条。
也验证了守卫第 3 条的「唯一名计数」在「命令名从两处裸调用搬进一处包装层」时**保持 112 不变**
（唯一名集合没变，只是出现位置变了）。

### 守卫计数的变化（每个都先红一次再更新）

| 断言 | 旧 | 新 |
|---|---|---|
| 包装层条目 | 1 | **6** |
| 直接 `import invoke` 的生产文件 | 29 | **23** |
| TS 字面量命令名（唯一） | 112 | **112**（不变——集合没变，只是位置变了） |

### 变异验收

| # | 变异 | 判色 | 结论 |
|---|---|---|---|
| **A** | 把 `get_session_tasks` 条目的 `invoke` 字面量抄成另一个**真实存在**的命令（`"open_log_file"`） | `tsc` **rc=0**（管不了）· 守卫红：「包装层条目 get_session_tasks 应当恰好调一个字面量命令名: expected [ 'open_log_file' ] to deeply equal [ 'get_session_tasks' ]」 | 成立。**C04a 那条对拍第一次面对带参数的多行条目，抓住了** |
| **B** | 把 `tasks-panel.ts` 改回裸 `invoke` | 「期望恰好 23 个，实得 24」 | 成立 |
| **C** | 三条等号在改动落地时**自动各红一次**（未刻意变异） | 包装层 `1 → 6` 红 · 文件数 `29 → 23` 红 · **字面量唯一名 `112` 没红** | 成立，且第三条印证了预判：命令名从裸调用搬进包装层，唯一名集合不变、只是位置变了 |

## 3b. 批次 2 明细

7 个派生：`AccountUsageProbeResult`（camelCase）· `SubagentLoadResult` · `CcmProbeResult` ·
`ConfigSurfaceReport` + 传递依赖 `SurfaceRow` / `SettingsScope` / `SurfaceState`（后者是
`#[serde(tag = "kind", rename_all = "snake_case")]` 的**内部标记枚举** → 生成判别联合）。
生成物 21 → **28**；`import invoke` 的生产文件 23 → **19**；包装层 6 → **10**。

**C04c 的投资在这里第一次收息**：`SubagentLoadResult.records: Vec<JsonlRecord>` 的传递依赖
**C04c 已经生成好了**，否则这一批得先啃那个 12-variant 的 enum。

**本批次零漂移**：4 个 TS 手写版与生成物**逐字等价** ⇒ 这一批的价值是**防将来漂**，
不是抓到了 bug。**如实这么说**，别把「防御性收益」讲成「发现了问题」。

**一处设计判据的复用**：`ccm-probe.ts` 同时有 `RawCcmProbeResult`（线上形状）和另一个
`CcmProbeResult`（TS 侧领域类型，`capabilities: Set<string>`）。**线上的换生成物、领域的留手写**
——同 C04c 处置 `ContentBlock` 的判据。故 import 时用 `as RawCcmProbeResult` 别名避免撞名。

### 批次 2 变异

| # | 变异 | 编译 | 生成物是本次产的？ | 判色 | 结论 |
|---|---|---|---|---|---|
| A | 给 `SurfaceState` 加一个 Rust variant | **rc=101** | — | — | **作废**（Rust 有穷尽 match） |
| B | 删 `SubagentLoadResult.agent_id` | **rc=101** | **否**（grep 到生成物里 `agent_id` 还在） | — | **作废** |
| **A′** | 给 `SurfaceState::Absent` 加显式 `serde(rename)` | rc=0 | ✔ grep 到 `absent_RENAMED` | `tsc` 同时报**生产代码**的 `case "absent"` **与它的 vitest** | **成立**。内部标记枚举的判别联合窄化真被消费 |
| **B′** | 给 `agent_id` 加 `serde(rename)` | rc=0 | ✔ | `tsc`: `Property 'agent_id' does not exist on type 'SubagentLoadResult'` | 成立 |
| C | 三条等号在改动落地时自动各红一次 | — | — | 生成物清单 `toEqual` 红 · 文件数 `23 → 19` 红 · 包装层 `6 → 10` 红 | 成立 |

**C04c 那条教训当场生效**：A/B 两条我**在判色前就发现无效**——因为我这次在判色步骤里加了
「grep 生成物确认它是本次变异产的」。B 的 grep 显示 `agent_id` 还在生成物里
⇒ `tsc rc=0` 是在读过期产物，不是「链条没牙」。**把那条教训变成了流程里的一步，而不是记忆。**

## 3c. 批次 3 明细

12 条包装层条目（10 → **22**）· 8 个派生（生成物 28 → **36**）· 5 个文件迁走（19 → **14**）。
两处**前序功能的投资在收息**：`UsageBucket → UsageTotals` 是 **C03** 生成的；
`SubagentLoadResult → JsonlRecord` 是 **C04c** 的（批 2）。

### 抓到两处真漂移

| # | 漂移 | 方向 | 后果 |
|---|---|---|---|
| ① | `AcctIsoStatus`：TS 写 `invoke<{ installed: boolean }>`，Rust 返 **3 个字段** | 手写版**更窄** | `path` 与 `vendor_id` 被藏掉。而 Rust 那两个字段的注释明写「附带回传，避免以后要它时再加一趟往返」⇒ **手写镜像把后端的好意抹掉了** |
| ② | `SessionUsageRow.origin`：手写 `origin?: string` | 手写版**说错了缺省语义** | Rust 是 `#[serde(default)] Option<String>` 且**无** `skip_serializing_if` ⇒ 线上**恒有该键、值可能为 null**（不是省略）。下游 `usage-pivot.test.ts` 的 **10 个夹具**都在造一个线上不存在的形状，本批次补上 `origin: null` 让夹具忠于线上 |

`ForwardStatus` / `HooksReport` 一族（4 个类型）与手写版**逐字等价** ⇒ 那几处零漂移，价值是防将来漂。

### 一处结构性缺口，如实登记不假装解决

`load_config` / `save_config` 的 Rust 签名是 `Result<serde_json::Value, String>`
——**它把配置当不透明 JSON 透传，Rust 自己就不知道形状**。这个边界**结构性无法**由生成物加固；
`Record<string, unknown>`（TS 的 `Config` 就是它的别名）已经是最诚实的类型。**不是我引入的缺陷。**

### 对三桶规则的一处细化

`aggregate_usage_all` 返回 `Result<u32, String>`，TS 侧今天不读它。按 §5 桶② 该写 `unknown`，
但那在这里**过度**了：桶② 的用意是「不为没人消费的 **payload 结构**生成类型」，
而这是个**原始类型**，写 `number` 零成本且更诚实。同理 `start_forward`/`deploy_remote_acct_iso`
（`String`）写 `string`。**桶② 只管结构体，不管原始类型。**

### ★ 守卫的一个真缺口：手写的派生源清单，我在同一个坑里踩了第二次

变异「删掉 `ForwardStatus.conn_count` 的 `ts(type)`」→ **守卫全绿**。
原因：`TS_DERIVING_SOURCES` 是**手写数组**，而批 2/批 3 在 **7 个新文件**里加了派生却没同步它
⇒ 两条通用性质（`skip_serializing_if ⇒ ts(optional)` · 大整数 ⇒ `ts(type)`）对它们**完全失效**。
`conn_count` 当初配上 `ts(type)` 只是因为我手工盯了生成物 ——
**与 C04c 的 `duration_ms` 同一个失效模式。**

**修法不是补清单，是取消清单**：改成递归全仓自动发现含 `ts_rs::TS` 的 `.rs` 文件，
再用等号钉住**文件个数**（`toBe(13)`）——范围不会漏，但扩大范围要被看见。
自动发现当场多扫出 1 个大整数字段（10 → **11**，正是 `conn_count`）。
重跑变异：两条性质在新文件上都有牙了，且 `conn_count` 那次顺手证实生成物真会回落成 `bigint`。

### 一条被迁移打空的老守卫（不是删，是适配）

`cc-bus-hooks-section.vitest.ts` 的【B04-2】断言「本文件只有三条**只读** invoke」——
文件迁走后它的扫描器只认裸 `invoke("name")` ⇒ 扫到**空集**、`toEqual` 红。
**这条守的性质与 C04a 那条 119 命令守卫不同，不被替代**：C04a 只保证名字存在，
这一条保证「**这个面板只碰只读命令**」（B04 立的安全不变量）。
⇒ 让扫描器同时认 `commands.xxx(`，并**加一条非空自检**——空集必须是失败而不是通过，
因为「迁移把调用形态换掉」正是这一格会静默变空的地方。

### 批次 3 变异

| # | 变异 | 编译 | 生成物是本次产的 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A** | `HookState::PathMissing` 加显式 `serde(rename)` | rc=0 | ✔ | `tsc` 报 `case "path-missing"` 不可比 + `Property 'path' does not exist on type 'never'` | 成立 |
| **B** | 删 `conn_count` 的 `ts(type)` | rc=0 | ✔（生成物变成 `connCount: bigint`） | **先是全绿（缺口）→ 改成自动发现后红并点名字段** | **抓到守卫缺口** |
| **B′** | 给 `config_surface.rs::path_resolved` 加 `skip_serializing_if` 不配 `ts(optional)` | rc=0 | ✔ | 守卫红并点名字段 | 自动发现后另一条性质也有牙 |
| C | 三条等号自动各红一次 | — | — | 生成物清单 · 文件数 19→14 · 包装层 10→22 | 成立 |

## 3d. 批次 4 明细 + **一处被协议挡住的拆分**

### 先读协议再动手，结果就是不能全做

主计划 §3 的跨工作区冲突协议原文：

> `src/accounts.ts` | 本区 C04 · `account-zero` Z01/Z02 | **`account-zero` 优先**。
> 本区 C04 迁移 `accounts.ts` 必须排在 `account-zero` Z02 之后，
> 否则会在**一个正在变形的类型上**做机械迁移

而 `account-zero` 的 Z01/Z02 **卡在外部授权上**（要动 `~/.claude/skills/cc-acct-iso/`），至今未做
⇒ **`accounts.ts` 那一份被协议挡住，拆成批 4b**。
**注意这不是 `tabs.ts` 红线**，是另一条独立的阻塞原因，两者别混。

**核实了会不会连带**：`accounts.ts` 的 3 个类型化调用（`list_remote_accounts` →
`AccountsResult` · `list_remote_session_accounts` → `SessionAccountsResult` ·
`check_account_trust` → `AccountTrustResult`）**全部**来自 `accounts.rs`；
而 `account-restart.ts`（`tmux_send_keys` ×3 · `kill_remote_tmux`）与
`settings/diagnostics-section.ts`（`logging.rs` 那几个）**一个都不碰 `accounts.rs`**
⇒ 两者可以安全先做。**这是先量再切，不是猜着切。**

### 交付

6 条包装层条目（22 → **28**）· 4 个派生（生成物 36 → **40**）· 2 个文件迁走（14 → **12**）。

**本批次零漂移**：4 个 TS 手写版与生成物**逐字等价**，包括 `RestartHint`
——它是**只有 unit variant 的外部标记枚举**（`rename_all = "snake_case"`，**没有** `tag`）
⇒ 线上就是字符串，生成物给的 `"none" | "needs_restart"` 与手写版完全相同。
价值是防将来漂。

### 三个大整数，两个量纲，分开论证

`LogFileInfo.current_size_bytes: u64` 与 `LogFileEntry.size_bytes: u64` 是**字节数**
（2^53-1 B ≈ **8 PB**，同 `SftpEntry.size` 那条）；`LogFileEntry.modified_ms: i64` 是
**毫秒时间戳**（≈ **28.5 万年**）。**刻意分开写**——把两个量纲混成一条论证是 C03 明确禁止的。
大整数字段计数 11 → **14**。

### 一处包装层的附带收益

换完调用点后 `LogFileInfo` / `LogFileEntry` / `RestartHint` 三个 import 变成**未使用**
——因为包装层的签名已经提供了类型，调用点不再需要本地标注。**这正是包装层该有的效果**，
删掉即可（它们仍被 `ipc/commands.ts` 与生成物之间的 import 链消费，不是死文件）。

### 批次 4 变异

| # | 变异 | 编译 | 生成物是本次产的 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A** | `RestartHint::NeedsRestart` 加显式 `serde(rename)` | rc=0 | ✔ | `tsc` TS2367：「types 'RestartHint' and `"needs_restart"` have no overlap」 | 成立。抓的是**字符串字面量比较**——运行时会静默失效的那一类 |
| **B** | 删 `modified_ms` 的 `ts(type)` | rc=0 | ✔（生成物变成 `modified_ms: bigint`） | 守卫红并点名字段 | 成立。**关键点：`logging.rs` 是本批次新加派生的文件，我没做任何清单维护它就自动进了守卫范围** ⇒ 批 3 那个「取消手写清单」的修法**在一个批次内就还本了** |
| C | 五条等号自动各红一次 | — | — | 生成物清单 · 派生源文件数 13→14 · 大整数 11→14 · 文件数 14→12 · 包装层 22→28 | 成立 |

## 3e. 批次 5a 明细：**两个新的守卫覆盖缺口**

12 条包装层条目（28 → **40**）· 10 个派生（生成物 40 → **50**）· 2 个文件迁走（12 → **10**）。
**本批零漂移**（9 个手写版与生成物逐字等价；TS 侧那个 `LegacyEntry` 只是名字不同、结构一致）。

### 缺口 3：守卫的类型头锚点是 `^pub (struct|enum)`，**非 pub 的跨边界类型看不见**

`lib.rs` 里 `CcStatusResponse` / `LegacyProfileEntry` / `CcPreviewResponse` 三个是**非 `pub`** 的
（模块内可见即可，它们只作命令返回类型），但**一样跨边界、一样该受那两条性质约束**。
⇒ 锚点放宽成 `^(pub )?(struct|enum)`。**「是不是 pub」与「会不会跨边界」无关**——
这是「范围必须等于性质的范围」这条纪律的第三次应用（前两次：不扫 enum · 手写清单）。

**变异验证**：给非 pub 的 `CcStatusResponse` 加一个不配 `ts(type)` 的 `u64` 字段 → 守卫红并点名。

### 缺口不是缺口的一例：`usize` 刻意不纳入大整数性质

`CcBusState.skipped: usize` —— **实测 `ts-rs` 把 `usize` 映射成 `number` 而不是 `bigint`**
⇒ 本条性质（「不许回落到 bigint」）对它**不适用**，不是漏。
64 位下 `usize` 确实能装超过 2^53，但那是**另一条性质**（精度上限），
而本仓的 `usize` 用在计数上（坏行数）⇒ 不构成风险。**已把这个事实写进那条断言的注释**，
免得下一个人以为是缺口而"顺手补上"。

### ★ 一条反向自检自己成了阻碍——校准于旧世界的下界

「直接 import invoke 的生产文件数」那条的反向自检原本写 `hits.length > 10`，
**校准于这个数还是 29 的年代**。而 C04d 的**目标就是把它降到 3-4**
⇒ 那个下界会在迁移快成功时**挡住正确的进展**，逼人去改自检而不是去看性质。

**修法**：反向自检该问「**遍历有没有工作**」，不该问「命中多少」
⇒ 改成断言**扫过的 `.ts` 文件数** `> 100`。
**教训**：反向自检的阈值不能挂在「被优化的那个量」上，否则它会随进展变成假红。

### ★ 对「判色三步」的一处细化

变异 A（给非 pub 结构加 u64 字段）**cargo rc=101 没编译过**，但**守卫仍然有效地红了**
——因为这条守卫扫的是 **Rust 源文本**，不是构建产物。

⇒ 「必须 cargo rc=0 + grep 生成物确认新鲜」这两步，**只适用于依赖生成物的 `tsc` 类变异**；
**扫源码的守卫，编译状态与它的判据无关**。判色前要问的是
「**我这次判色依赖的那个东西，是不是本次变异产的**」——对 tsc 是生成物，对源码扫描器是源文件本身。

### 一处我自己造的错：包装层实参名是**猜的**

我先按 TS 调用点猜了 `profilePath` 等参数名，与 Rust 签名不符。
**按 Rust 签名（真相）逐个核准后订正四处**：`cc_integration_install`/`_uninstall` 用 `path` 不是
`profilePath` · `cc_integration_scan_path` 还要 `commandName` · `cc_integration_status` 的
`commandName` 在 Rust 侧是 `Option<String>` ⇒ TS 侧可省。
**教训：实参名要从 Rust 签名量，不能从调用点猜**（调用点也可能一直在传错名字而没人发现）。

### 批次 5a 变异

| # | 变异 | 编译 | 判据来源新鲜 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A** | 非 pub 的 `CcStatusResponse` 加不配 `ts(type)` 的 `u64` 字段 | rc=101 | **判据是源文本，与编译无关** | 守卫红并点名 `probe_mutant_bytes` | **成立**（见上方对判色三步的细化） |
| **B** | `ProfileKind::Ps7` 加显式 `serde(rename)` | rc=0 | ✔ grep 到 `Ps7RENAMED` | `tsc` 报 `Record<ProfileKind, string \| null>` 上 `'Ps7' does not exist` ×2 | 成立。映射类型的键真被消费 |
| C | 等号自动各红一次 | — | — | 生成物清单 · 派生源 14→18 · 大整数 14→15 · 文件数 12→10 · 包装层 28→40 | 成立 |

## 3f. 批次 5b 明细：**一处「类型逼测试撒谎」的教科书例子**

13 条包装层条目（40 → **53**）· 1 个派生（生成物 50 → **51**）· 2 个文件迁走（10 → **8**）。
`main.ts` **零新派生**——`list_last_accounts` 返 `HashMap<String,String>`（= `Record<string,string>`）·
`list_active_sessions` 的类型 C04b 已生成 · `load_subagent` 批 2 已进包装层 ·
`open_settings_window`/`replay_session_to_window` 是桶①。

### ★ `McpServerEntry.scope`：手写版比线上**更窄**，而窄不是 fail-safe 方向

Rust 是 `pub scope: String`，手写版窄化成 `McpScope = "user" | "local" | "project"`。

**我先推断「来了第四种 scope 会 `undefined.push` 抛」——那是错的**，我把它写进注释后才去核实现：
`groupByScope` 里有**显式三值判断**，未知 scope 被跳过、**从不抛**；
它的 vitest 注释也明写「测未知 scope 被忽略」。**运行时一直是对的。**（注释已订正。）

**真正的问题**是：手写的窄 union 让那条测试**必须挂一个 `@ts-expect-error`**
才能构造一个**真实会从线上来的** entry ——

> `// @ts-expect-error 测未知 scope 被忽略`
> `ent("weird", "w", {})`

**是类型在逼测试撒谎，而运行时早就正确处理了这个情况。**
换成生成物后那个抑制指令变成**多余**（`tsc` 报 `Unused '@ts-expect-error' directive`），已删。
⇒ **类型说了实话，测试就不必撒谎。** 这是本工作区最直观的一次收益。

`McpScope` **保留**：它是 TS 侧的**域细化**，`groupByScope` 的返回类型用它是对的
（分组结果确实只有三档）。运行时逐字节不变。

### 另一处分叉被包装层结构性消除

`main.ts:294` 原来写 `invoke<{ path: string }>("load_subagent", …)`，而 `cards/subagent.ts`
用完整的 `SubagentLoadResult` —— **同一个命令在全仓有两种 TS 类型**。
包装层收敛成一处后这类分叉**结构性消失**（本处只读 `.path`，用完整类型完全够）。

### 三处我自己造的错（都被断言/tsc 当场拦住）

1. 一个批量插入脚本的锚点没找到 ⇒ **异常在写盘前抛出，文件没被改坏**（`git diff` 确认 0 行）。
   改成逐个核锚点的 `ins()` helper 重做。
2. 忘了给 `list_active_sessions` 加包装层条目（只换了调用点）⇒ `tsc` 报
   `Property 'list_active_sessions' does not exist`。
3. **包装层条目数我口算成 54，实际 53**（新增是 13 条不是 14）⇒ 等号守卫报
   `expected 53 to be 40` 时我按实测值改准，并把头注的「其余 66 个（119 − 53）」也算对。

### 批次 5b 变异

| # | 变异 | 编译 | 判据新鲜 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A** | `McpServerEntry.source_path` 加 `serde(rename)` | rc=0 | ✔ grep 到 `sourcePathRENAMED` | `tsc` 报**生产代码** `mcp-section.ts:559` **与它的 vitest** 各一处 | 成立 |
| **B** | 包装层里 `read_mcp_servers` 的字面量抄成**邻居真命令** `read_remote_mcp_servers` | — | — | `tsc` **rc=0**（管不了）· 守卫红并同时点出两个名字 | 成立。键↔字面量对拍仍是唯一的牙 |
| C | 四条等号自动各红一次 | — | — | 生成物清单 · 派生源 18→19 · 文件数 10→8 · 包装层 40→53 | 成立 |

## 3g. 批次 5c 明细：**两处「我的工具比守卫弱」+ 一条穷尽性兜底被激活**

9 条包装层条目（53 → **62**）· 6 个派生（生成物 51 → **57**）· 1 个文件迁走（8 → **7**）。
**本批零漂移**（6 个手写版与生成物逐字等价）。

### ★ 我用 `grep -P` 逐文件列调用点，漏了一处——因为 grep 是**按行**的

`remote-section.ts` 实际有 **9** 个调用点，不是我一直记的 8：`push_public_key` 写成**跨行**形式

```ts
const r = await invoke<{ outcome: string; pubPath: string }>(
  "push_public_key",
  { cfg, pubKeyPath },
);
```

我的临时 `grep -P '…invoke…["\']NAME'` 是**按行匹配**的 ⇒ 命令名在下一行就看不见。
而守卫里的 JS 正则跨整个文件、**一直数对**（`toBe(112)` 含它）。
**教训：临时 grep 比守卫弱，别拿它当账本。**（发现方式：`tsc` 报 `Cannot find name 'invoke'`
——我把 import 换掉后，那个漏掉的调用点没人给它 `invoke` 了。）

### ★ 包装层签名又抓到一处真的类型松散

`push_public_key` 我先写成 `pubKeyPath: string`，`tsc` 报 `'string | null' is not assignable`。
查下来：`pubKeyPath` 在「已填私钥」那条路上**确实是 `null`**（Rust 据 `keyPath` 推同名 `.pub`），
而 Rust 侧参数就是 `Option<String>`。**是我签名写窄了，不是代码有问题。**
裸 `invoke` 的 args 是宽松的 `InvokeArgs` ⇒ 这处松散一直没人管；包装层一上来就报了。
（再次印证：**实参类型也要从 Rust 签名量。**）

### ★ `ConnectStage` 一并生成，让一条早就写好的穷尽性兜底**第一次真正生效**

`describeStage` 里有：

```ts
default: {
  const _never: never = st;   // 未来新增 ConnectStage 变体时编译期(never)即报错
```

**但手写类型时它守的只是 TS 自己造的联合**——Rust 加一个 variant 并不会让它红。
换成生成物后，**变异 A 给 Rust 的 `ConnectStage` 加一个 variant → `tsc` 报
`Type '{ kind: "probeMutantStage"; }' is not assignable to type 'never'`**。
那条注释里写的意图，从此才是真的。**这是「把已有的好意图接上真实源头」的一例**，
比新增一条断言更有价值。

### 一处我自己造的错，与批 5b 同一个锚点

`ins("  /** 往远端 tmux 会话发按键。")` 又失败——那条注释是**多行**的
（`/**` 单独一行）。批 5b 撞过同一处。**异常在写盘前抛出 ⇒ `commands.ts` 未被改坏**
（`git diff` 确认 0 行），但 `remote-section.ts` 已改 ⇒ **中间态不一致**，
补做时要先确认哪个文件动了。锚点改成 `"  /**\n   * 往远端 tmux 会话发按键。"` 后通过。

### 批次 5c 变异

| # | 变异 | 编译 | 判据新鲜 | 判色 | 结论 |
|---|---|---|---|---|---|
| **A** | 给 Rust `ConnectStage` 加一个 variant | rc=0 | ✔ grep 到 `probeMutantStage` | `tsc` 报 `not assignable to type 'never'` | **成立**，且是本批最重要的一条 |
| B | 四条等号自动各红一次 | — | — | 生成物清单 · 派生源 19→21 · 文件数 8→7 · 包装层 53→62 | 成立 |

## 3h. 批次 6a 明细：**推翻自己的逃生口设计，换来盲区开始归零**

4 条包装层条目（62 → **66**）· 1 个派生（生成物 57 → **58**）· 1 个文件迁走（7 → **6**）·
**TS 字面量命令名 112 → 114**（盲区 7 → 5）。

### 两条 `stream_*` 的签名**刻意不同**，此前被超集 args 掩盖

Rust 侧：`stream_read_remote_session(jsonl_path, origin, on_chunk)` ——`origin` **必填**；
`stream_read_session_jsonl(jsonl_path, on_chunk)` ——**根本没有 origin**。

而 TS 那处三元给**两边传同一个超集** `{ jsonlPath, origin: opts.origin, onChunk }`，
本地那次靠「`origin` 是 `undefined` ⇒ Tauri 序列化时丢掉」才对。
改成两次静态调用后各拿精确签名 ⇒ **给本地命令传 `origin` 变成编译期错误**（变异 A 实证）。

### 批次 6a 变异

| # | 变异 | 判色 | 结论 |
|---|---|---|---|
| **A** | 给 `stream_read_session_jsonl` 传 `origin` | `tsc` 报 `'origin' does not exist in type '{ jsonlPath: string; onChunk: … }'` | **成立**。此前那个超集 args 一直合法 |
| **B** | 往 `DYNAMIC_ONLY` 塞回一个已消掉的名字 | 守卫红（盲区集被钉死，不许静默变大或变小） | 成立 |
| C | 五条等号自动各红一次 | 生成物清单 · 派生源 21→22 · 文件数 7→6 · 包装层 62→66 · **字面量名 112→114** | 成立 |

## 3i. 批次 6b 明细：`doWrite` 从 `(cmd: string, args: Record<string, unknown>)` 改成接 thunk

11 条包装层条目（66 → **77**）· **零新派生**（`SftpEntry`/`TransferProgress` C03 已生成）·
1 个文件迁走（6 → **5**）· **字面量命令名 114 → 117**（盲区 5 → **2**）。

### `doWrite` 的处置：thunk，而不是逃生口

原形态 `doWrite(cmd: string, args: Record<string, unknown>)` 被 C04a 记成盲区之一。
三个调用方传的全是字面量（`sftp_mkdir`/`sftp_rename`/`sftp_delete`）⇒ 改成
`doWrite(run: () => Promise<void>)`，调用方写 `this.doWrite(() => commands.sftp_delete({…}))`。

**两个收益，第二个是意外的**：
1. 命令名回到调用点成为**字面量**（守卫扫得到）⇒ 盲区少三个；
2. **每个命令的实参由包装层各自的精确签名把关**——原来那个 `Record<string, unknown>`
   会照收任何键。变异实证：给 `sftp_mkdir` 多传一个 `isDir` → `tsc` 报
   `'isDir' does not exist in type '{ cfg: unknown; path: string; }'`；
   把 `sftp_rename` 的 `from`/`to` 写成 `path` → 同样精确报错。
   **这一层此前完全没有把关。**

### `sftp_stat` 按 §5 桶② 写 `unknown` 并在那行注明

它是 C03 **刻意跳过**没生成类型的那一个（TS 侧裸 `invoke` 无类型参数、字段没人读）。
包装层给它 `unknown` + 一行说明为什么不是生成物 —— 不留下让人以为「漏了」的空白。

### 一处我自己造的错：`replace` 默认预期 1 处，而有两个命令各出现 2 处

`sftp_stat` 与 `sftp_upload` 各有 **2** 处（单文件路径 + 多文件循环）。
我的 helper 默认 `assert count==1` ⇒ 整个脚本在写盘前中止，
**但 import 替换是另一条独立命令、已经跑了** ⇒ 又一次中间态不一致（同批 5c）。
修法：helper 加一个**显式的预期处数**参数 `rep(a, b, n=1)`，2 处的地方显式写 `n=2`。
**「默认 1 处」这个假设本身就该被声明出来。**

### 批次 6b 变异

| # | 变异 | 判色 | 结论 |
|---|---|---|---|
| **A** | 给 `sftp_mkdir` 多传一个 `isDir` | `tsc` 报 `'isDir' does not exist in type '{ cfg: unknown; path: string; }'` | 成立。thunk 化前 `Record<string, unknown>` 会照收 |
| **B** | 把 `sftp_rename` 的 `from`/`to` 写成 `path` | `tsc` 报 `'path' does not exist in type '{ cfg: unknown; from: string; to: string; }'` | 成立 |
| C | 三条等号自动各红一次 | 文件数 6→5 · 包装层 66→77 · **字面量名 114→117** | 成立 |

## 3j. 批次 6c：★★ **里程碑——119 个命令全部静态可见，盲区归零**

12 条包装层条目（77 → **88**）· 8 个派生（生成物 58 → **66**）· 1 个文件迁走（5 → **4**）·
**TS 字面量命令名 117 → 119**、**`DYNAMIC_ONLY` → `toEqual([])`**。

### C04a 立的那个「已知盲区」被完全消除

C04a 写下：「7 个命令 TS 静态看不见 ⇒ 只做单向断言」。批 6a/6b/6c 逐个查实后，
**那 7 个从来不是任意字符串**：

| 位置 | 实际形状 | 批次 |
|---|---|---|
| `views/session-viewer.ts:211` | `origin ? "stream_read_remote_session" : "stream_read_session_jsonl"` | 6a |
| `sftp/panel.ts:485` | `doWrite(cmd: string, args)` 转发 helper，**调用方传的全是字面量** | 6b |
| `views/history.ts:489` | `origin ? "stream_remote_history_sessions" : "stream_history_sessions_in_project"` | 6c |

「动态」只在于名字从一个**封闭、静态可知的集合**里选 ⇒ 三处改成静态调用 / thunk，
**原计划的 `invokeDynamic(name, args)` 逃生口没有做**。
`DYNAMIC_ONLY` **保留为空数组而不是删掉断言**——它现在钉的性质变成
「**不许再出现新的动态命名调用**」：哪天有人写 `invoke(someVar, …)`，`rustOnly` 会非空、这条会红。

### 两处**更窄**的内联字面量被换宽

1. `list_remote_history_projects` 原来写 `invoke<{ projects: HistoryProject[]; failedHosts: string[] }>`
   —— 那正是 Rust 的 `RemoteProjectsResult`。
2. `get_search_index_status` 原来写 `invoke<{ ready; indexedSessions; indexedMessages }>`
   —— **少了 Rust 侧的 `builtAtMs`**。换成生成物后那个字段也进类型了（宽于原来、与线上一致）。

### 让守卫指出该配什么属性，而不是我猜

派生完先跑守卫，它逐个报出缺的属性：`ts(optional)` ×5（两个 `origin`、两个 `forked_from_*`、
`SessionHits.origin`）· `ts(type)` ×7 **全是毫秒时间戳量纲**。
其中 **`Hit.ts_ms` 是守卫指出来的**——`Hit` 是 `SessionHits` 的传递依赖，我派生它时没逐字段读。
计数：`skip_serializing_if` 5 → **10** · 大整数 15 → **22** · 派生源 22 → **24**。

### 一处刻意不生成：`MetadataPatch`

它的字段是 `Option<Option<String>>`，而 `#[serde(default)]`（非 double_option）下
**JSON `null` 到不了 `Some(None)`** ⇒ **`null` 的语义是「不改」，不是「清空」**。
生成 `customTitle?: string | null` 会让人以为 `null` 是清空 —— **那是说谎的类型**。
⇒ 包装层手写这个入参形状，并把陷阱写在签名旁边。
**顺带发现一个真 bug（已登记 E35，本轮刻意不修）**：`views/history.ts` 的
「留空恢复默认」传的正是 `null` ⇒ 后端什么都不做、标题清不掉。

### 批次 6c 变异

| # | 变异 | 判色 | 结论 |
|---|---|---|---|
| **A** | 给 `stream_history_sessions_in_project` 传 `origin` | `tsc` 报 `'origin' does not exist in type '{ projectDir: string; onEntry: Channel<HistorySessionEntry>; }'` | 成立 |
| **B** | 往 `DYNAMIC_ONLY` 塞一个名字 | 守卫红 | 成立（空集被钉死，不许静默变大） |
| C | 三条等号自动各红一次 | 文件数 5→4 · 包装层 77→88 · **字面量名 117→119** | 成立 |

## 3k. 批次 7：★★ **成功标准 4 达成** + 一处被文档强制的设计决定

21 条包装层条目（88 → **109**）· **1 个**派生（生成物 66 → **67**）· 1 个文件迁走（4 → **3**）。
**`import { invoke }` 的生产文件从 29 降到 3，即主计划 §0.1 成功标准 4 改写后的最终形态**：
`ipc/commands.ts`（包装层自己）· `tabs.ts`（等红线授权）· `accounts.ts`（等 Z02）。

### ★ 10 个返回类型住在 vendored 里 —— 决定被 `VENDOR.md` 强制

`Overview`/`NodeView`/`SubGraph`/`Edge`/`ImpactSet`/`Symbol`/`DocLink`/`Annotation`/
`DriftItem`/`IndexStats` 都在 `src-tauri/vendor/code-picture-core/src/model.rs`。
`VENDOR.md` 有一条明写的铁律：

> **副本是上游的镜子，不是分身**（SS-10）：**只照上游改，绝不在副本里改出自己的版本**。

⇒ 加派生就是违反它；「先改上游」要动 `code-picture` 仓（**在册红线**）。
**按 §5「名字钉死是普遍的、类型生成是按需的」**，本批只做名字钉死 + 实参把关，
类型生成**如实登记为结构性阻塞（BACKLOG E38，含三条备选路径与我的倾向）**。
`PanoramaStatus` 例外（在 `panorama.rs`、本仓自己的）⇒ 已生成。
**门禁复核 vendored 零改动**：`cargo test -p code-picture-core` 仍 **25**、
`git status -- src-tauri/vendor/` **0 行**。

### 守卫指出 `indexed_at`，规则按批 4 那条走

`PanoramaStatus.indexed_at: Option<u64>` 生成出 `bigint | null` ⇒ 守卫红。
它是 `Option` 且**无** `skip_serializing_if` ⇒ 按批 4 立的规则写
`ts(type = "number | null")`（不是 `"number"`——那会丢掉 `null`）。
`Option<大整数>` 计数 2 → **3**。

### ★ 一处我自己量错的 Rust 参数，被包装层当场揪出

`panorama_touching` 的 Rust 签名是 `(repo, files, ranges: Vec<(usize, usize)>)`，
**我提取参数的正则用了 `[^,]+?` ⇒ 被元组里的逗号截断，漏了 `ranges`**。
`tsc` 报「'ranges' does not exist in type '{ repo; files }'」才发现。
⇒ **量 Rust 签名时，参数类型里可能含逗号（元组/泛型），别用 `[^,]` 切。**
另两处 `budget`/`limit` 我写成 `number | null`，而调用方是 TS 可选参数（`number | undefined`）
——Rust 是 `Option<usize>`，**缺席/null/数字三种都合法** ⇒ 改成 `?: number | null` 才忠实。

### 批次 7 变异

| # | 变异 | 判色 | 结论 |
|---|---|---|---|
| **A** | 给 `panorama_touching` 少传 `ranges` | `tsc` 报 `Argument of type '{ repo; files }' is not assignable to … { repo; files; ranges }` | 成立。**那正是我漏量的参数**，此前裸 `invoke` 的 `InvokeArgs` 完全不管 |
| **B** | 删 `indexed_at` 的 `ts(type)` | 生成物变 `bigint \| null`、守卫红并点名 | 成立 |
| C | 五条等号自动各红一次 | 生成物清单 · 派生源 24→25 · 大整数 22→23 · `Option<大整数>` 2→3 · 文件数 4→**3** · 包装层 88→109 | 成立 |

## 4. 代码审计结果（Phase D）

**强度：低风险**（无逻辑改动，纯调用形式替换；三条等号守卫全程盯着）⇒
主线程变异验收 + 全门禁 + 真机套件，不开多 agent。

批次 1 的三条变异结果见 §3；每批完成后在此追加。

## 5. 工程审计结果（Phase E）

**账本第 7 行（包装层）**：批次 1 是它第一次容纳**带参数**的条目。
形状约束全部守住：扁平映射 · 命令名每条目只出现一次（机检）· 返回类型按三桶 ·
每加一条就换掉对应模块的裸 `invoke`（不许两条路并存）。

**账本第 3 行（`accounts.ts` 6 个 type）的冲突协议**：批次 4 会碰它，
而 `account-zero` 工作区的 Z02 也要改那些类型。**批次 4 开工前必须先查该行的冲突协议**，
不许先到者被后到者重排。

## 6. 签收

- [ ] 全部批次完成（1/7 完成 · `tabs.ts` 批等授权）
- [ ] 通过代码审计
- [ ] 通过工程审计
- [ ] 主计划已据此更新
