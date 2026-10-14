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
| `emits` | [string]? | 本 backend **会发射的帧 kind 集** （snake_case，如 "session_status"/"turn_end"） |
| `commands` | [string]? | 本 backend **接受的入方向命令集** |
| `unavailable` | [Unavailable]? | 本 backend **接得下、但在这台机器上做不到**的命令，以及原因（`[{command, code}]`，见 `Unavailable`） |
| `host_env` | {string: string}? | **起我的宿主交给我的那几格环境，原样回显**（`{名: 值}`，名单 `HOST_ECHO_ENVS`） |
| `uncancellable` | [string]? | `commands` 里**撤不动**、又不是 typed 的那几条（阻塞档：开跑之后 `cancel` 回 `not_cancellable`） |
| `shapes` | {string: string}? | typed 命令（入参 · 回包 · 错误码都从 Rust 类型来的那几条）的形状指纹 `{命令: 8 位十六进制}`：这份二进制编进来的那张表（`cmd_wire::SHAPES`，由判据从类型现算） |

### `line`

会话记录里的一行 —— 带的是**成品**：这一行在界面里是什么（`record`，通用记录 `agents::record::Record`；缺 ＝ 不进界面、照占号）与它自己的 `cwd`。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `seq` | number | 这一行在本条流里的序号（按文件单调递增）；不是续传键，续传用 `byte_offset` |
| `record` | Record? | 这一行的通用记录；缺 ＝ 不进界面、照占号 |
| `cwd` | string? | 这条记录自己的工作目录 |
| `byte_offset` | number? | 本行末尾（含 `\n`）在文件中的**累计原始字节 offset**——语义**逐字节对齐手机端 `LineFramer.endOffset`**：计 CRLF 的 `\r`、含 `\n`、残行不计；resume N ⇒ `tail -c +(N+1)` |
| `rid` | string? | 这一行的对账键（适配层 `RecordFace::response_id` 给；流的「开始」带同一个值） |

### `session_added`

A new session file appeared。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `agent_kind` | string | 会话属哪一家（注册表里那一家的 kind：`"claude"` · `"codex"` …） |
| `background` | bool? | 是不是后台会话（不是人坐在终端里对话的那种） |
| `attachable` | bool? | attach 进去对人有没有意义 |
| `cwd` | string? | pidfile 记的 `cwd`：进程起在哪个目录（客户端认「我刚起的那条起来了」用） |
| `project_dir` | string? | 会话的项目目录：会话起在哪个目录（tab 标题 · 打开工作目录 · 分组都用它） |
| `name` | string? | pidfile 里的会话名 |
| `path` | string? | 该会话 jsonl 的远端绝对路径（同 sid 多文件时取 mtime 最新者）——monitor 旁路快照（`--read-session`）用 |
| `face` | ActivityFace | 宣告时此刻在干什么（适配层翻好的那一态 · 写好的字 · 语气 · 监控板的序，`ActivityFace`，平铺在帧上） |
| `name_face` | NameFace | 叫这条会话什么（`NameFace`，平铺在帧上） |
| `waiting_for` | string? | 宣告时在等什么（同 `session_status`） |
| `container` | SessionContainer? | 这条会话住在什么容器里（见 `SessionContainer`） |
| `pid` | number | 那个 claude 进程的 **pid**（恒在） |

### `session_primed`

这条会话的记录读完了（宣告之后后台那一趟预读做完）：从这一帧起它的 `line` 帧从第 `lines` 行起、连续地来。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `path` | string? | 读完的那份记录（修改时刻最新的那份） |
| `lines` | number? | 那份记录此刻的完整行数 L（之后的第一帧 `line` 的 `seq` 就是它） |

### `session_status`

会话 status 变化（pidfile modify diff；CC 仅状态转换时重写，天然稀疏）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `face` | ActivityFace | 此刻在干什么（同 `session_added` 那几格，`ActivityFace`） |
| `name_face` | NameFace | 叫这条会话什么（同 `session_added` 那两格；改了名、来了标题 / 第一句也发这一帧） |
| `waiting_for` | string? | 在等什么（pidfile 里的 `waitingFor`） |
| `path` | string? | 这条会话的记录路径（同 `session_added` 那一格）：宣告之后才定下来（会话刚起、记录还没写出来）· 换了一份 ⇒ 发一帧带上最新的；没变 ⇒ 不上线（别的原因发的这一帧字节与本字段加进来之前一字不差） |

### `session_state`

**会话账本的成品**：这条会话离开「活」之后是可重连还是已结束（见 `SessionFate`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `state` | SessionFate | 离开「活」之后的去向 |
| `state_text` | Words | 那一种写好的短名（`SessionFate::cells`） |
| `state_hint` | Words | 悬停那一句 |
| `state_tone` | Tone | 语气 |

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
| `title` | Words | 通知的标题（写好的字，桌面与手机照抄；机器名由看的那一端照标签页的规矩加在前面）：「本轮完成 · 会话名」，会话名与标签页那一格同一个（`NameFace`） |
| `body` | Words | 通知的正文（写好的字） |

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

### `changed`

**这台的某样东西变了，客户端重读那一份**（账号清单 · 配置文件 · 额度账 · 会话轮换 · 规则表 · 计划 · 任务清单 —— 主题表 `Topic`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `topic` | Topic | 哪一样变了 |
| `key` | string? | 哪一个（会话 id · 工作区根）；主题不带 ⇒ 缺 |
| `rev` | string? | 变成了哪一版；主题不带 ⇒ 缺 |
| `body` | JSON? | 那一样的小成品；主题不带或超了上限 ⇒ 缺（客户端重问） |

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

### `live_card`

**活卡**（`crate::stream::live` 出的成品）：某个会话某个运行里正在说的那一次上游应答，一张一张地开、长、撤。

（无字段）

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
| `at` | number | 前几条照旧 |
| `off` | [string] | 第 `at` 条起的那几条 `id`（之后没有了：清单到此为止） |
| `sum` | string | 清单前 `at + off.len()` 条的摘要（`branch_sum`）：客户端记下最后一帧的条数与它，在 `stream-watch` 的 `branch` 里原样交回，后端照它判手上那份前缀还对不对、只补缺的那一截（换一条连接也一样） |

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

- `gone` —— 真的没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind
- `superseded` —— 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**

#### `SessionContainer`

`Frame::SessionAdded` 的 `container`：**这条活着的会话住在什么容器里**，词与 `terminals-list` 同一套。

（手写序列化，形状见上面那一句。）

#### `SessionFate`

`Frame::SessionState` 的 `state`：**一条会话离开「活」之后是什么** ——那台机器的后端自己裁（`observe::session_ledger`：摘除原因 ＋ 它自己那份 tmux 快照），客户端只收成品、不再猜。

- `reconnectable` —— claude 退了、它的 tmux 会话还在（`@ccm_sid` 仍挂着它）⇒ 接得回去
- `ended` —— 进程没了、容器也没了（或被顶替了）⇒ 只能 resume

#### `ActivityFace`

**活会话此刻在干什么的那几格的唯一构造口**（`session_added` · `session_status` 都带它，平铺在帧上）：那一态（`activity`，说不清 ⇒ 不上线）· 写好的字 · 语气 · 监控板组内的序（`activity_order`，小的在前）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `activity` | SessionActivity? |  |
| `activity_text` | Words |  |
| `activity_tone` | Tone |  |
| `activity_order` | number |  |

#### `NameFace`

**这条会话叫什么那两格**（`session_added` · `session_status` 都带它，平铺在帧上；一轮结束的通知标题也读它）：从会话的那几样事实造它只有一处 —— `crate::history::search_rules::name_face`（`label` ＝ 同一个 `session_label`，与历史清单 · 全文搜索同一个函数； `label_project` ＝ 项目目录最后一截，界面拼「[项目] 名字」；不知道项目目录 ⇒ 不带）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `label` | Words |  |
| `label_project` | Words? |  |

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

#### `Topic`

推送主题（线上 `changed.topic`）。

- `accounts` —— 这台的账号清单（账号 manifest 被改写）
- `profiles` —— 这台的配置文件（`~/.cc-monitor/profiles.toml`）
- `quota` —— 这台的额度账显示得出来的那几格
- `rotation` —— 这台某个会话的轮换（`key` ＝ sid）：那一条 · 它用的那份策略 · 事实变了
- `rotation_strategies` —— 这台的策略目录 · 新会话用哪份 · 哪个会话用哪份
- `plan` —— 这台某个 pb 工作区的计划（`key` ＝ 工作区根，`rev` ＝ 新的输出摘要，与手上那一份相同 ⇒ 不用问； `body` ＝ `{needs}`：这个工作区此刻要你看的数，没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）
- `tasks` —— 这台某个会话的任务清单（`key` ＝ sid）
- `needs` —— 这台某个会话挂着的待答请求（`key` ＝ sid：挂上 · 撤掉 · 答掉；重问 `sessions-needs` 或那个会话的 `history-facts`）
- `login` —— 一份登录作业自己收了（`key` ＝ 作业号；`body` ＝ `{state: done \| failed, said?, detail?}`）
- `manual` —— 这台会话外的需手动清单可能变了（账号清单动了 · 一份登录作业收了 · 某个号记「MCP 要登录」的那份文件翻了；重问 `manual-list`）
- `machines` —— 这一端的机器表（`machines.json` 被改写；重问 `machines-read`）

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

#### `LiveCard`

`Frame::Live` 的体：哪个会话哪个运行的哪张卡、这条连接上它的第几帧、这一帧做什么。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `run` | string? | 归哪个子运行；缺 ＝ 主运行（主 tab 只画主运行的） |
| `card` | number | 卡号（本进程的段号，整台唯一） |
| `n` | number | 这条连接上这张卡的第几帧：`open` 从 0 起，之后连续 |
| `op` | LiveOp | 这一帧做什么 |

#### `LiveOp`

活卡的一帧做什么（`op` 是标签）。

- `op: "open"`，带 `rid` string · `phase` LivePhase · `clipped` bool · `blocks` [LiveBlock] —— 整张卡（新开 · 这条连接刚开始看这个会话 · 发件箱满过 · 正文截过一次头）：照它整张换
- `op: "block"`，带 `i` number · `block` BlockKind · `tool` string? —— 第 `i` 块开始了（种类叫 `block`：`kind` 已是帧的标签）
- `op: "text"`，带 `i` number · `s` string —— 第 `i` 块（文字块）接着一段字
- `op: "awaiting"` —— 上游说完了，等写入记录
- `op: "close"`，带 `why` LiveWhy —— 撤卡（为什么见 `LiveWhy`；客户端对哪种都一样：撤）

#### `LivePhase`

活卡此刻在哪一相。

- `streaming` —— 上游还在说
- `awaiting` —— 上游说完了，那一轮的记录还没到

#### `LiveBlock`

活卡里的一块。

| 字段 | 类型 | 说明 |
|---|---|---|
| `kind` | BlockKind |  |
| `text` | string? | 文字块已到的正文（只有文字块有；思考不展开、工具入参不显示） |
| `tool` | string? | 工具块的工具名 |

#### `LiveWhy`

为什么撤卡。

- `record` —— 那一轮的记录到了（整轮覆盖）
- `broken` —— 上游断在半路（Esc 打断 · 上游断 · 抄流那一跳缺了号）
- `error` —— 上游报错收尾
- `evicted` —— 满了被挤掉（同一运行 2 张 · 整台 16 张）
- `gone` —— 会话走了 / 子运行收场了
- `unwatched` —— 出了这条连接的名单

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
| `started_text` | string? | `started_ms` 在看的那一台钟上的钟面 `HH:MM`（推出去那一下按这条流的时区写，`Frame::stamp`；界面照抄、不换算） |
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

#### `RereadWhy`

`Frame::SessionFileReread` 的「为什么从头重读」。

- `truncated` —— 变短了（比读到过的最长还短）
- `rewritten` —— 没变短，但游标之前那一截被原地改写过（末尾指纹对不上）

#### `TransferEnd`

一趟传输怎么收场的（`Frame::Transfer` 的 `end`）。

- `state: "done"`，带 `bytes` number · `sha256` string? —— 传完了
- `state: "failed"`，带 `why` string · `code` string? · `detail` string? —— 失败（那一句 ＋ 复制详情）
- `state: "cancelled"` —— 撤了（`transfer-stop` / 本机流断了）

#### `WatchFrom`

`stream-watch` 应答里的一格：这个会话刚进了这条流的名单，它的这份记录从第 `seq` 行起上流（`seq` 与行帧同一个行号空间）；`[0, seq)` 那段客户端按骨架补（`history-index` 从上次的末端续）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `seq` | number | 这条流从这一行起发它 |

#### `WatchAsk`

`stream-watch` 的入参：在看哪几个（整份换）＋ 要哪几个的主线外清单、手上各有什么（`branch`；不报 ＝ 一份都不要）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sids` | [string] | 在看的会话 id（空 ＝ 一个都不看） |
| `branch` | [BranchHeld]? | 要哪几个会话的主线外清单、手上各有什么（可缺 ＝ 一份都不要） |

#### `BranchHeld`

`stream-watch.branch` 的一格：这条连接要这个会话的主线外清单；手上已有那份记录（`path`）的前 `at` 条、摘要 `sum`（上一帧 `session_branch` 给的）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string |  |
| `path` | string? |  |
| `at` | number? | 手上那份的前几条（照手上最后一帧 `session_branch` 的 `at + off.len()`；手上没有 ⇒ `0`） |
| `sum` | string? |  |

#### `WatchReply`

`stream-watch` 的应答。

| 字段 | 类型 | 说明 |
|---|---|---|
| `from` | [WatchFrom] | 刚进名单的会话每份记录一格（先前就在名单里的不回） |

#### `Request`

**入方向**请求信封。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 客户端发号的不透明串，应答原样回显；同一时刻在跑的命令里不许重复 |
| `cmd` | string | 命令名（`hello.commands` 里的一个） |
| `args` | JSON? | 命令的参数对象；缺 ＝ `null` |
| `within_ms` | number? | 发起方这一发愿意等多久（毫秒） |
| `view` | JSON? | 出口的声明（要哪几格 · 哪几格不要）；缺 ＝ `null` ＝ 全量 |
| `tz` | Tz? | 看的那一台的时区（IANA 名）：回包里「几点」「今天 / 昨天」按它写 |

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

入参 `CancelReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CancelReq = { 
/**
 * 要撤的那条命令的 `id`；撤不存在的也回 `ok`。
 */
target: string, };
```

码：`bad_args`

#### `stream-watch`

这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）：不在名单里的会话，它的行 · 运行表 · 主线外清单 · 活卡不上这条流（刚进名单的会话此刻每张活卡补一张整卡，出名单的撤卡），宣告 · 状态 · 轮次边沿照发；主线外清单只给 `branch` 里要了的会话、照手上那份只补缺的；回刚进名单的那几个从第几行起上流。

收 `args` · 连接内就地做完 · 只在流上

入参 `WatchAsk` · 回包 `WatchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BranchHeld = { sid: string, path?: string | null, 
/**
 * 手上那份的前几条（照手上最后一帧 `session_branch` 的 `at + off.len()`；手上没有 ⇒ `0`）。
 */
at?: number, sum?: string | null, };
type WatchAsk = { 
/**
 * 在看的会话 id（空 ＝ 一个都不看）。
 */
sids: Array<string>, 
/**
 * 要哪几个会话的主线外清单、手上各有什么（可缺 ＝ 一份都不要）。
 */
branch?: Array<BranchHeld>, };
type WatchFrom = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 这份会话记录在那台机器上的绝对路径。
 */
path: string, 
/**
 * 这条流从这一行起发它。
 */
seq: number, };
type WatchReply = { 
/**
 * 刚进名单的会话每份记录一格（先前就在名单里的不回）。
 */
from: Array<WatchFrom>, };
```

码：`bad_args`

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

入参 `TransferUploadReq` · 回包 `TransferUploaded`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type TransferUploadReq = { 
/**
 * 拨号请求（同 `link-open` 的 `dial`）：后端拨号核心读它。
 */
dial: Record<string, unknown>, 
/**
 * 本机一份普通文件的路径。
 */
local_path: string, 
/**
 * 那台后端的 `$HOME`（同 [`TransferUploadAsk::home`]）。
 */
home?: string, };
type TransferUploaded = { 
/**
 * 传输单 id（`xfer-<n>`）：订阅 `transfer/<id>` 即起跑。
 */
id: string, 
/**
 * 暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端后端。
 */
key: string, };
```

码：`bad_args` · `io_failed` · `busy` · `too_many_transfers`

#### `transfer-download`

开一张下载单。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferDownloadReq` · 回包 `TransferOpened`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type TransferDownloadReq = { 
/**
 * 拨号请求（同 [`TransferUploadReq::dial`]）。
 */
dial: Record<string, unknown>, 
/**
 * 远端路径。
 */
remote_path: string, 
/**
 * 本机落点（同 [`TransferDownloadAsk::local_path`]）：开单时当场过路径解析，起跑时再过一次。
 */
local_path: WirePath, };
type TransferOpened = { 
/**
 * 传输单 id：订阅 `transfer/<id>` 即起跑。
 */
id: string, };
type WirePath = string | { b16: string };
```

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

入参 `NoArgs` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

### 4.2 文件管理

#### `files-create`

