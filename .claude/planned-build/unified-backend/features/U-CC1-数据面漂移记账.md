# U-CC1 — 数据面漂移记账（把「CC 变了」从不可观测变成看一眼就知道）

- 工作区：unified-backend · 新开件（与 U 序列**正交**：不动控制面、不动 daemon、零行为变化）
- 来源：`DESIGN-CC新功能的扩展缝.md` 的 R1（用户中途点名「后面 CC 加的功能怎么加进去」）
- 前置：无。**越早装上越好** —— 装上之后才开始有数据

## 摸底：先自己把 agent 的三条断言验一遍（血泪 7）

设计稿 B 由专职 agent 产出，三条关键断言我**逐条独立实测**，结果**两条属实、一条不成立**：

| # | agent 的断言 | 我的实测 | 判定 |
|---|---|---|---|
| 1 | CC 在 17 天里加了 3 个记录类型；`INVARIANTS §18.1` 的数字过期 | 本机 1,904 个 jsonl / **472,115** 条 / 非法 JSON **1** 条；**20 种 type**，monitor 认识 11 种（含自造的 `cc-monitor-unrecognized`）⇒ **未知 10 种 / 27,747 条 / 5.88%**。§18.1 记的是 7 种 / 8,774 条 / 157,385 行 | ✅ **属实**（数字与 agent 差 51 条，语料是活的） |
| 2 | `subagent.rs` 对 CC 新的 `subagents/workflows/wf_*/` **今天就加载失败**，用户会看到「加载失败」 | ❌ **不成立** —— 详见下节 | ❌ **订正** |
| 3 | `cards/slash.ts` 那条注释被语料证伪 | `<command-name>` 共 **56 种**；`/model` **74 条**、`/context` 11、`/login` 4、`/doctor` 3、`/ide` 3、`/exit` 3 都在 jsonl 里；只有 `/clear`(0) 与 `/help`(0) 是对的 | ✅ **属实**（agent 说 70，实测 74） |

### ★ 断言 2 为什么不成立（差点照着它「修」一个不存在的缺陷）

agent 说的两个成因都**属实**：
- `subagent.rs::list_meta_matches` 确实是**非递归** `read_dir`；
- `workflows/wf_*/` 下的 meta 确实**没有 `description`**（实测 6/6，keys 恒为 `{agentType, spawnDepth}`）。

**但那条路径今天根本不会被走到**：

- 展开子 agent 的入口是 `AGENT_PROFILE.agentTools`，实测**只有 `{Agent, Task}`**；
- 产出 workflow 目录的那个会话里，`tool_use` 名称是 `Agent`×135 + **`Workflow`×1** ——
  **`Task` 0 个**，而 `Workflow` **不在** `agentTools` 里 ⇒ 它压根不生成可展开的卡；
- 那 135 个 `Agent` 卡按 description 去顶层 `subagents/*.meta.json` 查，**135/135 全部命中**。

⇒ 正确的说法是：**`Workflow` 是 CC 的一个新工具面，cc-monitor 还不支持它**（缺功能），
**不是**「既有路径坏了」（缺陷）。而全机器只有 **1 个** `wf_*` 目录（agent 说 7 个，也不对）。

**处置：登记，不做。** 理由是本仓明写的原则「拆由具体架构病证成」——
一个实例、无用户可见故障、且 CC 侧信息缺失（meta 无 `description`，**自动定位在原理上做不到**，
上限只是「列出让用户选」）。等它变成常态、或有人真的按到，再做。
**而 U-CC1 装上之后，这类新面会自己出现在诊断表里** —— 这正是本件的价值。

## DoD

1. 四个**真有落点**的降级点各记一笔：未知记录 `type` · 已知类型解析失败 · 未登记的会话 `kind` ·
   daemon 声明的未知能力 token。
2. **零行为变化**：`parse_line` 的输出逐字节不变；不新增任何 warn；不新增任何轮询。
3. **有界**：键数上限 64（溢出并进 `<overflow>`），样例只留首见一条、按**字符边界**截到 400 字节。
4. 诊断面一节（只读、按需一次），**每一行都说清楚「这么降级之后会怎样」**。
5. **接缝有判据**：`parse_line` 真的把记账接上了（血泪 10）。

### 不做什么

- **不改成 warn**（实测 20,526 条 `mode` 会刷屏 —— 那个「刻意不 warn」的决定是对的）。
- **不改任何白名单/排他行为**（`kind` 是授权型判据，有事故背书）。
- **不加第五个面「未登记的 status」** —— 见下。
- **不做 subagent 递归**（断言 2 已证伪，登记）。

## 设计要点

### 只有四个面，没有「未登记的 `status`」

