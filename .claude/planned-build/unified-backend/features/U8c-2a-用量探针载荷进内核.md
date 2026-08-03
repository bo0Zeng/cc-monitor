# U8c-2a — 用量探针的载荷改由 Rust 内核产出（`render_payload` 的第一个生产调用方）

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 前置：U8c-1（`launch-core` 已建、跨语言夹具对拍已立）
- 本件性质：**退役一个 TS 载荷产出点**，并让内核第一次进生产。

## 摸底：原定的「本机那条先切」**是空转，而且 R07 明确否决过**（铁律 3 + 4）

本轮原定「`resume_history_session` 收结构化 plan，本机那条先切」。实测**它已经是结构化请求了**：

```rust
pub fn resume_history_session(session_id: String, cwd: String,
                              launcher: Option<String>, account: Option<LaunchAccount>)
```

四个参数全是结构化的，命令**整条在 Rust 里构造**（`build_local_posix_command` /
`build_local_ps_command`），TS 侧**一个渲染器都不经过**。这不是巧合 ——
`doc/INVARIANTS.md` **§36** 就叫「本地（Windows）路径**不经 IR** 产出命令」。

而且 `src/launch-requests.ts:143-165` 用一整段头注记着 **R07 为什么否决「真接上 IR」**：

> **`plan.action`/`plan.cwd` 在当前维度注册表下恒等于输入，取回来没有信息增量**
> …（原 `planLocal` 的返回值被 4 个生产调用点全部当语句丢弃，真命令由 Rust 独立构造）

⇒ **原定范围要么是空转、要么是在推翻一条已裁决的事**。改计划。

### 那第一刀该切哪

工程审计上一轮点名了一个「**今天在任何计划里都没有落点**」的缺口，它恰好是最合适的第一刀：

