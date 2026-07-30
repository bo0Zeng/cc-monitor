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
| 3 | 2-3 调用点的一组（`config` · `views/usage-view` · `views/port-forward` · `settings/accounts-section` · `settings/cc-bus-hooks-section`） | 是 | 14 | 待做 |
| 4 | `account-restart` · `accounts.ts` · `settings/diagnostics-section` | 是（`accounts.rs` 6 个，**踩账本第 3 行的冲突协议**） | 11 | 待做 |
| 5 | `settings/cc-bus-section` · `cc_integration` · `main.ts` · `remote-section` · `mcp-section` | 是 | 6 | 待做 |
| 6 | `sftp/panel` · `views/history` · `views/session-viewer`（**含 3 个动态派发口**） | 是 | 3 | 待做 |
| 7 | `panorama/api.ts`（21 处，最大） | 是 | **1**（包装层自己） | 待做 |
| — | **`tabs.ts`（15 处）** | — | — | **已跳过，等授权** |

**最终形态**：`import { invoke }` 只剩 `src/ipc/commands.ts`（那 1 个）
+ **`tabs.ts`（等授权）** + **一个动态派发逃生口**。这正是主计划 §0.1 成功标准 4 改写后的形态。

### 3 个动态派发口怎么处置（批 6 的核心判断）

`sftp/panel.ts:483-485` 的 `doWrite(cmd, args)` 转发 3 个 sftp 写命令 ·
`views/session-viewer.ts:211` `invoke<number>(ipc, …)` · `views/history.ts:489` `invoke(ipc, …)`
——命令名运行时才定，**结构性摘不掉裸 `invoke`**。

按账本第 7 行约束 ①：逃生口**必须是另一个导出**，不许塞进 `commands` 扁平表
（塞了会被守卫第 2 条抓红，那个 fail-safe 是刻意的）。计划形态：
`src/ipc/commands.ts` 里另导出一个 `invokeDynamic(name, args)`，
它内部断言 `name` 属于 `DYNAMIC_ONLY` 白名单——**把「动态」这件事本身也钉死**，
而不是留一个任意 `string` 的后门。这条留到批 6 时再定稿。

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
