<!-- 生成物，勿手改：由 tests/backend/protocol_doc_gen.rs 从代码生成。重生成：在 src/backend 下 CCM_REGEN_PROTOCOL_DOC=yes cargo test --lib -- protocol_doc_gen -->

# 协议参考（帧 · 命令 · CLI）

总述（载体 · 信封 · 握手 · 冻结面）在 [IPC-PROTOCOL.md](IPC-PROTOCOL.md)；本页逐格列形状。

## 1. 出方向帧

每一帧一行 JSON，`kind` 是下面的小标题。`?` ＝ 可缺（缺 ＝ 没有这一格，不是 `null`）。

### `hello`

Handshake sent once when a client connects。

| 字段 | 类型 | 说明 |
|---|---|---|
| `v` | number | 协议版本号 |
| `build_id` | string | 这份后端二进制的身份（`BUILD_ID`）；换不换后端按它判 |
| `host_arch` | string | 这台机器的架构（`x86_64` / `aarch64` …） |
| `claude_dir` | string | ⚠ **冻结兼容字段，不是欠账** |
| `homes` | [AgentHome]? | 本机上**各 agent 的 home 目录**（`[{agent_kind, path}]`） |
| `capabilities` | [string]? | 本后端声明支持的**能力 token 集**（开放字符串，加法式） |
| `emits` | [string]? | 本 backend **会发射的帧 kind 集** （snake_case，如 "session_status"/"turn_end"） |
| `commands` | [string]? | 本 backend **接受的入方向命令集** |
| `unavailable` | [Unavailable]? | 本 backend **接得下、但在这台机器上做不到**的命令，以及原因（`[{command, code}]`，见 `Unavailable`） |
| `host_env` | {string: string}? | **起我的宿主交给我的那几格环境，原样回显**（`{名: 值}`，名单 `HOST_ECHO_ENVS`） |
| `uncancellable` | [string]? | `commands` 里**撤不动**的那几条（阻塞档：开跑之后 `cancel` 回 `not_cancellable`） |

### `line`

One JSONL line tailed from a session file —— 带的是**成品**：这一行在渲染模型里是什么（`message`，缺 ＝ 不进界面、照占号）与它自己的 `cwd`。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `seq` | number | 这一行在本条流里的序号（按文件单调递增）；不是续传键，续传用 `byte_offset` |
| `message` | JSON? | 这一行在渲染模型里的成品；缺 ＝ 不进界面、照占号 |
| `cwd` | string? | 这条记录自己的工作目录 |
| `byte_offset` | number? | 本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——语义**逐字节对齐 aterm `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒ `tail -c +(N+1)` |
| `rid` | string? | 这一行的对账键（适配层 `RecordFace::response_id` 给；流的「开始」带同一个值） |
| `raw` | string? | 这一行记录的**原文**（去掉行尾换行） |

### `session_added`

A new session file appeared。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `agent_kind` | string? | 会话属哪 agent kind——`"codex"`（Codex 会话） |
| `liveness_confidence` | string? | 判活置信度——`"heuristic"`（Codex 无 pidfile、mtime/proc 启发） |
| `background` | bool? | 是不是后台会话（不是人坐在终端里对话的那种） |
| `attachable` | bool? | attach 进去对人有没有意义 |
| `cwd` | string? | pidfile 记的 `cwd`：进程起在哪个目录（客户端认「我刚起的那条起来了」用） |
| `project_dir` | string? | 会话的项目目录：会话起在哪个目录（tab 标题 · 打开工作目录 · 分组都用它） |
| `name` | string? | pidfile 里的会话名 |
| `path` | string? | 该会话 jsonl 的远端绝对路径（同 sid 多文件时取 mtime 最新者）——monitor 旁路快照（`--read-session`）用 |
| `lines` | number? | Batch8 审计 D-I2（additive）：tail-only 模式下 prime 时的完整行数 L ——monitor 校验快照拉到的行数 ≥ L 才算成功（不足 = 中途断/backend 报错，触发重试；exit status 经 ChannelStream 拿不到，行数校验更强） |
| `activity` | SessionActivity? | 宣告时此刻在干什么（适配层翻好的，`SessionActivity`） |
| `waiting_for` | string? | 宣告时在等什么（同 `session_status`） |
| `container` | SessionContainer? | 这条会话住在什么容器里（见 `SessionContainer`） |
| `pid` | number? | 那个 claude 进程的 **pid** |

### `session_status`

会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，天然稀疏）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `activity` | SessionActivity? | 此刻在干什么（同 `session_added.activity`） |
| `waiting_for` | string? | 在等什么（pidfile 里的 `waitingFor`） |
| `liveness_confidence` | string? | 判活置信度（同 SessionAdded；状态变化时带） |

### `session_state`

**会话账本的成品**：这条会话离开「活」之后是可重连还是已结束（见 `SessionFate`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `state` | SessionFate | 离开「活」之后的去向 |

### `session_removed`

A session file went away。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `cause` | RemovalCause? | 这个 sid 是「死了」还是「被顶替了」 |

### `turn_end`

turn-end 边沿（一轮 assistant 完成）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `uuid` | string | 这一轮最后那条 assistant 记录的 uuid |

### `overflow`

The bounded frame channel back-pressured and the reader had to drop `dropped` frames (a slow/wedged SSH pipe)。

| 字段 | 类型 | 说明 |
|---|---|---|
| `dropped` | number | 自上一次哨兵以来丢掉的帧数 |
| `lost` | [LostFrame]? | 那批丢帧里**不可恢复**的那些的身份 |
| `lost_truncated` | bool? | 身份表是**有界**的（见 `watcher.rs` 的 `LOST_IDENTITY_CAP`） |

### `reply`

**入方向命令的应答**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 回显请求的 `id` |
| `ok` | bool | 成功与否 |
| `code` | string? | 失败时的码（协议级或命令级） |
| `message` | string? | 失败时给人看的那一句（不含下层原话与码：那些进 `detail`） |
| `detail` | string? | 失败时「复制详情」那几行（句子下面的「项名：值」：时刻 · 机器 · 命令 · 码 · 原话），后端写好、界面原样复制 |
| `data` | JSON? | 命令的返回值（如 `resolve` 的 CommandPlan） |

### `cancelled`

某个在跑的命令**已被取消**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 被取消的那条命令的 `id` |

### `accounts_changed`

**这台机器上的账号清单变了**（账号 manifest 被改写）。

（无字段）

### `profiles_changed`

**这台机器上的配置文件（`~/.cc-monitor/profiles.toml`）变了**（别处改了它，或设置窗刚写了它）。

（无字段）

### `quota_changed`

**这台的额度账显示得出来的那几格变了**（某个号的用量取整后的百分比 · 重置时刻 · 状态 · 被拒）。

（无字段）

### `rotation_changed`

**这台某个会话的轮换或「账号」格变了**（换了号 · 记了一条 · 改了它的轮换 · 它跟随的默认轮换改了）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 轮换或「账号」格变了的会话 |

### `rotation_rules_changed`

**这台的轮换规则表或默认指向变了**（新建 · 改 · 改名 · 删 · 设为默认；本进程或别的进程写的都推）。

（无字段）

### `plan_changed`

**这台某个 pb 工作区的计划变了**（计划仓 `.planned-build/` 或工作区 `.env` 有动静，重跑 `pb dump` 后输出摘要或要你看的数变了；认可 / 撤销认可也推一帧新的数）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `workspace` | string | 工作区根 |
| `rev` | string | 新的输出摘要（同 `plan-read` 的 `rev`） |
| `needs` | number | 这个工作区此刻要你看的数（没认可的，不含 agent 问人那一种 —— 那一条由会话那一侧数；同 `plan-read` 的 `needCount`） |

### `tasks_changed`

**这台机器上某个会话的任务清单变了**（`<agent 家>/tasks/<sid>/` 里有动静）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 任务清单变了的会话 |

### `sessions_replayed`

**这台机器的活会话清单报完了**：`observe::watcher::watch_loop` 的 Phase 1 （同步扫 `sessions/`、对每个活 pidfile 发一帧 `session_added`）走完那一刻发**一次**。

（无字段）

### `session_file_gone`

**活会话的记录文件不见了**（被删 / 被改名走了）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `path` | string | 不见了的那份记录文件 |

### `session_file_reread`

**活会话的记录文件被改过了，已从头重读**（截短 · 或游标之前被原地改写）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `path` | string | 被改过、已从头重读的那份记录文件 |
| `why` | RereadWhy | 为什么从头重读 |

### `link_data`

**一条链路的下行字节**（`dial/link.rs`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `link` | string | 链路 id（`link-open` 时客户端给的） |
| `data` | string | 下行字节，标准 base64（解码后 ≤ 32 KiB） |

### `link_end`

**一条链路不会再有字节了**；后端已经忘掉这个 `link` id。

| 字段 | 类型 | 说明 |
|---|---|---|
| `link` | string | 链路 id |
| `error` | string? | 非正常收尾的原话；缺 ＝ 正常收尾 |

### `transfer`

**一趟传输此刻的样子**（`control/transfer.rs`，`transfer-start` 之后才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 传输单 id |
| `got` | number | 已传字节 |
| `total` | number | 总字节 |
| `end` | TransferEnd? | 只在最后一帧：这一趟怎么收场的 |

### `probe`

**测试连接那一趟的一格进度**（`dial/probe.rs`，`remote-probe` 在跑时才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 测试连接那一趟的票（`remote-probe` 交来的） |
| `cell` | JSON | 一格进度，恰好一个键：`stage` · `reached` · `end` |

### `tap`

**中转抄出来的一个 SSE 事件**（或一个响应的收尾）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `stream` | string | 请求自带的会话标识头的值 |
| `run` | string? | 这段流归哪个子运行（主运行 ⇒ 不上线） |
| `resp` | number | 本进程第几段 |
| `n` | number | 这一段里第几件，从 0 连续 |
| `ev` | StreamEv? | 归一事件（上游原始事件已在后端按协议面折过；界面不认任何一家的事件名） |
| `end` | TapEnd? | 这一段的收尾（与 `ev` 恰有一个） |

### `session_runs`

一个会话的运行表（主运行之外的那几个子运行：标签 · 状态 · 最近一件事 · 派出它的那次工具调用）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `runs` | [RunInfo] | 在表里的子运行（整份） |
| `ended` | [RunEnded] | 被挤出表的已收场子运行 |

### `session_branch`

一份会话记录的**主线外清单**（用户回退重发后留在文件里的旧那一支）：那几条记录的 `id`（与记录成品的 `id` 同值；只含进界面的，文件序）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `off` | [string] | 主线外那几条的 `id` |

### `terminal_screen`

**一个终端此刻的一整屏**（终端实时预览，`control/terminal_follow.rs`；`terminal-follow` 之后才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 订阅票（`terminal-follow` 时客户端给的，本后端只当不透明的串回填） |
| `seq` | number | 这条订阅里第几帧（从 1 连续） |
| `view` | JSON | 那一屏（同 `terminal-preview` 的回话） |

### `terminal_follow_end`

**一条终端订阅停了**（不会再有画面）；后端已经忘掉这张票。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 订阅票 |
| `why` | FollowEnd | 为什么停了（给程序认） |
| `said` | string | 给人看的那一句（后端写好，界面原样上屏） |

## 2. 信封与帧里用到的类型

#### `RemovalCause`

`Frame::SessionRemoved` 的原因。

- `gone` —— 真的没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind
- `superseded` —— 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**

#### `SessionContainer`

`Frame::SessionAdded` 的 `container`：**这条活着的会话住在什么容器里**，词与 `terminals-list` 同一套。

（手写序列化，形状见上面那一句。）

#### `SessionFate`

`Frame::SessionState` 的 `state`：**一条会话离开「活」之后是什么** ——那台机器的后端自己裁（`observe::session_ledger`：摘除原因 ＋ 它自己那份 tmux 快照），客户端只收成品、不再猜。

- `reconnectable` —— claude 退了、它的 tmux 会话还在（`@ccm_sid` 仍挂着它）⇒ 接得回去
- `ended` —— 进程没了、容器也没了（或被顶替了）⇒ 只能 resume

#### `LostFrame`

一条**丢了就不可恢复**的帧的身份。

| 字段 | 类型 | 说明 |
|---|---|---|
| `kind` | string | 帧种（与 `kind` tag 同一套 snake_case 名字） |
| `subject` | string? | 主体：会话 sid / tmux 会话名 |

#### `AgentHome`

`hello.homes` 的一项 —— **某个 agent 在这台机器上的 home 目录**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `agent_kind` | string | 哪个 agent —— **值**，与 `session_added.agent_kind` 同一套取值空间 |
| `path` | string | 该 agent 在这台机器上的 home 目录（绝对路径） |

#### `SessionActivity`

一条活会话此刻在干什么（与哪一家无关的几态；适配层从那一家的进程状态翻过来，翻不出 ⇒ 不给）。

- `working` —— 一轮在跑
- `needs_you` —— 在等人（批准 · 回答 · 弹窗）
- `idle` —— 闲着，等下一句输入
- `background_work` —— 一轮停了，它在后台起的命令还在跑（跑完多半会接着干）

#### `Unavailable`

`hello.unavailable` 的一项 —— **这条命令我接得下，但在这台机器上做不到，以及为什么**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `command` | string | 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::command_names`） |
| `code` | string | 为什么做不到 —— **就是真调用那一刻会回的那个命令级 code**（如 `no_tmux`） |

#### `FollowEnd`

一条终端订阅为什么停了（`Frame::TerminalFollowEnd` 的 `why`）。

- `gone` —— 那个终端没了（窗格 / tmux 会话关了）
- `lost` —— 看着它的那条路断了（tmux 控制模式客户端退了、抓屏失败），终端也许还在
- `too_big` —— 那一屏大过一帧的上限

#### `RunEnded`

被挤出运行表的一个已收场子运行（`Frame::SessionRuns` 的 `ended` 一项）：是哪个 · 派出它的那次工具调用 · 终态。

| 字段 | 类型 | 说明 |
|---|---|---|
| `run` | string |  |
| `tool` | string |  |
| `state` | RunState |  |

#### `RunInfo`

一个子运行此刻的样子（`Frame::SessionRuns` 的一项）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `run` | string |  |
| `label` | string? |  |
| `kind` | string? |  |
| `tool` | string? | 派出它的那次工具调用（父侧 id）；还没对上 ⇒ 不上线 |
| `parent` | string? | 派出它的那个子运行；主运行派的（或还没对上派出它的那次调用）⇒ 不上线 |
| `state` | RunState |  |
| `last` | RunDid? |  |
| `waiting` | string? | 在等哪个工具的结果（它最近一条记录是一次还没拿到结果的工具调用、且还在跑） |
| `started_ms` | number? | 开始：派出它的那条记录（没见到 ⇒ 它自己最早的一条） |
| `started_text` | string? | `started_ms` 在这台本地钟上的钟面 `HH:MM`（跟着 `started_ms` 一起写；界面照抄、不换算） |
| `active_ms` | number? | 最近动静：它自己最近一条记录 |
| `ended_ms` | number? | 收场：说它收场的那一条（还没收场 / 状态不明 ⇒ 不上线） |
| `why` | RunWhy? | 为什么是这个结局（在跑 ⇒ 不上线） |
| `error` | string? | 失败收场时的报错原话（说得出才有；至多 `observe::runs::ERROR_CHARS` 个字） |
| `calls` | number? | 它调了几次工具（它自己的记录里数的；零 ⇒ 不上线） |
| `background` | bool? | 派出那一方没等它、接着做自己的事（后台派出；否则不上线） |

#### `RunState`

子运行的五态。

- `running`
- `done`
- `failed`
- `stopped` —— 被叫停
- `unknown` —— 没有任何收场信号、子记录又久未再写（`observe::runs::STALE_AFTER`）：不当它在跑

#### `RunWhy`

一个子运行为什么是这个结局（`RunInfo::why`）。

- `reported` —— 派出它的那一方说的（拿到了结果 / 收到了收场通知）
- `own` —— 它自己的记录写出了终局
- `quiet` —— 没有收场信号、它的记录久未再写（状态不明）
- `orphaned` —— 派出它的会话退了，再也等不到收场信号（状态不明）

#### `TapEnd`

一个响应怎么收场的（`Frame::Tap` 的 `end`）。

- `done` —— 上游把响应说完了（转发正常收尾）
- `broken` —— 转发以错误收尾：下游走了（claude 被 Esc 打断）· 上游断了 · 写不动

#### `RereadWhy`

`Frame::SessionFileReread` 的「为什么从头重读」。

- `truncated` —— 变短了（比读到过的最长还短）
- `rewritten` —— 没变短，但游标之前那一截被原地改写过（末尾指纹对不上）

#### `TransferEnd`

一趟传输怎么收场的（`Frame::Transfer` 的 `end`）。

- `state: "done"`，带 `bytes` number · `sha256` string? —— 传完了
- `state: "failed"`，带 `why` string · `code` string? · `detail` string? —— 失败（那一句 ＋ 复制详情）
- `state: "cancelled"` —— 撤了（`transfer-stop` / 本机流断了）

#### `Request`

**入方向**请求信封。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 客户端发号的不透明串，应答原样回显；同一时刻在跑的命令里不许重复 |
| `cmd` | string | 命令名（`hello.commands` 里的一个） |
| `args` | JSON? | 命令的参数对象；缺 ＝ `null` |
| `within_ms` | number? | 发起方这一发愿意等多久（毫秒） |

## 3. 协议级错误码

与哪条命令无关；命令自己的码列在各命令下面。

| 码 | 什么时候 |
|---|---|
| `bad_request` | 这一行不是合法的请求信封 JSON（`id` 无从得知时回空串） |
| `line_too_long` | 单行超过 1 MiB，整行丢弃 |
| `unknown_command` | `cmd` 不在本后端的命令表里 |
| `duplicate_id` | 同一个 `id` 的命令还在跑 |
| `handler_panicked` | 处理器内部崩了（进程照常活着） |
| `not_cancellable` | `cancel` 指向一条阻塞档命令：开跑之后撤不动，去等它自己的应答 |
| `shutting_down` | 后端在收场，新来的阻塞档命令一个字节不动 |

## 4. 入方向命令

请求 `{"id","cmd","args","within_ms"?}`，应答 `reply` 帧：成功 `ok:true` ＋ `data`，失败 `ok:false` ＋ `code` ＋ `message`（个别码带 `data`）。
字段表里 → ＝ 在 `args` 里，← ＝ 在 `data` 里。嵌套对象的字段平铺在同一张表里。

### 4.1 连接本身

#### `cancel`

撤掉一条在跑的命令（`target` = 它的 `id`）；撤不存在的 id 也回 `ok`。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `target` | → | 要撤的那条命令的 `id` |

#### `link-open`

开一条链路。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `dial` | → | 拨号请求（`host` · `port` · `user` · `key_path` · `use` …，蛇形键）；`use:"files"` 开 sftp 一问一答 |
| `link` | → | 链路 id：客户端给、客户端负责唯一 |
| `window` | → | 初始下行信用（字节），在 [32 KiB, 16 MiB] 之内 |

码：`bad_args` · `unsupported_use` · `duplicate_link` · `too_many_links`

#### `link-data`

往链路里送一块上行字节。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `data` | → | 无 |
| `link` | → | 链路 id |

码：`bad_args` · `no_such_link` · `link_busy` · `link_closed`

#### `link-credit`

还下行信用。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | → | 客户端读走了多少字节（累计信用不超过 16 MiB） |
| `link` | → | 链路 id |

码：`bad_args` · `no_such_link`

#### `link-close`

关一条链路。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `link` | → | 链路 id；关不存在的也回 `ok` |

码：`bad_args`

#### `transfer-upload`

开一张上传单。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `dial` | → | 拨号请求，同 `link-open` |
| `id` | → | 传输单 id（`xfer-<n>`） |
| `key` | → | 暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端 |
| `local_path` | → | 本机一份普通文件的路径 |

码：`bad_args` · `io_failed` · `busy` · `too_many_transfers`

#### `transfer-download`

开一张下载单。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `dial` | → | 拨号请求，同 `link-open` |
| `id` | → | 传输单 id |
| `local_path` | → | 本机落点（绝对路径；也收 `{"b16":…}`） |
| `remote_path` | → | 远端路径 |

码：`bad_args` · `refused` · `too_many_transfers`

#### `transfer-start`

传输单起跑。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → | 传输单 id；之后进度与终局走 `transfer` 帧 |

码：`bad_args` · `no_such_transfer` · `already_started`

#### `transfer-stop`

撤一张传输单。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → | 传输单 id；撤不在册的也回 `ok` |

码：`bad_args`

#### `ping`

问活：零载荷，回 `ok`。

不收 `args` · 可撤 · CLI：`ccm -- --ping`

### 4.2 文件管理

#### `files-create`

在文件管理目标根底下新建一份此前不存在的文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-create`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 这一趟写进去了几个字节 |
| `content` | → | 要写进去的字节 |
| `path` | ← | 真正落盘的那个绝对路径，**解完 symlink 的**（原始字节形） |
| `rel` | → | 相对 `root` 的那一段 |
| `root` | → | **用户指定的那个文件管理目标根** |
| `single` | → | 可选布尔：`true` ⇒ `rel` 只许是**一段名字**（界面就地新建 / 改名敲的那一格）；`files-mkdir` 的 `rel`、`files-rename` 的 `to` 同 |

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-mkdir`

新建一个目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-mkdir`

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ← | 建出来的那个目录（父目录解完 symlink 的） |
| `rel` | → | 目标根 ＋ 相对段 |
| `root` | → | 目标根 ＋ 相对段 |
| `single` | → | 可选布尔：`true` ⇒ `rel` 只许一段名字；名字规则同 `files-create`，不合 ⇒ `bad_name` |

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-rename`

改名 / 同根内移动。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-rename`

| 字段 | 向 | 说明 |
|---|---|---|
| `from` | → | 两个相对段，**各过一遍路径解析**（只解 `from` 的话，`to` 半路一条链接就能把东西搬到根外） |
| `path` | ← | 新名字的落点 |
| `root` | → | 目标根 |
| `single` | → | 可选布尔：`true` ⇒ `to` 只许一段名字；`to` 的名字规则同 `files-create`，不合 ⇒ `bad_name` |
| `to` | → | 两个相对段，**各过一遍路径解析**（只解 `from` 的话，`to` 半路一条链接就能把东西搬到根外） |

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-delete`

删一个文件或一个空目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-delete`

| 字段 | 向 | 说明 |
|---|---|---|
| `expect` | → | 可选，字符串或 `{"b16": …}`：「我读到的是这一份」 |
| `limit` | → | 可选，只对 `recursive: true`：这一趟至多删几条 |
| `path` | ← | 删掉的那一项 |
| `recursive` | → | 布尔，**缺省 `false`** |
| `rel` | → | 目标根 ＋ 相对段 |
| `remaining` | ← | 还剩几条没删（只有带 `limit` 删够了停下时不是 `0`）：调用方再发一趟同样的请求接着删 |
| `removed` | ← | 这一趟真删掉了几条（含目标自己；不递归那一支恒 `1`） |
| `root` | → | 目标根 ＋ 相对段 |

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-chmod`