在文件管理目标根底下新建一份此前不存在的文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-create`

入参 `FilesCreateReq` · 回包 `FilesPathBytesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesCreateReq = { root: WirePath, rel: WirePath, content?: WirePath, 
/**
 * `true` ⇒ `rel` 只许是一段名字（界面就地新建 / 改名敲的那一格）。
 */
single?: boolean, };
type FilesPathBytesReply = { path: WirePath, bytes: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-mkdir`

新建一个目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-mkdir`

入参 `FilesMkdirReq` · 回包 `FilesPathReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesMkdirReq = { root: WirePath, rel: WirePath, single?: boolean, };
type FilesPathReply = { path: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-rename`

改名 / 同根内移动。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-rename`

入参 `FilesRenameReq` · 回包 `FilesPathReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesPathReply = { path: WirePath, };
type FilesRenameReq = { root: WirePath, from: WirePath, to: WirePath, single?: boolean, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_name` · `bad_path` · `exists` · `io_failed` · `refused`

#### `files-delete`

删一个文件或一个空目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-delete`

入参 `FilesDeleteReq` · 回包 `FilesDeleteReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DeleteExpect = { empty_dir: true } | string | { b16: string };
type FilesDeleteReply = { path: WirePath, removed: number, remaining: number, };
type FilesDeleteReq = { root: WirePath, rel: WirePath, recursive?: boolean, expect?: DeleteExpect, limit?: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-chmod`

改 unix 权限位。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-chmod` · 没有 unix 权限位的机器上做不到

入参 `FilesChmodReq` · 回包 `FilesChmodReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesChmodReply = { path: WirePath, mode: number, before?: number, };
type FilesChmodReq = { root: WirePath, rel: WirePath, mode: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `no_unix_mode` · `refused`

#### `files-copy`

同根内复制一份普通文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-copy`

入参 `FilesCopyReq` · 回包 `FilesTreeReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesCopyReq = { root: WirePath, from: WirePath, to: WirePath, overwrite?: boolean, recursive?: boolean, };
type FilesTreeReply = { path: WirePath, bytes: number, files: number, dirs: number, links: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused`

#### `files-extract`

解压到一个新目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-extract`

入参 `FilesExtractReq` · 回包 `FilesTreeReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesExtractReq = { root: WirePath, rel: WirePath, fresh?: boolean, };
type FilesTreeReply = { path: WirePath, bytes: number, files: number, dirs: number, links: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `exists` · `io_failed` · `refused` · `unsupported`

#### `files-link`

建一条符号链接。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-link`

入参 `FilesLinkReq` · 回包 `FilesPathReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesLinkReq = { root: WirePath, rel: WirePath, target: WirePath, };
type FilesPathReply = { path: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused`

#### `files-write-text`

覆盖写一份已经在的普通文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-write-text`

入参 `FilesWriteTextReq` · 回包 `FilesWrittenReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesWriteTextReq = { root: WirePath, rel: WirePath, content: WirePath, expect: Sha256Expect, };
type FilesWrittenReply = { path: WirePath, bytes: number, sha256: Sha256Hex, };
type Sha256Expect = { sha256: Sha256Hex, };
type Sha256Hex = string;
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-peek`

读改写的读那一半。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-peek`

入参 `FilesPeekReq` · 回包 `FilesPeekReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesPeekReply = { path: WirePath, exists: boolean, text: string | null, };
type FilesPeekReq = { root: WirePath, rel: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `not_text` · `refused` · `too_large`

#### `files-put`

整份替换一份文本文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-put`

入参 `FilesPutReq` · 回包 `FilesPutReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesPutReply = { path: WirePath, bytes: number, changed: boolean, created: boolean, backup: WirePath | null, };
type FilesPutReq = { root: WirePath, rel: WirePath, content: WirePath, expect: WirePath | null, backup?: boolean, parents?: boolean, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-delete-session`

删一份历史会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-delete-session`

入参 `FilesDeleteSessionReq` · 回包 `FilesPathReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesDeleteSessionReq = { sid: string, };
type FilesPathReply = { path: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `io_failed` · `refused`

#### `files-stage-chunk`

存盘的一块进暂存区。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-stage-chunk`

入参 `FilesStageChunkReq` · 回包 `FilesStageChunkReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ChunkBytes = string | { b16: string };
type FilesStageChunkReply = { bytes: number, };
type FilesStageChunkReq = { key: string, seq: number, content: ChunkBytes, };
```

码：`bad_args` · `io_failed` · `refused`

#### `files-commit-text`

按块读回、拼起来、覆盖写。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-commit-text`

入参 `FilesCommitTextReq` · 回包 `FilesWrittenReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesCommitTextReq = { root: WirePath, rel: WirePath, key: string, chunks: number, bytes: number, expect: Sha256Expect, };
type FilesWrittenReply = { path: WirePath, bytes: number, sha256: Sha256Hex, };
type Sha256Expect = { sha256: Sha256Hex, };
type Sha256Hex = string;
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-commit-upload`

把暂存区里一份传完的上传件挪进目标。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-commit-upload`

入参 `FilesCommitUploadReq` · 回包 `FilesPathBytesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesCommitUploadReq = { root: WirePath, rel: WirePath, key: string, overwrite: boolean, expect: Sha256Expect, chunks?: number, bytes?: number, };
type FilesPathBytesReply = { path: WirePath, bytes: number, };
type Sha256Expect = { sha256: Sha256Hex, };
type Sha256Hex = string;
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `io_failed` · `refused` · `stale`

#### `files-browse`

告诉后端「用户现在在看哪几个目录」。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-browse`

入参 `BrowseReq` · 回包 `BrowseReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BrowseReply = { added: number, removed: number, 
/**
 * 超过名单上限没收的。
 */
rejected: number, browse_watch_cap: number, 
/**
 * 监听器真挂上了几个。
 */
watching: number, watch_failed: number, 
/**
 * 没挂上的那一句原话。
 */
watch_error: string | null, };
type BrowseReq = { dirs: Array<WirePath>, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path`

#### `files-index-rebuild`

重建常驻文件名索引：走一遍，做完返回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-index-rebuild`

入参 `RebuildReq` · 回包 `RebuildReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RebuildReply = { path: WirePath, entries: number, resident_bytes: number, unreadable_dirs: number, unreadable_paths: Array<WirePath>, truncated: boolean, skipped_mounts: number, };
type RebuildReq = { path?: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `already_rebuilding` · `bad_path` · `no_home` · `unreadable`

#### `files-grep`

在一个目录底下按内容搜。

收 `args` · 可撤 · CLI：`ccm -- --files-grep`

入参 `GrepReq` · 回包 `GrepReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type GrepHit = { path: WirePath, 
/**
 * 相对搜索起点的那一段。
 */
rel: WirePath, line: number, text: WirePath, 
/**
 * 这份文件里一共命中几处。
 */
matches: number, lines: Array<GrepLine>, };
type GrepLine = { line: number, text: WirePath, 
/**
 * 命中那一段的字节区间（至多一段）。
 */
marks: Array<[number, number]>, };
type GrepReply = { path: WirePath, hits: Array<GrepHit>, files: number, bytes: number, links: number, skipped_binary: number, skipped_large: number, skipped_mounts: number, unreadable: number, 
/**
 * 这一趟用的命中上限。
 */
limit: number, truncated: boolean, 
/**
 * 没搜完就停了的原因；搜完了（或被撤）⇒ `null`。
 */
stopped: GrepStopped | null, };
type GrepReq = { path: WirePath, 
/**
 * 要找的那一串（1..=256 字节）。
 */
needle: WirePath, ignore_ascii_case?: boolean, 
/**
 * 至多几条命中（缺 · 0 ⇒ 后端默认；超上限按上限）。
 */
limit?: number, };
type GrepStopped = "hits" | "bytes";
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `unreadable`

#### `files-find`

在常驻索引里查。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-find`

入参 `FindReq` · 回包 `FindReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileKind = "dir" | "file" | "symlink" | "other";
type FilesFindHit = { path: WirePath, kind: FileKind, 
/**
 * 相对搜索起点的那一段（所在目录）。
 */
location: WirePath, size: number | null, mtime_secs: number | null, mtime_text: string | null, 
/**
 * 命中段的字节区间。
 */
marks: Array<[number, number]>, };
type FindReply = { hits: Array<FilesFindHit>, 
/**
 * 这一趟从哪儿搜起（缺 `under` ⇒ 家目录；都说不出 ⇒ `null`）。
 */
start: WirePath | null, sort: FindSort, desc: boolean, total_hits: number, truncated: boolean, scanned: number, index_age_secs: number, index_missing: boolean, stale: boolean, index_root: WirePath | null, 
/**
 * 搜的那一处在索引覆盖之外。
 */
out_of_index: boolean, cover_root: WirePath | null, seq: number | null, offset: number, };
type FindReq = { 
/**
 * 窗口原样发来的搜索词。
 */
query: string, under?: WirePath, scope?: FindScope, 
/**
 * 同一个搜索框的第几趟（新号到了，旧号收手）。
 */
seq?: number, 
/**
 * 哪一个搜索框。
 */
stream?: string, offset?: number, limit?: number, sort?: FindSort, desc?: boolean, };
type FindScope = "under" | "machine";
type FindSort = "relevance" | "name" | "location" | "mtime" | "size";
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `bad_query` · `superseded`

#### `files-index-status`

索引的新鲜度 / 条目数 / 常驻字节。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-index-status`

入参 `IndexStatusReq` · 回包 `IndexStatusReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type IndexStatusReply = { index_missing: boolean, entries: number, resident_bytes: number, unreadable_dirs: number, 
/**
 * 读不进去的那几个目录（前 20 个）。
 */
unreadable_paths: Array<WirePath>, truncated: boolean, 
/**
 * 没走进去的挂载点个数。
 */
skipped_mounts: number, age_secs: number, rewalk_interval_secs: number, stale: boolean, browse_watches: number, browse_watch_cap: number, cold_first_build_secs: number, };
type IndexStatusReq = Record<symbol, never>;
type WirePath = string | { b16: string };
```

码：`bad_args`

#### `files-ls`

列一个目录的直接子项。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-ls`

入参 `LsReq` · 回包 `LsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileKind = "dir" | "file" | "symlink" | "other";
type LinkTo = "dir" | "file" | "missing";
type LsEntry = { path: WirePath, kind: FileKind, size?: number, mtime_secs?: number, 
/**
 * 那台按本地钟写好的短写法。
 */
mtime_text?: string, 
/**
 * 那台按本地钟写好的完整写法。
 */
mtime_full?: string, 
/**
 * 链接指向的是不是目录（只在跟得过去的链接上）。
 */
link_dir?: boolean, 
/**
 * 链接指向哪一种（只在链接上）。
 */
link_to?: LinkTo, };
type LsReply = { entries: Array<LsEntry>, 
/**
 * 回送的比读到的少（被 `limit` 截了）。
 */
truncated: boolean, 
/**
 * 一共读到几项（含没回送的）。
 */
total: number, 
/**
 * 有几项读不出来（照数，不静默少一项）。
 */
unreadable: number, };
type LsReq = { path: WirePath, 
/**
 * 至多回几条（缺 · 0 ⇒ 后端默认 1000）。
 */
limit?: number, 
/**
 * `true` ⇒ 只列文件夹（含指向目录的链接）；截断与计数都只数文件夹。
 */
dirs_only?: boolean, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `denied` · `not_dir` · `not_found` · `unreadable`

#### `files-stat`

一个路径的元数据。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-stat`

入参 `StatReq` · 回包 `StatReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileKind = "dir" | "file" | "symlink" | "other";
type StatReply = { path: WirePath, kind: FileKind, size: number, readonly: boolean, 
/**
 * unix 权限位低 12 位；非 unix 缺席。
 */
mode?: number, mtime_secs?: number, mtime_text?: string, mtime_full?: string, 
/**
 * 属主名（查不到名字给 uid 数字串）；非 unix ⇒ `null`。
 */
owner: string | null, 
/**
 * 路径本身是链接 ⇒ 目标原文；否 ⇒ `null`。
 */
link_target: WirePath | null, };
type StatReq = { path: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `unreadable`

#### `files-read-text`

读一份文本进编辑器。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-read-text`

入参 `ReadTextReq` · 回包 `ReadTextReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ReadTextReply = { path: WirePath, text: string, bytes: number, 
/**
 * 读到的那一份字节的 SHA-256（小写十六进制）：存回时交回去当 `expect`。
 */
sha256: string, };
type ReadTextReq = { path: WirePath, 
/**
 * 这一趟许读的上限（每次都要给，1..=8 MiB）。
 */
max_bytes: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `not_text` · `too_large` · `unreadable`

#### `files-read-chunk`

按字节寻址分块读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-read-chunk`

入参 `ReadChunkReq` · 回包 `ReadChunkReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HexBytes = { b16: string };
type ReadChunkReply = { path: WirePath, offset: number, 
/**
 * 整份文件多大。
 */
size: number, eof: boolean, content: HexBytes, };
type ReadChunkReq = { path: WirePath, offset: number, len: number, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `not_text` · `unreadable`

#### `files-size`

算一个目录有多大。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-size`

入参 `SizeReq` · 回包 `SizeReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SizeReply = { path: WirePath, bytes: number, files: number, dirs: number, links: number, other: number, skipped_mounts: number, unreadable_dirs: number, };
type SizeReq = { path: WirePath, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `unreadable`

#### `files-home`

后端这个用户的 home。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-home`

入参 `HomeReq` · 回包 `FilesPathReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesPathReply = { path: WirePath, };
type HomeReq = Record<symbol, never>;
type WirePath = string | { b16: string };
```

码：`bad_args` · `no_home`

### 4.3 账号

#### `quota-read`

这台的额度账。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --quota-read`

入参 `NoArgs` · 回包 `QuotaRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LoginState = "ok" | "needsLogin" | "needsKey";
type NoArgs = Record<symbol, never>;
type QuotaAccount = { rows: Array<Array<QuotaCell>>, 
/**
 * 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）；按量号 ⇒ `null`。
 */
fiveHour: Words | null, warm: QuotaWarm, agent: string, account: string, seenAt: number, seenAtText: Words, 
/**
 * 别台的钟走在前面时才有。
 */
seenAtRelText?: Words, 
/**
 * 那一条账的额度快照（原数照原样；时刻旁边的字已添）。
 */
reading: QuotaReadingShown, 
/**
 * 各窗口几点、从哪看到的（原数照原样；账上没记 ⇒ 缺）。
 */
windowsSeen?: { [key in string]: QuotaWindowSeen }, kind: QuotaKind, state: QuotaState, 
/**
 * 最后一次看到距今超过 30 分钟（与 `state` 叠着画）。
 */
stale: boolean, 
/**
 * 按钮上那个窗口的语义位（卡着的那个；回包没说 ⇒ 用得最多的那个）；没有分窗口的数 ⇒ 缺。
 */
limiting?: string, 
/**
 * `5h` · `7d` 各一格（有数的才出）。
 */
slots: Array<SlotShow>, login: LoginState, 
/**
 * 同一订阅的稳定标识（账号身份的散列，不含原值）：两台看到的同一订阅它相同。订阅号读得出身份才有。
 */
subId?: string, 
/**
 * 各窗口照原名一格（`slots` 那两格照留：同一语义位几个窗口并成一格；这里一个窗口一格）；没有 ⇒ 缺。
 */
windows?: Array<WindowShow>, 
/**
 * 这个号用量那一格（状态栏按钮 · 切号下拉 · 「下一个」那一格同一份）：[`usage_of`] 写。
 */
usage: Usage, };
type QuotaCandidate = { agent: string, account: string, kind: QuotaKind, };
type QuotaCell = { text: Words, tone: Tone, };
type QuotaEarliestReturn = { account: string, at: number, atText: Words, atRelText?: Words, };
type QuotaKind = "sub" | "api";
type QuotaNames = { accounts: { [key in string]: Words }, slots: { [key in string]: Words }, };
type QuotaOverageShown = { status?: QuotaStatus, 
/**
 * 为什么不可用（那一家的原值）。
 */
disabled?: string, 
/**
 * 这一发正在用超额。
 */
inUse: boolean, 
/**
 * 几点重置（unix 秒）。
 */
resetsAt?: number, resetsAtText?: Words, resetsAtRelText?: Words, };
type QuotaRead = { 
/**
 * 那份文件读到的三态。
 */
state: QuotaReadState, 
/**
 * 只在 `unreadable` 时有：为什么读不出来（那一句不带原话）。
 */
reason: string | null, 
/**
 * 只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
 */
detail: string | null, 
/**
 * 那份文件的绝对路径（家推不出来时 `null`）。
 */
path: string | null, 
/**
 * 这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）。
 */
now: number, 
/**
 * 每个出过数的号一条，按 `(agent, account)` 排。
 */
accounts: Array<QuotaAccount>, 
/**
 * 账号库里有、额度账上从没出过数的号。
 */
unseen: Array<QuotaUnseen>, 
/**
 * 此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜。
 */
usableNow: Array<string>, 
/**
 * 换号 · 加号那几处的候选：出过数的在前、没出过的在后，`_` 不在里面（[`is_candidate`]）。界面只照它画，不另拼、不另滤。
 */
candidates: Array<QuotaCandidate>, 
/**
 * 被拒 / 超额在兜的号里最早回来的那个；没有、或都说不出时刻 ⇒ `null`。
 */
earliestReturn: QuotaEarliestReturn | null, 
/**
 * 读不出 / 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`（[`quota_rows::head_text`]）。
 */
text: Words | null, 
/**
 * 读不出 ⇒「5h 那一格」写好的字（`5h 读不到`，[`quota_rows::head_five_hour`]）；否则 `null`（每号自己那一格）。
 */
fiveHour: Words | null, 
/**
 * 号名 / 位名表（出口画号名 · 位名只照它）。
 */
names: QuotaNames, };
type QuotaReadState = "present" | "absent" | "unreadable";
type QuotaReadingShown = { 
/**
 * 限流器的结论；回包里没说 ⇒ 缺。
 */
status?: QuotaStatus, 
/**
 * 那一发被上游拒了（没被服务）。
 */
refused: boolean, 
/**
 * 此刻卡着的那个窗口名。
 */
limiting?: string, windows: Array<QuotaWindowRead>, overage?: QuotaOverageShown, 
/**
 * 几点重置（unix 秒）。
 */
resetsAt?: number, resetsAtText?: Words, resetsAtRelText?: Words, };
type QuotaSource = "headers" | "usage";
type QuotaState = "ok" | "near" | "refused" | "overageInUse" | "unseen" | "resetSinceSeen";
type QuotaStatus = "allowed" | "warning" | "rejected";
type QuotaUnseen = { rows: Array<Array<QuotaCell>>, 
/**
 * 没出过数 ⇒ 恒 `null`（这一格只给出过数的订阅号）。
 */
fiveHour: Words | null, warm: QuotaWarm, agent: string, account: string, kind: QuotaKind, login: LoginState, subId?: string, 
/**
 * 用量那一格（没出过数 ⇒ `5h —` / `按量`；同出过数的号那一格一个写法，只是没有重置那一格）。
 */
usage: QuotaUnseenUsage, };
type QuotaUnseenUsage = { slot?: string, window?: Words, value: Words, text: Words, tone: Tone, };
type QuotaWarm = { act: QuotaWarmAct, at?: number, 
/**
 * `at` 写给人看的样子（[`Warm::stamp`]）。
 */
atText?: Words, 
/**
 * `at` 距今。
 */
atRelText?: Words, 
/**
 * 为什么这样办（「5h 83% ↻14:20」「被拒（7d）↻10-12 09:00」「无采样 · 发一句才知道」…）。
 */
text: Words, };
type QuotaWarmAct = "send" | "wait";
type QuotaWindowRead = { name: string, 
/**
 * 已用比例（通常 0–1，可以超过 1）。
 */
used?: number, 
/**
 * 越过了哪条预警线（比例）；没越过就没有。
 */
warnedAt?: number, 
/**
 * 几点重置（unix 秒）。
 */
resetsAt?: number, resetsAtText?: Words, resetsAtRelText?: Words, };
type QuotaWindowSeen = { at: number, atText: Words, atRelText?: Words, from: QuotaSource, };
type SlotShow = { slot: string, pct?: number, resetsAt?: number, 
/**
 * `resets_at` 写给人看的样子（出口那一下按钟写，[`SlotShow::stamp`]；内部与记账一律不填）。
 */
resetsAtText?: string, 
/**
 * `resets_at` 距今（`+1h50m`；同上，只在还没到时有）。
 */
resetsAtRelText?: string, 
/**
 * 用满：这个窗口用到 100%、还没重置（画 `✕`；被拒而没用满画「{pct}% · 被拒」）。没用满 ⇒ 缺。
 */
full?: boolean, 
/**
 * 这一格的字（唯一写法 [`slot_words`]：超额在兜 `超额` · 上一窗已过 / 没数 `—` · 用满 `✕` · 被拒没用满 `58% · 被拒` · 否则 `58%`）。
 */
text: string, 
/**
 * 那一格的语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 卡着的窗口快满 ⇒ `warn`；否则 `plain`。
 */
tone: "plain" | "fail" | "warn", };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type Usage = { 
/**
 * 画的是哪个语义位（卡着的那个；回包没说 ⇒ `5h`）；按量号 ⇒ 缺。
 */
slot?: string, 
/**
 * 那个语义位的字（`5h`）；按量号 ⇒ 缺。
 */
window?: string, 
/**
 * 用量那一格（`63%` · `✕` · `超额` · `—` · `被拒` · `按量`）。
 */
value: string, 
/**
 * 被拒时几点重置（`↻19:00`，出口那一下按钟写，[`QuotaShow::stamp`]）；没有 ⇒ 缺。
 */
reset?: string, 
/**
 * 连成一句（`5h 63%` · `5h ✕ ↻19:00` · `按量`）：下拉项右侧 · 切换那颗按钮。
 */
text: string, 
/**
 * 语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 快满 ⇒ `warn`；否则 `plain`。
 */
tone: "plain" | "fail" | "warn", };
type WindowShow = { 
/**
 * 那一家的窗口名（`five_hour` · `seven_day` · `seven_day_<模型>` · …）。
 */
name: string, 
/**
 * 窗口键（`5h` · `7d` · `7d:<模型>`；轮换的上限 · 单段预算按它配）；不算用量窗口的（超额）⇒ 缺。
 */
key?: string, 
/**
 * 取整的百分比（已重置未计时 ⇒ 0）。
 */
pct?: number, 
/**
 * 几点重置；有 ＝ 这个窗口在计时。
 */
resetsAt?: number, 
/**
 * `resets_at` 写给人看的样子（出口那一下按钟写，[`WindowShow::stamp`]；内部与记账一律不填）。
 */
resetsAtText?: string, 
/**
 * `resets_at` 距今（同上，只在还没到时有）。
 */
resetsAtRelText?: string, seenAt: number, 
/**
 * `seen_at` 写给人看的样子（同上）。
 */
seenAtText?: string, 
/**
 * `seen_at` 距今（别台的钟走在前面时才有）。
 */
seenAtRelText?: string, from: QuotaSource, 
/**
 * 重置时刻已过、之后没再看到：上次的数不再作数（用量当 0、窗口没开）。
 */
resetSinceSeen?: boolean, };
type Words = string;
```

码：`bad_args`

#### `quota-probe`

用某个号查一次额度。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 32 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --quota-probe`

入参 `ProbeReq` · 回包 `Probed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProbeFrom = "usage";
type ProbeReq = { 
/**
 * 路由第 1 段：哪一家。
 */
agent: string, 
/**
 * 账号库里的号（配置目录末段；账号 0 是 `0`）。
 */
account: string, };
type ProbeState = "read" | "unreadable";
type Probed = { 
/**
 * 路由第 1 段：哪一家（同入参）。
 */
agent: string, 
/**
 * 账号库里的号（同入参）。
 */
account: string, 
/**
 * 这份读数从哪来。
 */
from: ProbeFrom, 
/**
 * 这台此刻的 unix 秒。
 */
now: number, 
/**
 * 读得懂（已记进额度账）· 输出对不上（不猜、不写账）。
 */
state: ProbeState, 
/**
 * 只在 `unreadable` 时有：哪一处读不懂（英文诊断）；`read` ⇒ `null`。
 */
reason: string | null, 
/**
 * 读到的窗口（形状同 `reading.windows`）；`unreadable` ⇒ `[]`。
 */
windows: Array<QuotaWindow>, 
/**
 * 额度账那份文件的绝对路径（家推不出来时 `null`）。
 */
path: string | null, };
type QuotaWindow = { 
/**
 * 窗口名（那一家读法表里的值，如 5 小时 · 7 天）。
 */
name: string, 
/**
 * 已用比例（通常 0–1，可以超过 1）。
 */
used?: number, 
/**
 * 几点重置（unix 秒）。
 */
resetsAt?: number, 
/**
 * 越过了哪条预警线（比例）；没越过就没有。
 */
warnedAt?: number, };
```

码：`bad_args` · `child_timed_out` · `failed` · `io_failed` · `not_found` · `unsupported`

#### `rotation-facts`

每个号此刻的事实。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-facts`

入参 `RotationFactsReq` · 回包 `RotationFactsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Fact = { 
/**
 * 路由第 2 段。
 */
account: string, kind: QuotaKind, w5h: WinFact, w7d: WinFact, login: LoginState, 
/**
 * 额度账上最后一次看到它（没出过数 ⇒ 缺）。
 */
seenAt?: number, 
/**
 * 事实上用不了 ⇒ 有；能用 ⇒ 缺。
 */
blocked?: FactBlocked, };
type FactBlocked = { why: FactWhy, until?: number, text: string, };
type FactWhy = "refused" | "overage" | "used" | "login" | "key";
type LoginState = "ok" | "needsLogin" | "needsKey";
type QuotaKind = "sub" | "api";
type RotationFactsReply = { 
/**
 * 这台此刻（unix 秒）。
 */
now: number, 
/**
 * 照账号库的先后。
 */
accounts: Array<Fact>, };
type RotationFactsReq = { 
/**
 * 哪一家（适配器 id）；缺 ⇒ 有账号库的那一家。
 */
agent?: string, };
type WinFact = { open: boolean, used: number, resets?: number, 
/**
 * `resets` 写给人看的样子（按看的那一台的时区）。
 */
resetsText?: string, rate?: number, };
```

码：`bad_args`

#### `rotation-try`

试跑一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-try`

入参 `RotationTryReq` · 回包 `RotationTryReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotTryDecision = { 
/**
 * 用哪个号（停发 ⇒ 缺）。
 */
pick?: string, 
/**
 * 停发到几点 · 到时恢复的号（停发才有）。
 */
holdUntil?: number, holdAccount?: string, 
/**
 * 策略交的先后（此刻愿意用的号）。
 */
order: Array<string>, 
/**
 * 每个号一句（策略写的）。
 */
why: Array<RotTryRow>, 
/**
 * 这一号来自用户拖出来的顺序。
 */
fromTemp: boolean, skipped: Array<string>, temp: Array<string>, nextNode?: number, 
/**
 * 策略这一判出错（走了兜底）：原话。
 */
strategyError?: string, };
type RotTryLane = { account: string, segs: Array<RotTryLaneSeg>, };
type RotTryLaneSeg = { from: number, to: number, 
/**
 * 事实上发得出去吗（被拒 · 登录 · key · 超额 · 用到 100 都算发不出去）。
 */
sendable: boolean, 
/**
 * 这一段开头两窗的用量（5 小时 · 7 天）。
 */
used: [number, number], reset: boolean, 
/**
 * 这一段起点的钟面（`HH:MM`，看的那一台的时区；重置那段写成 `↻18:30`，字由界面照文案表配）。
 */
fromText: string, 
/**
 * 这一段开头剩多少（事实：100 − 两窗里用得多的那个，不低于 0）。
 */
left: number, };
type RotTryRow = { account: string, why: string, };
type RotTrySwitch = { at: number, 
/**
 * `at` 的钟面（`HH:MM`）。
 */
atText: string, account: string, };
type RotTryTick = { at: number, text: string, };
type RotTryTimeline = { 
/**
 * 将来每次换到别的号（时间轴上方那几个「16:07 work」）。
 */
switches: Array<RotTrySwitch>, uses: Array<RotTryUse>, lanes: Array<RotTryLane>, truncated: boolean, 
/**
 * 此刻之后的整点刻度（推 12 小时以内每 2 小时、更长每 4 小时，看的那一台的钟）。
 */
ticks: Array<RotTryTick>, };
type RotTryUse = { from: number, to: number, account?: string, fromTemp: boolean, 
/**
 * 在用的号用到上限的时刻还不知道（额度账里量不到它的用速）：这一段只排确定的事件。
 */
capUnknown: boolean, skipped: Array<string>, };
type RotationTryReply = { now: number, 
/**
 * 这份策略的说明（原文开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段）。
 */
description: string, 
/**
 * 给了 `settings` ⇒ 换上这份设置以后的整份原文（头重写、代码原样），存成文件就是它；没给 ⇒ 缺。
 */
source?: string, 
/**
 * 试跑「现在用」碰上用不了：几点恢复（说不出 ⇒ `null`）；这时 `decision` 是不换的那一判。
 */
useNowRefused?: number | null, decision: RotTryDecision, timeline: RotTryTimeline, };
type RotationTryReq = { 
/**
 * 一份策略文件的原文（头 ＋ 代码）。
 */
source: string, 
/**
 * 换掉头里的设置（形状由这份策略自己定；缺 ⇒ 用头里的）。
 */
settings?: unknown, 
/**
 * 开会话用的号（缺 ⇒ 事实里第一个号）。
 */
start?: string, 
/**
 * 在用的号（缺 ⇒ 开会话用的号）。
 */
current?: string, 
/**
 * 临时条目。
 */
temp: Array<string>, 
/**
 * 往后推几小时（1–48，缺 ⇒ 12）。
 */
hours?: number, 
/**
 * 试跑「现在用这个号」：事实上发得出去 ⇒ 判的是换过去、放进拖出来的顺序最前之后的那一判；发不出去 ⇒ `useNowRefused` 带几点恢复。
 */
useNow?: string, 
/**
 * 哪一家（缺 ⇒ 有账号库的那一家）。
 */
agent?: string, };
```

码：`bad_args` · `bad_strategy`

#### `rotation-strategies`

策略目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategies`

入参 `RotationStrategiesReq` · 回包 `RotationStrategiesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotStrategyError = { line?: number, msg: string, };
type RotStrategyItem = { id: string, 
/**
 * 说明（文件开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段；读不懂 ⇒ 空）。标题就是 `id`（文件名，§2.1）。
 */
description: string, rev: string, error?: RotStrategyError, 
/**
 * 此刻还活着、用着它的会话。
 */
inUse: Array<string>, forNew: boolean, 
/**
 * 这份的设置说明写法不合文案规矩（设置照样能用）。
 */
wordsBad: boolean, 
/**
 * 文件头里没写时区（分时段按 UTC 算）。
 */
tzUnknown: boolean, };
type RotationStrategiesReply = { 
/**
 * 照 id 排。
 */
strategies: Array<RotStrategyItem>, forNew: string, 
/**
 * `strategies/v3/` 里没有加载的第三版脚本几份。
 */
oldScripts: number, };
type RotationStrategiesReq = Record<symbol, never>;
```

码：`bad_args`

#### `rotation-view`

一个会话或一份策略此刻的轮换。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-view`

入参 `RotationViewReq` · 回包 `RotationViewReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Fact = { 
/**
 * 路由第 2 段。
 */
account: string, kind: QuotaKind, w5h: WinFact, w7d: WinFact, login: LoginState, 
/**
 * 额度账上最后一次看到它（没出过数 ⇒ 缺）。
 */
seenAt?: number, 
/**
 * 事实上用不了 ⇒ 有；能用 ⇒ 缺。
 */
blocked?: FactBlocked, };
type FactBlocked = { why: FactWhy, until?: number, text: string, };
type FactWhy = "refused" | "overage" | "used" | "login" | "key";
type LoginState = "ok" | "needsLogin" | "needsKey";
type QuotaKind = "sub" | "api";
type RotBadCell = { cell: string, code: string, };
type RotFormCell = { key: string, kind: RotFormKind, scope: RotFormScope, 
/**
 * 只对 `scope: each`：号没另设就跟总体（总体那一份在 `settings[key]`）。
 */
inherit?: boolean, label: StrategyText, help?: StrategyText, 
/**
 * 只对 `daySegments`：哪几个窗口（`5h` · `7d`）。
 */
windows?: Array<string>, min?: number, max?: number, unit?: string, default: unknown, choices?: Array<RotFormChoice>, 
/**
 * 能是空（`null`）：空的那一项怎么叫（例如「不回切」）。
 */
nullable?: StrategyText, };
type RotFormChoice = { v: string, label: StrategyText, };
type RotFormKind = "percent" | "minutes" | "toggle" | "choice" | "daySegments";
type RotFormScope = "all" | "each";
type RotHint = { "k": "hold", until?: number, account?: string, 
/**
 * `until` 的钟面（`HH:MM`，看的那一台的时区；不在今天带日期）。
 */
untilText?: string, } | { "k": "strategyError", error: RotStrategyError, path?: string, } | { "k": "tzUnknown", zone?: string, } | { "k": "oldScripts", n: number, files: Array<RotOldFile>, } | { "k": "newer" } | { "k": "wordsBad" } | { "k": "bookUnreadable", text: string, };
type RotLogCause = { "k": "order" } | { "k": "unusable" } | { "k": "temp" } | { "k": "useNow" } | { "k": "hold", until?: number, } | { "k": "error" };
type RotLogRow = { because: string, at: number, from: string, to: string, cause: RotLogCause, why?: string, };
type RotNote = { hint: RotHint, text: string, 
/**
 * 复制详情（出错那一条带原话；别的没有）。
 */
detail?: string, };
type RotOldFile = { name: string, path: string, };
type RotPast = { from: number, uses: Array<RotPastUse>, lanes: Array<RotPastLane>, };
type RotPastLane = { account: string, segs: Array<RotPastSeg>, };
type RotPastSeg = { from: number, to: number, sendable: boolean, reset: boolean, fromText: string, };
type RotPastUse = { from: number, to: number, account?: string, };
type RotRefused = { cells: Array<RotBadCell>, };
type RotStrategyError = { line?: number, msg: string, };
type RotStrategyItem = { id: string, 
/**
 * 说明（文件开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段；读不懂 ⇒ 空）。标题就是 `id`（文件名，§2.1）。
 */
description: string, rev: string, error?: RotStrategyError, 
/**
 * 此刻还活着、用着它的会话。
 */
inUse: Array<string>, forNew: boolean, 
/**
 * 这份的设置说明写法不合文案规矩（设置照样能用）。
 */
wordsBad: boolean, 
/**
 * 文件头里没写时区（分时段按 UTC 算）。
 */
tzUnknown: boolean, };
type RotTarget = { sid: string, } | { strategy: string, settings?: unknown, 
/**
 * `settings` 是照哪个版本改出来的（缺 ⇒ 不比）：跟文件此刻的不同 ⇒ 提示「别处被改了」。
 */
rev?: string, };
type RotTryDecision = { 
/**
 * 用哪个号（停发 ⇒ 缺）。
 */
pick?: string, 
/**
 * 停发到几点 · 到时恢复的号（停发才有）。
 */
holdUntil?: number, holdAccount?: string, 
/**
 * 策略交的先后（此刻愿意用的号）。
 */
order: Array<string>, 
/**
 * 每个号一句（策略写的）。
 */
why: Array<RotTryRow>, 
/**
 * 这一号来自用户拖出来的顺序。
 */
fromTemp: boolean, skipped: Array<string>, temp: Array<string>, nextNode?: number, 
/**
 * 策略这一判出错（走了兜底）：原话。
 */
strategyError?: string, };
type RotTryLane = { account: string, segs: Array<RotTryLaneSeg>, };
type RotTryLaneSeg = { from: number, to: number, 
/**
 * 事实上发得出去吗（被拒 · 登录 · key · 超额 · 用到 100 都算发不出去）。
 */
sendable: boolean, 
/**
 * 这一段开头两窗的用量（5 小时 · 7 天）。
 */
used: [number, number], reset: boolean, 
/**
 * 这一段起点的钟面（`HH:MM`，看的那一台的时区；重置那段写成 `↻18:30`，字由界面照文案表配）。
 */
fromText: string, 
/**
 * 这一段开头剩多少（事实：100 − 两窗里用得多的那个，不低于 0）。
 */
left: number, };
type RotTryRow = { account: string, why: string, };
type RotTrySwitch = { at: number, 
/**
 * `at` 的钟面（`HH:MM`）。
 */
atText: string, account: string, };
type RotTryTick = { at: number, text: string, };
type RotTryTimeline = { 
/**
 * 将来每次换到别的号（时间轴上方那几个「16:07 work」）。
 */
switches: Array<RotTrySwitch>, uses: Array<RotTryUse>, lanes: Array<RotTryLane>, truncated: boolean, 
/**
 * 此刻之后的整点刻度（推 12 小时以内每 2 小时、更长每 4 小时，看的那一台的钟）。
 */
ticks: Array<RotTryTick>, };
type RotTryUse = { from: number, to: number, account?: string, fromTemp: boolean, 
/**
 * 在用的号用到上限的时刻还不知道（额度账里量不到它的用速）：这一段只排确定的事件。
 */
capUnknown: boolean, skipped: Array<string>, };
type RotationViewReply = { now: number, 
/**
 * `now` 的钟面（`HH:MM`，看的那一台的时区）。
 */
nowText: string, 
/**
 * 用的那份策略；会话不轮换（没选策略）⇒ `null`，这时 `decision` · `timeline` 也是 `null`、只用开会话的号。
 */
strategy: RotStrategyItem | null, 
/**
 * 设置怎么画（策略的声明）。
 */
form: Array<RotFormCell>, 
/**
 * 生效的设置（会话有草稿 ⇒ 草稿）。
 */
settings: unknown, 
/**
 * 跟文件里的不同（会话 ⇒ 有草稿；设置页 ⇒ 交来的跟文件里的不同）。
 */
dirty: boolean, 
/**
 * 开会话用的号 · 此刻在用的号。
 */
start: string, current: string, 
/**
 * 每个号的事实，照号列表的先后（列表里没有的接在后面）。
 */
accounts: Array<Fact>, 
/**
 * 号列表里有几个（`accounts` 前这么多个是列表里的）。
 */
listed: number, decision: RotTryDecision | null, timeline: RotTryTimeline | null, 
/**
 * 时间轴过去那一截（会话才有；设置页没有会话 ⇒ `null`）。
 */
past: RotPast | null, 
/**
 * 显示的这份设置照哪个版本（会话有草稿 ⇒ 草稿基于的；否则 ＝ 文件此刻的）：写的时候当 `ifRev`。
 */
baseRev: string, 
/**
 * 换号记录（会话才有）。
 */
log: Array<RotLogRow>, hints: Array<RotNote>, };
type RotationViewReq = { target: RotTarget, 
/**
 * 往后推几小时（1–48，缺 ⇒ 12）。
 */
hours?: number, };
type StrategyText = string | { zh: string, en?: string, };
type WinFact = { open: boolean, used: number, resets?: number, 
/**
 * `resets` 写给人看的样子（按看的那一台的时区）。
 */
resetsText?: string, rate?: number, };
```

码：`bad_args` · `no_session` · `no_strategy` · `refused`

失败带 `data` 的码：`refused` ⇒ `RotRefused`

#### `rotation-settings`

改一份设置。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-settings`

入参 `RotationSettingsReq` · 回包 `RotationSettingsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotBadCell = { cell: string, code: string, };
type RotConflict = { rev: string, };
type RotRefused = { cells: Array<RotBadCell>, };
type RotWhere = { sid: string, } | { strategy: string, };
type RotationSettingsReply = { 
/**
 * 跟文件里的不同（会话 ⇒ 存成了草稿；相同 ⇒ 草稿丢掉）。
 */
dirty: boolean, };
type RotationSettingsReq = { target: RotWhere, 
/**
 * 整份设置（形状照那份策略的声明）。
 */
settings: unknown, 
/**
 * 改之前读到的那份策略文件的版本；对不上 ⇒ `conflict`。
 */
ifRev?: string, };
```

码：`bad_args` · `no_session` · `no_strategy` · `refused` · `conflict` · `io_failed`

失败带 `data` 的码：`refused` ⇒ `RotRefused` · `conflict` ⇒ `RotConflict`

#### `rotation-save`

存设置。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-save`

入参 `RotationSaveReq` · 回包 `RotationSaveReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotBadCell = { cell: string, code: string, };
type RotConflict = { rev: string, };
type RotRefused = { cells: Array<RotBadCell>, };
type RotSaveAs = { name: string, };
type RotWhere = { sid: string, } | { strategy: string, };
type RotationSaveReply = { id: string, rev: string, };
type RotationSaveReq = { target: RotWhere, 
/**
 * 要存的设置（会话缺 ⇒ 存它的草稿；策略缺 ⇒ `bad_args`）。
 */
settings?: unknown, 
/**
 * 改之前读到的那份策略文件的版本。
 */
ifRev: string, 
/**
 * 另存成一份新的（会话改用它）；缺 ⇒ 写回原来那份。
 */
as?: RotSaveAs, };
```

码：`bad_args` · `no_session` · `no_strategy` · `refused` · `conflict` · `io_failed`

失败带 `data` 的码：`refused` ⇒ `RotRefused` · `conflict` ⇒ `RotConflict`

#### `rotation-revert`

丢掉会话里没存的设置。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-revert`

入参 `RotationRevertReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotationRevertReq = { sid: string, };
```

码：`bad_args` · `no_session` · `io_failed`

#### `rotation-use`

会话换一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-use`

入参 `RotationUseReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotationUseReq = { sid: string, 
/**
 * `null` ⇒ 这个会话不再轮换（只用开会话的号）。
 */
strategy: string | null, 
/**
 * 有没存的设置也换（丢掉它）。
 */
discard: boolean, };
```

码：`bad_args` · `no_session` · `no_strategy` · `dirty` · `io_failed`

#### `rotation-for-new`

新会话用哪份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-for-new`

入参 `RotationForNewReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotationForNewReq = { 
/**
 * `null` ⇒ 新会话不轮换（只用开会话的号）。
 */
strategy: string | null, };
```

码：`bad_args` · `no_strategy` · `io_failed`

#### `rotation-temp`

排接下来先用谁。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-temp`

入参 `RotationTempReq` · 回包 `RotationTempReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotTempOp = { "place": { account: string, at: number, } } | "clear";
type RotationTempReply = { temp: Array<string>, };
type RotationTempReq = { sid: string, op: RotTempOp, };
```

码：`bad_args` · `no_session` · `not_listed` · `io_failed`

#### `rotation-use-now`

现在用这个号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-use-now`

入参 `RotationUseNowReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FactWhy = "refused" | "overage" | "used" | "login" | "key";
type RotDone = Record<symbol, never>;
type RotUnusable = { why?: FactWhy, until?: number, };
type RotationUseNowReq = { sid: string, account: string, };
```

码：`bad_args` · `no_session` · `not_listed` · `unusable` · `io_failed`

失败带 `data` 的码：`unusable` ⇒ `RotUnusable`

#### `rotation-strategy-add`

加一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategy-add`

入参 `RotationStrategyAddReq` · 回包 `RotStrategyId`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotBadStrategy = { line?: number, };
type RotStrategyId = { id: string, };
type RotationStrategyAddReq = { 
/**
 * 一份策略文件的原文（说明 ＋ 头 ＋ 代码）。
 */
source: string, 
/**
 * 存成的文件名（＝ 标题）。
 */
name: string, };
```

码：`bad_args` · `bad_strategy` · `io_failed`

失败带 `data` 的码：`bad_strategy` ⇒ `RotBadStrategy`

#### `rotation-strategy-copy`

复制一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategy-copy`

入参 `RotationStrategyCopyReq` · 回包 `RotStrategyId`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotStrategyId = { id: string, };
type RotationStrategyCopyReq = { id: string, name: string, };
```

码：`bad_args` · `no_strategy` · `io_failed`

#### `rotation-strategy-rename`

给策略改名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategy-rename`

入参 `RotationStrategyRenameReq` · 回包 `RotStrategyId`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotConflict = { rev: string, };
type RotStrategyId = { id: string, };
type RotationStrategyRenameReq = { id: string, name: string, ifRev: string, };
```

码：`bad_args` · `no_strategy` · `conflict` · `io_failed`

失败带 `data` 的码：`conflict` ⇒ `RotConflict`

#### `rotation-strategy-delete`

删一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategy-delete`

入参 `RotationStrategyDeleteReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotInUse = { sids: Array<string>, };
type RotationStrategyDeleteReq = { id: string, };
```

码：`bad_args` · `no_strategy` · `in_use` · `io_failed`

失败带 `data` 的码：`in_use` ⇒ `RotInUse`

#### `rotation-strategy-tz`

给策略设时区。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-strategy-tz`

入参 `RotationStrategyTzReq` · 回包 `RotStrategyRev`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotConflict = { rev: string, };
type RotStrategyRev = { rev: string, };
type RotationStrategyTzReq = { id: string, tz: string, ifRev: string, };
```

码：`bad_args` · `no_strategy` · `conflict` · `io_failed`

失败带 `data` 的码：`conflict` ⇒ `RotConflict`

#### `rotation-day-paint`

在 24 小时条上刷一段。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-day-paint`

入参 `RotationDayPaintReq` · 回包 `RotationDayPaintReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotSeg = { from: number, to: number, v: number, };
type RotationDayPaintReply = { segs: Array<RotSeg>, };
type RotationDayPaintReq = { segs: Array<RotSeg>, 
/**
 * 刷哪一截（一天里的分钟；`to ≤ from` ⇒ 跨午夜，相等 ⇒ 一整天）。
 */
from: number, to: number, 
/**
 * 刷成多少；`null` ⇒ 删掉那一截（回到跟总体）。
 */
v: number | null, };
```

码：`bad_args`

#### `apikey-key-set`

给一个账号写 key，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --apikey-key-set`

入参 `KeySetReq` · 回包 `KeySet`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type KeySet = { 
/**
 * 推出来的账号 id。
 */
account: string, 
/**
 * 那份文件的绝对路径。
 */
path: string, 
/**
 * 这一行 key 的掩码（只留末四位）。
 */
masked: string, 
/**
 * 这一行此刻的端点（没配 ⇒ `null`）。
 */
baseUrl: string | null, };
type KeySetReq = { 
/**
 * 这个号的账号目录。
 */
configDir: string, 
/**
 * 明文。
 */
key: string, 
/**
 * 这个账号的第三方端点：缺 / `null` / 空串 ＝ 不碰那一格（只配 key 时已有端点原样留着）。
 */
baseUrl?: string | null, };
```

码：`bad_args` · `bad_file` · `io_failed`

#### `accounts-list`

账号清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `AccountsListReq` · 回包 `AccountsList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountRow = { 
/**
 * API 号在这台 apikey 表里那一行的掩码（只留末四位）；订阅号 / 表里没有 / 没配 key ⇒ `null`。key 本体从不出后端。
 */
keyMasked: string | null, 
/**
 * 同上，那一行的端点。
 */
baseUrl: string | null, 
/**
 * 账号菜单每一项右边那一枚徽章。
 */
badge: Badge, name: string, 
/**
 * 号在人眼里叫什么（账号 0 ⇒ 「~/.claude」那一句，其余照原名）。
 */
label: string, email: string, 
/**
 * `null` ＝ 账号 0（下游据此不注入配置目录那一格）。
 */
configDir: string | null, isDefault: boolean, 
/**
 * `isolated` · `in-place`。
 */
mode: string, exists: boolean, 
/**
 * 凭据文件在不在（不代表凭据有效；可用性看 `authReady`）。
 */
loggedIn: boolean, 
/**
 * `subscription` · `api-key`。
 */
authKind: string, authReady: boolean, 
/**
 * 能拿来起会话 / 选为默认（`acct_core::account_selectable`）。
 */
selectable: boolean, 
/**
 * 号色（`--acct-c0..c7` 第几格，账号库里的事实），界面只画它。
 */
color: number, };
type AccountsList = { meta: ListMeta, 
/**
 * 每账号一行，清单次序。
 */
accounts: Array<AccountRow>, 
/**
 * 「能用但有缺」的那一句（几件都有 ⇒ 连成一句）：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）·
 * 这台的 API key 凭据文件不是只给本人（宽在哪 ＋ 怎么修；查不出 ⇒ 为什么）；都没有 ⇒ `null`。
 */
notice: string | null, };
type AccountsListReq = { 
/**
 * 这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）。
 */
agent: string, };
type Badge = { text: Words, warn: boolean, title: Words, };
type ListMeta = { 
/**
 * 这台建了账号库（清单读得出）。
 */
enabled: boolean, acctsDir: string, manifestPath: string, updatedAt: string | null, 
/**
 * 账号 0 的配置目录（清单里写了才有）。
 */
sharedStore: string | null, 
/**
 * 清单里认得的号数。
 */
count: number, 
/**
 * 清单读不出的那一句。
 */
error: string | null, 
/**
 * 这台做不了多账号的那一句（做得了 ⇒ `null`）。
 */
unsupported: string | null, 
/**
 * 删掉默认号之后新会话默认谁（同删号那一条的答案）。
 */
nextDefault: string | null, 
/**
 * 这台的默认号（标了的第一个，没标 ⇒ 第一个；`acct_core::effective_default`）。
 */
effectiveDefault: string | null, 
/**
 * 这台的家目录（界面把路径里的它缩成 `~`；推不出 ⇒ `null`）。
 */
home: string | null, };
type Words = string;
```

码：`bad_args` · `too_large`

#### `machine-interrupts`

停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machine-interrupts`

入参 `MachineInterruptsReq` · 回包 `Interrupts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Interrupts = { 
/**
 * 这台的活会话里经本机中转走请求的几个（停了就断，直到再启动）。
 */
relayedSessions: number, 
/**
 * 活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它）。
 */
relayedMaybe: number, 
/**
 * 这台活着的会话数（停的那几秒 cc-monitor 里它们不更新）。
 */
liveStreams: number, 
/**
 * 这台账上通往问的那台机器的转发（问的是重启 / 退出 cc-monitor ⇒ 账上全部）。
 */
forwards: number, };
type MachineInterruptsReq = { 
/**
 * 数通往哪台的转发（缺 ⇒ 不数）。
 */
machine?: string | null, 
/**
 * `true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零。
 */
appExit?: boolean | null, };
```

码：`bad_args`

#### `accounts-sessions`

正在跑的会话各属哪个账号。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-sessions`

入参 `NoArgs` · 回包 `SessionLines`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type NoArgs = Record<symbol, never>;
type SessionLines = { lines: Array<string>, };
```

码：`bad_args` · `too_large`

#### `accounts-trust`

换号前的信任预检。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-trust`

入参 `AccountsTrustReq` · 回包 `Trust`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountsTrustReq = { 
/**
 * 目标账号的 config dir（必须逐字 ∈ manifest，否则 `unknown_config_dir`）；**缺席或 `null` = 账号 0**
 * （读 agent 自己那份用户级配置，不收路径）。
 */
configDir?: string | null, 
/**
 * 要预检的工作目录。
 */
cwd: string, };
type Trust = { 
/**
 * 这个账号接受过该目录的信任对话框。
 */
trusted: boolean, 
/**
 * 这个账号的用户级配置里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认）。
 */
known: boolean, };
```

码：`bad_args` · `failed` · `manifest_unavailable` · `no_home` · `unknown_config_dir` · `unsafe_config_dir`

#### `accounts-init`

建账号库。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-init`

入参 `AccountInitArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountInitArgs = { name: string, 
/**
 * 真 ⇒ 只算不做（回将要做的那几步）。
 */
dryRun?: boolean, };
type AccountRef = { name: string, configDir: string, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-add`

新建一个号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-add`

入参 `AccountAddArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountAddArgs = { name: string, kind: AccountKind, 
/**
 * 订阅号：导入哪一份凭据文件（家目录底下；`~/` 开头也认）。缺席 ⇒ 建好之后另行登录（`login-start`）。
 */
credFile?: string, 
/**
 * API 号：上游地址（缺席 / 空 = 这个 agent 的默认上游）。
 */
baseUrl?: string, 
/**
 * API 号：key 明文。只在入方向出现一次。
 */
key?: string, 
/**
 * 真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它）。
 */
isDefault?: boolean, dryRun?: boolean, };
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountKind = "subscription" | "api-key";
type AccountRef = { name: string, configDir: string, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-remove`

删一个号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-remove`

入参 `AccountRemoveArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountRef = { name: string, configDir: string, };
type AccountRemoveArgs = { name: string, 
/**
 * 删的是默认号时必须给真。
 */
force?: boolean, dryRun?: boolean, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-set-default`

设默认号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-set-default`

入参 `AccountNameArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountNameArgs = { name: string, dryRun?: boolean, };
type AccountRef = { name: string, configDir: string, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-repair`

修复账号库（幂等）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-repair`

入参 `AccountRepairArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountRef = { name: string, configDir: string, };
type AccountRepairArgs = { dryRun?: boolean, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-rollback`

按一份备份还原。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-rollback`

入参 `AccountRollbackArgs` · 回包 `AccountChange`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountChange = { applied: boolean, 
/**
 * 做了（或将要做）的每一步，一句一行。
 */
steps: Array<string>, 
/**
 * 提示（不挡这一趟）。
 */
notes: Array<string>, 
/**
 * 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
 */
backup: string | null, 
/**
 * `accounts-add`：建出来的那个号。
 */
account: AccountRef | null, 
/**
 * `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
 */
aliasNames: Array<string>, 
/**
 * `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
 */
keyMasked: string | null, 
/**
 * 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
 */
keyProblem: string | null, 
/**
 * 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
 */
aliases: Array<AliasChange>, };
type AccountRef = { name: string, configDir: string, };
type AccountRollbackArgs = { backup?: string, dryRun?: boolean, };
type AliasChange = { path: string, 
/**
 * 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
 */
changed: boolean, 
/**
 * 建号：加进去的别名（如 `workcc` `workcct`）。
 */
added: Array<string>, 
/**
 * 删号：删掉的别名（参数指向这个号的全部）。
 */
removed: Array<string>, 
/**
 * 建号：名字被别的别名占着、没加的那几个。
 */
skipped: Array<string>, 
/**
 * 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
 */
note: string | null, };
```

码：`bad_args` · `io_failed` · `not_enabled` · `refused` · `unsupported`

#### `accounts-verify`

核对账号库。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-verify`

入参 `NoArgs` · 回包 `VerifyReport`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CheckLevel = "ok" | "warn" | "fail" | "skip";
type NoArgs = Record<symbol, never>;
type VerifyCheck = { level: CheckLevel, 
/**
 * 说的是哪个号；全局的那几条 ⇒ `None`。
 */
account: string | null, text: string, };
type VerifyReport = { 
/**
 * 没有一条 `fail`。
 */
pass: boolean, fails: number, warns: number, checks: Array<VerifyCheck>, };
```

码：`bad_args` · `io_failed` · `unsupported`

#### `accounts-login-cmd`

在终端里登录一个号的那一行。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-login-cmd`

入参 `AccountNameArgs` · 回包 `AccountLoginCmd`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountLoginCmd = { cmd: string, };
type AccountNameArgs = { name: string, dryRun?: boolean, };
```

码：`bad_args` · `io_failed` · `refused` · `unsupported`

#### `accounts-mcp-read`

这台各账号共用的用户级 MCP 此刻的样子。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-read`

入参 `NoArgs` · 回包 `AccountMcpView`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountMcpChoice = { 
/**
 * 挑这一版时交回去的 `from`（`None` = 共享的那一版）。
 */
from: string | null, 
/**
 * 此刻是这一版的那几个号。
 */
holders: Array<string>, 
/**
 * 这一版是「没有这一条」（在 cc-monitor 里删过）。
 */
gone: boolean, };
type AccountMcpConflict = { name: string, choices: Array<AccountMcpChoice>, };
type AccountMcpView = { 
/**
 * 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
 */
enabled: boolean, 
/**
 * 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的，`conflicts` 恒空）。
 */
sync: boolean, 
/**
 * 共享集合里的名字（排好序）。
 */
servers: Array<string>, conflicts: Array<AccountMcpConflict>, 
/**
 * 这一趟改写了哪几个号（只读那一条恒空）。
 */
changed: Array<string>, 
/**
 * 提示（某个号读不出来 · 这一趟没写进去）。
 */
notes: Array<string>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `io_failed` · `refused`

#### `accounts-mcp-remove`

从各账号共用的用户级 MCP 里删一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-remove`

入参 `AccountMcpNameArgs` · 回包 `AccountMcpView`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountMcpChoice = { 
/**
 * 挑这一版时交回去的 `from`（`None` = 共享的那一版）。
 */
from: string | null, 
/**
 * 此刻是这一版的那几个号。
 */
holders: Array<string>, 
/**
 * 这一版是「没有这一条」（在 cc-monitor 里删过）。
 */
gone: boolean, };
type AccountMcpConflict = { name: string, choices: Array<AccountMcpChoice>, };
type AccountMcpNameArgs = { name: string, };
type AccountMcpView = { 
/**
 * 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
 */
enabled: boolean, 
/**
 * 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的，`conflicts` 恒空）。
 */
sync: boolean, 
/**
 * 共享集合里的名字（排好序）。
 */
servers: Array<string>, conflicts: Array<AccountMcpConflict>, 
/**
 * 这一趟改写了哪几个号（只读那一条恒空）。
 */
changed: Array<string>, 
/**
 * 提示（某个号读不出来 · 这一趟没写进去）。
 */
notes: Array<string>, };
```

码：`bad_args` · `io_failed` · `not_found` · `refused`

#### `accounts-mcp-pick`

两边都改了的那一条用哪一版。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-pick`

入参 `AccountMcpPickArgs` · 回包 `AccountMcpView`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountMcpChoice = { 
/**
 * 挑这一版时交回去的 `from`（`None` = 共享的那一版）。
 */
from: string | null, 
/**
 * 此刻是这一版的那几个号。
 */
holders: Array<string>, 
/**
 * 这一版是「没有这一条」（在 cc-monitor 里删过）。
 */
gone: boolean, };
type AccountMcpConflict = { name: string, choices: Array<AccountMcpChoice>, };
type AccountMcpPickArgs = { name: string, from?: string, };
type AccountMcpView = { 
/**
 * 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
 */
enabled: boolean, 
/**
 * 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的，`conflicts` 恒空）。
 */
sync: boolean, 
/**
 * 共享集合里的名字（排好序）。
 */
servers: Array<string>, conflicts: Array<AccountMcpConflict>, 
/**
 * 这一趟改写了哪几个号（只读那一条恒空）。
 */
changed: Array<string>, 
/**
 * 提示（某个号读不出来 · 这一趟没写进去）。
 */
notes: Array<string>, };
```

码：`bad_args` · `io_failed` · `not_found` · `refused`

#### `accounts-mcp-sync`

停 / 开各账号之间同步用户级 MCP（定义与登录同一个开关）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --accounts-mcp-sync`

入参 `AccountMcpSyncArgs` · 回包 `AccountMcpView`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountMcpChoice = { 
/**
 * 挑这一版时交回去的 `from`（`None` = 共享的那一版）。
 */
from: string | null, 
/**
 * 此刻是这一版的那几个号。
 */
holders: Array<string>, 
/**
 * 这一版是「没有这一条」（在 cc-monitor 里删过）。
 */
gone: boolean, };
type AccountMcpConflict = { name: string, choices: Array<AccountMcpChoice>, };
type AccountMcpSyncArgs = { on: boolean, };
type AccountMcpView = { 
/**
 * 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
 */
enabled: boolean, 
/**
 * 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的，`conflicts` 恒空）。
 */
sync: boolean, 
/**
 * 共享集合里的名字（排好序）。
 */
servers: Array<string>, conflicts: Array<AccountMcpConflict>, 
/**
 * 这一趟改写了哪几个号（只读那一条恒空）。
 */
changed: Array<string>, 
/**
 * 提示（某个号读不出来 · 这一趟没写进去）。
 */
notes: Array<string>, };
```

码：`bad_args` · `io_failed` · `refused`

#### `login-start`

在 app 里登录一个号或一个号里的一个 MCP 服务器：起那一家的登录子命令，交回登录链接（本连接断了 ⇒ 作业收）。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | → | 账号库里的号（账号 0 是 `0`；`mcp` 那一种可为 `null`：那一家的默认配置目录） |
| `agent` | → | 哪一家（同 `accounts-list` 的 `agent`） |
| `kind` | → | 登录什么：`account`（一个号）· `mcp`（一个号里的一个 MCP 服务器） |
| `projectDir` | → | `mcp` 那一种：那一条配置所属的项目目录（用户级 ⇒ `null`） |
| `server` | → | `mcp` 那一种：服务器名（`mcp-read` 里那一条） |
| `auto` | ← | 会不会自己登完（本机浏览器回调到那一家自己开的本机端口；登完推 `changed {login}`，粘码只是后备） |
| `job` | ← | 作业号（`login-finish` · `login-cancel` 交它） |
| `paste` | ← | 粘回来的是什么：`code`（登录页上给的那一段）· `redirectUrl`（浏览器回跳到本机的那一整条地址） |
| `url` | ← | 登录链接（在浏览器里打开，不用内嵌网页） |

码：`bad_args` · `not_found` · `unsupported` · `failed`

#### `login-finish`

把粘回来的那一段（登录页上的码 · 浏览器回跳的地址）交给那份登录作业，等那一家换完。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `LoginFinishReq` · 回包 `LoginDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LoginDone = { target: LoginStartReq, };
type LoginFinishReq = { job: string, 
/**
 * 登录页上给的那一段（原样；不进日志、不回显）。
 */
pasted: string, };
type LoginStartReq = { "kind": "account", agent: string, account: string, } | { "kind": "mcp", agent: string, account: string | null, server: string, projectDir: string | null, };
```

码：`bad_args` · `bad_paste` · `refused` · `no_job` · `child_timed_out`

#### `login-cancel`

收一份登录作业。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `LoginCancelReq` · 回包 `LoginCancelled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LoginCancelReq = { job: string, };
type LoginCancelled = { was: boolean, };
```

码：`bad_args`

#### `manual-list`

会话外的需手动清单：需登录的号 · 需登录的 MCP。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --manual-list`

入参 `NoArgs` · 回包 `ManualList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LoginStartReq = { "kind": "account", agent: string, account: string, } | { "kind": "mcp", agent: string, account: string | null, server: string, projectDir: string | null, };
type ManualFix = { command: string, args: LoginStartReq, };
type ManualItem = { "kind": "accountLogin", agent: string, account: string, said: string, fix: ManualFix, } | { "kind": "mcpLogin", 
/**
 * 在哪个号里；没设账号的那一份 ⇒ `null`。
 */
account: string | null, server: string, scope: string, said: string, fix: ManualFix | null, };
type ManualList = { items: Array<ManualItem>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

### 4.4 历史会话与它的读面

#### `history-annotate`

改一条历史注解。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-annotate`

入参 `AnnotateReq` · 回包 `Annotated`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AnnotateReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 要改的那几格。
 */
patch: Patch, };
type Annotated = { entry: Entry, };
type Entry = { starred: boolean, customTitle: string | null, hidden: boolean, updatedAt: number, };
type Patch = { 
/**
 * 星标。
 */
starred?: boolean | null, 
/**
 * 自定义标题：`null` = 不改；空白串 = 清空（也认蛇形 `custom_title`）。
 */
customTitle?: string | null, 
/**
 * 隐藏。
 */
hidden?: boolean | null, };
```

码：`bad_args` · `annotations_unreadable` · `io_failed` · `no_annotations`

#### `history-forget`

删一条历史注解。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-forget`

入参 `ForgetReq` · 回包 `Forgotten`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ForgetReq = { 
/**
 * 会话 id。
 */
sid: string, };
type Forgotten = { 
/**
 * 真删了一条没有（`false` = 本来就没有这一条，文件没动）。
 */
removed: boolean, };
```

码：`bad_args` · `annotations_unreadable` · `io_failed` · `no_annotations`

#### `history-last-accounts`

sid → 上次用哪个号起。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-last-accounts`

入参 `NoArgs` · 回包 `LastAccounts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LastAccounts = { 
/**
 * `{sid: 账号名}`：只含真记过的那几条。
 */
accounts: { [key in string]: string }, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `unreadable`

#### `history-list`

历史页的平铺会话清单。

收 `args` · 可撤 · CLI：`ccm -- --history-list`

入参 `HistoryListReq` · 回包 `HistoryListReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Can = { 
/**
 * `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（恢复主会话）。
 */
resume: CanResume, 
/**
 * 恢复时能不能选号。
 */
accounts: boolean, 
/**
 * 能不能从某一轮分叉。
 */
fork: boolean, 
/**
 * `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑。
 */
delete: CanDelete, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type Entry = { starred: boolean, customTitle: string | null, hidden: boolean, updatedAt: number, };
type HistoryFailedDir = { projectDir: string, error: string, };
type HistoryGroup = { key: string, agent: string, projectName: string, projectPath: string, projectDir: string, count: number, 
/**
 * 有在跑的 ⇒ `true`；有答不了的 ⇒ `null`；都没在跑 ⇒ `false`。
 */
hasLive: boolean | null, starred: boolean, lastActivity: number, 
/**
 * 几台的组并成一列时按它（大的在前）。
 */
order: number, 
/**
 * 读不了的那个记录目录：原因。
 */
failed: string | null, 
/**
 * 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
 */
origin?: string, };
type HistoryList = { rows: Array<HistoryRow>, groups: Array<HistoryGroup>, total: number, truncated: boolean, 
/**
 * 注解没并上的那一句；并上了 ⇒ `null`。
 */
notice: string | null, };
type HistoryListReply = HistoryListing | HistoryList;
type HistoryListReq = { 
/**
 * `true` ⇒ 只回**这台自己**的清单 `{rows, failed, annotations | annotationsNotice}`（不筛不排、不看别的入参）——
 * 远端那一份就是那台常驻答的它；注解跟着会话住在那台，并的是清单带来的那份。
 */
raw?: boolean, 
/**
 * 那台的名字（`listing` 是它的；行与组都标上它）。缺 ⇒ 这台。
 */
origin?: string, 
/**
 * 那一台（`origin`）自己答的 `raw` 清单原样（界面经长连接问那台常驻拿回来）；本进程记着、换掉旧的。
 * 缺 ⇒ 用记着的那份，没记着 ⇒ `no_listing`；不带 `origin` ⇒ `bad_args`。
 */
listing?: HistoryListing, 
/**
 * 只要这一个会话那一行（独立查看窗；别的筛一概不看、不补父会话）。形状同 `history-record`（`[A-Za-z0-9-]`，1..=64）。
 */
sid?: string, 
/**
 * 只留显示标题 · 第一句 · 项目名里含这几个字的（不分大小写，子串；不比路径、不搜内容 —— 内容走 `history-search`）。
 */
query?: string, 
/**
 * 缺 ⇒ 按最后活动。
 */
sort?: HistorySort, 
/**
 * 只留那个键（同 `sort`）落在最近 N 天里的（1–3650）。
 */
within_days?: number, 
/**
 * `true` ⇒ 隐藏的也出（缺省不出）。
 */
hidden?: boolean, 
/**
 * 最多回几行（缺省 2000，1–20000）；多出的不回、`truncated`。
 */
limit?: number, };
type HistoryListing = { rows: Array<HistoryRawRow>, failed: Array<HistoryFailedDir>, 
/**
 * 那台自己那份注解（sid ⇒ 注解）；读不到 ⇒ 缺，`annotationsNotice` 说为什么。
 */
annotations?: { [key in string]: Entry }, annotationsNotice?: string, };
type HistoryRawRow = { agent: string, sessionId: string, projectPath: string, projectName: string, aiTitle: string | null, firstUserExcerpt: string, 
/**
 * 显示标题（用户改过的另在 `customTitle`）：与全文搜索同一条规则。
 */
title: string, startedAt: number, updatedAt: number, jsonlPath: string, 
/**
 * 判不了活（合成历史没有 pidfile）⇒ `null`。
 */
isLive: boolean | null, messageCountApprox: number, isBg: boolean, starred: boolean, customTitle: string | null, hidden: boolean, forkedFromSessionId?: string, forkedFromMessageUuid?: string, 
/**
 * 读它那一组用的键（记录目录名 · 合成历史 `<kind>:<cwd>`）。
 */
projectDir: string, 
/**
 * 分组键：哪一家 ＋ 真实目录。
 */
group: string, lastAccount?: string, };
type HistoryRow = { agent: string, sessionId: string, projectPath: string, projectName: string, aiTitle: string | null, firstUserExcerpt: string, title: string, startedAt: number, updatedAt: number, jsonlPath: string, messageCountApprox: number, isBg: boolean, starred: boolean, customTitle: string | null, hidden: boolean, forkedFromSessionId?: string, forkedFromMessageUuid?: string, projectDir: string, group: string, lastAccount?: string, 
/**
 * 显示的标题：用户改过的 ＞ 标题 ＞ 第一句。
 */
label: Words, 
/**
 * 改过的标题 · 标题 · 第一句都没有（界面写「没有说过话的会话」）。
 */
untitled: boolean, 
/**
 * `live` · `ended` · `unknown`（活不活只出这一格）。
 */
status: "live" | "ended" | "unknown", can: Can, 
/**
 * 行上那一家的小牌（对用户的叫法）；默认那一家 ⇒ `null`。
 */
agentTag: Words | null, 
/**
 * 被筛掉、只为挂住分叉子会话才带上的父会话（界面淡显）。
 */
context?: true, 
/**
 * 排序键（`activity` ⇒ 最后活动，`created` ⇒ 开始；毫秒）。
 */
at: number, atText: Words, sectionText: Words, spanText: Words, 
/**
 * 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
 */
origin?: string, };
type HistorySort = "activity" | "created";
type Words = string;
```

码：`bad_args` · `failed` · `no_listing`

#### `history-search`

全文搜索。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-search`

入参 `HistorySearchReq` · 回包 `HistorySearchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Can = { 
/**
 * `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（恢复主会话）。
 */
resume: CanResume, 
/**
 * 恢复时能不能选号。
 */
accounts: boolean, 
/**
 * 能不能从某一轮分叉。
 */
fork: boolean, 
/**
 * `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑。
 */
delete: CanDelete, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type HistorySearchReply = { 
/**
 * 每命中会话一行，最近优先。
 */
sessions: Array<SessionHits>, 
/**
 * 这一趟有几份会话记录读不动、没搜到。
 */
unreadable: number, 
/**
 * 内容搜索不覆盖、这台上又有它的会话记录的那几家（对用户的叫法）。
 */
skipped: Array<string>, };
type HistorySearchReq = { 
/**
 * 搜索词（不分大小写，子串）。
 */
query: string, 
/**
 * 工具调用与结果也搜。
 */
include_tools?: boolean, 
/**
 * 只搜哪一种：人说的话 · agent 的回答 · agent 回报；缺 ⇒ 全部。
 */
scope?: SearchScope, 
/**
 * 只要这一刻（毫秒）之后的。
 */
after_ms?: number, 
/**
 * 全局片段上限（`hitCount` 仍报全量）。
 */
limit?: number, 
/**
 * 只比会话标题与第一句（不搜内容）。
 */
titles?: boolean, };
type Liveness = "live" | "ended";
type SearchHit = { 
/**
 * 消息 uuid：打开查看器后据此滚动定位、高亮。
 */
uuid: string, tsMs: number, 
/**
 * `user` · `assistant` · `tool` · `report`。
 */
kind: string, before: string, matched: string, after: string, 
/**
 * 它落在哪次工具调用的卡里（调用 id，同会话内查找的 `tool`）；缺 ⇒ 落在它自己那张卡上。
 */
tool?: string, };
type SearchScope = "user" | "assistant" | "report";
type SessionHits = { 
/**
 * 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
 */
agent: string, sessionId: string, projectPath: string, projectName: string, jsonlPath: string, 
/**
 * 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
 */
title: string, 
/**
 * 记录文件最后改动的毫秒。
 */
updatedAt: number, 
/**
 * 本会话命中总数（可能 > `hits` 条数）。
 */
hitCount: number, hits: Array<SearchHit>, 
/**
 * 本会话有命中被「全局 snippet 预算用完」挡下了。
 */
hitsTruncated: boolean, 
/**
 * 后台分身会话。
 */
isBg: boolean, 
/**
 * 那台判的：在跑 · 已结束（全文搜索只扫记录树，判得了活）。
 */
status: Liveness, 
/**
 * 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
 */
can: Can, };
```

码：`bad_args` · `failed` · `too_large`

#### `history-search-merge`

把各台的搜索结果合成一份。

收 `args` · 可撤 · CLI：`ccm -- --history-search-merge`

入参 `HistorySearchMergeReq` · 回包 `Merged`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Can = { 
/**
 * `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（恢复主会话）。
 */
resume: CanResume, 
/**
 * 恢复时能不能选号。
 */
accounts: boolean, 
/**
 * 能不能从某一轮分叉。
 */
fork: boolean, 
/**
 * `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑。
 */
delete: CanDelete, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type HistorySearchMergeReq = { machines: Array<MachineHits>, };
type Liveness = "live" | "ended";
type MachineHits = { origin: Origin, sessions: Array<SessionHits>, };
type Merged = { 
/**
 * `hitCount` 之和。
 */
totalHits: number, sessionCount: number, 
/**
 * 任一行 `hitsTruncated`。
 */
truncated: boolean, 
/**
 * 各台的会话行，按 `updatedAt` 倒序。
 */
sessions: Array<MergedRow>, };
type MergedRow = { 
/**
 * 那台远端的名字；本机 ⇒ 缺。
 */
origin?: string, 
/**
 * 行尾那一格。
 */
atText: string, 
/**
 * 内容头那一段。
 */
spanText: string, 
/**
 * 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
 */
agent: string, sessionId: string, projectPath: string, projectName: string, jsonlPath: string, 
/**
 * 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
 */
title: string, 
/**
 * 记录文件最后改动的毫秒。
 */
updatedAt: number, 
/**
 * 本会话命中总数（可能 > `hits` 条数）。
 */
hitCount: number, hits: Array<SearchHit>, 
/**
 * 本会话有命中被「全局 snippet 预算用完」挡下了。
 */
hitsTruncated: boolean, 
/**
 * 后台分身会话。
 */
isBg: boolean, 
/**
 * 那台判的：在跑 · 已结束（全文搜索只扫记录树，判得了活）。
 */
status: Liveness, 
/**
 * 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
 */
can: Can, };
type Origin = string;
type SearchHit = { 
/**
 * 消息 uuid：打开查看器后据此滚动定位、高亮。
 */
uuid: string, tsMs: number, 
/**
 * `user` · `assistant` · `tool` · `report`。
 */
kind: string, before: string, matched: string, after: string, 
/**
 * 它落在哪次工具调用的卡里（调用 id，同会话内查找的 `tool`）；缺 ⇒ 落在它自己那张卡上。
 */
tool?: string, };
type SessionHits = { 
/**
 * 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
 */
agent: string, sessionId: string, projectPath: string, projectName: string, jsonlPath: string, 
/**
 * 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
 */
title: string, 
/**
 * 记录文件最后改动的毫秒。
 */
updatedAt: number, 
/**
 * 本会话命中总数（可能 > `hits` 条数）。
 */
hitCount: number, hits: Array<SearchHit>, 
/**
 * 本会话有命中被「全局 snippet 预算用完」挡下了。
 */
hitsTruncated: boolean, 
/**
 * 后台分身会话。
 */
isBg: boolean, 
/**
 * 那台判的：在跑 · 已结束（全文搜索只扫记录树，判得了活）。
 */
status: Liveness, 
/**
 * 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
 */
can: Can, };
```

码：`bad_args`

#### `history-run`

一个子运行的记录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`rows[].record` 是 `record`） · CLI：`ccm -- --history-run`

入参 `HistoryRunReq` · 回包 `HistoryRun`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Answer = { "kind": "approved" } | { "kind": "picked", options: Array<string>, };
type ApiReason = "overloaded" | "quota" | "network" | "auth" | "context" | "unknown";
type AskOption = { label: string, description?: string, };
type AskQuestion = { 
/**
 * 题前那一小格（原文）。没有 ⇒ 缺。
 */
header?: string, 
/**
 * 题目（原文）。
 */
question: string, 
/**
 * 可以选几项（`true` ＝ 多选）。
 */
multi: boolean, options: Array<AskOption>, };
type Block = { "type": "text", text: string, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, } | { "type": "image", 
/**
 * 图片来源原样（形状照那一家的写法透传）。
 */
source: unknown, };
type ChildRunTag = { label: string, kind?: string, };
type HistoryRun = { 
/**
 * 读的是哪个子运行。
 */
run: string, 
/**
 * 那份子运行记录（不透明，给查看器整份打开用）。
 */
path: string, 
/**
 * 这一页里每一条进界面的记录。
 */
rows: Array<RunRow>, 
/**
 * 读到哪了（最后一个整行之后）。
 */
end: number, 
/**
 * 这一页没读到头（再从 `end` 读）。
 */
more: boolean, };
type HistoryRunReq = { 
/**
 * 父会话的记录路径（围栏照旧；越界 ⇒ `path_refused`）。
 */
parent: string, 
/**
 * 子运行标识（运行表里那一格）；与 `tool` 至少给一个，都给以它为准。
 */
run?: string, 
/**
 * 派出它的那次工具调用的 id。
 */
tool?: string, 
/**
 * 从这个字节起读（缺 ⇒ 0）；续读拿上一次的 `end`。
 */
from?: number | null, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: string, 
/**
 * 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
 */
id: string, 
/**
 * 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
 */
at?: string, 
/**
 * `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
 */
timeText?: string, 
/**
 * `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
 */
atMs?: number, } & ({ "t": "said", 
/**
 * 谁说的、要显示的正文（判定只在适配层）。
 */
who: UserText, blocks: Array<Block>, 
/**
 * 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
 */
results?: { [key in string]: StepResult }, 
/**
 * 这条记录自己的工作目录。
 */
cwd?: string, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
/**
 * 这条是一轮的结束（与帧 `turn_end` 同一个判定）。
 */
endsTurn: boolean, 
/**
 * 工具调用 id ⇒ 卡型（普通工具卡不列）。
 */
cards?: { [key in string]: ToolCard }, 
/**
 * 工具调用 id ⇒ 一行人话。
 */
steps?: { [key in string]: ToolStep }, 
/**
 * 派出子运行的那几次工具调用 ⇒ 标签与类别。
 */
runs?: { [key in string]: ChildRunTag }, 
/**
 * 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
 */
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "queued", who: UserText, } | { "t": "unread", why: UnreadWhy, 
/**
 * 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
 */
type: string | null, 
/**
 * 写好的一句（[`unread_text`]）。
 */
text: string, 
/**
 * 恒 `warn`。
 */
tone: "warn", 
/**
 * 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
 */
excerpt: string, 
/**
 * 并了几条（适配层每行给 1）。
 */
count: number, });
type Pasted = { id?: string, start: number, end: number, bodyStart: number, bodyEnd: number, 
/**
 * 正文的行数（两头的空行不算）。
 */
lines: number, };
type PatchHunk = { 
/**
 * 改之前这段从第几行起（1 起）· 占几行；改之后同。
 */
oldStart: number, oldLines: number, newStart: number, newLines: number, lines: Array<string>, };
type PlanTitle = { 
/**
 * 等着批。
 */
waiting: string, 
/**
 * 批了。
 */
approved: string, 
/**
 * 没批（接着规划）。
 */
kept: string, };
type ReplyError = { reason: ApiReason, 
/**
 * HTTP 状态码（只有 HTTP 类错误有）。
 */
status?: number, };
type ResultKind = "launched" | "handback";
type RunRow = { 
/**
 * 通用记录（同 [`RecordLine::record`]）。
 */
record: LineRecord, 
/**
 * 它的对账键（界面拿它撤那个子运行的活卡）；没有 ⇒ 这一格不出现。
 */
rid?: string, };
type Speaker = { "kind": "human" } | { "kind": "slashCommand", name: string, args: string, } | { "kind": "bashInput", command: string, } | { "kind": "bashOutput", stdout: string, stderr: string, } | { "kind": "commandOutput" } | { "kind": "taskNotification", taskId?: string, status?: string, summary?: string, toolUseId?: string, } | { "kind": "agentMessage", from?: string, name?: string, handback: boolean, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "peerSession", from?: string, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "coordinator", 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "agentTask" } | { "kind": "system", 
/**
 * 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
 */
body?: string, } | { "kind": "compactSummary", 
/**
 * 摘要有多少字（写好的数，`copy_core::count_text`；界面拼进「摘要 {n} 字」）。
 */
charsText: string, } | { "kind": "interrupt" } | { "kind": "toolResult" };
type StepAsk = { "kind": "questions", questions: Array<AskQuestion>, } | { "kind": "plan", text: string, title: PlanTitle, };
type StepResult = { 
/**
 * 成功（`false` ＝ 失败或被拒）。
 */
ok: boolean, 
/**
 * 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
 */
rejected?: boolean, 
/**
 * 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
 */
lines?: number, 
/**
 * 改动：加了几行 · 删了几行。
 */
added?: number, removed?: number, 
/**
 * 命中几个文件（搜索 / 列文件）。
 */
files?: number, 
/**
 * 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
 */
answer?: Answer, 
/**
 * 命令失败时的退出码（结果里写着才有）。
 */
exitCode?: number, 
/**
 * 这次改动说的是哪个文件（结果里写着的路径，原样）。只有改文件那几类结果有。
 */
file?: string, 
/**
 * 逐段的改动本身（[`PatchHunk`]）。新建整份文件 / 没有改动 ⇒ 缺。
 */
patch?: Array<PatchHunk>, 
/**
 * diff 太大、只给了前几段（`added` / `removed` 仍是整份的数）。
 */
patchTruncated?: boolean, 
/**
 * 过程那一行右侧那一句（[`StepResult::written`] 写）：改动 `+38 −6` · 读了几行（只读文件那一类）· 几个文件 ·
 * 失败 · 未批准 · 提问 / 计划答了什么（已选「…」· 已批准）；都说不上 ⇒ 空串（出口写耗时）。
 */
text: string, 
/**
 * 同一句带耗时的那一形（耗时那一截留 `{dur}`，出口填两条记录之间的用时）：失败 · 都说不上的那一步才有；没有 ⇒ 缺（照 `text`）。
 */
timed?: string, 
/**
 * 结果的首行预览：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾；界面不再截。结果是空的 ⇒ 缺。
 */
preview?: string, 
/**
 * 结果正文多少字（按字符，与预览同一份正文）：派出子运行那张卡写「交回结果 · N 字」读它（出口省掉正文之后照样有）。结果是空的 ⇒ 缺。
 */
chars?: number, 
/**
 * 派出子运行那次调用的结果是哪一种（[`ResultKind`]）：派出卡照它画（回执不画「交回结果」）。不是派出子运行的结果 ⇒ 缺。
 */
kind?: ResultKind, };
type TitleBy = "agent" | "user";
type ToolCard = "agent" | "interactive" | "diff" | "md" | "command";
type ToolStep = { 
/**
 * 工具名（原样）。
 */
tool: string, 
/**
 * 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格），至多 200 字（按字符），截了以「…」收尾；界面不再截。认不出主参数 ⇒ 缺。
 */
arg?: string, 
/**
 * 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
 */
path?: boolean, 
/**
 * 说明（Bash 的 `description` 那一类）。
 */
note?: string, 
/**
 * 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
 */
known: boolean, 
/**
 * 认不出的工具：结果到了之后右侧那一句（「未识别结果」）；认得的 ⇒ 缺（照结果那一句）。
 */
text?: string, 
/**
 * 提问 / 计划那几张卡要画的东西（从入参里读出来的，各家的格式知识）：界面只按这一格建卡，不认入参结构。
 * 不是这两类、或入参形状不对 ⇒ 缺（界面画普通工具卡）。
 */
ask?: StepAsk, };
type UnreadWhy = "unknownType" | "parseFailed";
type UserText = { speaker: Speaker, 
/**
 * 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
 */
text: string, 
/**
 * `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
 */
pasted?: Array<Pasted>, };
```

码：`bad_args` · `failed` · `not_found` · `path_refused` · `refused` · `too_large`

#### `history-record`

这条会话的记录还在不在。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-record`

入参 `HistoryRecordReq` · 回包 `RecordProbe`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HistoryRecordReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 这次 resume 要用的账号配置目录（缺 / `null` ⇒ 这台的家目录）。
 */
configDir?: string, };
type RecordProbe = { 
/**
 * `<sid>.jsonl` 在记录树里（根那一层或项目目录那一层）找得到。
 */
present: boolean, 
/**
 * 查的那棵记录树的根。
 */
root: string, };
```

码：`bad_args`

#### `history-lines`

按行号取回一段。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`lines[].record` 是 `record`） · CLI：`ccm -- --history-lines`

入参 `HistoryLinesReq` · 回包 `HistoryLines`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Answer = { "kind": "approved" } | { "kind": "picked", options: Array<string>, };
type ApiReason = "overloaded" | "quota" | "network" | "auth" | "context" | "unknown";
type AskOption = { label: string, description?: string, };
type AskQuestion = { 
/**
 * 题前那一小格（原文）。没有 ⇒ 缺。
 */
header?: string, 
/**
 * 题目（原文）。
 */
question: string, 
/**
 * 可以选几项（`true` ＝ 多选）。
 */
multi: boolean, options: Array<AskOption>, };
type Block = { "type": "text", text: string, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, } | { "type": "image", 
/**
 * 图片来源原样（形状照那一家的写法透传）。
 */
source: unknown, };
type ChildRunTag = { label: string, kind?: string, };
type HistoryLines = { 
/**
 * 第一行的行号。
 */
from: number, 
/**
 * 下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数）。
 */
next: number, 
/**
 * 读到了最后一个完整行之后。
 */
eof: boolean, 
/**
 * `[from, next)` 里进界面的那些（不进界面的照占号、不出现）。
 */
lines: Array<RecordLine>, };
type HistoryLinesReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 第一行的行号（缺 ⇒ 0）。
 */
from?: number | null, 
/**
 * 可选右端（半开区间）；缺 ⇒ 到最后一个完整行为止。
 */
until?: number | null, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: string, 
/**
 * 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
 */
id: string, 
/**
 * 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
 */
at?: string, 
/**
 * `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
 */
timeText?: string, 
/**
 * `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
 */
atMs?: number, } & ({ "t": "said", 
/**
 * 谁说的、要显示的正文（判定只在适配层）。
 */
who: UserText, blocks: Array<Block>, 
/**
 * 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
 */
results?: { [key in string]: StepResult }, 
/**
 * 这条记录自己的工作目录。
 */
cwd?: string, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
/**
 * 这条是一轮的结束（与帧 `turn_end` 同一个判定）。
 */
endsTurn: boolean, 
/**
 * 工具调用 id ⇒ 卡型（普通工具卡不列）。
 */
cards?: { [key in string]: ToolCard }, 
/**
 * 工具调用 id ⇒ 一行人话。
 */
steps?: { [key in string]: ToolStep }, 
/**
 * 派出子运行的那几次工具调用 ⇒ 标签与类别。
 */
runs?: { [key in string]: ChildRunTag }, 
/**
 * 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
 */
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "queued", who: UserText, } | { "t": "unread", why: UnreadWhy, 
/**
 * 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
 */
type: string | null, 
/**
 * 写好的一句（[`unread_text`]）。
 */
text: string, 
/**
 * 恒 `warn`。
 */
tone: "warn", 
/**
 * 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
 */
excerpt: string, 
/**
 * 并了几条（适配层每行给 1）。
 */
count: number, });
type Pasted = { id?: string, start: number, end: number, bodyStart: number, bodyEnd: number, 
/**
 * 正文的行数（两头的空行不算）。
 */
lines: number, };
type PatchHunk = { 
/**
 * 改之前这段从第几行起（1 起）· 占几行；改之后同。
 */
oldStart: number, oldLines: number, newStart: number, newLines: number, lines: Array<string>, };
type PlanTitle = { 
/**
 * 等着批。
 */
waiting: string, 
/**
 * 批了。
 */
approved: string, 
/**
 * 没批（接着规划）。
 */
kept: string, };
type RecordLine = { 
/**
 * 这份记录是哪个会话的（那一家按路径认）。
 */
session_id: string, 
/**
 * 那份记录的路径。
 */
path: string, 
/**
 * 行号（可计行，从 0 起）。
 */
seq: number, 
/**
 * 那一行记的工作目录；没有 ⇒ `null`。
 */
cwd: string | null, 
/**
 * 通用记录。出口的声明可能去过格（`faces/project.rs`），所以是原样的一团。
 */
record: LineRecord, };
type ReplyError = { reason: ApiReason, 
/**
 * HTTP 状态码（只有 HTTP 类错误有）。
 */
status?: number, };
type ResultKind = "launched" | "handback";
type Speaker = { "kind": "human" } | { "kind": "slashCommand", name: string, args: string, } | { "kind": "bashInput", command: string, } | { "kind": "bashOutput", stdout: string, stderr: string, } | { "kind": "commandOutput" } | { "kind": "taskNotification", taskId?: string, status?: string, summary?: string, toolUseId?: string, } | { "kind": "agentMessage", from?: string, name?: string, handback: boolean, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "peerSession", from?: string, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "coordinator", 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "agentTask" } | { "kind": "system", 
/**
 * 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
 */
body?: string, } | { "kind": "compactSummary", 
/**
 * 摘要有多少字（写好的数，`copy_core::count_text`；界面拼进「摘要 {n} 字」）。
 */
charsText: string, } | { "kind": "interrupt" } | { "kind": "toolResult" };
type StepAsk = { "kind": "questions", questions: Array<AskQuestion>, } | { "kind": "plan", text: string, title: PlanTitle, };
type StepResult = { 
/**
 * 成功（`false` ＝ 失败或被拒）。
 */
ok: boolean, 
/**
 * 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
 */
rejected?: boolean, 
/**
 * 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
 */
lines?: number, 
/**
 * 改动：加了几行 · 删了几行。
 */
added?: number, removed?: number, 
/**
 * 命中几个文件（搜索 / 列文件）。
 */
files?: number, 
/**
 * 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
 */
answer?: Answer, 
/**
 * 命令失败时的退出码（结果里写着才有）。
 */
exitCode?: number, 
/**
 * 这次改动说的是哪个文件（结果里写着的路径，原样）。只有改文件那几类结果有。
 */
file?: string, 
/**
 * 逐段的改动本身（[`PatchHunk`]）。新建整份文件 / 没有改动 ⇒ 缺。
 */
patch?: Array<PatchHunk>, 
/**
 * diff 太大、只给了前几段（`added` / `removed` 仍是整份的数）。
 */
patchTruncated?: boolean, 
/**
 * 过程那一行右侧那一句（[`StepResult::written`] 写）：改动 `+38 −6` · 读了几行（只读文件那一类）· 几个文件 ·
 * 失败 · 未批准 · 提问 / 计划答了什么（已选「…」· 已批准）；都说不上 ⇒ 空串（出口写耗时）。
 */
text: string, 
/**
 * 同一句带耗时的那一形（耗时那一截留 `{dur}`，出口填两条记录之间的用时）：失败 · 都说不上的那一步才有；没有 ⇒ 缺（照 `text`）。
 */
timed?: string, 
/**
 * 结果的首行预览：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾；界面不再截。结果是空的 ⇒ 缺。
 */
preview?: string, 
/**
 * 结果正文多少字（按字符，与预览同一份正文）：派出子运行那张卡写「交回结果 · N 字」读它（出口省掉正文之后照样有）。结果是空的 ⇒ 缺。
 */
chars?: number, 
/**
 * 派出子运行那次调用的结果是哪一种（[`ResultKind`]）：派出卡照它画（回执不画「交回结果」）。不是派出子运行的结果 ⇒ 缺。
 */
kind?: ResultKind, };
type TitleBy = "agent" | "user";
type ToolCard = "agent" | "interactive" | "diff" | "md" | "command";
type ToolStep = { 
/**
 * 工具名（原样）。
 */
tool: string, 
/**
 * 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格），至多 200 字（按字符），截了以「…」收尾；界面不再截。认不出主参数 ⇒ 缺。
 */
arg?: string, 
/**
 * 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
 */
path?: boolean, 
/**
 * 说明（Bash 的 `description` 那一类）。
 */
note?: string, 
/**
 * 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
 */
known: boolean, 
/**
 * 认不出的工具：结果到了之后右侧那一句（「未识别结果」）；认得的 ⇒ 缺（照结果那一句）。
 */
text?: string, 
/**
 * 提问 / 计划那几张卡要画的东西（从入参里读出来的，各家的格式知识）：界面只按这一格建卡，不认入参结构。
 * 不是这两类、或入参形状不对 ⇒ 缺（界面画普通工具卡）。
 */
ask?: StepAsk, };
type UnreadWhy = "unknownType" | "parseFailed";
type UserText = { speaker: Speaker, 
/**
 * 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
 */
text: string, 
/**
 * `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
 */
pasted?: Array<Pasted>, };
```

码：`bad_args` · `failed` · `oversized_line` · `refused`

#### `history-read`

按字节分页读一份会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`rows[]` 是 `read_row`，`rows[].record` 是 `record`） · CLI：`ccm -- --history-read`

入参 `HistoryReadReq` · 回包 `HistoryRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Answer = { "kind": "approved" } | { "kind": "picked", options: Array<string>, };
type ApiReason = "overloaded" | "quota" | "network" | "auth" | "context" | "unknown";
type AskOption = { label: string, description?: string, };
type AskQuestion = { 
/**
 * 题前那一小格（原文）。没有 ⇒ 缺。
 */
header?: string, 
/**
 * 题目（原文）。
 */
question: string, 
/**
 * 可以选几项（`true` ＝ 多选）。
 */
multi: boolean, options: Array<AskOption>, };
type Block = { "type": "text", text: string, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, } | { "type": "image", 
/**
 * 图片来源原样（形状照那一家的写法透传）。
 */
source: unknown, };
type ChildRunTag = { label: string, kind?: string, };
type HistoryRead = { 
/**
 * 这一页里每个可计行一条（空白 / 纯 BOM 行不占）。
 */
rows: Array<ReadRow>, 
/**
 * 下一页从这里起（＝ `offset` ＋ 这一页的原始字节数）。
 */
next: number, 
/**
 * 区间到头了（`until` 或读时的文件长度）。
 */
eof: boolean, };
type HistoryReadReq = { 
/**
 * jsonl 路径（围栏：记录树里）。
 */
path: string, 
/**
 * 从这个字节起（缺 ⇒ 0）。
 */
offset?: number | null, 
/**
 * 可选右端（半开区间）；缺 ⇒ 读到文件尾。
 */
until?: number | null, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: string, 
/**
 * 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
 */
id: string, 
/**
 * 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
 */
at?: string, 
/**
 * `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
 */
timeText?: string, 
/**
 * `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
 */
atMs?: number, } & ({ "t": "said", 
/**
 * 谁说的、要显示的正文（判定只在适配层）。
 */
who: UserText, blocks: Array<Block>, 
/**
 * 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
 */
results?: { [key in string]: StepResult }, 
/**
 * 这条记录自己的工作目录。
 */
cwd?: string, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
/**
 * 这条是一轮的结束（与帧 `turn_end` 同一个判定）。
 */
endsTurn: boolean, 
/**
 * 工具调用 id ⇒ 卡型（普通工具卡不列）。
 */
cards?: { [key in string]: ToolCard }, 
/**
 * 工具调用 id ⇒ 一行人话。
 */
steps?: { [key in string]: ToolStep }, 
/**
 * 派出子运行的那几次工具调用 ⇒ 标签与类别。
 */
runs?: { [key in string]: ChildRunTag }, 
/**
 * 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
 */
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "queued", who: UserText, } | { "t": "unread", why: UnreadWhy, 
/**
 * 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
 */
type: string | null, 
/**
 * 写好的一句（[`unread_text`]）。
 */
text: string, 
/**
 * 恒 `warn`。
 */
tone: "warn", 
/**
 * 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
 */
excerpt: string, 
/**
 * 并了几条（适配层每行给 1）。
 */
count: number, });
type Pasted = { id?: string, start: number, end: number, bodyStart: number, bodyEnd: number, 
/**
 * 正文的行数（两头的空行不算）。
 */
lines: number, };
type PatchHunk = { 
/**
 * 改之前这段从第几行起（1 起）· 占几行；改之后同。
 */
oldStart: number, oldLines: number, newStart: number, newLines: number, lines: Array<string>, };
type PlanTitle = { 
/**
 * 等着批。
 */
waiting: string, 
/**
 * 批了。
 */
approved: string, 
/**
 * 没批（接着规划）。
 */
kept: string, };
type ReadRow = { 
/**
 * 这一行（含 `\n`）之后那个字节的偏移；没 `\n` 收尾的残尾 ⇒ `null`。
 */
end: number | null, 
/**
 * 这一行正文的摘要（[`line_hash`]；64 位，过 JSON 数会失精度，只拿来比相等）。
 */
hash: number, 
/**
 * 通用记录（缺 ＝ 不进界面，照占号）。出口的声明可能去过格（`faces/project.rs`），所以是原样的一团。
 */
record?: LineRecord, 
/**
 * 那一行记的工作目录（只在有记录时带）。
 */
cwd?: string, };
type ReplyError = { reason: ApiReason, 
/**
 * HTTP 状态码（只有 HTTP 类错误有）。
 */
status?: number, };
type ResultKind = "launched" | "handback";
type Speaker = { "kind": "human" } | { "kind": "slashCommand", name: string, args: string, } | { "kind": "bashInput", command: string, } | { "kind": "bashOutput", stdout: string, stderr: string, } | { "kind": "commandOutput" } | { "kind": "taskNotification", taskId?: string, status?: string, summary?: string, toolUseId?: string, } | { "kind": "agentMessage", from?: string, name?: string, handback: boolean, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "peerSession", from?: string, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "coordinator", 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "agentTask" } | { "kind": "system", 
/**
 * 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
 */
body?: string, } | { "kind": "compactSummary", 
/**
 * 摘要有多少字（写好的数，`copy_core::count_text`；界面拼进「摘要 {n} 字」）。
 */
charsText: string, } | { "kind": "interrupt" } | { "kind": "toolResult" };
type StepAsk = { "kind": "questions", questions: Array<AskQuestion>, } | { "kind": "plan", text: string, title: PlanTitle, };
type StepResult = { 
/**
 * 成功（`false` ＝ 失败或被拒）。
 */
ok: boolean, 
/**
 * 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
 */
rejected?: boolean, 
/**
 * 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
 */
lines?: number, 
/**
 * 改动：加了几行 · 删了几行。
 */
added?: number, removed?: number, 
/**
 * 命中几个文件（搜索 / 列文件）。
 */
files?: number, 
/**
 * 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
 */
answer?: Answer, 
/**
 * 命令失败时的退出码（结果里写着才有）。
 */
exitCode?: number, 
/**
 * 这次改动说的是哪个文件（结果里写着的路径，原样）。只有改文件那几类结果有。
 */
file?: string, 
/**
 * 逐段的改动本身（[`PatchHunk`]）。新建整份文件 / 没有改动 ⇒ 缺。
 */
patch?: Array<PatchHunk>, 
/**
 * diff 太大、只给了前几段（`added` / `removed` 仍是整份的数）。
 */
patchTruncated?: boolean, 
/**
 * 过程那一行右侧那一句（[`StepResult::written`] 写）：改动 `+38 −6` · 读了几行（只读文件那一类）· 几个文件 ·
 * 失败 · 未批准 · 提问 / 计划答了什么（已选「…」· 已批准）；都说不上 ⇒ 空串（出口写耗时）。
 */
text: string, 
/**
 * 同一句带耗时的那一形（耗时那一截留 `{dur}`，出口填两条记录之间的用时）：失败 · 都说不上的那一步才有；没有 ⇒ 缺（照 `text`）。
 */
timed?: string, 
/**
 * 结果的首行预览：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾；界面不再截。结果是空的 ⇒ 缺。
 */
preview?: string, 
/**
 * 结果正文多少字（按字符，与预览同一份正文）：派出子运行那张卡写「交回结果 · N 字」读它（出口省掉正文之后照样有）。结果是空的 ⇒ 缺。
 */
chars?: number, 
/**
 * 派出子运行那次调用的结果是哪一种（[`ResultKind`]）：派出卡照它画（回执不画「交回结果」）。不是派出子运行的结果 ⇒ 缺。
 */
kind?: ResultKind, };
type TitleBy = "agent" | "user";
type ToolCard = "agent" | "interactive" | "diff" | "md" | "command";
type ToolStep = { 
/**
 * 工具名（原样）。
 */
tool: string, 
/**
 * 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格），至多 200 字（按字符），截了以「…」收尾；界面不再截。认不出主参数 ⇒ 缺。
 */
arg?: string, 
/**
 * 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
 */
path?: boolean, 
/**
 * 说明（Bash 的 `description` 那一类）。
 */
note?: string, 
/**
 * 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
 */
known: boolean, 
/**
 * 认不出的工具：结果到了之后右侧那一句（「未识别结果」）；认得的 ⇒ 缺（照结果那一句）。
 */
text?: string, 
/**
 * 提问 / 计划那几张卡要画的东西（从入参里读出来的，各家的格式知识）：界面只按这一格建卡，不认入参结构。
 * 不是这两类、或入参形状不对 ⇒ 缺（界面画普通工具卡）。
 */
ask?: StepAsk, };
type UnreadWhy = "unknownType" | "parseFailed";
type UserText = { speaker: Speaker, 
/**
 * 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
 */
text: string, 
/**
 * `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
 */
pasted?: Array<Pasted>, };
```

码：`bad_args` · `failed` · `oversized_line` · `refused`

#### `history-page`

按字节分页读，出记录行。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`lines[].record` 是 `record`） · CLI：`ccm -- --history-page`

入参 `HistoryPageReq` · 回包 `HistoryPage`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Answer = { "kind": "approved" } | { "kind": "picked", options: Array<string>, };
type ApiReason = "overloaded" | "quota" | "network" | "auth" | "context" | "unknown";
type AskOption = { label: string, description?: string, };
type AskQuestion = { 
/**
 * 题前那一小格（原文）。没有 ⇒ 缺。
 */
header?: string, 
/**
 * 题目（原文）。
 */
question: string, 
/**
 * 可以选几项（`true` ＝ 多选）。
 */
multi: boolean, options: Array<AskOption>, };
type Block = { "type": "text", text: string, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, } | { "type": "image", 
/**
 * 图片来源原样（形状照那一家的写法透传）。
 */
source: unknown, };
type ChildRunTag = { label: string, kind?: string, };
type HistoryPage = { 
/**
 * 只装进界面的记录行（第 k 个可计行 ＝ `seq + k`）。
 */
lines: Array<RecordLine>, 
/**
 * 同 `history-read`。
 */
next: number, 
/**
 * 下一页第一行的行号。
 */
nextSeq: number, 
/**
 * 同 `history-read`。
 */
eof: boolean, };
type HistoryPageReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 从这个字节起（缺 ⇒ 0）。
 */
offset?: number | null, 
/**
 * 可选右端（半开区间）。
 */
until?: number | null, 
/**
 * `offset` 那一行的行号（缺 ⇒ 0）；续页交上一页的 `nextSeq`。
 */
seq?: number | null, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: string, 
/**
 * 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
 */
id: string, 
/**
 * 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
 */
at?: string, 
/**
 * `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
 */
timeText?: string, 
/**
 * `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
 */
atMs?: number, } & ({ "t": "said", 
/**
 * 谁说的、要显示的正文（判定只在适配层）。
 */
who: UserText, blocks: Array<Block>, 
/**
 * 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
 */
results?: { [key in string]: StepResult }, 
/**
 * 这条记录自己的工作目录。
 */
cwd?: string, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
/**
 * 这条是一轮的结束（与帧 `turn_end` 同一个判定）。
 */
endsTurn: boolean, 
/**
 * 工具调用 id ⇒ 卡型（普通工具卡不列）。
 */
cards?: { [key in string]: ToolCard }, 
/**
 * 工具调用 id ⇒ 一行人话。
 */
steps?: { [key in string]: ToolStep }, 
/**
 * 派出子运行的那几次工具调用 ⇒ 标签与类别。
 */
runs?: { [key in string]: ChildRunTag }, 
/**
 * 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
 */
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "queued", who: UserText, } | { "t": "unread", why: UnreadWhy, 
/**
 * 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
 */
type: string | null, 
/**
 * 写好的一句（[`unread_text`]）。
 */
text: string, 
/**
 * 恒 `warn`。
 */
tone: "warn", 
/**
 * 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
 */
excerpt: string, 
/**
 * 并了几条（适配层每行给 1）。
 */
count: number, });
type Pasted = { id?: string, start: number, end: number, bodyStart: number, bodyEnd: number, 
/**
 * 正文的行数（两头的空行不算）。
 */
lines: number, };
type PatchHunk = { 
/**
 * 改之前这段从第几行起（1 起）· 占几行；改之后同。
 */
oldStart: number, oldLines: number, newStart: number, newLines: number, lines: Array<string>, };
type PlanTitle = { 
/**
 * 等着批。
 */
waiting: string, 
/**
 * 批了。
 */
approved: string, 
/**
 * 没批（接着规划）。
 */
kept: string, };
type RecordLine = { 
/**
 * 这份记录是哪个会话的（那一家按路径认）。
 */
session_id: string, 
/**
 * 那份记录的路径。
 */
path: string, 
/**
 * 行号（可计行，从 0 起）。
 */
seq: number, 
/**
 * 那一行记的工作目录；没有 ⇒ `null`。
 */
cwd: string | null, 
/**
 * 通用记录。出口的声明可能去过格（`faces/project.rs`），所以是原样的一团。
 */
record: LineRecord, };
type ReplyError = { reason: ApiReason, 
/**
 * HTTP 状态码（只有 HTTP 类错误有）。
 */
status?: number, };
type ResultKind = "launched" | "handback";
type Speaker = { "kind": "human" } | { "kind": "slashCommand", name: string, args: string, } | { "kind": "bashInput", command: string, } | { "kind": "bashOutput", stdout: string, stderr: string, } | { "kind": "commandOutput" } | { "kind": "taskNotification", taskId?: string, status?: string, summary?: string, toolUseId?: string, } | { "kind": "agentMessage", from?: string, name?: string, handback: boolean, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "peerSession", from?: string, 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "coordinator", 
/**
 * 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
 */
body?: string, } | { "kind": "agentTask" } | { "kind": "system", 
/**
 * 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
 */
body?: string, } | { "kind": "compactSummary", 
/**
 * 摘要有多少字（写好的数，`copy_core::count_text`；界面拼进「摘要 {n} 字」）。
 */
charsText: string, } | { "kind": "interrupt" } | { "kind": "toolResult" };
type StepAsk = { "kind": "questions", questions: Array<AskQuestion>, } | { "kind": "plan", text: string, title: PlanTitle, };
type StepResult = { 
/**
 * 成功（`false` ＝ 失败或被拒）。
 */
ok: boolean, 
/**
 * 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
 */
rejected?: boolean, 
/**
 * 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
 */
lines?: number, 
/**
 * 改动：加了几行 · 删了几行。
 */
added?: number, removed?: number, 
/**
 * 命中几个文件（搜索 / 列文件）。
 */
files?: number, 
/**
 * 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
 */
answer?: Answer, 
/**
 * 命令失败时的退出码（结果里写着才有）。
 */
exitCode?: number, 
/**
 * 这次改动说的是哪个文件（结果里写着的路径，原样）。只有改文件那几类结果有。
 */
file?: string, 
/**
 * 逐段的改动本身（[`PatchHunk`]）。新建整份文件 / 没有改动 ⇒ 缺。
 */
patch?: Array<PatchHunk>, 
/**
 * diff 太大、只给了前几段（`added` / `removed` 仍是整份的数）。
 */
patchTruncated?: boolean, 
/**
 * 过程那一行右侧那一句（[`StepResult::written`] 写）：改动 `+38 −6` · 读了几行（只读文件那一类）· 几个文件 ·
 * 失败 · 未批准 · 提问 / 计划答了什么（已选「…」· 已批准）；都说不上 ⇒ 空串（出口写耗时）。
 */
text: string, 
/**
 * 同一句带耗时的那一形（耗时那一截留 `{dur}`，出口填两条记录之间的用时）：失败 · 都说不上的那一步才有；没有 ⇒ 缺（照 `text`）。
 */
timed?: string, 
/**
 * 结果的首行预览：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾；界面不再截。结果是空的 ⇒ 缺。
 */
preview?: string, 
/**
 * 结果正文多少字（按字符，与预览同一份正文）：派出子运行那张卡写「交回结果 · N 字」读它（出口省掉正文之后照样有）。结果是空的 ⇒ 缺。
 */
chars?: number, 
/**
 * 派出子运行那次调用的结果是哪一种（[`ResultKind`]）：派出卡照它画（回执不画「交回结果」）。不是派出子运行的结果 ⇒ 缺。
 */
kind?: ResultKind, };
type TitleBy = "agent" | "user";
type ToolCard = "agent" | "interactive" | "diff" | "md" | "command";
type ToolStep = { 
/**
 * 工具名（原样）。
 */
tool: string, 
/**
 * 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格），至多 200 字（按字符），截了以「…」收尾；界面不再截。认不出主参数 ⇒ 缺。
 */
arg?: string, 
/**
 * 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
 */
path?: boolean, 
/**
 * 说明（Bash 的 `description` 那一类）。
 */
note?: string, 
/**
 * 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
 */
known: boolean, 
/**
 * 认不出的工具：结果到了之后右侧那一句（「未识别结果」）；认得的 ⇒ 缺（照结果那一句）。
 */
text?: string, 
/**
 * 提问 / 计划那几张卡要画的东西（从入参里读出来的，各家的格式知识）：界面只按这一格建卡，不认入参结构。
 * 不是这两类、或入参形状不对 ⇒ 缺（界面画普通工具卡）。
 */
ask?: StepAsk, };
type UnreadWhy = "unknownType" | "parseFailed";
type UserText = { speaker: Speaker, 
/**
 * 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
 */
text: string, 
/**
 * `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
 */
pasted?: Array<Pasted>, };
```

码：`bad_args` · `failed` · `oversized_line` · `refused`

#### `history-turns`

一轮的摘要。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-turns`

入参 `HistoryTurnsReq` · 回包 `HistoryTurns`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FoldSegment = { 
/**
 * 这一段跟在哪一条后面：这一轮你那句，或露着的某一条（uuid）。一段只有一个 `after`，界面按它认段。
 */
after: string, 
/**
 * 那一行的字（后端写好，界面按语气排版、段间加分隔）。
 */
parts: Array<TurnPart>, 
/**
 * 那一行右端那一截。
 */
span: TurnSpan, 
/**
 * 这一段管到哪一条为止（不含）：段后面紧跟的那一条 —— 这一轮下一条露着的，或下一轮你那句（那一轮在不在主线上都一样：
 * 回退掉的那一轮不归上一轮的段）；还开着的最后一轮的尾巴那一段 ⇒ 没有（到记录尾）。
 * 界面按 `after` 与它在账本里取这一段的那几行，不自己找下一个边界。
 */
until: string | null, 
/**
 * 此刻：只有还没收尾的那一轮的尾巴那一段会是在跑 / 在等你（[`dress_live`]）。
 */
phase: TurnPhase, };
type HistoryTurns = { 
/**
 * 起点字节（＝ 请求的 `from`）。
 */
from: number, 
/**
 * 这一页管到哪个字节（有下一页 ⇒ ＝ `next`；最后一页 ⇒ 最后一个完整行的末字节，残尾不计）。
 */
end: number, 
/**
 * 下一页从哪个字节起（＝ 下一页第一轮的 `at`）；没有下一页 ⇒ `null`。
 */
next: number | null, 
/**
 * 这一段里的每一轮，文件序；`startText` · `endText` 是看的那一台钟上的 `HH:MM`（界面照抄；解不出 ⇒ 空串）。
 */
turns: Array<TurnRow>, };
type HistoryTurnsReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 从这个字节起扫（缺 ⇒ 0；续取传还在跑的那一轮的 `at`）。
 */
from?: number | null, 
/**
 * 可选：一页最多几轮（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
 */
limit?: number | null, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type TurnPart = { text: string, tone: Tone, };
type TurnPhase = "idle" | "running" | "awaiting";
type TurnRow = { 
/**
 * 这一轮开头那一行（你说的那句）的字节位置 ⇒ 下次从这里接着取；这一轮的范围到下一轮的 `at`（最后一轮到文件尾）。
 */
at: number, 
/**
 * 你那句的 uuid（跳过去用）。
 */
uuid: string, 
/**
 * 起：你那句的时刻；止：这一轮最后一条主线记录的时刻（还没有回应 ⇒ 同起）。
 */
start: string, end: string, 
/**
 * 起止各自在看的那一台钟上的钟面 `HH:MM`（答 `history-turns` 那一下按请求的时区写，[`TurnRow::stamp`]；解不出 ⇒ 空串）。界面照抄、不换算。
 */
startText: string, endText: string, 
/**
 * 你那句的第一行（≤ [`SAID_MAX`] 字）。
 */
said: string, 
/**
 * 整轮合计：工具调用几次（派 agent 不算）· 思考几段 · 派出几个 agent · 后台任务通知几条（交回过的子 agent 的收场通知不算）·
 * API 重试几次 · 另一会话 / 子 agent 来话几段（交回不算）。
 */
tools: number, thinking: number, agents: number, background: number, retries: number, peers: number, 
/**
 * 失败合计：工具（含派 agent）结果标了出错、且不是人拒的 ＋ 后台任务 / 子 agent 收场说失败。
 */
fails: number, 
/**
 * 露着的那几条（uuid，文件序）：带正文的 assistant 记录（中间说的话与结论）＋ 停下这一轮的那一条中断标记 / 报错卡。
 */
shown: Array<string>, 
/**
 * 回复头三行（≤ [`REPLY_MAX`] 字）：最后一次工具调用之后的结论正文的开头；没有结论 ⇒ 空串。
 */
reply: string, 
/**
 * 这一轮收尾了：后面又有你的一句，或 Claude 说完了（`end_turn`），或你中断了它。
 */
done: boolean, 
/**
 * 此刻：没在跑（收尾了 / 没有活进程）· 在跑 · 在等你（[`dress_live`] 按这台的会话事实现判）。
 */
phase: TurnPhase, 
/**
 * 过程段（文件序）：一段一行，界面把那一行插在 `after` 那张卡后面、把这一段的卡收进去。空 ⇒ 这一轮什么都不折。
 */
folds: Array<FoldSegment>, 
/**
 * 这一轮里落成了人话的排队插话（排队那一条的记录 id）：下一句你说的原文与它一字不差 ⇒ 那句话已经作为你的一句进了记录，
 * 界面不再单独出排队那一条（同一句不上屏两次）；还在排队 / 下一句不是它 ⇒ 不在这里（照常露着）。
 */
landed: Array<string>, };
type TurnSpan = { text: string, from: number | null, to: number | null, };
```

码：`bad_args` · `failed` · `too_large`

#### `sessions-needs`

这台上需手动的会话清单。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --sessions-needs`

入参 `NoArgs` · 回包 `NeedsList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AskKind = "approve" | "answer" | "plan";
type Needs = { kind: NeedsKind, 
/**
 * 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。
 */
tool: string | null, 
/**
 * 那个调用的 id（记录里 `tool_use` 的 `id`）：界面据此把过程里那一步画成「在等你批准」；判不出 ⇒ `null`。
 */
call: string | null, 
/**
 * 批准：那一步的主参数；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
 */
what: string | null, 
/**
 * 何时起等（那台 pidfile 的 `statusUpdatedAt`，epoch ms）；没有 ⇒ `null`。
 */
sinceMs: number | null, 
/**
 * 写好的字（等批准 · 等回答 · 需手动），出口照抄。
 */
text: Words, 
/**
 * 钉条第一行（种类 ＋ 工具名：「等批准 · Bash」· 等批准 · 计划 · 等放行 · 联网 …，[`needs_head`]），出口照抄；短的那一格是 `text`。
 */
head: Words, 
/**
 * 钉条里等宽那一段（批准 · 回答 · 放行带那一步的主参数 / 问题；别的种类不带）⇒ `null`。
 */
headCode: string | null, 
/**
 * 语气（恒 `need`）。
 */
tone: Tone, 
/**
 * 先答哪个的序（0 最先）：顶上那个框先答（`kind` 已按它判），再按危险度（[`NEEDS_BY_DANGER`]）。
 */
rank: number, 
/**
 * 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
 * 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：手机与 CLI 按节拍重问；桌面从收到那一刻起接着加（时长读口对同一份金样）。
 */
waitedMs: number | null, 
/**
 * `waitedMs` 写好的字（短时长那一处 `copy_core::short_duration`，与过程行步骤耗时同一种写法）；没有起点 ⇒ `null`。
 */
waitedText: Words | null, 
/**
 * 这一条可以怎么答（挂着的待答请求，`control::asks`）：选项的字 · 语气 · 要不要附话都写好；没有挂着的 ⇒ `null`（只能去终端）。
 */
ask: NeedsAsk | null, 
/**
 * `ask` 为 `null` 时写好的那一句（「只能在终端里答」）；有 `ask` ⇒ `null`。
 */
manualText: Words | null, };
type NeedsAsk = { 
/**
 * 这一次请求的 id（后端发的，不是那一家的）：答的时候原样交回。
 */
id: string, 
/**
 * 种类（同 `needs.kind` 那一套里的三种：approve · answer · plan）。
 */
kind: AskKind, 
/**
 * 工具名原样。
 */
tool: string, 
/**
 * 子运行（子 agent）发的 ⇒ 是哪一趟 `{id, kind}`（那一家原样）；主会话 ⇒ `null`。
 */
run: NeedsAskRun | null, 
/**
 * 一行摘要（命令头一行 · 路径 · 第一道题）；说不出 ⇒ `null`。
 */
what: string | null, 
/**
 * 要批的东西的全文（命令全文 · 写哪个文件写成什么 · 计划原文），不截。
 */
detail: string, 
/**
 * 能选的几项，排好序（「以后不再问」那几项不放第一位）。
 */
choices: Array<NeedsChoice>, 
/**
 * 提问那一种的题目；别的种类为空。
 */
questions: Array<NeedsQuestion>, 
/**
 * 何时挂上的（epoch ms，这台的钟）。
 */
sinceMs: number, };
type NeedsAskRun = { id: string, 
/**
 * 它是哪一种（那一家原样，如 `general-purpose`）；没说 ⇒ `null`。
 */
kind: string | null, };
type NeedsChoice = { 
/**
 * 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
 */
id: string, text: Words, tone: Tone, 
/**
 * 要不要附一句话：none · optional · required。
 */
note: NeedsNote, };
type NeedsKind = "approve" | "answer" | "plan" | "network" | "worker" | "goal" | "choose" | "unknown";
type NeedsList = { 
/**
 * 此刻活着、那台说在等人的会话（`needs.rank` 先答的在前，同一档等得久的在前）；一个都没有 ⇒ 空。
 */
waiting: Array<NeedsRow>, };
type NeedsNote = "none" | "optional" | "required";
type NeedsQuestion = { 
/**
 * `q0` · `q1` …（答的时候 `picks` 按它键）。
 */
id: string, text: string, header: string | null, multi: boolean, options: Array<NeedsQuestionOption>, 
/**
 * 恒 `true`：每题都许写自由文本（那一家框里的「Type something」）。
 */
other: boolean, };
type NeedsQuestionOption = { 
/**
 * `q0o0` …
 */
id: string, text: string, detail: string | null, };
type NeedsRow = { sid: string, needs: Needs, };
type NoArgs = Record<symbol, never>;
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type Words = string;
```

码：`bad_args`

#### `needs-ask`

答题 hook 挂一条待答请求、等人答（hook 客户端 `--answer-hook` 用；本连接断了 ⇒ 撤）。

收 `args` · 连接内就地做完 · 只在流上

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 哪一家（线上的 kind） |
| `hook` | → | 那一家的 hook 交来的那一份原样 |
| `decision` | ← | 人的答案写成那一家要的那一整份，hook 原样打到 stdout；`null` ＝ 不给决定（不是要人答的那一种 · 改到终端里答 · 别处关了），hook 空手退 |

码：`bad_args`

#### `needs-answer`

答一条挂着的待答请求（`needs.ask`）：第一个答案落下，其余回 `already_answered`。

收 `args` · 可撤 · 只在流上

入参 `AnswerReq` · 回包 `Answered`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AnswerReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * `needs.ask.id`。
 */
ask: string, 
/**
 * `needs.ask.choices[].id`。
 */
choice: string, 
/**
 * 可缺：附的那句话（选项 `note` 是 required 时必给）。
 */
note?: string | null, 
/**
 * 提问那一种选 `answer` 时必给：题 id ⇒ 那一题的答，每题都要答、单选至多一项。
 */
picks?: { [key in string]: AskPick } | null, };
type AnswerResult = "answered";
type Answered = { 
/**
 * 恒 `answered`。
 */
result: AnswerResult, };
type AskPick = { 
/**
 * 选项 id（`needs.ask.questions[].options[].id`）；缺 ⇒ 没选。
 */
options?: Array<string>, 
/**
 * 自由文本；缺 / 空白 ⇒ 没写。
 */
text?: string | null, };
```

码：`bad_args` · `not_found` · `already_answered` · `closed_elsewhere` · `bad_choice` · `note_required`

#### `history-facts`

会话事实。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（整份是 `facts`） · CLI：`ccm -- --history-facts`

入参 `HistoryFactsReq` · 回包 `SessionFacts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AskKind = "approve" | "answer" | "plan";
type BgTask = { 
/**
 * 起它的那次工具调用 id。
 */
call: string, 
/**
 * 它当场拿到的任务号；还没回 ⇒ `null`。
 */
task: string | null, 
/**
 * 那条命令原样（适配层给的）；没有 ⇒ `null`。
 */
cmd: string | null, 
/**
 * 起它的那条记录的 `timestamp` 原样；没有 ⇒ `null`。
 */
at: string | null, };
type FactsBackground = { 
/**
 * 「后台任务运行中 · make test-all · 12m」；命令拿不到 ⇒ 「后台任务运行中」。手机与 CLI 照抄、按节拍重问。
 */
text: Words, 
/**
 * 同一句、时长那一截留 `{dur}`：`{text, from}`，桌面填 现在 − `from`（`duration-format.ts::fmtDur`，与 `copy_core::short_duration` 对同一份金样）。
 * 命令或起始时刻拿不到 ⇒ `null`。
 */
clock: FactsClock | null, 
/**
 * 命令那一格（「make test-all」·「python train.py 等 2 条」）；拿不到 ⇒ `null`。
 */
what: Words | null, 
/**
 * 还在跑的后台命令几条（这一次进程起来之后起的；记录里一条都对不上 ⇒ 0）。
 */
count: number, 
/**
 * 语气（恒 `busy`）。
 */
tone: Tone, };
type FactsClock = { text: Words, from: number, };
type FactsCost = { micros: number, partial: boolean, text: Words, };
type FactsLastRequest = { id: string, tokens: Array<number>, };
type FactsToken = { end: number, rev: number, };
type HistoryFactsReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 可缺：续传令牌 ＝ 上一次应答里的 `{end, rev}`。缺 / `null` ⇒ 从字节 0 扫；给了 ⇒ 从那台续算表里那一份（[`facts_cache`]）
 * 接着扫、累加在它上面；表里没有 ⇒ `stale_prior`（调用方不带令牌从 0 要）。带不带 `view` 都是这两格（投影不去它们）。
 *
 * [`facts_cache`]: crate::observe::facts_cache
 */
prior?: FactsToken, };
type LastSay = { text: string, 
/**
 * 那条记录的 `timestamp` 原样；没有 ⇒ `null`。
 */
at: string | null, 
/**
 * `at` 的毫秒（同 [`PendingCall::at_ms`]：「完成多久前」交那一个读口走字）。
 */
atMs: number | null, };
type LimitFrom = "relay" | "setting" | "model" | "observed" | "assumed";
type McpStatus = "needsLogin" | "failed" | "pending" | "disabled" | "unknown";
type McpTrouble = { name: string, 
/**
 * `needsLogin` · `failed` · `pending`（[`crate::agents::McpStatus`] 的这三种）。
 */
status: McpStatus, 
/**
 * 断开时那一家写的原话（复制详情用，不当句子显示）；别的 ⇒ `null`。
 */
detail: string | null, 
/**
 * 说它的那条记录的 `timestamp` 原样；没有 ⇒ `null`。
 */
at: string | null, };
type Needs = { kind: NeedsKind, 
/**
 * 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。
 */
tool: string | null, 
/**
 * 那个调用的 id（记录里 `tool_use` 的 `id`）：界面据此把过程里那一步画成「在等你批准」；判不出 ⇒ `null`。
 */
call: string | null, 
/**
 * 批准：那一步的主参数；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
 */
what: string | null, 
/**
 * 何时起等（那台 pidfile 的 `statusUpdatedAt`，epoch ms）；没有 ⇒ `null`。
 */
sinceMs: number | null, 
/**
 * 写好的字（等批准 · 等回答 · 需手动），出口照抄。
 */
text: Words, 
/**
 * 钉条第一行（种类 ＋ 工具名：「等批准 · Bash」· 等批准 · 计划 · 等放行 · 联网 …，[`needs_head`]），出口照抄；短的那一格是 `text`。
 */
head: Words, 
/**
 * 钉条里等宽那一段（批准 · 回答 · 放行带那一步的主参数 / 问题；别的种类不带）⇒ `null`。
 */
headCode: string | null, 
/**
 * 语气（恒 `need`）。
 */
tone: Tone, 
/**
 * 先答哪个的序（0 最先）：顶上那个框先答（`kind` 已按它判），再按危险度（[`NEEDS_BY_DANGER`]）。
 */
rank: number, 
/**
 * 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
 * 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：手机与 CLI 按节拍重问；桌面从收到那一刻起接着加（时长读口对同一份金样）。
 */
waitedMs: number | null, 
/**
 * `waitedMs` 写好的字（短时长那一处 `copy_core::short_duration`，与过程行步骤耗时同一种写法）；没有起点 ⇒ `null`。
 */
waitedText: Words | null, 
/**
 * 这一条可以怎么答（挂着的待答请求，`control::asks`）：选项的字 · 语气 · 要不要附话都写好；没有挂着的 ⇒ `null`（只能去终端）。
 */
ask: NeedsAsk | null, 
/**
 * `ask` 为 `null` 时写好的那一句（「只能在终端里答」）；有 `ask` ⇒ `null`。
 */
manualText: Words | null, };
type NeedsAsk = { 
/**
 * 这一次请求的 id（后端发的，不是那一家的）：答的时候原样交回。
 */
id: string, 
/**
 * 种类（同 `needs.kind` 那一套里的三种：approve · answer · plan）。
 */
kind: AskKind, 
/**
 * 工具名原样。
 */
tool: string, 
/**
 * 子运行（子 agent）发的 ⇒ 是哪一趟 `{id, kind}`（那一家原样）；主会话 ⇒ `null`。
 */
run: NeedsAskRun | null, 
/**
 * 一行摘要（命令头一行 · 路径 · 第一道题）；说不出 ⇒ `null`。
 */
what: string | null, 
/**
 * 要批的东西的全文（命令全文 · 写哪个文件写成什么 · 计划原文），不截。
 */
detail: string, 
/**
 * 能选的几项，排好序（「以后不再问」那几项不放第一位）。
 */
choices: Array<NeedsChoice>, 
/**
 * 提问那一种的题目；别的种类为空。
 */
questions: Array<NeedsQuestion>, 
/**
 * 何时挂上的（epoch ms，这台的钟）。
 */
sinceMs: number, };
type NeedsAskRun = { id: string, 
/**
 * 它是哪一种（那一家原样，如 `general-purpose`）；没说 ⇒ `null`。
 */
kind: string | null, };
type NeedsChoice = { 
/**
 * 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
 */
id: string, text: Words, tone: Tone, 
/**
 * 要不要附一句话：none · optional · required。
 */
note: NeedsNote, };
type NeedsKind = "approve" | "answer" | "plan" | "network" | "worker" | "goal" | "choose" | "unknown";
type NeedsNote = "none" | "optional" | "required";
type NeedsQuestion = { 
/**
 * `q0` · `q1` …（答的时候 `picks` 按它键）。
 */
id: string, text: string, header: string | null, multi: boolean, options: Array<NeedsQuestionOption>, 
/**
 * 恒 `true`：每题都许写自由文本（那一家框里的「Type something」）。
 */
other: boolean, };
type NeedsQuestionOption = { 
/**
 * `q0o0` …
 */
id: string, text: string, detail: string | null, };
type PendingCall = { 
/**
 * 工具调用 id（等它的 `tool_result` 来摘）。
 */
id: string, 
/**
 * 工具名原样。
 */
name: string, 
/**
 * 主参数一行（[`WHAT_KEYS`]；提问取第一问）；没登记 / 入参里没有 ⇒ `null`。
 */
what: string | null, 
/**
 * 那条记录的 `timestamp` 原样（从何时起在跑）；没有 ⇒ `null`。
 */
at: string | null, 
/**
 * `at` 的毫秒（自 1970）：出口算「已经跑了多久」只用这一格（交那一个读口走字），不自己解析 `at`；解不出 ⇒ `null`。
 */
atMs: number | null, 
/**
 * 这一步此刻的样子（[`settle_pending`] 每次现判，不累加；`prior` 里那一份不用）。
 */
state: StepWait, 
/**
 * `state` 是 `unclear` 时为什么判不了；别的 ⇒ `null`。
 */
why: UnclearWhy | null, 
/**
 * 过程那一行右侧那一句（状态不明 ⇒「状态不明」；在跑 · 在等你 ⇒ `null`，在等你那一格出口写已等多久）。
 */
text: Words | null, 
/**
 * 状态不明时那一行悬停说的为什么（照 `why` 写好）；别的 ⇒ `null`。
 */
whyText: Words | null, };
type RetryOutcome = "retrying" | "recovered" | "failed" | "interrupted";
type RetryRun = { 
/**
 * 这一串首条重试记录的 `uuid`（界面那条细条就挂在它上面）。
 */
id: string, outcome: RetryOutcome, };
type SessionFacts = { 
/**
 * 最后一个完整行的末字节 ＝ 下一次的续点（续传令牌 `{end, rev}` 的一半）。
 */
end: number, 
/**
 * 这一份在那台常驻后端续算表里的号（续传令牌的另一半，[`super::facts_cache`]）：下一问交回 `{end, rev}` 就从这一份接着扫。
 * 扫描本身不读它；只在出成品那一刻由续算表铸。
 */
rev: number, 
/**
 * 源会话 sid；不是分叉来的 ⇒ `null`。
 */
forkedFrom: string | null, 
/**
 * 写类工具碰过的文件（原样，可能相对 / Windows 路径），近因序：最近碰的在末尾。
 */
touchedFiles: Array<string>, 
/**
 * 最后一条带有效 usage 的 assistant 记录；一条都没有 ⇒ `null`。
 */
usage: UsageFact | null, 
/**
 * 会话的项目目录（会话起在哪个目录）：适配层读记录开头给（`agents::project_dir_of`），读到即锁定；
 * 开头里还没有 ⇒ `null`，下一次再读。与会话宣告那一帧的 `project_dir` 同一个函数。
 */
projectDir: string | null, 
/**
 * 这份记录是哪一家的（线上的 kind，适配层按记录认：`agents::record_kind_of`）；认不出 ⇒ `null`（界面要分家的那几项灰着，不落哪一家）。
 */
agent: string | null, 
/**
 * 此刻持着这条会话的活进程 pid（`accounts_query::session_holders`：照那一家声明的判活方式查），升序；不累加，每次现查（`prior` 里那一份不用）。
 * 不止一个 ⇒ 同一条会话有几个进程在同时写。判不了 ⇒ 空。
 */
writers: Array<number>, 
/**
 * 还没有结果的工具调用（文件序，正在跑 / 在等批准的那几步）。
 */
pending: Array<PendingCall>, 
/**
 * 最后一段正文的头一行（悬停卡「它最后一句」）。
 */
lastSay: LastSay | null, 
/**
 * **需手动**：那台说在等、等的是什么（不累加，每次现查；`prior` 里那一份不用）。不在等 ⇒ `null`。
 */
needs: Needs | null, 
/**
 * 交回了的子运行（子 agent 的 id，文件序、去重）：同一个子运行的收场通知以交回为准，只报一次。
 */
handedBack: Array<string>, 
/**
 * 一串一串的 API 重试与结局（按首条的 `uuid`，文件序）；最后一串可能还没有下文（`retrying`）。
 */
retries: Array<RetryRun>, 
/**
 * 此刻的许可档（最后一条许可档记录写的那一档，原样）；没有 ⇒ `null`。
 */
permissionMode: string | null, 
/**
 * 全会话用量（按请求去重）；一条带用量的回复都没有 ⇒ `null`。
 */
tokens: TokenUse | null, 
/**
 * 全会话花费（记录里那一家自己记的花费那一条，最后一条为准）；记录里没有 ⇒ `null`（不按定价自己算）。
 */
cost: FactsCost | null, 
/**
 * 还没收场的后台命令（文件序）：累加的那一份账，续传时原样带回来。
 */
bgTasks: Array<BgTask>, 
/**
 * **后台任务运行中**：那台说一轮停了、后台命令还在跑 ⇒ 状态一句（不累加，每次现查；`prior` 里那一份不用）。不是这一态 ⇒ `null`。
 */
background: FactsBackground | null, 
/**
 * **答题口没接上**：ccm 起这条会话时挂不上答题 hook（用户自己给的会话级设置读不出 / 形状不认）· 不是 ccm 起的、持着它的进程身上看不出挂过 ⇒ 写好的那一句
 * （「这条会话只能在终端里答批准」；不累加，每次按持着它的进程现查）。接上了 / 判不了 ⇒ `null`。
 */
answerOff: Words | null, 
/**
 * 这个会话里那一家说有毛病的 MCP 服务器（要登录 · 断开 · 还在连），按名字排；没列的不等于连上了。
 */
mcp: Array<McpTrouble>, };
type StepWait = "running" | "awaiting" | "unclear";
type TokenUse = { input: number, output: number, cacheRead: number, cacheWrite5m: number, cacheWrite1h: number, 
/**
 * 算进来的请求数。
 */
requests: number, 
/**
 * 写好的串（输入 · 输出 · 读缓存 · 写缓存）。
 */
text: Words, 
/**
 * 上一次请求（续传时同一次请求的后一条要替掉它）：键 · 那一次的五个数。
 */
last: FactsLastRequest | null, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type UnclearWhy = "noWriter" | "untracked";
type UsageFact = { 
/**
 * `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`。
 */
promptTokens: number, model: string | null, 
/**
 * 全会话最大的一轮（见过超过标准上限的 ⇒ 上限不可能是标准那一档）。
 */
peakPromptTokens: number, 
/**
 * 上下文上限（tokens），恒 ≥ `peak_prompt_tokens` ⇒ 百分比不会超过 100。
 */
limit: number, limitFrom: LimitFrom, 
/**
 * 最新一轮占上限的百分比（四舍五入，0–100）；上限判不出（`assumed`）⇒ `null`。
 */
percent: number | null, 
/**
 * 上下文那一格的字：上限判得出写百分比（`35%`），判不出只写用了多少（`350k`）。
 */
contextText: Words, 
/**
 * 那一格的语气：到了 [`CONTEXT_WARN_AT`] ⇒ `warn`，否则 `plain`（判不出上限 ⇒ 恒 `plain`）。
 */
contextTone: Tone, 
/**
 * 最新一轮用了多少（`350k`）。
 */
promptTokensText: Words, 
/**
 * 上限（`1M`）。
 */
limitText: Words, 
/**
 * 上限从哪来的字（中转请求 · 设置 · 模型名 · 用量超过 200k）；判不出 ⇒ `null`。
 */
limitFromText: Words | null, };
type Words = string;
```

码：`bad_args` · `failed` · `too_large` · `stale_prior`

#### `history-branch`

主线外清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-branch`

入参 `BranchReq` · 回包 `Branch`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Branch = { 
/**
 * 回退掉的那几条记录的 `id`（只含进界面的；文件序），这一页是清单里从 `from` 起的那几条；这一家的记录没有链 ⇒ 恒空。
 */
off: Array<string>, 
/**
 * 最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说）。
 */
end: number, 
/**
 * 下一页从清单第几条起；没有下一页 ⇒ `null`。
 */
next: number | null, };
type BranchReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 从清单第几条起（缺 ⇒ 0；下一页传 `next`）。
 */
from?: number | null, 
/**
 * 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
 */
limit?: number | null, };
```

码：`bad_args` · `failed`

#### `history-find`

会话内查找。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-find`

入参 `HistoryFindReq` · 回包 `HistoryFind`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FindHit = { 
/**
 * 命中的那条记录的 uuid（界面按骨架索引 `uuid → seq` 跳过去）。
 */
uuid: string, 
/**
 * `user` · `assistant` · `report`（agent 回报）· `tool`。
 */
kind: string, 
/**
 * 片段三段（同全局搜索的 `Hit`）。
 */
before: string, matched: string, after: string, 
/**
 * 第几轮（这条之前含它人说过几句；第一句之前 ＝ 0）。
 */
turn: number, 
/**
 * 那条记录的时刻（毫秒；读不出 ＝ 0）。
 */
tsMs: number, 
/**
 * `tsMs` 按看的那一台的时区写好（今天 `HH:MM` · 昨天 · 更早带日期；读不出 ⇒ 空串）：出口那一下添（[`with_hit_text`]）。
 */
tsText: string, 
/**
 * 它收在哪次工具调用的那一块里（调用 id；工具结果流里不自己成卡）；落在自己那张卡上 ⇒ 缺。
 */
tool?: string, };
type HistoryFind = { 
/**
 * 全量命中数（≥ 条数；大于 ⇒ 被上限砍过）。
 */
total: number, 
/**
 * 这一页的命中，文件序；只搜流里画的那几张卡（自动写的应答不搜）。
 */
hits: Array<FindHit>, };
type HistoryFindReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 查询串（原样；以 `--` 起头也照样是查询，不是选项）。
 */
query: string, 
/**
 * 工具结果也搜（缺 ⇒ 不搜）。
 */
include_tools?: boolean, 
/**
 * 只留谁说的，同 `history-search` 同名同义（`user` 时工具内容不搜）。
 */
scope?: SearchScope, 
/**
 * 最多列几条（缺 ⇒ 500，封顶 2000）。
 */
limit?: number | null, 
/**
 * 跳过前几条（续下一页；缺 ⇒ 0）。
 */
skip?: number | null, };
type SearchScope = "user" | "assistant" | "report";
```

码：`bad_args` · `failed` · `too_large`

#### `history-index`

会话骨架索引。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-index`

入参 `HistoryIndexReq` · 回包 `HistoryIndex`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HistoryIndex = { 
/**
 * 起点字节。
 */
from: number, 
/**
 * 这一页最后一行的末字节 ＝ 下一次续传该带的 `offset`（最后一页 ⇒ 最后一个完整行的末字节）。
 */
end: number, 
/**
 * 下一页从哪个字节起（＝ 下一页第一行的起点）；没有下一页 ⇒ `null`。
 */
next: number | null, 
/**
 * 行数（每一列都这么长）。
 */
count: number, 
/**
 * 本份里出现过的记录类（`t` 列的号从 1 起指它）。
 */
classes: Array<RecordClass>, o: Array<number>, n: Array<number>, t: Array<number>, fd: Array<number>, u: Array<string | null>, body: IndexBody, sp: IndexSpeaker, inputs: IndexInputs, };
type HistoryIndexReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 从哪个字节起（缺 ⇒ 0；续传带上次的 `end`）。
 */
offset?: number | null, 
/**
 * 可选：只收起点 `< until` 的行。
 */
until?: number | null, 
/**
 * 可选：一页最多几行（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
 */
limit?: number | null, };
type IndexBody = { 
/**
 * 行号差。
 */
at: Array<number>, ch: Array<number>, cj: Array<number>, pl: Array<number>, cb: Array<number>, cl: Array<number>, };
type IndexInputs = { 
/**
 * 行号差。
 */
at: Array<number>, x: Array<string>, ts: Array<string>, };
type IndexSpeaker = { 
/**
 * 行号差。
 */
at: Array<number>, v: Array<string>, };
type RecordClass = "said" | "reply" | "retry" | "title" | "queued" | "unread";
```

码：`bad_args` · `failed` · `too_large`

#### `history-user-inputs`

「人说过的话」清单。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-user-inputs`

入参 `UserInputsReq` · 回包 `UserInputs`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type UserInputRow = { 
/**
 * 这一句那一行在记录里的字节起点（分页的续点按它给；逐条认的那一处不知道位置时是 0，由扫的那一处填）。
 */
at: number, uuid: string, 
/**
 * 记录的 `timestamp`；没有 ⇒ 空串。
 */
timestamp: string, 
/**
 * 摘要：多行/多空白压成一行，截到 [`EXCERPT_MAX`]。
 */
excerpt: string, };
type UserInputs = { 
/**
 * 这份清单从哪个字节起（＝ 请求的 `from`）。
 */
from: number, 
/**
 * 这一页管到哪个字节 ＝ 下一次增量该带的 `from`（最后一页 ⇒ 最后一个完整行的末字节）。
 */
end: number, 
/**
 * 下一页从哪个字节起（＝ 下一页第一条那一行的起点）；没有下一页 ⇒ `null`。
 */
next: number | null, 
/**
 * 每条用户输入，对话序。
 */
entries: Array<UserInputRow>, };
type UserInputsReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 起点（缺 ⇒ 0；增量传上次的 `end`，下一页传 `next`）。
 */
from?: number | null, 
/**
 * 可选：一页最多几条（一页另有字节上限 `REPLY_PAGE_BYTES`，先到哪个算哪个）。
 */
limit?: number | null, };
```

码：`bad_args` · `failed` · `too_large`

#### `history-tail`

一份会话的尾段从哪个字节起。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-tail`

