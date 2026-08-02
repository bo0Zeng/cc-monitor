# U8a-2b — `launch` 命令本体（平面 ② 搬进 daemon `control/`）

- 工作区：unified-backend · 第六梯队 · 任务 #96
- 前置：U8a（三平面分解）· D1 裁决（起进程受管清单）· U8a-2a（入方向通道已可达）
- 前身：[`U8a-2b-threat-model-and-blocker.md`](U8a-2b-threat-model-and-blocker.md) 是 **2a 之前**写的
  威胁模型 + 顺序订正。载体已经变了，本件按现状重写；那份留档不改。

## 摸底（先量，别照抄计划）

### 今天「起远端会话」是怎么走的

```
前端 remote-launch.ts / launch-render-*.ts
   └─ 渲染出一整条 **shell 命令串**（含 tmux new-session … && send-keys … ; tmux attach …）
        └─ tauri `launch_remote_terminal(origin, remote_cmd)`
             └─ launch.rs::build_remote_ssh_ps_command → ssh user@host -t "bash -lic '<串>'"
                  └─ launch_powershell_window（**Windows-only**）
```

⇒ **平面 ② 与平面 ③ 今天是焊死的**：本机开的那个终端窗口，**同时**就是执行远端 tmux 命令的
那条 ssh。命令串以 `tmux attach -t …` 收尾，正因为它要在那个窗口里把人接进去。

### 这决定了 2b 能做到哪、做不到哪

| | |
|---|---|
| **能做** | daemon 侧长出真正的平面 ②：一条 `launch` 入方向命令，**直接调 tmux（argv，不过 shell）**，建会话 / 键入载荷，**不 attach** |
| **做不到（本轮）** | 把生产路径切过去。原因是**结构性的，不是懒**：tauri 命令 `launch_remote_terminal(origin, remote_cmd)` 收到的已经是**一条渲染好的 shell 串**，拆不回结构化的计划。要切，得让前端改成发结构化请求 —— 那正是 **U8c**（两个 TS 渲染器 + IR 退役）的活 |

⇒ 本件的产出边界写死在这里，**报告里不许含糊成「起会话已经走 daemon 了」**。
生产切换登记为 **U8a-2c**（依赖 U8c 的前端改造）。

### `send-into` 为什么是这一切的关键

`shared/ccm` 的 `--tmux` 只有幂等 create-or-attach 一种形态，**没有**「就地复用已存在的 idle
tmux、不新建」。硬套会让 #76 复发（claude 已退但 tmux 还在 ⇒ 短路跳过 send-keys ⇒ 把用户
attach 进空 shell）。所以 `launch-render-cli.ts` **诚实放弃**：`send-into` 强制走兜底渲染器。

daemon 直接调 tmux 之后，这个表达力缺口**消失** —— `send-into` 成为一等模式。
这就是 U8a 说的「#76 防线的形态会变：从『渲染器拒绝渲染』变成『daemon 有一条专门的命令』」。

## DoD

1. daemon `control/launch.rs`：`launch` 入方向命令，三样都真做到 —— 建会话（幂等）、
   设 `@ccm_sid` + 标题、键入载荷；**`send-into` 是一等模式**，会话不存在时**报错而不是新建**。
2. **不 attach**。attach 是平面 ③，daemon 在远端，开不了你面前的窗。
3. **argv，不过 shell**。tmux 命令用 `Command::new("tmux").args([...])`，
   于是「引号 / 注入」这一整类问题在这条路上**不存在**，不是「被挡住了」。
4. daemon 侧只做**形状校验**（fail-fast + 可诊断），**代码里写明它不是安全边界**。
   ⚠ `launch.rs` 的「禁双引号」是 PowerShell 专属，**不成立也不许照抄**。
5. 新增的起进程点**登记进 `readonly_guard::spawn_registry`** + `INVARIANTS §41.6` 写面补一行。
6. 失败语义能分辨**「没起成」**与**「起了但没确认」**（后者= 会话在、载荷未必落）。
7. **真跑通**：真 daemon 二进制 + **真 tmux**（私有 socket 隔离），建出会话、载荷真落进去。
8. **#76 防线形态迁移**：TS 侧那条（`send-into → 强制兜底` + `#76 防线应自述` 测试）
   **原样保留**（生产路径还在走它），daemon 侧新增**对应的自述测试**，两边互相指认。