改 unix 权限位。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-chmod` · 没有 unix 权限位的机器上做不到

| 字段 | 向 | 说明 |
|---|---|---|
| `before` | ← | **改之前**的权限位（十进制、低 12 位）：撤销 ＝ 拿它再发一趟 |
| `mode` | → ← | **十进制数值**（`493` = `0o755`），只收低 12 位；超出 ⇒ `refused` |
| `path` | ← | **解到底**的那个真路径 |
| `rel` | → | 目标根 ＋ 相对段 |
| `root` | → | 目标根 ＋ 相对段 |

码：`bad_args` · `bad_path` · `io_failed` · `no_unix_mode` · `refused`

#### `files-copy`

同根内复制一份普通文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-copy`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 复制了几个字节（整棵时是全部普通文件之和） |
| `dirs` | ← | 几个目录（单文件那一形恒是 `1` / `0`） |
| `files` | ← | 复制了几个普通文件 |
| `from` | → | 两个相对段 |
| `links` | ← | 照原样复制了几条符号链接（单文件那一形恒是 `0`） |
| `overwrite` | → | 🔴 **覆盖策略显式** |
| `path` | ← | 落点（父目录解完 symlink 的） |
| `recursive` | → | **复制目录显式** |
| `root` | → | 目标根（与写面其余几条同形） |
| `to` | → | 两个相对段 |

码：`bad_args` · `bad_path` · `io_failed` · `refused`

#### `files-extract`

解压到一个新目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-extract`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 文件字节之和 |
| `dirs` | ← | 几个目录（含补出来的上级） |
| `files` | ← | 建了几份文件（含硬链接落成的拷贝） |
| `fresh` | → | 可缺席的布尔 |
| `links` | ← | 几条符号链接 |
| `path` | ← | 落点目录（父目录解完 symlink 的） |
| `rel` | → | 包在 `root` 下的相对段；格式按名字后缀认：`.zip` · `.tar` · `.tar.gz` · `.tgz`（不分大小写），其余 ⇒ `unsupported`（「不认这种包」） |
| `root` | → | 那个目录（字符串或 `{"b16": …}`） |

码：`bad_args` · `bad_path` · `exists` · `io_failed` · `refused` · `unsupported`

#### `files-link`

建一条符号链接。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-link`

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ← | 建出来的那条链接（父目录解完 symlink 的） |
| `rel` | → | 链接自己那条路径：`rel` 在 `root` 下过路径解析（同写面其余几条；字符串或 `{"b16": …}`） |
| `root` | → | 链接自己那条路径：`rel` 在 `root` 下过路径解析（同写面其余几条；字符串或 `{"b16": …}`） |
| `target` | → | 链接的目标文本，**原样**写进去（不解、不判，同 `cp -P`） |

码：`bad_args` · `bad_path` · `io_failed` · `refused`

#### `files-write-text`

覆盖写一份已经在的普通文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-write-text`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 写进去了几个字节 |
| `content` | → | 字符串或 `{"b16":…}` |
| `expect` | → | 🔴 **必须给**，恰好 `{"sha256": "<64 位小写十六进制>"}`：「我看的时候那一份」的摘要（`files-read-text` 交的那个） |
| `path` | ← | **解到底**的那个真路径（它跟链接，理由同 `files-chmod`） |
| `rel` | → | 目标根 ＋ 相对段 |
| `root` | → | 目标根 ＋ 相对段 |
| `sha256` | ← | 写进去那份的摘要 —— 调用方拿它当下一次存的 `expect`（连存两次不自撞） |

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-peek`

读改写的读那一半。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-peek`

| 字段 | 向 | 说明 |
|---|---|---|
| `exists` | ← | `false` ⇒ **确定不存在**（`text` 为 `null`） |
| `path` | ← | 读的是哪一份（最后一段是链接时是解到底的那一份） |
| `rel` | → | 与写面其余几条同形；**与 `files-put` 同一道围栏**（读的那一份就是写的那一份） |
| `root` | → | 与写面其余几条同形；**与 `files-put` 同一道围栏**（读的那一份就是写的那一份） |
| `text` | ← | 全文（UTF-8）；不在时 `null` |

码：`bad_args` · `bad_path` · `io_failed` · `not_text` · `refused` · `too_large`

#### `files-put`

整份替换一份文本文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-put`

| 字段 | 向 | 说明 |
|---|---|---|
| `backup` | → | 可缺席的布尔（缺省否） |
| `bytes` | ← | 新内容的字节数 |
| `changed` | ← | 真的写了吗（新内容与盘上逐字节相同 ⇒ `false`，一个字节不动） |
| `content` | → | 新全文（字符串或 `{"b16":…}`），**必须给** |
| `created` | ← | 这份文件是这一次新建的 |
| `expect` | → | 🔴 **必须给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」 |
| `parents` | → | 可缺席的布尔（缺省否） |
| `path` | ← | 落点 |
| `rel` | → | 同 `files-peek` |
| `root` | → | 同 `files-peek` |

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-delete-session`

删一份历史会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-delete-session`

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ← | 删掉的那一份 |
| `sid` | → | 🔴 **只收 sid**：多给任何一个键 ⇒ `bad_args` |

码：`bad_args` · `io_failed` · `refused`

#### `files-stage-chunk`

存盘的一块进暂存区。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-stage-chunk`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 这一块写进去的字节数 |
| `content` | → | 字符串或 `{"b16":…}`，**至少 1 字节**（空块 ⇒ `bad_args`） |
| `key` | → | 这一次存盘的键：**恰好 32 位小写十六进制**（调用方每次存盘现造一个） |
| `seq` | → | 块号，从 0 起的非负整数 |

码：`bad_args` · `io_failed` · `refused`

#### `files-commit-text`

按块读回、拼起来、覆盖写。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-commit-text`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | → | 拼起来**必须恰好**这么长；最多 8 MiB（`files-read-text` 一趟的天花板：存得回的要读得回来），超了 ⇒ `bad_args` |
| `chunks` | → | 块数：读回 `0..chunks` 这几块 |
| `expect` | → | 🔴 **必须给**，与 `files-write-text` 的 `expect` 同形同义（摘要形 CAS）：盘上那份对不上 ⇒ `stale`，目标一个字节没动（块照样删掉） |
| `key` | → | 与 `files-stage-chunk` 同一个键 |
| `path` | ← | 解到底的那个真路径 |
| `rel` | → | 目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原子地换（权限位沿用；属主 / 硬链接不再保留，见 `files-write-text`） |
| `root` | → | 目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原子地换（权限位沿用；属主 / 硬链接不再保留，见 `files-write-text`） |
| `sha256` | ← | 写进去那份的摘要 |

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-commit-upload`

把暂存区里一份传完的上传件挪进目标。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-commit-upload`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | → | 可缺席（缺席 ＝ 此前那一形） |
| `chunks` | → | 可缺席（缺席 ＝ 此前那一形） |
| `expect` | → | 🔴 **必须给**，恰好 `{"sha256": "<64 位小写十六进制>"}`：传输台上传时对**本机那份整份**算的摘要（`transfer` 帧 `end.sha256`，窗口原样交来） |
| `key` | → | 暂存件的键：**恰好 32 位小写十六进制** |
| `overwrite` | → | 🔴 **必须给**（`true` / `false`），不给默认值 |
| `path` | ← | 落点（父目录解过 symlink 的那一个） |
| `rel` | → | 目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；「会话文件那一问」删了）；`rel` 也收 `{"b16": …}` |
| `root` | → | 目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；「会话文件那一问」删了）；`rel` 也收 `{"b16": …}` |

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-browse`

告诉后端「用户现在在看哪几个目录」。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-browse`

| 字段 | 向 | 说明 |
|---|---|---|
| `added` | ← | 这一趟新挂上几个 |
| `browse_watch_cap` | ← | 上限（今天 64） |
| `dirs` | → | **此刻的整份名单**（数组，每项是字符串或 `{"b16":…}`） |
| `rejected` | ← | 超过上限被**拒掉**几个 |
| `removed` | ← | 这一趟卸掉几个（用户不再看它们了） |
| `watch_error` | ← | 这一趟没挂上的条数 ＋ 第一条原因（`null` ＝ 都挂上了） |
| `watch_failed` | ← | 这一趟没挂上的条数 ＋ 第一条原因（`null` ＝ 都挂上了） |
| `watching` | ← | 此刻**真挂着** watch 的目录数（进程里那一个监听器，跟着名单挂 / 卸） |

码：`bad_args` · `bad_path`

#### `files-index-rebuild`

重建常驻文件名索引：走一遍，做完返回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-index-rebuild`

| 字段 | 向 | 说明 |
|---|---|---|
| `entries` | ← | 这一趟走出来多少条（目录 ＋ 文件 ＋ 符号链接，根自己不算） |
| `path` | → ← | 要走的那个**根** |
| `resident_bytes` | ← | 新那份索引在后端内存里占多少字节（路径总长 ＋ 5×条数：每条 4 字节界桩 ＋ 1 字节类型，**算得出的量**） |
| `skipped_mounts` | ← | 根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在） |
| `truncated` | ← | 撞到条目上限、没走完 ⇒ 这份索引是**不完整**的 |
| `unreadable_dirs` | ← | 这一趟有几个子目录读不进去（权限等） |
| `unreadable_paths` | ← | 那几个子目录（前 20 个，同 `files-index-status` 那一格） |

码：`already_rebuilding` · `bad_path` · `no_home` · `unreadable`

#### `files-grep`

在一个目录底下按内容搜。

收 `args` · 可撤 · CLI：`ccm -- --files-grep`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 这一趟读了几份、多少字节 |
| `files` | ← | 这一趟读了几份、多少字节 |
| `hits` | ← | 每份命中的文件一项：`path` |
| `ignore_ascii_case` | → | 只对 ASCII 段大小写不敏感 |
| `limit` | → ← | 最多回几份命中的文件 |
| `links` | ← | 碰到几条链接 —— **不跟**（它不一定在这棵树里，跟进去就是出界）；根本身是链接 ⇒ 不进去 |
| `needle` | → | 要找的**字节**子串（字符串或 `{"b16":…}`），1 至 256 字节 |
| `path` | → ← | 从哪个目录往下搜（字符串或 `{"b16":…}`；也可以是一份文件）；回送原样那一格 |
| `skipped_binary` | ← | 看着像二进制（前 8 KiB 有 NUL） |
| `skipped_large` | ← | 超过 8 MiB |
| `skipped_mounts` | ← | 挂在底下的别的文件系统 |
| `stopped` | ← | `null` |
| `truncated` | ← | 上界到了没走完：`"hits"`（命中份数到 `limit`）/ `"bytes"`（一趟累计读到 256 MiB）；走完 ⇒ `false` |
| `unreadable` | ← | 读不了的，各跳过几项 |

码：`bad_args` · `bad_path` · `unreadable`

#### `files-find`

在常驻索引里查。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-find`

| 字段 | 向 | 说明 |
|---|---|---|
| `cover_root` | ← | 要搜全这一趟，重走该走哪个根：手上那份盖得住 ⇒ 它的根；否则范围在家目录里 ⇒ 家目录；否则 ⇒ 范围本身（都说不出 ⇒ `null`） |
| `desc` | → ← | `true` ⇒ 倒过来（默认 `false`） |
| `hits` | ← | 这一屏的命中，每条一个对象：`path` · `kind` · `location` · `size` · `mtime_secs` · `mtime_text`（修改时间的短写法，这台本地钟写好）· `marks` |
| `index_age_secs` | ← | 答这一趟用的那份索引，是多久以前建的 |
| `index_missing` | ← | 索引还没建过 ⇒ 几个计数全是 0，而那不是「没搜到」；客户端要自己发 `files-index-rebuild` |
| `index_root` | ← | 手上那份索引的根（没建过 ⇒ `null`） |
| `limit` | → | 这一屏最多回几条 |
| `offset` | → ← | 从第几条命中起回（前面的只数不回） |
| `out_of_index` | ← | 这一趟的范围（`under` 或家目录）不在手上那份索引里 ⇒ 结果只是索引里碰巧有的那一部分 |
| `query` | → | 原样的搜索词（字符串） |
| `scanned` | ← | 这一趟扫了几条（= 索引条目数） |
| `scope` | → | `"under"`（默认，照 `under`）/ `"machine"`（整台机器：范围由这台自己定 —— unix `/`，Windows 家目录那块盘的根；`under` 不看） |
| `seq` | → ← | 这一趟的号（非负整数，可不给） |
| `sort` | → ← | 按哪一列排：`relevance`（默认）· `name` · `location` · `mtime` · `size` |
| `stale` | ← | 该重走了（`index_age_secs > rewalk_interval_secs`） |
| `start` | ← | 这一趟的搜索起点（`location` 相对它算；说不出 ⇒ `null`） |
| `stream` | → | 这个号属于哪一个搜索框（字符串，不给 ⇒ 空串） |
| `total_hits` | ← | 一共命中几条，**不受分页影响** |
| `truncated` | ← | 这一屏之后还有（往下翻：同号、`offset` 加上这一屏的条数） |
| `under` | → | 只搜这个目录**底下**（不含它自己；字符串或 `{"b16":…}`） |

码：`bad_args` · `bad_path` · `bad_query` · `superseded`

#### `files-index-status`

索引的新鲜度 / 条目数 / 常驻字节。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-index-status`

| 字段 | 向 | 说明 |
|---|---|---|
| `age_secs` | ← | 这份索引建好到现在多少秒 |
| `browse_watch_cap` | ← | 最多挂几个 |
| `browse_watches` | ← | 此刻给「用户正在浏览的那几个目录」挂着几个 watch |
| `cold_first_build_secs` | ← | 后端**声明**的冷启动首建大约要几秒（今天 10，出处见 `index.rs::COLD_FIRST_BUILD_SECS`：一台 NVMe 上 `find` 的冷缓存读数取上整，**代理指标、不是实测**） |
| `entries` | ← | 索引里有几条 |
| `index_missing` | ← | 还没建过 |
| `resident_bytes` | ← | 它在后端内存里占多少字节（索引**只在内存里，重启重建**） |
| `rewalk_interval_secs` | ← | 🔴 **后端声明的重走周期**（今天 300） |
| `skipped_mounts` | ← | 根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在） |
| `stale` | ← | `age_secs > rewalk_interval_secs` |
| `truncated` | ← | 上一趟遍历撞到了条目数上限，没走完 |
| `unreadable_dirs` | ← | 上一趟遍历里有几个目录读不进去（权限等） |
| `unreadable_paths` | ← | 那几个读不进去的目录（前 20 个，按遍历先后；字符串或 `{"b16":…}`；根自己不在里面 —— 根读不进去是 `files-index-rebuild` 的 `unreadable`） |

#### `files-ls`

列一个目录的直接子项。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-ls`

| 字段 | 向 | 说明 |
|---|---|---|
| `entries` | ← | 每项一个对象：`path`（原始字节形）· `kind` · `size` · `mtime_secs`（后两个拿不到就**不出这个键**，不填 0）· `mtime_text` · `mtime_full`（跟着 `mtime_secs` 出） |
| `kind` | ← | **闭集四个词**：`dir` / `file` / `symlink` / `other` |
| `limit` | → | 这一趟最多回几条 |
| `link_dir` | ← | 只在 `kind` 是 `symlink` 时出：它指向的是不是目录（跟链接问一次） |
| `link_to` | ← | 只在 `kind` 是 `symlink` 时出：它指向什么 —— `dir` · `file` · `missing`（断了：指向的东西不在 / 读不到） |
| `mtime_full` | ← | 修改时间的完整写法 `YYYY-MM-DD HH:MM:SS`（这台本地钟写好，窗口照抄） |
| `mtime_secs` | ← | Unix 纪元秒 |
| `mtime_text` | ← | 修改时间列里那一格：今天 `HH:MM` · 今年 `MM-DD` · 往年 `YYYY-MM-DD`（这台本地钟写好，窗口照抄） |
| `path` | → | 要列的那个目录 |
| `size` | ← | 字节数 |
| `total` | ← | 目录里一共读到几项（含没回送的；截断时界面写「前 n / total 项」） |
| `truncated` | ← | 目录里的项数多于回送的条数（被 `limit` 截了） |
| `unreadable` | ← | 目录打开了、其中几项读不出来（没有回送、不算进 `truncated`） |

码：`bad_path` · `denied` · `not_dir` · `not_found` · `unreadable`

#### `files-stat`

一个路径的元数据。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-stat`

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` | ← | `dir` · `file` · `symlink` · `other` |
| `link_target` | ← | 路径**本身**是符号链接 ⇒ 它的目标原文（`readlink`，不解不跟；原始字节形：字符串或 `{"b16":…}`）；不是链接 ⇒ `null` |
| `mode` | ← | unix 权限位的低 12 位（十进制数；`420` = `0o644`） |
| `mtime_full` | ← | 修改时间的完整写法 `YYYY-MM-DD HH:MM:SS`（这台本地钟写好；跟着 `mtime_secs` 出） |
| `mtime_secs` | ← | Unix 纪元秒（`mtime_secs` 拿不到就不出这个键） |
| `mtime_text` | ← | 修改时间的短写法（今天 `HH:MM` · 今年 `MM-DD` · 往年带年；跟着 `mtime_secs` 出） |
| `owner` | ← | 属主（跟链接）：用户名；查不到名字 ⇒ uid 的数字串；非 unix ⇒ `null` |
| `path` | → ← | 入方向是要问的那个路径；出方向原样回送（原始字节形） |
| `readonly` | ← | 这个路径此刻是不是只读 |
| `size` | ← | 字节数 |

码：`bad_path` · `unreadable`

#### `files-read-text`

读一份文本进编辑器。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-read-text`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 字节数 |
| `max_bytes` | → | 🔴 **必须给**：编辑上限是**调用方**的（它答的是「这个文本控件打字卡不卡」） |
| `path` | → ← | 要读的那份文件 |
| `sha256` | ← | 交出去的那份字节的 SHA-256（64 位小写十六进制） |
| `text` | ← | 整份内容（合法 UTF-8） |

码：`bad_args` · `bad_path` · `not_text` · `too_large` · `unreadable`

#### `files-read-chunk`

按字节寻址分块读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-read-chunk`

| 字段 | 向 | 说明 |
|---|---|---|
| `content` | ← | 这一块的原始字节，恒为 `{"b16": …}` |
| `eof` | ← | 这一块读到了末尾（`offset` 越过末尾 ⇒ 空块、`eof: true`） |
| `len` | → | 从哪读、读多少；`len` 只收 `1..=READ_CHUNK_MAX_BYTES`（256 KiB），越界 `bad_args`、不夹小 |
| `offset` | → | 从哪读、读多少；`len` 只收 `1..=READ_CHUNK_MAX_BYTES`（256 KiB），越界 `bad_args`、不夹小 |
| `path` | → ← | 一份普通文件（字符串或 `{"b16": …}`；非 UTF-8 名的下载就走这一形，SFTP 库的路径是 `String` 寻址不到） |
| `size` | ← | 此刻整份多大（调用方据此报进度、判读完） |

码：`bad_args` · `bad_path` · `not_text` · `unreadable`

#### `files-size`

算一个目录有多大。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-size`

| 字段 | 向 | 说明 |
|---|---|---|
| `bytes` | ← | 普通文件的**表观大小**之和（与 `files-ls` 的 `size` 同口径，不是占盘块数） |
| `dirs` | ← | 目录数（含顶上那个目录自己） |
| `files` | ← | 普通文件数 |
| `links` | ← | 符号链接数 —— **不跟、不算字节**（它指向的东西不一定在这棵树里） |
| `other` | ← | 设备 / 管道 / 套接字之类 |
| `path` | → ← | 要算的那个路径（字符串或 `{"b16":…}`）；出方向原样回送（原始字节形） |
| `skipped_mounts` | ← | 底下挂着的**别的文件系统**，没走进去的个数（设备号比对） |
| `unreadable_dirs` | ← | 读不进去、跳过的目录数（不中断） |

码：`bad_path` · `unreadable`

#### `files-home`

后端这个用户的 home。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-home`

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ← | 后端这个进程环境里的 home（原始字节形） |

码：`no_home`

### 4.3 账号

#### `quota-read`

这台的额度账。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --quota-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `accounts` | ← | 每个号一条，按 `(agent, account)` 排：`agent` 路由第 1 段（哪一家）· `account` 路由第 2 段（哪个号 |
| `detail` | ← | 只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答），`reason` 那一句不带原话 |
| `earliestReturn` | ← | 被拒 / 超额在兜的号里最早回来的那个 `{account, at}`；没有、或都说不出时刻 ⇒ `null` |
| `now` | ← | 这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）；回包里每个时刻（`at` · `seenAt` · `resetsAt` · `fromResetsAt` · `since`）旁边有一格 `…Text`：出口按这台本地钟写好的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年），界面照抄、不换算 |
| `path` | ← | 那份文件的绝对路径（家推不出来时 `null`） |
| `reason` | ← | 只在 `unreadable` 时有：为什么读不出来；其余 `null` |
| `state` | ← | `"present"`（读得懂）· `"absent"`（还没看到过任何回包）· `"unreadable"`（文件读不出来 / 家推不出来） |
| `unseen` | ← | 账号库里有、额度账上从没出过数的号：`{agent, account, kind, login, subId?}`（几格同下） |
| `usableNow` | ← | 此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜（快满 · 数旧 · 没采样 · 上一窗已过都算） |

#### `quota-probe`

用某个号查一次额度。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --quota-probe`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → ← | 账号库里的号（配置目录末段；账号 0 是 `0`） |
| `agent` | → ← | 路由第 1 段：哪一家 |
| `from` | ← | 恒 `"usage"` |
| `now` | ← | 这台此刻的 unix 秒 |
| `path` | ← | 额度账那份文件的绝对路径 |
| `reason` | ← | 只在 `unreadable` 时有：哪一处读不懂（英文诊断） |
| `state` | ← | `"read"`（读得懂、已记进额度账）· `"unreadable"`（输出对不上：**不猜、不写账**） |
| `windows` | ← | 读到的窗口（形状同 `reading.windows`）；`unreadable` ⇒ `[]` |

码：`bad_args` · `child_timed_out` · `failed` · `io_failed` · `not_found` · `unsupported`

#### `rotation-rules-read`

这台的轮换规则表。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-rules-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `defaultRule` | ← | 默认规则的 id |
| `detail` | ← | 只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答），`reason` 那一句不带原话 |
| `path` | ← | 那份文件的绝对路径（家推不出 ⇒ `null`） |
| `reason` | ← | 只在 `unreadable` 时有 |
| `rules` | ← | 每条一项（默认那条在最前、其余按名字）：`{id, name, rotation, rev, updatedAt, isDefault, users: {live, ended, follow, doing, sids, endedSids}, summary, explain, missing, atLimitApplies}`；`users` 只数此刻生效的是这条的会话（跟随默认的算在默认那条，`follow` 是其中几个；`sids` 活着的、`endedSids` 已结束的；`doing` ＝ 每个 sid 此刻的状态 `{state, needs}`，与主窗口标签页同一判：`state` 是 `working` · `idle` · `needsYou` · `ended`，`needs` 只在 `needsYou` 时有：`approve` · `answer` · `plan` · `unknown`），`missing` ＝ 顺序里这台账号库没有的号，`summary` / `explain` 是后端写好的两句 |
| `state` | ← | `"present"` · `"absent"`（没动过：只有缺省的「默认」一条）· `"unreadable"` |

#### `rotation-rule-save`

新建或整份改一条轮换规则。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-rule-save`