设计稿列了四个降级点，其中「未登记的 `status`」在**前端**（`session-status.ts::activityLightClass`）；
Rust 侧对 `status` **没有任何白名单分支**（逐行核过，只是原样透传）。
**在 Rust 账本里加一个没有落点的面 = 「登记了但不产生信号」，比不加更糟。** TS 侧那个面另记。

### 计数的量纲逐面不同，不许横向比

- 记录类两面：**每条记录一次**（解析热路径）；
- 会话 `kind`：**每次扫描一次**（`scan_dir` 随文件事件重扫）⇒ 数字是「观测了多少次」，**不是会话数**；
- daemon token：每次握手一次。

⇒ 后端逐面写清楚，前端 `countUnit()` 逐面给不同单位，并**有测试钉住**
（防「用户把 4000 看成有 4000 个这样的会话」）。

### 热路径代价

只在**降级分支**里多一次 map 查找：未知 type 那条占全部行的 5.88%，且那个分支本来就已经在做
**第二遍** `serde_json::from_str`（先 `JsonlRecord` 再 `Value`）。可忽略。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | `drift_ledger.rs`：有界账本 + 四个面 + `consequence()` + `snapshot()` | 模块单测 5 条（含**有界**与**字符边界截断**） |
| 2 | 接进 `parser.rs`（两处）· `session_map.rs`（一处）· `ssh_source.rs` hello（一处） | **接缝测试** `parse_line_feeds_the_drift_ledger`；变异复验 |
| 3 | `KNOWN_CAPABILITY_TOKENS` 与 `decide_stream_flags` 同源 | `known_capability_tokens_match_decide_stream_flags` |
| 4 | tauri 命令 + parity ledger 登记（`Both`） | `every_tauri_command_is_declared_in_the_ledger` |
| 5 | 前端 `drift-ledger-section.ts` 挂到「改动足迹」页 | 10 条 vitest（含「读不到 ≠ 没有漂移」） |
| 6 | 订正 `cards/slash.ts` 那条被证伪的注释 | 语料实测数字写进注释 |

## 代码审计结果（D）

本件的 D 由**仓里既有的护栏**代劳，而且它们全部按设计咬了人 —— 逐条列出来，因为
「新增一个 tauri 命令要同步改哪几处」这件事本身就是本仓最值钱的资产之一：

| 护栏 | 咬到什么 | 处置 |
|---|---|---|
| `parity_ledger::every_tauri_command_is_declared_in_the_ledger` | 新命令没进平价对账表 | 登记为 `Both`（本地行与远端行都经同一个 `parse_line`，一个读口覆盖两侧） |
| `parity_ledger::ledger_shape_is_pinned` | 命令总数 123→124、能力总数 50→51 | 逐个确认后棘 |
| `parity_ledger::local_or_both_commands_take_no_remote_only_parameter` | Local/Both 计数 68→69 | 同上 |
| `generated-boundary-guard`（三条） | 派生 `ts_rs::TS` 的文件数 27→28 · 生成目录清单 +3 · **`u64` 没配 `ts(type)`** | 第三条是真的：不配会回落成 `bigint`，与 JSON IPC 运行时不一致 ⇒ 补 `#[cfg_attr(test, ts(type = "number"))]` |
| `paste-block-guard` | 新的 `clipboard.writeText` 不在两张名单里 | 登记进族 B（复制完就完事，不贴进任何配置） |
| `commands.vitest`（三条） | Rust 命令数 / 包装层覆盖数 / TS 字面量数 | 逐个棘 |
| `panel-groups.vitest` | 「改动足迹」页的叶子块清单变了 | 更新清单 + 叶子块总数 14→15 |

**变异复验**：删掉 `parse_line` 里那处 `drift_ledger::record` ⇒ `parse_line_feeds_the_drift_ledger`
**红**（报「未知 type 没有被记账」）。这条就是血泪 10 要的接缝判据 ——
`drift_ledger` 有自己的 5 条测试、`parse_line` 有自己的 12 条，**中间那一行删掉本来是全绿的**。

## 工程审计结果（E）

- **与 U 序列正交**：不动 daemon、不动控制面、不动协议。可以在任何时候单独回滚。
- **诊断面的纪律照搬 T02**：读不到就说读不到，**绝不显示成「没有漂移」**（有测试）；
  「本次运行期间没有」而不是「没有」（计数重启归零，措辞不许暗示这是历史结论）。
- **后端加第五个面而前端没跟** ⇒ 显示原始枚举名，不崩也不静默吞（有测试）。
- 账本新增 **S24**（数据面漂移记账的四个落点）。
- **`INVARIANTS §18.1` 的数字已过期**，本轮更新并注明「以后靠这一页看，不靠人扫语料」。

## 签收

- [x] 过代码审计（D）—— 七族既有护栏全部咬人并逐个处置；接缝判据变异复验
- [x] 过工程审计（E）—— 账本 S24；§18.1 更新
- [x] 主计划已更新（F）
