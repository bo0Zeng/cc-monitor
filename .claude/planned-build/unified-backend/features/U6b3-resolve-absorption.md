# U6b-3 · `--resolve` 吸收 + 护栏结构性收口

- 工作区：unified-backend · 任务 #105（U6b 第三件，收尾）
- 风险档：**高**（动一条**与仓外 aterm 冻结过的契约**；并把两条护栏从"字符串判据"改成"类型不可表示"）
- 前置：U6b-1 骨架 + U6b-2 argv 三分已闭环（`217835c` / `82c086c`）

## Phase B 摸底

### ① 「吸收」不能是搬走 —— 契约冻结着

`control/resolve_query.rs` 头注写死：

> 契约定死（cc-bus 与 `android-terminal_cc` 对齐 **2026-07-18**，见 `daemon-协议-v1 §3`）
> **exec 模型**：1 exec = 1 请求 1 响应 1 退出、天然 1:1，**无 request-id**；超时 = 客户端杀 exec

且 MVP 注写着 aterm **现走 β TailTransport、DaemonTransport 未建、暂不消费 resolve** ——
也就是说这条契约**随时可能开始被消费**，现在拆掉它是拿别人的集成期赌。

⇒ **一次性 exec 路径逐字不动**；流通道上**新增**一条 `resolve` 命令，两者**复用同一个纯函数**。

### ② 好消息：核心早就是纯的

`resolve_from_json(&str) -> Result<String, (code, message)>` 已经与 stdin/stdout 分离
（当初审计 quality-阻塞 逼出来的）。⇒ 吸收的代价只有「接一条命令臂」。

### ③ D 审计明确登记归本轮的三条

| # | 问题 | 审计给的方向 |
|---|---|---|
| G1 | 两条结构机检**被普通重构击穿**：把调用点抽成函数 ⇒ 位置比较失效；尾随注释 / 同臂混用 / 或模式 ⇒ 分派臂扫描失效 | **让违规不可表示**：`HelloFlushed` 类型见证 + 非 async 的 `dispatch` 返回 `Disposition::Spawn|Inline` |
| G2 | 字段判据仍太宽：把 §10 帧字段表整段挖掉，**31 个里 16 个照样通过**（`sid` 命中文件名 `sid-hwnd-cache.json`、`attachable` 命中 `p1v-attachable`） | 收进 §10 表区间（子命令那条已经是这个思路） |
| G3 | `inbound.rs` 头注宣称「不许出现任何 `observe::`」**没有机检**（`layering_guard` 只遍历 `observe/` 与 `control/`，顶层文件不在采集面） | 加进采集面 |

### ④ F90（不透明稳定键）：**今天已经满足，要做的是把它钉住**

F90 说「会话/后端登记表主键必须 opaque + 稳定，不许拿 tmux 会话名 / 主机名 / 路径当持久主键」。
U6b-1 的 `running` 表主键是**客户端给的不透明 `id`**，daemon 不解析、只回显 —— 天然合规。
⇒ 本轮不建新登记表，而是**把「不许解析 id」钉成机检**（防后人「顺手」从 id 里抠信息）。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | 流通道上有 `resolve` 命令 | 发 `{"id":"x","cmd":"resolve","args":{ResumeSpec}}` ⇒ 回 `reply` 带 `data`（CommandPlan） |
| ② | **一次性 exec 路径逐字不变** | `--resolve` 的 stdout/stderr/退出码与 HEAD 逐字相同（含错误路径） |
| ③ | 两条路复用**同一个**纯函数 | 机检：`resolve_from_json` 恰好有 2 个生产调用点 |
| ④ | **G1**：违规不可表示 | `dispatch` 非 async、返回 `Disposition`；`inbound::spawn` 要 `HelloFlushed`。两条旧的字符串机检**删掉**（不可表示之后它们是死重量）。变异：试图在分派臂里 `.await` ⇒ **编译不过** |
| ⑤ | **G2**：字段判据收进 §10 区间 | 变异：把帧字段表整段挖掉 ⇒ 红（今天是绿） |
| ⑥ | **G3**：`inbound.rs` 不许 `observe::` | 变异：加一行 `use crate::observe::…` ⇒ 红 |
| ⑦ | **F90**：`id` 不许被解析 | 机检：`inbound.rs` 生产段里 `req.id` / `id` 只能被 clone/比较/回显，不许出现 `parse` / `split` / `strip_prefix` 之类 |
| ⑧ | 全量门禁绿 + 文档同步 | 每步失败都 `exit 1`；新字段被 U6a 护栏逼进文档 |

**不做**：不删一次性 `--resolve`（契约冻结）· 不改 CommandPlan 任何字段 · 不建新登记表。

## 测试策略