| | 今天 | 本件之后 |
|---|---|---|
| `account_usage` IPC | `(origin, account_name, **launch_payload: String**)` —— 前端把**渲染好的串**递进来 | `(origin, account_name, **config_dir: Option<String>**) —— 前端只报「哪个账号」，Rust 自己编译载荷 |
| 载荷产出方 | TS `remote-launch.ts::buildUsageProbePayload`（S28 的第 ② 份） | `launch_core::render_payload`（**它的第一个生产调用方**） |

为什么是它：

- **形状正好**：探针载荷是 `<账号前缀>unset <嵌套env>; claude` —— 就是
  `PayloadSpec { env, cwd: None, launcher, args: [], wrap: [] }`，**连 `cd` 都没有**，
  顺带把「`render_payload` 只覆盖 `container:"none"`」那个盲区的另一半（`cwd: None`）走通。
- **六份副本真的少一份**（S28 的 ②），不是搬来搬去。
- **有既有判据兜底**：`usage-probe` e2e 9 条 + `account-usage.vitest.ts`。
- **不碰远端起会话主路**（那条要 U8a-2c，是另一件）。

## DoD

1. **`account_usage` 改收结构化账号表态**：`config_dir: Option<String>`
   （`None` = **账号 0**，不是「不表态」—— 探针恒是 per-account，没有第三态）。
2. **载荷由 `launch_core::render_payload` 产出**，键表取 `adapter::active().nested_env_to_scrub()`、
   启动器取 `agent.default_launcher()`（两者都已有 TS↔Rust 对拍守卫）。
3. **TS 侧 `buildUsageProbePayload` 退役**（连同它的导出与测试一起处置，不留死代码）。
4. **先解决两个登记在案的盲区**（动 Rust 生产者之前，用户点名）：
   - **`args` 不 quote**：`render_payload` 给每个 arg 加白名单，含空白/元字符 ⇒ `Err`。
     **保持逐字节兼容**（合法输入仍是 `join(" ")`），只是把会裂/会注入的那类变成不可表示。
   - **`cwd: None` 的形态**：本件就是它的第一个生产用例，头注那句约束落到真调用上。
5. **字节零变化**（⚠ 初稿这里写的是「`unset` 顺序会变、如实登记」—— 代码审计建议了一个零成本的
   更好处置：**把 Rust 的 `CLAUDE_NESTED_ENV` 改成与 TS 同序**。两侧守卫都按集合比、改序零成本，
   于是搬家前后送到远端的字节**完全相同**，黄金串夹具也重新代表生产字节）。
6. **校验变严**：探针路径的 configDir 从 TS 旧表换成 `launch_core` 的并集 —— 与 U8c-1 同一条，
   S18 的缺口再收一格。

### 不做什么

- **不碰远端起会话主路**（`runRemoteResume*`）—— 那是 U8c-2b，且牵 U8a-2c。
- **不动本机那条**（§36 + R07 已裁决，见摸底）。
- **不搬维度注册表**（U8c-2c）。
- **不动 `session-backend.ts` / 外层容器**（U8c-3，且有三个必答问题未回答）。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | `render_payload` 给 `args` 加白名单（fail-closed） | 单测：含空格/分号/换行的 arg ⇒ `Err`；合法 arg 仍逐字节 `join(" ")`；黄金串夹具不变 |
| 2 | `launch-core` 加 `usage_probe_payload()`（或直接用 `render_payload` + 组装） | 单测覆盖具名/账号 0 两态 + 空串 ⇒ `Err` |
| 3 | `account_usage` 换参数、内部调内核 | 既有 Rust 测试全绿；新增「两态各自的载荷」断言 |
| 4 | TS：`account-usage.ts` 改传 `configDir`；`buildUsageProbePayload` 删除 | `tsc` 0；`account-usage.vitest.ts` 改到新形态 |
| 5 | 平价对账表 / 生成类型 / 命令数三处跟着改 | 既有守卫会咬人，逐条处置 |
| 6 | 全量门禁 + 17 套 e2e（重点 `usage-probe 9`） | 逐套对数 |

## 代码审计结果（D）

一个 agent，**零阻塞**，但它抓到一条**相对 HEAD 的真实覆盖回归**（我造的）。

### ★ 重要 1：接线无判据 —— DoD 步骤 3 那句「新增两态载荷断言」我没写，而那正是唯一能杀它的判据

审计做了 4 个变异，在 **729 条 Rust + 1168 条 TS 全绿**下**全部存活**：

| 变异 | 后果 | 修复前 | 修复后 |
|---|---|---|---|
| R1 `account_usage` 恒当账号 0 | **静默串号** | 存活 | **仍存活**（见下） |
| R2 探写死的别的号 | **静默串号** | 存活 | **仍存活**（见下） |
| R3 只清一个嵌套 env 键 | 远端 claude 自认嵌套子会话 | 存活 | **红** |
| R4 换掉启动器 | 探针跑错程序 | 存活 | **红** |

审计还两向实证了这是**回归**：在 HEAD 副本上把 `buildUsageProbePayload(configDir)` 改成
写死别的号 ⇒ **红 9 条**，其中就有那条「`launchPayload` 是逐字节确定的载荷」。
也就是说我把一条真判据换成了**更弱**的东西，而功能件里我写的「那三件事一件都没丢」——
审计逐条核实 **①② 不成立、只有 ③ 成立**。

**处置**：把构造那一段整体抽成纯函数（`probe_payload_for` / `probe_command_for`），
两态的载荷**逐字节断言**（R3/R4 当场红），并补一条接缝判据（载荷真的被塞进整条命令）。

**R1/R2 的收口（U8c-2a-fix，同一 commit 内补上）**：

审计建议的「让 e2e 由真接线驱动」**走不通** —— e2e 用的是 `FAKECLAUDE` stand-in，
而 `probe_payload_for` 的启动器来自 `agent.default_launcher()`（恒 `claude`），沙箱里没有真 claude。
而「这个 async tauri 命令有没有把收到的参数转发下去」**在 Rust 类型里表达不了**：
换任何一层包装，变异只会跟着下移一层（审计实测两种写法都杀不掉）。

⇒ 退而求其次，**钉住那一行的源码形态**（`guard_core::production_code` 剥测试段后，
断言 `probe_command_for` 的调用点**恰好一个**、且实参里含 `config_dir`）。
三个变异实测：

| 变异 | 结果 |
|---|---|
| R1 恒传 `None`（恒当账号 0） | **红**（逐字报出实参 `&slug, None, …`） |
| R2 写死别的号 | **红**（报出 `Some("/h/.claude-accts/OTHER"`） |
| 自检：另加一条绕过调用 | **红**（调用点 2 ≠ 1） |

⚠ **它是约定不是事实**（同 `protocol_doc_guard` 的 `doc_anchor`、`TS_HALF` 那一族）：
挡得住「顺手把参数换成常量」，挡不住「换个名字继续错」。**比没有强，但别读成证明。**
这段话逐字写在测试的头注里。

### ★ 建议 1 采纳（零成本，把一条「行为变化」直接消掉）

我原本要如实登记「`unset` 顺序从 TS 序变成 Rust 序」。审计给了更好的处置：
**把 `adapter/claude_code.rs::CLAUDE_NESTED_ENV` 改成与 TS 同序**。
两侧守卫都按**集合**比（`agent-profile-parity` 与 `fixture_nested_env_keys_match_…`），
改序零成本 ⇒ 搬家前后送到远端的字节**完全相同**，黄金串夹具也重新代表生产字节。
DoD #5 从「行为变化」改成「**字节零变化**」。

### 审计实跑的两项穷举（比我自己做的强，逐条采信）

| 项 | 方法 | 结果 |
|---|---|---|
| **行为等价性** | 旧 TS（从 HEAD 取回真函数）vs 新 Rust（path-dep 到仓里真 crate），**86 个输入**，hex 逐字节比 | ORDER-ONLY 27 · BOTH-ERR 50 · **变严 9** · **真差异 0** · **变松 0**。首个差异字节位都落在 `unset` 段内部；`env -i` 在 sh/bash/dash 下实测两种顺序等价 |
| **fail-closed 有没有变松** | `isValidConfigDir` vs `config_dir_command_safe` 穷举 U+0000–U+FFFF + 星域 + 29 结构性用例，**63,520 条** | `same=63500 · tightened=20 · **LOOSENED=0**` |

### 其余处置

- **重要 2 · 四处注释已经在说假话**（这个仓两轮前刚因为同类问题被咬过）：`launch-core` 头注三句
  （「六份」「只有一个生产消费方」「TS 那两个产出点仍然活着」）+ `account_usage.rs` 模块头
  （「`launch_payload` 由 TS 侧 `buildUsageProbePayload` 构造好传入」）+ `account-usage.ts` 的
  JSDoc + `account-chip.ts` 的悬空引用。**全部订正。**
- **重要 3 · 账本 5 处残留**：S28 抬头「六份」与格内「六份变五份」自相矛盾 · U8c-2b 行里还挂着
  本轮做掉的项 · U8c-2b 行还在推荐已被否掉的「本机那条先切」· `BACKLOG.md` **E23 结案**
  （它直指的就是 `buildUsageProbePayload`）· 摸底 A 表第 14 行「载荷 TS + 编排 Rust」。**全部改。**
- **建议 2 采纳**：`arg_is_join_safe` 用 `is_ascii_alphanumeric` ⇒ **非 ASCII 一律拒**，
  而同 crate 的 `config_dir_command_safe` 是**放行中文的** —— 不对称且过严。今天零生产流量，
  但 U8c-2b 往 args 塞 `--add-dir <中文路径>` 时会撞上，且错误文案会误导。**写进头注。**
- **建议 3 登记**：`configDir === ""` 那道 TS 闸今天**没有独立可观测性**（审计变异 T7 存活）——
  `isValidConfigDir("")` 本就为假，它只改错误文案而文案没被断言。属**等价变异不是伪测试**，
  留着当纵深防御，但知道它今天不独立。
- **建议 4**：DoD 编号乱了（有个孤立的 `3.`），已修。

### 变异复验总表（本轮 8 次）

| 变异 | 结果 |
|---|---|
| R5 `usage_probe_payload` 账号 0 退化成裸载荷 | **红** |
| R6 `arg_is_join_safe` 恒真 | **红** |
| R7 `config_dir_command_safe` 换回旧表 | **红 ×3** |
| **R3 只清一个嵌套 env 键**（修复前存活） | **红**（逐字节报出两串） |
| **R4 换掉启动器**（修复前存活） | **红** |
| T1/T2 TS 送 `undefined` / 省掉 `configDir` 键 | **红 ×4**（正是「静默变成账号 0」那一类） |
| T4/T6 恒送 null / 把渲染好的串塞回 IPC | **红** |
| R1/R2 接线那一行 | **仍存活 —— 已登记，需 e2e 真接线驱动** |

## 工程审计结果（E）

- **原定范围是空转，摸底当场改计划**（本轮第一件事）：`resume_history_session` 早就收结构化参数、
  本机路径不经 TS 渲染器（§36），且 `launch-requests.ts` 里 **R07 明确否决过**「真接上 IR」。
  审计独立核实三条断言**全部属实**，只订正一处措辞：R07 的原话是「接了也拿不到新东西，
  **不是技术上不可能**」，我写成「推翻一条已裁决的事」略重 —— 但**操作结论（空转）是对的**。
  审计也确认「本机那条**没有**真活可做」。
- **账本**：**S28 六份 → 五份**（② 退役，消费方那句改写）；**S18** 补「探针路径也接上并集、
  两道闸外松内严」；`BACKLOG.md` **E23 结案**；U8c-2 拆成 **U8c-2a ✅ / U8c-2b**。
- **`render_payload` 有了第一个生产调用方** —— U8c-1 交付时它是「加了个 crate 没人用」的一半，
  本轮补上了。
- **U8c-2b 仍然要再拆**，且本轮否掉了原来给它写的那条建议。新建议：**按 plan 形态切**，
  先 `container:"none"`（`render_payload` 已覆盖），tmux 那两种要连 `session-backend.ts` 一起想。
- **`src-tauri/Cargo.toml` 的 `[profile.dev]` 是用户自己的改动**，本轮与前六轮一样刻意不提交
  （审计确认本轮没碰它）。

## 签收

- [x] 过代码审计（D）—— **零阻塞**；抓到一条**我造的覆盖回归**（4 个变异在全绿门禁下存活），
      **四个全部收口**（R3/R4 逐字节断言；R1/R2 源码接线守卫，诚实边界写在头注里）；
      四处说假话的注释全部订正
- [x] 过工程审计（E）—— S28 六份→五份 · S18 再收一格 · BACKLOG E23 结案 · U8c-2 拆成 2a/2b
- [x] 主计划已更新（F）