入参 `HistoryTailReq` · 回包 `TailPlan`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HistoryTailReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 要最新几行。
 */
n: number, };
type TailPlan = { 
/**
 * 可计行总数（`line_counts` 口径）。
 */
total: number, 
/**
 * 尾段第一行的行号。
 */
tail_from: number, 
/**
 * 尾段第一行的字节起点（＝ 头段的字节终点）。
 */
split_at: number, 
/**
 * 最后一个完整行（`\n` 收尾）之后的字节位置；torn 残尾不在任何一段里。
 */
end: number, };
```

码：`bad_args` · `failed`

#### `session-fork`

从某条消息处分叉出一个新会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-fork`

入参 `ForkReq` · 回包 `Forked`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ForkLaunch = { cwd: ForkSlot<string>, 
/**
 * 值 `null` ＝ 账号 0，串 ＝ 号名。
 */
account: ForkSlot<string | null>, terminal: ForkSlot<ForkTerminal>, };
type ForkReq = { 
/**
 * 源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件）。
 */
sid: string, 
/**
 * 从哪条消息处分叉。
 */
uuid: string, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkTerminal = { "host": "tmux", terminal: string, } | { "host": "none" };
type ForkWhence = "record" | "process" | "terminal_list";
type ForkWhy = "exited" | "no_cwd" | "live_no_account";
type Forked = { 
/**
 * 新会话的 id。
 */
sessionId: string, 
/**
 * 新会话的落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖）。
 */
jsonlPath: string, 
/**
 * 起分叉出来的新会话要的三格。
 */
launch: ForkLaunch, };
```

码：`bad_args` · `fork_failed`

#### `resync`

手动对齐：重发这条流上的会话状态。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `ResyncReq` · 回包 `Resynced`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ResyncReq = { 
/**
 * 可缺：给了 ⇒ 只对那一个会话（关卡 2「对齐后重试」，不许空串）；不给 / `null` ⇒ 整机。
 */
sid?: string | null, };
type Resynced = { 
/**
 * 这次补宣告的会话数。
 */
added: number, 
/**
 * 补移除的会话数（各份 watcher 取最大：看的是同一台机器）。
 */
removed: number, 
/**
 * 这次真写了几处 `@ccm_sid`（值一样的不写）。
 */
retagged: number, 
/**
 * 在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加）。
 */
caught_up: number, 
/**
 * 几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份）。
 */
watchers: number, 
/**
 * 这台当下接得下但做不到的命令（同 hello 那一格）：握手之后才装上 tmux 的机器，靠「重新对齐」让客户端认出来。
 */
unavailable: Array<Unavailable>, 
/**
 * 这台当下开跑之后打不断的命令，按字母排（同 hello 那一格）。
 */
uncancellable: Array<string>, };
type Unavailable = { 
/**
 * 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::command_names`）。
 * ⚠ 它**必须**同时出现在 `commands` 里：本表说的是「接得下但做不到」，
 * 「根本不接」那一格由不在 `commands` 里表达（客户端 `accepts()` 一个字节都不发）。
 */
command: string, 
/**
 * 为什么做不到 —— **就是真调用那一刻会回的那个命令级 code**（如 `no_tmux`）。
 */
code: string, };
```

码：`bad_args`

#### `tasks-list`

一个会话的任务列表。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --tasks-list`