| 字段 | 向 | 说明 |
|---|---|---|
| `dedupe` | → | 可缺席：`true` ⇒ 重名不拒，名后加 ` 2` · ` 3` … 取第一个不重的（复制 · 复制到别的机器） |
| `from` | → | 新建时不给 `rotation`：从哪条规则拷（`"blank"` ＝ 只有起始账号） |
| `id` | → | 改哪条；不给 ＝ 新建 |
| `ifRev` | → | 改之前读到的 `rev`；对不上 ⇒ `{state:"conflict", rev}`、不写 |
| `name` | → | 规则名（1–24 字，这台不重名：去首尾空白、不分大小写） |
| `rotation` | → | 整份 `{order, enabled, when, atLimit?, cap?, stint?, preempt?, fallback?, wait?}` |
| `state` | ← | `"saved"`（带 `rule`，形状同 `rotation-rules-read` 的一项）· `"refused"`（带 `errors: [{cell, code, with?}]`：哪一格 · 短码 `empty` `dup` `tooLong` `range` `time` `same` `overlap` · 重叠时与第几段）· `"conflict"`（带 `rev`：此刻的版本） |

码：`bad_args` · `io_failed` · `no_such_rule`

#### `rotation-rule-rename`

给一条轮换规则改名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-rule-rename`

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → | 哪条 |
| `ifRev` | → | 同 `rotation-rule-save` |
| `name` | → | 新名字 |
| `state` | ← | 同 `rotation-rule-save` |

码：`bad_args` · `io_failed` · `no_such_rule`

#### `rotation-rule-delete`

删轮换规则。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-rule-delete`

| 字段 | 向 | 说明 |
|---|---|---|
| `ids` | → | 要删的规则 id |
| `moved` | ← | 用着它们的会话落到了哪 `{sid: "custom" \| "follow"}` |
| `then` | → | 用着它们的会话怎么办：`"custom"`（照那条拷一份成本会话的，行为不变）· `"follow"`（改跟随默认） |

码：`bad_args` · `io_failed` · `is_default` · `no_such_rule`

#### `rotation-default-set`

设这台的默认规则。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-default-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `defaultRule` | ← | 此刻的默认规则 |
| `followers` | ← | 跟随默认、此刻活着的会话有几个 |
| `rule` | → | 设为默认的那条 |

码：`bad_args` · `io_failed` · `no_such_rule`

#### `rotation-plan`

一份轮换接下来会怎么走 ＋ 草稿逐格校验（都不写）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-plan`

| 字段 | 向 | 说明 |
|---|---|---|
| `detail` | ← | 同 `rotation-rules-read` |
| `effective` | ← | 号 → 窗口键（`5h` · `7d` · `*` ＝ 全部窗口 · 封顶里写过的别的键）→ `{v, layer, below: {v, layer}}`：此刻实际取的上限（`v` 为 `null` ＝ 不封顶）与来自哪一层（`window` 这号这窗口 · `all` 这号全部窗口 · `trigger` 触发 · `none`），`below` ＝ 这一格不算时往下一层取到的（封顶浮层「其余时段 ＝ …」） |
| `errors` | ← | 逐格错 `[{cell, code, with?}]`（形状同 `rotation-rule-save` 的 `refused`）；空 ＝ 没错；草稿有错 ⇒ 只回这一格 |
| `from` | ← | 视窗起（另有 `fromText`）：带 `view` ⇒ 此刻之前那一截的起点；不带 ⇒ ＝ `now` |
| `grid` | ← | 只在带 `view` 时有：刻度 `[{at, atText, label?}]`，按这台本地钟对齐（`6h` 一格 15m · `24h` 1h · `7d` 6h；悬停与键盘按格走），轴上写字的那几格带 `label`（`6h` 每小时 · `24h` 每 3h 写 `HH:MM`；`7d` 每天零点写 `MM-DD`） |
| `head` | ← | 只在带 `view`、问的不是 `machine` 时有：时间轴顶行。`{account, w?, pct?, toTrigger?, est?}`（此刻用的号 · 卡人的窗口与用了多少 % · 触发是 ≥N% 时还差几点 · `est` ＝ 按目前涨法几点用到这号这窗口此刻取的上限 `{at, atText, pct, w}`：只在额度账上这一窗有两次不同的采样、最近 30 分钟在涨时给，按这两点的斜率外推，到之前先重置就不给）；池里此刻都不能用（被拒 · 过封顶 · 时段停用）或预览说停发 ⇒ `{blocked: {account, at, atText, w?}}`（最早回来的号 · 几点 · 哪个窗口重置） |
| `lanes` | ← | 池里每个号一条（按池序；`machine` ⇒ 这台全部号）：`{account, spans: [{from, to, state, n}], resets: [{w, at}], pct, usedBy?, warm?}`；`pct` ＝ 此刻卡人的那个窗口用了多少 %（没出过数 ⇒ `null`）；`usedBy` 只在 `machine` 时有：此刻活着、走这个号的会话数；`warm` ＝ quota-warm 下一次开窗 `[{at, atText}]`（只在带 `view`、读得到它的状态文件且它还在跑时有）；`state` 是不能用的样子 `refused` · `capped`（`n` ＝ 那个上限）· `off`（时段停用）· `overage`；`resets` ＝ 视窗里的重置时刻（`w` ＝ 语义位 `5h` / `7d`，没有 ⇒ 窗口键） |
| `now` | ← | 这台此刻的 unix 秒（另有 `nowText`）；`until` ＝ 视窗止 |
| `past` | ← | 只在 `sid` ＋ `view` 时有：`[{from, to, account, why}]`，这个会话在视窗起到此刻走过哪几个号（照换号记录切段，`why` ＝ 换进那一段的原因，头一段 `null`） |
| `plan` | ← | `[{from, to, account, why}]`：`[from, to)` 用 `account`（`null` ＝ 那一段不发上游：硬上限停着 · 切兜底前等着）；`why` ＝ 那一段开头为什么换（形状同换号记录的 `why`；头一段 · 没换 ⇒ `null`）。用量只按此刻的算（以后涨多快没根据，不预测；单段预算不预测），结论只在重置 · 时段起止时变；`view` 是 `7d` 时只到此刻 +1d；每个时刻旁有 `…Text` |
| `reason` | ← | 同 `rotation-rules-read` |
| `state` | ← | 那份文件的三态（同 `rotation-rules-read`）；`unreadable` 时照缺省那一份算 |
| `machine` | → | `true`：这台全部号（设置里的时间轴），按默认规则判封顶 |
| `rotation` | → | 草稿 `{order, enabled, when, atLimit?, cap?, stint?, preempt?, fallback?, wait?}`（从池里排第一的号起）；与 `rule` · `sid` · `machine` 四选一 |
| `rule` | → | 这台的一条规则 id（从池里排第一的号起） |
| `sid` | → | 一个会话：此刻生效的那一份，从它此刻的号起 |
| `span` | → | 可缺：视窗 `6h` · `12h`（缺省）· `24h` · `7d`（从此刻起；带 `view` 时不看） |
| `view` | → | 可缺：时间轴视窗 `6h`（前 2h · 后 4h）· `24h`（前 6h · 后 18h）· `7d`（前 1d · 后 6d） |

码：`bad_args` · `no_such_rule`

#### `rotation-session-read`

一批会话的轮换与「账号」格。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-session-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `detail` | ← | 同 `rotation-rules-read` |
| `now` | ← | 那份文件的三态（同 `rotation-rules-read`）· 这台此刻的 unix 秒；回包里每个时刻（`at` · `seenAt` · `resetsAt` · `fromResetsAt` · `since`）旁边有一格 `…Text`：出口按这台本地钟写好的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年），界面照抄、不换算 |
| `reason` | ← | 那份文件的三态（同 `rotation-rules-read`）· 这台此刻的 unix 秒 |
| `sessions` | ← | 每个 sid 一份 |
| `sids` | → | 会话 id 的数组 |
| `state` | ← | 那份文件的三态（同 `rotation-rules-read`）· 这台此刻的 unix 秒 |

码：`bad_args` · `failed`

#### `rotation-session-set`

改一批会话的轮换。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-session-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 这台没见过的会话要另给：哪一家 |
| `rotation` | → | `"follow"` · `{"rule": id}` · `"custom"`（恢复本会话上一份，没有就照此刻生效的那份拷）· `"detach"`（照此刻生效的那份拷成本会话的）· `{"custom":{…}}` |
| `sessions` | ← | 逐个结果 `{sid: {state:"done"} \| {state:"skipped", code}}` |
| `sids` | → | 要改的会话 |
| `start` | → | 起它的号 |

码：`bad_args` · `failed` · `io_failed` · `no_such_rule`

#### `rotation-switch`

现在就换。

收 `args` · 可撤 · CLI：`ccm -- --rotation-switch`

| 字段 | 向 | 说明 |
|---|---|---|
| `mode` | → | `hot`（不重启，下一发起就走它）· `restart`（换号重启） |
| `sessions` | → ← | `hot`：会话 id；`restart`：每项是 `session-restart` 的入参（不带 `account`）；应答里是逐个结果 |
| `target` | → | 换到哪个号 |

码：`bad_args` · `failed`

#### `apikey-key-set`

给一个账号写 key，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --apikey-key-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | ← | 推出来的账号 id |
| `baseUrl` | → ← | 入（可选）：这个账号的第三方端点 |
| `configDir` | → | 这个号的账号目录 |
| `key` | → | 明文 |
| `masked` | ← | 写完**再读一遍**、这一行 key 的掩码（盘上的事实） |
| `path` | ← | 那份文件的绝对路径 |

码：`bad_args` · `bad_file` · `io_failed`

#### `apikey-read`

上游选择凭据文件在这台的状态。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --apikey-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `configured` | ← | **顶层那一把**（历史格式那一行）配没配、掩码 |
| `masked` | ← | **顶层那一把**（历史格式那一行）配没配、掩码 |
| `notice` | ← | 权限过宽 / 查不出来时的一句话（文件不在时 `null`） |
| `path` | ← | 那份文件的绝对路径 |
| `problem` | ← | 读不动 / 解析不了时的一句话 |

#### `apikey-routing`

这几个号在这台的表里有没有行 · 这台的中转在不在。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `routed` | ← | 传进来的里面、**表里有对应行**的那几个（原样回） |
| `running` | ← | 这台机器上**我们的**中转在不在听（读常驻后端进程内的监听状态；中转住这里） |

码：`bad_args`

#### `accounts-list`

账号清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `accounts` | ← | 每账号一个对象，字段同 `--list-accounts` 的账号行 |
| `agent` | → | 必填：这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`） |
| `meta` | ← | `{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault, home}` |
| `notice` | ← | 「能用但有缺」：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）时的一句话；否则 `null` |

码：`bad_args` · `too_large`

#### `machine-interrupts`

停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machine-interrupts`

| 字段 | 向 | 说明 |
|---|---|---|
| `appExit` | → | 可缺；`true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零 |
| `forwards` | ← | 见 `machine` |
| `liveStreams` | ← | 这台活着的会话数（停的那几秒 cc-monitor 里它们不更新） |
| `machine` | → | 可缺 |
| `relayedMaybe` | ← | 活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它） |
| `relayedSessions` | ← | 这台的活会话里经本机中转走请求的几个（停了就断，直到再启动） |

码：`bad_args`

#### `accounts-sessions`

正在跑的会话各属哪个账号。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-sessions`

| 字段 | 向 | 说明 |
|---|---|---|
| `lines` | ← | 同 `--session-accounts`：每条运行中会话一行 |

码：`too_large`

#### `accounts-trust`

换号前的信任预检。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-trust`

| 字段 | 向 | 说明 |
|---|---|---|
| `configDir` | → | 目标账号的 config dir（必须逐字 ∈ manifest，否则 `unknown_config_dir`）；**缺席或 `null` = 账号 0**（读 agent 自己那份用户级配置，不收路径） |
| `cwd` | → | 要预检的工作目录（必填） |
| `known` | ← | 这个账号的用户级配置里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认） |
| `trusted` | ← | 这个账号接受过该目录的信任对话框 |

码：`bad_args` · `failed` · `manifest_unavailable` · `no_home` · `unknown_config_dir` · `unsafe_config_dir`

#### `accounts-init`

建账号库。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-init`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliasNames` | ← | 这个号会拿到的别名名字（`accounts-init` / `accounts-add`，预演时也给；别的命令 ⇒ `[]`） |
| `aliases` | ← | 改配置文件（`profiles.toml`）的结局，恰一条 `{path, changed, added, removed, skipped, note}`：加了（基于 `cc` / `cct`、只写自己的号）/ 删了（合下来用这个号的全部段）/ 名字被占跳过的那几段；配置文件有写错的地方 ⇒ 不动、`note` 说一句 |
| `applied` | ← | 这一趟真改了盘没有 |
| `backup` | ← | 这一趟留的备份（`~/.cc-monitor/accounts/.backup-<这一段>`，回滚用它）；没改动 ⇒ `null` |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `name` | → | 默认号的名字：过 `shell_quote_core::account_name_ok`（与 `ccm … --account` 同一条），`0` 是保留名 |
| `notes` | ← | 提示（不挡这一趟），比如共享库里还没有可共享的项 |
| `steps` | ← | 做了（预演时：将要做）的每一步，一句一行 |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-add`

新建一个号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-add`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | ← | 建出来的那个号：`{name, configDir}` |
| `aliasNames` | ← | 同 `accounts-init` |
| `aliases` | ← | 同 `accounts-init` |
| `applied` | ← | 同 `accounts-init` |
| `backup` | ← | 同 `accounts-init` |
| `baseUrl` | → | 只 API 号：上游地址（缺席 = 默认上游）与 key 明文 |
| `configDir` | ← | 那个号的配置目录（`account` 里） |
| `credFile` | → | 只订阅号：导入哪一份凭据（家目录底下的绝对路径或 `~/…`；是链接 / 空文件 / 不在 ⇒ `refused`） |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `isDefault` | → | 可缺席的布尔：真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它） |
| `key` | → | 只 API 号：上游地址（缺席 = 默认上游）与 key 明文 |
| `keyMasked` | ← | API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填） |
| `keyProblem` | ← | API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填） |
| `kind` | → | `"subscription"`（订阅号）或 `"api-key"`（API 号，清单里写 `authKind: "api-key"`） |
| `loginCmd` | ← | 订阅号没导入凭据时：在终端里跑这一行登录（`'<家>/.cc-monitor/bin/ccm' -- --account '<名>'`，agent 自己的登录界面）；否则 `null` |
| `name` | → | 同 `accounts-init`；已有同名号 / 同名目录 ⇒ `refused` |
| `notes` | ← | 同 `accounts-init` |
| `steps` | ← | 同 `accounts-init` |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-remove`

删一个号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-remove`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |
| `applied` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |
| `backup` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `force` | → | 可缺席的布尔：删的是默认号时必须给真（剩下的第一个号接着当默认） |
| `name` | → | 要删的号；`0` · 不认识的号 ⇒ `refused` |
| `notes` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |
| `steps` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-set-default`

设默认号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-set-default`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |
| `applied` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |
| `backup` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `name` | → | 要当默认的号（不认识 ⇒ `refused`） |
| `notes` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |
| `steps` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-repair`

修复账号库（幂等）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-repair`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |
| `applied` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |
| `backup` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `notes` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |
| `steps` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-rollback`

按一份备份还原。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-rollback`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | ← | 同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名 |
| `applied` | ← | 同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名 |
| `backup` | → ← | 入：用哪一份（`.backup-` 后面那一段，只许 `[0-9A-Za-z._-]`、不含 `..`）；缺席 ⇒ 最近一份还没还原过的 |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `notes` | ← | 同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名 |
| `steps` | ← | 同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名 |

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-verify`

核对账号库。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-verify`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | ← | 说的是哪个号；全局那几条 ⇒ `null` |
| `checks` | ← | 每条 `{level, account, text}` |
| `fails` | ← | `fail` |
| `level` | ← | `ok` · `warn` · `fail` · `skip` |
| `pass` | ← | 没有一条 `fail` |
| `text` | ← | 给人看的那一句 |
| `warns` | ← | `warn` 各几条 |

码：`bad_args` · `io_failed` · `unsupported`

#### `accounts-login-cmd`

在终端里登录一个号的那一行。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-login-cmd`

| 字段 | 向 | 说明 |
|---|---|---|
| `cmd` | ← | 那一行：这台的 `ccm` 带 `--account` 起 agent（它自己的登录界面）；值一律经唯一的 quote（`shell_quote_core::posix_quote`） |
| `name` | → | 清单里的一个号（不认识 ⇒ `refused`） |

码：`bad_args` · `io_failed` · `refused` · `unsupported`

#### `accounts-mcp-read`

这台各账号共用的用户级 MCP 此刻的样子。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 这一趟改写了哪几个号（这一条恒空） |
| `choices` | ← | 每一版 `{from, holders, gone}` |
| `conflicts` | ← | 两边都改了、等用户挑的那几条：每条 `{name, choices}` |
| `enabled` | ← | 这台有没有账号库（没有 ⇒ 不做同步，其余几格为空） |
| `from` | ← | 挑这一版时交回 `accounts-mcp-pick` 的 `from`：`null` = 共享的那一版；号名 = 那个号里的那一版 |
| `gone` | ← | 这一版是「没有这一条」（在 cc-monitor 里删过） |
| `holders` | ← | 此刻是这一版的那几个号 |
| `name` | ← | 那一条的名字 |
| `notes` | ← | 提示：某个号的配置读不出来（这一趟不同步它）· 没写进去 |
| `servers` | ← | 共享集合里的名字（排好序） |
| `sync` | ← | 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的、`conflicts` 恒空；见 `accounts-mcp-sync`） |

码：`bad_args` · `io_failed` · `refused`

#### `accounts-mcp-remove`

从各账号共用的用户级 MCP 里删一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-remove`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `choices` | ← | 同 `accounts-mcp-read` |
| `conflicts` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `enabled` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `from` | ← | 同 `accounts-mcp-read` |
| `gone` | ← | 同 `accounts-mcp-read` |
| `holders` | ← | 同 `accounts-mcp-read` |
| `name` | → | 要删的那一条（共享集合里与哪个号里都没有 ⇒ `not_found`） |
| `notes` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `servers` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `sync` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |

码：`bad_args` · `io_failed` · `not_found` · `refused`

#### `accounts-mcp-pick`

两边都改了的那一条用哪一版。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-pick`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `choices` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `conflicts` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `enabled` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `from` | → | → 那个号里此刻的那一版；缺席 / `null` = 共享的那一版（共享集合里已经删了 ⇒ 删） |
| `gone` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `holders` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `name` | → | 那一条 |
| `notes` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `servers` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |
| `sync` | ← | 同 `accounts-mcp-read`；停着同步时拒（`refused`） |

码：`bad_args` · `io_failed` · `not_found` · `refused`

#### `accounts-mcp-sync`

停 / 开各账号之间同步用户级 MCP。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-sync`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |
| `choices` | ← | 同 `accounts-mcp-read` |
| `conflicts` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |
| `enabled` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |
| `from` | ← | 同 `accounts-mcp-read` |
| `gone` | ← | 同 `accounts-mcp-read` |
| `holders` | ← | 同 `accounts-mcp-read` |
| `name` | ← | 同 `accounts-mcp-read` |
| `notes` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |
| `on` | → | `false` = 停；`true` = 开回来 |
| `servers` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |
| `sync` | ← | 同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号 |

码：`bad_args` · `io_failed` · `refused`

### 4.4 历史会话与它的读面

#### `history-annotate`

改一条历史注解。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-annotate`

| 字段 | 向 | 说明 |
|---|---|---|
| `customTitle` | ← | 自定义标题（`null` = 没改过名） |
| `entry` | ← | 改完的那一条：`starred` · `customTitle`（`null` = 没改过名）· `hidden` · `updatedAt`（毫秒，= 这一次） |
| `hidden` | ← | 隐藏 |
| `patch` | → | 要改的那几格：`starred` / `customTitle` / `hidden`（`customTitle` 也认蛇形 `custom_title`） |
| `sid` | → | 会话 id |
| `starred` | → ← | 星标 |
| `updatedAt` | ← | 毫秒，= 这一次 |

码：`annotations_unreadable` · `bad_args` · `io_failed` · `no_annotations`

#### `history-forget`

删一条历史注解。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-forget`

| 字段 | 向 | 说明 |
|---|---|---|
| `removed` | ← | 真删了一条没有（`false` = 本来就没有这一条，文件没动） |
| `sid` | → | 会话 id |

码：`annotations_unreadable` · `bad_args` · `io_failed` · `no_annotations`

#### `history-last-accounts`

sid → 上次用哪个号起。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-last-accounts`

| 字段 | 向 | 说明 |
|---|---|---|
| `accounts` | ← | `{sid: 账号名}` |

码：`unreadable`

#### `history-list`

历史页的平铺会话清单。

收 `args` · 可撤 · CLI：`ccm -- --history-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `fresh` | → | 可缺席：`true` ⇒ 远端那一台不用记着的、再问一次（开页 · 「刷新」） |
| `groups` | ← | 按项目看时的分组（只数 `rows` 里不是 `context` 的）：`key` · `agent` · `projectName` · `projectPath` · `projectDir` · `count` · `hasLive`（`null` = 有判不了活的、又没有确定在跑的）· `starred`（组里有星标的）· `lastActivity` · `order`（几台的组并成一列时的序，大的在前：档位 × 10¹⁴ ＋ 有星标 × 10¹³ ＋ 最后动过的毫秒；界面只按它并）· `failed`（读不了的那个记录目录 ⇒ 一组、`count` 0、带那一句；别的 ⇒ `null`）· `origin` |
| `hidden` | → | 可缺席：`true` ⇒ 隐藏的也出（默认不出） |
| `limit` | → | 可缺席：最多回几行（默认 2000，1–20000）；多出的不回、`truncated` |
| `notice` | ← | 注解没并上的那句话；`null` = 并上了 |
| `origin` | → | 可缺席：那台的名字（可达表的键） |
| `query` | → | 可缺席：只留显示标题（`label`）· 第一句 · 项目名里含这几个字的（不分大小写，子串；不比路径、不搜内容 —— 内容走 `history-search`） |
| `raw` | → | 可缺席：`true` ⇒ 只回**这台自己**的清单 `{rows, failed}`（不并注解、不筛不排、不认别的入参）—— 远端那一支问的就是它 |
| `rows` | ← | 每会话一行，按 `at` 倒序：`agent` · `agentTag`（行上那一家的小牌，对用户的叫法）· `atText`（行尾那一格）· `sectionText`（分段头）· `spanText`（内容头那一段）—— 这三格按这台本地钟写好，界面照抄 |
| `sort` | → | 可缺席：`activity`（默认，按最后活动）· `created`（按开始） |
| `total` | ← | 筛完留下几个（截之前，不含 `context`） |
| `truncated` | ← | `rows` 被 `limit` 截过 |
| `within_days` | → | 可缺席：只留那个键（同 `sort`）落在最近 N 天里的（1–3650） |

码：`bad_args` · `failed` · `unreachable`

#### `history-search`