9. 既有护栏全绿：`spawn_registry` · `readonly_guard` · `layering_guard` · `no_timer_guard` ·
   `hello_commands_match_the_dispatch_table` · `every_inbound_command_appears_in_the_protocol_doc`。

### 不做什么

- **不切生产路径**（见摸底；登记 U8a-2c，依赖 U8c）。
- **不动 `shared/ccm`**（U9）。
- **不做平面 ③ 的 OS 分派**（U8b）。
- **不加 attach 命令** —— 平面 ③ 的事，且今天的 attach 走本机窗口没有问题。

## 设计

### 命令形状

```text
→ {"id":"…","cmd":"launch","args":{
     "mode":"create-or-attach" | "send-into",
     "name":"cc-1a2b3c4d",
     "payload":"cd '/x' && claude --resume …",
     "cwd":"/x",            // 可选，仅 create-or-attach 用
     "ccm_sid":"<完整 sid>"  // 可选，仅 create-or-attach 用
   }}
← {"kind":"reply","id":"…","ok":true,"data":{"created":true,"typed":true,"session":"cc-1a2b3c4d"}}
```

`data` 三个字段就是失败语义的载体（DoD 6）：

| 结局 | `ok` | `code` | 含义 |
|---|---|---|---|
| 新建 + 键入成功 | true | — | `created=true, typed=true` |
| 会话已存在（幂等短路） | true | — | `created=false, typed=false` —— **不重复 resume**，与今天逐字同义 |
| `send-into` 键入成功 | true | — | `created=false, typed=true` |
| tmux 不在 PATH | false | `no_tmux` | **没起成** |
| `send-into` 但会话不存在 | false | `no_such_session` | **没起成**（绝不顺手新建 —— 那是 #76 的反向） |
| `new-session` 失败且会话也不存在 | false | `create_failed` | **没起成** |
| 会话在，但 `send-keys` 失败 | false | `typed_unconfirmed` | **起了但没确认** —— 调用方不许重试 create（它在），要告诉用户 |
| 形状不合 | false | `invalid_args` | 没起成（**刻意不叫 `bad_request`** —— 那是协议级那一层的，一词两义会让客户端分不出「我发的 JSON 坏了」与「参数不合适」；D 审计 P6）|

### 校验：形状，不是安全

入方向命令来自**已经握着这台机器 SSH 会话**的对端 —— 它本来就能在那台机器上跑任意命令。
daemon 再校验一遍挡不住任何它原本挡不住的东西（U8a-2 摸底的结论，不重新讨论）。
所以这里只做**形状校验**：缺字段 / 类型不对 / 空 / 超长 / 含控制字符 ⇒ 回结构化错误，
而不是把畸形串塞给 tmux 让它以奇怪的方式失败。**这个区别要写在代码里**，
否则下一个人会以为那层是安全边界而放松上游。

**明确不抄的一条**：`launch.rs::build_remote_ssh_ps_command` 的「禁双引号」是
PowerShell 5.1 向 native 程序传参的历史畸变（`wt.exe` 那条路），与 daemon 无关；
这条路根本不过 shell，抄它等于把一个 Windows 怪癖套到 tmux argv 上。

### `=name:` 精确匹配（F01，血的教训）

裸 `-t <名>` 不是精确匹配（tmux 按「精确 → 前缀 → glob」解析），本仓踩过「杀错/打错兄弟会话」。
daemon 侧需要同一条规则，但**不需要 shell 引号**（argv 直传）。
⇒ daemon 自己一份 `exact_target(name) -> "=name:"`，并加一条**跨轨对拍**：
monitor `tmux.rs` 的 `={target}:` 形状与 daemon 的必须一致（`include_str!` 钉住）。
登记进账本（命令面又多一处副本，但这处有护栏）。

## 逐条步骤