入参 `TasksListReq` · 回包 `TasksList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Task = { 
/**
 * 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）。
 */
id: string, 
/**
 * 同上。
 */
subject: string, 
/**
 * 同上。
 */
status: string, 
/**
 * 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务）。
 */
blocks: Array<string>, 
/**
 * 同上。
 */
blockedBy: Array<string>, 
/**
 * 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串。
 */
description?: string, 
/**
 * 同上。
 */
activeForm?: string, };
type TasksList = { 
/**
 * `<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序。
 */
tasks: Array<Task>, };
type TasksListReq = { 
/**
 * 会话 id（围栏在本体里：走出任务根 ⇒ `bad_args`）。
 */
sid: string, };
```

码：`bad_args` · `failed` · `too_large`

#### `resolve`

按 `ResumeSpec` 推出恢复命令 `CommandPlan`。

收 `args` · 可撤 · CLI：`ccm -- --resolve`

入参 `ResumeSpec` · 回包 `CommandPlan`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Capabilities = { supportsSendKeys: boolean, supportsCapture: boolean, supportsMultiClient: boolean, supportsMultiWindow: boolean, };
type CommandPlan = { 
/**
 * **唯一可信的一项**。
 */
command: string, 
/**
 * "PtyInject"（resume 走 pty send-keys 注入 §5④）| "ExecOnce"（未来）。MVP 恒 PtyInject。
 */
mode: string, 
/**
 * **典型档，不是探测结果**（见结构体头注）。
 */
capabilities: Capabilities, 
/**
 * **纯从 sid 派生，不是探测结果**（见结构体头注）——
 * 拿它去 attach 一个「并不存在」的 tmux 会话是现实风险。要判断会话是否真的存在，
 * 问 `terminals-list`（那是真 `tmux list-panes`）。
 */
sessionName?: string, launchLabel?: string, substitutedFrom?: string, };
type ResumeSpec = { sessionId: string, 
/**
 * 可缺：候选启动命令，取第一个非空的；都没有 ⇒ 这一家的默认启动器。
 */
launchCandidates?: Array<string | null>, 
/**
 * 会话属哪个 agent kind ⇒ 据此构 `codex resume <uuid>` 或 `claude --resume`（additive，线上 `agentKind`）。
 * 注册表里的一家 = 那一家；缺 / `""` = 默认那一家；别的名字 ⇒ `bad_args`。
 */
agentKind?: string, };
```