全文搜索。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-search`

| 字段 | 向 | 说明 |
|---|---|---|
| `after_ms` | → | 可选，= `--after-ms` |
| `include_tools` | → | 可选布尔，= `--include-tools` |
| `limit` | → | 可选，= `--limit` |
| `lines` | ← | 每命中会话一行 `SessionHits`（带 `agent`：只扫记录树 ⇒ 记录树那一家），形状与行序同 `--search` |
| `query` | → | 搜索词（必填） |
| `scope` | → | 可选，`user`（人说的）/ `assistant`（那一家说的）/ `report`（agent 回报：子 agent 交回 / 发来的话 · 另一个会话发来的话），= `--scope` |
| `skipped` | ← | 内容搜索不覆盖、这台上又有它的会话记录的那几家（对用户的叫法）：它们的会话不在结果里 |
| `titles` | → | 可选布尔：只比会话标题与第一句（不搜内容）；命中的会话照样一行，`hitCount` 为 `0`、`hits` 空 |
| `unreadable` | ← | 这一趟有几份会话记录读不动、没搜到（权限 / IO 错 / 不是合法 UTF-8） |

码：`bad_args` · `failed` · `too_large`

#### `history-search-merge`

把各台的搜索结果合成一份。

收 `args` · 可撤 · CLI：`ccm -- --history-search-merge`

| 字段 | 向 | 说明 |
|---|---|---|
| `sessionCount` | ← | 会话数 |
| `sessions` | → ← | 各台的会话行（远端的带 `origin`）；回的是合好的、按 `updatedAt` 倒序，每行添 `atText`（行尾那一格）· `spanText`（内容头那一段）：按 `updatedAt`、这台本地钟写好 |
| `totalHits` | ← | `hitCount` 之和 |
| `truncated` | ← | 任一行 `hitsTruncated` |

码：`bad_args`

#### `history-run`

一个子运行的记录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-run`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 读到哪了（最后一个整行之后） |
| `from` | → | 从这个字节起读（缺 ＝ 0）；续读拿上一次的 `end` |
| `more` | ← | 这一页没读到头（再从 `end` 读） |
| `parent` | → | 父会话的记录路径（读会话那道围栏照旧；越界 ⇒ `path_refused`） |
| `path` | ← | 那份子运行记录（不透明，给查看器整份打开用） |
| `rows` | ← | 这一页里每一条认得出的记录：`message` 在渲染模型里的样子（与主会话同一套记录成品）· `rid` 它的对账键（没有 ⇒ 省略） |
| `run` | → ← | 子运行标识（运行表 `session_runs` 里那一格）；与 `tool` 至少给一个，都给以它为准 |
| `tool` | → | 派出它的那次工具调用的 id：后端在父记录里找那次调用的派出链接（适配层 `child_link`）；还没对上（前台子运行跑完才写明是哪一个）⇒ `not_found` |

码：`bad_args` · `failed` · `not_found` · `path_refused` · `refused` · `too_large`

#### `history-record`

这条会话的记录还在不在。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-record`

| 字段 | 向 | 说明 |
|---|---|---|
| `configDir` | → | 可选：这次 resume 要用的**账号配置目录** |
| `present` | ← | 这条会话的记录文件在那棵记录树里找得到（根那一层或项目目录那一层 |
| `root` | ← | 查的那棵记录树的根（报错时说清查了什么） |
| `sid` | → | 会话 id |

码：`bad_args`

#### `history-lines`

按行号取回一段。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-lines`

| 字段 | 向 | 说明 |
|---|---|---|
| `eof` | ← | 读到了最后一个完整行之后 |
| `from` | → ← | 第一行的行号（缺省 0） |
| `lines` | ← | **记录行**（形状同 `history-page` 的 `lines`）：`[from, next)` 里进界面的那些，第 k 个可计行的行号是 `from + k`（不进界面的照占号、不出现） |
| `next` | ← | 下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数） |
| `path` | → | jsonl 路径，围栏同 `history-read`（越界 ⇒ `refused`） |
| `summaryOnly` | → | 只要**折起那一行的成品**：每条的 `message` 剥掉正文那几格（`message.content` —— 正文 · 思考 · 工具入参 · 工具结果；`cc-monitor-unrecognized` 的 `raw`；`queue-operation` 的 `content`），折起那一行要用的那几格照给（`timeText` · `userText` · `toolSteps` · `toolCards` · `toolResults` · 链上身份）。缺省 `false` ＝ 给全文（今天的行为） |
| `until` | → | 可选右端（半开区间 `[from, until)`）；缺 ＝ 到最后一个完整行为止 |

码：`bad_args` · `failed` · `oversized_line` · `refused`

#### `history-read`

按字节分页读一份会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `eof` | ← | 区间到头了（`until` 或读时的文件长度） |
| `next` | ← | 下一页从这里起（= `offset` ＋ 这一页的原始字节数） |
| `offset` | → | 从这个字节起（缺省 0） |
| `path` | → | jsonl 路径，围栏同 `--read-session`（越界 ⇒ `refused`） |
| `rows` | ← | 这一页里每个**可计行**一条（空白 / 纯 BOM 行不占）：`end` ＝ 这一行（含 `\n`）之后那个字节的偏移（原始字节，永远说得准 |
| `summaryOnly` | → | 只要**折起那一行的成品**：每条的 `message` 剥掉正文那几格（`message.content` —— 正文 · 思考 · 工具入参 · 工具结果；`cc-monitor-unrecognized` 的 `raw`；`queue-operation` 的 `content`），折起那一行要用的那几格照给（`timeText` · `userText` · `toolSteps` · `toolCards` · `toolResults` · 链上身份）。缺省 `false` ＝ 给全文（今天的行为） |
| `until` | → | 可选右端（半开区间 `[offset, until)`），= `--until` |

码：`bad_args` · `failed` · `oversized_line` · `refused`

#### `history-page`

按字节分页读，出记录行。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-page`

| 字段 | 向 | 说明 |
|---|---|---|
| `eof` | ← | 同 `history-read` |
| `lines` | ← | 只装**进界面**的记录行：`session_id` · `path` · `seq`（第 k 个可计行 ＝ `seq + k`）· `cwd` · `message`（`JsonlRecord`） |
| `next` | ← | 同 `history-read` |
| `nextSeq` | ← | 下一页第一行的行号 |
| `offset` | → | 同 `history-read` |
| `path` | → | 同 `history-read` |
| `seq` | → | `offset` 那一行的行号（缺省 0）；续页交上一页的 `nextSeq` |
| `summaryOnly` | → | 同 `history-read` |
| `until` | → | 同 `history-read` |
| `whole` | → | 这是「整份读进查看器」那一件：读过 256 MiB 就明拒 `too_large`（那句话说读到了哪；不许静默截断，F06） |

码：`bad_args` · `failed` · `oversized_line` · `refused` · `too_large`

#### `history-turns`

一轮的摘要。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-turns`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 最后一个完整行的末字节（残尾不计） |
| `from` | → ← | 可选，缺 ⇒ 0：从这个字节起扫 |
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `turns` | ← | 这一段里的每一轮，文件序；起止（`start` · `end`）旁边各有一格 `startText` · `endText`：这台本地钟的 `HH:MM`（界面照抄、不换算；解不出 ⇒ 空串） |

码：`bad_args` · `failed` · `too_large`

#### `history-facts`

会话事实。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-facts`

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | ← | 这份记录是哪一家的（线上的 kind，适配层按记录认）；认不出 ⇒ `null` |
| `cost` | ← | 全会话花费（记录里那一家自己记的那一条，最后一条为准）`{micros, partial, text}`（`text` 写好）；记录里没有 ⇒ `null`（不按定价自己算） |
| `end` | ← | 最后一个完整行的末字节 |
| `forkedFrom` | ← | 源会话 sid：首条带 `forkedFrom`（`sessionId` 与 `messageUuid` 都是串）的 user / assistant 记录；不是分叉来的 ⇒ `null` |
| `handedBack` | ← | 交回了的子运行 id（按「谁说的」认），去重、文件序 |
| `lastSay` | ← | 最后一段正文的头一行 `{text, at}`；没有 ⇒ `null` |
| `limits` | → | 可选：设置里的上下文上限表 `{<模型名子串>: 正整数}`（最长匹配的子串胜）；缺 / `null` ⇒ 空表；形状不对 ⇒ `bad_args` |
| `needs` | ← | 那台说在等人 ⇒ `{kind, tool, call, what, sinceMs}`（`kind`：approve 批准 · answer 回答 · plan 批准计划 · network 放行联网 · worker 批准协作请求 · goal 确认会话目标 · choose 在对话框里选 · unknown 判不出）；不在等 ⇒ `null` |
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `pending` | ← | 还没结果的工具调用 `{id, name, what, at, state, why}`：`state` 在跑 running · 在等你 awaiting · 状态不明 unclear（每次现判）；`why` 只在 unclear 时给：noWriter（没有活进程持着这条会话）· untracked（这一家不留 pidfile，判不了活） |
| `permissionMode` | ← | 此刻的许可档（最后一条许可档记录写的那一档，原样）；没有 ⇒ `null` |
| `prior` | → | 可选：**上一次应答的 `data` 原样**（续传令牌） |
| `projectDir` | ← | 会话起在哪个目录（记录开头）；还没读到 ⇒ `null` |
| `retries` | ← | 一串相邻的 API 重试按首条的 `uuid` 记一件 `{id, outcome}`：retrying（还没下文）· recovered（后面来了正常回复）· failed（来了报错那条）· interrupted（人发了一句 / 打断）；文件序，至多 200 件 |
| `tokens` | ← | 全会话用量（按请求去重）`{input, output, cacheRead, cacheWrite5m, cacheWrite1h, requests, text, last}`（写缓存分 5 分钟 / 1 小时两档；`text` 写好）；一条带用量的回复都没有 ⇒ `null` |
| `touchedFiles` | ← | 写类工具（Edit / Write / MultiEdit → `file_path`，NotebookEdit → `notebook_path`）碰过的文件，原样、去重、近因序（最近碰的在末尾），至多 1000 条（超 ⇒ 丢最久没碰的） |
| `usage` | ← | 文件序最后一条 `input_tokens + cache_creation_input_tokens + cache_read_input_tokens > 0` 的 assistant 记录 ⇒ `{promptTokens, model, peakPromptTokens, limit, limitFrom}`（`model` 缺 ⇒ `null`）；一条都没有 ⇒ `null` |
| `writers` | ← | 此刻持着这条会话的活进程 pid（这台的 pidfile），升序；不留 pidfile 的那一家恒空 |

码：`bad_args` · `failed` · `too_large`

#### `history-branch`

主线外清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-branch`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说） |
| `off` | ← | 回退掉的那几条记录的 `id`（只含进界面的；文件序）；这一家的记录没有链 ⇒ 恒空 |
| `path` | → | jsonl 路径（围栏同 `history-read`） |

码：`bad_args` · `failed` · `too_large`

#### `history-find`

会话内查找。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-find`

| 字段 | 向 | 说明 |
|---|---|---|
| `hits` | ← | 命中，每条 `{uuid, kind, before, matched, after, turn, tsMs, tsText}`（与 `--find-in-session` 的中段逐行相同）；`tsText` ＝ 那条的时刻按这台本地钟写好（今天 `HH:MM` · 昨天 · 更早带日期；读不出 ⇒ 空串） |
| `include_tools` | → | 可选，缺省 `false`：工具结果也搜 |
| `limit` | → | 可选，缺省 500、封顶 2000（与 CLI 的 `--limit` 同一对常量） |
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `query` | → | 查询串（原样；以 `--` 起头也照样是查询，不是选项） |
| `total` | ← | 全量命中数（≥ 条数；大于 ⇒ 被上限砍过） |

码：`bad_args` · `failed` · `too_large`

#### `history-index`

会话骨架索引。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-index`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 最后一个完整行的末字节 ＝ 下一次续传该带的 `offset` |
| `from` | ← | 起点字节 |
| `offset` | → | 从哪个字节起（缺省 0；续传带上次尾行的 `end`） |
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `rows` | ← | 每个可计行一条 `IndexRow`（见第 6 节） |
| `until` | → | 可选：只收起点 `< until` 的行 |

码：`bad_args` · `failed` · `too_large`

#### `history-user-inputs`

「你说过的话」清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-user-inputs`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 最后一个完整行的末字节 ＝ 下一次增量该带的 `from` |
| `entries` | ← | 每条用户输入 `{uuid, timestamp, excerpt}`（对话序；与 `--list-user-inputs` 的中段逐行相同） |
| `from` | → ← | 增量起点（缺省 0；传上次尾行的 `end`） |
| `path` | → | jsonl 路径（围栏同 `history-read`） |

码：`bad_args` · `failed` · `too_large`

#### `history-tail`

一份会话的尾段从哪个字节起。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-tail`

| 字段 | 向 | 说明 |
|---|---|---|
| `end` | ← | 最后一个完整行之后的字节位置 |
| `n` | → | 要最新几行 |
| `path` | → | jsonl 路径（路径围栏同 `history-read`） |
| `split_at` | ← | 尾段第一行的字节起点 |
| `tail_from` | ← | 尾段第一行的行号 |
| `total` | ← | 可计行总数（口径同 `--read-session-tail` 的 `snapshot_meta`） |

码：`bad_args` · `failed`

#### `session-fork`

从某条消息处分叉出一个新会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-fork`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | ← | `process`（源会话进程此刻的配置目录，同 `accounts-sessions`：没设 ＝ 账号 0，设了 ＝ 账号库里认得那个目录的号）\|`exited`（源会话已退出）· `live_no_account`（活着，但说不出号：进程名单里没有它 · 那个目录账号库不认得 · 环境这一刻读不出） |
| `cwd` | ← | `record`\|`no_cwd`（记录里没有） |
| `from` | ← | `known` 时值的来源 |
| `host` | ← | `terminal` 那一格的终端宿主 |
| `jsonlPath` | ← | 新会话的 id 与落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖） |
| `kind` | ← | `launch` 三格每格的状态：`known` · `unknown` |
| `launch` | ← | 起分叉出来的新会话要的三格：`cwd` · `account` · `terminal`，每格 `{kind:"known", value, from}` 或 `{kind:"unknown", why}` |
| `sessionId` | ← | 新会话的 id 与落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖） |
| `sid` | → | 源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件，`branch_core::find_session_file`） |
| `terminal` | ← | `terminal_list`（挂着它的 `@ccm_sid`、前台是 agent 的那一个；命中多个取第一个）\|`exited` |
| `uuid` | → | 从哪条消息处分叉 |
| `value` | ← | `known` 时的值 |
| `why` | ← | `unknown` 时为什么说不出 |

码：`bad_args` · `fork_failed`

#### `resync`

手动对齐：重发这条流上的会话状态。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `added` | ← | 这次补宣告 |
| `caught_up` | ← | 在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加） |
| `removed` | ← | 补移除的会话数（各份 watcher 取最大：看的是同一台机器） |
| `retagged` | ← | 这次真写了几处 `@ccm_sid`（值一样的不写） |
| `sid` | → | 可选 |
| `uncancellable` | ← | 这台**当下**的能力事实，与 hello 那两格同一个函数、同形（`[{command, code}]` · `[op]`，后者按字母排、当集合用） |
| `unavailable` | ← | 这台**当下**的能力事实，与 hello 那两格同一个函数、同形（`[{command, code}]` · `[op]`，后者按字母排、当集合用） |
| `watchers` | ← | 几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份） |

码：`bad_args`

#### `tasks-list`

一个会话的任务列表。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --tasks-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `activeForm` | ← | 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串 |
| `blockedBy` | ← | 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务） |
| `blocks` | ← | 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务） |
| `description` | ← | 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串 |
| `id` | ← | 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过） |
| `sid` | → | 会话 id |
| `status` | ← | 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过） |
| `subject` | ← | 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过） |
| `tasks` | ← | **成品**：`<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序（此前是原样对象的 `lines`，字段由 monitor 解） |

码：`bad_args` · `failed` · `too_large`

#### `resolve`

按 `ResumeSpec` 推出恢复命令 `CommandPlan`（与一次性 `--resolve` 同一个函数）。

收 `args` · 可撤 · CLI：`ccm -- --resolve`

码：`bad_request` · `invalid_session_id` · `unsafe_launch_candidate` · `serialize_failed`

### 4.5 资产

#### `footprint-report`

「足迹」由这台后端出整份成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --footprint-report`

| 字段 | 向 | 说明 |
|---|---|---|
| `agent_home` | ← | 解析基准：这台那一家的家目录（展示用） |
| `client` | → | 可选 |
| `home` | ← | 这台的家目录 |
| `rows` | ← | 每个足迹一行（落在哪 · 是谁写的 · 怎么收） |
| `settings_scopes` | ← | 各层设置文件（用户 · 项目 · 本地）的读法与先后 |

码：`bad_args` · `failed`

#### `data-report`

「文件与数据」那一份成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --data-report`

| 字段 | 向 | 说明 |
|---|---|---|
| `client` | → | 同 `footprint-report`（本机那一栏才带） |
| `changedFiles` | ← | cc-monitor 写进你的文件的那几处 `{path, what, undo}`（`undo` ＝ 撤回在哪：设置窗的页 · 栏 · 锚点） |
| `chores` | ← | 「要你动手」里进角标的件数：要做 ＋ 要装 ＋ 要你定，还没做完的 |
| `home` | ← | 这台家目录（显示时 `~` 缩写按它） |
| `own` | ← | cc-monitor 在这台自己家里放的每一样 `{id, path, dir, class, exists, size}`：`id` 闭集同 `~/.cc-monitor/` 下的契约常量；`class` ＝ `truth`（删了会丢）· `cache`（能重建）；目录 `size` 为 `null` |
| `tmux` | ← | 这台有没有 tmux（查不动 ⇒ `null`） |
| `todo` | ← | 「要你动手」各件 `{id, kind, state, name, loc, said, why, steps, diff, copy, whole, wholeCovers, file, go, howUrl, mask, action}`：`kind` 闭集 `must` · `install` · `decide` · `installOptional` · `optional`；`state` 闭集 `todo` · `done` · `expired` · `blocked` · `declined`；`action` 闭集 `copyCommand` · `copySnippet` · `decide` · `locate` · `how` · `installFirst`；`diff` 每行 `{n, op, text}`（`op` ＝ `same` · `del` · `add`，加的那几行 `n` 为 `null`）；`mask` ＝ 显示时要遮住的那把钥匙 |

码：`bad_args` · `failed`

#### `first-run`

首次运行「开始用」三步各自打没打勾。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --first-run`

| 字段 | 向 | 说明 |
|---|---|---|
| `left` | ← | 必做而还没打勾的几步（主窗口状态栏那一枚只数它） |
| `remotes` | → | 机器表里有几台远端（机器表住 monitor 那一侧，问的那一方带上） |
| `skipped` | ← | 「开始用」那一块点过「跳过」（`chores-mark` 的 `skipStart` 写） |
| `steps` | ← | 三步 `{id, done, required}`（`required` = 必做；今天只有 `terminal`），`id` 闭集 `terminal`（让终端认得 ccm 和别名：某份启动文件里有别名块）· `named`（给现在登录的号起名字：启用了多账号）· `remote`（加一台远端：`remotes` > 0） |

码：`bad_args`

#### `last-seen-read`

读离线那台的上次值。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --last-seen-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 哪台（机器名） |
| `accounts` | ← | 上次读成的 `accounts-list` 应答 `{atMs, value}`；没记过 ⇒ `null` |
| `data` | ← | 上次读成的 `data-report` 应答 `{atMs, value}`；没记过 ⇒ `null` |

码：`bad_args` · `io_failed`

#### `last-seen-write`

记下一台这一次读成的那一份（或清掉那台）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --last-seen-write`

| 字段 | 向 | 说明 |
|---|---|---|
| `atMs` | ← | 记下的时刻（毫秒） |
| `forget` | → | `true` ⇒ 清掉这台的上次值（删机器时；不带 `kind` / `value`） |
| `kind` | → | 闭集 `accounts` · `data` |
| `origin` | → | 哪台（机器名） |
| `value` | → | 那一份应答（对象，序列化后 ≤ 256 KiB）；最多记 64 台，超了先丢最久没更新的那台 |

码：`bad_args` · `io_failed` · `too_large`

#### `chores-mark`

记下「要你动手」里的一个选择。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --chores-mark`

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → | `decline` / `undecline` 那一件的 `id`（同 `data-report` 的 `todo[].id`） |
| `op` | → | `decline`（不用了）· `undecline`（还是要做）· `selfPaste`（我自己贴）· `unselfPaste`（改回让 cc-monitor 接上）· `skipStart` / `unskipStart`（首次运行「开始用」那一块跳过 / 撤回） |
| `rc` | → | `selfPaste` 那一份启动文件（绝对路径） |
| `declined` | ← | 改完记着的「不用了」那几件 |
| `selfPaste` | ← | 改完记着的「我自己贴」那份启动文件；没选 ⇒ `null` |
| `startSkipped` | ← | 改完记着的「开始用」跳过没有 |

码：`bad_args` · `io_failed` · `marks_unreadable`

#### `agent-home-check`

那个目录能不能当 agent 家目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --agent-home-check`

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | 要查的目录：绝对路径，或 `~/` 开头（按这台家目录展开） |
| `state` | ← | `ok`（在、是目录、里面有那一家的记录树）· `missing`（不在）· `not_dir`（不是目录）· `no_records`（里面没有记录树） |

码：`bad_args`

#### `mcp-sync-plan`

MCP 资产同步的判定。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-plan`

| 字段 | 向 | 说明 |
|---|---|---|
| `overwrite` | → | 可缺席 |
| `rows` | ← | 每个条目名一行 `{name, state, suspects}` |
| `source` | → | 拷出来的那一份原文（字符串，必给） |
| `take` | → | 可缺席 |
| `target` | → | 要写进去的那一份原文；那份文件不存在 ⇒ `null`（**必给**：缺席不当成不存在） |
| `write` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要写的条目名（排序）：`same` 不写、`new` 写、`differs` 在 `overwrite` 里才写 |

码：`bad_args` · `bad_file` · `needs_consent`

#### `assets-catalog`

资产目录。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --assets-catalog`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 这一趟目录有没有变（变了才写盘） |
| `machines` | ← | 各台一份快照 `{id, label, gen, seenAt, assets}`：`gen` 是那台**自己**的代数（它自己那份变了才 +1）；`seenAt` 是那一代的时刻（那台的钟，只给人看、不参与合并） |
| `path` | ← | 目录文件在这台上的路径 |
| `problems` | ← | 这一趟扫描读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」分开说 |
| `rows` | ← | 别的机器有的每个（`kind`, `name`）一行 `{kind, name, state, from}`：`state` 闭集 `missing`（这台一条同名的都没有）· `differs`（有同名的，摘要都不同）· `same`（有一条摘要相同）；`from` 是别处那几条 `{machine, project, digest, summary}` |
| `self` | ← | 这台机器的 id（第一次记目录时生成，之后不变） |

码：`catalog_unreadable` · `io_failed`

#### `assets-catalog-merge`

把另一台后端的整份目录并进来。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --assets-catalog-merge`

| 字段 | 向 | 说明 |
|---|---|---|
| `catalog` | → | 另一台后端的整份目录，形状就是 `assets-catalog` 的应答（只读它的 `machines`） |
| `changed` | ← | 这一次有没有改动 |
| `machines` | → ← | 各台的快照（同一台取 `gen` 大的那一份整份） |
| `path` | ← | 目录文件的路径 |
| `problems` | ← | 读不出的那几处各一句 |
| `rows` | ← | 并完之后的资产行 |
| `self` | → ← | 这台的机器 id |

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `assets-sync`

本机常驻后端沿池里那条 SSH 同步资产目录。

收 `args` · 可撤 · CLI：`ccm -- --assets-sync`

| 字段 | 向 | 说明 |
|---|---|---|
| `dial` | → | 可缺席 |
| `origin` | → | 可缺席 |
| `reach` | ← | 可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）—— 界面据它把目录里的机器 id 对回 origin |
| `self` | ← | 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>` |
| `synced` | ← | 每一趟一行 `{origin, peer, changed, pushed, error}`：`peer` 是那台目录的 `self` |

码：`bad_args` · `io_failed` · `unreachable`

#### `skill-read`

读来源那台上的一个 skill。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `dir` | ← | 这个 skill 的目录（绝对路径） |
| `files` | ← | 每个普通文件一条 `{path, text, bytes, exec, why}`：`path` 是 skill 里的相对路径（`/` 分段） |
| `name` | → | skill 的目录名（一段：不许分隔符 / `..` / 点开头） |
| `project` | → | 可缺席：给了 ⇒ 那个项目里的 skill（项目目录是这台上的绝对路径）；缺 ⇒ 用户级 |
| `root` | ← | 这台 skill 的根 |
| `skipped` | ← | 没读的那几处（指向目录的链接 / 特殊文件） |