| # | 做什么 | 怎么验证 |
|---|---|---|
| 1 | daemon `control/launch.rs`：`LaunchRequest` 解析 + 形状校验 + 三种结局 | 模块单测（纯函数部分） |
| 2 | tmux 动作用 argv 跑；`exact_target` + 跨轨对拍 | 单测 + 对拍变异 |
| 3 | 接进 `inbound::dispatch` + `COMMANDS`；**`Disposition::SpawnBlocking`**（起进程会阻塞，既不能占读循环、也不能占 tokio worker —— D 审计 P5 订正了原计划里的 `spawn`） | `hello_commands_match_the_dispatch_table` + `the_dispatch_table_puts_blocking_commands_on_the_blocking_arm` |
| 4 | `spawn_registry` 登记 + `INVARIANTS §41.6` 补一行 | `every_process_spawn_in_production_is_registered` |
| 5 | `doc/IPC-PROTOCOL.md` 补 `launch` 小节 | `every_inbound_command_appears_in_the_protocol_doc` |
| 6 | e2e：真 daemon + **真 tmux**（私有 socket），建会话 / 幂等短路 / send-into / 会话不存在 / 载荷真落 | 新断言进 `inbound-frames`，地板同步 |
| 7 | monitor 侧 `inbound_client::launch_args` 参数构造器 + **跨轨对拍**（键名必须恰好等于 daemon 解析器认的那几个） | `launch_args_field_names_match_the_daemon_parser` |
| 8 | #76 防线两侧互指 + daemon 侧自述测试 | 两侧各一条 |

## 测试策略

- 纯函数（校验 / argv 构造 / 结局映射）→ 单测。
- **真 tmux** → e2e（私有 `TMUX_TMPDIR` + `unset TMUX`，绝不碰用户真实 server）。
- 每条新判据自己变异一次（血泪 5）；接缝单独有判据（血泪 10）。

## 代码审计结果（D）

本轮的 D 走的是**设计审计**（用户中途点名「daemon 架构要仔细全面考虑」，两个专职设计 agent 并行）。
它比常规 D 更值：**两条真缺陷是在实现中途被逮到的，不是事后。**

### 阻塞级（两条，都已修 + 都做了变异复验）

| # | 发现 | 处置 |
|---|---|---|
| **P4** | **`cancel` 对同步处理器无效，而 `launch` 就是同步的。** `abort()` 只在 await 点生效 ⇒ 客户端收到 `Cancelled`、`CallError::Cancelled`，而**远端 tmux 会话照样建出来、载荷照样键入**。控制面在骗调用方 | 登记表改 `InFlight { abort, cancellable }`；不可取消的**留在表里**并回 `not_cancellable`。两条判据：机制（`spawn_handler(..., false)` 的行为）+ **接缝**（按 `dispatch` 返回的变体判） |
| **P5** | **阻塞处理器跑在 tokio worker 上。** `main` 是裸 `#[tokio::main]`（worker 数 = 可用并行度），单核机器（Pi 那一档，正是目标机型）上一条在跑的 `launch` 占住唯一 worker，而 `writer_task`（出方向唯一出口）与入方向 reader 在同一 runtime ⇒ 「远端还活着但一句话不说」 | 新增 `Disposition::SpawnBlocking` → `tokio::task::spawn_blocking` |

> **P4 的判据踩了本区第 10 条纪律，当场复现**：第一条判据直接调 `spawn_handler`，
> 把 `dispatch` 里那一档改回 `true` 它**照样绿** —— 因为它不经过 `dispatch`。
> 补了 `the_dispatch_table_puts_blocking_commands_on_the_blocking_arm`（按数据判变体，不是扫文本），
> 变异（`launch` 退回普通 `spawn`）复验红。

### 重要（已修）

- **P1 · `spawn_registry` 的扫描面是手写文件名单、没有反查** ⇒ `control/launch.rs` 起 `tmux`
  **落在盲区**，本件 DoD 第 5 条不落地也不会红。改成递归遍历 `src/` + 文件数自检；
  变异（起进程改成未登记的程序名）复验红。**这是「扫描面画小了」那一族的第五次，而且是 D1 那轮我自己埋的。**