码：`bad_args` · `invalid_session_id` · `unsafe_launch_candidate`

### 4.5 资产

#### `footprint-report`

「足迹」由这台后端出整份成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --footprint-report`

入参 `FootprintReq` · 回包 `ConfigSurfaceReport`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ConfigSurfaceReport = { rows: Array<SurfaceRow>, settings_scopes: Array<SettingsScope>, 
/**
 * 解析基准，展示用（让用户知道 `~/.claude` 被解释成了哪里）。
 */
agent_home: string, home: string, };
type EnvTier = "AppInstalls" | "AppShipsNoInstallerYet" | "UserInstallsWePrompt" | "AppOnlyChecks";
type FootprintClient = { home: string, path?: string | null, };
type FootprintReq = { client?: FootprintClient | null, };
type SettingsScope = { scope: string, path: string, state: SurfaceState, 
/**
 * 文件里有没有 cc-bus 那两个钩子程序的字样。读不到 → `None`（**不猜**）。
 */
has_cc_bus_hooks: boolean | null, precedence_note: string, };
type SurfaceRow = { tool_id: string, tool_name: string, 
/**
 * 🔴 **档进线上形状了。**
 *
 * 上一版 `config-surface-section.ts` 的 `describeUndo` 逐字写着
 * 「⚠ 两类**今天在行上分不开**：`SurfaceRow` 的线上形状里没有档这一格」——
 * 于是「不该由我们装」与「该我们装而还没写」在前端只能靠一句和稀泥的措辞盖过去。
 * 今天档在行上，前端按**值**分档，不按措辞猜。
 */
tier: EnvTier, source_label: string, path_declared: string, 
/**
 * 解析出的本机路径（远端 / 项目相对 / `$PROFILE` 一律 `None`）。
 */
path_resolved: string | null, note: string | null, host_label: string, effect_label: string, state: SurfaceState, installable: boolean, uninstallable: boolean, };
type SurfaceState = { "kind": "present", detail: string, } | { "kind": "absent" } | { "kind": "undetermined", why: string, };
```

码：`bad_args` · `failed`

#### `data-report`

「文件与数据」那一份成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --data-report`

入参 `FootprintReq` · 回包 `DataReport`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ChangedFile = { path: string, what: string, 
/**
 * 撤回在哪（设置窗的页 · 栏 · 锚点）；没有 ⇒ `null`。
 */
undo: UndoAt | null, };
type Chore = { id: string, kind: ChoreKind, state: ChoreState, name: string, loc: string, said: string, why: string, steps: Array<string>, diff: Array<DiffRow>, copy: string | null, whole: string | null, 
/**
 * 「整份」盖了哪几件（同一份文件里两件都没做 ⇒ 两件的 id）。
 */
wholeCovers: Array<string>, file: string | null, 
/**
 * 去哪一处做（设置窗的页 · 栏 · 锚点）。
 */
go: UndoAt | null, howUrl: string | null, 
/**
 * 显示时要遮住的那把钥匙。
 */
mask: string | null, action: ChoreAction, };
type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
type ChoreKind = "must" | "decide" | "install" | "installOptional" | "optional";
type ChoreState = "todo" | "done" | "expired" | "blocked" | "declined";
type DataReport = { 
/**
 * 这台家目录（显示时 `~` 缩写按它）。
 */
home: string, changedFiles: Array<ChangedFile>, todo: Array<Chore>, 
/**
 * 这台有没有 tmux（查不动 ⇒ `null`）。
 */
tmux: boolean | null, 
/**
 * 「待办」里进角标的件数：要做 ＋ 要装 ＋ 待定，还没做完的。
 */
chores: number, own: Array<OwnItem>, };
type DiffOp = "same" | "del" | "add";
type DiffRow = { n: number | null, op: DiffOp, text: string, };
type FootprintClient = { home: string, path?: string | null, };
type FootprintReq = { client?: FootprintClient | null, };
type OwnClass = "truth" | "cache";
type OwnItem = { 
/**
 * 闭集同 `relay_route_core::OWN_HOME_ENTRIES`。
 */
id: string, 
/**
 * 是什么（写好的那一句，界面照抄）。
 */
what: string, 
/**
 * `~/.cc-monitor/…` 写法。
 */
path: string, dir: boolean, class: OwnClass, exists: boolean, 
/**
 * 文件的字节数；目录 / 不在 ⇒ `null`。
 */
size: number | null, };
type UndoAt = { page: string, tab?: string, anchor?: string, };
```

码：`bad_args` · `failed`

#### `first-run`

首次运行「开始用」三步各自打没打勾。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --first-run`

入参 `FirstRunReq` · 回包 `FirstRun`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FirstRun = { steps: Array<FirstRunStep>, 
/**
 * 必做而还没打勾的几步（主窗口状态栏那一枚只数它）。
 */
left: number, 
/**
 * 「开始用」那一块点过「跳过」（`chores-mark` 的 `skipStart` 写）。
 */
skipped: boolean, };
type FirstRunReq = { 
/**
 * 机器表里有几台远端（机器表住 monitor 那一侧，问的那一方带上）。
 */
remotes: number, };
type FirstRunStep = { id: FirstRunStepId, done: boolean, 
/**
 * 必做（今天只有 `terminal`）。
 */
required: boolean, };
type FirstRunStepId = "terminal" | "named" | "remote";
```

码：`bad_args`

#### `last-seen-read`

读离线那台的上次值。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --last-seen-read`

入参 `LastSeenReadReq` · 回包 `LastSeen`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ChangedFile = { path: string, what: string, 
/**
 * 撤回在哪（设置窗的页 · 栏 · 锚点）；没有 ⇒ `null`。
 */
undo: UndoAt | null, };
type Chore = { id: string, kind: ChoreKind, state: ChoreState, name: string, loc: string, said: string, why: string, steps: Array<string>, diff: Array<DiffRow>, copy: string | null, whole: string | null, 
/**
 * 「整份」盖了哪几件（同一份文件里两件都没做 ⇒ 两件的 id）。
 */
wholeCovers: Array<string>, file: string | null, 
/**
 * 去哪一处做（设置窗的页 · 栏 · 锚点）。
 */
go: UndoAt | null, howUrl: string | null, 
/**
 * 显示时要遮住的那把钥匙。
 */
mask: string | null, action: ChoreAction, };
type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
type ChoreKind = "must" | "decide" | "install" | "installOptional" | "optional";
type ChoreState = "todo" | "done" | "expired" | "blocked" | "declined";
type DataReport = { 
/**
 * 这台家目录（显示时 `~` 缩写按它）。
 */
home: string, changedFiles: Array<ChangedFile>, todo: Array<Chore>, 
/**
 * 这台有没有 tmux（查不动 ⇒ `null`）。
 */
tmux: boolean | null, 
/**
 * 「待办」里进角标的件数：要做 ＋ 要装 ＋ 待定，还没做完的。
 */
chores: number, own: Array<OwnItem>, };
type DiffOp = "same" | "del" | "add";
type DiffRow = { n: number | null, op: DiffOp, text: string, };
type LastSeen = { accounts: SeenEntry | null, data: SeenData | null, };
type LastSeenReadReq = { 
/**
 * 哪台（机器名）。
 */
origin: Origin, };
type Origin = string;
type OwnClass = "truth" | "cache";
type OwnItem = { 
/**
 * 闭集同 `relay_route_core::OWN_HOME_ENTRIES`。
 */
id: string, 
/**
 * 是什么（写好的那一句，界面照抄）。
 */
what: string, 
/**
 * `~/.cc-monitor/…` 写法。
 */
path: string, dir: boolean, class: OwnClass, exists: boolean, 
/**
 * 文件的字节数；目录 / 不在 ⇒ `null`。
 */
size: number | null, };
type SeenData = { "state": "kept", 
/**
 * 记下的时刻（毫秒）。
 */
atMs: number, value: DataReport, } | { "state": "unreadable", atMs: number, };
type SeenEntry = { 
/**
 * 记下的时刻（毫秒）。
 */
atMs: number, 
/**
 * 那一份应答原样。
 */
value: Record<string, unknown>, };
type UndoAt = { page: string, tab?: string, anchor?: string, };
```

码：`bad_args` · `io_failed`

#### `last-seen-write`

记下一台这一次读成的那一份（或清掉那台）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --last-seen-write`

入参 `LastSeenWriteReq` · 回包 `LastSeenWritten`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LastSeenWriteReq = { origin: Origin, kind?: SeenKind, 
/**
 * 那一份应答（对象，序列化后 ≤ 256 KiB）。
 */
value?: Record<string, unknown>, 
/**
 * `true` ⇒ 清掉这台的上次值（删机器时）。
 */
forget?: boolean, };
type LastSeenWritten = { 
/**
 * 记下的时刻（毫秒）。
 */
atMs: number, };
type Origin = string;
type SeenKind = "accounts" | "data";
```

码：`bad_args` · `io_failed` · `too_large`

#### `chores-mark`

记下「待办」里的一个选择。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --chores-mark`

入参 `ChoresMarkReq` · 回包 `ChoresMarks`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ChoresMarkReq = { "op": "decline", id: string, } | { "op": "undecline", id: string, } | { "op": "selfPaste", rc: string, } | { "op": "unselfPaste", } | { "op": "skipStart", } | { "op": "unskipStart", };
type ChoresMarks = { 
/**
 * 点过「不用了」的那几件（件的 `id`）。
 */
declined: Array<string>, 
/**
 * 选了「我自己贴」的那份启动文件（绝对路径）；没选 ⇒ `None`。
 */
selfPaste: string | null, 
/**
 * 首次运行「开始用」那一块点过「跳过」（`footprint/readiness.rs` 读它）。
 */
startSkipped: boolean, };
```

码：`bad_args` · `io_failed` · `marks_unreadable`

#### `agent-home-check`

那个目录能不能当 agent 家目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --agent-home-check`

入参 `AgentHomeCheckReq` · 回包 `AgentHomeCheck`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentHomeCheck = { state: AgentHomeState, };
type AgentHomeCheckReq = { 
/**
 * 要查的目录：绝对路径，或 `~/` 开头（按这台家目录展开）。
 */
path: string, };
type AgentHomeState = "ok" | "missing" | "not_dir" | "no_records";
```

码：`bad_args`

#### `assets-catalog`

这台现扫一次、记下，回整份资产目录 ＋「这台缺什么」。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --assets-catalog`

入参 `NoArgs` · 回包 `CatalogReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CatalogAsset = { kind: string, name: string, 
/**
 * 那台上的项目目录（项目级）；`None` = 用户级。
 */
project?: string, 
/**
 * 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
 */
dir?: string, digest: string, 
/**
 * 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义）。
 */
summary: unknown, };
type CatalogFrom = { 
/**
 * 那台目录的 id。
 */
machine: string, 
/**
 * 那台上的项目目录；用户级 ⇒ `null`。
 */
project: string | null, digest: string, summary: unknown, };
type CatalogMachine = { id: string, label: string, gen: number, seenAt: number, assets: Array<CatalogAsset>, projectDirs: Array<string>, notes: { [key in string]: CatalogNote }, sharedMcp: boolean, };
type CatalogNote = { text: string, rev: number, };
type CatalogReply = { 
/**
 * 这台机器的 id。
 */
self: string, machines: Array<CatalogMachine>, rows: Array<CatalogRow>, 
/**
 * 这一趟扫描读不出来的那几份（一句话一份）。
 */
problems: Array<string>, 
/**
 * 这一趟目录有没有变（变了才写盘）。
 */
changed: boolean, 
/**
 * 目录文件在这台上的路径。
 */
path: string | null, };
type CatalogRow = { kind: string, name: string, state: string, from: Array<CatalogFrom>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `assets-catalog-merge`

把另一台后端的整份目录并进来。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --assets-catalog-merge`

入参 `CatalogMergeReq` · 回包 `CatalogReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CatalogAsset = { kind: string, name: string, 
/**
 * 那台上的项目目录（项目级）；`None` = 用户级。
 */
project?: string, 
/**
 * 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
 */
dir?: string, digest: string, 
/**
 * 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义）。
 */
summary: unknown, };
type CatalogFrom = { 
/**
 * 那台目录的 id。
 */
machine: string, 
/**
 * 那台上的项目目录；用户级 ⇒ `null`。
 */
project: string | null, digest: string, summary: unknown, };
type CatalogIn = { machines: Array<CatalogMachine>, };
type CatalogMachine = { id: string, label: string, gen: number, seenAt: number, assets: Array<CatalogAsset>, projectDirs: Array<string>, notes: { [key in string]: CatalogNote }, sharedMcp: boolean, };
type CatalogMergeReq = { catalog: CatalogIn, };
type CatalogNote = { text: string, rev: number, };
type CatalogReply = { 
/**
 * 这台机器的 id。
 */
self: string, machines: Array<CatalogMachine>, rows: Array<CatalogRow>, 
/**
 * 这一趟扫描读不出来的那几份（一句话一份）。
 */
problems: Array<string>, 
/**
 * 这一趟目录有没有变（变了才写盘）。
 */
changed: boolean, 
/**
 * 目录文件在这台上的路径。
 */
path: string | null, };
type CatalogRow = { kind: string, name: string, state: string, from: Array<CatalogFrom>, };
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `assets-sync`

本机常驻后端沿池里那条 SSH 同步资产目录。

收 `args` · 可撤 · CLI：`ccm -- --assets-sync`

入参 `AssetsSyncReq` · 回包 `AssetsSynced`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AssetsSyncReq = { 
/**
 * 那台的名字（monitor 的 origin 名）。
 */
origin?: string, 
/**
 * 那台的拨号（只有路径，没有私钥本体）。
 */
dial?: MachineDial, };
type AssetsSyncRow = { 
/**
 * 可达表的键（monitor 交来的 origin 串）。
 */
origin: Origin, 
/**
 * 那台目录的 `self`（没拉成 ⇒ `null`）。
 */
peer: string | null, 
/**
 * 本机目录因这一趟变了没有。
 */
changed: boolean, 
/**
 * 推过去几台快照。
 */
pushed: number, 
/**
 * 那一趟哪里没办成（`null` ＝ 全办成了）。
 */
error: string | null, };
type AssetsSynced = { 
/**
 * 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>`。
 */
self: string, synced: Array<AssetsSyncRow>, 
/**
 * 可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）。
 */
reach: Array<ReachRow>, };
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type MachineDial = { machine: MachineConfig, saved?: MachineConfig | null, jump?: MachineConfig | null, prefer?: Prefer | null, 
/**
 * 用法（存着不读：问那台的每一趟按自己的用法改，`capture` · `forward`）。
 */
use?: string | null, 
/**
 * ssh-agent 套接字的路径（界面进程此刻的 `SSH_AUTH_SOCK`；缺 ⇒ 用本进程自己的）。
 */
agent_sock?: string | null, };
type Origin = string;
type Prefer = { host: string, port: number, };
type ReachRow = { 
/**
 * 那台的名字。
 */
origin: Origin, 
/**
 * 对面的资产目录 id。
 */
machine: string | null, };
```

码：`bad_args` · `io_failed` · `unreachable`

#### `skill-read`

读来源那台上的一个 skill。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-read`

入参 `SkillReadReq` · 回包 `SkillRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SkillRead = { 
/**
 * 这台 skill 的根。
 */
root: string, 
/**
 * 这个 skill 的目录（绝对路径）。
 */
dir: string, files: Array<SkillReadFile>, 
/**
 * 没读的那几处（指向目录的链接 / 特殊文件）。
 */
skipped: Array<string>, };
type SkillReadFile = { 
/**
 * skill 里的相对路径（`/` 分段）。
 */
path: string, 
/**
 * 原文；不是文本 / 太大 / 读不出来 ⇒ `null`（这一个装不过去），原因在 `why`。
 */
text: string | null, bytes: bigint, exec: boolean, why: string | null, };
type SkillReadReq = { 
/**
 * skill 的目录名（一段：不许分隔符 / `..` / 点开头）。
 */
name: string, project: string | null, };
```

码：`bad_args` · `io_failed` · `not_found` · `too_large`

#### `skill-install-plan`

在要被写的那一台判 skill 装不装得过来。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-plan`

入参 `SkillPlanReq` · 回包 `SkillPlan`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SkillLedgerFile = { digest: string, created: boolean, };
type SkillPlan = { root: string, dir: string, 
/**
 * 写的时候 `files-put` 用的 `root`。
 */
base: string, 
/**
 * 写的时候 `files-put` 用的相对前缀：`rel` = `<prefix>/<path>`。
 */
prefix: string, rows: Array<SkillPlanRow>, target: Array<SkillTargetFile>, 
/**
 * 没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序）。
 */
write: Array<string> | null, 
/**
 * 没给 `take` ⇒ `null`。
 */
ledger: { [key in string]: SkillLedgerFile } | null, };
type SkillPlanReq = { name: string, source: Array<SkillSourceFile>, project: string | null, take: Array<string> | null, overwrite: Array<string> | null, };
type SkillPlanRow = { path: string, state: string, suspects: Array<SkillSuspect>, 
/**
 * 这台那一份盖不了（读不出原文 / 不是普通文件）的原因。
 */
blocked: string | null, };
type SkillSourceFile = { path: string, 
/**
 * `null` = 装不过去的那一个。
 */
text: string | null, exec: boolean, };
type SkillSuspect = { kind: string, value: string, there: string | null, };
type SkillTargetFile = { path: string, text: string, };
```

码：`bad_args` · `bad_file` · `io_failed` · `needs_consent` · `too_large`

#### `skill-install-record`

skill 装记录的写口。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-record`

入参 `RecordReq` · 回包 `RecordReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LedgerFile = { digest: string, created: boolean, };
type RecordAt = "home";
type RecordReply = { 
/**
 * 这一条记录的目录（MCP：那份配置文件）。
 */
dir: string, name: string, 
/**
 * 记录变没变（没变不写）。
 */
changed: boolean, 
/**
 * 这一条还剩几个文件（MCP：那份配置里还记着几条）。
 */
remaining: number, };
type RecordReq = { "op": "add", 
/**
 * skill 的目录名。
 */
name: string, files: { [key in string]: LedgerFile }, 
/**
 * 给了 ⇒ 目录按那个项目里的 skill 根算。
 */
project?: string, 
/**
 * 缺 ⇒ 目录按 skill 根算；`home` ⇒ 目录 = 记录所在那个家目录（装在家目录底下、不在 skill 根下的东西用）。
 */
at?: RecordAt, } | { "op": "drop", dir: string, paths: Array<string>, } | { "op": "mcp-add", file: string, name: string, digest: string, } | { "op": "mcp-drop", file: string, name: string, };
```

码：`bad_args` · `io_failed` · `ledger_unreadable` · `not_found` · `too_large`

#### `mcp-read`

这台机器的 MCP 列表成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-read`

入参 `McpReadReq` · 回包 `McpReadReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type McpFailedMark = { said: string, tip: string, detail: string | null, };
type McpLoginMark = { said: string, tip: string, copy: string, };
type McpMark = "needsLogin" | "failed";
type McpReadEntry = { 
/**
 * 闭集 `user` · `local` · `project`（适配层那一格写的）。
 */
scope: "user" | "local" | "project", name: string, 
/**
 * 那一条配置原样（未知字段不丢）。
 */
server: unknown, 
/**
 * 读自哪一份文件。
 */
sourcePath: string, 
/**
 * 配置层只说三种：`needsLogin` · `disabled` · `unknown`（不说连上了）。
 */
status: "needsLogin" | "disabled" | "unknown", 
/**
 * `needsLogin` 时：在哪几个号里要登录；别的状态恒空。
 */
loginIn: Array<string>, 
/**
 * `needsLogin` 时：最近一次看到是何时（epoch ms）；别的状态 `null`。
 */
seenAt: number | null, 
/**
 * 小标画哪一种：断开压过需登录；都没有 ⇒ `null`（只说装了，不说连上了）。
 */
mark: McpMark | null, login: McpLoginMark | null, failed: McpFailedMark | null, };
type McpReadReply = { entries: Array<McpReadEntry>, 
/**
 * agent 用户级配置里那张项目表的键（排序）：用过的项目目录。
 */
dirs: Array<string>, 
/**
 * 在而读不出 / 不是 JSON 的那几份各一句；不在的静默。
 */
problems: Array<string>, };
type McpReadReq = { 
/**
 * 项目目录（绝对路径；缺 · `null` ⇒ 不看项目那一段）。
 */
projectDir?: string, };
```

码：`bad_args` · `too_large`

#### `cc-bus-install-state`

装 cc-bus 到这台之前看一眼。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --cc-bus-install-state`

入参 `NoReq` · 回包 `CcBusState`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CcBusState = { 
/**
 * 落点（`<skills 根>/cc-bus`）。
 */
dest: string, 
/**
 * 内容会变的那几个（缺的 ＋ 不一样的）；空 ⇒ 已是这一版。
 */
writes: Array<string>, 
/**
 * 落点上已经有东西（要写时先整个改名留作备份）。
 */
existing: boolean, 
/**
 * 内嵌那一份的摘要。
 */
version: string, };
type NoReq = Record<symbol, never>;
```

码：`bad_args` · `refused`

#### `cc-bus-install`

把这台二进制带着的 cc-bus 装到这台。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --cc-bus-install`

入参 `NoReq` · 回包 `CcBusInstalled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CcBusInstalled = { dest: string, 
/**
 * 写了的那几个相对路径（全一致 ⇒ 空）。
 */
written: Array<string>, 
/**
 * 覆盖前整个目录改名成的那一份（`null` = 之前没装过）。
 */
backup: string | null, recordFailed: string | null, };
type NoReq = Record<symbol, never>;
```

码：`bad_args` · `bad_file` · `refused`

#### `ext-hub-preview`

装到几台之前那一张卡：本机后端当枢纽，向各台问完并好。

收 `args` · 可撤 · 只在流上

入参 `HubReq` · 回包 `HubPreview`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtCard = { kind: ExtKind, name: string, 
/**
 * 被写那台上的落点（skill 目录 / MCP 配置文件）。
 */
path: string, 
/**
 * 要写的那几个（skill：目录里的相对路径；MCP：那份配置文件）。
 */
writes: Array<string>, 
/**
 * 装上之后和现在一样（没什么可写）。
 */
unchanged: boolean, 
/**
 * 要留意的几件（说人话）。
 */
suspects: Array<string>, 
/**
 * 装不了的原因（有它就不给确认）。
 */
stop: string | null, 
/**
 * MCP：装上之后那一条（待填的值是 `null`）。
 */
config: string | null, slots: Array<ExtSlot>, tokens: ExtTokens, };
type ExtKind = "skill" | "mcp";
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtSlot = { 
/**
 * `env` 或 `headers`。
 */
field: string, key: string, 
/**
 * 目标机那一条里已有这个键的值（可以沿用；不填就沿用）。
 */
kept: boolean, };
type ExtTarget = { at: ExtLoc, ok: boolean, note: string | null, };
type ExtTokens = { source: string, target: string | null, };
type HubMachineCard = { to: string | null, name: string, card: ExtCard | null, 
/**
 * 会写的文件那一行（MCP：那份配置文件 ＋ 键；skill：目录 ＋ 要写的几个）。
 */
files: Array<string>, error: string | null, detail: string | null, };
type HubPreview = { 
/**
 * 各台能装的各处并起来：勾上的每台都能装才 `ok`。
 */
places: Array<ExtTarget>, 
/**
 * 共用的那一处（没有每台都能装的 ⇒ `null`）。
 */
place: ExtLoc | null, slots: Array<HubSlot>, machines: Array<HubMachineCard>, };
type HubReq = { kind: ExtKind, name: string, to: Array<string | null>, place: ExtLoc | null, };
type HubSlot = { field: string, key: string, kept: Array<string>, };
```

码：`bad_args` · `missing` · `catalog_unreadable` · `io_failed`

#### `ext-hub-apply`

装到几台，本机后端当枢纽；各台各自结局。

收 `args` · 可撤 · 只在流上

入参 `HubApplyReq` · 回包 `HubApplied`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtDone = { path: string, 
/**
 * 写了 / 删了的那几个。
 */
changed: Array<string>, 
/**
 * 做成了，但有一件要知道的（执行位没改成 · 没记下来 · 空目录没收掉 …）。
 */
note: string | null, };
type ExtKind = "skill" | "mcp";
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtTokens = { source: string, target: string | null, };
type HubApplied = { machines: Array<HubMachineDone>, };
type HubApplyReq = { kind: ExtKind, name: string, to: Array<string | null>, place: ExtLoc | null, tokens: { [key in string]: ExtTokens }, fill: { [key in string]: { [key in string]: string } }, };
type HubMachineDone = { to: string | null, name: string, done: ExtDone | null, error: string | null, detail: string | null, };
```

码：`bad_args` · `missing` · `catalog_unreadable` · `io_failed`

#### `ext-list`

设置「扩展」页那张表。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `ExtListReq` · 回包 `ExtList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtBring = { from: string | null, 
/**
 * 来源给人看的名字（本机 · 别的台 · cc-monitor 自带）。
 */
fromName: string, scope: ExtScope, targets: Array<ExtTarget>, };
type ExtBuiltin = { note: string, 
/**
 * 装上之后要在那台的 agent 配置里加钩子：抽屉里每台一行钩子状态（问那台的 `hooks-diag`）。
 */
hooks: boolean, };
type ExtCell = { 
/**
 * 表上那个点。
 */
state: ExtState, 
/**
 * 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目。
 */
places: Array<ExtPlace>, bring: ExtBring | null, 
/**
 * 没有「装到…」时为什么（现状 ＋ 能做什么）。
 */
note: string | null, };
type ExtDetail = { label: string, value: string, };
type ExtKind = "skill" | "mcp";
type ExtList = { machines: Array<ExtMachine>, rows: Array<ExtRow>, problems: Array<string>, };
type ExtListReq = { 
/**
 * `true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算。缺 · `null` ⇒ 不算。
 */
visit?: boolean, };
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtMachine = { 
/**
 * 枢纽认它的键：本机后端自己 = `null`；别的台 = 可达表里的名字；没连上 = `null` 且 `reachable = false`。
 */
key: string | null, 
/**
 * 就是本机后端这一台。
 */
here: boolean, reachable: boolean, 
/**
 * 给人看的名字（可达表里的名字；没有就是那台自报的称呼）。
 */
name: string, 
/**
 * 那台上开过会话的项目目录（装到项目时给人选）。
 */
projects: Array<string>, };
type ExtPlace = { at: ExtLoc, 
/**
 * 全局那一行：`same` · `differs` · `missing`；项目那几行：`same` · `differs`（与「这一版」比）。
 */
state: ExtState, 
/**
 * skill：那一处的目录（「在文件窗口里打开」用）。
 */
dir: string | null, 
/**
 * 这一处有「卸载」。
 */
uninstall: boolean, 
/**
 * 有它、却没有「卸载」时为什么。
 */
note: string | null, };
type ExtRow = { kind: ExtKind, name: string, about: string | null, detail: Array<ExtDetail>, new: boolean, builtin: ExtBuiltin | null, 
/**
 * 用户写的备注（随目录在后端之间同步）。
 */
note: string | null, cells: Array<ExtCell>, };
type ExtScope = { from: ExtLoc, to: ExtLoc, };
type ExtState = "same" | "differs" | "missing" | "project";
type ExtTarget = { at: ExtLoc, ok: boolean, note: string | null, };
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-list-here`

设置「扩展」页那张表的本机那一半（不问可达表）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-list-here`

入参 `ExtListReq` · 回包 `ExtList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtBring = { from: string | null, 
/**
 * 来源给人看的名字（本机 · 别的台 · cc-monitor 自带）。
 */
fromName: string, scope: ExtScope, targets: Array<ExtTarget>, };
type ExtBuiltin = { note: string, 
/**
 * 装上之后要在那台的 agent 配置里加钩子：抽屉里每台一行钩子状态（问那台的 `hooks-diag`）。
 */
hooks: boolean, };
type ExtCell = { 
/**
 * 表上那个点。
 */
state: ExtState, 
/**
 * 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目。
 */
places: Array<ExtPlace>, bring: ExtBring | null, 
/**
 * 没有「装到…」时为什么（现状 ＋ 能做什么）。
 */
note: string | null, };
type ExtDetail = { label: string, value: string, };
type ExtKind = "skill" | "mcp";
type ExtList = { machines: Array<ExtMachine>, rows: Array<ExtRow>, problems: Array<string>, };
type ExtListReq = { 
/**
 * `true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算。缺 · `null` ⇒ 不算。
 */
visit?: boolean, };
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtMachine = { 
/**
 * 枢纽认它的键：本机后端自己 = `null`；别的台 = 可达表里的名字；没连上 = `null` 且 `reachable = false`。
 */
key: string | null, 
/**
 * 就是本机后端这一台。
 */
here: boolean, reachable: boolean, 
/**
 * 给人看的名字（可达表里的名字；没有就是那台自报的称呼）。
 */
name: string, 
/**
 * 那台上开过会话的项目目录（装到项目时给人选）。
 */
projects: Array<string>, };
type ExtPlace = { at: ExtLoc, 
/**
 * 全局那一行：`same` · `differs` · `missing`；项目那几行：`same` · `differs`（与「这一版」比）。
 */
state: ExtState, 
/**
 * skill：那一处的目录（「在文件窗口里打开」用）。
 */
dir: string | null, 
/**
 * 这一处有「卸载」。
 */
uninstall: boolean, 
/**
 * 有它、却没有「卸载」时为什么。
 */
note: string | null, };
type ExtRow = { kind: ExtKind, name: string, about: string | null, detail: Array<ExtDetail>, new: boolean, builtin: ExtBuiltin | null, 
/**
 * 用户写的备注（随目录在后端之间同步）。
 */
note: string | null, cells: Array<ExtCell>, };
type ExtScope = { from: ExtLoc, to: ExtLoc, };
type ExtState = "same" | "differs" | "missing" | "project";
type ExtTarget = { at: ExtLoc, ok: boolean, note: string | null, };
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-note-set`

写 / 改 / 清一个扩展的备注。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-note-set`