码：`bad_args` · `io_failed` · `not_found` · `too_large`

#### `skill-install-plan`

在要被写的那一台判 skill 装不装得过来。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-plan`

| 字段 | 向 | 说明 |
|---|---|---|
| `base` | ← | 写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>` |
| `dir` | ← | 要写进去的目录 |
| `name` | → | skill 的目录名 |
| `overwrite` | → | 可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名） |
| `prefix` | ← | 写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>` |
| `project` | → | 同 `skill-read`（装到哪一级） |
| `root` | ← | 这台 skill 的根 |
| `rows` | ← | 每个路径一行 `{path, state, suspects, blocked}`：`state` 闭集同 `mcp-sync-plan` |
| `source` | → | `skill-read` 读到的 `[{path, text, exec}]`（`text` 可为 `null` = 装不过去的那一个） |
| `take` | → | 可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名） |
| `target` | ← | 这一趟拷的那几个路径在这台上现有的原文 `[{path, text}]` —— 写的时候当 CAS 期望 |
| `write` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序） |
| `ledger` | ← | 没给 `take` ⇒ `null` |

码：`bad_args` · `bad_file` · `io_failed` · `needs_consent` · `too_large`

#### `skill-install-record`

skill 装记录的写口。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-record`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | `add` 可缺席：缺 ⇒ 目录按 skill 根算；`"home"` ⇒ 目录 = 记录所在那个家目录（装在家目录底下、不在 skill 根下的东西用）；其余值 ⇒ `bad_args` |
| `changed` | ← | 记录变没变（没变不写） |
| `digest` | → | 装进去的那一条的摘要（`mcp-add`；卸时对得上就不用问） |
| `dir` | → ← | `drop` 的入参：记录里那个 skill 目录；应答里是这一条记录的目录 |
| `file` | → | `mcp-add` / `mcp-drop`：配置文件的绝对路径（键） |
| `files` | → | `add`：`{<相对路径>: {digest, created}}` —— `skill-install-plan` 答的 `ledger` 里真写成了的那几个 |
| `name` | → | `add`：skill 的目录名 |
| `op` | → | `add`（装完记）或 `drop`（卸掉 / 已经不在的摘掉）；MCP 那一条：`mcp-add` · `mcp-drop` |
| `paths` | → | `drop`：要摘的相对路径（不在记录里 ⇒ `bad_args`，一个字节不动）；摘到零个 ⇒ 整条记录摘掉 |
| `project` | → | `add` 可缺席：给了 ⇒ 目录按那个项目里的 skill 根算 |
| `remaining` | ← | 这一条还剩几个文件（MCP：那份配置里还记着几条） |

码：`bad_args` · `io_failed` · `ledger_unreadable` · `not_found` · `too_large`

#### `mcp-read`

这台机器的 MCP 列表成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `dirs` | ← | agent 用户级配置里那张项目表的键（排序）：用过的项目目录 |
| `entries` | ← | `{scope, name, server, sourcePath, status, loginIn, seenAt}`：`scope` 闭集 `user` · `local` · `project`；`server` 原样（未知字段不丢） |
| `loginIn` | ← | `status` 是 `needsLogin` 时：在哪几个号里要登录（账号库里的名字，排序；没设账号的那一份不出名字）；别的状态恒空 |
| `name` | ← | server 名 |
| `problems` | ← | 在而读不出 / 不是 JSON 的那几份各一句（「这台没有」与「那份坏了」不合成一句）；不在的静默 |
| `projectDir` | → | 可缺席 / `null` |
| `scope` | ← | `user` · `local` · `project` |
| `seenAt` | ← | `status` 是 `needsLogin` 时：最近一次看到是何时（epoch ms）；别的状态 `null` |
| `server` | ← | 那一条配置原样（未知字段不丢） |
| `sourcePath` | ← | 读自哪一份文件 |
| `status` | ← | 闭集：`needsLogin`（agent 最近一次连它时要登录、还在有效期内）· `disabled`（这个项目里停用了）· `unknown`（配置层判不出：不说连上了） |

码：`bad_args` · `too_large`

#### `mcp-server-put`

增 / 改这台一个项目 `.mcp.json` 里的一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-server-put`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 真写了吗（算出来与盘上逐字节相同 ⇒ `false`，一个字节不动） |
| `name` | → | server 名（空 ⇒ 拒） |
| `path` | ← | 写到了哪（解过链接的那一份） |
| `projectDir` | → | 这台机器上的绝对路径（不含 `..`）；落点恒是 `<它>/.mcp.json`（写面只此一个，agent 自己的配置一个字节不碰） |
| `server` | → | 那一条的配置，原样写进 `mcpServers[name]` |

码：`bad_args` · `bad_path` · `refused`

#### `mcp-server-remove`

删这台一个项目 `.mcp.json` 里的一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-server-remove`

| 字段 | 向 | 说明 |
|---|---|---|
| `changed` | ← | 文件不在 / 那一条不在 ⇒ `false`（一个字节不写、不建文件） |
| `name` | → | 要删的那一条 |
| `path` | ← | 那份文件 |
| `projectDir` | → | 同 `mcp-server-put` |

码：`bad_args` · `bad_path` · `refused`

#### `cc-bus-install-state`

装 cc-bus 到这台之前看一眼。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --cc-bus-install-state`

| 字段 | 向 | 说明 |
|---|---|---|
| `dest` | ← | 落点（`<skills 根>/cc-bus`） |
| `existing` | ← | 落点上已经有东西（要写时先整个改名留作备份） |
| `version` | ← | 内嵌那一份的摘要（只答「相同 / 不同」；枢纽拿它当卡上的记号） |
| `writes` | ← | 与这台二进制带着的那一份逐文件比，内容会变的那几个（缺的 ＋ 不一样的；清单外的文件不算）；空 ⇒ 已是这一版 |

码：`refused`

#### `cc-bus-install`

把这台二进制带着的 cc-bus 装到这台。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --cc-bus-install`

| 字段 | 向 | 说明 |
|---|---|---|
| `backup` | ← | 覆盖前整个目录改名成的那一份（`cc-bus.bak-<秒>`，`null` = 之前没装过） |
| `dest` | ← | 落点（`<skills 根>/cc-bus`，过独立 realpath 围栏） |
| `recordFailed` | ← | 装好了但没记进 skill 装记录时那一句（这一趟装的卸不掉）；装卸账复用 `skill-install-record` 那一份 |
| `written` | ← | 写了的那几个相对路径（全一致 ⇒ 空：一个字节不写、不备份、不记） |

码：`bad_file` · `refused`

#### `ext-hub-preview`

装到几台之前那一张卡：本机后端当枢纽，向各台问完并好。

收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` | → | `skill` / `mcp` |
| `machines` | ← | 勾上的每台一项 `{to, name, card, files, error}`：`card` 同单台那张确认卡（`{kind, name, path, writes, unchanged, suspects, stop, config, slots, tokens}`）；`files` = 会写的文件那一行（MCP：那份配置文件 ＋ 键；skill：目录 ＋ 要写的几个）；那台没拼成 ⇒ `card` 为 `null`、`error` = 那台说的那一句 |
| `name` | → | 名字 |
| `place` | → | 可缺：用户在卡上选的那一处 `{level:"user"}` / `{level:"project", dir}`；对勾上的每台都能装才照它 |
| `place` | ← | 共用的那一处（没有每台都能装的 ⇒ `null`） |
| `places` | ← | 各台能装的各处并起来 `{at, ok, note}`：勾上的每台都能装才 `ok`，否则 `note` 说第一台为什么不行 |
| `slots` | ← | 几张卡的要填格并成一份 `{field, key, kept}`：每格一次，`kept` = 那一格已经有值的几台（名字） |
| `to` | → | 勾上的几台：可达表的键的列表，**`null` = 这台自己**；来源与那台原来那一处照扩展页那张表 |

码：`bad_args` · `missing` · `catalog_unreadable` · `io_failed`

#### `ext-hub-apply`

装到几台，本机后端当枢纽；各台各自结局。

收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `fill` | → | 用户在卡上填的值 `{field: {key: 值}}`（只填一次；每台只交它那张卡要的几格，空的不交 ⇒ 沿用那台已有的）；来源机上的值从不经过这里 |
| `kind` | → | 同 `ext-hub-preview` |
| `machines` | ← | 每台一项 `{to, name, done, error}`：`done` = `{path, changed, note}`；没成 ⇒ `error` = 那台说的那一句（看过之后变了的，那台一个字节不写），一台没成不挡别台 |
| `name` | → | 同 `ext-hub-preview` |
| `place` | → | 卡上那一处（`ext-hub-preview` 回的 `place`）；那一组现算的那一处不是它 ⇒ 各台都 `stale` |
| `to` | → | 同 `ext-hub-preview` |
| `tokens` | → | 每台那张卡上的记号 `{机器键（本机 = 空串）: {source, target}}`：枢纽两头都再看一次，任一头对不上 ⇒ 那台 `stale`、**一个字节不写** |

码：`bad_args` · `missing` · `catalog_unreadable` · `io_failed`

#### `ext-list`

设置「扩展」页那张表。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `machines` | ← | 每台一列 `{key, here, reachable, name, projects}`：`key` = 枢纽认它的键（本机后端自己 = `null` |
| `problems` | ← | 这台扫的时候读不出来的那几份 |
| `rows` | ← | 每个条目一行 `{kind, name, about, detail, new, builtin, note, cells}`，`cells` 与 `machines` 同序 |
| `visit` | → | 可缺席：`true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算 |

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-list-here`

设置「扩展」页那张表的本机那一半（不问可达表）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-list-here`

| 字段 | 向 | 说明 |
|---|---|---|
| `machines` | ← | 恒一列（这台自己）`{key: null, here: true, reachable: true, name, projects}`：裁掉了别的台，于是没有一格要问可达表 |
| `problems` | ← | 同 `ext-list`：这台扫的时候读不出来的那几份 |
| `rows` | ← | 同 `ext-list` 每个条目一行，`cells` 与 `machines` 同序 ⇒ 恒一格 |
| `visit` | → | 同 `ext-list`：`true` = 这一问算「来看了一次」 |

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-note-set`

写 / 改 / 清一个扩展的备注。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-note-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` | → | `skill` / `mcp` |
| `name` | → | 名字 |
| `note` | ← | 现在生效的那一份（清掉了 ⇒ `null`） |
| `text` | → | 备注正文（首尾空白去掉；空串 = 清掉；最长 2000 字，超了拒、不截断） |

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-uninstall-preview`

从这台卸一个扩展之前那张卡。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-uninstall-preview`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 在这台哪一级（用户级 MCP：这台有账号库 ⇒ 从各账号共用的那一份里删、所有号一起撤；没有 ⇒ 只读、`refused`） |
| `backup` | ← | 不是 cc-monitor 装的：删之前先放到哪（`~/.cc-monitor/backups/`）；否则 `null` |
| `files` | ← | 要删的那几个（skill：相对路径；MCP：那一条） |
| `kind` | → | 种类 |
| `name` | → | 名字 |
| `path` | ← | skill 目录 / MCP 配置文件 |
| `recorded` | ← | 装记录里有（cc-monitor 装的）⇒ 只撤装时写进去的；没有 ⇒ 不是 cc-monitor 装的 |
| `said` | ← | 这一趟会做什么（说人话：改过没有 · 删了回不回得去） |
| `token` | ← | 看到的那一份的记号 —— 卸的时候原样交回 |

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `not_found` · `refused`

#### `ext-uninstall-apply`

从这台卸一个扩展。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-uninstall-apply`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 同 `ext-uninstall-preview` |
| `changed` | ← | 删了的那几个 |
| `kind` | → | 同 `ext-uninstall-preview` |
| `name` | → | 同 `ext-uninstall-preview` |
| `note` | ← | 要知道的一件（挪 / 抄到了哪 · 没从装记录里摘掉 · 空目录没收掉） |
| `path` | ← | 卸的是哪 |
| `token` | → | 卡上那一份：现在对不上 ⇒ `stale`、一个字节不动 |

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `needs_consent` · `not_found` · `refused` · `stale`

#### `mcp-sync-source`

装到别的机器时来源那一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-source`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 在这台上哪一级：`{level:"project", dir}`（`<dir>/.mcp.json`）或 `{level:"user"}`（agent 自己那份用户级配置，只读） |
| `def` | ← | 那一条；**`env` / `headers` 的值在这台就换成 `null`**（值不出来源机，只交键名） |
| `field` | ← | 空位在哪一格：`env` · `headers` |
| `key` | ← | 空位的键名 |
| `name` | → | server 名 |
| `path` | ← | 读的是哪一份 |
| `slots` | ← | 换成空位的那几格 `[{field, key}]` |
| `token` | ← | 按原样那一条（含密钥值）算的记号：它变了（连只改了一个密钥值也算）⇒ 应用时判 `stale` |

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `missing` · `refused`

#### `mcp-sync-preview`

在要被写的那一台看装上之后那一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-preview`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 同 `mcp-sync-source`；`at` 只收项目那一级（用户级只读 ⇒ `refused`） |
| `def` | → | 来源那台交回的那一条，原样；`env` / `headers` 里夹着值 ⇒ `bad_args` |
| `field` | ← | 空位 / 可疑项在哪一格 |
| `key` | ← | 空位的键名 |
| `kept` | ← | 这台那一条原来就有这个键的值（不填就沿用） |
| `kind` | ← | 可疑项的种类（闭集同 `mcp-sync-plan`） |
| `name` | → | 同 `mcp-sync-source`；`at` 只收项目那一级（用户级只读 ⇒ `refused`） |
| `path` | ← | 这台那份的路径 |
| `slots` | ← | 每个空位 `{field, key, kept}`：`kept` = 这台那一条原来就有这个键的值（不填就沿用） |
| `state` | ← | `new`（这台没有这一条）· `same`（除空位外一样）· `differs` |
| `suspects` | ← | 可疑项，每条 `{kind, field, value, there}`，闭集同 `mcp-sync-plan` |
| `target` | ← | 这台那份的记号（不存在 ⇒ `null`）—— 写的时候原样交回 |
| `there` | ← | 它在这台指向的东西在不在 |
| `value` | ← | 可疑的那个值 |

码：`bad_args` · `bad_file` · `bad_path` · `refused`

#### `mcp-sync-apply`

在要被写的那一台把那一条写进去。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-apply`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 同 `mcp-sync-preview` |
| `def` | → | 同 `mcp-sync-preview` |
| `fill` | → | 用户在确认卡上填的值 `{env: {键: 值}, headers: {…}}`；没填的键沿用这台原有的值，两样都没有 ⇒ `needs_input`、一个字节不写 |
| `name` | → | 同 `mcp-sync-preview` |
| `path` | ← | 写到了哪 |
| `recordFailed` | ← | 装记录没记下来时那一句（`null` = 记下了） |
| `target` | → | 看卡时这台那份的记号：这台在那之后变了 ⇒ `stale`，**一个字节不写、不重读重算** |
| `written` | ← | 真写了吗（与原有那一条逐字相同 ⇒ `false`） |

码：`bad_args` · `bad_file` · `bad_path` · `needs_input` · `refused` · `stale`

#### `skill-install-apply`

在要被写的那一台把勾的那几个 skill 文件写进去。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-apply`

| 字段 | 向 | 说明 |
|---|---|---|
| `chmodFailed` | ← | 写成了但执行位没置上的那几个 |
| `dir` | ← | 装到了哪 |
| `name` | → | 同 `skill-read` |
| `overwrite` | → | 同 `skill-install-plan`（`differs` 的必须在 `overwrite` 里点名，否则整趟拒 `needs_consent`） |
| `project` | → | 同 `skill-read` |
| `recordFailed` | ← | 装记录没记下来时那一句（`null` = 记下了）—— 记不下来 ⇒ 这一趟装的卸不掉 |
| `source` | → | 来源那台 `skill-read` 的 `files`，原样（`path` · `text` · `exec`；`text` 为 `null` 的装不过去） |
| `take` | → | 同 `skill-install-plan`（`differs` 的必须在 `overwrite` 里点名，否则整趟拒 `needs_consent`） |
| `target` | → | 看差异时这台 `skill-install-plan` 回的 `target`（这台那几份原文），原样 —— 写时的 CAS 期望 |
| `written` | ← | 真写成了的那几个（按写的顺序） |

码：`bad_args` · `bad_file` · `io_failed` · `needs_consent` · `stale`

#### `hooks-diag`

cc-bus 钩子诊断。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --hooks-diag`

| 字段 | 向 | 说明 |
|---|---|---|
| `command` | ← | 钩子原文（去首尾空白） |
| `diagnosis` | ← | `session_start`（→ `cc-register`）· `stop`（→ `cc-bus-stop-hook`）各一态 ＋ `note`（读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串） |
| `kind` | ← | 一态：`not-installed` · `installed-via-path` · `installed-at-path` · `path-missing` · `unknown` |
| `note` | ← | 读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串 |
| `path` | ← | 钩子点名的路径（原样，环境变量不展开） |
| `session_start` | ← | `SessionStart` 钩子（→ `cc-register`）那一态 |
| `snippet` | ← | 要合并进那份文件的内容：两条钩子直接指向这台 cc-bus 的两个脚本（家目录底下写成相对家目录的形，否则绝对路径），不依赖 `PATH` |
| `source` | ← | 读的是哪份文件：这台后端的 agent 配置根下的设置文件 |
| `stop` | ← | `Stop` 钩子（→ `cc-bus-stop-hook`）那一态 |
| `supported` | ← | 这台跑得了 cc-bus（它要 tmux；这份后端编到的平台没有原生 tmux ⇒ `false`，`snippet` 恒 `null`，界面只说这台不支持自动收信） |

码：`failed` · `too_large`

### 4.6 别名

#### `aliases-read`

启动文件候选（接入那一格）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `home` | ← | 这台的家目录 |
| `otherRc` | ← | `rcPath` 过了围栏之后的绝对路径 |
| `rcCandidates` | ← | 启动文件候选（方言答列哪几份）：每份 `{path, sourced, exists, block, unreadable, policy, blockLines}`（`blockLines` = 把别名块装进这一份会写几行），`block` = 别名块现状 `{present, version, outdated, conflictingFunctions, manualCleanupHint}`（`conflictingFunctions` = 块外自己定义的、与配置文件里某一段同名的函数 `{name, line, wins}`；`wins` = 新开的终端里敲这个名字起的是哪一个：`yours`（你写的）· `list`（清单那条）· `unclear`（说不清）） |
| `rcPath` | → | 人另指的那一份（`null` = 不指）：过围栏（只许落在 home 之内 · 符号链接不许跑出去）后并进候选 |
| `shell` | → | `posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒） |

码：`bad_args` · `refused`

#### `profiles-read`

读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `accounts` | ← | 这台的账号表（具名号，按账号库的顺序；表单「账号」那一格的选项） |
| `binDir` | ← | 链接住的目录（`~/.cc-monitor/bin`） |
| `editedAt` | ← | 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按这台本地钟写好：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年）；没改过 · cc-monitor 没写过 ⇒ `null` |
| `exists` | ← | 配置文件在不在 |
| `fileProblem` | ← | TOML 本身写坏 ⇒ `{line, message}`（这时 `profiles` 为空、不能按条目改）；否则 `null` |
| `fingerprint` | ← | 盘上那份的指纹（不在 ⇒ `null`），存的时候交回 `profiles-write` |
| `home` | ← | 这台的家目录（界面拿它把路径写成 `~/…`） |
| `migrated` | ← | 旧别名清单一次性转进来之后那张说明 `{count, path, skipped}`（「知道了」之后 `null`） |
| `path` | ← | 配置文件的路径 |
| `profiles` | ← | 每段 `{name, from, own: [{key, slot, vals, line}], agent, usable, problem: {line, message} \| null, kind: link/function, functionWhy, functionLine, said, form, accountShape}`：`accountShape` 是「账号那一形」`{account, tmux}`（自己只写了号、可再加 tmux，按合并下来的算；其余 `null`）：`said` 是树里那一行（自己写的几项，「标签 值」）；`form` 是表单回填（没写的格 `null` ＝ 继承）；`problem` 的原话与终端里敲这个名字得到的同一句 |
| `seed` | ← | 配置文件不在时首建那两条的预览（同 `profiles` 一条的形状；在 ⇒ `[]`） |
| `tmux` | ← | 这台有没有 tmux（「在哪起」那一格选不选得了 tmux 看它）；探不出 ⇒ `null` |

码：`refused`

#### `profiles-resolve`

一段合下来的合并表与「等于」那一行（可按未存的表单算）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-resolve`

| 字段 | 向 | 说明 |
|---|---|---|
| `at` | → | 假设在这个目录敲（`~` 打头按这台家目录展开；`null` ＝ 家目录） |
| `chain` | ← | 继承链（父 → 子） |
| `edit` | → | 未存的表单（同 `profiles-read` 一段的 `form`；`null` ＝ 按盘上那份算） |
| `line` | ← | 这台后端算的「等于」那一行（同 `ccm @名 -- --ccm-print`）；算不出 ⇒ `null` |
| `lineError` | ← | 算不出那一行时 ccm 的原话 |
| `name` | → | 哪一段（带 `edit` 时是正在改的那一段原来的名字，新增写表单里的名字） |
| `problem` | ← | 合不下来 ⇒ 那一句（同终端里敲这个名字）；否则 `null` |
| `rows` | ← | 合并表 `[{key, slot, label, vals, said, from, overriddenBy}]`：父 → 子、层内照写的顺序；被后来那一层盖掉的也在，`overriddenBy` 是盖掉它的那一段 |

码：`bad_args` · `refused`

#### `profiles-impact`

这几处改动会让哪几段合下来变（改前改后）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-impact`

| 字段 | 向 | 说明 |
|---|---|---|
| `affected` | ← | 改动直接点名的那几段之外、合下来会变的 `[{name, changes: [{slot, label, before, after}], problem}]`（变得合不下来 ⇒ `problem` 是那一句） |
| `changes` | → | 同 `profiles-write` 的 `changes`（一个字节不写） |

码：`bad_args` · `refused`

#### `profiles-bases`

「基于」下拉能选的几段（选了不成圈）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-bases`

| 字段 | 向 | 说明 |
|---|---|---|
| `bases` | ← | `[{name, from, said, selectable}]`：选了不会绕成圈的那几段 ＋ 自己（`selectable: false`） |
| `name` | → | 正在改 / 新建的那一段的名字 |

码：`bad_args` · `refused`

#### `profiles-write`

按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-write`

| 字段 | 向 | 说明 |
|---|---|---|
| `changes` | → | 依次做的改动：`{op: "set", was, form}`（新增 `was: null`；名字变了 ⇒ 改名，基于它的跟着改）· `{op: "remove", name, children}`（还被基于 ⇒ `children` 必给：`reparent` 改成基于它的父 · `cascade` 一起删）· `{op: "init", seed}`（配置文件不在时建：`seed` ⇒ 带首建那两条）· `{op: "ackMigrated"}`（迁移说明知道了）。改完多出坏处 ⇒ 整批不写、`refused` |
| `fingerprint` | → ← | 入：必给（字符串或 `null`），读回时的指纹；盘上此刻不是那一份 ⇒ `stale`。出：写完那一份的指纹 |
| `reload` | ← | 终端函数那份文件真改了 ⇒ 给人的那一句「已开的终端要重读」；否则 `null` |
| `wrote` | ← | 配置文件 / 终端函数文件 / 链接动没动 |

码：`bad_args` · `refused` · `stale`

#### `aliases-block-render`

别名块预览。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-render`

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 目标文件（方言由它的扩展名定：`.ps1` ⇒ PowerShell） |
| `text` | ← | 往一份空文件里装一次会写成什么（与 `aliases-block-install` 调同一个 `plan_install`） |