- **P7 · `protocol_doc_guard` 头注宣称的强度与实现不符**：头注两处写「必须落进 §10 的两张表之一 ——
  不是『全文出现过就行』」，实现却是 `DOC.contains()` 全文子串。收紧到 §10 的代码跨度 + 订正头注。
- **P3 · 命令名的文档对拍能被同名字段白嫖**（§10 的帧字段表里本来就有 `status`/`name`/`sid`）。
  收到「入方向」小节。**并且验证了这条断言本身**：退回旧作用域 + 加一条 `status` 命令 ⇒ 旧判据**真的绿**。
- **P6 · 错误码一词两义**：`launch` 的形状错误改叫 `invalid_args`；协议级 code 闭集写进 §10。
  `resolve` 那条**刻意不动**（与仓外 aterm 的一次性契约冻结在 2026-07-18，两条路复用同一纯函数）。

### 登记不做

`P2` 命令载荷无文档对拍 · `P8` e2e 断言数不随命令数增长 · `R1/R2` 命令面注册表
⇒ 全部归**新开的 U8a-2d**（排在本件之后、**U10 之前**）。理由采纳设计 agent 自己的反对意见：
4 条命令不值得付注册表的结构成本；等 U10/U11 把命令推到 8-9 条之前做，那时手上还有
`launch` 这个「既阻塞、又有 args/data、又还没冻结」的真样本。

`P9` monitor 侧读行无上限 ⇒ 归 U10（`Reply.data` 变大之前不动）。

### 变异复验清单

| 判据 | 变异 | 结果 |
|---|---|---|
| `every_process_spawn_in_production_is_registered` | `control/launch.rs` 起未登记的程序名 | 红 |
| `send_into_never_creates_a_session`（源码扫描） | `send-into` 分支里加 `new-session` | 红 |
| e2e「send-into 没有顺手新建会话」 | 同上（真 tmux 验证） | 红 |
| `the_dispatch_table_puts_blocking_commands_on_the_blocking_arm` | `launch` 退回普通 `spawn` | 红 |
| `cancelling_a_blocking_command_says_not_cancellable_instead_of_lying` | 阻塞命令标成可取消 | 红（但**只对机制**，接缝要靠上一条 —— 已登记） |
| `every_inbound_command_appears_in_the_protocol_doc` | 加一条与帧字段同名的 `status` 命令 | 红（收紧前**绿**，已实证） |

## 工程审计结果（E）

- **生产路径没切，理由是结构性的**：tauri 命令 `launch_remote_terminal(origin, remote_cmd)` 收到的
  已经是一条渲染好的 shell 串，拆不回结构化计划 ⇒ 切换要等前端改成发结构化请求（U8c）。
  登记 **U8a-2c**。**报告里不许含糊成「起会话已经走 daemon 了」。**
- **#76 防线的形态迁移已落地且两侧互指**：TS 侧那条（`send-into` 强制走兜底）**原样保留**
  （生产路径还在走它）；daemon 侧 `send-into` 成为一等模式，语义陷阱换了形状（不是「近似不了」，
  是「别顺手新建」），由源码扫描 + 真 tmux e2e 两条钉住。
- **账本**：S16 命令面从五处副本变六处（e2e 新增 launch 断言）；新增 **S23**（`=name:` 精确匹配的
  第三处副本，daemon argv 版，有跨轨对拍）。
- **两份设计稿落盘**（`DESIGN-命令面怎么长大.md` / `DESIGN-CC新功能的扩展缝.md`），
  其中后者带出两条**今天就坏/就错**的读面缺陷（`subagent.rs` 对新 workflow 目录加载失败；
  `cards/slash.ts` 的注释被语料证伪）—— **刻意不塞进本轮**，新开件。

## 签收

- [x] 过代码审计（D）—— 2 阻塞 + 4 重要全部处置，6 条判据逐个变异复验
- [x] 过工程审计（E）—— 账本更新；生产切换的结构性理由写清并登记 U8a-2c
- [x] 主计划已更新（F）