入参 `ExtNoteReq` · 回包 `ExtNote`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtKind = "skill" | "mcp";
type ExtNote = { note: string | null, };
type ExtNoteReq = { kind: ExtKind, name: string, 
/**
 * 备注正文（首尾空白去掉；空串 = 清掉；最长 [`NOTE_MAX_CHARS`] 字，超了拒、不截断）。
 */
text: string, };
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-uninstall-preview`

从这台卸一个扩展之前那张卡。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-uninstall-preview`

入参 `ExtUninstallReq` · 回包 `ExtUninstallCard`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtKind = "skill" | "mcp";
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtUninstallCard = { kind: ExtKind, name: string, path: string, 
/**
 * cc-monitor 装的（装记录里有）。
 */
recorded: boolean, 
/**
 * 要删的那几个（skill：相对路径；MCP：那份配置文件里的那一条）。
 */
files: Array<string>, 
/**
 * 删之前先放到哪（不是 cc-monitor 装的才有）。
 */
backup: string | null, 
/**
 * 这一趟会做什么（说人话：装的 / 不是装的 · 改过没有 · 删了回不回得去）。
 */
said: string, token: string, };
type ExtUninstallReq = { kind: ExtKind, name: string, 
/**
 * 在这台哪一级（项目目录须是这台上的绝对路径）。
 */
at: ExtLoc, };
```

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `not_found` · `refused`

#### `ext-uninstall-apply`

从这台卸一个扩展。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-uninstall-apply`

入参 `ExtUninstallApplyReq` · 回包 `ExtDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtDone = { path: string, 
/**
 * 写了 / 删了的那几个。
 */
changed: Array<string>, 
/**
 * 做成了，但有一件要知道的（执行位没改成 · 没记下来 · 空目录没收掉 …）。
 */
note: string | null, };
type ExtKind = "skill" | "mcp";
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtUninstallApplyReq = { kind: ExtKind, name: string, at: ExtLoc, token: string, };
```

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `needs_consent` · `not_found` · `refused` · `stale`

#### `mcp-sync-source`

装到别的机器时来源那一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-source`

入参 `McpSourceReq` · 回包 `McpSource`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type McpSource = { 
/**
 * 读的是哪一份。
 */
path: string, 
/**
 * 那一条；`env` / `headers` 的值在这台就换成 `null`。
 */
def: unknown, slots: Array<McpSourceSlot>, 
/**
 * 按原样那一条（含密钥值）算的记号。
 */
token: string, };
type McpSourceReq = { name: string, at: ExtLoc, };
type McpSourceSlot = { 
/**
 * `env` · `headers`。
 */
field: string, key: string, };
```

码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `missing` · `refused`

#### `mcp-sync-preview`

在要被写的那一台看装上之后那一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-preview`

入参 `McpPreviewReq` · 回包 `McpPreview`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtSlot = { 
/**
 * `env` 或 `headers`。
 */
field: string, key: string, 
/**
 * 目标机那一条里已有这个键的值（可以沿用；不填就沿用）。
 */
kept: boolean, };
type McpPreview = { 
/**
 * 这台那份的路径。
 */
path: string, 
/**
 * `new` · `same` · `differs`。
 */
state: string, def: unknown, slots: Array<ExtSlot>, suspects: Array<McpSuspect>, 
/**
 * 这台那份的记号（不存在 ⇒ `null`）—— 写的时候原样交回。
 */
target: string | null, };
type McpPreviewReq = { name: string, at: ExtLoc, def: unknown, };
type McpSuspect = { kind: string, field: string, value: string, there: string | null, };
```

码：`bad_args` · `bad_file` · `bad_path` · `refused`

#### `mcp-sync-apply`

在要被写的那一台把那一条写进去。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-sync-apply`

入参 `McpApplyReq` · 回包 `McpApplied`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type McpApplied = { path: string, 
/**
 * 真写了吗（与原有那一条逐字相同 ⇒ `false`）。
 */
written: boolean, recordFailed: string | null, };
type McpApplyReq = { name: string, at: ExtLoc, def: unknown, fill: { [key in string]: { [key in string]: string } } | null, target: string | null, };
```

码：`bad_args` · `bad_file` · `bad_path` · `needs_input` · `refused` · `stale`

#### `skill-install-apply`

在要被写的那一台把勾的那几个 skill 文件写进去。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --skill-install-apply`

入参 `SkillApplyReq` · 回包 `SkillApplied`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SkillApplied = { dir: string, 
/**
 * 真写成了的那几个（按写的顺序）。
 */
written: Array<string>, 
/**
 * 写成了但执行位没置上的那几个。
 */
chmodFailed: Array<string>, 
/**
 * 装记录没记下来时那一句（`null` = 记下了）。
 */
recordFailed: string | null, };
type SkillApplyReq = { name: string, project: string | null, source: Array<SkillSourceFile>, target: Array<SkillTargetFile>, take: Array<string> | null, overwrite: Array<string> | null, };
type SkillSourceFile = { path: string, 
/**
 * `null` = 装不过去的那一个。
 */
text: string | null, exec: boolean, };
type SkillTargetFile = { path: string, text: string, };
```

码：`bad_args` · `bad_file` · `io_failed` · `needs_consent` · `stale` · `too_large`

#### `hooks-diag`

cc-bus 钩子诊断。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --hooks-diag`

入参 `NoArgs` · 回包 `HooksReport`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HookState = { "kind": "not-installed" } | { "kind": "installed-via-path", command: string, } | { "kind": "installed-at-path", command: string, path: string, } | { "kind": "path-missing", command: string, path: string, } | { "kind": "unknown", command: string, };
type HooksDiagnosis = { session_start: HookState, stop: HookState, note: string, };
type HooksReport = { 
/**
 * 这台跑得了 cc-bus（它要 tmux）；跑不了 ⇒ 不给要加的内容，界面只说这台不支持自动收信。
 */
supported: boolean, diagnosis: HooksDiagnosis, 
/**
 * 要合并进那份 `settings.json` 的内容；这台的 cc-bus 没装（那两个脚本不在）⇒ `None`。
 */
snippet: string | null, 
/**
 * 读的是哪份文件（这台后端看到的路径）。
 */
source: string, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `too_large`

### 4.6 别名

#### `aliases-read`

启动文件候选（接入那一格）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --aliases-read`

入参 `AliasesReadReq` · 回包 `AliasListing`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AliasBlockState = { 
/**
 * 这份文件里有没有 cc-monitor 的别名块（**悬空的 BEGIN 也算在**，见 `block_presence`）。
 */
present: boolean, 
/**
 * 块外自己定义的、与清单里某条同名的函数（只提醒，不替人改）。
 */
conflictingFunctions: Array<AliasNameClash>, };
type AliasListing = { 
/**
 * 这台的家目录（界面拿它把路径写成 `~/…` 短形，只做这一步缩写）。
 */
home: string, 
/**
 * 这台机器上这种 shell 的启动文件候选（「那一行 source 加进哪份」·「别名块装进哪份」，同一批）。
 */
rcCandidates: Array<AliasStartupFile>, 
/**
 * 人另指的那一份（`read_via` 的 `extra_rc`）过了围栏之后的绝对路径 —— 界面拿它在候选里认出「刚指的是哪一份」。
 */
otherRc: string | null, };
type AliasNameClash = { name: string, 
/**
 * 1 起的行号。
 */
line: number, 
/**
 * 新开的终端里敲这个名字，起的是哪一个（[`Wins`]）。
 */
wins: AliasWins, };
type AliasStartupFile = { 
/**
 * 绝对路径。
 */
path: string, 
/**
 * 这份文件今天已经把生成文件 `source` 进去了吗（装过 ccm 别名块的人这一格就是 true）。
 */
sourced: boolean, 
/**
 * 这份文件在不在盘上。POSIX 只列在的（恒 true）；PowerShell 的 `$PROFILE` 常常要装的时候才建。
 */
exists: boolean, 
/**
 * 这份文件里别名块的现状（不在盘上 ⇒ 全空）。
 */
block: AliasBlockState, 
/**
 * 在盘上、可那台后端读不了它（非 UTF-8 · 太大 · 解到 home 外 · I/O）—— 后端原话；`None` = 读得了或不在。
 * 从前本机直读时这一形被吞成「没有别名块」。
 */
unreadable: string | null, 
/**
 * 加载它的那一代 PowerShell 的执行策略（现问；块装在这里它会不会跑）。POSIX 与人另指的那一份 ⇒ `None`。
 */
policy: PsExecPolicy | null, 
/**
 * 把别名块装进这一份会写几行（与装进去的那一块同源：[`block::render_block`]，方言按这份的扩展名；渲染不出 ⇒ 0）。
 */
blockLines: number, };
type AliasWins = "yours" | "list" | "unclear";
type AliasesReadReq = { 
/**
 * `posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒）。
 */
shell: ShellKind, rcPath?: string | null, };
type PsExecPolicy = { host: PsHost, 
/**
 * 生效那一档的原词（`Get-ExecutionPolicy`）；问不到 ⇒ `None`，原因词在 `error`。
 */
effective: string | null, 
/**
 * 按这一档，这一代会不会加载本地、未签名的 profile；说不清 ⇒ `None`。
 */
loads: boolean | null, 
/**
 * 组策略（`MachinePolicy` / `UserPolicy`）钉着 ⇒ 改当前用户那一档没用。
 */
groupPolicy: boolean, 
/**
 * 问不到的原因词（原因词闭集里的一个；PowerShell / 系统的原话只记一行日志，不随应答走）。
 */
error: string | null, };
type PsHost = "powershell" | "pwsh";
type ShellKind = "posix" | "powershell";
```

码：`bad_args` · `refused`

#### `profiles-read`

读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-read`

入参 `NoArgs` · 回包 `ProfilesBook`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type NoArgs = Record<symbol, never>;
type ProfileAccountPick = { "kind": "account", name: string, } | { "kind": "base" };
type ProfileAccountShape = { account: string, tmux: boolean, };
type ProfileCwdCase = { at: string, to: string, };
type ProfileForm = { name: string, from: string | null, account: ProfileAccountPick | null, tmux: ProfileTmuxPick | null, cwdIf: Array<ProfileCwdCase> | null, cwd: string | null, agent: string | null, 
/**
 * 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
 */
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, busRegister: boolean, busNote: string | null, };
type ProfileKind = "link" | "function";
type ProfileOwnItem = { key: string, slot: ProfileSlotId, vals: Array<string>, line: number, };
type ProfileProblem = { line: number | null, message: string, 
/**
 * 复制详情（有下层原话时才有：TOML 解析器那一句）；没有 ⇒ `null`。
 */
detail: string | null, };
type ProfileRow = { name: string, from: string | null, own: Array<ProfileOwnItem>, agent: Array<string>, usable: boolean, problem: ProfileProblem | null, kind: ProfileKind, functionWhy: string | null, 
/**
 * 写成函数时写在哪份文件、写成什么。
 */
functionLine: string | null, 
/**
 * 树里那一行（按表单同一套词写好）。
 */
said: string, form: ProfileForm, 
/**
 * 「账号那一形」（自己只写了号、可再加 tmux）⇒ 号与 tmux 按合并下来的算；其余 ⇒ `null`（账号页按它列各号的命令名）。
 */
accountShape: ProfileAccountShape | null, };
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "busRegister" | "busNote";
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesBook = { 
/**
 * 这台的家目录（界面拿它把路径写成 `~/…`）。
 */
home: string, 
/**
 * 配置文件的路径。
 */
path: string, 
/**
 * 配置文件在不在。
 */
exists: boolean, 
/**
 * 盘上那份的指纹（不在 ⇒ `null`），存的时候交回 `profiles-write`。
 */
fingerprint: string | null, 
/**
 * 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按看的那一台的时区写好）；没改过 ⇒ `null`。
 */
editedAt: string | null, 
/**
 * TOML 本身写坏 ⇒ 那一处（这时 `profiles` 为空、不能按条目改）；否则 `null`。
 */
fileProblem: ProfileProblem | null, profiles: Array<ProfileRow>, 
/**
 * 配置文件不在时首建那两条的预览（在 ⇒ 空）。
 */
seed: Array<ProfileRow>, 
/**
 * 旧别名清单一次性转进来之后那张说明（「知道了」之后 `null`）。
 */
migrated: ProfilesMigrated | null, 
/**
 * 链接住的目录（`~/.cc-monitor/bin`）。
 */
binDir: string, 
/**
 * 这台的账号表（具名号，按账号库的顺序；表单「账号」那一格的选项）。
 */
accounts: Array<string>, 
/**
 * 这台有没有 tmux（「在哪起」那一格选不选得了 tmux 看它）；探不出 ⇒ `null`。
 */
tmux: boolean | null, };
type ProfilesMigrated = { 
/**
 * 转进来几段。
 */
count: number, 
/**
 * 配置文件的路径。
 */
path: string, 
/**
 * 没能转的那几行（原样）。
 */
skipped: Array<string>, };
```

码：`bad_args` · `refused`

#### `profiles-resolve`

一段合下来的合并表与「等于」那一行（可按未存的表单算）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-resolve`

入参 `ProfilesResolveReq` · 回包 `ProfileResolved`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProfileAccountPick = { "kind": "account", name: string, } | { "kind": "base" };
type ProfileCwdCase = { at: string, to: string, };
type ProfileForm = { name: string, from: string | null, account: ProfileAccountPick | null, tmux: ProfileTmuxPick | null, cwdIf: Array<ProfileCwdCase> | null, cwd: string | null, agent: string | null, 
/**
 * 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
 */
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, busRegister: boolean, busNote: string | null, };
type ProfileMergeRow = { key: string, slot: ProfileSlotId, label: string, vals: Array<string>, said: string, from: string, overriddenBy: string | null, };
type ProfileResolved = { 
/**
 * 继承链（父 → 子）。
 */
chain: Array<string>, rows: Array<ProfileMergeRow>, 
/**
 * 这台后端算的「等于」那一行（同 `ccm @名 -- --ccm-print`）；算不出 ⇒ `null`。
 */
line: string | null, 
/**
 * 「等于」那一行的短形（设置页那一格照抄）；算不出 ⇒ `null`。
 */
lineShort: string | null, 
/**
 * 算不出那一行时 ccm 的原话。
 */
lineError: string | null, 
/**
 * 合不下来 ⇒ 那一句（同终端里敲这个名字）；否则 `null`。
 */
problem: string | null, };
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "busRegister" | "busNote";
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesResolveReq = { 
/**
 * 哪一段（带 `edit` 时是正在改的那一段原来的名字，新增写表单里的名字）。
 */
name: string, edit?: ProfileForm | null, 
/**
 * `~` 打头按这台家目录展开。
 */
at?: string | null, };
```

码：`bad_args` · `refused`

#### `profiles-impact`

这几处改动会让哪几段合下来变（改前改后）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-impact`

入参 `ProfilesImpactReq` · 回包 `ProfilesImpact`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProfileAccountPick = { "kind": "account", name: string, } | { "kind": "base" };
type ProfileAffected = { name: string, changes: Array<ProfileSlotChange>, problem: string | null, };
type ProfileChildren = "reparent" | "cascade";
type ProfileCwdCase = { at: string, to: string, };
type ProfileForm = { name: string, from: string | null, account: ProfileAccountPick | null, tmux: ProfileTmuxPick | null, cwdIf: Array<ProfileCwdCase> | null, cwd: string | null, agent: string | null, 
/**
 * 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
 */
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, busRegister: boolean, busNote: string | null, };
type ProfileOp = { "op": "set", was: string | null, form: ProfileForm, } | { "op": "remove", name: string, children?: ProfileChildren, } | { "op": "init", seed: boolean, } | { "op": "ackMigrated", };
type ProfileSlotChange = { slot: ProfileSlotId, label: string, before: string, after: string, };
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "busRegister" | "busNote";
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesImpact = { affected: Array<ProfileAffected>, };
type ProfilesImpactReq = { changes: Array<ProfileOp>, };
```

码：`bad_args` · `refused`

#### `profiles-bases`

「基于」下拉能选的几段（选了不成圈）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-bases`

入参 `ProfilesBasesReq` · 回包 `ProfilesBases`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProfileBase = { name: string, from: string | null, said: string, selectable: boolean, };
type ProfilesBases = { bases: Array<ProfileBase>, };
type ProfilesBasesReq = { name: string, };
```

码：`bad_args` · `refused`

#### `profiles-write`

按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-write`

入参 `ProfilesWriteReq` · 回包 `ProfilesWriteDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProfileAccountPick = { "kind": "account", name: string, } | { "kind": "base" };
type ProfileChildren = "reparent" | "cascade";
type ProfileCwdCase = { at: string, to: string, };
type ProfileForm = { name: string, from: string | null, account: ProfileAccountPick | null, tmux: ProfileTmuxPick | null, cwdIf: Array<ProfileCwdCase> | null, cwd: string | null, agent: string | null, 
/**
 * 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
 */
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, busRegister: boolean, busNote: string | null, };
type ProfileOp = { "op": "set", was: string | null, form: ProfileForm, } | { "op": "remove", name: string, children?: ProfileChildren, } | { "op": "init", seed: boolean, } | { "op": "ackMigrated", };
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesWriteDone = { wrote: boolean, fingerprint: string | null, reload: string | null, };
type ProfilesWriteReq = { changes: Array<ProfileOp>, fingerprint: string | null, };
```

码：`bad_args` · `refused` · `stale`

#### `aliases-block-render`

别名块预览。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-render`

入参 `BlockReq` · 回包 `BlockText`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BlockReq = { rcPath: string, };
type BlockText = { text: string, };
```

码：`bad_args` · `refused`

#### `aliases-block-install`

别名块装进人选的那份启动文件。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-install`

入参 `BlockReq` · 回包 `BlockDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BlockDone = Record<symbol, never>;
type BlockReq = { rcPath: string, };
```

码：`bad_args` · `refused`

#### `aliases-block-remove`

别名块卸掉。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --aliases-block-remove`

入参 `BlockReq` · 回包 `BlockDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BlockDone = Record<symbol, never>;
type BlockReq = { rcPath: string, };
```

码：`bad_args` · `refused`

### 4.7 cc-bus 总线

#### `bus-list`

谁在线 + 各自待读多少。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 13 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --bus-list`

入参 `NoArgs` · 回包 `BusList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusAgent = { 
/**
 * 总线身份。
 */
id: string, 
/**
 * tmux 地址。
 */
target: string, 
/**
 * 待读条数。
 */
unread: number, 
/**
 * 这个地址今天还在不在：`true` / `false` / `null`（问不到身份空间，**不是**「不在」）。
 */
live: boolean | null, 
/**
 * 那个会话绑的会话 id，没绑 ⇒ `null`。
 */
ccm_sid: string | null, };
type BusList = { agents: Array<BusAgent>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `not_installed` · `child_timed_out` · `failed`

#### `bus-broadcast`

给总线上在线的成员群发一条。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --bus-broadcast`

入参 `BusBroadcastReq` · 回包 `BusBroadcast`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusBroadcast = { sent: number, skipped_offline: number, 
/**
 * 问不到身份空间（全是 `null`），退回发给所有登记的。
 */
liveness_unknown: boolean, failed: Array<BusUndelivered>, };
type BusBroadcastReq = { 
/**
 * 正文（非空）。
 */
text: string, 
/**
 * 以谁的身份发，也用来「不发给自己」（可缺）。
 */
from?: string | null, };
type BusUndelivered = { id: string, 
/**
 * `bus-send` 那一套码。
 */
error: string, 
/**
 * 原话。
 */
detail: string, };
```

码：`bad_args` · `not_installed` · `child_timed_out` · `failed` · `bad_id`

#### `bus-kill`

收掉一个总线成员。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-kill`

入参 `BusKillReq` · 回包 `BusKilled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusKillReq = { 
/**
 * 总线身份。
 */
id: string, };
type BusKilled = { id: string, 
/**
 * 会话真的被杀了。
 */
killed: boolean, 
/**
 * 身份对不上：只摘掉那条陈旧登记，会话与收件箱都没动。
 */
stale_only: boolean, };
```

码：`bad_args` · `bad_id` · `not_installed` · `child_timed_out` · `failed`

#### `bus-send`

发一条消息。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --bus-send`

入参 `BusSendReq` · 回包 `BusSent`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusSendReq = { 
/**
 * 收件人身份。
 */
to: string, text: string, 
/**
 * 以谁的身份发（可缺；空白当没给）。
 */
from?: string | null, };
type BusSent = { to: string, 
/**
 * 投出去了（恒 `true`）。
 */
sent: boolean, 
/**
 * 在总线名单里吗。
 */
registered: boolean, 
/**
 * 那个会话今天活着吗，与 `bus-list` 同一套三态。
 */
live: boolean | null, 
/**
 * 实际以谁的身份发的（没给 ⇒ `null`）。
 */
from: string | null, };
```

码：`bad_args` · `bad_id` · `not_installed` · `rejected` · `child_timed_out` · `too_long` · `failed`

#### `bus-spawn`

派生一个协作 agent。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-spawn`

入参 `BusSpawnReq` · 回包 `BusSpawnedNow`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusSpawnReq = { 
/**
 * 哪一家（缺 / 空 ⇒ 默认那一家）。
 */
tool?: string | null, 
/**
 * 工作目录（非空）。
 */
dir: string, 
/**
 * 交给新会话的任务（可缺）。
 */
task?: string | null, 
/**
 * 用哪个号起。
 */
account?: string | null, 
/**
 * 显式不注入账号。
 */
base?: boolean | null, };
type BusSpawnedNow = { 
/**
 * 恒 `true`。
 */
spawned: boolean, 
/**
 * 新会话的总线身份；从回显里认不出 ⇒ `null`（起了，只是名字没认出来，**不是**没起来）。
 */
id: string | null, 
/**
 * `cc-spawn` 的原始回显，给人看。
 */
said: string, };
```

码：`bad_args` · `bad_id` · `not_installed` · `child_timed_out` · `failed`

#### `bus-state`

总线名单 ＋ spawn 台账一次回全。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --bus-state`

入参 `NoArgs` · 回包 `BusState`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusRosterRow = { id: string, 
/**
 * 登记的 pane 地址。
 */
target: string, 
/**
 * 登记时间，cc-bus 原样。
 */
registered_at: string, unread: number, 
/**
 * 三态，同 [`BusAgent::live`]。
 */
live: boolean | null, ccm_sid: string | null, };
type BusSpawned = { id: string, 
/**
 * 派生时的目录。
 */
dir: string, 
/**
 * 派生时间。
 */
spawned_at: string, 
/**
 * 余下全部，含 TAB。
 */
task: string, 
/**
 * 三态：`true` / `false` / `null` = 核不了。
 */
live: boolean | null, };
type BusState = { agents: Array<BusRosterRow>, spawned: Array<BusSpawned>, 
/**
 * 两张表里读不懂的行数。
 */
skipped: number, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `not_installed` · `child_timed_out` · `failed`

#### `bus-inbox`

只读看一个 agent 收件箱的尾巴。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-inbox`

入参 `BusInboxReq` · 回包 `BusInbox`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusInbox = { messages: Array<BusMessage>, 
/**
 * 读不懂的行数。
 */
skipped: number, 
/**
 * 回显超过 4 MiB ⇒ 保尾，`true`。
 */
truncated: boolean, };
type BusInboxReq = { 
/**
 * 交给 `cc-log` 之前先过形状判定，不过 ⇒ `bad_id`、一个进程都不起。
 */
id: string, 
/**
 * 1..=2000，缺 ⇒ 200。
 */
lines?: number, };
type BusMessage = { 
/**
 * 发件人。
 */
from: string, 
/**
 * 时间。
 */
ts: string, 
/**
 * 正文。
 */
text: string, 
/**
 * 消息类别（cc-bus 原样）。
 */
class: string, };
```

码：`bad_args` · `bad_id` · `not_installed` · `child_timed_out` · `failed`

### 4.8 终端与会话

#### `launch-local`

本机起会话那一行 `ccm …`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `LocalLaunchRequest` · 回包 `Rendered`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountAsk = { "kind": "follow" } | { "kind": "base" } | { "kind": "named", name: string, };
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type LocalAction = { "kind": "new" } | { "kind": "resume", sid: string, } | { "kind": "attach" };
type LocalLaunchRequest = { 
/**
 * 这个会话是哪一家（线上的 kind）。必填：没说就不知道该怎么起，不落默认那一家。
 */
agent: string, action: LocalAction, 
/**
 * 只用来核「新起」那一格的目录在不在；终端的工作目录由 monitor 开窗口时给。
 */
cwd?: string | null, 
/**
 * 缺席 = 不表态（继承环境；接回那一形）· 跟随 · 账号 0 · 用户点名（[`AccountAsk`]）。
 */
account?: AccountAsk | null, 
/**
 * 要建进 tmux 时的会话名（界面铸名口铸的；这里不铸 —— 撞名避让只有一个家）。
 */
tmuxName?: string | null, };
type Rendered = { 
/**
 * 那一行 `ccm …`。
 */
cmd: string, 
/**
 * 实际用的号（账号库里的那一形；别的 ⇒ `null`）。
 */
account: LaunchedAccount | null, };
```

码：`bad_args` · `refused` · `account_unavailable`

失败带 `data` 的码：`account_unavailable` ⇒ `AccountUnavailable`

#### `launch-render-cli`

远端起会话那一行 `ccm …`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --launch-render-cli`

入参 `CliRenderRequest` · 回包 `Rendered`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountAsk = { "kind": "follow" } | { "kind": "base" } | { "kind": "named", name: string, };
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type CliRenderRequest = { 
/**
 * 这个会话是哪一家（线上的 kind）。必填：没说就不知道该怎么起，不落默认那一家。
 */
agent: string, action: WireAction, container: WireContainer, cwd: string | null, account: AccountAsk, ccmSid: string | null, 
/**
 * 显式指定的模型（压过下面那张表）。
 */
model: string | null, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type Rendered = { 
/**
 * 那一行 `ccm …`。
 */
cmd: string, 
/**
 * 实际用的号（账号库里的那一形；别的 ⇒ `null`）。
 */
account: LaunchedAccount | null, };
type WireAction = { "kind": "new" } | { "kind": "resume", sid: string, } | { "kind": "attach", name: string, };
type WireContainer = { "kind": "none" } | { "kind": "tmux", name: string, send_into: boolean, };
```

码：`bad_args` · `refused` · `account_unavailable`

失败带 `data` 的码：`account_unavailable` ⇒ `AccountUnavailable`

#### `terminal-ssh`

给一台远端开终端要跑的那一串。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-ssh`

入参 `TerminalSshReq` · 回包 `SshCommand`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type Prefer = { host: string, port: number, };
type SshCommand = { 
/**
 * 这台电脑终端里跑的那一行（Windows：PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'`；别处：同一组参数的 POSIX shell 一行）。
 */
command: string, };
type TerminalSshReq = { 
/**
 * 那台的配置（可能还没保存）。
 */
machine: MachineConfig, 
/**
 * 已保存的那一份（指纹只从同一个 host 的那一份继承）。
 */
saved?: MachineConfig | null, 
/**
 * 跳板那一台的配置（`machine.jump` 指它时必给）。
 */
jump?: MachineConfig | null, 
/**
 * 上次赢的那条地址（仍在这台的地址里 ⇒ 用它）。
 */
prefer?: Prefer | null, 
/**
 * 要在那台跑的命令。
 */
command?: string, 
/**
 * 在那台的这个目录开登录 shell（串，或非 UTF-8 的 `{"b16": "<hex>"}`）。
 */
cwd?: string | { b16: string }, };
```

码：`bad_args` · `bad_jump` · `refused` · `no_ssh_client` · `unobservable`

#### `terminal-processes`

那台报来的终端连接是这台电脑上哪个进程开的。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `TerminalProcessesReq` · 回包 `Processes`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Link = { 
/**
 * 进程号。
 */
pid: number, 
/**
 * 进程名。
 */
name: string, 
/**
 * 启动时刻（系统原值；拿不到 ⇒ 0）。
 */
start: number, };
type Processes = { 
/**
 * 这台上开着那条连接的进程链（自下而上）。
 */
chain: Array<Link>, 
/**
 * 对不上时：`not-ssh` · `elsewhere`（不是这台开的）· `mismatch`（经跳板 / 端口转换）· `query-failed`。
 */
why?: "not-ssh" | "elsewhere" | "mismatch" | "query-failed", 
/**
 * `elsewhere` 时对面那个地址。
 */
addr?: string, };
type SshConnection = { 
/**
 * 对面地址。
 */
clientAddr: string, 
/**
 * 对面端口。
 */
clientPort: number, 
/**
 * 本机地址。
 */
serverAddr: string, 
/**
 * 本机端口。
 */
serverPort: number, };
type Terminal = { 
/**
 * 经 ssh 连过来的那条连接；`null` = 那个终端不是经 ssh 连的（这台机器自己的屏幕上，或别的方式）。
 * 键恒在：产这一格的 `session-terminals` 不是经 ssh 也写 `null`；交来的缺这一键 ⇒ 认不得的形（`bad_args`），不当「不是经 ssh」。
 */
ssh: SshConnection | null, 
/**
 * 最近一次动静（tmux 客户端的 `client_activity`，Unix 秒）；不在 tmux 里 ⇒ `null`。
 */
activity: number | null, 
/**
 * 本机那个终端窗口的标签（`LC_CCM_WINDOW`，形如 `<进程号>-<起始时刻>`）；没送过来 / 形状不对 ⇒ 不带。
 */
window?: string, };
type TerminalProcessesReq = { 
/**
 * 1–16 个，最近动静在前。
 */
terminals: Array<Terminal>, };
```

码：`bad_args`

#### `session-terminals`

此刻是哪个终端在显示这个会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `SidReq` · 回包 `Showing`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Showing = { 
/**
 * 此刻显示它的终端，最近动静在前，最多 16 个。
 */
terminals: Array<Terminal>, 
/**
 * 一个都没有时：`detached`（在 tmux 里、没终端连着）· `no-terminal`（不在 tmux 里、进程没有终端）· `unreadable`（读不了）。
 */
why?: "detached" | "no-terminal" | "unreadable", };
type SidReq = { 
/**
 * 会话 id。
 */
sid: string, };
type SshConnection = { 
/**
 * 对面地址。
 */
clientAddr: string, 
/**
 * 对面端口。
 */
clientPort: number, 
/**
 * 本机地址。
 */
serverAddr: string, 
/**
 * 本机端口。
 */
serverPort: number, };
type Terminal = { 
/**
 * 经 ssh 连过来的那条连接；`null` = 那个终端不是经 ssh 连的（这台机器自己的屏幕上，或别的方式）。
 * 键恒在：产这一格的 `session-terminals` 不是经 ssh 也写 `null`；交来的缺这一键 ⇒ 认不得的形（`bad_args`），不当「不是经 ssh」。
 */
ssh: SshConnection | null, 
/**
 * 最近一次动静（tmux 客户端的 `client_activity`，Unix 秒）；不在 tmux 里 ⇒ `null`。
 */
activity: number | null, 
/**
 * 本机那个终端窗口的标签（`LC_CCM_WINDOW`，形如 `<进程号>-<起始时刻>`）；没送过来 / 形状不对 ⇒ 不带。
 */
window?: string, };
```

码：`bad_args` · `no_such_session` · `failed` · `child_timed_out`

#### `terminals-list`

这台的终端名单（形状与宿主无关；这一版宿主是 tmux）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 13 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminals-list`

入参 `TerminalsListReq` · 回包 `TerminalsList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Attached = { 
/**
 * 客户端种类（`terminal-window`）。
 */
kind: "terminal-window", 
/**
 * 连上的时刻（秒）。
 */
since: number, 
/**
 * 最近动静（秒）。
 */
last_activity: number, };
type StartedBy = { 
/**
 * 会话上的 `@ccm_client`（没声明 ⇒ `null`）。
 */
client: string | null, 
/**
 * 这个调用方过不过身份门（与 `can.input` 是不是 `true` 同一个判定；结束还要看 `can.end`）。
 */
mine: boolean, };
type TermSession = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 哪一家（没标就缺）。
 */
agent?: string, };
type TermState = "running" | "idle" | "program_exited";
type TerminalCan = { 
/**
 * 能不能送字 / 送键。
 */
input: true | { no: "not_yours" | "not_managed", said: string }, 
/**
 * 能不能结束。
 */
end: true | { no: "not_yours" | "not_managed" | "other_windows", said: string }, };
type TerminalItem = { 
/**
 * 名单里那一行的不透明句柄（前端不拼、不解析；送字 / 抓屏时交回）。
 */
terminal: string, 
/**
 * 终端宿主（这一版是 `tmux`）。
 */
host: "tmux", 
/**
 * tmux 会话名。
 */
tmux_name: string, 
/**
 * 里面跑着会话、前台不是 shell 时是窗格标题；否则是前台程序名。
 */
title: string, 
/**
 * 前台程序名。
 */
program: string, 
/**
 * 当前目录。
 */
cwd: string, 
/**
 * 里面跑着会话（`@ccm_sid`）时才有。
 */
session?: TermSession, 
/**
 * 谁起的。
 */
started_by: StartedBy, 
/**
 * 此刻连着它的终端客户端；空 ＝ 后台。
 */
clients: Array<Attached>, 
/**
 * 输入方式：`shared`（tmux：各端都能打字）。
 */
input: "shared", 
/**
 * 此刻的样子。
 */
state: TermState, 
/**
 * 最近动静（秒）。
 */
last_activity: number, 
/**
 * 这个调用方能做什么（抓屏不过身份门、恒可用，不在这里）。
 */
can: TerminalCan, };
type TerminalsList = { 
/**
 * 终端名单（每行一个终端）。
 */
terminals: Array<TerminalItem>, 
/**
 * `false` ＝ 名单里有读不懂的行（画「部分」）。
 */
complete: boolean, };
type TerminalsListReq = { 
/**
 * 自报的前端（1–32 个 `[a-z0-9-]`）：决定每行的 `can` 与 `started_by.mine`。
 */
client?: string, };
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `terminal-preview`

抓一个终端的一屏（只抓一次，轮询归调用方）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminal-preview` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `TerminalPreviewReq` · 回包 `Preview`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Cursor = { 
/**
 * 列。
 */
x: number, 
/**
 * 行。
 */
y: number, 
/**
 * 看得见。
 */
visible: boolean, };
type Preview = { 
/**
 * 这一屏的指纹（16 位十六进制）：内容一变就变，送字时带回来。
 */
screen: string, 
/**
 * 列数。
 */
cols: number, 
/**
 * 行数。
 */
rows: number, 
/**
 * 光标。
 */
cursor: Cursor, 
/**
 * 自上而下的行（往回要的在最前）。
 */
lines: Array<ScreenLine>, 
/**
 * 实际往回给了几行。
 */
scrollback_lines: number, 
/**
 * 要的比上限多、截到了上限。
 */
capped: boolean, 
/**
 * 抓屏时刻（秒）。
 */
captured_at: number, 
/**
 * 抓屏时刻在看的那一台钟上的（请求的 `tz`）`HH:MM:SS`（界面照抄、不换算）。
 */
captured_at_text: string, };
type ScreenLine = { 
/**
 * 那一行的文字。
 */
text: string, 
/**
 * 着色段（默认样子不出段）；`color: false` ⇒ 不给。
 */
spans?: Array<Span>, };
type Span = { 
/**
 * 起（含）。
 */
from: number, 
/**
 * 止（不含）。
 */
to: number, 
/**
 * 前景：16 色名（`red` · `bright-red` …）或 `#rrggbb`。
 */
fg?: string, 
/**
 * 背景：同 `fg`。
 */
bg?: string, 
/**
 * 粗。
 */
bold?: true, 
/**
 * 暗。
 */
dim?: true, 
/**
 * 斜。
 */
italic?: true, 
/**
 * 下划线。
 */
underline?: true, 
/**
 * 反显。
 */
inverse?: true, };
type TerminalPreviewReq = { 
/**
 * 目标：名单里的句柄（与 `sid` 恰给一个）。
 */
terminal?: string, 
/**
 * 目标：会话 id（与 `terminal` 恰给一个）。
 */
sid?: string, 
/**
 * 要不要颜色；缺省 `true`。
 */
color?: boolean, 
/**
 * 往回多要几行；缺省 0、上限 2000（超了截到上限、回包 `capped: true`）。
 */
scrollback?: number, };
```

码：`bad_args` · `bad_target` · `not_known` · `ambiguous` · `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `terminal-input`

往一个终端送字或送键（过身份门）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminal-input` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `TerminalInputReq` · 回包 `Sent`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "elsewhere" | "not_resumed";
type Sent = { 
/**
 * 结局。
 */
result: SentResult, 
/**
 * `refused` 的原因。
 */
why?: Refused, 
/**
 * `refused` 时给人看的那一句（后端写好）。
 */
said?: string, 
/**
 * `screen_changed` 时带的新指纹。
 */
screen?: string, 
/**
 * `session-say` 那一下终端已经没了、按记下的号恢复了这条会话、那一句当第一句交了出去 ⇒ `true`（只在这一支出现）。
 */
resumed?: boolean, };
type SentResult = "delivered" | "unsure" | "refused";
type TerminalInputReq = { 
/**
 * 目标：名单里的不透明句柄（前端不拼、不解析）。
 */
terminal?: string, 
/**
 * 目标：会话 id（与 `terminal` 恰给一个）。
 */
sid?: string, 
/**
 * 送字：字面字，原样送、不解释成键名；多行按粘贴送（与 `key` 恰给一个）。
 */
text?: string, 
/**
 * `text` 之后补一个回车；缺省 `true`（只跟 `text`）。
 */
enter?: boolean, 
/**
 * 送键：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down`。
 */
key?: string, 
/**
 * 送之前看到的那一屏的指纹；画面已经变了 ⇒ 不送、回 `refused` ＋ `screen_changed`。
 */
seen_screen?: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维）。
 */
client?: string, };
```

码：`bad_args` · `bad_target` · `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `session-commands`

一条会话能用的斜杠命令与 Skill（内置 ＋ 本项目 ＋ 它用的号的配置目录）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-commands`

入参 `SessionCommandsReq` · 回包 `SessionCommands`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CommandScope = "builtin" | "project" | "user";
type SessionCommand = { 
/**
 * 命令名（不带 `/`；界面填进输入框的是 `/name `）。
 */
name: string, 
/**
 * 简介；文件里没写 ⇒ `null`。
 */
text: string | null, 
/**
 * 住哪。
 */
scope: CommandScope, 
/**
 * 同一格写给人看的字。
 */
scopeText: string, 
/**
 * 要跟参数（填进去停住等补）。
 */
needsArgs: boolean, 
/**
 * 发之前先确认。
 */
confirm: boolean, };
type SessionCommands = { 
/**
 * 内置的在前，再本项目、用户级（各按名字排）。
 */
commands: Array<SessionCommand>, 
/**
 * 本项目的在前，再用户级。
 */
skills: Array<SessionSkill>, 
/**
 * 读不出来的那几份（一句一份；不整条失败）。
 */
problems: Array<string>, };
type SessionCommandsReq = { 
/**
 * 会话 id。
 */
sid: string, };
type SessionSkill = { 
/**
 * 名字。
 */
name: string, 
/**
 * 简介；没写 ⇒ `null`。
 */
text: string | null, 
/**
 * 点它时发进会话的那一句（后端写好，经 `session-say` 发）。
 */
say: string, };
```

码：`bad_args` · `no_such_session`

#### `session-model`

会话开着时换模型（送那一家的那一行、从记录里读回换成了什么；在等人不送）。

收 `args` · 可撤 · CLI：`ccm -- --session-model` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SessionModelReq` · 回包 `ModelSwitched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ModeSwitchResult = "done" | "unsure" | "refused";
type ModelSwitched = { 
/**
 * 结局（同切权限档那一组）。
 */
result: ModeSwitchResult, 
/**
 * 读回的显示名（那一家写的原样，例如 `Opus 5.5 (1M context)`）；没读到 ⇒ `null`。
 */
model: string | null, 
/**
 * 读回时那一家顺带说的思考档（原样）；没说 ⇒ 不出。
 */
effort?: string, 
/**
 * `refused` 的原因（同 `terminal-input`）。
 */
why?: Refused, 
/**
 * 给人看的那一句（换成了 · 没变 · 结果未知 · 被拒）。
 */
said?: string, };
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "elsewhere" | "not_resumed";
type SessionModelReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 要换成的模型（原值；必须是 `session-new-facts` 那一家 `models` 里的一个）。
 */
model: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
 */
client?: string, };
```

码：`bad_args` · `unsupported` · `no_such_session` · `no_tmux` · `unobservable` · `child_timed_out`

#### `session-permission`

会话开着时切权限档（按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-permission` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SessionPermissionReq` · 回包 `ModeSwitched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ModeSwitchResult = "done" | "unsure" | "refused";
type ModeSwitched = { 
/**
 * 结局。
 */
result: ModeSwitchResult, 
/**
 * 最后读到的档（原值）；没读 · 认不出 ⇒ `null`。
 */
mode: string | null, 
/**
 * 同一档写给人看的字；那一家起会话不给这一档 · 没读到 ⇒ `null`。
 */
modeText: string | null, 
/**
 * `refused` 的原因（同 `terminal-input`）。
 */
why?: Refused, 
/**
 * `unsure` · `refused` 时给人看的那一句。
 */
said?: string, };
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "elsewhere" | "not_resumed";
type SessionPermissionReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 要切到的档（原值；必须是 `session-new-facts` 那一家 `permissionModes` 里的一个）。
 */
mode: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
 */
client?: string, };
```

码：`bad_args` · `unsupported` · `not_known` · `ambiguous` · `no_tmux` · `tmux_too_old` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `session-say`

往一条会话说一句（在等人 ⇒ 不送；不在等 ⇒ 照送字那一条送、补回车；终端没了 ⇒ 按记下的号恢复、那一句当第一句交）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-say` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SessionSayReq` · 回包 `Sent`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "elsewhere" | "not_resumed";
type Sent = { 
/**
 * 结局。
 */
result: SentResult, 
/**
 * `refused` 的原因。
 */
why?: Refused, 
/**
 * `refused` 时给人看的那一句（后端写好）。
 */
said?: string, 
/**
 * `screen_changed` 时带的新指纹。
 */
screen?: string, 
/**
 * `session-say` 那一下终端已经没了、按记下的号恢复了这条会话、那一句当第一句交了出去 ⇒ `true`（只在这一支出现）。
 */
resumed?: boolean, };
type SentResult = "delivered" | "unsure" | "refused";
type SessionSayReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 那一句（字面字，原样送；多行按粘贴送；后端补回车）。
 */
text: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
 */
client?: string, };
```