码：`bad_args` · `refused`

#### `aliases-block-install`

别名块装进人选的那份启动文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-install`

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 人选的那份启动文件（过围栏；方言由扩展名定，再过方言那一道闸） |

码：`bad_args` · `refused`

#### `aliases-block-remove`

别名块卸掉。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-remove`

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 同 `aliases-block-install` |

码：`bad_args` · `refused`

### 4.7 cc-bus 总线

#### `bus-list`

谁在线 + 各自待读多少。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `agents` | ← | 在线成员，每项 `{id, target, unread, live, ccm_sid}` |
| `ccm_sid` | ← | 那个会话绑的会话 id，没绑 ⇒ `null` |
| `id` | → ← | 总线身份 |
| `live` | ← | 这个地址**今天还在不在** |
| `target` | ← | tmux 地址 |
| `unread` | ← | 待读条数 |

码：`not_installed` · `timed_out` · `failed`

#### `bus-broadcast`

给总线上在线的成员群发一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-broadcast`

| 字段 | 向 | 说明 |
|---|---|---|
| `detail` | ← | 失败那一项的原话 |
| `error` | ← | 失败那一项的码（`bus-send` 那一套） |
| `failed` | ← | 逐个列， `{id, error, detail}`，`error` 是 `bus-send` 那一套码；键刻意不叫 `code` / `message` —— 那一对是整条失败的错误信封 |
| `from` | → | **可选**，同 `bus-send`：以谁的身份发，也用来「不发给自己」 |
| `id` | → ← | 失败那一项的收件人 |
| `liveness_unknown` | ← | 问不到身份空间（全是 `null`），退回发给所有登记的 |
| `sent` | ← | 投出去几条 |
| `skipped_offline` | ← | 因不在线跳过几个 |
| `text` | → | 正文（非空） |

码：`bad_args` · `not_installed` · `timed_out` · `failed` · `bad_id`

#### `bus-kill`

收掉一个总线成员。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-kill`

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → ← | 总线身份 |
| `killed` | ← | 会话真的被杀了 |
| `stale_only` | ← | 身份对不上：只摘掉那条陈旧登记，会话与收件箱都没动 |

码：`bad_args` · `bad_id` · `not_installed` · `timed_out` · `failed`

#### `bus-send`

发一条消息。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-send`

| 字段 | 向 | 说明 |
|---|---|---|
| `from` | → ← | **可选**：自报的发件人；应答里回显实际用的那个 |
| `live` | ← | 那个会话今天活着吗，与 `bus-list` 同一套三态 |
| `registered` | ← | 在总线名单里吗 |
| `sent` | ← | 投出去了 |
| `to` | → ← | 收件人身份 |

码：`bad_args` · `bad_id` · `not_installed` · `rejected` · `timed_out` · `too_long` · `failed`

#### `bus-spawn`

派生一个协作 agent。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-spawn`

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → ← | 新会话的总线身份；从 `cc-spawn` 的回显里认，**认不出就是 `null`** —— 那是「起了，但名字没认出来」，**不是**「没起来」 |
| `said` | ← | `cc-spawn` 的原始回显，给人看 |
| `spawned` | ← | 恒 `true` |

码：`bad_args` · `bad_id` · `not_installed` · `timed_out` · `failed`

#### `bus-state`

总线名单 ＋ spawn 台账一次回全。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-state`

| 字段 | 向 | 说明 |
|---|---|---|
| `agents` | ← | 名册，每项 `{id, target, registered_at, unread, live, ccm_sid}` |
| `ccm_sid` | ← | 后两格与 `bus-list` 同一套：对身份空间对账，「登记 ≠ 在线」 |
| `dir` | ← | 派生时的目录 |
| `id` | → ← | 总线身份 |
| `live` | ← | 三态：`true` / `false` / `null` = 核不了，**不是**「不在」 |
| `registered_at` | ← | 登记时间，cc-bus 原样 |
| `skipped` | ← | 两张表里读不懂的行数 |
| `spawned` | ← | `cc-spawn` 派生过的会话，每项 `{id, dir, spawned_at, task, live}` |
| `spawned_at` | ← | 派生时间 |
| `target` | ← | 登记的 pane 地址 |
| `task` | ← | 余下全部，含 TAB |
| `unread` | ← | 待读条数 |

码：`not_installed` · `timed_out` · `failed`

#### `bus-inbox`

只读看一个 agent 收件箱的尾巴。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-inbox`

| 字段 | 向 | 说明 |
|---|---|---|
| `class` | ← | 消息类别（cc-bus 原样） |
| `from` | ← | 发件人 |
| `id` | → | 必给；交给 `cc-log` 之前先过 `bus_id_ok`，不过 ⇒ `bad_id`、一个进程都不起 |
| `lines` | → | 可缺席，1..=2000，缺省 200 |
| `messages` | ← | 逐行解析，只取 `from` · `ts` · `text` · `class`；`from` 与 `text` 都空的行不算消息 |
| `skipped` | ← | 读不懂的行数 |
| `text` | ← | 正文 |
| `truncated` | ← | 回显超过 4 MiB ⇒ 保尾，`true` |
| `ts` | ← | 时间 |

码：`bad_args` · `bad_id` · `not_installed` · `timed_out` · `failed`

### 4.8 终端与会话

#### `launch-local`

本机起会话那一行 `ccm …`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → | 缺席（不表态：继承；接回那一形）· 同 `launch-render-cli`（`follow` 什么都没选上 ⇒ 也是不表态） |
| `action` | → | `{"kind":"new"}` · `{"kind":"resume","sid":…}` · `{"kind":"attach"}`（接回 `tmuxName` 那个会话，不起 agent） |
| `cmd` | ← | 那一行 `ccm …` |
| `configDir` | ← | 应答 `account` 里：那个号的配置目录 |
| `cwd` | → | 只用来核「新起」那一格的目录在不在 |
| `defaultLauncher` | → | 这一家 agent 的默认启动器（等于它就不吐 `--launcher`） |
| `kind` | → ← | `action` 的种类：`new` · `resume` · `attach`；`account` 的种类：`follow` · `base` · `named` |
| `launcher` | → | 自定义启动命令（空 = 没设） |
| `model` | ← | 应答 `account` 里：用的模型 |
| `name` | ← | `account` 为 `named` 时的号名；应答 `account` 里是实际用的号 |
| `tmuxName` | → | 建进 tmux 时的会话名（界面铸名口铸的，这里不铸）；缺 ⇒ 直路 |

码：`bad_args` · `refused` · `account_unavailable`

#### `launch-render-cli`

远端起会话那一行 `ccm …`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --launch-render-cli`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → | `{"kind":"follow"}` · `{"kind":"base"}` · `{"kind":"named","name":…}`（按名字判）—— 生成的类型 `AccountAsk` |
| `action` | → | `{"kind":"new"}` · `{"kind":"resume","sid":…}` · `{"kind":"attach","name":…}` |
| `ccmSid` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`） |
| `cmd` | ← | 那一行 `ccm …` |
| `configDir` | ← | 应答 `account` 里：那个号的配置目录 |
| `container` | → | `{"kind":"none"}`（直路）· `{"kind":"tmux","name":…,"send_into":bool}`（建会话 / 键进已有 pane） |
| `cwd` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`） |
| `defaultLauncher` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`） |
| `kind` | → ← | `action` / `account` 的种类（同 `launch-local`） |
| `launcher` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`） |
| `model` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`） |
| `models` | → | 可缺：这台的模型偏好表（`{号: 模型}`，用户设置的原值）；判出来的号在表里 ⇒ `--model` 用那一条 |
| `name` | → ← | 号名（同 `launch-local`） |

码：`bad_args` · `refused` · `account_unavailable`

#### `terminal-ssh`

给一台远端开终端要跑的那一串。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-ssh`

| 字段 | 向 | 说明 |
|---|---|---|
| `command` | → ← | 要在那台跑的命令；应答里是那一整行 PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'` |

码：`bad_args` · `bad_jump` · `refused` · `no_ssh_client` · `unobservable`

#### `terminal-processes`

那台报来的终端连接是这台电脑上哪个进程开的。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `addr` | ← | `elsewhere` 时对面那个地址 |
| `chain` | ← | 这台上开着那条连接的进程链（自下而上），每格 `{pid, name, start}` |
| `name` | ← | 进程名 |
| `pid` | ← | 进程号 |
| `start` | ← | 启动时刻（系统原值；拿不到 ⇒ 0） |
| `why` | ← | 对不上时：`mismatch`（经跳板 / 端口转换）· `elsewhere`（不是这台开的）… |

码：`bad_args`

#### `session-terminals`

此刻是哪个终端在显示这个会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `activity` | ← | 最近动静（Unix 秒；不在 tmux 里 ⇒ `null`） |
| `clientAddr` | ← | 对面地址 |
| `clientPort` | ← | 对面端口 |
| `serverAddr` | ← | 本机地址 |
| `serverPort` | ← | 本机端口 |
| `ssh` | ← | 那个终端的 `SSH_CONNECTION` 四段；不是经 ssh 连的 ⇒ `null` |
| `terminals` | ← | 此刻显示它的终端，每格 `{ssh, activity}`，最近动静在前，最多 16 格 |
| `why` | ← | 一个都没有时：`detached` · `no-terminal` · `unreadable` |

码：`bad_args` · `no_such_session` · `failed` · `child_timed_out`

#### `terminals-list`

这台的终端名单（形状与宿主无关；这一版宿主是 tmux）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminals-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | ← | `session` 里：哪一家（没标就缺） |
| `can` | ← | 这个调用方能做什么：`input` · `end`，做不了的写成 `{no: 原因, said: 给人看的那一句}`（抓屏不过身份门、恒可用，不在这里） |
| `client` | → ← | 请求里可选：自报的前端，决定每行的 `can` 与 `started_by.mine`；`started_by` 里：会话上的 `@ccm_client`（没声明 ⇒ `null`） |
| `clients` | ← | 此刻连着它的终端客户端，每项 `{kind, since, last_activity}`；空 ＝ 后台 |
| `complete` | ← | `false` ＝ 名单里有读不懂的行（画「部分」） |
| `cwd` | ← | 当前目录 |
| `end` | ← | 能不能结束（`{no: "not_yours" \| "not_managed" \| "other_windows"}`） |
| `host` | ← | 终端宿主（这一版是 `tmux`） |
| `input` | ← | 输入方式：`shared`（tmux：各端都能打字） |
| `kind` | ← | `clients` 一项：客户端种类（`terminal-window` …） |
| `last_activity` | ← | 最近动静（秒） |
| `mine` | ← | `started_by` 里：这个调用方过不过身份门（与 `can.input` 是不是 `true` 同一个判定；结束还要看 `can.end`） |
| `no` | ← | 做不了的原因 |
| `program` | ← | 前台程序名 |
| `purpose` | ← | 这一版恒为 `normal`（只有这一种） |
| `session` | ← | 里面跑着会话（`@ccm_sid`）时才有：`{sid, agent?}` |
| `sid` | ← | `session` 里：会话 id |
| `since` | ← | `clients` 一项：连上的时刻（秒） |
| `started_by` | ← | 谁起的：`{client, mine}` |
| `state` | ← | `running` · `idle`（没会话、前台是 shell）· `program_exited`（有会话、前台是 shell） |
| `terminal` | ← | 名单里那一行的不透明句柄（前端不拼、不解析；送字 / 抓屏时交回） |
| `terminals` | ← | 终端名单（每行一个终端） |
| `title` | ← | 里面跑着会话、前台不是 shell 时是窗格标题；否则是前台程序名 |
| `tmux_name` | ← | tmux 会话名 |

码：`bad_args` · `unobservable` · `child_timed_out`

#### `terminal-preview`

抓一个终端的一屏（只抓一次，轮询归调用方）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-preview` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `capped` | ← | 要的比上限多、截到了上限 |
| `captured_at` | ← | 抓屏时刻（秒） |
| `captured_at_text` | ← | 抓屏时刻在这台本地钟上的 `HH:MM:SS`（界面照抄、不换算） |
| `color` | → | 要不要颜色；缺省 `true` |
| `cols` | ← | 列数 |
| `cursor` | ← | 光标 `{x, y, visible}` |
| `lines` | ← | 自上而下的行，每行 `{text, spans?}`（往回要的在最前） |
| `rows` | ← | 行数 |
| `screen` | ← | 这一屏的指纹（16 位十六进制）：内容一变就变，送字时带回来 |
| `scrollback` | → | 往回多要几行；缺省 0、上限 2000 |
| `scrollback_lines` | ← | 实际往回给了几行 |
| `sid` | → | 目标：会话 id（与 `terminal` 恰给一个） |
| `spans` | ← | 着色段 `{from, to, fg?, bg?, bold?, dim?, italic?, underline?, inverse?}`；`color:false` ⇒ 不给 |
| `terminal` | → | 目标：名单里的句柄（与 `sid` 恰给一个） |
| `text` | ← | `lines` 一项：那一行的文字 |

码：`bad_target` · `bad_args` · `not_known` · `ambiguous` · `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `terminal-input`

往一个终端送字或送键（过身份门）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-input` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `client` | → | 自报的前端（过「哪个前端的会话」那一维） |
| `enter` | → | `text` 之后补一个回车；缺省 `true` |
| `key` | → | 送键：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down` |
| `result` | ← | `delivered` · `unsure`（不知道送没送到，别重发）· `refused` |
| `said` | ← | `refused` 时给人看的那一句（后端写好） |
| `screen` | ← | `screen_changed` 时带的新指纹 |
| `seen_screen` | → | 送之前看到的那一屏的指纹；画面已经变了 ⇒ 不送、回 `refused` ＋ `screen_changed` |
| `sid` | → | 目标：会话 id（与 `terminal` 恰给一个） |
| `terminal` | → | 目标：名单里的不透明句柄（前端不拼、不解析） |
| `text` | → | 送字：字面字，原样送、不解释成键名；多行按粘贴送（与 `key` 恰给一个） |
| `why` | ← | `refused` 的原因：`not_known` · `ambiguous` · `ended` · `not_yours` · `not_managed` · `screen_changed` |

码：`bad_target` · `bad_args` · `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `terminal-follow`

订阅一个终端的画面（有变化推一整屏，一帧在途等回执；订着时那台 tmux 里多一个只读客户端，用户自己配的 client-attached / client-detached 钩子会被它触发）。

收 `args` · 连接内就地做完 · 只在流上 · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `live` | ← | 失败时：实时那一格落在哪 —— `snapshot_only`（这台只能快照：没装 tmux · tmux 低于 3.2）· `stopped`（别的） |
| `sid` | → | 目标：会话 id（与 `terminal` 恰给一个） |
| `terminal` | → | 目标：名单里的句柄（与 `sid` 恰给一个） |
| `ticket` | → | 订阅票（客户端铸的不透明串，至多 128 字节，只用一次）；之后的 `terminal_screen` / `terminal_follow_end` 帧带它 |

码：`bad_target` · `bad_args` · `not_known` · `ambiguous` · `no_tmux` · `tmux_too_old` · `too_many_follows` · `unobservable` · `child_timed_out`

#### `terminal-follow-ack`

订阅的第 `seq` 帧画完了（画面有变化就推下一帧）。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `seq` | → | 画完的那一帧的序号 |
| `ticket` | → | 订阅票 |

码：`bad_args` · `not_known`

#### `terminal-unfollow`

退订一个终端的画面（幂等，不发收尾帧）。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `ticket` | → | 订阅票；退不在册的也回 `ok`（记下它：订阅那一问若还在路上，到了也不起） |

码：`bad_args`

#### `terminal-name-mint`

起会话要的终端名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-name-mint`

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | ← | 铸出来的终端名（这台避让过） |

码：`bad_args` · `child_timed_out`

#### `kill`

杀一个 tmux 会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --kill` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `bus` | ← | 顺手从 cc-bus 注销的结果 `{removed, failed, unread, said, detail}`：`said` 是那几行句子（没有 ⇒ `null`），`detail` 是复制详情（没有 ⇒ 空串） |
| `client` | → | 自报的前端（过「哪个前端的会话」那一维） |
| `killed` | ← | 杀成了 |
| `name` | → | 要杀的 tmux 会话名 |
| `session` | ← | 杀掉的 tmux 会话名 |
| `sid` | → | 可带：只结束挂着这个会话（`@ccm_sid`）的窗格 |

码：`bad_args` · `no_tmux` · `no_such_session` · `wrong_owner` · `too_many_windows` · `kill_failed` · `child_timed_out`

#### `sessions-stop`

停一批会话（逐个答，一个不成不挡下一个）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --sessions-stop`

| 字段 | 向 | 说明 |
|---|---|---|
| `bus` | ← | 同 `kill` |
| `client` | → | 自报的前端，同 `kill` |
| `cmd` | → ← | 只有开终端那一形有 |
| `copyDetail` | ← | 失败那一个的复制详情（码 ＋ 原话；别的 ⇒ 空串） |
| `detail` | ← | 那一个的原话 |
| `said` | ← | 停失败那一个的那一句（与 `kill` 被拒同一张表）；别的 ⇒ `null` |
| `outcome` | ← | `done` · `skipped` · `failed` |
| `results` | ← | 逐个结果，与入参同序 |
| `session` | ← | 落在哪个 tmux 会话上 |
| `sid` | → ← | 会话 id |
| `sids` | → | 要停的会话（1–64 个，不重复） |
| `why` | ← | `skipped` / `failed` 的码 |

码：`bad_args` · `unobservable`

#### `sessions-where`

一批会话各在这台哪个终端里。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --sessions-where`

| 字段 | 向 | 说明 |
|---|---|---|
| `client` | → | 自报的前端，同 `kill` |
| `host` | ← | 终端宿主 |
| `names` | ← | 带着它的 tmux 会话名 |
| `results` | ← | 逐个结果，与入参同序 |
| `sid` | → ← | 会话 id |
| `sids` | → | 要问的会话（1–64 个，不重复） |
| `standing` | ← | `running` · `ambiguous` · `idle` · `none` · `no_tmux` |
| `terminal` | ← | 名单里那一行的句柄 |
| `terminals` | ← | 与 `names` 同序同数，每项 `{host, terminal}` |

码：`bad_args` · `unobservable`

#### `sessions-start`

起 / 接回一批会话（逐个答）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --sessions-start`

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → ← | 那一项用哪个号：缺 ＝ 跟随 · `{kind:"base"}` · `{kind:"named", name}`；应答里是实际用的号 |
| `client` | → | 自报的前端，同 `kill` |
| `cmd` | → ← | 开终端那一形要跑的那一行 |
| `configDir` | ← | `account` 里：那个号的配置目录 |
| `cwd` | → | 那一项的工作目录 |
| `defaultLauncher` | → | 整批一份：那一家的默认启动器 |
| `fork_of` | → | 可缺：源会话 sid（只许与 `fresh_terminal: true` 一起） |
| `copyDetail` | ← | 失败那一个的复制详情（码 ＋ 原话；别的 ⇒ 空串） |
| `detail` | ← | 那一个的原话 |
| `said` | ← | 停失败那一个的那一句（与 `kill` 被拒同一张表）；别的 ⇒ `null` |
| `fresh_terminal` | → | 可缺：分叉出来的那一条 ⇒ 必铸新终端名 |
| `items` | → | 要起的会话，每项 `{sid, cwd, account?, fresh_terminal?, fork_of?}` |
| `kind` | → ← | `account` 的种类 |
| `launcher` | → | 整批一份：用户设置的 resume 命令原值 |
| `local` | → | 这台是不是界面所在那台（开终端那一形按它选本机 / 远端那一行） |
| `mode` | → | `tmux`（在 tmux 里后台起）· `window`（只渲那一行交回，窗口由界面开） |
| `model` | ← | `account` 里：用的模型 |
| `name` | → ← | `account` 为 `named` 时的号名 |
| `outcome` | ← | `done` · `skipped` · `failed` |
| `results` | ← | 逐个结果，与入参同序 |
| `session` | ← | 落在哪个 tmux 会话上 |
| `sid` | → ← | 会话 id |
| `unavailable` | ← | 选不了号的那一项：`{requested, pinned, listKnown, alternative}` |
| `why` | ← | `skipped` / `failed` 的码 |

码：`bad_args` · `unobservable`

#### `session-new`

起一个新会话（全产品一个框、一个请求）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new`

| 字段 | 向 | 说明 |
|---|---|---|
| `rotation` | → | 可缺：轮换来源。缺 / `"follow"` ＝ 跟随默认（不写）· `{rule: id}` ＝ 起之前这台先定好 sid（那一家起新会话认的旗标，如 `--session-id`）、按它把来源写成那条规则，回包 `sid` 就是它；规则不在 ⇒ `no_such_rule`；那一家不认先定 sid ⇒ `bad_args` |
| `account` | → | 可缺 ＝ 跟随（分叉跟源会话上次的号；新起的 ⇒ 这台的默认号）· `{kind:"base"}` · `{kind:"named", name}` |
| `agent` | → | 哪一家（线上的 kind） |
| `cmd` | → ← | `open` 时界面要在终端里跑的那一行 |
| `command` | → | 启动命令；空 / 缺 ⇒ 那一家的默认启动器 |
| `configDir` | ← | 应答 `account` 里：那个号的配置目录 |
| `cwd` | → | 工作目录（开头的 `~` 按这台的家目录读） |
| `field` | ← | 失败时不行的那一格（`agent` · `command` · `cwd` · `account` · `place` · `tmuxName`；整体的 ⇒ `null`） |
| `forkFrom` | → | 可缺 |
| `kind` | → ← | `account` 的种类：`follow` · `base` · `named` |
| `local` | → | 发请求的界面就在这台上（开窗那一形本机与远端渲法不同） |
| `model` | ← | 应答 `account` 里：用的模型 |
| `models` | → | 可缺 |
| `name` | → ← | `account` 为 `named` 时的号名；应答 `account` 里是实际用的号 |
| `outcome` | ← | `started`（tmux 里起好了，`session` 是会话名）· `open`（界面开一个终端跑 `cmd`） |
| `place` | → | `tmux`（在这台 tmux 里后台起，关终端不断）· `window`（开一个新终端窗口直接跑） |
| `session` | ← | `started` 时的 tmux 会话名 |
| `sid` | ← | 分叉出来的新会话 sid；新起的 ⇒ `null`（报到之前说不出） |
| `ticket` | → | 可缺：这一趟的票（界面每次点［新建］一张）；同一张票再问 ⇒ 起好了回原样那一份 · 还在起 ⇒ `launch_pending` · 没见过 / 没起成 ⇒ 照常起。票只记在这台常驻后端的进程里（最近 64 张），后端重启就忘了 ⇒ 照常起（已知边界） |
| `tmuxName` | → | 可缺 ⇒ 这台铸 |
| `unavailable` | ← | `account_unavailable` 时那一形，带替代号 |
| `uuid` | → | `forkFrom` 里：从哪条消息处分叉 |

码：`bad_args` · `unknown_agent` · `bad_command` · `no_dir` · `account_unavailable` · `place_unavailable` · `bad_tmux_name` · `tmux_taken` · `unobservable` · `fork_failed` · `refused` · `start_failed` · `child_timed_out` · `launch_pending` · `no_such_rule`

#### `session-new-facts`

起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new-facts`

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 哪一家（线上的 kind） |
| `agents` | ← | 这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序） |
| `at` | → | 可缺（随 `forkOf`） |
| `cwd` | → | 工作目录（开头的 `~` 按这台的家目录读） |
| `fork` | ← | 没给 `forkOf` ⇒ `null` |
| `forkOf` | → | 可缺 |
| `lastMs` | ← | `recent` 一项：那个目录最近一次会话的修改时刻（毫秒） |
| `launch` | ← | `fork` 里：起分叉会话要的三格（同 `session-fork` 的 `launch`） |
| `recent` | ← | 这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个；`lastMs` 是那个目录最近一次会话的修改时刻） |
| `startText` | ← | `fork` 里：那一轮你那句在这台本地钟上的钟面 `HH:MM`（界面照抄；说不出 ⇒ `null`） |
| `tmux` | ← | 这台有没有 tmux（`false` ⇒ 只能开终端窗口） |
| `turn` | ← | `fork` 里：`at` 那一条在第几轮 |