- **新增测试自己先用真变异打一遍**（本会话那条内存测试改到第三版才不是安慰剂）。
- 对拍的「非空 / 条数下限」做成**会让命令失败**的硬前置。
- 关键文件改动用 Edit 工具或落盘后 `grep -c` 复核（heredoc 静默失败过多次）。

## 实现期与计划的偏离

### 「让违规不可表示」比预想的更彻底 —— 两条机检**整条删掉**

计划写的是「改成类型见证 + `Disposition`」，我原以为机检还要留着做补充。做完发现不用：

- **Hello 顺序**：`inbound::spawn` 现在要一个 [`wire::HelloFlushed`]，而它**只能由
  `wire::write_and_flush_hello` 产出**（构造函数私有）。拿不到就调不了。
- **处理器不许跑在读循环上**：`dispatch` 改成**非 async** ⇒ 分派臂里**没有 `.await` 可写**。
  实测把 `send(...).await` 塞回一条臂里 ⇒ `error[E0728]: await is only allowed inside async
  functions and blocks`，**编译不过**，不是测试红。

⇒ 那两条机检成了死重量，删。**这是本轮最值得记的一条**：
D 审计击穿它们之后，我的第一反应是「把判据写得更严」——审计给的方向是反的，
**别再往判据上加正则**。加正则是追着变异跑，永远慢一步；改成不可表示是一次性的。

### `Disposition` 的第三档是被 `unknown_command` 逼出来的

原设计只有 `Done | Spawn`。但 `unknown_command` 那条臂原来用 `send().await`（保背压），
非 async 之后只能 `try_send` —— 那会在通道满时**丢掉错误应答**，客户端空等。
⇒ 加 `Reply(Frame)`：由调用方 await 发送，背压保住，而臂里仍然没有 `.await`。

### 「吸收」的实际含义与计划一致，但值得复述

一次性 `--resolve` **逐字不动**（实测三条用例 651 字节逐字节相同：正常 / 非法 sid / 坏 JSON）。
它的契约与仓外 aterm 冻结在 2026-07-18，且 aterm **暂不消费** —— 也就是说
它**随时可能开始被消费**，现在拆掉是拿别人的集成期赌。两条路复用同一个纯函数。

### F90 的落点与计划一致：不建表，把「不许解析 id」钉住

`running` 表的主键本来就是客户端给的不透明 `id`，天然合规。真正的风险是**后人顺手从里面抠信息**
（约定个 `sid:` 前缀然后 `strip_prefix`），那一刻 `id` 就不再不透明、daemon 开始依赖一个
它无权定义的结构。⇒ `the_request_id_is_never_parsed`。

### 字段判据收紧后，既有 31 个字段**一个都没误伤**

收进 §10 区间之后只有新加的 `data` 落网。反过来验证：把 §10 的帧字段表整段挖掉 ⇒
**20 个字段当场报缺**（旧判据下审计实测 16/31 照样通过）。

## 代码审计结果（D）

（本轮为 U6b 收尾，D 审计与 U6b-1 同一份报告的三条「登记归 U6b-3」项，已在上方逐条收口并变异复验：

| 变异 | 结果 |
|---|---|
| §10 帧字段表整段挖掉 | 红，报 20 个字段缺失 |
| `inbound.rs` 加 `use crate::observe::watcher as _;` | 红 |
| 从 `id` 里 `strip_prefix("sid:")` | 红 |
| 分派臂里写 `.await` | **编译不过**（`E0728`） |
| 一次性 `--resolve` 三条用例 | 与 HEAD 逐字节相同（651 字节，非空已核） |

）

## 工程审计结果（E）

- **账本 S6（wire 协议 + `IPC-PROTOCOL.md`）最终形态达成**：文档先修再冻结（U6a）+
  双向入方向（U6b-1）+ 能力协商两侧（U6b-2）+ **第一条真业务命令**（U6b-3）。
- **账本 S14（`--resolve`）**：账本原文「吸收进 backend 的计划面；线上形状逐字不变」——
  **交付**，且「吸收」落成**并存**而非搬走（契约冻结，理由已写进文档与代码）。
- **账本 S16（入方向命令面）**：四处双写已由 `hello_commands_match_the_dispatch_table` 收成一份。

**给 U7–U11 的移交**：控制面每搬一条动作进 daemon，就是加一条 `Disposition::spawn` 的臂 +
一条 `COMMANDS` 登记。三条纪律（不许跑在读循环上 / 必须声明 / 必须进文档）**自动生效**，
且前者是编译期的。这是 U6b 系列的主要结构收益。

## 签收

- [x] 过代码审计（D）
- [x] 过工程审计（E）
- [x] 主计划已更新（F）

### 门禁终态（每步失败都 exit 1）

daemon `cargo test` **224** · monitor `--lib` **668 / 3 ignored** · `npm test` **80 文件 1154 例** ·
`tsc` 0 · 两侧 `cargo fmt --check` 干净 · daemon clippy **0**。