码：`bad_args` · `no_tmux` · `no_such_session` · `unobservable` · `child_timed_out`

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

入参 `TerminalNameMintReq` · 回包 `Minted`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Minted = { 
/**
 * 铸出来的终端名（这台避让过）。
 */
name: string, };
type TerminalNameMintReq = { 
/**
 * 工作目录 ⇒ `<项目名>-cc`。
 */
cwd?: string, 
/**
 * 分叉出来的那一条：源会话的终端名或工作目录 ⇒ `<…>-fork-cc`。
 */
forkOf?: string, };
```

码：`bad_args` · `child_timed_out`

#### `kill`

杀一个 tmux 会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 8 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --kill` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `KillReq` · 回包 `Killed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusFailed = { 
/**
 * 那个 id。
 */
id: string, 
/**
 * 原话。
 */
why: string, };
type BusNote = { 
/**
 * 注销掉了的 id。
 */
removed: Array<string>, 
/**
 * 没注销成的那几个（`why` 是原话）。
 */
failed: Array<BusFailed>, 
/**
 * 名册读不到（原话）⇒ 登记在上面的一个都没注销；读得到 ⇒ `null`。
 */
unread: string | null, 
/**
 * 给人看的那几行（没有 ⇒ `null`）。
 */
said: string | null, 
/**
 * 复制详情（没有 ⇒ 空串）。
 */
detail: string, };
type KillReq = { 
/**
 * 要杀的 tmux 会话名。
 */
name: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维）。
 */
client?: string, 
/**
 * 可带：只结束挂着这个会话（`@ccm_sid`）的窗格（1–128 个 `[A-Za-z0-9_-]`）。
 */
sid?: string, };
type Killed = { 
/**
 * 杀掉的 tmux 会话名。
 */
session: string, 
/**
 * 杀成了（恒 `true`；界面照它确认，不当成功之外的任何东西）。
 */
killed: true, 
/**
 * 顺手从 cc-bus 注销的结局。
 */
bus: BusNote, };
```

码：`bad_args` · `no_tmux` · `no_such_session` · `wrong_owner` · `too_many_windows` · `kill_failed` · `child_timed_out`

#### `sessions-stop`