码：`bad_args` · `fork_failed`

#### `session-new-dir`

起新会话前核一个目录：在不在 · 会用哪个终端名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new-dir`

| 字段 | 向 | 说明 |
|---|---|---|
| `cwd` | → | 工作目录（开头的 `~` 按这台的家目录读） |
| `exists` | ← | 这个目录在不在 |
| `forkOf` | → | 可缺 |
| `tmuxName` | → | 可缺 ⇒ 这台铸 |

码：`bad_args`

#### `session-interrupts`

动一个会话之前，会打断什么。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-interrupts`

| 字段 | 向 | 说明 |
|---|---|---|
| `families` | ← | 按族的清单，空族不出现；一族都没有 ⇒ `[]`（界面直接做，不问） |
| `family` | ← | `turn`（那个会话有一轮在跑：活着的会话进程状态说在干活）· `agent`（它派出去还在跑的子运行）· `task`（它任务表里 `in_progress` 的） |
| `names` | ← | 显示名：子运行的标签（无标签用种类 / 运行号）· 任务主题；`turn` 那一族为空表 |
| `sid` | → | 会话 id（空 / 缺 ⇒ `bad_args`） |

码：`bad_args` · `failed`

#### `session-restart`

换号重启。

收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → ← | 用户点名的号（只收名字）；应答里是实际用的号 |
| `agent` | → | 同 `sessions-start` 的整批那几格 |
| `arrive_within_ms` | → | 等新进程报出的期限（≤ 3 600 000） |
| `client` | → | 自报的前端，同 `kill` |
| `compact` | ← | 先压缩那一步：`done` · `timed_out` · `skipped` · `unsupported` · `failed` |
| `compact_first` | → | 先请求压缩、等记录里出现压缩摘要再停 |
| `compact_within_ms` | → | 等压缩的期限（≤ 3 600 000） |
| `configDir` | ← | 应答 `account` 里：配置目录 |
| `cwd` | → | 工作目录 |
| `defaultLauncher` | → | 同 `sessions-start` |
| `launcher` | → | 同 `sessions-start` |
| `local` | → | 同 `sessions-start` |
| `model` | ← | 应答 `account` 里：模型 |
| `models` | → | 可缺：这台的模型偏好表原值 |
| `name` | ← | 应答 `account` 里：实际用的号 |
| `names` | ← | `ambiguous` 失败的 `data`：在跑的那几个终端名 |
| `pids` | ← | `session_already_live` 失败的 `data`：那几个 pid |
| `sid` | → | 会话 id |
| `started` | ← | `arrived`（新进程报出了）· `missed` |
| `terminal` | ← | 所在的终端（会话名） |
| `why` | ← | `stop_failed` / `start_failed` 失败的 `data`：那一步的码 |

码：`bad_args` · `unobservable` · `account_unavailable` · `not_in_terminal` · `ambiguous` · `session_already_live` · `stop_failed` · `start_failed`

#### `launch`

建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --launch` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 可选，仅 create-or-attach：注册表里的一家（空 ⇒ 默认那一家） |
| `ccm_sid` | → | 可选：create-or-attach 写成意图键 `@ccm_sid_expect`；send-into 键入挂着它的那个窗格 |
| `client` | → | 可选：自报的前端（send-into 过「哪个前端的会话」那一维；create-or-attach 写成会话的 `@ccm_client`） |
| `created` | ← | 这一次新建了会话 |
| `cwd` | → | 可选，仅 create-or-attach |
| `height` | → | 同 `width` |
| `mode` | → | `create-or-attach`（建或接）· `send-into`（往已在的会话里键入） |
| `name` | → | tmux 会话名 |
| `payload` | → | 要键入的那一行 |
| `session` | ← | 落在哪个会话上 |
| `typed` | ← | `send-keys` 退出 0：只有 `send-keys` 的退出码那么强（pane 在 copy-mode 时照样退 0、键被吃掉） |
| `width` | → | 可选，仅 create-or-attach：1–4 位十进制字符串，与 `height` 同时给或都不给 |

码：`bad_args` · `no_tmux` · `no_such_session` · `wrong_owner` · `create_failed` · `typed_unconfirmed` · `child_timed_out`

### 4.9 机器

#### `ccm-print`

一条别名实际会执行什么。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `args` | → | 一条别名的预置参数（原样 ccm argv，`<交给 agent 的…> -- <ccm 的…>` 那一形；最多 64 个、每个最长 4096 字节） |
| `line` | ← | `ccm --print` 那一行 |

码：`bad_args` · `refused`

#### `ccm-probe`

这台的 `ccm` 会哪些。

不收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `agents` | ← | 认得的 agent 种类 |
| `build` | ← | 这一份的 `BUILD_ID` |
| `capabilities` | ← | 这台 `ccm` 认得的能力（与名片的 `capabilities=` 行同一份） |
| `version` | ← | CLI 契约版本（`CCM_VERSION`） |

#### `deploy-plan`

那台的后端要不要换、换成哪一格。

收 `args` · 可撤 · CLI：`ccm -- --deploy-plan`

| 字段 | 向 | 说明 |
|---|---|---|
| `ack` | ← | 问 `uname` 那一趟拨号的 `DialAck` 原样（逐地址指纹 · 严格与否）：拨号在本机后端里，monitor 按它固化指纹（与自己开链路那几条同一个判定） |
| `action` | ← | `skip`（已是这一版）· `deploy`（没装 / 0 字节 / 更旧）· `keep`（另一版、不比这一版旧 ⇒ 不动它） |
| `arch` | ← | 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`） |
| `expected` | ← | 那一格这一版带着的字节自报的身份（对照物） |
| `label` | ← | 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`） |
| `leftovers` | ← | 落点目录里没人要的上传残件（家目录相对，排序）；列不出那个目录 ⇒ `[]`（下次连上再问） |
| `os` | ← | 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`） |
| `theirs` | ← | `keep` 时那台上那一份自报的身份，否则 `null` |
| `why` | ← | 人读原因（`skip` 时空串） |

码：`bad_args` · `io_failed` · `refused` · `unreachable` · `undecidable`

#### `resident-verdict`

远端常驻后端要不要换一次。

收 `args` · 可撤 · CLI：`ccm -- --resident-verdict`

| 字段 | 向 | 说明 |
|---|---|---|
| `action` | ← | `replace`（换掉再接）· `attach`（接上它） |
| `mine` | → | monitor 手上这一版后端自报的身份（非空） |
| `older` | ← | 那台是不是比手上这一版旧（与 `replaced` 无关；版本提示那句话按它挑「会换掉」还是「不会换回去」） |
| `replaced` | → | 这一趟是不是已经换过一次 |
| `theirs` | → | 那台 hello 报的 `build_id`（缺 ⇒ 空串） |

码：`bad_args`

#### `place-verdict`

本机那一份放不放。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --place-verdict`

| 字段 | 向 | 说明 |
|---|---|---|
| `action` | ← | `place`（放 / 换上去）· `keep`（盘上那一份不比这一份旧 ⇒ 不动、用它） |
| `dest` | → | 落点的绝对路径（不在 ⇒ 没装 ⇒ 放；读不了 ⇒ `undecidable`，不当成没装） |
| `machine` | → | 对人说话时这台叫什么（monitor 交「本机」） |
| `why` | ← | 人读原因（`keep` 时点名两边各是哪一版） |

码：`bad_args` · `refused` · `undecidable`

#### `pubkey-push`

把本机公钥推进那台的 `authorized_keys`。

收 `args` · 可撤 · CLI：`ccm -- --pubkey-push`

| 字段 | 向 | 说明 |
|---|---|---|
| `jump` | → | 同 `remote-probe` |
| `machine` | → | 同 `remote-probe` |
| `outcome` | ← | `added`（新加的）· `already`（本就有整行相等的一行，没写） |
| `pubKeyPath` | → | 本机那份 `.pub` 的路径；缺席 / 空 ⇒ 私钥同名 `.pub`（两样都没有 ⇒ `refused`，界面让用户挑文件） |
| `pubPath` | ← | 实际推的是哪一份（给人看） |
| `saved` | → | 同 `remote-probe` |
| `via` | ← | 走了哪条：`backend`（那台后端的文件管理面）· `exec`（那一次 exec） |

码：`bad_args` · `bad_jump` · `refused` · `failed`

#### `authorized-keys-add`

把一行公钥并进这台的 `authorized_keys`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --authorized-keys-add`

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 一行公钥（本条自己也校验一遍） |
| `outcome` | ← | `added` · `already` |

码：`bad_args` · `refused` · `io_failed`

#### `exit-policy-read`

读「退出行为」那个值（值住后端所在那台）。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --exit-policy-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `detail` | ← | `unreadable` 时的复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答），`reason` 那一句不带原话 |
| `killOnExit` | ← | monitor 退出时结束本机常驻后端 |
| `path` | ← | 那份文件的路径（在 `~/.cc-monitor/` 下） |
| `reason` | ← | `unreadable` 时的原因 |
| `said` | ← | 这个值意味着什么的一句话 |
| `state` | ← | `absent`（没设过）· `chosen` · `unreadable`（读不出：也是 `ok:true`） |

#### `exit-policy-set`

写「退出行为」那个值，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --exit-policy-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `detail` | ← | 同 `exit-policy-read` |
| `killOnExit` | → ← | 要写的值（布尔） |
| `path` | ← | 同 `exit-policy-read` |
| `reason` | ← | 同 `exit-policy-read` |
| `said` | ← | 同 `exit-policy-read` |
| `state` | ← | 写完再读一遍的状态 |

码：`bad_args` · `io_failed`

#### `relay-optin`

直接敲的 agent 也走中转。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 哪一家的那一份（适配器 id）。空串 ⇒ 默认那一家；注册表里没有 ⇒ `bad_args`，那句话列出认得的几家 |
| `listening` | ← | 这台我们的中转此刻在不在听（与 `apikey-routing.running` 同一个判准） |
| `missing` | ← | 那一段为什么生成不了（这台的中转还没起来过、没有钥匙 · 决策表不给这一条）；已装 / 生成得了 ⇒ 空串 |
| `note` | ← | 那份文件为什么读不了（`unreadable` 才有，其余空串） |
| `snippet` | ← | 要合并进那份文件的那一段（**带钥匙**：设置文件里写不了 `$(cat …)`）；`installed` 或生成不了 ⇒ `null` |
| `source` | ← | 读的是哪份文件（这台后端看到的路径） |
| `state` | ← | `installed`（写着的就是现在那一条）· `stale`（是我们那一形 |

码：`bad_args` · `failed`

#### `drift-report`

这台后端的漂移账。

不收 `args` · 可撤 · CLI：`ccm -- --drift-report`

| 字段 | 向 | 说明 |
|---|---|---|
| `faces` | ← | 各家记录解释面记下的「看不懂的记录」：`face` |

码：`bad_args`

#### `forward-start`

起一条本地端口转发。

收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | ← | 这条转发的号（`fwd-<n>`，本进程内单调） |
| `jump` | → | 跳板那一台的配置（可缺） |
| `localPort` | → | 绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip |
| `machine` | → | 那台的配置（界面 `remote-config` 那一格，camelCase）· 已保存的那一份 · 跳板那一台：可达表里**没有**那台（流没起来）时按它自己组请求去拨（`dial/machine.rs`，同 `remote-probe`），不拒 |
| `origin` | → | 那台的名字（可达表 `remote-reach` 的键：那台的流握手过 ⇒ 用那一份拨号请求） |
| `remoteHost` | → | 绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip |
| `remotePort` | → | 绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip |
| `saved` | → | 已保存的那一份机器配置（可缺） |

码：`bad_args` · `bad_spec` · `unreachable` · `bad_jump` · `full` · `failed`

#### `remote-probe`

测试连接。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `backendHello` | ← | 那台后端的一句（往返毫秒 · 版本 · 做不到几项） |
| `backendOk` | ← | 那台后端答没答（hello ＋ `ping` 往返） |
| `end` | ← | 结局，**最后一格** |
| `endpoint` | ← | 实际连上的地址 |
| `fingerprint` | ← | 那台的主机指纹（`sshOk:false` 时不给） |
| `jump` | → | 跳板那一台的配置（可缺） |
| `machine` | → | 那台的配置（可能还没保存）：`host` · `port` · `user` · `keyPath` · `addresses` · `jump` … |
| `message` | ← | 结局那一句 |
| `reached` | ← | `ssh` 握手过了 · `hello` 那台后端回了 hello · `control` ping 往返了 |
| `saved` | → | 已保存的那一份（可缺） |
| `sshOk` | ← | SSH 握手 ＋ 鉴权过没过（结局 `end` 里） |
| `stage` | ← | 拨号阶段行，与界面 `ConnectStage` 同形 |
| `ticket` | → ← | 界面交来的票（1..=64 个 `[A-Za-z0-9-]`），进度帧 `probe` 原样回填 |

码：`bad_args` · `bad_jump` · `failed`

#### `forward-stop`

停一条转发。

收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → ← | 转发号（`fwd-<n>`） |

码：`bad_args` · `not_found`

#### `forward-list`

列转发。

不收 `args` · 可撤 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `connCount` | ← | 累计接进的连接数 |
| `forwards` | ← | 转发清单，每项 `{id, origin, localPort, remoteHost, remotePort, state, connCount}` |
| `id` | → ← | 转发号 |
| `localPort` | ← | 本机口 |
| `origin` | ← | 那台的名字 |
| `remoteHost` | ← | 那台上的目标 host |
| `remotePort` | ← | 那台上的目标口 |
| `state` | ← | `running`（链路还在）· `error`（链路自己收工了，留在账上等用户停） |

#### `remote-reach`

本机后端的可达表登记。

收 `args` · 可撤 · CLI：`ccm -- --remote-reach`

| 字段 | 向 | 说明 |
|---|---|---|
| `dial` | → | 那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体） |
| `origin` | → | 那台的名字（monitor 的 origin 名，本后端只当不透明的键用） |
| `reach` | ← | 登记之后的可达表 `[{origin, machine}]`（同 `assets-sync` 的那一格） |

码：`bad_args`

#### `backend-log`

这台后端的 stderr 诊断文件尾部。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --backend-log`

| 字段 | 向 | 说明 |
|---|---|---|
| `maxBytes` | → | 可选，缺省 = 封顶 256 KiB：只回尾部这么多字节 |
| `path` | ← | 本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null` |
| `size` | ← | 那份文件的总字节数 |
| `text` | ← | 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行 |
| `truncated` | ← | 前面还有没回的字节 |

码：`bad_args` · `failed`

#### `powershell-policy-set`

那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --powershell-policy-set`

| 字段 | 向 | 说明 |
|---|---|---|
| `host` | → | 哪一代：`powershell`（5.1）· `pwsh`（7） |
| `policy` | ← | 设完现问的那一份（形状同 `aliases-read` 候选里的 `policy`） |
| `setError` | ← | 设的那一下 PowerShell 的原话（组策略压着时它会报）；`null` = 没报 |

码：`bad_args` · `refused`

#### `ssh-config-aliases`

这台 `~/.ssh/config` 里可点的别名。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ssh-config-aliases`

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | ← | `Host` 行里非通配的别名（去重保序）；没有 config ⇒ `[]` |

#### `ssh-config-resolve`

一个别名的有效连接参数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ssh-config-resolve`

| 字段 | 向 | 说明 |
|---|---|---|
| `alias` | → | 必填 |
| `host` | ← | `ssh -G` 的 `hostname`（缺省回退别名） |
| `keyPath` | ← | 第一个**展开后存在**的 `identityfile`（只问在不在，不读内容）；都不存在 ⇒ `null` |
| `port` | ← | `port`（缺省 22） |
| `proxyJump` | ← | `proxyjump`（`none` ⇒ `null`） |
| `user` | ← | `user`（缺省空串） |

码：`bad_args` · `bad_alias` · `failed` · `child_timed_out`

#### `ssh-config-import`

批量导入预览。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ssh-config-import`

| 字段 | 向 | 说明 |
|---|---|---|
| `addresses` | ← | 其余地址，端口不同则 `host:port` |
| `alias` | ← | 成员的别名 |
| `groups` | ← | 聚成组的机器，每组 `{label, host, port, user, keyPath, addresses, jump, members, inList}` |
| `host` | → ← | 组首的 host |
| `inList` | ← | 组首或任一成员的地址 ＋ 组的用户 ＋ 端口与 `known` 里某台相同，去首尾空白比 ⇒ 已在列表里，界面照它灰、不自己比 |
| `jump` | ← | 组内首个非空 proxyjump |
| `keyPath` | ← | 组首 |
| `known` | → | 机器列表里已有的那几台，`[{host, user, port}]`；缺 / 形状不对 ⇒ `bad_args` |
| `label` | ← | 单成员组 = 完整别名，多成员 = 基名 |
| `members` | ← | `alias` / `host` / `port` / `proxyJump`，界面「拆分」时据此还原 |
| `port` | → ← | 组首的端口 |
| `proxyJump` | ← | 成员的跳板 |
| `user` | → ← | 组首的用户 |

码：`bad_args`

### 4.10 计划

#### `plan-list`

这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-list`

| 字段 | 向 | 说明 |
|---|---|---|
| `dirs` | → | 可选：另要问的目录（串的数组） |
| `fresh` | → | 可选布尔：`true` ⇒ 认过的目录也重问 |
| `pb` | ← | `{state: ok\|missing\|unsupported, said, version}`：pb 装没装、认不认得它的输出 |
| `workspaces` | ← | 每个工作区一格 `{workspace, repo, auto, rev, stale, needCount, slices: [{name, domain, current, progress, needCount, error, stale}]}` |

码：`bad_args`

#### `plan-read`

一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-read`

| 字段 | 向 | 说明 |
|---|---|---|
| `workspace` | → | 工作区根（`plan-list` 给的那个） |
| `rev` | ← | 这一份输出的摘要：变了才算计划变了 |
| `readAt` | ← | 读到的时刻（epoch ms） |
| `needCount` | ← | 这个工作区要你看的数（没认可的，不含 agent 问人那一种） |
| `slices` | ← | 每片一格：读不成 ⇒ `error`；这一刻读不成但读好过 ⇒ 上一次那一份 ＋ `stale {said, since}`；`needs: [{key, kind: top\|red\|ended\|ask, block, cell, sid, acked}]` · `needCount`；每格 `returned`：退回过 ⇒ `{at, to, state: returned\|unsure\|landed, by: child\|body, child}`，没有 ⇒ `null` |
| `stale` | ← | 整次读不成、给的是上一次那一份 ⇒ `{said, raw, since}`；否则 `null` |

码：`bad_args` · `failed` · `no_pb` · `not_workspace` · `pb_unsupported`

#### `plan-cell-view`

一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-cell-view`

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → | 格的编号 |
| `slice` | → | 片名 |
| `view` | ← | 原样那一段 |
| `workspace` | → | 工作区根 |

码：`bad_args` · `no_view`

#### `plan-ack`

认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `plan_changed` 带新的数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-ack`

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 那一条的键（`plan-read` 里 `needs[].key`） |
| `slice` | → | 片名 |
| `workspace` | → | 工作区根 |
| `acked` | ← | `true` |
| `key` | ← | 原样 |
| `needCount` | ← | 这个工作区此刻要你看的数 |

码：`bad_args` · `io_failed` · `no_such_need` · `not_ackable` · `not_read` · `review_unreadable`

#### `plan-unack`

撤掉一条认可（没有也不算错）；推一帧 `plan_changed` 带新的数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-unack`

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 那一条的键 |
| `slice` | → | 片名 |
| `workspace` | → | 工作区根 |
| `acked` | ← | `false` |
| `key` | ← | 原样 |
| `needCount` | ← | 这个工作区此刻要你看的数 |

码：`bad_args` · `io_failed` · `not_read` · `review_unreadable`

#### `plan-return`

把人的话送给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，在等你 ⇒ 拒，已结束 / 认不出 ⇒ 只给复制，能送走 `terminal-input`；送到了记一条已退回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-return` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

| 字段 | 向 | 说明 |
|---|---|---|
| `client` | → | 自报的前端（过 `terminal-input` 那一道身份门） |
| `id` | → | 格的编号 |
| `line` | ← | 送出的那一行（后端拼；只读） |
| `result` | ← | `delivered` · `unsure`（送达未知，别重发）· `refused` · `copy`（送不了，只给这一行去复制） |
| `said` | ← | `refused` / `copy` 时给人看的那一句 |
| `screen` | ← | `screen-changed` 时带的新指纹 |
| `seen_screen` | → | 送之前看到的那一屏的指纹（同 `terminal-input`） |
| `slice` | → | 片名 |
| `text` | → | 人的原话（换行并成空格） |
| `to` | → | 可选：`owner`（在长它的，缺省）· `signer`（签它的，pb 给了才有） |
| `to` | ← | 送给的那一位（同 `plan-read` 里 `owner` 的形状） |
| `why` | ← | `waiting`（在等你批准或回答）· `ended` · `unknown` · 或 `terminal-input` 的原因 |
| `workspace` | → | 工作区根 |

码：`bad_args` · `bad_target` · `capture_failed` · `child_timed_out` · `failed` · `io_failed` · `no_pb` · `no_server` · `no_such_cell` · `no_such_session` · `no_target` · `no_tmux` · `not_workspace` · `pb_unsupported` · `review_unreadable` · `unobservable`

## 5. CLI 子命令

一次性调用：`ccm -- --子命令 …`（打头的 `--` 之后才归后端）；收 `args` 的那一段 JSON 从 stdin 读，或经 `--args-b64 <base64>` 走 argv（二选一，上限与码见 `IPC-PROTOCOL.md` §8），回一行 JSON 到 stdout。
「＝ 帧命令」的那几条与流上同名命令是同一个处理器。

| 子命令 | 说明 |
|---|---|
| `--account-trust` `<configDir> <cwd>` | 换号恢复前的信任预检：单行 `{trusted, known, error}`；`configDir` 必须逐字是账号清单里的一个，否则退出 2、`unknown_config_dir`；不回那份配置的内容 |
| `--account-trust-zero` `<cwd>` | 账号 0（没启用多账号时那个原生身份）的信任预检，形状同 `--account-trust`；`cwd` 只当查表键，不收路径参数 |
| `--accounts-add` | ＝ 帧命令 `accounts-add`：新建一个号 |
| `--accounts-init` | ＝ 帧命令 `accounts-init`：建账号库 |
| `--accounts-list` | ＝ 帧命令 `accounts-list`：账号清单 |
| `--accounts-login-cmd` | ＝ 帧命令 `accounts-login-cmd`：在终端里登录一个号的那一行 |
| `--accounts-mcp-pick` | ＝ 帧命令 `accounts-mcp-pick`：两边都改了的那一条用哪一版 |
| `--accounts-mcp-read` | ＝ 帧命令 `accounts-mcp-read`：这台各账号共用的用户级 MCP 此刻的样子 |
| `--accounts-mcp-remove` | ＝ 帧命令 `accounts-mcp-remove`：从各账号共用的用户级 MCP 里删一条 |
| `--accounts-mcp-sync` | ＝ 帧命令 `accounts-mcp-sync`：停 / 开各账号之间同步用户级 MCP |
| `--accounts-remove` | ＝ 帧命令 `accounts-remove`：删一个号 |
| `--accounts-repair` | ＝ 帧命令 `accounts-repair`：修复账号库（幂等） |
| `--accounts-rollback` | ＝ 帧命令 `accounts-rollback`：按一份备份还原 |
| `--accounts-sessions` | ＝ 帧命令 `accounts-sessions`：正在跑的会话各属哪个账号 |
| `--accounts-set-default` | ＝ 帧命令 `accounts-set-default`：设默认号 |
| `--accounts-trust` | ＝ 帧命令 `accounts-trust`：换号前的信任预检 |
| `--accounts-verify` | ＝ 帧命令 `accounts-verify`：核对账号库 |
| `--agent-home-check` | ＝ 帧命令 `agent-home-check`：那个目录能不能当 agent 家目录 |
| `--aliases-block-install` | ＝ 帧命令 `aliases-block-install`：别名块装进人选的那份启动文件 |
| `--aliases-block-remove` | ＝ 帧命令 `aliases-block-remove`：别名块卸掉 |
| `--aliases-block-render` | ＝ 帧命令 `aliases-block-render`：别名块预览 |
| `--aliases-read` | ＝ 帧命令 `aliases-read`：启动文件候选（接入那一格） |
| `--apikey-key-set` | ＝ 帧命令 `apikey-key-set`：给一个账号写 key，写完读回 |
| `--apikey-read` | ＝ 帧命令 `apikey-read`：上游选择凭据文件在这台的状态 |
| `--assets-catalog` | ＝ 帧命令 `assets-catalog`：资产目录 |
| `--assets-catalog-merge` | ＝ 帧命令 `assets-catalog-merge`：把另一台后端的整份目录并进来 |
| `--assets-sync` | ＝ 帧命令 `assets-sync`：本机常驻后端沿池里那条 SSH 同步资产目录 |
| `--authorized-keys-add` | ＝ 帧命令 `authorized-keys-add`：把一行公钥并进这台的 `authorized_keys` |
| `--backend-log` | ＝ 帧命令 `backend-log`：这台后端的 stderr 诊断文件尾部 |
| `--backend-probe` | 能力探测：回 `{proto, buildId, commands}`，`commands` 是这台真能派发的 CLI 控制面子命令；不读 stdin |
| `--bus-broadcast` | ＝ 帧命令 `bus-broadcast`：给总线上在线的成员群发一条 |
| `--bus-inbox` | ＝ 帧命令 `bus-inbox`：只读看一个 agent 收件箱的尾巴 |
| `--bus-kill` | ＝ 帧命令 `bus-kill`：收掉一个总线成员 |
| `--bus-list` | ＝ 帧命令 `bus-list`：谁在线 + 各自待读多少 |
| `--bus-send` | ＝ 帧命令 `bus-send`：发一条消息 |
| `--bus-spawn` | ＝ 帧命令 `bus-spawn`：派生一个协作 agent |
| `--bus-state` | ＝ 帧命令 `bus-state`：总线名单 ＋ spawn 台账一次回全 |
| `--cc-bus-install` | ＝ 帧命令 `cc-bus-install`：把这台二进制带着的 cc-bus 装到这台 |
| `--cc-bus-install-state` | ＝ 帧命令 `cc-bus-install-state`：装 cc-bus 到这台之前看一眼 |
| `--chores-mark` | ＝ 帧命令 `chores-mark`：记下「要你动手」里的一个选择 |
| `--data-report` | ＝ 帧命令 `data-report`：「文件与数据」那一份成品 |
| `--deploy-plan` | ＝ 帧命令 `deploy-plan`：那台的后端要不要换、换成哪一格 |
| `--drift-report` | ＝ 帧命令 `drift-report`：这台后端的漂移账 |
| `--exit-policy-read` | ＝ 帧命令 `exit-policy-read`：读「退出行为」那个值（值住后端所在那台） |
| `--exit-policy-set` | ＝ 帧命令 `exit-policy-set`：写「退出行为」那个值，写完读回 |
| `--ext-list-here` | ＝ 帧命令 `ext-list-here`：设置「扩展」页那张表的本机那一半（不问可达表） |
| `--ext-note-set` | ＝ 帧命令 `ext-note-set`：写 / 改 / 清一个扩展的备注 |
| `--ext-uninstall-apply` | ＝ 帧命令 `ext-uninstall-apply`：从这台卸一个扩展 |
| `--ext-uninstall-preview` | ＝ 帧命令 `ext-uninstall-preview`：从这台卸一个扩展之前那张卡 |
| `--files-browse` | ＝ 帧命令 `files-browse`：告诉后端「用户现在在看哪几个目录」 |
| `--files-chmod` | ＝ 帧命令 `files-chmod`：改 unix 权限位 |
| `--files-commit-text` | ＝ 帧命令 `files-commit-text`：按块读回、拼起来、覆盖写 |
| `--files-commit-upload` | ＝ 帧命令 `files-commit-upload`：把暂存区里一份传完的上传件挪进目标 |
| `--files-copy` | ＝ 帧命令 `files-copy`：同根内复制一份普通文件 |
| `--files-create` | ＝ 帧命令 `files-create`：在文件管理目标根底下新建一份此前不存在的文件 |
| `--files-delete` | ＝ 帧命令 `files-delete`：删一个文件或一个空目录 |
| `--files-delete-session` | ＝ 帧命令 `files-delete-session`：删一份历史会话 |
| `--files-extract` | ＝ 帧命令 `files-extract`：解压到一个新目录 |
| `--files-find` | ＝ 帧命令 `files-find`：在常驻索引里查 |
| `--files-grep` | ＝ 帧命令 `files-grep`：在一个目录底下按内容搜 |
| `--files-home` | ＝ 帧命令 `files-home`：后端这个用户的 home |
| `--files-index-rebuild` | ＝ 帧命令 `files-index-rebuild`：重建常驻文件名索引：走一遍，做完返回 |
| `--files-index-status` | ＝ 帧命令 `files-index-status`：索引的新鲜度 / 条目数 / 常驻字节 |
| `--files-link` | ＝ 帧命令 `files-link`：建一条符号链接 |
| `--files-ls` | ＝ 帧命令 `files-ls`：列一个目录的直接子项 |
| `--files-mkdir` | ＝ 帧命令 `files-mkdir`：新建一个目录 |
| `--files-peek` | ＝ 帧命令 `files-peek`：读改写的读那一半 |
| `--files-put` | ＝ 帧命令 `files-put`：整份替换一份文本文件 |
| `--files-read-chunk` | ＝ 帧命令 `files-read-chunk`：按字节寻址分块读回 |
| `--files-read-text` | ＝ 帧命令 `files-read-text`：读一份文本进编辑器 |
| `--files-rename` | ＝ 帧命令 `files-rename`：改名 / 同根内移动 |
| `--files-size` | ＝ 帧命令 `files-size`：算一个目录有多大 |
| `--files-stage-chunk` | ＝ 帧命令 `files-stage-chunk`：存盘的一块进暂存区 |
| `--files-stat` | ＝ 帧命令 `files-stat`：一个路径的元数据 |
| `--files-write-text` | ＝ 帧命令 `files-write-text`：覆盖写一份已经在的普通文件 |
| `--find-in-session` `[--include-tools] [--limit <n>] --query <q> <jsonl>` | 在一份会话里找一段文字：头 `{kind:"session_find",v:1}` · 每条命中 `{uuid, kind, before, matched, after, turn, tsMs, tsText}`（`tsText` ＝ 那条的时刻按这台本地钟写好） · 尾 `{kind:"session_find_end",count,total}`；`limit` 缺省 500、封顶 2000 |
| `--first-run` | ＝ 帧命令 `first-run`：首次运行「开始用」三步各自打没打勾 |
| `--footprint-report` | ＝ 帧命令 `footprint-report`：「足迹」由这台后端出整份成品 |
| `--fork-session` `<args>` | 从某条消息处分叉出一个新会话文件，出参 `ForkResult`（见下） |
| `--history-annotate` | ＝ 帧命令 `history-annotate`：改一条历史注解 |
| `--history-branch` | ＝ 帧命令 `history-branch`：主线外清单 |
| `--history-facts` | ＝ 帧命令 `history-facts`：会话事实 |
| `--history-find` | ＝ 帧命令 `history-find`：会话内查找 |
| `--history-forget` | ＝ 帧命令 `history-forget`：删一条历史注解 |
| `--history-index` | ＝ 帧命令 `history-index`：会话骨架索引 |
| `--history-last-accounts` | ＝ 帧命令 `history-last-accounts`：sid → 上次用哪个号起 |
| `--history-lines` | ＝ 帧命令 `history-lines`：按行号取回一段 |
| `--history-list` | ＝ 帧命令 `history-list`：历史页的平铺会话清单 |
| `--history-page` | ＝ 帧命令 `history-page`：按字节分页读，出记录行 |
| `--history-read` | ＝ 帧命令 `history-read`：按字节分页读一份会话 |
| `--history-record` | ＝ 帧命令 `history-record`：这条会话的记录还在不在 |
| `--history-run` | ＝ 帧命令 `history-run`：一个子运行的记录 |
| `--history-search` | ＝ 帧命令 `history-search`：全文搜索 |
| `--history-search-merge` | ＝ 帧命令 `history-search-merge`：把各台的搜索结果合成一份 |
| `--history-tail` | ＝ 帧命令 `history-tail`：一份会话的尾段从哪个字节起 |
| `--history-turns` | ＝ 帧命令 `history-turns`：一轮的摘要 |
| `--history-user-inputs` | ＝ 帧命令 `history-user-inputs`：「你说过的话」清单 |
| `--hooks-diag` | ＝ 帧命令 `hooks-diag`：cc-bus 钩子诊断 |
| `--kill` | ＝ 帧命令 `kill`：杀一个 tmux 会话 |
| `--last-seen-read` | ＝ 帧命令 `last-seen-read`：读离线那台的上次值 |
| `--last-seen-write` | ＝ 帧命令 `last-seen-write`：记下一台这一次读成的那一份（或清掉那台） |
| `--launch` | ＝ 帧命令 `launch`：建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面） |
| `--launch-render-cli` | ＝ 帧命令 `launch-render-cli`：远端起会话那一行 `ccm …` |
| `--list-accounts` | 账号清单：首行 `{kind:"accounts-meta", enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault}`，其后每号一行 `{name, email, configDir, isDefault, mode, exists, loggedIn}`；没启用多账号 ⇒ `enabled:false`、退出 0 |
| `--list-projects` | 项目清单：每行 `{dirName, projectPath, sessionCount, lastActivityMs}`；工作目录在 `~/.cc-monitor/autostart/` 下的会话不出 |
| `--list-sessions` `<project_dir>` | 一个项目的会话：每行 `{sessionId, jsonlPath, startedAtMs, updatedAtMs, messageCountApprox, firstUserExcerpt, aiTitle, cwd}` |
| `--list-user-inputs` `[--from <offset>] <jsonl>` | 「你说过的话」：头 `{kind:"user_inputs",v:1,from}` · 每条 `{uuid, timestamp, excerpt}`（对话序）· 尾 `{kind:"user_inputs_end",count,end}`；`end` 是下次增量的 `--from`；`offset` 过了文件尾 ⇒ 退出 2 |
| `--machine-interrupts` | ＝ 帧命令 `machine-interrupts`：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么 |
| `--mcp-read` | ＝ 帧命令 `mcp-read`：这台机器的 MCP 列表成品 |
| `--mcp-server-put` | ＝ 帧命令 `mcp-server-put`：增 / 改这台一个项目 `.mcp.json` 里的一条 |
| `--mcp-server-remove` | ＝ 帧命令 `mcp-server-remove`：删这台一个项目 `.mcp.json` 里的一条 |
| `--mcp-sync-apply` | ＝ 帧命令 `mcp-sync-apply`：在要被写的那一台把那一条写进去 |
| `--mcp-sync-plan` | ＝ 帧命令 `mcp-sync-plan`：MCP 资产同步的判定 |
| `--mcp-sync-preview` | ＝ 帧命令 `mcp-sync-preview`：在要被写的那一台看装上之后那一条 |
| `--mcp-sync-source` | ＝ 帧命令 `mcp-sync-source`：装到别的机器时来源那一条 |
| `--ping` | ＝ 帧命令 `ping`：问活：零载荷，回 `ok` |
| `--place-verdict` | ＝ 帧命令 `place-verdict`：本机那一份放不放 |
| `--plan-ack` | ＝ 帧命令 `plan-ack`：认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `plan_changed` 带新的数 |
| `--plan-cell-view` | ＝ 帧命令 `plan-cell-view`：一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样） |
| `--plan-list` | ＝ 帧命令 `plan-list`：这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区） |
| `--plan-read` | ＝ 帧命令 `plan-read`：一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view） |
| `--plan-return` | ＝ 帧命令 `plan-return`：把人的话送给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，在等你 ⇒ 拒，已结束 / 认不出 ⇒ 只给复制，能送走 `terminal-input`；送到了记一条已退回 |
| `--plan-unack` | ＝ 帧命令 `plan-unack`：撤掉一条认可（没有也不算错）；推一帧 `plan_changed` 带新的数 |
| `--powershell-policy-set` | ＝ 帧命令 `powershell-policy-set`：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned` |
| `--profiles-bases` | ＝ 帧命令 `profiles-bases`：「基于」下拉能选的几段（选了不成圈） |
| `--profiles-impact` | ＝ 帧命令 `profiles-impact`：这几处改动会让哪几段合下来变（改前改后） |
| `--profiles-read` | ＝ 帧命令 `profiles-read`：读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填 |
| `--profiles-resolve` | ＝ 帧命令 `profiles-resolve`：一段合下来的合并表与「等于」那一行（可按未存的表单算） |
| `--profiles-write` | ＝ 帧命令 `profiles-write`：按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数 |
| `--pubkey-push` | ＝ 帧命令 `pubkey-push`：把本机公钥推进那台的 `authorized_keys` |
| `--quota-probe` | ＝ 帧命令 `quota-probe`：用某个号查一次额度 |
| `--quota-read` | ＝ 帧命令 `quota-read`：这台的额度账 |
| `--read-session` `<jsonl>` | 原样透传整份会话字节 |
| `--read-session-from-offset` `[--index] [--until <end>] <jsonl> <offset>` | 从字节 `offset` 续读：原样透传 `[offset, EOF)`（`--until` ⇒ `[offset, end)`）；`--index` ⇒ 出骨架索引：头 `{kind:"session_index",v:1,from}` · 每个可计行一条 `IndexRow`（见下）· 尾 `{kind:"session_index_end",count,end}`。续点用 `line` 帧的 `byte_offset`，别用 `seq` |
| `--read-session-tail` `<jsonl> <N>` | 尾部优先：首行 `{kind:"snapshot_meta",total,tail_from}`，随后原样输出最新 N 行 `[tail_from,total)`，再输出 `[0,tail_from)` |
| `--remote-reach` | ＝ 帧命令 `remote-reach`：本机后端的可达表登记 |
| `--resident-ensure` `[--replace]` | 确保这台的常驻后端在听：已在 ⇒ `{port, token, pid:null}`；没在 ⇒ 起一个脱离的自己、回 `{port, token:null, pid}`（钥匙由它绑上口后写进钥匙文件）；`--replace` 先停掉口上那一位再起 |
| `--resident-stop` `[--grace <秒>]` | 停这台的常驻后端：核身份 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful\|killed\|not_running, pid}` |
| `--resident-verdict` | ＝ 帧命令 `resident-verdict`：远端常驻后端要不要换一次 |
| `--resolve` | ＝ 帧命令 `resolve`：按 `ResumeSpec` 推出恢复命令 `CommandPlan`（与一次性 `--resolve` 同一个函数） |
| `--rotation-default-set` | ＝ 帧命令 `rotation-default-set`：设这台的默认规则 |
| `--rotation-plan` | ＝ 帧命令 `rotation-plan`：一份轮换接下来会怎么走 ＋ 草稿逐格校验（都不写） |
| `--rotation-rule-delete` | ＝ 帧命令 `rotation-rule-delete`：删轮换规则 |
| `--rotation-rule-rename` | ＝ 帧命令 `rotation-rule-rename`：给一条轮换规则改名 |
| `--rotation-rule-save` | ＝ 帧命令 `rotation-rule-save`：新建或整份改一条轮换规则 |
| `--rotation-rules-read` | ＝ 帧命令 `rotation-rules-read`：这台的轮换规则表 |
| `--rotation-session-read` | ＝ 帧命令 `rotation-session-read`：一批会话的轮换与「账号」格 |
| `--rotation-session-set` | ＝ 帧命令 `rotation-session-set`：改一批会话的轮换 |
| `--rotation-switch` | ＝ 帧命令 `rotation-switch`：现在就换 |
| `--search` `<query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]` | 全库全文搜索：每命中会话一行 `SessionHits`（camelCase，含 `hitsTruncated`），行序 = 最近优先 |
| `--session-accounts` | 正在跑的会话各属哪个号：每条 `{pid, sessionId, cwd, configDir, account, bare, alive, viaRelay}`；`account:null` ＝ 查不到（不猜） |
| `--session-fork` | ＝ 帧命令 `session-fork`：从某条消息处分叉出一个新会话 |
| `--session-interrupts` | ＝ 帧命令 `session-interrupts`：动一个会话之前，会打断什么 |
| `--session-new` | ＝ 帧命令 `session-new`：起一个新会话（全产品一个框、一个请求） |
| `--session-new-dir` | ＝ 帧命令 `session-new-dir`：起新会话前核一个目录：在不在 · 会用哪个终端名 |
| `--session-new-facts` | ＝ 帧命令 `session-new-facts`：起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法 |
| `--sessions-start` | ＝ 帧命令 `sessions-start`：起 / 接回一批会话（逐个答） |
| `--sessions-stop` | ＝ 帧命令 `sessions-stop`：停一批会话（逐个答，一个不成不挡下一个） |
| `--sessions-where` | ＝ 帧命令 `sessions-where`：一批会话各在这台哪个终端里 |
| `--skill-install-apply` | ＝ 帧命令 `skill-install-apply`：在要被写的那一台把勾的那几个 skill 文件写进去 |
| `--skill-install-plan` | ＝ 帧命令 `skill-install-plan`：在要被写的那一台判 skill 装不装得过来 |
| `--skill-install-record` | ＝ 帧命令 `skill-install-record`：skill 装记录的写口 |
| `--skill-read` | ＝ 帧命令 `skill-read`：读来源那台上的一个 skill |
| `--ssh-config-aliases` | ＝ 帧命令 `ssh-config-aliases`：这台 `~/.ssh/config` 里可点的别名 |
| `--ssh-config-import` | ＝ 帧命令 `ssh-config-import`：批量导入预览 |
| `--ssh-config-resolve` | ＝ 帧命令 `ssh-config-resolve`：一个别名的有效连接参数 |
| `--tasks-list` | ＝ 帧命令 `tasks-list`：一个会话的任务列表 |
| `--terminal-input` | ＝ 帧命令 `terminal-input`：往一个终端送字或送键（过身份门） |
| `--terminal-name-mint` | ＝ 帧命令 `terminal-name-mint`：起会话要的终端名 |
| `--terminal-preview` | ＝ 帧命令 `terminal-preview`：抓一个终端的一屏（只抓一次，轮询归调用方） |
| `--terminal-ssh` | ＝ 帧命令 `terminal-ssh`：给一台远端开终端要跑的那一串 |
| `--terminals-list` | ＝ 帧命令 `terminals-list`：这台的终端名单（形状与宿主无关；这一版宿主是 tmux） |
| `--tmux-notify` `<backend_pid> <backend_starttime>` | tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统 |

## 6. 一次性子命令的出入参类型

### `--resolve`（入参 `ResumeSpec` 从 stdin，出参 `CommandPlan` / `ResolveError`）

#### `ResumeSpec`

stdin 入参（camelCase 对齐 aterm `ResumeSpec`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sessionId` | string |  |
| `launchCandidates` | [string]? |  |
| `claudeDir` | string? | 冻结兼容字段，不许改名（与 `hello` 帧的 `claude_dir` 同族）：它是 `--resolve` 的 stdin 契约（线上是 `claudeDir`），与仓外 aterm 冻结对齐 |
| `fallbackCwd` | string? |  |
| `alreadyInTmux` | bool? |  |
| `agentKind` | string? | 会话属哪个 agent kind ⇒ 据此构 `codex resume <uuid>` 或 `claude --resume`（additive，线上 `agentKind`） |

#### `Capabilities`

stdout 出参 caps（4 名**逐字复用 aterm `SessionCapabilities`**，camelCase 免映射）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `supportsSendKeys` | bool |  |
| `supportsCapture` | bool |  |
| `supportsMultiClient` | bool |  |
| `supportsMultiWindow` | bool |  |

#### `CommandPlan`

stdout 出参（camelCase 对齐 aterm `ResumePlan`，另加 mode/capabilities）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `command` | string | **唯一可信的一项** |
| `mode` | string | "PtyInject"（resume 走 pty send-keys 注入 §5④）\| "ExecOnce"（未来） |
| `capabilities` | Capabilities | **典型档，不是探测结果**（见结构体头注） |
| `sessionName` | string? | **纯从 sid 派生，不是探测结果**（见结构体头注）——拿它去 attach 一个「并不存在」的 tmux 会话是现实风险 |
| `launchLabel` | string? |  |
| `substitutedFrom` | string? |  |

#### `ResolveError`

错误信封（exit 2 + stderr 出此 JSON）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `code` | string |  |
| `message` | string |  |

### `--fork-session` 的出参

#### `ForkResult`

成功时 stdout 输出的一行 JSON（camelCase，与 monitor 侧 `BranchResult` 同形）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sessionId` | string |  |
| `jsonlPath` | string |  |

### `--read-session-from-offset … --index` 的出参行

#### `IndexRow`

骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（第一格 · `§2.5b 路 D`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `o` | number | 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)` |
| `n` | number | 行字节长（**含**结尾 `\n`） |
| `t` | string? | 记录 `type`；解析不出（非 JSON / 没有 type）⇒ 省略 |
| `u` | string? | `uuid`（前端 `uuidToIdx` —— 跳转与对账的锚） |
| `sc` | bool? | 这条记录属于某个子运行（适配层 `RecordFace::run_of` 答得出） |
| `sp` | string? | user 记录是谁说的（`Speaker` 的 `kind`），人说的与工具结果省略（那两种按正文 / 折叠单元就分得清） |
| `ch` | number? | 正文字符数（**代码块之外**；Unicode 标量计，不含换行） |
| `cj` | number? | 其中 CJK/全宽字符数（口径 = `height-estimate.ts::fallbackTextHeight` 的 `> 0x2E80`） |
| `pl` | number? | 正文**非空**硬行数（代码块之外） |
| `cb` | number? | 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。 |
| `cl` | number? | 代码块内总行数 |
| `fd` | number? | 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块 |
| `x` | string? | 这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略 |
| `ts` | string? | 同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席） |