停一批会话（逐个答，一个不成不挡下一个）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 392 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --sessions-stop`

入参 `SidsReq` · 回包 `Batched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type Batched = { results: Array<Outcome>, };
type BusFailed = { 
/**
 * 那个 id。
 */
id: string, 
/**
 * 原话。
 */
why: string, };
type BusNote = { 
/**
 * 注销掉了的 id。
 */
removed: Array<string>, 
/**
 * 没注销成的那几个（`why` 是原话）。
 */
failed: Array<BusFailed>, 
/**
 * 名册读不到（原话）⇒ 登记在上面的一个都没注销；读得到 ⇒ `null`。
 */
unread: string | null, 
/**
 * 给人看的那几行（没有 ⇒ `null`）。
 */
said: string | null, 
/**
 * 复制详情（没有 ⇒ 空串）。
 */
detail: string, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type Outcome = { 
/**
 * 会话 id（与入参同序）。
 */
sid: string, 
/**
 * `done` 做成了 · `skipped` 这一个不用做 / 做不了 · `failed` 做了没成。
 */
outcome: "done" | "skipped" | "failed", 
/**
 * 跳过 / 失败的原因码（做成了 ⇒ `null`）。
 */
why: string | null, 
/**
 * 那一个的原话。
 */
detail: string, 
/**
 * 停失败那一个的那一句（与 `kill` 被拒同一张表）；别的 ⇒ `null`。
 */
said: string | null, 
/**
 * 失败那一个的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
 */
copyDetail: string, 
/**
 * 落在哪个 tmux 会话上。
 */
session: string | null, 
/**
 * 停做成了那一个：同 `kill` 应答的 `bus`。
 */
bus: BusNote | null, 
/**
 * 开终端那一形要跑的那一行。
 */
cmd: string | null, 
/**
 * 起的那一条实际用的号（停 / 没起 ⇒ `null`）。
 */
account: LaunchedAccount | null, 
/**
 * 选不了号的那一项：带要的号与替代（别的 ⇒ `null`）。
 */
unavailable: AccountUnavailable | null, };
type SidsReq = { 
/**
 * 那几个会话（1–64 个，不重复）。
 */
sids: Array<string>, 
/**
 * 自报的前端（「哪个前端的会话」那一维，同 `kill`）。
 */
client?: string, };
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `sessions-where`

一批会话各在这台哪个终端里。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 8 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --sessions-where`

入参 `SidsReq` · 回包 `Whereabouts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SidsReq = { 
/**
 * 那几个会话（1–64 个，不重复）。
 */
sids: Array<string>, 
/**
 * 自报的前端（「哪个前端的会话」那一维，同 `kill`）。
 */
client?: string, };
type StandingWord = "running" | "ambiguous" | "idle" | "none" | "no_tmux";
type TerminalAt = { 
/**
 * 终端宿主。
 */
host: string, 
/**
 * 名单里那一行的句柄。
 */
terminal: string, };
type Whereabout = { 
/**
 * 会话 id（与入参同序）。
 */
sid: string, 
/**
 * `running` · `ambiguous` · `idle` · `none` · `no_tmux`。
 */
standing: StandingWord, 
/**
 * 带着它的 tmux 会话名。
 */
names: Array<string>, 
/**
 * 与 `names` 同序同数：名单里那一行的句柄。
 */
terminals: Array<TerminalAt>, };
type Whereabouts = { results: Array<Whereabout>, };
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `sessions-start`

起 / 接回一批会话（逐个答）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 392 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --sessions-start`

入参 `SessionsStartReq` · 回包 `Batched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountAsk = { "kind": "follow" } | { "kind": "base" } | { "kind": "named", name: string, };
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type Batched = { results: Array<Outcome>, };
type BusFailed = { 
/**
 * 那个 id。
 */
id: string, 
/**
 * 原话。
 */
why: string, };
type BusNote = { 
/**
 * 注销掉了的 id。
 */
removed: Array<string>, 
/**
 * 没注销成的那几个（`why` 是原话）。
 */
failed: Array<BusFailed>, 
/**
 * 名册读不到（原话）⇒ 登记在上面的一个都没注销；读得到 ⇒ `null`。
 */
unread: string | null, 
/**
 * 给人看的那几行（没有 ⇒ `null`）。
 */
said: string | null, 
/**
 * 复制详情（没有 ⇒ 空串）。
 */
detail: string, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type Outcome = { 
/**
 * 会话 id（与入参同序）。
 */
sid: string, 
/**
 * `done` 做成了 · `skipped` 这一个不用做 / 做不了 · `failed` 做了没成。
 */
outcome: "done" | "skipped" | "failed", 
/**
 * 跳过 / 失败的原因码（做成了 ⇒ `null`）。
 */
why: string | null, 
/**
 * 那一个的原话。
 */
detail: string, 
/**
 * 停失败那一个的那一句（与 `kill` 被拒同一张表）；别的 ⇒ `null`。
 */
said: string | null, 
/**
 * 失败那一个的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
 */
copyDetail: string, 
/**
 * 落在哪个 tmux 会话上。
 */
session: string | null, 
/**
 * 停做成了那一个：同 `kill` 应答的 `bus`。
 */
bus: BusNote | null, 
/**
 * 开终端那一形要跑的那一行。
 */
cmd: string | null, 
/**
 * 起的那一条实际用的号（停 / 没起 ⇒ `null`）。
 */
account: LaunchedAccount | null, 
/**
 * 选不了号的那一项：带要的号与替代（别的 ⇒ `null`）。
 */
unavailable: AccountUnavailable | null, };
type SessionsStartReq = { 
/**
 * `tmux`（在 tmux 里后台起）· `window`（只渲那一行交回，窗口由界面开）。
 */
mode: StartMode, 
/**
 * 这台是不是界面所在那台（开终端那一形按它选本机 / 远端那一行）。
 */
local: boolean, 
/**
 * 整批一家（线上的 kind）。启动命令 · 模型表这台自己读（`prefs.json`），不随请求交。
 */
agent: string, 
/**
 * 要起的会话（1–64 项，sid 不重复）。
 */
items: Array<StartItemReq>, 
/**
 * 自报的前端，同 `kill`。
 */
client?: string, };
type StartItemReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 那一项的工作目录。
 */
cwd: string, 
/**
 * 用哪个号：缺 ＝ 跟随。
 */
account?: AccountAsk, 
/**
 * 分叉出来的那一条 ⇒ 必铸新终端名。
 */
fresh_terminal?: boolean, 
/**
 * 源会话 sid（只许与 `fresh_terminal: true` 一起）：新名从源会话此刻所在终端的名字铸。
 */
fork_of?: string, };
type StartMode = "tmux" | "window";
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `session-new`

起一个新会话（全产品一个框、一个请求）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new`

入参 `NewRequest` · 回包 `SessionNew`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountAsk = { "kind": "follow" } | { "kind": "base" } | { "kind": "named", name: string, };
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type FirstSay = { 
/**
 * `delivered` · `unsure`（不知道送没送到，别重发）· `refused`。
 */
result: string, 
/**
 * `refused` 的原因（同 `terminal-input`）；别的 ⇒ `null`。
 */
why: string | null, 
/**
 * `refused` 时给人看的那一句（写好）；别的 ⇒ `null`。
 */
said: string | null, };
type ForkFrom = { 
/**
 * 源会话 id。
 */
sid: string, 
/**
 * 从哪条消息处分叉。
 */
uuid: string, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type NewRequest = { 
/**
 * 哪一家（线上的 kind）。必填。
 */
agent: string, 
/**
 * 工作目录（开头的 `~` 按这台的家目录读）。
 */
cwd: string, 
/**
 * 缺席 ＝ 跟随（分叉跟随源会话上次的号；新起的 ⇒ 这台的默认号）。
 */
account?: AccountAsk, 
/**
 * `tmux`（在这台 tmux 里后台起，关终端不断）· `window`（开一个新终端窗口直接跑）。
 */
place: Place, 
/**
 * 用户改过的终端名；缺席 ⇒ 这台铸（分叉不收这一格：名字从源会话的终端名铸）。
 */
tmuxName?: string, 
/**
 * 这一趟手填的启动命令（空 / 缺席 ⇒ 这台偏好里那一家的启动命令，没设过 ⇒ 那一家的默认启动器）。
 */
command?: string, 
/**
 * 分叉自哪一条（只在这一步写分支记录）。
 */
forkFrom?: ForkFrom, 
/**
 * 发请求的界面就在这台上（开窗那一形本机与远端渲法不同）。
 */
local: boolean, 
/**
 * 这一趟的票（界面每次点［新建］一张；期限到了再问一次带同一张 ⇒ 认出同一趟，不起第二个：[`super::session_new_ticket`]）。缺 ⇒ 不认。
 */
ticket?: string, 
/**
 * 轮换：缺 ＝ 用「新会话用」的那份策略（新会话本来就这样，不写）· `{strategy: id}` ＝ 起之前按定好的 sid 写成用那一份。
 */
rotation?: RotationPick, 
/**
 * 第一句（B4）：起好了之后由这台交进会话（宿主等它就绪 · 信任框过了再送）；只跟 `place: tmux`。缺 ⇒ 不交。
 */
prompt?: string, 
/**
 * 这一趟用的模型（压过 `models` 偏好表）；缺 ＝ 跟随。
 */
model?: string, 
/**
 * 起在哪一个权限档（那一家声明的档之一，`LaunchFace::permission`）；缺 ＝ 跟随那一家自己的默认。
 */
permissionMode?: string, };
type Place = "tmux" | "window";
type RotationPick = { 
/**
 * 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）；`null` ⇒ 这个会话不轮换（新会话用的那份不管它）。
 */
strategy: string | null, };
type SessionNew = { outcome: SessionNewOutcome, 
/**
 * tmux 会话名（`started` 那一形；开窗那一形 ⇒ `null`）。
 */
session: string | null, 
/**
 * 分叉出来的新会话 sid · 新起时这台先定好的 sid（那一家认先定 sid）；那一家不认 ⇒ `null`（报到之前说不出）。
 */
sid: string | null, 
/**
 * 开窗那一形要跑的那一行（`started` ⇒ `null`）。
 */
cmd: string | null, 
/**
 * 实际用的号（账号 0 / 不指定 ⇒ `null`）。
 */
account: LaunchedAccount | null, 
/**
 * 起的是哪一家。
 */
agent: string, 
/**
 * 起在哪个目录（`~` 已按这台的家目录展开：认报到的会话按它）。
 */
cwd: string, 
/**
 * 第一句交进去的结局；没给第一句 ⇒ `null`。
 */
prompt: FirstSay | null, };
type SessionNewField = "agent" | "command" | "cwd" | "account" | "place" | "tmuxName";
type SessionNewOutcome = "started" | "open";
type SessionNewRefusal = { 
/**
 * 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
 */
field: SessionNewField | null, 
/**
 * 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
 */
unavailable: AccountUnavailable | null, };
```

码：`bad_args` · `unknown_agent` · `bad_command` · `no_dir` · `account_unavailable` · `place_unavailable` · `bad_tmux_name` · `tmux_taken` · `unobservable` · `fork_failed` · `refused` · `start_failed` · `child_timed_out` · `launch_pending` · `no_strategy`

失败带 `data` 的码：`bad_args` ⇒ `SessionNewRefusal` · `unknown_agent` ⇒ `SessionNewRefusal` · `bad_command` ⇒ `SessionNewRefusal` · `no_dir` ⇒ `SessionNewRefusal` · `account_unavailable` ⇒ `SessionNewRefusal` · `place_unavailable` ⇒ `SessionNewRefusal` · `bad_tmux_name` ⇒ `SessionNewRefusal` · `tmux_taken` ⇒ `SessionNewRefusal` · `unobservable` ⇒ `SessionNewRefusal` · `fork_failed` ⇒ `SessionNewRefusal` · `refused` ⇒ `SessionNewRefusal` · `start_failed` ⇒ `SessionNewRefusal` · `child_timed_out` ⇒ `SessionNewRefusal` · `launch_pending` ⇒ `SessionNewRefusal` · `no_strategy` ⇒ `SessionNewRefusal`

#### `session-new-facts`

起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new-facts`

入参 `NewFactsReq` · 回包 `NewFacts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentFacts = { 
/**
 * 线上的 kind（交回 `session-new.agent` 的那一格）。
 */
kind: string, 
/**
 * 这一家写给人看的名字。
 */
text: string, 
/**
 * 起会话可选的模型（不选 ＝ 跟着那一家自己的默认）；这一家没声明 ⇒ 空。
 */
models: Array<NewSessionPick>, 
/**
 * 起会话可选的权限档（不给越过批准那一档）；这一家没有 ⇒ 空。
 */
permissionModes: Array<NewSessionPick>, };
type ForkFacts = { 
/**
 * 源会话是哪一家（说不出 ⇒ 空串）。
 */
agent: string, 
/**
 * 起分叉会话要的三格（同 `session-fork` 的 `launch`）。
 */
launch: ForkLaunch, 
/**
 * `at` 那一条在第几轮（没给 / 说不出 ⇒ `null`）。
 */
turn: number | null, 
/**
 * 那一轮那句在看的那一台钟上的（请求的 `tz`） `HH:MM`（说不出 ⇒ `null`）。
 */
startText: string | null, };
type ForkLaunch = { cwd: ForkSlot<string>, 
/**
 * 值 `null` ＝ 账号 0，串 ＝ 号名。
 */
account: ForkSlot<string | null>, terminal: ForkSlot<ForkTerminal>, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkSlot<T> = { "kind": "known", value: T, from: ForkWhence, } | { "kind": "unknown", why: ForkWhy, };
type ForkTerminal = { "host": "tmux", terminal: string, } | { "host": "none" };
type ForkWhence = "record" | "process" | "terminal_list";
type ForkWhy = "exited" | "no_cwd" | "live_no_account";
type NewFacts = { 
/**
 * 这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个）。
 */
recent: Array<RecentDir>, 
/**
 * 这台有没有 tmux（`false` ⇒ 只能开终端窗口）。
 */
tmux: boolean, 
/**
 * 这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序），各带名字与可选的模型 · 权限档。
 */
agents: Array<AgentFacts>, 
/**
 * 没给 `forkOf` ⇒ `null`。
 */
fork: ForkFacts | null, };
type NewFactsReq = { 
/**
 * 可缺：分叉源会话 sid。
 */
forkOf?: string, 
/**
 * 可缺（随 `forkOf`）：从哪条消息处分叉（框顶说第几轮、几点）。
 */
at?: string, };
type NewSessionPick = { 
/**
 * 原值（`session-new` 的 `model` / `permissionMode` 收的就是它）。
 */
value: string, 
/**
 * 写给人看的字。
 */
text: string, };
type RecentDir = { 
/**
 * 工作目录。
 */
cwd: string, 
/**
 * 那个目录最近一次会话的修改时刻（毫秒）。
 */
lastMs: number, 
/**
 * 同一刻按请求的 `tz` 写给人看的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年）。
 */
lastText: string, };
```

码：`bad_args` · `fork_failed`

#### `session-new-dir`

起新会话前核一个目录：在不在 · 会用哪个终端名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-new-dir`

入参 `SessionNewDirReq` · 回包 `DirChecked`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DirChecked = { 
/**
 * 这个目录在不在。
 */
exists: boolean, 
/**
 * 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
 */
tmuxName: string | null, };
type SessionNewDirReq = { 
/**
 * 工作目录（开头的 `~` 按这台的家目录读）。
 */
cwd: string, 
/**
 * 分叉出来的那一条：源会话 sid（铸名按它此刻所在的终端名派生 `<…>-fork-cc`）。
 */
forkOf?: string, };
```

码：`bad_args`

#### `session-restart`

换号重启。

收 `args` · 可撤 · 只在流上

入参 `RestartReq` · 回包 `Restarted`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AccountUnavailable = { 
/**
 * 要的那个号。
 */
requested: string, 
/**
 * 它是这条会话上次用的号（跟随），不是这一次点名的。
 */
pinned: boolean, 
/**
 * 这台的账号清单读得出来。
 */
listKnown: boolean, 
/**
 * 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
 */
alternative: string | null, };
type Arrival = "arrived" | "missed";
type Compact = "done" | "timed_out" | "skipped" | "unsupported" | "failed";
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type RestartRefused = { names: Array<string>, } | { pids: Array<number>, stopped: boolean, } | { why: string, } | { terminal: string, why: string | null, stopped: boolean, };
type RestartReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 工作目录。
 */
cwd: string, 
/**
 * 用户点名的号（只收名字，非空）。
 */
account: string, 
/**
 * 先请求压缩、等记录里出现压缩摘要再停。
 */
compact_first: boolean, 
/**
 * 等压缩的期限（≤ 3 600 000）。
 */
compact_within_ms: number, 
/**
 * 等新进程报出的期限（≤ 3 600 000）。
 */
arrive_within_ms: number, 
/**
 * 这台是不是界面所在那台（同 `sessions-start`）。
 */
local: boolean, 
/**
 * 哪一家（线上的 kind；同 `sessions-start` 的整批那一格）。启动命令 · 模型表这台自己读。
 */
agent: string, 
/**
 * 自报的前端，同 `kill`。
 */
client?: string, };
type Restarted = { 
/**
 * 先压缩那一步：`done` · `timed_out` · `skipped` · `unsupported` · `failed`。
 */
compact: Compact, 
/**
 * 新进程报没报出：`arrived` · `missed`。
 */
started: Arrival, 
/**
 * 所在的终端（会话名）。
 */
terminal: string, 
/**
 * 实际用的号（账号 0 / 不指定 ⇒ `null`）。
 */
account: LaunchedAccount | null, };
```

码：`bad_args` · `unobservable` · `account_unavailable` · `not_in_terminal` · `ambiguous` · `session_already_live` · `stop_failed` · `start_failed` · `child_timed_out`

失败带 `data` 的码：`account_unavailable` ⇒ `AccountUnavailable` · `ambiguous` ⇒ `RestartRefused` · `session_already_live` ⇒ `RestartRefused` · `stop_failed` ⇒ `RestartRefused` · `start_failed` ⇒ `RestartRefused`

#### `launch`

建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 8 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --launch` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `LaunchReq` · 回包 `Launched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LaunchReq = { 
/**
 * `create-or-attach`（建或接）· `send-into`（往已在的会话里键入）。
 */
mode: Mode, 
/**
 * tmux 会话名。
 */
name: string, 
/**
 * 要键入的那一行（`create-or-attach` 放行换行 · 制表）。
 */
payload: string, 
/**
 * 可选，仅 create-or-attach。
 */
cwd?: string, 
/**
 * 可选：create-or-attach 写成意图键 `@ccm_sid_expect`；send-into 键入挂着它的那个窗格。
 */
ccm_sid?: string, 
/**
 * 可选，仅 create-or-attach：注册表里的一家（空 ⇒ 默认那一家）。
 */
agent?: string, 
/**
 * 可选，仅 create-or-attach：1–4 位十进制字符串，与 `height` 同时给或都不给。
 */
width?: string, 
/**
 * 同 `width`。
 */
height?: string, 
/**
 * 可选：自报的前端（send-into 过「哪个前端的会话」那一维；create-or-attach 写成会话的 `@ccm_client`）。
 */
client?: string, };
type Launched = { 
/**
 * 落在哪个会话上。
 */
session: string, 
/**
 * 这一次新建了会话。
 */
created: boolean, 
/**
 * `send-keys` 退出 0：只有 `send-keys` 的退出码那么强（pane 在 copy-mode 时照样退 0、键被吃掉）。
 */
typed: boolean, };
type Mode = "create-or-attach" | "send-into";
```

码：`bad_args` · `no_tmux` · `no_such_session` · `wrong_owner` · `create_failed` · `typed_unconfirmed` · `child_timed_out`

### 4.9 机器

#### `ccm-print`

一条别名实际会执行什么。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `CcmPrintReq` · 回包 `CcmLine`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CcmLine = { line: string, };
type CcmPrintReq = { 
/**
 * 最多 64 个、每个最长 4096 字节。
 */
args: Array<string>, };
```

码：`bad_args` · `refused`

#### `ccm-probe`

这台的 `ccm` 会哪些。

不收 `args` · 可撤 · 只在流上

入参 `NoArgs` · 回包 `CcmCard`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CcmCard = { 
/**
 * CLI 契约版本（`CCM_VERSION`）。
 */
version: string, 
/**
 * 这台 `ccm` 认得的能力（与名片的 `capabilities=` 行同一份）。
 */
capabilities: Array<string>, 
/**
 * 认得的 agent 种类。
 */
agents: Array<string>, 
/**
 * 这一份的 `BUILD_ID`。
 */
build: string, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

#### `deploy-plan`

那台的后端要不要换、换成哪一格。

收 `args` · 可撤 · CLI：`ccm -- --deploy-plan`

入参 `DeployPlanReq` · 回包 `Planned`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Carried = { os: string, arch: string, id: string, };
type DeployPlanReq = { dial: MachineDial, carried: Array<Carried>, 
/**
 * 那台的名字（非空）。
 */
machine: string, };
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type MachineDial = { machine: MachineConfig, saved?: MachineConfig | null, jump?: MachineConfig | null, prefer?: Prefer | null, 
/**
 * 用法（存着不读：问那台的每一趟按自己的用法改，`capture` · `forward`）。
 */
use?: string | null, 
/**
 * ssh-agent 套接字的路径（界面进程此刻的 `SSH_AUTH_SOCK`；缺 ⇒ 用本进程自己的）。
 */
agent_sock?: string | null, };
type Planned = { 
/**
 * 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`）。
 */
os: string, arch: string, label: string, 
/**
 * 那一格这一版带着的字节自报的身份（对照物）。
 */
expected: string, 
/**
 * `skip`（已是这一版）· `deploy`（没装 / 0 字节 / 更旧）· `keep`（另一版、不比这一版旧 ⇒ 不动它）。
 */
action: "skip" | "deploy" | "keep", 
/**
 * 人读原因（`skip` 时空串）。
 */
why: string, 
/**
 * `keep` 时那台上那一份自报的身份，否则 `null`。
 */
theirs: string | null, 
/**
 * 落点目录里没人要的上传残件（家目录相对，排序）；列不出那个目录 ⇒ `[]`（下次连上再问）。
 */
leftovers: Array<string>, 
/**
 * 问 `uname` 那一趟拨号的 ack 原样（逐地址指纹 · 严格与否，拨号核心 `DialAck` 那一形）：monitor 按它固化指纹。
 */
ack: unknown, };
type Prefer = { host: string, port: number, };
```

码：`bad_args` · `io_failed` · `refused` · `unreachable` · `undecidable`

#### `resident-verdict`

远端常驻后端要不要换一次。

收 `args` · 可撤 · CLI：`ccm -- --resident-verdict`

入参 `VerdictReq` · 回包 `VerdictReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type VerdictAction = "replace" | "attach";
type VerdictReply = { 
/**
 * 换掉再接 · 接上它。
 */
action: VerdictAction, 
/**
 * 那台是不是比手上这一版旧（与 `replaced` 无关；版本提示那句话按它挑「会换掉」还是「不会换回去」）。
 */
older: boolean, };
type VerdictReq = { 
/**
 * monitor 手上这一版后端自报的身份（非空）。
 */
mine: string, 
/**
 * 那台 hello 报的 `build_id`（缺 ⇒ 空串）。
 */
theirs: string, 
/**
 * 这一趟是不是已经换过一次。
 */
replaced: boolean, };
```

码：`bad_args`

#### `place-verdict`

本机那一份放不放。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --place-verdict`

入参 `PlaceReq` · 回包 `PlaceAnswer`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlaceAnswer = { 
/**
 * `place`（放 / 换上去）· `keep`（盘上那一份不比这一份旧 ⇒ 不动、用它）。
 */
action: "place" | "keep", 
/**
 * 人读原因（`keep` 时点名两边各是哪一版）。
 */
why: string, };
type PlaceReq = { 
/**
 * 不在 ⇒ 没装 ⇒ 放；读不了 ⇒ `undecidable`，不当成没装。
 */
dest: string, machine: string, };
```

码：`bad_args` · `refused` · `undecidable`

#### `pubkey-push`

把本机公钥推进那台的 `authorized_keys`。

收 `args` · 可撤 · CLI：`ccm -- --pubkey-push`

入参 `PubkeyPushReq` · 回包 `PubkeyPushed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type PubkeyPushReq = { 
/**
 * 那台的配置（设置页表单里那台，可能还没保存）。
 */
machine: MachineConfig, 
/**
 * 已保存的那一份。
 */
saved?: MachineConfig | null, 
/**
 * 跳板那一台。
 */
jump?: MachineConfig | null, 
/**
 * 本机那份 `.pub` 的路径；缺 / 空 ⇒ 私钥同名 `.pub`（两样都没有 ⇒ `refused`，界面让人挑文件）。
 */
pubKeyPath?: string | null, };
type PubkeyPushed = { 
/**
 * `added`（新加的）· `already`（本就有整行相等的一行，没写）。
 */
outcome: PushOutcome, 
/**
 * 实际推的是哪一份（给人看）。
 */
pubPath: string, 
/**
 * 走了哪条。
 */
via: PushVia, };
type PushOutcome = "added" | "already";
type PushVia = "backend" | "exec";
```

码：`bad_args` · `bad_jump` · `refused` · `failed`

#### `authorized-keys-add`

把一行公钥并进这台的 `authorized_keys`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --authorized-keys-add`

入参 `KeyAddReq` · 回包 `KeyAdded`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type KeyAddReq = { key: string, };
type KeyAdded = { outcome: PushOutcome, };
type PushOutcome = "added" | "already";
```

码：`bad_args` · `refused` · `io_failed`

#### `exit-policy-read`

读「退出行为」那个值（值住后端所在那台）。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --exit-policy-read`

入参 `NoArgs` · 回包 `ExitPolicy`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExitPolicy = { 
/**
 * `absent`（没设过）· `chosen` · `unreadable`（读不出：也照答，不是失败）。
 */
state: "absent" | "chosen" | "unreadable", 
/**
 * 生效值：monitor 退出时结束本机常驻后端。
 */
killOnExit: boolean, 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
 */
detail: string | null, 
/**
 * 那份文件的路径（在 `~/.cc-monitor/` 下；家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 这个值意味着什么的一句话。
 */
said: string, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

#### `exit-policy-set`

写「退出行为」那个值，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --exit-policy-set`

入参 `ExitPolicySet` · 回包 `ExitPolicy`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExitPolicy = { 
/**
 * `absent`（没设过）· `chosen` · `unreadable`（读不出：也照答，不是失败）。
 */
state: "absent" | "chosen" | "unreadable", 
/**
 * 生效值：monitor 退出时结束本机常驻后端。
 */
killOnExit: boolean, 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
 */
detail: string | null, 
/**
 * 那份文件的路径（在 `~/.cc-monitor/` 下；家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 这个值意味着什么的一句话。
 */
said: string, };
type ExitPolicySet = { killOnExit: boolean, };
```

码：`bad_args` · `io_failed`

#### `prefs-read`

读这台的偏好。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --prefs-read`

入参 `NoArgs` · 回包 `PrefsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type NoArgs = Record<symbol, never>;
type Prefs = { 
/**
 * 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
 */
launchers: { [key in string]: string }, 
/**
 * 恢复会话默认进这台的 tmux。
 */
resumeInTmux: boolean, 
/**
 * 这台每个号默认用哪个模型（号 → 模型，原值）。
 */
models: { [key in string]: string }, 
/**
 * 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
 */
contextLimits: { [key in string]: number }, 
/**
 * 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
 * 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
 */
agentHomes: { [key in string]: string }, };
type PrefsReply = { 
/**
 * `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: "absent" | "ok" | "unreadable", 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情。
 */
detail: string | null, 
/**
 * 那份文件的路径（家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 照用的那一份。
 */
prefs: Prefs, };
```

码：`bad_args`

#### `prefs-set`

改这台的偏好，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --prefs-set`

入参 `PrefsSet` · 回包 `PrefsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Prefs = { 
/**
 * 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
 */
launchers: { [key in string]: string }, 
/**
 * 恢复会话默认进这台的 tmux。
 */
resumeInTmux: boolean, 
/**
 * 这台每个号默认用哪个模型（号 → 模型，原值）。
 */
models: { [key in string]: string }, 
/**
 * 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
 */
contextLimits: { [key in string]: number }, 
/**
 * 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
 * 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
 */
agentHomes: { [key in string]: string }, };
type PrefsReply = { 
/**
 * `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: "absent" | "ok" | "unreadable", 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情。
 */
detail: string | null, 
/**
 * 那份文件的路径（家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 照用的那一份。
 */
prefs: Prefs, };
type PrefsSet = { 
/**
 * 某一家的启动命令：`{kind: 命令}`，空串 ⇒ 删掉那一家（回到它的默认）。只动点到的那几家。
 */
launchers?: { [key in string]: string }, resumeInTmux?: boolean, 
/**
 * 某几个号的模型：`{号: 模型 | null}`，`null` ⇒ 删掉那一条。只动点到的那几个号。
 */
models?: { [key in string]: string | null }, 
/**
 * 整张上限表（整张换掉）。
 */
contextLimits?: { [key in string]: number }, 
/**
 * 某一家的数据目录：`{kind: 路径}`，空串 ⇒ 删掉那一家（回到它自己的解析）。只动点到的那几家；改了要重启后端才生效。
 */
agentHomes?: { [key in string]: string }, };
```

码：`bad_args` · `unreadable` · `io_failed`

#### `machines-read`

读这一端的机器表。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machines-read`

入参 `NoArgs` · 回包 `MachinesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Host = { 
/**
 * 这台的名字（origin）；空 ⇒ 用 `host`。
 */
label: string, 
/**
 * 首选地址。
 */
host: string, 
/**
 * 端口（缺 ＝ 22）。
 */
port: number, user: string, 
/**
 * 钥匙文件路径（这台上的；空 ＝ 照 ssh 的缺省找）。
 */
keyPath: string, 
/**
 * 固化的主机指纹（空 ＝ 没固化）。
 */
hostKeyFingerprint: string, 
/**
 * 指纹记下的那一天（`YYYY-MM-DD`，写口盖；空 ＝ 没记过）。
 */
hostKeyPinnedAt: string, 
/**
 * 备用地址（竞发）。
 */
addresses: Array<string>, 
/**
 * 跳板：另一台的名字（空 ＝ 直连）。
 */
jump: string, 
/**
 * 连不连这台。
 */
connect: boolean, };
type MachinesReply = { 
/**
 * `absent`（没有这份）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: "absent" | "ok" | "unreadable", 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情。
 */
detail: string | null, 
/**
 * 那份文件的路径（家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 照用的那一张。
 */
hosts: Array<Host>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

#### `machines-try`

这串补丁若现在落盘，机器表那一道过不过（盘上不动）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machines-try`

入参 `MachinesEdits` · 回包 `MachinesTried`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ElemKey = { fields: Array<string>, equals: string, };
type MachineEdit = { "op": "setin", where: Array<ElemKey>, field: string, value: unknown, ifEmpty?: boolean, } | { "op": "insertin", where: Array<ElemKey>, value: unknown, } | { "op": "removein", where: Array<ElemKey>, };
type MachineFault = { 
/**
 * `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名。
 */
code: MachineFaultCode, 
/**
 * 出问题的那台的名字（`label` 非空取它、否则 `host`）。
 */
name: string, 
/**
 * 那一格下面的那一句（添加机器框 · 机器卡同读）。
 */
said: string, };
type MachineFaultCode = "no_host" | "no_user" | "port" | "name_taken";
type MachinesEdits = { edits: Array<MachineEdit>, };
type MachinesTried = { 
/**
 * 这串补丁若现在落盘，机器表那一道没过的那一处；过 ⇒ `null`（补丁本身认不出那一台 · 不成形 ⇒ 也是 `null`，留给写口去拒）。
 */
fault: MachineFault | null, };
```

码：`bad_args`

#### `machines-set`

改机器表（按键认那一台），写完读回、推 `changed {machines}`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machines-set`

入参 `MachinesEdits` · 回包 `MachinesSet`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ElemKey = { fields: Array<string>, equals: string, };
type Gone = { 
/**
 * 认出了不止一台（不猜是哪台）。
 */
ambiguous: boolean, };
type Host = { 
/**
 * 这台的名字（origin）；空 ⇒ 用 `host`。
 */
label: string, 
/**
 * 首选地址。
 */
host: string, 
/**
 * 端口（缺 ＝ 22）。
 */
port: number, user: string, 
/**
 * 钥匙文件路径（这台上的；空 ＝ 照 ssh 的缺省找）。
 */
keyPath: string, 
/**
 * 固化的主机指纹（空 ＝ 没固化）。
 */
hostKeyFingerprint: string, 
/**
 * 指纹记下的那一天（`YYYY-MM-DD`，写口盖；空 ＝ 没记过）。
 */
hostKeyPinnedAt: string, 
/**
 * 备用地址（竞发）。
 */
addresses: Array<string>, 
/**
 * 跳板：另一台的名字（空 ＝ 直连）。
 */
jump: string, 
/**
 * 连不连这台。
 */
connect: boolean, };
type MachineEdit = { "op": "setin", where: Array<ElemKey>, field: string, value: unknown, ifEmpty?: boolean, } | { "op": "insertin", where: Array<ElemKey>, value: unknown, } | { "op": "removein", where: Array<ElemKey>, };
type MachineFault = { 
/**
 * `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名。
 */
code: MachineFaultCode, 
/**
 * 出问题的那台的名字（`label` 非空取它、否则 `host`）。
 */
name: string, 
/**
 * 那一格下面的那一句（添加机器框 · 机器卡同读）。
 */
said: string, };
type MachineFaultCode = "no_host" | "no_user" | "port" | "name_taken";
type MachinesEdits = { edits: Array<MachineEdit>, };
type MachinesReply = { 
/**
 * `absent`（没有这份）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: "absent" | "ok" | "unreadable", 
/**
 * `unreadable` 时的原因（那一句不带原话）。
 */
reason: string | null, 
/**
 * `unreadable` 时的复制详情。
 */
detail: string | null, 
/**
 * 那份文件的路径（家目录说不出 ⇒ `null`）。
 */
path: string | null, 
/**
 * 照用的那一张。
 */
hosts: Array<Host>, };
type MachinesSet = { machines: MachinesReply, pinned: Array<PinnedDay>, kept: number, };
type Origin = string;
type PinnedDay = { origin: Origin, day: string, };
```

码：`bad_args` · `unreadable` · `element_gone` · `element_exists` · `bad_machine` · `io_failed`

失败带 `data` 的码：`element_gone` ⇒ `Gone` · `bad_machine` ⇒ `MachineFault`

#### `relay-optin`

直接敲的 agent 也走中转。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `OptinReq` · 回包 `Optin`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Optin = { 
/**
 * `installed`（写着的就是现在那一条）· `stale`（是我们那一形，钥匙 / 端口 / 路由换了）· `absent` · `other`（别的上游）· `unreadable`。
 */
state: "installed" | "stale" | "absent" | "other" | "unreadable", 
/**
 * 那份文件为什么读不了（`unreadable` 才有，其余空串）。
 */
note: string, 
/**
 * 那一段为什么生成不了；已装 / 生成得了 ⇒ 空串。
 */
missing: string, 
/**
 * 读的是哪份文件（这台后端看到的路径）。
 */
source: string, 
/**
 * 要合并进那份文件的那一段（**带钥匙**）；已装或生成不了 ⇒ `null`。
 */
snippet: string | null, 
/**
 * 这台我们的中转此刻在不在听。
 */
listening: boolean, };
type OptinReq = { agent: string, };
```

码：`bad_args` · `failed`

#### `drift-report`

这台后端的漂移账。

不收 `args` · 可撤 · CLI：`ccm -- --drift-report`

入参 `NoArgs` · 回包 `DriftReport`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DriftEntry = { 
/**
 * 看不懂的那个值（记录 type / kind / status / token）。
 */
key: string, 
/**
 * 见过多少次（本进程内的计数，不落盘）。
 */
count: number, 
/**
 * **首见**的一条样例（截断）。后来的不再覆盖 —— 第一条最接近「它刚出现时长什么样」。
 */
first_sample: string | null, };
type DriftFaceReport = { 
/**
 * 那一家的面的名字（那一家自己的词）。
 */
face: string, 
/**
 * 这个面「看不懂时会发生什么」（那一家写好的那一句）。
 */
consequence: string, 
/**
 * 按 `count` 降序、同数按键名升序。
 */
entries: Array<DriftEntry>, 
/**
 * 该面是否已经溢出（键数触顶）。溢出后新键并进那一家的溢出键。
 */
overflowed: boolean, };
type DriftReport = { faces: Array<DriftFaceReport>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

#### `forward-start`

起一条本地端口转发。

收 `args` · 可撤 · 只在流上

入参 `ForwardStartReq` · 回包 `ForwardStarted`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ForwardStartReq = { 
/**
 * 那台的名字（可达表的键）。
 */
origin: Origin, 
/**
 * 绑本机 `127.0.0.1:localPort`。
 */
localPort: number, 
/**
 * 每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip。
 */
remoteHost: string, remotePort: number, 
/**
 * 那台的配置（可达表里**没有**那台、流没起来时按它组请求去拨，不拒）。
 */
machine?: MachineConfig | null, 
/**
 * 已保存的那一份。
 */
saved?: MachineConfig | null, 
/**
 * 跳板那一台。
 */
jump?: MachineConfig | null, };
type ForwardStarted = { 
/**
 * `fwd-<n>`，本进程内单调。
 */
id: string, };
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type Origin = string;
```

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

入参 `ForwardId` · 回包 `ForwardId`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ForwardId = { 
/**
 * 转发号（`fwd-<n>`）。
 */
id: string, };
```

码：`bad_args` · `not_found`

#### `forward-list`

列转发。

不收 `args` · 可撤 · 只在流上

入参 `NoArgs` · 回包 `Forwards`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ForwardRow = { id: string, 
/**
 * 那台的名字。
 */
origin: Origin, localPort: number, remoteHost: string, remotePort: number, 
/**
 * `running`（链路还在）· `error`（链路自己收工了，留在账上等人停）。
 */
state: "running" | "error", 
/**
 * 累计接进的连接数。
 */
connCount: number, };
type Forwards = { forwards: Array<ForwardRow>, };
type NoArgs = Record<symbol, never>;
type Origin = string;
```

码：`bad_args`

#### `remote-reach`

本机后端的可达表登记。

收 `args` · 可撤 · CLI：`ccm -- --remote-reach`

入参 `ReachReq` · 回包 `Reached`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineConfig = { 
/**
 * 首选地址。
 */
host: string, 
/**
 * 登录用户。
 */
user: string, 
/**
 * 这台的名字（空 ⇒ 用 `host`）。
 */
label?: string, 
/**
 * 口（缺 ⇒ 22）。
 */
port?: number, 
/**
 * 私钥路径（缺 / 空 ⇒ 走 ssh-agent）。
 */
keyPath?: string, 
/**
 * 记下的主机指纹（`SHA256:…`）。
 */
hostKeyFingerprint?: string, 
/**
 * 其余地址（`host` / `host:port` / `[v6]:port` / 裸 v6）。
 */
addresses: Array<string>, 
/**
 * 跳板那一台的名字（空 ⇒ 直连）。
 */
jump?: string, };
type MachineDial = { machine: MachineConfig, saved?: MachineConfig | null, jump?: MachineConfig | null, prefer?: Prefer | null, 
/**
 * 用法（存着不读：问那台的每一趟按自己的用法改，`capture` · `forward`）。
 */
use?: string | null, 
/**
 * ssh-agent 套接字的路径（界面进程此刻的 `SSH_AUTH_SOCK`；缺 ⇒ 用本进程自己的）。
 */
agent_sock?: string | null, };
type Origin = string;
type Prefer = { host: string, port: number, };
type ReachReq = { 
/**
 * 那台的名字（monitor 的 origin 名）。
 */
origin: Origin, 
/**
 * 那台的拨号（只有路径，没有私钥本体）。
 */
dial: MachineDial, };
type ReachRow = { 
/**
 * 那台的名字。
 */
origin: Origin, 
/**
 * 对面的资产目录 id。
 */
machine: string | null, };
type Reached = { origin: Origin, reach: Array<ReachRow>, };
```

码：`bad_args`

#### `backend-log`

这台后端的 stderr 诊断文件尾部。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --backend-log`

入参 `BackendLogReq` · 回包 `LogTail`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BackendLogReq = { 
/**
 * 只回尾部这么多字节；缺 ⇒ 封顶 256 KiB（给多了也照封顶）。
 */
maxBytes?: number, };
type LogTail = { 
/**
 * 本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null`。
 */
path: string | null, 
/**
 * 那份文件的总字节数。
 */
size: number, 
/**
 * 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行。
 */
text: string, 
/**
 * 前面还有没回的字节。
 */
truncated: boolean, };
```

码：`bad_args` · `failed`

#### `powershell-policy-set`

那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --powershell-policy-set`

入参 `PolicySetReq` · 回包 `PolicySet`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PolicySet = { 
/**
 * 形状同 `aliases-read` 候选里的 `policy`。
 */
policy: PsExecPolicy, 
/**
 * 设的那一下没成的原因词（组策略压着时 PowerShell 会报；它的原话只记后端日志）；`null` = 没报。
 */
setError: string | null, };
type PolicySetReq = { 
/**
 * `powershell`（5.1）· `pwsh`（7）。
 */
host: PsHost, };
type PsExecPolicy = { host: PsHost, 
/**
 * 生效那一档的原词（`Get-ExecutionPolicy`）；问不到 ⇒ `None`，原因词在 `error`。
 */
effective: string | null, 
/**
 * 按这一档，这一代会不会加载本地、未签名的 profile；说不清 ⇒ `None`。
 */
loads: boolean | null, 
/**
 * 组策略（`MachinePolicy` / `UserPolicy`）钉着 ⇒ 改当前用户那一档没用。
 */
groupPolicy: boolean, 
/**
 * 问不到的原因词（原因词闭集里的一个；PowerShell / 系统的原话只记一行日志，不随应答走）。
 */
error: string | null, };
type PsHost = "powershell" | "pwsh";
```

码：`bad_args` · `refused`

#### `ssh-config-import`

批量导入预览。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 28 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --ssh-config-import`

入参 `SshImportReq` · 回包 `SshImported`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type KnownHost = { host: string, user: string, port: number, };
type SshImportGroup = { label: string, host: string, port: number, user: string, keyPath: string | null, 
/**
 * 组内除 `host` 外的其余地址（去重；端口与组首不同则 `host:port`）。单机组为空。
 */
addresses: Array<string>, 
/**
 * 组内首个非空 proxyjump（别名）。
 */
jump: string | null, members: Array<SshImportMember>, 
/**
 * 这一组已在机器列表里（组里任一地址 ＋ 用户 ＋ 端口与列表里某台相同；列表由界面交来）。
 */
inList: boolean, };
type SshImportMember = { alias: string, host: string, port: number, proxyJump: string | null, };
type SshImportReq = { known: Array<KnownHost>, };
type SshImported = { groups: Array<SshImportGroup>, };
```

码：`bad_args`

### 4.10 计划

#### `plan-list`

这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-list`

入参 `PlanListReq` · 回包 `PlanList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanList = { pb: PlanPb, workspaces: Array<PlanListWorkspace>, };
type PlanListReq = { 
/**
 * 另要问的目录（这台的路径）。
 */
dirs?: Array<string> | null, 
/**
 * `true` ⇒ 认过的目录也重问 pb。
 */
fresh?: boolean | null, };
type PlanListSlice = { name: string, domain: string | null, current: boolean, progress: PlanProgress | null, needCount: number, error: string | null, stale: PlanStale | null, };
type PlanListWorkspace = { workspace: string, repo: string | null, auto: boolean, rev: string, stale: PlanStale | null, needCount: number, bySession: { [key in string]: PlanSessionBlock }, slices: Array<PlanListSlice>, };
type PlanPb = { state: "ok" | "missing" | "unsupported", said: string | null, version?: string, };
type PlanProgress = { done: number, open: number, dropped: number, };
type PlanSessionBlock = { slice: string, block: string, 
/**
 * 块根格（编号；点会话头那一枚标选中它）。
 */
cell: string | null, 
/**
 * 块根格的标题；顶块 ⇒ `null`。
 */
title: string | null, top: boolean, phase: string | null, at: string | null, atTitle: string | null, via: "session" | "subagent", };
type PlanStale = { said: string | null, raw: string | null, since: number, sinceText?: string, };
```

码：`bad_args` · `failed`

#### `plan-read`

一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-read`

入参 `PlanReadReq` · 回包 `PlanRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanArchived = { id: string, title: string | null, kind: string | null, replacedBy: string | null, };
type PlanBlock = { id: string, cells: Array<string>, dir: string | null, owner: PlanWho | null, phase: string | null, 
/**
 * 站在哪一格（编号）。
 */
at: string | null, row: boolean, };
type PlanCell = { id: string, title: string | null, kind: string | null, body: string | null, parent: string | null, children: Array<string>, edges: PlanEdges, pointedBy: PlanEdges, files: Array<PlanFile>, 
/**
 * pb 的原话（做完了 · 没做完 · 不做了）。
 */
status: string | null, 
/**
 * 状态的码；pb 给了别的字 ⇒ `null`。
 */
statusCode: "done" | "open" | "dropped" | null, 
/**
 * 没做完时的原因（pb 原话）。
 */
why: string | null, 
/**
 * 原因拆好的那一份；认不出 ⇒ `null`（界面照出 `why` 原话）。
 */
whyCode: PlanWhyCode | null, signs: Array<PlanSign>, owner: PlanWho | null, 
/**
 * 签它的（上一级）；pb 没给 ⇒ `null`。
 */
signer: PlanWho | null, refs: PlanRefs, hasView: boolean, 
/**
 * 退回过 ⇒ 那一次的状态；没有 ⇒ `null`。
 */
returned: PlanReturned | null, };
type PlanCheck = { unreadable: unknown[], red: Array<PlanRed>, undecidable: Array<PlanUndecidable>, };
type PlanEdgeWords = { to: string | null, with: string | null, after: string | null, replaces: string | null, };
type PlanEdges = { to: Array<string>, with: Array<string>, after: Array<string>, replaces: Array<string>, };
type PlanFile = { path: string, 
/**
 * pb 的原话（在 · 缺 · 空 · 坏）。
 */
state: string | null, 
/**
 * 对账的码；认不出 ⇒ `null`。
 */
stateCode: "ok" | "missing" | "empty" | "broken" | null, note: string | null, 
/**
 * 同一份文件还挂在哪几格（编号）。
 */
alsoBy: Array<string>, };
type PlanKind = { name: string | null, edgeWords: PlanEdgeWords, };
type PlanNeed = { key: string, kind: "top" | "red" | "ended" | "ask", block: string | null, cell: string | null, 
/**
 * 那一条牵着的会话（问人 · 停了的那一个）。
 */
sid: string | null, 
/**
 * 判据红那一条在 `check.red` 里的位置。
 */
red?: number, acked: boolean, };
type PlanPhase = { name: string | null, does: string | null, marks: Array<string>, };
type PlanProgress = { done: number, open: number, dropped: number, };
type PlanRead = { pb: string | null, workspace: string, repo: string | null, auto: boolean, slices: Array<PlanSlice>, bySession: { [key in string]: PlanSessionBlock }, 
/**
 * 这一份输出的摘要：变了才算计划变了。
 */
rev: string, readAt: number, readAtText?: string, stale: PlanStale | null, needCount: number, };
type PlanReadReq = { 
/**
 * 工作区根（`plan-list` 给的那个）。
 */
workspace: string, };
type PlanRed = { rule: string | null, what: string | null, block: string | null, fix: string | null, };
type PlanRef = { start: number, end: number, ids: Array<string>, };
type PlanRefs = { title: Array<PlanRef>, body: Array<PlanRef>, };
type PlanReturned = { 
/**
 * 退回那一刻（epoch ms）。
 */
at: number, atText?: string, to: PlanWho | null, state: "returned" | "unsure" | "landed", 
/**
 * 落地的那一种：底下多出新格 · 正文改了。
 */
by: "child" | "body" | null, child: PlanReturnedChild | null, };
type PlanReturnedChild = { id: string, title: string | null, };
type PlanSessionBlock = { slice: string, block: string, 
/**
 * 块根格（编号；点会话头那一枚标选中它）。
 */
cell: string | null, 
/**
 * 块根格的标题；顶块 ⇒ `null`。
 */
title: string | null, top: boolean, phase: string | null, at: string | null, atTitle: string | null, via: "session" | "subagent", };
type PlanSign = { 
/**
 * pb 的 ISO 时刻。
 */
at: string | null, 
/**
 * 那一刻写成给人看的字（按看的那一台的时区）；读不出时刻 ⇒ 没有。
 */
atText?: string, 
/**
 * 那一刻的毫秒（自 1970，排序用）；读不出时刻 ⇒ `null`。
 */
atMs: number | null, by: PlanWho | null, reason: string | null, refs: Array<PlanRef>, };
type PlanSlice = { name: string, domain: string | null, 
/**
 * 这一片的仓库相对工作区根在哪（pb 登记的；旧 pb 没给 ⇒ 片名）。
 */
path: string, current: boolean, 
/**
 * 这一刻读不成的那一句（pb 原样）。
 */
error: string | null, stale: PlanStale | null, bare: boolean, kinds: Array<PlanKind>, phases: Array<PlanPhase>, done: boolean, top: Array<string>, progress: PlanProgress | null, blocks: Array<PlanBlock>, check: PlanCheck, cells: Array<PlanCell>, archived: Array<PlanArchived>, needs: Array<PlanNeed>, 
/**
 * 这一片要你看的数（没认可的，不含问人）。
 */
needCount: number, 
/**
 * 顶块（`project`）退回过 ⇒ 那一次此刻的状态；没退回过 ⇒ `null`（缺 ⇒ 同）。
 */
returned: PlanReturned | null, };
type PlanStale = { said: string | null, raw: string | null, since: number, sinceText?: string, };
type PlanUndecidable = { rule: string | null, why: string | null, };
type PlanWho = { id: string, kind: "session" | "subagent" | "unknown", 
/**
 * 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。
 */
sid: string | null, alive: boolean, activity: "working" | "needs_you" | "idle" | null, 
/**
 * 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
 */
needs: string | null, };
type PlanWhyCode = { "kind": "nosign" } | { "kind": "inside", done: number, of: number, } | { "kind": "upper" };
```

码：`bad_args` · `failed` · `no_pb` · `not_workspace` · `pb_unsupported`

#### `plan-cell-view`

一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-cell-view`

入参 `PlanCellViewReq` · 回包 `PlanCellView`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanCellView = { view: string, };
type PlanCellViewReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * 片名。
 */
slice: string, 
/**
 * 格的编号。
 */
id: string, };
```

码：`bad_args` · `no_view`

#### `plan-command`

以人的身份代敲 pb 的用户命令：`continue` · `pause`（开关自动接着做，管整个工作区）· `view`（pb 画整张图写进系统临时目录，回页面路径）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-command`

入参 `PlanCommandReq` · 回包 `PlanCmdReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanCmdReply = { rc: number, said: string | null, path: string | null, };
type PlanCommandReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * `continue` · `pause` · `view`。
 */
cmd: "continue" | "pause" | "view", };
```

码：`bad_args` · `failed` · `no_pb` · `not_workspace` · `pb_unsupported` · `refused`

#### `plan-files`

文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-files`

入参 `PlanFilesReq` · 回包 `PlanFiles`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanFileDup = { id: string, title: string | null, };
type PlanFileOwner = { 
/**
 * 文件名（目录里的那一项）。
 */
name: string, 
/**
 * 作主的那一格（排期上先声明它的那一格）。
 */
id: string, title: string | null, statusCode: string | null, status: string | null, 
/**
 * 那一格住的那一块（块根格的标题）；住顶块 ⇒ `null`。
 */
block: string | null, 
/**
 * 作数那一条签收的时刻（成了字）；没签过 ⇒ `null`。
 */
signAtText: string | null, 
/**
 * 对账：`ok` · `missing` · `empty` · `broken`。
 */
fileState: string | null, fileNote: string | null, 
/**
 * 也声明它的那几格（pb 判据红「两个节点声明同一个文件」）。
 */
dup: Array<PlanFileDup>, };
type PlanFiles = { workspace: string | null, slice: string | null, 
/**
 * 那一片此刻读不成的那一句（条目空）。
 */
unreadable: string | null, entries: Array<PlanFileOwner>, 
/**
 * 无主的那几份（要 pb 给 `unowned`；没给 ⇒ `null`）。
 */
unowned: Array<string> | null, };
type PlanFilesReq = { 
/**
 * 那台机器上的绝对路径。
 */
dir: string, };
```

码：`bad_args` · `failed`

#### `plan-ack`

认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `changed {plan}` 带新的数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-ack`

入参 `PlanAckReq` · 回包 `PlanAckReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanAckReply = { acked: boolean, key: string, 
/**
 * 这个工作区此刻要人过目的数。
 */
needCount: number, };
type PlanAckReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * 片名。
 */
slice: string, 
/**
 * 那一条的键（`plan-read` 里 `needs[].key`）。
 */
key: string, };
```

码：`bad_args` · `io_failed` · `no_such_need` · `not_ackable` · `not_read` · `review_unreadable`

#### `plan-unack`

撤掉一条认可（没有也不算错）；推一帧 `changed {plan}` 带新的数。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-unack`

入参 `PlanAckReq` · 回包 `PlanAckReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanAckReply = { acked: boolean, key: string, 
/**
 * 这个工作区此刻要人过目的数。
 */
needCount: number, };
type PlanAckReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * 片名。
 */
slice: string, 
/**
 * 那一条的键（`plan-read` 里 `needs[].key`）。
 */
key: string, };
```

码：`bad_args` · `io_failed` · `not_read` · `review_unreadable`

#### `plan-return`

把人的话送给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，在等你 ⇒ 拒，已结束 / 认不出 ⇒ 只给复制，能送走 `terminal-input`；送到了记一条已退回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-return` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `PlanReturnReq` · 回包 `PlanReturnReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanReturnReply = { line: string, to: PlanWho | null, 
/**
 * `delivered` · `unsure`（送达未知，别重发）· `refused` · `copy`（送不了，只给这一行去复制）。
 */
result: "delivered" | "unsure" | "refused" | "copy", why: string | null, said: string | null, screen: string | null, };
type PlanReturnReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * 片名。
 */
slice: string, 
/**
 * 格的编号；顶块 ⇒ `project`（标题是片名，送给顶块接手）。
 */
id: string, 
/**
 * 人的原话（换行并成空格）。
 */
text: string, 
/**
 * `owner`（在长它的，缺省）· `signer`（签它的，pb 给了才有）。
 */
to?: "owner" | "signer", 
/**
 * 自报的前端（过 `terminal-input` 那一道身份门）。
 */
client?: string | null, 
/**
 * 送之前看到的那一屏的指纹（同 `terminal-input`）。
 */
seen_screen?: string | null, };
type PlanWho = { id: string, kind: "session" | "subagent" | "unknown", 
/**
 * 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。
 */
sid: string | null, alive: boolean, activity: "working" | "needs_you" | "idle" | null, 
/**
 * 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
 */
needs: string | null, };
```

码：`bad_args` · `bad_target` · `capture_failed` · `child_timed_out` · `failed` · `io_failed` · `no_pb` · `no_server` · `no_such_cell` · `no_such_session` · `no_target` · `no_tmux` · `not_workspace` · `pb_unsupported` · `review_unreadable` · `unobservable`

## 5. CLI 子命令

一次性调用：`ccm -- --子命令 …`（打头的 `--` 之后才归后端）；收 `args` 的那一段 JSON 从 stdin 读，或经 `--args-b64 <base64>` 走 argv（二选一，上限与码见 `IPC-PROTOCOL.md` §8），回一行 JSON 到 stdout。
「＝ 帧命令」的那几条与流上同名命令是同一个处理器。

| 子命令 | 说明 |
|---|---|
| `--accounts-add` | ＝ 帧命令 `accounts-add`：新建一个号 |
| `--accounts-init` | ＝ 帧命令 `accounts-init`：建账号库 |
| `--accounts-login-cmd` | ＝ 帧命令 `accounts-login-cmd`：在终端里登录一个号的那一行 |
| `--accounts-mcp-pick` | ＝ 帧命令 `accounts-mcp-pick`：两边都改了的那一条用哪一版 |
| `--accounts-mcp-read` | ＝ 帧命令 `accounts-mcp-read`：这台各账号共用的用户级 MCP 此刻的样子 |
| `--accounts-mcp-remove` | ＝ 帧命令 `accounts-mcp-remove`：从各账号共用的用户级 MCP 里删一条 |
| `--accounts-mcp-sync` | ＝ 帧命令 `accounts-mcp-sync`：停 / 开各账号之间同步用户级 MCP（定义与登录同一个开关） |
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
| `--answer-hook` `<agent>` | 会话级答题 hook 的客户端（那一家的 hook 设置里直接叫它，不经 shell）：读 hook 的 stdin，经本机常驻后端 `needs-ask` 挂上、等人答；拿到答案 ⇒ stdout 打那一家要的那一整份；常驻不在 · 断开 · 不是要人答的那一种 · 终端那边先答了 ⇒ 一个字节不打。恒退出 0 |
| `--apikey-key-set` | ＝ 帧命令 `apikey-key-set`：给一个账号写 key，写完读回 |
| `--assets-catalog` | ＝ 帧命令 `assets-catalog`：这台现扫一次、记下，回整份资产目录 ＋「这台缺什么」 |
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
| `--chores-mark` | ＝ 帧命令 `chores-mark`：记下「待办」里的一个选择 |
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
| `--first-run` | ＝ 帧命令 `first-run`：首次运行「开始用」三步各自打没打勾 |
| `--footprint-report` | ＝ 帧命令 `footprint-report`：「足迹」由这台后端出整份成品 |
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
| `--history-user-inputs` | ＝ 帧命令 `history-user-inputs`：「人说过的话」清单 |
| `--hooks-diag` | ＝ 帧命令 `hooks-diag`：cc-bus 钩子诊断 |
| `--kill` | ＝ 帧命令 `kill`：杀一个 tmux 会话 |
| `--last-seen-read` | ＝ 帧命令 `last-seen-read`：读离线那台的上次值 |
| `--last-seen-write` | ＝ 帧命令 `last-seen-write`：记下一台这一次读成的那一份（或清掉那台） |
| `--launch` | ＝ 帧命令 `launch`：建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面） |
| `--launch-render-cli` | ＝ 帧命令 `launch-render-cli`：远端起会话那一行 `ccm …` |
| `--machine-interrupts` | ＝ 帧命令 `machine-interrupts`：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么 |
| `--machines-read` | ＝ 帧命令 `machines-read`：读这一端的机器表 |
| `--machines-set` | ＝ 帧命令 `machines-set`：改机器表（按键认那一台），写完读回、推 `changed {machines}` |
| `--machines-try` | ＝ 帧命令 `machines-try`：这串补丁若现在落盘，机器表那一道过不过（盘上不动） |
| `--manual-list` | ＝ 帧命令 `manual-list`：会话外的需手动清单：需登录的号 · 需登录的 MCP |
| `--mcp-read` | ＝ 帧命令 `mcp-read`：这台机器的 MCP 列表成品 |
| `--mcp-sync-apply` | ＝ 帧命令 `mcp-sync-apply`：在要被写的那一台把那一条写进去 |
| `--mcp-sync-preview` | ＝ 帧命令 `mcp-sync-preview`：在要被写的那一台看装上之后那一条 |
| `--mcp-sync-source` | ＝ 帧命令 `mcp-sync-source`：装到别的机器时来源那一条 |
| `--ping` | ＝ 帧命令 `ping`：问活：零载荷，回 `ok` |
| `--place-verdict` | ＝ 帧命令 `place-verdict`：本机那一份放不放 |
| `--plan-ack` | ＝ 帧命令 `plan-ack`：认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `changed {plan}` 带新的数 |
| `--plan-cell-view` | ＝ 帧命令 `plan-cell-view`：一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样） |
| `--plan-command` | ＝ 帧命令 `plan-command`：以人的身份代敲 pb 的用户命令：`continue` · `pause`（开关自动接着做，管整个工作区）· `view`（pb 画整张图写进系统临时目录，回页面路径） |
| `--plan-files` | ＝ 帧命令 `plan-files`：文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb） |
| `--plan-list` | ＝ 帧命令 `plan-list`：这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区） |
| `--plan-read` | ＝ 帧命令 `plan-read`：一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view） |
| `--plan-return` | ＝ 帧命令 `plan-return`：把人的话送给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，在等你 ⇒ 拒，已结束 / 认不出 ⇒ 只给复制，能送走 `terminal-input`；送到了记一条已退回 |
| `--plan-unack` | ＝ 帧命令 `plan-unack`：撤掉一条认可（没有也不算错）；推一帧 `changed {plan}` 带新的数 |
| `--powershell-policy-set` | ＝ 帧命令 `powershell-policy-set`：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned` |
| `--prefs-read` | ＝ 帧命令 `prefs-read`：读这台的偏好 |
| `--prefs-set` | ＝ 帧命令 `prefs-set`：改这台的偏好，写完读回 |
| `--profiles-bases` | ＝ 帧命令 `profiles-bases`：「基于」下拉能选的几段（选了不成圈） |
| `--profiles-impact` | ＝ 帧命令 `profiles-impact`：这几处改动会让哪几段合下来变（改前改后） |
| `--profiles-read` | ＝ 帧命令 `profiles-read`：读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填 |
| `--profiles-resolve` | ＝ 帧命令 `profiles-resolve`：一段合下来的合并表与「等于」那一行（可按未存的表单算） |
| `--profiles-write` | ＝ 帧命令 `profiles-write`：按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数 |
| `--pubkey-push` | ＝ 帧命令 `pubkey-push`：把本机公钥推进那台的 `authorized_keys` |
| `--quota-probe` | ＝ 帧命令 `quota-probe`：用某个号查一次额度 |
| `--quota-read` | ＝ 帧命令 `quota-read`：这台的额度账 |
| `--remote-reach` | ＝ 帧命令 `remote-reach`：本机后端的可达表登记 |
| `--resident-attach` | 小中继：连这台家里常驻后端的套接字，stdin → 套接字、套接字 → stdout 原样对拷，任一边断就退；断开 ⇒ stdout 一行 `{attach:"refused", reason: absent\|unreachable}`、退出 2 |
| `--resident-ensure` `[--replace]` | 确保这台的常驻后端在听：已在 ⇒ `{pid}`（在听那一位，读不到记录 ⇒ `null`）；没在 ⇒ 起一个脱离的自己、回 `{pid}`；`--replace` 先停掉在听那一位再起 |
| `--resident-stop` `[--grace <秒>]` | 停这台的常驻后端：核身份 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful\|killed\|not_running, pid}` |
| `--resident-verdict` | ＝ 帧命令 `resident-verdict`：远端常驻后端要不要换一次 |
| `--resolve` | ＝ 帧命令 `resolve`：按 `ResumeSpec` 推出恢复命令 `CommandPlan` |
| `--rotation-day-paint` | ＝ 帧命令 `rotation-day-paint`：在 24 小时条上刷一段 |
| `--rotation-facts` | ＝ 帧命令 `rotation-facts`：每个号此刻的事实 |
| `--rotation-for-new` | ＝ 帧命令 `rotation-for-new`：新会话用哪份策略 |
| `--rotation-revert` | ＝ 帧命令 `rotation-revert`：丢掉会话里没存的设置 |
| `--rotation-save` | ＝ 帧命令 `rotation-save`：存设置 |
| `--rotation-settings` | ＝ 帧命令 `rotation-settings`：改一份设置 |
| `--rotation-strategies` | ＝ 帧命令 `rotation-strategies`：策略目录 |
| `--rotation-strategy-add` | ＝ 帧命令 `rotation-strategy-add`：加一份策略 |
| `--rotation-strategy-copy` | ＝ 帧命令 `rotation-strategy-copy`：复制一份策略 |
| `--rotation-strategy-delete` | ＝ 帧命令 `rotation-strategy-delete`：删一份策略 |
| `--rotation-strategy-rename` | ＝ 帧命令 `rotation-strategy-rename`：给策略改名 |
| `--rotation-strategy-tz` | ＝ 帧命令 `rotation-strategy-tz`：给策略设时区 |
| `--rotation-temp` | ＝ 帧命令 `rotation-temp`：排接下来先用谁 |
| `--rotation-try` | ＝ 帧命令 `rotation-try`：试跑一份策略 |
| `--rotation-use` | ＝ 帧命令 `rotation-use`：会话换一份策略 |
| `--rotation-use-now` | ＝ 帧命令 `rotation-use-now`：现在用这个号 |
| `--rotation-view` | ＝ 帧命令 `rotation-view`：一个会话或一份策略此刻的轮换 |
| `--session-commands` | ＝ 帧命令 `session-commands`：一条会话能用的斜杠命令与 Skill（内置 ＋ 本项目 ＋ 它用的号的配置目录） |
| `--session-fork` | ＝ 帧命令 `session-fork`：从某条消息处分叉出一个新会话 |
| `--session-model` | ＝ 帧命令 `session-model`：会话开着时换模型（送那一家的那一行、从记录里读回换成了什么；在等人不送） |
| `--session-new` | ＝ 帧命令 `session-new`：起一个新会话（全产品一个框、一个请求） |
| `--session-new-dir` | ＝ 帧命令 `session-new-dir`：起新会话前核一个目录：在不在 · 会用哪个终端名 |
| `--session-new-facts` | ＝ 帧命令 `session-new-facts`：起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法 |
| `--session-permission` | ＝ 帧命令 `session-permission`：会话开着时切权限档（按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按） |
| `--session-say` | ＝ 帧命令 `session-say`：往一条会话说一句（在等人 ⇒ 不送；不在等 ⇒ 照送字那一条送、补回车；终端没了 ⇒ 按记下的号恢复、那一句当第一句交） |
| `--sessions-needs` | ＝ 帧命令 `sessions-needs`：这台上需手动的会话清单 |
| `--sessions-start` | ＝ 帧命令 `sessions-start`：起 / 接回一批会话（逐个答） |
| `--sessions-stop` | ＝ 帧命令 `sessions-stop`：停一批会话（逐个答，一个不成不挡下一个） |
| `--sessions-where` | ＝ 帧命令 `sessions-where`：一批会话各在这台哪个终端里 |
| `--skill-install-apply` | ＝ 帧命令 `skill-install-apply`：在要被写的那一台把勾的那几个 skill 文件写进去 |
| `--skill-install-plan` | ＝ 帧命令 `skill-install-plan`：在要被写的那一台判 skill 装不装得过来 |
| `--skill-install-record` | ＝ 帧命令 `skill-install-record`：skill 装记录的写口 |
| `--skill-read` | ＝ 帧命令 `skill-read`：读来源那台上的一个 skill |
| `--ssh-config-import` | ＝ 帧命令 `ssh-config-import`：批量导入预览 |
| `--tasks-list` | ＝ 帧命令 `tasks-list`：一个会话的任务列表 |
| `--terminal-input` | ＝ 帧命令 `terminal-input`：往一个终端送字或送键（过身份门） |
| `--terminal-name-mint` | ＝ 帧命令 `terminal-name-mint`：起会话要的终端名 |
| `--terminal-preview` | ＝ 帧命令 `terminal-preview`：抓一个终端的一屏（只抓一次，轮询归调用方） |
| `--terminal-ssh` | ＝ 帧命令 `terminal-ssh`：给一台远端开终端要跑的那一串 |
| `--terminals-list` | ＝ 帧命令 `terminals-list`：这台的终端名单（形状与宿主无关；这一版宿主是 tmux） |
| `--tmux-notify` `<backend_pid> <backend_starttime>` | tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统 |

## 6. 几条命令的出入参类型

### `resolve`（入参 `ResumeSpec`，出参 `CommandPlan`）

#### `ResumeSpec`

入参（camelCase 对齐手机端 `ResumeSpec`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sessionId` | string |  |
| `launchCandidates` | [string]? | 可缺：候选启动命令，取第一个非空的；都没有 ⇒ 这一家的默认启动器 |
| `agentKind` | string? | 会话属哪个 agent kind ⇒ 据此构 `codex resume <uuid>` 或 `claude --resume`（additive，线上 `agentKind`） |

#### `Capabilities`

出参 caps（4 名**逐字复用手机端 `SessionCapabilities`**，camelCase 免映射）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `supportsSendKeys` | bool |  |
| `supportsCapture` | bool |  |
| `supportsMultiClient` | bool |  |
| `supportsMultiWindow` | bool |  |

#### `CommandPlan`

出参（camelCase 对齐手机端 `ResumePlan`，另加 mode/capabilities）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `command` | string | **唯一可信的一项** |
| `mode` | string | "PtyInject"（resume 走 pty send-keys 注入 §5④）\| "ExecOnce"（未来） |
| `capabilities` | Capabilities | **典型档，不是探测结果**（见结构体头注） |
| `sessionName` | string? | **纯从 sid 派生，不是探测结果**（见结构体头注）——拿它去 attach 一个「并不存在」的 tmux 会话是现实风险 |
| `launchLabel` | string? |  |
| `substitutedFrom` | string? |  |

### `session-fork` 的出参（应答另多 `launch` 一格）

#### `ForkReq`

`session-fork` 的入参：从 `sid` 那条会话的 `uuid` 那条消息处分叉。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件） |
| `uuid` | string | 从哪条消息处分叉 |

#### `Forked`

`session-fork` 的回包。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sessionId` | string | 新会话的 id |
| `jsonlPath` | string | 新会话的落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖） |
| `launch` | Launch | 起分叉出来的新会话要的三格 |

### `history-list` 每行与全文搜索每行的「这一行能做什么」

#### `Can`

`history-list` 每行（与全文搜索每个会话）的「这一行能做什么」（`can_of` 的成品）：闭集的词与两个开关。

| 字段 | 类型 | 说明 |
|---|---|---|
| `resume` | CanResume | `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（恢复主会话） |
| `accounts` | bool | 恢复时能不能选号 |
| `fork` | bool | 能不能从某一轮分叉 |
| `delete` | CanDelete | `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑 |

#### `CanResume`

`Can::resume` 的三个词。

- `yes`
- `switch`
- `bg`

#### `CanDelete`

`Can::delete` 的三个词。

- `yes`
- `live`
- `unsure`

### 骨架索引的一行（`history-index` 的 `rows` 装运之前）

#### `IndexRow`

骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（第一格 · `§2.5b 路 D`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `o` | number | 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)` |
| `n` | number | 行字节长（**含**结尾 `\n`） |
| `t` | RecordClass? | 这一行会翻成哪一类通用记录（`said` · `reply` · `retry` · `title` · `queued`，与读正文时那条记录的 `t` 同一个词）；不进界面（元数据 · 解析不出 · 认不出）⇒ 省略 |
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

#### `RecordProbe`

`record_in` 的答案。

| 字段 | 类型 | 说明 |
|---|---|---|
| `present` | bool | `<sid>.jsonl` 在记录树里（根那一层或项目目录那一层）找得到 |
| `root` | string | 查的那棵记录树的根 |
