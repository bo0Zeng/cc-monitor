<!-- 生成物，勿手改：由 tests/backend/protocol_doc_gen.rs 从代码生成。重生成：在 src/backend 下 cargo test --lib -- protocol_doc_gen 现打，再 bash tests/scripts/bless-golden.sh 拷回 -->

# 协议参考（帧 · 命令 · CLI）

总述（载体 · 信封 · 握手 · 冻结面）在 [IPC-PROTOCOL.md](IPC-PROTOCOL.md)；本页逐格列形状。

## 1. 出方向帧

每一帧一行 JSON，`kind` 是下面的小标题。`?` ＝ 可缺（缺 ＝ 没有这一格，不是 `null`）。

### `hello`

握手：连上之后那台发的第一帧。

| 字段 | 类型 | 说明 |
|---|---|---|
| `build_id` | string | 这份后端二进制的身份（`BUILD_ID`）；换不换后端按它判 |
| `host_arch` | string | 这台机器的架构（`x86_64` / `aarch64` …） |
| `version` | string? | 这份后端出自的那一版（仓根 `package.json` 的 `version`）；机器页那台的版本号就是它 |
| `commands` | [string]? | 这台后端接受的入方向命令；空 ⇒ 它不读命令 |
| `unavailable` | [Unavailable]? | 接得下、但这台做不到的命令与原因（见 `Unavailable`）；空 ＝ 没有「做不到」的把握 |
| `host_env` | {string: string}? | 起它的宿主交给它的那几格环境，原样回显（`{名: 值}`，名单住后端 `wire::HOST_ECHO_ENVS`） |
| `shapes` | {string: string}? | typed 命令的形状指纹 `{命令: 8 位十六进制}`（`crate::shapes::SHAPES`） |
| `frames` | {string: string}? | 每种帧（`hello` 除外）的形状指纹 `{kind: 8 位十六进制}`（`crate::frame_shapes::FRAMES`） |

### `line`

会话记录里的一行 —— 带的是**成品**：这一行在界面里是什么（`record`，缺 ＝ 不进界面、照占号）与它自己的 `cwd`。

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `seq` | number | 这一行在本条流里的序号（按文件单调递增）；不是续传键，续传用 `byte_offset` |
| `record` | Record? | 这一行的通用记录；缺 ＝ 不进界面、照占号 |
| `cwd` | string? | 这条记录自己的工作目录 |
| `byte_offset` | number? | 本行末尾（含 `\n`）在文件中的**累计原始字节 offset** ＝ 下一行的起点，两端拿它续读：计 CRLF 的 `\r`、含 `\n`、残行不计；从 N 续读 ⇒ `tail -c +(N+1)` |
| `rid` | string? | 这一行的对账键（适配层给；流的「开始」带同一个值） |

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
| `path` | string? | 该会话 jsonl 的远端绝对路径（同 sid 多文件时取 mtime 最新者）；宣告时还没写首行 ⇒ 缺 |
| `face` | SessionFace | 宣告时这条会话的整份脸 |
| `name_face` | NameFace | 叫这条会话什么（`NameFace`，平铺在帧上） |
| `hidden` | Hidden? | 隐藏那一格（不占标签页）；不隐藏 ⇒ 不上线 |
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
| `face` | SessionFace | 此刻这条会话的整份脸 |
| `name_face` | NameFace | 叫这条会话什么（同 `session_added` 那两格；改了名、来了标题 / 第一句也发这一帧） |
| `waiting_for` | string? | 在等什么（pidfile 里的 `waitingFor`） |
| `path` | string? | 这条会话的记录路径（同 `session_added` 那一格）：宣告之后才定下来 · 换了一份 ⇒ 带上最新的；没变 ⇒ 不上线 |

### `session_left`

**一条会话离开「活」的那一帧**（只这一帧）：去向就是 `face.life`（可重连 · 已结束· 记录已不在）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `face` | SessionFace | 离开「活」之后这条会话的整份脸（一生在 `face.life`） |

### `overflow`

这条连接拥塞丢过帧：那台写好的那一句（只丢了行 · 会话状态已重新对齐 · 两样都有）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `said` | Words | 那一句 |

### `reply`

**入方向命令的应答**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 回显请求的 `id` |
| `ok` | bool | 成功与否 |
| `code` | string? | 失败时的码（协议级或命令级） |
| `message` | string? | 失败时给人看的那一句（不含下层原话与码：那些进 `detail`） |
| `detail` | string? | 失败时「复制详情」那几行（时刻 · 机器 · 命令 · 码 · 原话），后端写好、界面原样复制 |
| `data` | JSON? | 命令的返回值（如 `resolve` 的 CommandPlan） |

### `cancelled`

某个在跑的命令**已被取消**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 被取消的那条命令的 `id` |

### `changed`

**这台的某样东西变了，客户端重读那一份**：按主题（`Topic`，线上 `topic`）分臂，每臂只有那个主题带的那几格。

（无字段）

### `sessions_replayed`

**这台机器的活会话清单报完了**：观察那一侧第一趟扫完、对每个活会话发过 `session_added` 之后发**一次**。

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
| `lines` | number | 重读完那份记录的完整行数 L（之后的第一帧 `line` 的 `seq` 就是它） |

### `transfer`

**一趟传输此刻的样子**（`transfer-start` 之后才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 传输单 id |
| `got` | number | 已传字节 |
| `total` | number | 总字节 |
| `end` | TransferEnd? | 只在最后一帧：这一趟怎么收场的 |

### `files_batch`

**一组（同一台上的复制 · 删除 · 改权限，两台之间的复制）此刻做到哪了**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 那一组的票（发起时交来的，后端只当不透明的串回填） |
| `done` | number | 做完了几项 |
| `total` | number | 一共几项 |
| `current` | WirePath? | 手上那一项（根底下的相对段）；做完了缺席 |
| `bytes_done` | number? | 整组按字节已传几个（只有搬字节的那一种报：两台之间的复制）；不报 ⇒ 缺席 |
| `bytes_total` | number? | 整组按字节一共几个（同 `bytes_done`，两格同在同缺） |

### `probe`

**测试连接那一趟的一格进度**（`remote-probe` 在跑时才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 测试连接那一趟的票（`remote-probe` 交来的） |
| `cell` | JSON | 一格进度：`said` · `tone` · `segs` · `end`（只最后一格有） |

### `live_card`

`live_card` 帧的体：哪个会话哪个运行的哪张卡、这条连接上它的第几帧、这一帧做什么。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `run` | string? | 归哪个子运行；缺 ＝ 主运行（主 tab 只画主运行的） |
| `card` | number | 卡号（本进程的段号，整台唯一） |
| `n` | number | 这条连接上这张卡的第几帧：`open` 从 0 起，之后连续 |
| `op` | LiveOp | 这一帧做什么 |

### `session_runs`

一个会话的运行表（主运行之外的那几个子运行）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `runs` | [RunInfo] | 在表里的子运行（整份） |
| `ended` | [RunEnded] | 被挤出表的已收场子运行 |
| `groups` | RunGroups? | 列表的三组（各是 `runs` 里的运行 id，最近动过的在前）：出口照这三组排，不按 `state` 分 |
| `summary` | Words? | 这条会话的 agent 那一枚写好的字（「agent n · m 在跑」·「agent n」）；表是空的 ⇒ 不上线 |

### `session_branch`

一份会话记录的**主线外清单**（用户回退重发后留在文件里的旧那一支）：那几条记录的 `id`。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `at` | number | 前几条照旧 |
| `off` | [string] | 第 `at` 条起的那几条 `id`（之后没有了：清单到此为止） |
| `sum` | string | 清单前 `at + off.len()` 条的摘要：客户端在 `stream-watch` 的 `branch` 里原样交回，后端照它判手上那份前缀还对不对 |

### `terminal_screen`

**一个终端此刻的一整屏**（终端实时预览；`terminal-follow` 之后才出现）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 订阅票（`terminal-follow` 时客户端给的，后端只当不透明的串回填） |
| `seq` | number | 这条订阅里第几帧（从 1 连续） |
| `view` | Preview | 那一屏（同 `terminal-preview` 的回话） |
| `input` | TermInput | 输入怎么进（同名单那一格）：托管那一形写此刻的持有者，持有者变了画面没变也推一帧；tmux 那一形恒 `shared` |

### `terminal_follow_end`

**一条终端订阅停了**（不会再有画面）；后端已经忘掉这张票。

| 字段 | 类型 | 说明 |
|---|---|---|
| `ticket` | string | 订阅票 |
| `why` | FollowEnd | 为什么停了（给程序认） |
| `said` | string | 给人看的那一句（后端写好，界面原样上屏） |

### `relayed`

**本机后端转来的别台那条常驻流上的事**（各台的常驻流归本机后端，`control/feeds.rs`；壳只读本机这一条）。

（无字段）

## 2. 信封与帧里用到的类型

#### `HelloHead`

hello 的头：版本门第一步只解它（全形解不开时也认得出是不是我们的后端、是哪一版）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `build_id` | string | 这份后端二进制的身份（`BUILD_ID`） |

#### `PlanNeeds`

`changed {plan}` 的体：这个工作区此刻要你看的数（没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `needs` | number |  |

#### `HiddenNow`

`changed {session_hidden}` 的体：那个会话隐藏那一格此刻的样子（不隐藏 ⇒ `null`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `hidden` | Hidden? |  |

#### `RunGroups`

运行表分成的三组（`SessionRuns::groups`）：在跑的全列 · 最近结束的至多几个 · 其余（连状态不明的）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `running` | [string] |  |
| `recent` | [string] |  |
| `older` | [string] |  |
| `older_text` | Words? | 更早那一组的组头字（「更早 ×n」），两端照抄；那一组空 ⇒ 不上线 |

#### `RunLine`

写好的一截字。

| 字段 | 类型 | 说明 |
|---|---|---|
| `text` | Words |  |
| `from` | number? |  |

#### `RunExplain`

说明那一块（`RunInfo::explain`）：为什么是这个样子。

| 字段 | 类型 | 说明 |
|---|---|---|
| `lead` | Words? | 打头那一格（状态不明那一种单独写状态字）；别的 ⇒ 不上线 |
| `line` | RunLine |  |
| `note` | Words? | 小一号的补一句 |
| `refresh` | bool? | 带［刷新］（状态不明） |

#### `RunEndLine`

尾巴上的结束线（`RunInfo::end`）：状态字（图标由出口照 `state` 画）＋ 一句。

| 字段 | 类型 | 说明 |
|---|---|---|
| `head` | Words |  |
| `sub` | RunLine |  |

#### `SessionActivity`

一条活会话此刻在干什么（与哪一家无关的几态；适配层从那一家的进程状态翻过来，翻不出 ⇒ 不给）。

- `working` —— 一轮在跑
- `needs_you` —— 在等人（批准 · 回答 · 弹窗）
- `idle` —— 闲着，等下一句输入
- `background_work` —— 一轮停了，它在后台起的命令还在跑（跑完多半会接着干）

#### `SessionFace`

**一条会话此刻的整份脸**（会话三帧 · 清单行 · 需手动行 · 计划 · 血缘都带它，嵌一格 `face`）：点 · 短名 · 悬停句 · 状态一句 · 序 · 能做什么。

| 字段 | 类型 | 说明 |
|---|---|---|
| `dot` | SessionDot |  |
| `life` | Life | 这条会话的一生（在干什么只住在「活」那一支里） |
| `settled` | bool | 那台看不见时照旧作数（已结束 · 记录已不在） |
| `name` | Words | 短名（运行中 · 需手动 · 已结束 …） |
| `tip` | Words | 悬停那一句 |
| `line` | Words | 没有会话事实时的状态一句 |
| `tone` | Tone |  |
| `order` | number | 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5 |
| `can` | SessionCan |  |
| `account` | string? | 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它 |

#### `Life`

**一条会话的一生**（线上只此一处说活不活）：`{is: "live", activity?}`（判得出在干什么才带）· `{is: "reconnectable"}` 程序退了、tmux 会话还在 · `{is: "ended", by?}` 进程没了（被顶替 ⇒ `by` 是顶替它的那条）· `{is: "gone"}` 已结束、记录也不在了 · `{is: "unclear"}` 说不清（判不了 · 那台看不见）。

- `is: "live"`，带 `activity` SessionActivity?
- `is: "reconnectable"`
- `is: "ended"`，带 `by` string?
- `is: "gone"`
- `is: "unclear"`

#### `SessionDot`

会话那颗点（线上名与界面状态点同一套）。

- `running`
- `needs-you`
- `background`
- `idle`
- `exited`
- `ended`
- `gone`
- `unknown`

#### `SessionCan`

这条会话此刻能做什么（界面照 `can` 灰不灰、照 `why` 写跳过的理由，不自己判）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `close` | CanClose | 关掉它的标签页 |
| `resume` | Allow | 恢复（起一个新进程接着那份记录） |
| `term` | CanTerm | 还有终端可去（拉前 · 停 · 杀 · 终端页） |
| `reattach` | Allow | 程序退了、tmux 会话还在 ⇒ 接回 |
| `model` | Allow | 换模型（`session-model`） |
| `permission` | Allow | 切权限档（`session-permission`） |
| `stop` | Allow | 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能 |
| `say` | CanSay | 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、 `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好 |
| `end` | Allow | 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好 |

#### `Allow`

一样此刻能不能做：`{can: "yes"}` · `{can: "no", why: 写好的那一句}`。

- `can: "yes"`
- `can: "no"`，带 `why` Words

#### `CanSay`

说不说得了一句：`{can: "yes", hint: 框里的占位那一句}` · `{can: "no", why: 写好的那一句}`。

- `can: "yes"`，带 `hint` Words
- `can: "no"`，带 `why` Words

#### `CanClose`

关不关得掉：`quiet` × · 中键 · W · 批量都关得掉 · `menu` 只在右键菜单里关（那台看不见时界面自己给的那一形）· `no`。

- `can: "quiet"`
- `can: "menu"`
- `can: "no"`，带 `why` Words

#### `CanTerm`

还有没有终端可去：`yes` 带它住在哪种宿主里（`tmux` · `managed`；不在任何宿主里 ⇒ `direct`）。

- `can: "yes"`，带 `host` Launch
- `can: "no"`，带 `why` Words

#### `NameFace`

**这条会话叫什么那两格**（`session_added` · `session_status` 都带它，平铺在帧上；一轮结束的通知标题也读它）： `label` 与历史清单 · 全文搜索同一个函数算；`label_project` ＝ 项目目录最后一截（不知道项目目录 ⇒ 不带）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `label` | Words |  |
| `label_project` | Words? |  |

#### `TerminalHost`

终端住在哪种宿主里（线上 `host`）。

- `tmux`
- `managed` —— cc-monitor 自己持伪终端的终端（后端 `host/managed.rs` 报的行）

#### `SessionContainer`

`session_added` 的 `container`：**这条活着的会话住在什么容器里**，词与 `terminals-list` 同一套。

- `host: "tmux"`，带 `terminal` string? —— 在 tmux 的终端里（`TMUX_PANE` 核过形状、tmux 认得那个 pane）
- `host: "managed"`，带 `terminal` string? —— 在托管终端里（进程环境里的 `CCM_HOST` 认得那个托管终端，`terminal` 是它的句柄）
- `host: "none"` —— 环境读得到、不在任何宿主里

#### `Unavailable`

`hello.unavailable` 的一项 —— **这条命令我接得下，但在这台机器上做不到，以及为什么**。

| 字段 | 类型 | 说明 |
|---|---|---|
| `command` | string | 哪条命令 —— 取值空间与 `Hello.commands` **同一套**（`inbound::command_names`） |
| `code` | string | 为什么做不到 —— **就是真调用那一刻会回的那个命令级 code**（如 `no_tmux`） |

#### `Topic`

推送主题（线上 `changed.topic`）。

- `accounts` —— 这台的账号清单（账号 manifest 被改写）
- `profiles` —— 这台的配置文件（`~/.cc-monitor/profiles.toml`）
- `quota` —— 这台的额度账显示得出来的那几格
- `rotation` —— 这台某个会话的轮换（`key` ＝ sid）：那一条 · 它用的那份策略 · 事实变了
- `rotation_strategies` —— 这台的策略目录 · 新会话用哪份 · 哪个会话用哪份
- `plan` —— 这台某个 pb 工作区的计划（`key` ＝ 工作区根，`rev` ＝ 新的输出摘要，与手上那一份相同 ⇒ 不用问； `body` ＝ `{needs}`：这个工作区此刻要你看的数，没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）
- `tasks` —— 这台某个会话的任务清单（`key` ＝ sid）
- `needs` —— 这台的需手动可能变了（`key` ＝ sid：那条会话挂着的请求挂上 · 撤掉 · 答掉 · 「没人答时自动答」改了 · 进出等人 · 离开「活」，重问 `sessions-needs` 与那个会话的 `history-facts`；不带 `key` ＝ 会话外那几项（账号要登录 · MCP 要登录 · 登录作业收了）或计划要人看的数变了，重问 `sessions-needs`）
- `login` —— 一份登录作业自己收了（`key` ＝ 作业号；`body` ＝ `{state: done \| failed, said?, detail?}`）
- `machines` —— 这一端的机器表（`machines.json` 被改写；重问 `machines-read`）
- `machine` —— 这一端连的某台远端此刻的状态（`key` ＝ 那台的名字；`body` ＝ 那台的成品 `MachineState`；丢了重问 `machine-read`）
- `empty` —— 这一端的首屏那一句（一条会话都没有时正文区中央那一块；`body` ＝ `EmptyFace`，各台的成品与报完了没有合的；丢了重问 `machine-read`）
- `session_hidden` —— 这台某个活会话隐不隐藏变了（`key` ＝ sid；`body` ＝ `{hidden}`：那一格，不隐藏 ⇒ `null`）：注解 · 这台偏好 · 会话血缘动了之后判出来的
- `files_dir` —— 这台某个正被浏览的目录里有了动静（`key` ＝ 那个目录的 `WirePath::key`；名单由 `files-browse` 交）：正开着那个目录的那一屏带着手上的记号重问 `files-ls`，只拿变了的那几行
- `history_list` —— 这台的会话清单可能动了（会话来去 · 名字 / 状态变 · 记录长了 · 注解 · 隐藏规则；同一个会话在这台答下一份清单之前只说一次）：正看着清单的那一屏带着手上的记号重问 `history-list`（远端那台由本机后端带 `fresh` 去同步），只拿变了的那几行
- `groups` —— 这台的组那一片动了（这台有组员的组改名 · 解散，或这台会话进出组 · 固定变了）：看着组的那一屏重问本机后端 `groups-read`
- `session_queue` —— 这台某条会话的输入队动了（`key` ＝ sid；`body` ＝ 整份名单，同 `session-queue-read` 的回包）：投进 · 撤 · 送进 · 标未送达，哪一端投的都算

#### `RereadWhy`

`Frame::SessionFileReread` 的「为什么从头重读」。

- `truncated` —— 变短了（比读到过的最长还短）
- `rewritten` —— 没变短，但游标之前那一截被原地改写过（末尾指纹对不上）

#### `TransferEnd`

一趟传输怎么收场的（`Frame::Transfer` 的 `end`）。

- `state: "done"`，带 `bytes` number · `sha256` string? —— 传完了
- `state: "failed"`，带 `why` string · `code` string? · `detail` string? —— 失败（那一句 ＋ 复制详情）
- `state: "cancelled"` —— 撤了（`transfer-stop` / 本机流断了）

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
| `status_text` | Words? | 终态写好的字（同 `RunInfo::status_text`） |

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
| `status_text` | Words? | 状态写好的字（在跑 · 完成 · 失败 · 已中止 · 状态不明）：出口照抄，不按 `state` 挑字 |
| `label_text` | Words? | 名字写好的字：`label`，没有 ⇒ 通称（「agent」） |
| `last` | RunDid? |  |
| `last_text` | Words? | `last` 写好的字（写回复中 · 思考中 · 调用 X）；没有 `last` ⇒ 不上线 |
| `waiting` | string? | 在等哪个工具的结果（它最近一条记录是一次还没拿到结果的工具调用、且还在跑） |
| `started_ms` | number? | 开始：派出它的那条记录（没见到 ⇒ 它自己最早的一条） |
| `started_text` | string? | 「几点起」那一句（`started_ms` 在看的那一台钟上的钟面，推出去那一下按这条流的时区写，`Frame::stamp`；界面照抄、不换算）：事实那一行的头一截 |
| `active_ms` | number? | 最近动静：它自己最近一条记录 |
| `ended_ms` | number? | 收场：说它收场的那一条（还没收场 / 状态不明 ⇒ 不上线） |
| `why` | RunWhy? | 为什么是这个结局（在跑 ⇒ 不上线） |
| `error` | string? | 失败收场时的报错原话（说得出才有；至多 `observe::runs::ERROR_CHARS` 个字） |
| `calls` | number? | 它调了几次工具（它自己的记录里数的；零 ⇒ 不上线） |
| `background` | bool? | 派出那一方没等它、接着做自己的事（后台派出；否则不上线） |
| `facts` | [RunLine]? | 事实那一行的各截（已跑 · 用时 · 最近 · 工具 ×n · agent ×n · 后台派出；「几点起」在 `started_text`）；一截都没有 ⇒ 不上线 |
| `explain` | RunExplain? | 说明（在跑且没在等 ⇒ 不上线） |
| `end` | RunEndLine? | 结束线（在跑 ⇒ 不上线） |
| `needs` | Needs? | 它挂着的待答请求（同会话事实的 `needs` 那一形，最早挂上的那一条）；没有 ⇒ 不上线 |

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

#### `RunDid`

一个运行最近做的那件事（通用的值域；某一家的哪种记录算哪一件由那一家判）。

- `t: "say"` —— 在写回复
- `t: "think"` —— 在想
- `t: "tool"`，带 `name` string —— 调用了一个工具

#### `LiveBlock`

活卡里的一块。

| 字段 | 类型 | 说明 |
|---|---|---|
| `kind` | BlockKind |  |
| `text` | string? | 文字块已到的正文（只有文字块有；思考不展开、工具入参不显示） |
| `tool` | string? | 工具块的工具名 |

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

#### `LiveWhy`

为什么撤卡。

- `record` —— 那一轮的记录到了（整轮覆盖）
- `broken` —— 上游断在半路（Esc 打断 · 上游断 · 抄流那一跳缺了号）
- `error` —— 上游报错收尾
- `evicted` —— 满了被挤掉（同一运行 2 张 · 整台 16 张）
- `gone` —— 会话走了 / 子运行收场了
- `unwatched` —— 出了这条连接的名单

#### `BranchHeld`

`stream-watch.branch` 的一格：这条连接要这个会话的主线外清单；手上已有那份记录（`path`）的前 `at` 条、摘要 `sum`（上一帧 `session_branch` 给的）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string |  |
| `path` | string? |  |
| `at` | number? | 手上那份的前几条（照手上最后一帧 `session_branch` 的 `at + off.len()`；手上没有 ⇒ `0`） |
| `sum` | string? |  |

#### `WatchAsk`

`stream-watch` 的入参：在看哪几个（整份换）＋ 要哪几个的主线外清单、手上各有什么（`branch`；不报 ＝ 一份都不要）＋ 手上留着、要那台说清去向的（`kept`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sids` | [string] | 在看的会话 id（空 ＝ 一个都不看） |
| `branch` | [BranchHeld]? | 要哪几个会话的主线外清单、手上各有什么（可缺 ＝ 一份都不要） |
| `kept` | [string]? | 手上留着记录、要那台说清去向的会话 id：那台报完清单、又不在说的 ⇒ 这条连接收一帧已结束（可缺 ＝ 一个都没有） |

#### `WatchFrom`

`stream-watch` 应答里的一格：这个会话刚进了这条流的名单，它的这份记录从第 `seq` 行起上流（`seq` 与行帧同一个行号空间）；`[0, seq)` 那段客户端按骨架补（`history-index` 从上次的末端续）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `sid` | string | 会话 id |
| `path` | string | 这份会话记录在那台机器上的绝对路径 |
| `seq` | number | 这条流从这一行起发它 |

#### `WatchReply`

`stream-watch` 的应答。

| 字段 | 类型 | 说明 |
|---|---|---|
| `from` | [WatchFrom] | 刚进名单的会话每份记录一格（已在名单里的不回） |

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
branch?: Array<BranchHeld>, 
/**
 * 手上留着记录、要那台说清去向的会话 id：那台报完清单、又不在说的 ⇒ 这条连接收一帧已结束（可缺 ＝ 一个都没有）。
 */
kept?: Array<string>, };
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
 * 刚进名单的会话每份记录一格（已在名单里的不回）。
 */
from: Array<WatchFrom>, };
```

码：`bad_args`

#### `transfer-upload`

开一张上传单。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferUploadReq` · 回包 `TransferOpened`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Conflict = { 
/**
 * 同 [`BatchOutcome::key`]。
 */
key: string, 
/**
 * 落点那一段名字。
 */
name: WirePath, 
/**
 * 要放过去的那一项是什么。
 */
kind: FileKind, 
/**
 * 要放过去的那一项的大小（文件）。
 */
size?: number, 
/**
 * `size` 写好的字（`2.1 MB`）。
 */
size_text?: string, 
/**
 * 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
 */
mtime_text?: string, 
/**
 * 已在那儿的那一项是什么。
 */
there: FileKind, 
/**
 * 已在那儿的那一项的大小（文件）。
 */
there_size?: number, 
/**
 * `there_size` 写好的字。
 */
there_size_text?: string, 
/**
 * 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
 */
there_mtime_text?: string, 
/**
 * 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
 */
answers: Array<Decide>, };
type Conflicts = { conflicts: Array<Conflict>, };
type Decide = "overwrite" | "skip" | "keep_both";
type FileKind = "dir" | "file" | "symlink" | "other";
type OnExists = "ask" | "overwrite" | "skip" | "keep_both";
type OpenState = "opened" | "skipped" | "failed";
type OpenedItem = { key: string, state: OpenState, 
/**
 * 传输单 id（`xfer-<n>`）。
 */
id?: string, 
/**
 * 上传：暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端后端。
 */
staged?: string, 
/**
 * 下载：本机落点。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（下载：本机落名；上传：提交时远端那一段名字）。
 */
new_name?: WirePath, 
/**
 * 上传：提交时盖掉远端已有的同名（同名照答覆盖的那几项）。
 */
overwrite?: boolean, 
/**
 * 没开成的那一项的码 · 原因词 · 复制详情（同一组那几条的结局）。
 */
code?: string, reason?: string, detail?: string, };
type Origin = string;
type TransferOpened = { items: Array<OpenedItem>, };
type TransferUploadReq = { 
/**
 * 那台远端的名字。
 */
machine: Origin, items: Array<UploadItem>, remote_dir: WirePath, home?: string, on_exists?: OnExists, decide?: { [key in string]: Decide }, };
type UploadItem = { local_path: string, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `exists`

失败带 `data` 的码：`exists` ⇒ `Conflicts`

#### `transfer-download`

开一张下载单。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferDownloadReq` · 回包 `TransferOpened`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Conflict = { 
/**
 * 同 [`BatchOutcome::key`]。
 */
key: string, 
/**
 * 落点那一段名字。
 */
name: WirePath, 
/**
 * 要放过去的那一项是什么。
 */
kind: FileKind, 
/**
 * 要放过去的那一项的大小（文件）。
 */
size?: number, 
/**
 * `size` 写好的字（`2.1 MB`）。
 */
size_text?: string, 
/**
 * 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
 */
mtime_text?: string, 
/**
 * 已在那儿的那一项是什么。
 */
there: FileKind, 
/**
 * 已在那儿的那一项的大小（文件）。
 */
there_size?: number, 
/**
 * `there_size` 写好的字。
 */
there_size_text?: string, 
/**
 * 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
 */
there_mtime_text?: string, 
/**
 * 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
 */
answers: Array<Decide>, };
type Conflicts = { conflicts: Array<Conflict>, };
type Decide = "overwrite" | "skip" | "keep_both";
type DownloadItem = { remote_path: string, name?: WirePath, };
type FileKind = "dir" | "file" | "symlink" | "other";
type OnExists = "ask" | "overwrite" | "skip" | "keep_both";
type OpenState = "opened" | "skipped" | "failed";
type OpenedItem = { key: string, state: OpenState, 
/**
 * 传输单 id（`xfer-<n>`）。
 */
id?: string, 
/**
 * 上传：暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端后端。
 */
staged?: string, 
/**
 * 下载：本机落点。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（下载：本机落名；上传：提交时远端那一段名字）。
 */
new_name?: WirePath, 
/**
 * 上传：提交时盖掉远端已有的同名（同名照答覆盖的那几项）。
 */
overwrite?: boolean, 
/**
 * 没开成的那一项的码 · 原因词 · 复制详情（同一组那几条的结局）。
 */
code?: string, reason?: string, detail?: string, };
type Origin = string;
type TransferDownloadReq = { 
/**
 * 那台远端的名字。
 */
machine: Origin, items: Array<DownloadItem>, local_dir: WirePath, on_exists?: OnExists, decide?: { [key in string]: Decide }, };
type TransferOpened = { items: Array<OpenedItem>, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `refused` · `exists`

失败带 `data` 的码：`exists` ⇒ `Conflicts`

#### `transfer-start`

传输单起跑。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferStartReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type TransferStartReq = { id: string, };
```

码：`bad_args` · `no_such_transfer` · `already_started`

#### `transfer-stop`

撤一张传输单。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferStopReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type TransferStopReq = { id: string, };
```

码：`bad_args`

#### `transfer-copy`

两台之间复制一组。

收 `args` · 连接内就地做完 · 只在流上

入参 `TransferCopyReq` · 回包 `BatchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BatchOutcome = { key: string, state: ItemState, 
/**
 * 落在哪（做完的那几项；保留两份时是另起的那个名）。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（只一段）。
 */
new_name?: WirePath, 
/**
 * 这一项落了几个字节（复制 · 传输）。
 */
bytes?: number, 
/**
 * 这一项复制了几份文件 · 几个目录（复制：目录那一项是整棵的数）。
 */
files?: number, dirs?: number, 
/**
 * 这一项删了几条（删除：整棵树按条数；撤在半路时是已删的那几条）。
 */
removed?: number, 
/**
 * 改权限之前的低 12 位（撤销 ＝ 拿它再改一次；读不到就缺席）。
 */
before?: number, 
/**
 * 没做成的那一项的码（同单条命令的码表：`refused` · `io_failed` · `stale` · `no_unix_mode` · `exists` …）。
 */
code?: string, 
/**
 * 原因词（闭集，同界面「失败 · X」那一格）。
 */
reason?: string, 
/**
 * 复制详情（下层那一句 ＋ 原话），不上界面那一行。
 */
detail?: string, };
type BatchReply = { outcomes: Array<BatchOutcome>, said: string, };
type Conflict = { 
/**
 * 同 [`BatchOutcome::key`]。
 */
key: string, 
/**
 * 落点那一段名字。
 */
name: WirePath, 
/**
 * 要放过去的那一项是什么。
 */
kind: FileKind, 
/**
 * 要放过去的那一项的大小（文件）。
 */
size?: number, 
/**
 * `size` 写好的字（`2.1 MB`）。
 */
size_text?: string, 
/**
 * 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
 */
mtime_text?: string, 
/**
 * 已在那儿的那一项是什么。
 */
there: FileKind, 
/**
 * 已在那儿的那一项的大小（文件）。
 */
there_size?: number, 
/**
 * `there_size` 写好的字。
 */
there_size_text?: string, 
/**
 * 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
 */
there_mtime_text?: string, 
/**
 * 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
 */
answers: Array<Decide>, };
type Conflicts = { conflicts: Array<Conflict>, };
type CopyFrom = { remote_path: string, };
type Decide = "overwrite" | "skip" | "keep_both";
type FileKind = "dir" | "file" | "symlink" | "other";
type ItemState = "done" | "skipped" | "exists" | "failed" | "cancelled";
type OnExists = "ask" | "overwrite" | "skip" | "keep_both";
type Origin = string;
type TransferCopyReq = { 
/**
 * 源那台的名字。
 */
from: Origin, 
/**
 * 落点那台的名字。
 */
to: Origin, items: Array<CopyFrom>, 
/**
 * 落点那台上的目录（绝对路径）。
 */
to_dir: string, on_exists?: OnExists, decide?: { [key in string]: Decide }, ticket?: string, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `exists` · `unreachable` · `sftp_home_mismatch`

失败带 `data` 的码：`exists` ⇒ `Conflicts`

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

删一组（文件 · 空目录 · 显式递归的整棵）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-delete`

入参 `FilesDeleteReq` · 回包 `BatchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BatchOutcome = { key: string, state: ItemState, 
/**
 * 落在哪（做完的那几项；保留两份时是另起的那个名）。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（只一段）。
 */
new_name?: WirePath, 
/**
 * 这一项落了几个字节（复制 · 传输）。
 */
bytes?: number, 
/**
 * 这一项复制了几份文件 · 几个目录（复制：目录那一项是整棵的数）。
 */
files?: number, dirs?: number, 
/**
 * 这一项删了几条（删除：整棵树按条数；撤在半路时是已删的那几条）。
 */
removed?: number, 
/**
 * 改权限之前的低 12 位（撤销 ＝ 拿它再改一次；读不到就缺席）。
 */
before?: number, 
/**
 * 没做成的那一项的码（同单条命令的码表：`refused` · `io_failed` · `stale` · `no_unix_mode` · `exists` …）。
 */
code?: string, 
/**
 * 原因词（闭集，同界面「失败 · X」那一格）。
 */
reason?: string, 
/**
 * 复制详情（下层那一句 ＋ 原话），不上界面那一行。
 */
detail?: string, };
type BatchReply = { outcomes: Array<BatchOutcome>, said: string, };
type DeleteExpect = { empty_dir: true } | string | { b16: string };
type DeleteItem = { rel: WirePath, recursive?: boolean, expect?: DeleteExpect, };
type FilesDeleteReq = { root: WirePath, items: Array<DeleteItem>, ticket?: string, };
type ItemState = "done" | "skipped" | "exists" | "failed" | "cancelled";
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path`

#### `files-chmod`

一组改成同一个 unix 权限位。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-chmod` · 没有 unix 权限位的机器上做不到

入参 `FilesChmodReq` · 回包 `BatchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BatchOutcome = { key: string, state: ItemState, 
/**
 * 落在哪（做完的那几项；保留两份时是另起的那个名）。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（只一段）。
 */
new_name?: WirePath, 
/**
 * 这一项落了几个字节（复制 · 传输）。
 */
bytes?: number, 
/**
 * 这一项复制了几份文件 · 几个目录（复制：目录那一项是整棵的数）。
 */
files?: number, dirs?: number, 
/**
 * 这一项删了几条（删除：整棵树按条数；撤在半路时是已删的那几条）。
 */
removed?: number, 
/**
 * 改权限之前的低 12 位（撤销 ＝ 拿它再改一次；读不到就缺席）。
 */
before?: number, 
/**
 * 没做成的那一项的码（同单条命令的码表：`refused` · `io_failed` · `stale` · `no_unix_mode` · `exists` …）。
 */
code?: string, 
/**
 * 原因词（闭集，同界面「失败 · X」那一格）。
 */
reason?: string, 
/**
 * 复制详情（下层那一句 ＋ 原话），不上界面那一行。
 */
detail?: string, };
type BatchReply = { outcomes: Array<BatchOutcome>, said: string, };
type ChmodItem = { rel: WirePath, };
type FilesChmodReq = { root: WirePath, items: Array<ChmodItem>, 
/**
 * 八进制写法原样（`644` · `0755`，框里敲的就交这几个字）；由那台解（空 · 不是八进制 · 超出 7777 ⇒ 整组 `refused`、那一句那台写）。
 */
mode: string, ticket?: string, };
type ItemState = "done" | "skipped" | "exists" | "failed" | "cancelled";
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `no_unix_mode` · `refused`

#### `files-copy`

同根内复制一组。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-copy`

入参 `FilesCopyReq` · 回包 `BatchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BatchOutcome = { key: string, state: ItemState, 
/**
 * 落在哪（做完的那几项；保留两份时是另起的那个名）。
 */
path?: WirePath, 
/**
 * 保留两份时另起的那个名（只一段）。
 */
new_name?: WirePath, 
/**
 * 这一项落了几个字节（复制 · 传输）。
 */
bytes?: number, 
/**
 * 这一项复制了几份文件 · 几个目录（复制：目录那一项是整棵的数）。
 */
files?: number, dirs?: number, 
/**
 * 这一项删了几条（删除：整棵树按条数；撤在半路时是已删的那几条）。
 */
removed?: number, 
/**
 * 改权限之前的低 12 位（撤销 ＝ 拿它再改一次；读不到就缺席）。
 */
before?: number, 
/**
 * 没做成的那一项的码（同单条命令的码表：`refused` · `io_failed` · `stale` · `no_unix_mode` · `exists` …）。
 */
code?: string, 
/**
 * 原因词（闭集，同界面「失败 · X」那一格）。
 */
reason?: string, 
/**
 * 复制详情（下层那一句 ＋ 原话），不上界面那一行。
 */
detail?: string, };
type BatchReply = { outcomes: Array<BatchOutcome>, said: string, };
type Conflict = { 
/**
 * 同 [`BatchOutcome::key`]。
 */
key: string, 
/**
 * 落点那一段名字。
 */
name: WirePath, 
/**
 * 要放过去的那一项是什么。
 */
kind: FileKind, 
/**
 * 要放过去的那一项的大小（文件）。
 */
size?: number, 
/**
 * `size` 写好的字（`2.1 MB`）。
 */
size_text?: string, 
/**
 * 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
 */
mtime_text?: string, 
/**
 * 已在那儿的那一项是什么。
 */
there: FileKind, 
/**
 * 已在那儿的那一项的大小（文件）。
 */
there_size?: number, 
/**
 * `there_size` 写好的字。
 */
there_size_text?: string, 
/**
 * 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
 */
there_mtime_text?: string, 
/**
 * 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
 */
answers: Array<Decide>, };
type Conflicts = { conflicts: Array<Conflict>, };
type CopyItem = { from: WirePath, to: WirePath, };
type Decide = "overwrite" | "skip" | "keep_both";
type FileKind = "dir" | "file" | "symlink" | "other";
type FilesCopyReq = { root: WirePath, items: Array<CopyItem>, on_exists?: OnExists, decide?: { [key in string]: Decide }, ticket?: string, 
/**
 * `true` ⇒ 每项的 `to` 只许是一段名字、且不同于 `from`（界面「复制为」那一格敲的就是一个名字）。
 */
single?: boolean, };
type ItemState = "done" | "skipped" | "exists" | "failed" | "cancelled";
type OnExists = "ask" | "overwrite" | "skip" | "keep_both";
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_name` · `bad_path` · `exists`

失败带 `data` 的码：`exists` ⇒ `Conflicts`

#### `files-batch-stop`

撤一组复制 · 删除 · 改权限。

收 `args` · 可撤 · CLI：`ccm -- --files-batch-stop`

入参 `FilesBatchStopReq` · 回包 `FilesBatchStopReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FilesBatchStopReply = { stopping: boolean, };
type FilesBatchStopReq = { ticket: string, };
```

码：`bad_args`

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
type Conflict = { 
/**
 * 同 [`BatchOutcome::key`]。
 */
key: string, 
/**
 * 落点那一段名字。
 */
name: WirePath, 
/**
 * 要放过去的那一项是什么。
 */
kind: FileKind, 
/**
 * 要放过去的那一项的大小（文件）。
 */
size?: number, 
/**
 * `size` 写好的字（`2.1 MB`）。
 */
size_text?: string, 
/**
 * 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
 */
mtime_text?: string, 
/**
 * 已在那儿的那一项是什么。
 */
there: FileKind, 
/**
 * 已在那儿的那一项的大小（文件）。
 */
there_size?: number, 
/**
 * `there_size` 写好的字。
 */
there_size_text?: string, 
/**
 * 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
 */
there_mtime_text?: string, 
/**
 * 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
 */
answers: Array<Decide>, };
type Conflicts = { conflicts: Array<Conflict>, };
type Decide = "overwrite" | "skip" | "keep_both";
type FileKind = "dir" | "file" | "symlink" | "other";
type FilesCommitUploadReq = { root: WirePath, rel: WirePath, key: string, 
/**
 * 落点已有同名：同成组复制那一条（缺 ⇒ `ask`：码 `exists`、`data` 是同名清单，一个字节不动、块留着，改个答原样再交）。
 * 只有一件 ⇒ `skip` 不收（不放就别提交）。
 */
on_exists?: OnExists, expect: Sha256Expect, chunks?: number, bytes?: number, 
/**
 * `true` ⇒ `rel` 只许是一段名字（界面上传一份文件：名字是本机那份的名字，或别的 App 交来的显示名）。不论给没给，每一段都过这台的名字规则。
 */
single?: boolean, };
type FilesPathBytesReply = { path: WirePath, bytes: number, };
type OnExists = "ask" | "overwrite" | "skip" | "keep_both";
type Sha256Expect = { sha256: Sha256Hex, };
type Sha256Hex = string;
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_name` · `bad_path` · `io_failed` · `refused` · `stale` · `exists`

失败带 `data` 的码：`exists` ⇒ `Conflicts`

#### `session-attachment-check`

开传前问一次：这一条的附件逐份收不收（上限 · 目录 · 工作目录可写）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-attachment-check`

入参 `SessionAttachmentCheckReq` · 回包 `SessionAttachmentCheckReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AttachItem = { name: string, bytes: number, dir?: boolean, };
type AttachRefused = "is_dir" | "too_big" | "too_many" | "too_much" | "dir_full" | "not_writable" | "no_session";
type AttachVerdict = { ok: boolean, 
/**
 * 照名字判的那一种（目录 ⇒ 文件夹；后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好，同文件管理「类型」那一列的词）：chip 上的类别字。
 */
class_text: string, why?: AttachRefused, said?: string, };
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
type SessionAttachmentCheckReply = { items: Array<AttachVerdict>, };
type SessionAttachmentCheckReq = { sid: Sid, items: Array<AttachItem>, };
type Sid = string;
```

码：`bad_args`

#### `session-attachment-commit`

把暂存区里传完的一份落进会话工作目录下一个新的附件目录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-attachment-commit`

入参 `SessionAttachmentCommitReq` · 回包 `SessionAttachmentCommitReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
type SessionAttachmentCommitReply = { id: string, name: string, path: string, rel: string, bytes: number, sha256: Sha256Hex, class: FileClass, };
type SessionAttachmentCommitReq = { sid: Sid, key: string, 
/**
 * 原名（一段，过那台的名字规则）。
 */
name: string, bytes: number, chunks?: number, expect: Sha256Expect, };
type Sha256Expect = { sha256: Sha256Hex, };
type Sha256Hex = string;
type Sid = string;
```

码：`bad_args` · `too_big` · `dir_full` · `not_writable` · `no_session` · `bad_name` · `io_failed` · `refused` · `stale` · `exists`

#### `session-attachment-drop`

删掉会话工作目录下的一个附件目录（草稿上撤掉那一份）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-attachment-drop`

入参 `SessionAttachmentDropReq` · 回包 `SessionAttachmentDropReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SessionAttachmentDropReply = { removed: boolean, };
type SessionAttachmentDropReq = { sid: Sid, id: string, };
type Sid = string;
```

码：`bad_args` · `no_session` · `not_writable` · `io_failed` · `refused`

#### `files-browse`

告诉后端「这条连接现在在看哪几个目录」（各条连接的并集被盯着；连接断了摘掉它那份）。

收 `args` · 连接内就地做完 · 只在流上

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
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
type FileKind = "dir" | "file" | "symlink" | "other";
type FilesFindHit = { path: WirePath, kind: FileKind, 
/**
 * 画哪种图标（索引不跟链接 ⇒ 链接画 [`FileClass::Link`]）。
 */
class: FileClass, 
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

入参 `LsReq` · 回包 `SyncPatch<LsHead, LsEntry>`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Crumb = { name: WirePath, path: WirePath, };
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
type FileKind = "dir" | "file" | "symlink" | "other";
type Gate = { "cap": number } | { "no": string };
type Gates = { preview: PreviewGate, 
/**
 * 查看页：只在两端解得开的那几种图上有。
 */
view?: Gate, edit: Gate, };
type LinkTo = "dir" | "file" | "missing";
type LsEntry = { path: WirePath, 
/**
 * 末段名（那台按自己的分隔符切好）。
 */
name: WirePath, kind: FileKind, size?: number, 
/**
 * `size` 写好的字（`2.1 MB`，`copy_core::size_text`）；目录 · 指向目录的链接 · 读不到大小的 ⇒ 缺。
 */
size_text?: string, mtime_secs?: number, 
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
link_to?: LinkTo, 
/**
 * 画哪种图标（链接画它指向的那一种：指向目录 ⇒ 文件夹，其余按名字）。
 */
class: FileClass, 
/**
 * 「类型」那一列的字（那台写好）。
 */
type_text: string, 
/**
 * 读得起来的那几道门；目录（含指向目录的链接）缺。
 */
gates?: Gates, 
/**
 * `path` 的显示形（那台按自己的家目录写好的 `~/…`，原始字节同 `path`），两端照抄。
 */
shown: WirePath, };
type LsHead = { 
/**
 * 列的这一级的显示形（那台按自己的家目录写好的 `~/…`，原始字节），两端照抄。
 */
shown: WirePath, 
/**
 * 上一级；到顶 ⇒ `null`。
 */
up: WirePath | null, 
/**
 * 根到这一级的每一段，根在最前（那台按自己的路径规则切：Windows 的盘符那一段是 `C:\`）。
 */
crumbs: Array<Crumb>, 
/**
 * 读到的比这一份多（被 `limit` 截了）。
 */
truncated: boolean, 
/**
 * 目录里一共读到几项（含被 `limit` 截掉的）。
 */
total: number, 
/**
 * 有几项读不出来（照数，不静默少一项）。
 */
unreadable: number, };
type LsReq = { path: WirePath, 
/**
 * 这一份至多几条（缺 · 0 ⇒ 后端默认 1000）：排好之后截，截下的那一份才是集合（[`LsHead::truncated`]）。
 */
limit?: number, 
/**
 * `true` ⇒ 只列文件夹（含指向目录的链接）；截断与计数都只数文件夹。
 */
dirs_only?: boolean, 
/**
 * 按哪一列排（缺 ⇒ 名称）。目录（含指向目录的链接）恒在前；截断在排好之后。
 */
sort?: LsSort, 
/**
 * 手上那一份的记号（上一次回包的 `held`，原样交回）：缺 ⇒ 整份。同一问法（目录 · 排法 · 只列文件夹 · 上限）才认。
 */
held?: SyncHeld, 
/**
 * 只要排好的那一份里的一窗；缺 ⇒ 整份（至多 `limit` 条）。
 */
range?: SyncRange, };
type LsSort = { by: LsSortBy, reverse?: boolean, };
type LsSortBy = "name" | "size" | "mtime" | "type";
type PreviewGate = { "text": TextPreview } | { "image": number } | { "no": string };
type SyncHead<H> = { total: number, info: H, };
type SyncHeld = string;
type SyncPatch<H, T> = { held: SyncHeld, whole: boolean, same: boolean, order: Array<string> | null, up: { [key in string]: T }, head: SyncHead<H> | null, };
type SyncRange = { from: number, limit: number, };
type TextPreview = { cap: number, lang?: string, };
type WirePath = string | { b16: string };
```

码：`bad_args` · `bad_path` · `denied` · `not_dir` · `not_found` · `unreadable`

#### `files-stat`

一个路径的元数据。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --files-stat`

入参 `StatReq` · 回包 `StatReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileKind = "dir" | "file" | "symlink" | "other";
type StatReply = { path: WirePath, 
/**
 * 上一级（那台按自己的路径规则切）；到顶 ⇒ `null`。
 */
up: WirePath | null, kind: FileKind, size: number, readonly: boolean, 
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

#### `bookmarks-read`

这台的书签。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bookmarks-read`

入参 `BookmarksReadReq` · 回包 `Bookmarks`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Bookmarks = { dirs: Array<string>, 
/**
 * 入参 `at` 里的每一串（原样作键）⇒ 它按归一之后的形在 `dirs` 里的下标；不在的不列，没问 ⇒ 空。
 */
here?: { [key in string]: number }, };
type BookmarksReadReq = { 
/**
 * 各栏此刻的目录（这台的绝对路径）：回包的 `here` 答每一个是第几条。
 */
at?: Array<string>, };
```

码：`bad_args` · `io_failed` · `bookmarks_unreadable`

#### `bookmarks-edit`

切换 · 删 · 加几条书签，回改完的整份。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bookmarks-edit`

入参 `BookmarksEditReq` · 回包 `Bookmarks`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Bookmarks = { dirs: Array<string>, 
/**
 * 入参 `at` 里的每一串（原样作键）⇒ 它按归一之后的形在 `dirs` 里的下标；不在的不列，没问 ⇒ 空。
 */
here?: { [key in string]: number }, };
type BookmarksEditReq = { op: "toggle" | "remove" | "add", 
/**
 * 那几个目录（这台的绝对路径；按归一之后的形比、存）。
 */
dirs: Array<string>, 
/**
 * 各栏此刻的目录：回包的 `here` 答每一个在改完的那一份里是第几条。
 */
at?: Array<string>, };
```

码：`bad_args` · `io_failed` · `bookmarks_unreadable`

### 4.3 账号

#### `quota-read`

这台的额度账。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --quota-read`

入参 `QuotaReadReq` · 回包 `QuotaRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type LoginState = "ok" | "needsLogin" | "needsKey";
type QuotaAccount = { rows: Array<Array<QuotaCell>>, 
/**
 * 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）；按量号 ⇒ `null`。
 */
fiveHour: Words | null, warm: QuotaWarm, agent: AgentKind, account: string, seenAt: number, seenAtText: Words, 
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
 * 这个号用量那一格（状态栏按钮 · 切号下拉 · 「下一个」那一格同一份）：`usage_of` 写。
 */
usage: Usage, };
type QuotaCandidate = { agent: AgentKind, account: string, kind: QuotaKind, };
type QuotaCell = { text: Words, tone: Tone, };
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
 * 每个出过数的号一条，按 `(agent, account)` 排（不按能不能发重排：额度一变就重读，按状态排的话卡片会跳位）。
 */
accounts: Array<QuotaAccount>, 
/**
 * 账号库里有、额度账上从没出过数的号。
 */
unseen: Array<QuotaUnseen>, 
/**
 * 换号 · 加号那几处的候选：出过数的在前、没出过的在后，`_` 不在里面（`is_candidate`）。界面只照它画，不另拼、不另滤。
 */
candidates: Array<QuotaCandidate>, 
/**
 * 读不出 / 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`（`quota_rows::head_text`）。
 */
text: Words | null, 
/**
 * 读不出 ⇒「5h 那一格」写好的字（`5h 读不到`，`quota_rows::head_five_hour`）；否则 `null`（每号自己那一格）。
 */
fiveHour: Words | null, 
/**
 * 号名 / 位名表（出口画号名 · 位名只照它）。
 */
names: QuotaNames, 
/**
 * 「现在还能不能发」那两行写好的字（用量页的结论 · 全满时那一句）；读不出 / 一个号都没有 ⇒ `null`（那时看 `text`）。
 */
verdict: QuotaVerdict | null, };
type QuotaReadReq = Record<symbol, never>;
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
fiveHour: Words | null, warm: QuotaWarm, agent: AgentKind, account: string, kind: QuotaKind, login: LoginState, subId?: string, 
/**
 * 用量那一格（没出过数 ⇒ `5h —` / `按量`；同出过数的号那一格一个写法，只是没有重置那一格）。
 */
usage: QuotaUnseenUsage, };
type QuotaUnseenUsage = { slot?: string, window?: Words, value: Words, text: Words, tone: "plain" | "fail" | "warn", };
type QuotaVerdict = { text: string, sub: string | null, tone: "plain" | "fail" | "warn", };
type QuotaWarm = { act: QuotaWarmAct, at?: number, 
/**
 * `at` 写给人看的样子（`Warm::stamp`）。
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
 * `resets_at` 写给人看的样子（出口那一下按钟写，`SlotShow::stamp`；内部与记账一律不填）。
 */
resetsAtText?: string, 
/**
 * `resets_at` 距今（`+1h50m`；同上，只在还没到时有）。
 */
resetsAtRelText?: string, 
/**
 * 重置那一格写好的字（`↻18:30` · 已过 `↻13:10 已过`；同 `rows` 里那一格）；说不出几点 ⇒ 缺。
 */
resetText?: string, 
/**
 * 用满：这个窗口用到 100%、还没重置（画 `✕`；被拒而没用满画「{pct}% · 被拒」）。没用满 ⇒ 缺。
 */
full?: boolean, 
/**
 * 这一格的字（唯一写法 `slot_words`：超额在兜 `超额` · 上一窗已过 / 没数 `—` · 用满 `✕` · 被拒没用满 `58% · 被拒` · 否则 `58%`）。
 */
text: string, 
/**
 * 那一格的语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 卡着的窗口快满 ⇒ `warn`；否则 `plain`。
 */
tone: "plain" | "fail" | "warn", 
/**
 * 这一格那个窗口「1% ≈ 多少用量」写好的字（`1% ≈ 2.1M`，同 [`WindowShow::per_pct_text`]）；量不到 ⇒ 缺。
 */
perPctText?: string, };
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
 * 被拒时几点重置（`↻19:00`，出口那一下按钟写，`QuotaShow::stamp`）；没有 ⇒ 缺。
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
 * `resets_at` 写给人看的样子（出口那一下按钟写，`WindowShow::stamp`；内部与记账一律不填）。
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
 * 这个号这个窗口「1% ≈ 多少用量」（折合输入 token：各类 token 按那一家的价比折）：中转数到的每发用量 ÷ 读数涨的点数；
 * 量不到（读数没动 · 样本太少 · 只有一次读数 · 量出来超过 7 天）⇒ 缺。
 */
tokensPerPct?: number, 
/**
 * `tokens_per_pct` 写好的字（`1% ≈ 2.1M`）；量不到 ⇒ 缺。
 */
perPctText?: string, 
/**
 * 重置时刻已过、之后没再看到：上次的数不再作数（用量当 0、窗口没开）。
 */
resetSinceSeen?: boolean, 
/**
 * 这一窗口的语气（`rows` 里更细窗口那一行的值照它上色）：用满 ⇒ `fail`；快满 ⇒ `warn`；否则 `plain`。
 */
tone: "plain" | "fail" | "warn", };
type Words = string;
```

码：`bad_args`

#### `quota-probe`

用某个号查一次额度。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 32 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --quota-probe`

入参 `QuotaProbeReq` · 回包 `Probed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type ProbeFrom = "usage";
type ProbeState = "read" | "unreadable";
type Probed = { 
/**
 * 哪一家（同入参，线上的 kind）。
 */
agent: AgentKind, 
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
type QuotaProbeReq = { 
/**
 * 哪一家（线上的 kind）。
 */
agent: AgentKind, 
/**
 * 账号库里的号（配置目录末段；账号 0 是 `0`）。
 */
account: string, };
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

#### `quota-warm`

给一个号开窗。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 125 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --quota-warm`

入参 `QuotaWarmReq` · 回包 `Warmed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type QuotaWarmReq = { 
/**
 * 哪一家（线上的 kind）。
 */
agent: AgentKind, 
/**
 * 账号库里的号（配置目录末段；账号 0 是 `0`）。
 */
account: string, };
type Warmed = { 
/**
 * 哪一家（同入参）。
 */
agent: AgentKind, 
/**
 * 号（同入参）。
 */
account: string, 
/**
 * 这一趟真发了（`false` ＝ 15 分钟内发过，照那一次答）。
 */
sent: boolean, 
/**
 * 那一发发出的时刻（unix 秒）。
 */
at: number, 
/**
 * 回来的重置时刻（额度账里那个号的 5 小时窗；没读到 ⇒ `null`）。
 */
resets_at: number | null, };
```

码：`bad_args` · `child_timed_out` · `failed` · `io_failed` · `not_found` · `unsupported`

#### `rotation-facts`

每个号此刻的事实。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-facts`

入参 `RotationFactsReq` · 回包 `RotationFactsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Fact = { 
/**
 * 路由第 2 段。
 */
account: string, kind: QuotaKind, w5h: WinFact, w7d: WinFact, 
/**
 * 有周上限：读数里有周用量那一格（没有 · 没出过数 ⇒ 缺）。
 */
weekly?: boolean, login: LoginState, 
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
 * 哪一家（线上的 kind）；缺 ⇒ 有账号库的那一家。
 */
agent?: AgentKind, };
type WinFact = { open: boolean, used: number, resets?: number, 
/**
 * `resets` 写给人看的样子（按看的那一台的时区）。
 */
resetsText?: string, rate?: number, 
/**
 * 这一窗「1% ≈ 多少用量」（折合输入 token；同额度账那一格）：量不到 ⇒ 缺。
 */
tokensPerPct?: number, };
```

码：`bad_args`

#### `session-account`

一条会话的号。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-account`

入参 `SessionAccountReq` · 回包 `SessionAccountReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type SessionAccountReply = { 
/**
 * 号名（头像按它取色；`_` ＝ 没指定号）；不知道 ⇒ `null`。
 */
account: string | null, 
/**
 * 按钮上的名字（号名表里的叫法；不知道 ⇒ 那一句「未实时显示」）。
 */
name: string, 
/**
 * 生成的类型里就地写开（不另出一份文件）。
 */
when: "now" | "last" | "unknown", 
/**
 * 轮换换过号 ⇒ 开会话的那个号（按钮前那个 ⇄ · 悬停卡末行）；没换过 ⇒ `null`。
 */
from: string | null, 
/**
 * 标签页悬停卡那一行「账号 X」；号不知道 ⇒ `null`。
 */
accountLine: string | null, 
/**
 * 标签页悬停卡那一行「从 X 换过来」；没换过 ⇒ `null`。
 */
fromLine: string | null, 
/**
 * 用量那几格（额度账里在用那个号那一条）；没走中转 · 账上没有 ⇒ `null`（只画名字）。
 */
usage: Usage | null, 
/**
 * 那一条最后一次看到距今超过 30 分钟。
 */
stale: boolean, 
/**
 * 悬停卡的行，一格一个字。
 */
hover: Array<Array<string>>, 
/**
 * 悬停卡里采样那一格在第几行：出口在那一行后面接上问的是哪台（那台叫什么是出口那一侧的事）；没有 ⇒ `null`。
 */
sampled: number | null, 
/**
 * 这个号与那台的默认号都确知、而且不一样。
 */
differs: boolean, badge: { 
/**
 * 画成「上次的」（幽灵头像）：不是此刻在跑的那个号。
 */
ghost: boolean, 
/**
 * 悬停那一句。
 */
title: string, } | null, 
/**
 * 那台的号查得了（账号库启用了）。
 */
queryable: boolean, 
/**
 * 上游是自设的（用户自己的网关）⇒ 写好的那一句（额度看不到 · 换号不适用）：这时不画用量、不出徽章，面板照它出一条提示。不是 ⇒ 不上线。
 */
ownUpstream?: string, };
type SessionAccountReq = { sid: string, };
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
 * 被拒时几点重置（`↻19:00`，出口那一下按钟写，`QuotaShow::stamp`）；没有 ⇒ 缺。
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
```

码：`bad_args`

#### `rotation-try`

试跑一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-try`

入参 `RotationTryReq` · 回包 `RotationTryReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type RotTryLane = { account: string, segs: Array<RotTrySeg>, };
type RotTryRow = { account: string, why: string, };
type RotTrySeg = { from: number, to: number, sendable: boolean, reset: boolean, text: string, };
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
capUnknown: boolean, skipped: Array<string>, 
/**
 * 这一段写的字：号名 · 停发那一句。
 */
text: string, };
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
agent?: AgentKind, };
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
description: string, 
/**
 * 说明的第一行（下拉里那行灰字；没有 ⇒ 空）。
 */
summary: string, rev: string, error?: RotStrategyError, 
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
tzUnknown: boolean, 
/**
 * 不是照编辑器那一份存出来的（代码或声明跟它此刻的不一样）。
 */
hand: boolean, };
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
type FactBlocked = { why: FactWhy, until?: number, text: string, };
type FactWhy = "refused" | "overage" | "used" | "login" | "key";
type LoginState = "ok" | "needsLogin" | "needsKey";
type QuotaKind = "sub" | "api";
type RotAccount = { plate: RotPlate, full: Array<string>, 
/**
 * 路由第 2 段。
 */
account: string, kind: QuotaKind, w5h: WinFact, w7d: WinFact, 
/**
 * 有周上限：读数里有周用量那一格（没有 · 没出过数 ⇒ 缺）。
 */
weekly?: boolean, login: LoginState, 
/**
 * 额度账上最后一次看到它（没出过数 ⇒ 缺）。
 */
seenAt?: number, 
/**
 * 事实上用不了 ⇒ 有；能用 ⇒ 缺。
 */
blocked?: FactBlocked, };
type RotBoard = { all: Array<RotCell>, rows: Array<RotRow>, };
type RotCan = { reorder: boolean, off: boolean, edit: boolean, };
type RotCell = { key: string, kind: RotFormKind, 
/**
 * 24 小时条那一格是哪一窗（`5h` · `7d`）。
 */
window?: string, 
/**
 * 标签（24 小时条那一格带窗名）。
 */
label: string, help?: string, 
/**
 * 此刻生效的值（`null` ＝ 可空的那一格选了空的那一项；24 小时条 ⇒ `null`，段在 `segs`）。
 */
v: unknown, 
/**
 * 数格框里写的数（值是空的那一项 ⇒ 声明默认 · 下限 · 0 里头一个是数的）。
 */
num?: number, from: RotFrom, 
/**
 * 能是空（`null`）：空的那一项怎么叫。
 */
nullable?: string, choices?: Array<RotChoice>, min?: number, max?: number, segs?: Array<RotDaySeg>, 
/**
 * 数格前面那半句。
 */
lead?: string, 
/**
 * 这一格只在那一格取某值时出：挂在它下面（那一格的 `key`）。
 */
under?: string, 
/**
 * 分段后面那几窗：能「同第一窗」，那颗按钮的字（「同 5h」）。
 */
same?: string, };
type RotChoice = { v: string, label: string, };
type RotDaySeg = { from: number, to: number, fromText: string, toText: string, v: number, text: string, tip: string, };
type RotFormKind = "percent" | "minutes" | "toggle" | "choice" | "daySegments";
type RotFrom = "own" | "all" | "default" | "same";
type RotGroup = "use" | "next" | "out" | "off";
type RotHint = { "k": "hold", until?: number, account?: string, 
/**
 * `until` 的钟面（`HH:MM`，看的那一台的时区；不在今天带日期）。
 */
untilText?: string, } | { "k": "strategyError", error: RotStrategyError, path?: string, } | { "k": "tzUnknown", zone: string, } | { "k": "oldScripts", n: number, files: Array<RotOldFile>, } | { "k": "newer" } | { "k": "older", n: number, } | { "k": "wordsBad" } | { "k": "bookUnreadable", text: string, };
type RotLine = { account: string, group: RotGroup, state: string, why?: string, movable: boolean, stay?: string, };
type RotLogCause = { "k": "order" } | { "k": "unusable" } | { "k": "temp" } | { "k": "useNow" } | { "k": "hold", until?: number, } | { "k": "error" };
type RotLogRow = { because: string, at: number, from: string, to: string, cause: RotLogCause, why?: string, };
type RotMark = { glyph?: string, dashed: boolean, tip: string, };
type RotNote = { hint: RotHint, tone: Tone, text: string, 
/**
 * 复制详情（出错那一条带原话；别的没有）。
 */
detail?: string, };
type RotOldFile = { name: string, path: string, };
type RotPlate = { color: number, label: Words, text: Words, };
type RotRow = { account: string, 
/**
 * 这一行在 `settings.accounts` 里的下标：改设置 · 移出 · 拖动一律交它（不拿显示的先后当行号）。
 */
row: number, line: string, hover: string, cells: Array<RotCell>, rules: Array<string>, marks: Array<RotMark>, };
type RotStrategyError = { line?: number, msg: string, };
type RotStrategyItem = { id: string, 
/**
 * 说明（文件开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段；读不懂 ⇒ 空）。标题就是 `id`（文件名，§2.1）。
 */
description: string, 
/**
 * 说明的第一行（下拉里那行灰字；没有 ⇒ 空）。
 */
summary: string, rev: string, error?: RotStrategyError, 
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
tzUnknown: boolean, 
/**
 * 不是照编辑器那一份存出来的（代码或声明跟它此刻的不一样）。
 */
hand: boolean, };
type RotTarget = { sid: string, } | { strategy: string, };
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
type RotTryLane = { account: string, segs: Array<RotTrySeg>, };
type RotTryRow = { account: string, why: string, };
type RotTrySeg = { from: number, to: number, sendable: boolean, reset: boolean, text: string, };
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
capUnknown: boolean, skipped: Array<string>, 
/**
 * 这一段写的字：号名 · 停发那一句。
 */
text: string, };
type RotationViewReply = { now: number, 
/**
 * `now` 的钟面（`HH:MM`，看的那一台的时区）。
 */
nowText: string, 
/**
 * 会话那一形：这条会话叫什么（同历史清单那一格：改过的 ＞ 标题 ＞ 第一句；这台记录树里找不到 ⇒ 会话 id 前 8 位）；设置页 ⇒ `null`。
 */
title: string | null, 
/**
 * 用的那份策略；会话不轮换（没选策略）⇒ `null`，这时 `decision` 也是 `null`、只用开会话的号。
 */
strategy: RotStrategyItem | null, 
/**
 * 设置那几格照声明解好（此刻的值 · 从哪来 · 字）：会话 ⇒ 它那份副本里的；设置页 ⇒ 文件此刻的。
 */
board: RotBoard, 
/**
 * 开会话用的号 · 此刻在用的号。
 */
start: string, current: string, 
/**
 * 会话那一形的顺序（设置页 ⇒ 空）：第一格 ＝ 在用的号，往下是接下来先用谁（拖出来的 · 策略交的先后），此刻用不了的与先不用的沉底。
 */
lineup: Array<RotLine>, 
/**
 * 会话那一形：这个会话有拖出来的顺序（「照策略排」清掉它）。
 */
manual: boolean, 
/**
 * 这个会话先不用的号。
 */
off: Array<string>, 
/**
 * 每个号的事实 ＋ 号牌，照号列表的先后（列表里没有的接在后面）。
 */
accounts: Array<RotAccount>, 
/**
 * 号列表里有几个（`accounts` 前这么多个是列表里的）。
 */
listed: number, decision: RotTryDecision | null, 
/**
 * 推演（设置页那一形才有：改一格就看得到接下来怎么换；会话那一形 · 不轮换 ⇒ 缺）。
 */
timeline?: RotTryTimeline, 
/**
 * 换号记录（会话才有）。
 */
log: Array<RotLogRow>, hints: Array<RotNote>, 
/**
 * 会话那一形此刻能做的几样（设置页 ⇒ 缺）。
 */
can?: RotCan, };
type RotationViewReq = { target: RotTarget, 
/**
 * 设置页那一形往后推几小时（1–48，缺 ⇒ 12）；会话那一形不画推演，不看它。
 */
hours?: number, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WinFact = { open: boolean, used: number, resets?: number, 
/**
 * `resets` 写给人看的样子（按看的那一台的时区）。
 */
resetsText?: string, rate?: number, 
/**
 * 这一窗「1% ≈ 多少用量」（折合输入 token；同额度账那一格）：量不到 ⇒ 缺。
 */
tokensPerPct?: number, };
type Words = string;
```

码：`bad_args` · `no_session` · `no_strategy`

#### `rotation-settings`

改一份策略的设置。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-settings`

入参 `RotationSettingsReq` · 回包 `RotStrategyRev`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotBadCell = { cell: string, code: string, };
type RotEdit = { "set": { key: string, row?: number, v: unknown, } } | { "seg": { key: string, row?: number, window: string, at?: number, seg?: RotSeg, } } | { "same": { key: string, row?: number, window: string, same: boolean, } } | { "move": { from: number, to: number, } } | { "add": { account: string, } } | { "remove": { row: number, } };
type RotRefused = { cells: Array<RotBadCell>, };
type RotSeg = { from: string, to: string, v: number, };
type RotStale = { rev: string, };
type RotStrategyRev = { rev: string, };
type RotationSettingsReq = { strategy: string, edit: RotEdit, ifRev: string, };
```

码：`bad_args` · `no_strategy` · `refused` · `stale` · `io_failed`

失败带 `data` 的码：`refused` ⇒ `RotRefused` · `stale` ⇒ `RotStale`

#### `rotation-use`

会话载入一份策略。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-use`

入参 `RotationUseReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotationUseReq = { sid: string, 
/**
 * `null` ⇒ 这个会话不再轮换（只用开会话的号）。
 */
strategy: string | null, };
```

码：`bad_args` · `no_session` · `no_strategy` · `io_failed`

#### `rotation-reload`

在用旧版的会话都载入新版。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-reload`

入参 `RotationReloadReq` · 回包 `RotDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RotDone = Record<symbol, never>;
type RotationReloadReq = { strategy: string, };
```

码：`bad_args` · `no_strategy` · `io_failed`

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

会话侧的临时动作。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --rotation-temp`

入参 `RotationTempReq` · 回包 `RotationTempReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FactWhy = "refused" | "overage" | "used" | "login" | "key";
type RotTempOp = { "move": { account: string, to: number, } } | "clear" | { "off": { account: string, off: boolean, } };
type RotUnusable = { why?: FactWhy, until?: number, };
type RotationTempReply = { temp: Array<string>, off: Array<string>, };
type RotationTempReq = { sid: string, op: RotTempOp, };
```

码：`bad_args` · `no_session` · `not_listed` · `unusable` · `io_failed`

失败带 `data` 的码：`unusable` ⇒ `RotUnusable`

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
 * 一份策略文件的原文（说明 ＋ 头 ＋ 代码）：从文件添加；缺 ⇒ 新建（照编辑器那一份，号列表 ＝ 账号库全部号，时区 ＝ 这台的）。
 */
source?: string, 
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
type RotStale = { rev: string, };
type RotStrategyId = { id: string, };
type RotationStrategyRenameReq = { id: string, name: string, ifRev: string, };
```

码：`bad_args` · `no_strategy` · `stale` · `io_failed`

失败带 `data` 的码：`stale` ⇒ `RotStale`

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
type RotStale = { rev: string, };
type RotStrategyRev = { rev: string, };
type RotationStrategyTzReq = { id: string, tz: string, ifRev: string, };
```

码：`bad_args` · `no_strategy` · `stale` · `io_failed`

失败带 `data` 的码：`stale` ⇒ `RotStale`

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
 * `configDir` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）；账号 0 ⇒ `null`。
 */
configDirShown: string | null, 
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
badge: Badge, 
/**
 * 号牌上那一两个字（号色那一格里画的，唯一写法在后端）。
 */
plate: Words, 
/**
 * 这个号「5h 那一格」写好的字（同 `quota-read` 那一格：账读不出 ⇒ `5h 读不到`）；没出过数 · 按量号 ⇒ `null`。
 */
quota: Words | null, 
/**
 * 选不了（`selectable: false`）的那一句：需登录 · 没配 key · 不隔离 · 号目录不在；选得了 ⇒ `null`。
 * 账号 0 不经选号（起它走「不指定号」那一条）⇒ 恒 `null`。
 */
unselectable: Words | null, 
/**
 * 给不给［登录…］：需登录、而这一家能在 app 里登录 ⇒ `true`（需登录却登不了的，`unselectable` 那一句说为什么）。
 */
login: boolean, 
/**
 * 进不进新建会话的选号表：隔离、有配置目录 ⇒ `true`（需登录的照样列、灰着，选不选得了看 `selectable`）；
 * 账号 0（不经选号）· 原地模式 ⇒ `false`。两个出口照它列，不自己判。
 */
newSession: boolean, name: string, 
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
 * 这次起会话的是哪一家（线上的 kind，同 `session-open-facts` 给的那一格；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）。
 */
agent: AgentKind, };
type AgentKind = string;
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
effectiveDefault: string | null, };
type Words = string;
```

码：`bad_args` · `too_large`

#### `machine-interrupts`

停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前，会打断什么。

收 `args` · 可撤 · CLI：`ccm -- --machine-interrupts`

入参 `MachineInterruptsReq` · 回包 `MachineInterruptsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineInterruptsReply = { relayedSessions: number | null, relayedMaybe: number, liveStreams: number, forwards: number, };
type MachineInterruptsReq = { 
/**
 * 停的是哪台：别的台 ⇒ 这台后端经那台的常驻流问它自己那一份（会话三格）、再数自己账上通往它的转发；缺 · 本机 ⇒ 只答这台。
 */
machine?: string | null, 
/**
 * `true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零。
 */
appExit?: boolean | null, 
/**
 * 带 `appExit` 时一并问的那几台（各照它自己的退出行为答，答到的加起来；问不到的不算）。问到那几台时不带它（不往下扇）。
 */
machines?: Array<Origin> | null, };
type Origin = string;
```

码：`bad_args`

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

入参 `LoginStartArgs` · 回包 `LoginStarted`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type LoginAgain = { again: LoginAgainHow, terminal: boolean, };
type LoginAgainHow = "paste" | "restart" | "none";
type LoginStartArgs = { 
/**
 * 打开登录链接的浏览器在后端这台上（桌面连本机）：登录页回跳到这台的本机端口，那一家自己登完。
 * 缺 ⇒ 按「不在」答（说「会自己登完」却登不完是假话，反过来只是多留一个粘的框）。
 */
browserHere?: boolean, } & ({ "kind": "account", agent: AgentKind, account: string, } | { "kind": "mcp", agent: AgentKind, account: string | null, server: string, projectDir: string | null, });
type LoginStarted = { job: string, url: string, paste: PasteKind, auto: boolean, 
/**
 * 那一家自己在这台开了登录页（它那一形自己开浏览器、发起它的浏览器又在这台）⇒ 界面不再开第二页。
 */
opened: boolean, };
type PasteKind = "code" | "redirectUrl";
```

码：`bad_args` · `not_found` · `unsupported` · `failed`

失败带 `data` 的码：`not_found` ⇒ `LoginAgain` · `unsupported` ⇒ `LoginAgain` · `failed` ⇒ `LoginAgain`

#### `login-finish`

把粘回来的那一段（登录页上的码 · 浏览器回跳的地址）交给那份登录作业，等那一家换完。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `LoginFinishReq` · 回包 `LoginDone`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type LoginAgain = { again: LoginAgainHow, terminal: boolean, };
type LoginAgainHow = "paste" | "restart" | "none";
type LoginDone = { target: LoginStartReq, };
type LoginFinishReq = { job: string, 
/**
 * 登录页上给的那一段（原样；不进日志、不回显）。
 */
pasted: string, };
type LoginStartReq = { "kind": "account", agent: AgentKind, account: string, } | { "kind": "mcp", agent: AgentKind, account: string | null, server: string, projectDir: string | null, };
```

码：`bad_args` · `bad_paste` · `refused` · `no_job` · `child_timed_out`

失败带 `data` 的码：`bad_paste` ⇒ `LoginAgain` · `refused` ⇒ `LoginAgain` · `no_job` ⇒ `LoginAgain` · `child_timed_out` ⇒ `LoginAgain`

#### `login-cancel`

收一份登录作业。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `LoginCancelReq` · 回包 `LoginCancelled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type LoginCancelReq = { job: string, };
type LoginCancelled = { was: boolean, };
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
type Annotated = { entry: Entry, 
/**
 * 改之前单个隐藏那一格（`on` · `off` · 没有 ⇒ `null`）：撤销「隐藏」照它交回去（`null` ⇒ `rule`），不把照规则显示的钉成单独显示。
 */
prev: Hide | null, 
/**
 * 改之前改过的名（没有 ⇒ `null`）：撤销改名照它交回去（`null` ⇒ 交空串，回到原标题）—— 标签页上只有定好的名，不知道这一格。
 */
prev_title: string | null, };
type AutoAnswer = { 
/**
 * 挂上的提问等满几分钟还没人答就替人答；补丁里给 0 ＝ 关（摘掉这一格）。
 */
afterMin: number, 
/**
 * 替人答的那一句；缺 ⇒ 后端那一句缺省的。
 */
say?: string, };
type Entry = { starred: boolean, customTitle: string | null, 
/**
 * 单个那一格：`on` 隐藏 · `off` 不隐藏 · 缺 ＝ 照规则。
 */
hide?: Hide, 
/**
 * 没人答的提问等几分钟替人答一句（判定住后端 `control::asks`）；缺 ＝ 关。
 */
autoAnswer?: AutoAnswer, 
/**
 * 终端没了时换的模型 / 权限档，下次恢复照它起（只由那两条换的命令写）；缺 ＝ 没记。
 */
resumeWith?: ResumeWith, 
/**
 * 这条会话在哪几组（组 id；那一组的名字等抄在这一台注解文件 `groups` 那一节，每台只记自己有组员的那几组）。一条会话可在几组；空 ＝ 不在组里。
 */
groups?: Array<string>, 
/**
 * 固定（跟着组一起跨端）；缺 ＝ 没固定。
 */
pinned?: boolean, updatedAt: number, };
type Hide = "on" | "off";
type HideAsk = "on" | "off" | "rule";
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
 * 单个那一格：`on` · `off` · `rule`（摘掉、回到规则）。
 */
hide?: HideAsk | null, 
/**
 * 没人答时自动答：`{afterMin, say?}`，`afterMin: 0` 摘掉（关）；没给 `say` ⇒ 留着原来那一句、空白 ⇒ 回到缺省；值怎么算数由 `history_annotations::auto_checked` 判。
 */
autoAnswer?: AutoAnswer | null, 
/**
 * 固定。
 */
pinned?: boolean | null, };
type ResumeWith = { model?: string, permissionMode?: string, end: number, };
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

#### `groups-here`

这台的组那一半：这台有组员的那几片 ＋ 带组或固定的会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --groups-here`

入参 `NoArgs` · 回包 `GroupsHere`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type GroupsHere = { groups: Array<HereGroup>, members: Array<HereMember>, };
type HereGroup = { id: string, name: string, made: number, named: number, };
type HereMember = { sid: string, groups: Array<string>, pinned: boolean, face: SessionFace, facts: MemberFacts, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type MemberFacts = { 
/**
 * 显示的标题（改过的 ＞ 标题 ＞ 第一句 ＞ 清单那一句「没名字」）。
 */
title: string, 
/**
 * 项目目录（清单里没有那一条 ⇒ 缺）。
 */
cwd: string | null, 
/**
 * 记录文件（resume 照它；清单里没有 ⇒ 缺）。
 */
record: string | null, 
/**
 * 上次跑在哪个号下（没记 ⇒ 缺）。
 */
account: string | null, 
/**
 * 后台会话。
 */
bg: boolean, 
/**
 * 最后活动（毫秒；清单里没有 ⇒ 缺）。
 */
at: number | null, };
type NoArgs = Record<symbol, never>;
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed`

#### `groups-read`

问到各台的组那一半、并成一份。

收 `args` · 可撤 · CLI：`ccm -- --groups-read`

入参 `GroupsReadReq` · 回包 `GroupsRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type GroupCount = { dot: SessionDot, n: number, word: string, 
/**
 * 组头那一句（「需手动 2」那样，出口照摆）。
 */
text: string, };
type GroupKnown = { "is": "now" } | { "is": "before", at: string, } | { "is": "unknown" };
type GroupMember = { origin: string, sid: string, pinned: boolean, face: SessionFace, facts: MemberFacts, };
type GroupRead = { id: string, 
/**
 * 名字：各片里 `named` 最新的那一份（哪一片都没有记录 ⇒ 缺）。
 */
name: string | null, 
/**
 * 叫它什么（出口照摆）：有名字 ⇒ 名字；没有 ⇒ 照实说不知道。
 */
title: string, 
/**
 * 各片里最早的那个建组时刻（没有记录 ⇒ 缺）。
 */
made: number | null, 
/**
 * 取到的那个名字几时起的（没有记录 ⇒ 缺）：进组时同名字一起抄进组员那一台。
 */
named: number | null, known: GroupKnown, 
/**
 * 组员：照问的台序、再照 sid。
 */
members: Array<GroupMember>, counts: Array<GroupCount>, };
type GroupsMachine = { origin: string, read: HistoryMachineRead, };
type GroupsRead = { 
/**
 * 各组：照建的先后、再照 id。
 */
groups: Array<GroupRead>, 
/**
 * 各台固定的会话（不论在不在组里），同组员那一形。
 */
pinned: Array<GroupMember>, 
/**
 * 问的每一台这一问的结局（照问的先后）。
 */
machines: Array<GroupsMachine>, };
type GroupsReadReq = { machines: Array<string>, };
type HistoryMachineRead = { "is": "ok", notice: string | null, } | { "is": "stale", why: string, at: string, } | { "is": "failed", why: string, } | { "is": "unread" };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type MemberFacts = { 
/**
 * 显示的标题（改过的 ＞ 标题 ＞ 第一句 ＞ 清单那一句「没名字」）。
 */
title: string, 
/**
 * 项目目录（清单里没有那一条 ⇒ 缺）。
 */
cwd: string | null, 
/**
 * 记录文件（resume 照它；清单里没有 ⇒ 缺）。
 */
record: string | null, 
/**
 * 上次跑在哪个号下（没记 ⇒ 缺）。
 */
account: string | null, 
/**
 * 后台会话。
 */
bg: boolean, 
/**
 * 最后活动（毫秒；清单里没有 ⇒ 缺）。
 */
at: number | null, };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args`

#### `groups-write`

建组 · 进出组 · 改名与解散扇出给各片。

收 `args` · 可撤 · CLI：`ccm -- --groups-write`

入参 `GroupsWriteReq` · 回包 `GroupsWritten`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type GroupMissed = { origin: string, why: string, };
type GroupsWriteReq = { "op": "make", name: string, } | { "op": "move", sid: string, join?: HereGroup | null, leave?: string | null, } | { "op": "rename", id: string, name: string, machines: Array<string>, } | { "op": "dissolve", id: string, machines: Array<string>, };
type GroupsWritten = { "is": "made" } & HereGroup | { "is": "moved", sid: string, groups: Array<string>, } | { "is": "sent", id: string, missed: Array<GroupMissed>, };
type HereGroup = { id: string, name: string, made: number, named: number, };
```

码：`bad_args` · `annotations_unreadable` · `io_failed` · `no_annotations`

#### `session-self`

我是哪条会话 · 在哪几组 · 组员有谁。

收 `args` · 可撤 · CLI：`ccm -- --session-self`

入参 `NoArgs` · 回包 `SessionSelf`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type GroupCount = { dot: SessionDot, n: number, word: string, 
/**
 * 组头那一句（「需手动 2」那样，出口照摆）。
 */
text: string, };
type GroupKnown = { "is": "now" } | { "is": "before", at: string, } | { "is": "unknown" };
type GroupMember = { origin: string, sid: string, pinned: boolean, face: SessionFace, facts: MemberFacts, };
type GroupRead = { id: string, 
/**
 * 名字：各片里 `named` 最新的那一份（哪一片都没有记录 ⇒ 缺）。
 */
name: string | null, 
/**
 * 叫它什么（出口照摆）：有名字 ⇒ 名字；没有 ⇒ 照实说不知道。
 */
title: string, 
/**
 * 各片里最早的那个建组时刻（没有记录 ⇒ 缺）。
 */
made: number | null, 
/**
 * 取到的那个名字几时起的（没有记录 ⇒ 缺）：进组时同名字一起抄进组员那一台。
 */
named: number | null, known: GroupKnown, 
/**
 * 组员：照问的台序、再照 sid。
 */
members: Array<GroupMember>, counts: Array<GroupCount>, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type Looked = "every" | "this_machine" | "nothing";
type MemberFacts = { 
/**
 * 显示的标题（改过的 ＞ 标题 ＞ 第一句 ＞ 清单那一句「没名字」）。
 */
title: string, 
/**
 * 项目目录（清单里没有那一条 ⇒ 缺）。
 */
cwd: string | null, 
/**
 * 记录文件（resume 照它；清单里没有 ⇒ 缺）。
 */
record: string | null, 
/**
 * 上次跑在哪个号下（没记 ⇒ 缺）。
 */
account: string | null, 
/**
 * 后台会话。
 */
bg: boolean, 
/**
 * 最后活动（毫秒；清单里没有 ⇒ 缺）。
 */
at: number | null, };
type NoArgs = Record<symbol, never>;
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type SessionSelf = { 
/**
 * 我是哪条会话（这台的）。
 */
sid: string, 
/**
 * 我在的那几组（组员整组）。
 */
groups: Array<GroupRead>, looked: Looked, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `not_in_session`

#### `history-list`

历史页的平铺会话清单。

收 `args` · 可撤 · CLI：`ccm -- --history-list`

入参 `HistoryListReq` · 回包 `SyncPatch<HistoryHead, HistoryRow>`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
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
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type ForkedFrom = { label: string, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type HistoryGroup = { key: string, agent: AgentKind, projectName: string, projectPath: string, projectDir: string, count: number, 
/**
 * 有在跑的 ⇒ `true`；有答不了的 ⇒ `null`；都没在跑 ⇒ `false`。
 */
hasLive: boolean | null, starred: boolean, lastActivity: number, 
/**
 * `last_activity` 写好的字（同行尾那一格的写法）；读不了的那一组 ⇒ `null`。
 */
atText: string | null, 
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
type HistoryHead = { groups: Array<HistoryGroup>, 
/**
 * 注解没并上的那一句；并上了 ⇒ `null`。
 */
notice: string | null, };
type HistoryListReq = { 
/**
 * 那台的名字（行与组都标上它）。缺 ⇒ 这台。问那一台要带期限（请求信封 `within_ms`）：等多久归发起方，没带 ⇒ `bad_args`。
 */
origin?: string, 
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
 * 上次这一问答的记号（原样交回）：对得上 ⇒ 没变 · 只给变了的那几行；缺 · 对不上（那台重启过 · 换了问法）⇒ 整份。
 */
held?: SyncHeld, 
/**
 * 只要排好的那一窗（`limit` 1–20000）；缺 ⇒ 头 2000 行。总数在回包的头里。
 */
range?: SyncRange, };
type HistoryRow = { agent: AgentKind, sessionId: string, projectPath: string, projectName: string, aiTitle: string | null, firstUserExcerpt: string, title: string, startedAt: number, updatedAt: number, jsonlPath: string, messageCountApprox: number, isBg: boolean, starred: boolean, customTitle: string | null, 
/**
 * 隐藏那一格（判定只住后端 `history/session_hidden.rs`）；不隐藏 ⇒ `null`（格恒在）。
 */
hidden: Hidden | null, forkedFromSessionId?: string, forkedFromMessageUuid?: string, 
/**
 * 分叉来的：父会话那一行（显示标题）；不是分叉来的 · 父会话不在这台的清单里 ⇒ 缺（界面写「分叉自其他会话」）。
 */
forkedFrom?: ForkedFrom, projectDir: string, group: string, lastAccount?: string, 
/**
 * 显示的标题：用户改过的 ＞ 标题 ＞ 第一句。
 */
label: Words, 
/**
 * 改过的标题 · 标题 · 第一句都没有（界面写「没有说过话的会话」）。
 */
untitled: boolean, 
/**
 * 那台此刻对这一行的说法（活着的照它的进程状态文件判在干什么；判不了活 ⇒ 状态不明）。
 */
face: SessionFace, can: Can, 
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
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type SyncHead<H> = { total: number, info: H, };
type SyncHeld = string;
type SyncPatch<H, T> = { held: SyncHeld, whole: boolean, same: boolean, order: Array<string> | null, up: { [key in string]: T }, head: SyncHead<H> | null, };
type SyncRange = { from: number, limit: number, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed` · `unreachable` · `resident_absent`

#### `history-browse`

历史页那一列（几台并成一列）。

收 `args` · 可撤 · CLI：`ccm -- --history-browse`

入参 `HistoryBrowseReq` · 回包 `SyncPatch<HistoryBrowseHead, HistoryLine>`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
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
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type ForkedFrom = { label: string, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type HistoryBrowseHead = { 
/**
 * 问到的每台（先后同入参：这台在前）。
 */
machines: Array<HistoryMachine>, 
/**
 * 几个项目 · 组里一共几个会话（按项目看时列表头那一句）。
 */
projects: number, grouped: number, 
/**
 * 按时间看的段头（按项目看 ⇒ 空）。
 */
sections: Array<HistorySection>, };
type HistoryBrowseReq = { 
/**
 * 带不带这台。
 */
here: boolean, 
/**
 * 远端那几台（并列时同一刻的行照这台 · 再照这里的先后）。
 */
remotes: Array<string>, 
/**
 * 先同步这几台的副本再答（开页 · 刷新 · 勾回一台；别的时候空）；须在 `remotes` 里。带了要带期限（请求信封 `within_ms`）：等多久归发起方。
 */
fresh: Array<string>, 
/**
 * 同 `history-list`。
 */
query?: string, sort?: HistorySort, within_days?: number, hidden?: boolean, 
/**
 * 怎么看。
 */
by: HistoryBy, 
/**
 * 按项目看时哪几组开着（搜着字时一律开着）。
 */
groups: HistoryFold, 
/**
 * 展开了分叉的那几行。
 */
forks: Array<HistoryRowId>, 
/**
 * 要哪一窗（`limit` 1–2000）。总数在回包的头里。
 */
range: SyncRange, };
type HistoryBy = "time" | "project";
type HistoryFold = { open: boolean, except: Array<HistoryGroupId>, };
type HistoryGroup = { key: string, agent: AgentKind, projectName: string, projectPath: string, projectDir: string, count: number, 
/**
 * 有在跑的 ⇒ `true`；有答不了的 ⇒ `null`；都没在跑 ⇒ `false`。
 */
hasLive: boolean | null, starred: boolean, lastActivity: number, 
/**
 * `last_activity` 写好的字（同行尾那一格的写法）；读不了的那一组 ⇒ `null`。
 */
atText: string | null, 
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
type HistoryGroupId = { 
/**
 * 哪台（这台 ⇒ 缺）。
 */
origin?: string, key: string, };
type HistoryLine = { "kind": "group", group: HistoryGroup, open: boolean, } | { "kind": "row", row: HistoryRow, child: boolean, forks: number, forksOpen: boolean, orphan: boolean, };
type HistoryMachine = { 
/**
 * 哪台（这台 ⇒ 缺）。
 */
origin?: string, 
/**
 * 那台筛好的几行（算上只为挂住分叉才带上的父会话）。
 */
total: number, read: HistoryMachineRead, };
type HistoryMachineRead = { "is": "ok", notice: string | null, } | { "is": "stale", why: string, at: string, } | { "is": "failed", why: string, } | { "is": "unread" };
type HistoryRow = { agent: AgentKind, sessionId: string, projectPath: string, projectName: string, aiTitle: string | null, firstUserExcerpt: string, title: string, startedAt: number, updatedAt: number, jsonlPath: string, messageCountApprox: number, isBg: boolean, starred: boolean, customTitle: string | null, 
/**
 * 隐藏那一格（判定只住后端 `history/session_hidden.rs`）；不隐藏 ⇒ `null`（格恒在）。
 */
hidden: Hidden | null, forkedFromSessionId?: string, forkedFromMessageUuid?: string, 
/**
 * 分叉来的：父会话那一行（显示标题）；不是分叉来的 · 父会话不在这台的清单里 ⇒ 缺（界面写「分叉自其他会话」）。
 */
forkedFrom?: ForkedFrom, projectDir: string, group: string, lastAccount?: string, 
/**
 * 显示的标题：用户改过的 ＞ 标题 ＞ 第一句。
 */
label: Words, 
/**
 * 改过的标题 · 标题 · 第一句都没有（界面写「没有说过话的会话」）。
 */
untitled: boolean, 
/**
 * 那台此刻对这一行的说法（活着的照它的进程状态文件判在干什么；判不了活 ⇒ 状态不明）。
 */
face: SessionFace, can: Can, 
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
type HistoryRowId = { 
/**
 * 哪台（这台 ⇒ 缺）。
 */
origin?: string, sid: string, };
type HistorySection = { at: number, text: string, };
type HistorySort = "activity" | "created";
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type SyncHead<H> = { total: number, info: H, };
type SyncHeld = string;
type SyncPatch<H, T> = { held: SyncHeld, whole: boolean, same: boolean, order: Array<string> | null, up: { [key in string]: T }, head: SyncHead<H> | null, };
type SyncRange = { from: number, limit: number, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed`

#### `history-listing`

这台的清单（按记号只给变了的）。

收 `args` · 可撤 · CLI：`ccm -- --history-listing`

入参 `HistoryListingReq` · 回包 `SyncPatch<HistoryListingHead, HistoryRawRow>`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type AutoAnswer = { 
/**
 * 挂上的提问等满几分钟还没人答就替人答；补丁里给 0 ＝ 关（摘掉这一格）。
 */
afterMin: number, 
/**
 * 替人答的那一句；缺 ⇒ 后端那一句缺省的。
 */
say?: string, };
type Entry = { starred: boolean, customTitle: string | null, 
/**
 * 单个那一格：`on` 隐藏 · `off` 不隐藏 · 缺 ＝ 照规则。
 */
hide?: Hide, 
/**
 * 没人答的提问等几分钟替人答一句（判定住后端 `control::asks`）；缺 ＝ 关。
 */
autoAnswer?: AutoAnswer, 
/**
 * 终端没了时换的模型 / 权限档，下次恢复照它起（只由那两条换的命令写）；缺 ＝ 没记。
 */
resumeWith?: ResumeWith, 
/**
 * 这条会话在哪几组（组 id；那一组的名字等抄在这一台注解文件 `groups` 那一节，每台只记自己有组员的那几组）。一条会话可在几组；空 ＝ 不在组里。
 */
groups?: Array<string>, 
/**
 * 固定（跟着组一起跨端）；缺 ＝ 没固定。
 */
pinned?: boolean, updatedAt: number, };
type Hide = "on" | "off";
type HistoryFailedDir = { projectDir: string, error: string, };
type HistoryListingHead = { failed: Array<HistoryFailedDir>, annotations: { [key in string]: Entry } | null, annotationsNotice: string | null, rules: Rules, };
type HistoryListingReq = { held: SyncHeld | null, };
type HistoryRawRow = { agent: AgentKind, sessionId: string, projectPath: string, projectName: string, aiTitle: string | null, firstUserExcerpt: string, 
/**
 * 显示标题（用户改过的另在 `customTitle`）：与全文搜索同一条规则。
 */
title: string, startedAt: number, updatedAt: number, jsonlPath: string, 
/**
 * 这台判的这条会话的一生（判不了：合成历史没有 pidfile ⇒ 说不清）。
 */
life: Life, messageCountApprox: number, isBg: boolean, starred: boolean, customTitle: string | null, 
/**
 * 起会话时给了 `--tab` 的那一格（那台的启动台账）；没给 ⇒ 缺。
 */
launchTab?: LaunchTab, 
/**
 * 主会话（那台的会话血缘）；没有 ⇒ 缺。
 */
parent?: string, forkedFromSessionId?: string, forkedFromMessageUuid?: string, 
/**
 * 读它那一组用的键（记录目录名 · 合成历史 `<kind>:<cwd>`）。
 */
projectDir: string, 
/**
 * 分组键：哪一家 ＋ 真实目录。
 */
group: string, lastAccount?: string, };
type LaunchTab = { tab: Tab, alias?: string, };
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type ResumeWith = { model?: string, permissionMode?: string, end: number, };
type Rules = { childTab: Tab, bgTab: Tab, };
type SyncHead<H> = { total: number, info: H, };
type SyncHeld = string;
type SyncPatch<H, T> = { held: SyncHeld, whole: boolean, same: boolean, order: Array<string> | null, up: { [key in string]: T }, head: SyncHead<H> | null, };
type Tab = "show" | "hide";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
```

码：`bad_args` · `failed`

#### `history-search`

全文搜索。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-search`

入参 `HistorySearchReq` · 回包 `HistorySearchReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
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
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type ForkedFrom = { label: string, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type HistorySearchReply = { 
/**
 * 每命中会话一行，最近优先。
 */
sessions: Array<SearchRow>, 
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
 * 全局片段上限（没给的照样记进 `moreHits`）。
 */
limit?: number, 
/**
 * 只比会话标题与第一句（不搜内容）。
 */
titles?: boolean, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
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
tool?: string, 
/**
 * 这条记录那一行在记录文件里的起点（字节；交给 `history-read` 的 `offset` 就从它读起）。
 */
offset: number, 
/**
 * 这条记录的行号（与实时行的 `seq` 同一个空间；打开查看器后按它物化、按 `uuid` 找卡）。
 */
line: number, };
type SearchRow = { 
/**
 * 行尾那一格。
 */
atText: string, 
/**
 * 内容头那一段。
 */
spanText: string, 
/**
 * 显示的名字：改过的名 ＞ 那台扫出的标题（同历史清单那一行的 `label`）。
 */
label: string, 
/**
 * 星标（那台的注解）。
 */
starred: boolean, 
/**
 * 隐藏那一格（那台判的，与历史清单那一行同一条规则）；不隐藏 ⇒ `null`。
 */
hidden: Hidden | null, 
/**
 * 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
 */
agent: AgentKind, sessionId: string, projectPath: string, projectName: string, jsonlPath: string, 
/**
 * 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
 */
title: string, 
/**
 * 记录文件最后改动的毫秒。
 */
updatedAt: number, hits: Array<SearchHit>, 
/**
 * `hits` 之外这个会话还有几处命中。
 */
moreHits: number, 
/**
 * 本会话有命中被「全局 snippet 预算用完」挡下了。
 */
hitsTruncated: boolean, 
/**
 * 后台分身会话。
 */
isBg: boolean, 
/**
 * 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
 */
can: Can, 
/**
 * 分叉来的：父会话的显示标题（同历史清单那一格）；不是分叉来的 · 父会话这一趟没走到 ⇒ 缺。
 */
forkedFrom?: ForkedFrom, 
/**
 * 那台此刻对这个会话的说法（同历史清单那一行的 `face`）。
 */
face: SessionFace, };
type SearchScope = "user" | "assistant" | "report";
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed` · `too_large`

#### `history-search-merge`

问到各台搜一遍、合成一份。

收 `args` · 可撤 · CLI：`ccm -- --history-search-merge`

入参 `HistorySearchMergeReq` · 回包 `Merged`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
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
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanDelete = "yes" | "live" | "unsure";
type CanResume = "yes" | "switch" | "bg";
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type ForkedFrom = { label: string, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type HistorySearchMergeReq = { machines: Array<Origin>, search: HistorySearchReq, };
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
 * 全局片段上限（没给的照样记进 `moreHits`）。
 */
limit?: number, 
/**
 * 只比会话标题与第一句（不搜内容）。
 */
titles?: boolean, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type Merged = { 
/**
 * 各会话命中数（`hits` 条数 ＋ `moreHits`）之和。
 */
totalHits: number, sessionCount: number, 
/**
 * 任一行 `hitsTruncated`（每一台都被自己的 `limit` 砍过）。
 */
truncated: boolean, 
/**
 * 各台的会话行，按 `updatedAt` 倒序（同一刻照台的先后）。
 */
sessions: Array<MergedRow>, 
/**
 * 结果头底下那几句，写好的（两端照摆）：答了的那几台读不动的会话记录份数之和（0 份不写）；
 * 再是内容搜索不覆盖、只按标题的那几家各一句（各台说的，去重、照先说的那台的先后）。
 */
notes: Array<string>, 
/**
 * 没答上的那几台（它们的会话没搜到）：哪台 · 那一句。
 */
down: Array<SearchDown>, 
/**
 * 结果头那一句（搜的词 · 命中几处 · 几个会话；截过 ⇒ 截过那一句）：两端照摆。
 */
summary: string, };
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
 * 显示的名字：改过的名 ＞ 那台扫出的标题（同历史清单那一行的 `label`）。
 */
label: string, 
/**
 * 星标（那台的注解）。
 */
starred: boolean, 
/**
 * 隐藏那一格（那台判的，与历史清单那一行同一条规则）；不隐藏 ⇒ `null`。
 */
hidden: Hidden | null, 
/**
 * 这个会话是哪一家（线上的 kind）：恢复按它起。全文搜索只扫记录树 ⇒ 都是记录树那一家。
 */
agent: AgentKind, sessionId: string, projectPath: string, projectName: string, jsonlPath: string, 
/**
 * 显示标题（ai-title / 首条 user 摘要 / sid 前 8 位之一）。
 */
title: string, 
/**
 * 记录文件最后改动的毫秒。
 */
updatedAt: number, hits: Array<SearchHit>, 
/**
 * `hits` 之外这个会话还有几处命中。
 */
moreHits: number, 
/**
 * 本会话有命中被「全局 snippet 预算用完」挡下了。
 */
hitsTruncated: boolean, 
/**
 * 后台分身会话。
 */
isBg: boolean, 
/**
 * 那台判的：这个会话能做什么（与历史清单的行同一张规则）。
 */
can: Can, 
/**
 * 分叉来的：父会话的显示标题（同历史清单那一格）；不是分叉来的 · 父会话这一趟没走到 ⇒ 缺。
 */
forkedFrom?: ForkedFrom, 
/**
 * 那台此刻对这个会话的说法（同历史清单那一行的 `face`）。
 */
face: SessionFace, };
type Origin = string;
type SearchDown = { origin: Origin, said: string, };
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
tool?: string, 
/**
 * 这条记录那一行在记录文件里的起点（字节；交给 `history-read` 的 `offset` 就从它读起）。
 */
offset: number, 
/**
 * 这条记录的行号（与实时行的 `seq` 同一个空间；打开查看器后按它物化、按 `uuid` 找卡）。
 */
line: number, };
type SearchScope = "user" | "assistant" | "report";
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed` · `too_large`

#### `history-run`

一个子运行的记录。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`rows[].record` 是 `record`） · CLI：`ccm -- --history-run`

入参 `HistoryRunReq` · 回包 `HistoryRun`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type Attachment = { name: string, 
/**
 * 相对会话工作目录的路径。
 */
rel: string, 
/**
 * 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好）：chip 上的类别字。
 */
classText: string, 
/**
 * 那一份此刻多大；不在了 ⇒ 缺。
 */
bytes?: number, 
/**
 * 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
 */
sizeText?: string, 
/**
 * 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
 */
gone?: boolean, };
type Block = { "type": "text", text: string, 
/**
 * 正文多少字节（UTF-8）：界面写「展开 3.0 KB」照它，不自己量。
 */
bytes: number, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, 
/**
 * 内容多少字节（各正文块的 UTF-8 字节 ＋ 各图的原图字节）：出口省掉内容之后照样有，界面写大小照它。
 */
bytes: number, } | { "type": "image", 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 原图多少字节（base64 解开之后）。
 */
bytes: number, 
/**
 * 图在哪一行：那一行在记录里的起点字节偏移（翻译那一下写）。
 */
at: number, 
/**
 * 这一行里第几张（从 0 起，按块的先后深度优先，工具结果里的也数）。
 */
n: number, };
type ChildRunTag = { label: string, kind?: string, };
type Draw = "card" | "steps" | "strip" | "none";
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
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
 * 这一页从哪个字节起（行头）：往前再读一页拿它当 `before`；0 ⇒ 前面没有了。
 */
from: number, 
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
from?: number | null, 
/**
 * 从尾巴读（在跑的那一个从底部读起）：回记录里最后一页；与 `from` 都给以它为准。
 */
tail?: boolean | null, 
/**
 * 往前再读一页：读到这个字节为止（拿上一页的 `from`）；给了就是往前读，与 `from` 都给以它为准。
 */
before?: number | null, };
type LineDiff = { rows: Array<LineDiffRow>, 
/**
 * 收着时先画前几行；`rows` 比它多 ⇒ 其余等展开。
 */
shown: number, 
/**
 * 行没给全（超了字数上界，在段的边界停下）；`added` / `removed` 仍是整份的数。
 */
truncated: boolean, 
/**
 * 整份加了几行 · 删了几行。
 */
added: number, removed: number, 
/**
 * 「+N −M」那一句（与过程那一行右侧那一句同一处写）；没有增删 ⇒ 空串。
 */
stat: string, 
/**
 * 降级了的那一句：改前改后行数太多、没逐行比，只画整删 ＋ 整增（两端照摆）；逐行比了 ⇒ 缺。
 */
notice?: string, };
type LineDiffKind = "add" | "del" | "ctx" | "head";
type LineDiffRow = { kind: LineDiffKind, 
/**
 * 行正文（不带头字；过长的一行截了以「…」收尾）。段头 ⇒ 写好的那一格。
 */
text: string, 
/**
 * 改之前 · 改之后的行号（1 起）：加的行没有改前，删的行没有改后；段头 · 从入参算的那一份（不知道在文件第几行）⇒ 缺。
 */
oldNo?: number, newNo?: number, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: AgentKind, 
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
atMs?: number, 
/**
 * 这条在消息流里画成哪一形（后端从这条通用记录算一次；界面照它摆，不再判画不画）。
 */
draw: Draw, 
/**
 * 人在这条里说了话（人打的字 · 斜杠命令 · `!` 命令）：切 tab 那一道认它。
 */
human: boolean, } & ({ "t": "said", 
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
cwd?: string, 
/**
 * 随这一句发的附件（适配层从正文里认出、正文里那几行引用已去掉；图片附件照旧是 `blocks` 里的图片块）。没有 ⇒ 缺。
 */
attachments?: Array<Attachment>, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
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
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "unread", why: UnreadWhy, 
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
type StepBody = { "kind": "changes", diff: LineDiff, } | { "kind": "command", line: string, };
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
 * 这次改动算好的行差（照结果里的逐段改动，带真行号；[`LineDiff`]）。新建整份文件 · 没有改动 · 一段也放不下 ⇒ 缺，
 * 两端画调用那一份（[`StepBody::Changes`]）。
 */
patch?: LineDiff, 
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
 * 说明（命令那一句注一类）。
 */
note?: string, 
/**
 * 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
 */
file?: string, 
/**
 * 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
 */
body?: StepBody, 
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
 * 那条会话的记录在这台找得到（哪一家的都算）。
 */
present: boolean, 
/**
 * 找到 ⇒ 那份所在的记录根；找不到 ⇒ 查的那棵记录树的根。
 */
root: string, };
```

码：`bad_args`

#### `history-lines`

按行号取回一段。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`lines[].record` 是 `record`） · CLI：`ccm -- --history-lines`

入参 `HistoryLinesReq` · 回包 `HistoryLines`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type Attachment = { name: string, 
/**
 * 相对会话工作目录的路径。
 */
rel: string, 
/**
 * 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好）：chip 上的类别字。
 */
classText: string, 
/**
 * 那一份此刻多大；不在了 ⇒ 缺。
 */
bytes?: number, 
/**
 * 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
 */
sizeText?: string, 
/**
 * 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
 */
gone?: boolean, };
type Block = { "type": "text", text: string, 
/**
 * 正文多少字节（UTF-8）：界面写「展开 3.0 KB」照它，不自己量。
 */
bytes: number, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, 
/**
 * 内容多少字节（各正文块的 UTF-8 字节 ＋ 各图的原图字节）：出口省掉内容之后照样有，界面写大小照它。
 */
bytes: number, } | { "type": "image", 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 原图多少字节（base64 解开之后）。
 */
bytes: number, 
/**
 * 图在哪一行：那一行在记录里的起点字节偏移（翻译那一下写）。
 */
at: number, 
/**
 * 这一行里第几张（从 0 起，按块的先后深度优先，工具结果里的也数）。
 */
n: number, };
type ChildRunTag = { label: string, kind?: string, };
type Draw = "card" | "steps" | "strip" | "none";
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
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
 * `[from, next)` 里最后一个可计行之后那个字节（一行都没交 ⇒ `from` 之前最后一个可计行之后）：续点 ＝（路径 · 它 · `next`）。
 */
end: number, 
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
type LineDiff = { rows: Array<LineDiffRow>, 
/**
 * 收着时先画前几行；`rows` 比它多 ⇒ 其余等展开。
 */
shown: number, 
/**
 * 行没给全（超了字数上界，在段的边界停下）；`added` / `removed` 仍是整份的数。
 */
truncated: boolean, 
/**
 * 整份加了几行 · 删了几行。
 */
added: number, removed: number, 
/**
 * 「+N −M」那一句（与过程那一行右侧那一句同一处写）；没有增删 ⇒ 空串。
 */
stat: string, 
/**
 * 降级了的那一句：改前改后行数太多、没逐行比，只画整删 ＋ 整增（两端照摆）；逐行比了 ⇒ 缺。
 */
notice?: string, };
type LineDiffKind = "add" | "del" | "ctx" | "head";
type LineDiffRow = { kind: LineDiffKind, 
/**
 * 行正文（不带头字；过长的一行截了以「…」收尾）。段头 ⇒ 写好的那一格。
 */
text: string, 
/**
 * 改之前 · 改之后的行号（1 起）：加的行没有改前，删的行没有改后；段头 · 从入参算的那一份（不知道在文件第几行）⇒ 缺。
 */
oldNo?: number, newNo?: number, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: AgentKind, 
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
atMs?: number, 
/**
 * 这条在消息流里画成哪一形（后端从这条通用记录算一次；界面照它摆，不再判画不画）。
 */
draw: Draw, 
/**
 * 人在这条里说了话（人打的字 · 斜杠命令 · `!` 命令）：切 tab 那一道认它。
 */
human: boolean, } & ({ "t": "said", 
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
cwd?: string, 
/**
 * 随这一句发的附件（适配层从正文里认出、正文里那几行引用已去掉；图片附件照旧是 `blocks` 里的图片块）。没有 ⇒ 缺。
 */
attachments?: Array<Attachment>, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
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
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "unread", why: UnreadWhy, 
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
 * 这一行（含 `\n`）之后那个字节；相邻认不出的几行并成这一条时 ＝ 最后那一行之后。没 `\n` 收尾的残尾 ⇒ `null`。
 */
end: number | null, 
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
type StepBody = { "kind": "changes", diff: LineDiff, } | { "kind": "command", line: string, };
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
 * 这次改动算好的行差（照结果里的逐段改动，带真行号；[`LineDiff`]）。新建整份文件 · 没有改动 · 一段也放不下 ⇒ 缺，
 * 两端画调用那一份（[`StepBody::Changes`]）。
 */
patch?: LineDiff, 
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
 * 说明（命令那一句注一类）。
 */
note?: string, 
/**
 * 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
 */
file?: string, 
/**
 * 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
 */
body?: StepBody, 
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

#### `history-image`

按引用取回一张图。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --history-image`

入参 `HistoryImageReq` · 回包 `HistoryImage`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HistoryImage = { 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 图的字节，base64（记录里那一块照抄）。
 */
data: string, };
type HistoryImageReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 那一行的起点字节（引用里的 `at`）。
 */
at: number, 
/**
 * 这一行里第几张（引用里的 `n`）。
 */
n: number, 
/**
 * 那一条记录的 `id`：起点上读出来的不是它 ⇒ `not_found`（记录被改写过也不给错的图）。
 */
id: string, };
```

码：`bad_args` · `failed` · `oversized_line` · `refused` · `not_found`

#### `history-read`

按字节分页读一份会话。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`rows[]` 是 `read_row`，`rows[].record` 是 `record`） · CLI：`ccm -- --history-read`

入参 `HistoryReadReq` · 回包 `HistoryRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type Attachment = { name: string, 
/**
 * 相对会话工作目录的路径。
 */
rel: string, 
/**
 * 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好）：chip 上的类别字。
 */
classText: string, 
/**
 * 那一份此刻多大；不在了 ⇒ 缺。
 */
bytes?: number, 
/**
 * 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
 */
sizeText?: string, 
/**
 * 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
 */
gone?: boolean, };
type Block = { "type": "text", text: string, 
/**
 * 正文多少字节（UTF-8）：界面写「展开 3.0 KB」照它，不自己量。
 */
bytes: number, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, 
/**
 * 内容多少字节（各正文块的 UTF-8 字节 ＋ 各图的原图字节）：出口省掉内容之后照样有，界面写大小照它。
 */
bytes: number, } | { "type": "image", 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 原图多少字节（base64 解开之后）。
 */
bytes: number, 
/**
 * 图在哪一行：那一行在记录里的起点字节偏移（翻译那一下写）。
 */
at: number, 
/**
 * 这一行里第几张（从 0 起，按块的先后深度优先，工具结果里的也数）。
 */
n: number, };
type ChildRunTag = { label: string, kind?: string, };
type Draw = "card" | "steps" | "strip" | "none";
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
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
type LineDiff = { rows: Array<LineDiffRow>, 
/**
 * 收着时先画前几行；`rows` 比它多 ⇒ 其余等展开。
 */
shown: number, 
/**
 * 行没给全（超了字数上界，在段的边界停下）；`added` / `removed` 仍是整份的数。
 */
truncated: boolean, 
/**
 * 整份加了几行 · 删了几行。
 */
added: number, removed: number, 
/**
 * 「+N −M」那一句（与过程那一行右侧那一句同一处写）；没有增删 ⇒ 空串。
 */
stat: string, 
/**
 * 降级了的那一句：改前改后行数太多、没逐行比，只画整删 ＋ 整增（两端照摆）；逐行比了 ⇒ 缺。
 */
notice?: string, };
type LineDiffKind = "add" | "del" | "ctx" | "head";
type LineDiffRow = { kind: LineDiffKind, 
/**
 * 行正文（不带头字；过长的一行截了以「…」收尾）。段头 ⇒ 写好的那一格。
 */
text: string, 
/**
 * 改之前 · 改之后的行号（1 起）：加的行没有改前，删的行没有改后；段头 · 从入参算的那一份（不知道在文件第几行）⇒ 缺。
 */
oldNo?: number, newNo?: number, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: AgentKind, 
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
atMs?: number, 
/**
 * 这条在消息流里画成哪一形（后端从这条通用记录算一次；界面照它摆，不再判画不画）。
 */
draw: Draw, 
/**
 * 人在这条里说了话（人打的字 · 斜杠命令 · `!` 命令）：切 tab 那一道认它。
 */
human: boolean, } & ({ "t": "said", 
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
cwd?: string, 
/**
 * 随这一句发的附件（适配层从正文里认出、正文里那几行引用已去掉；图片附件照旧是 `blocks` 里的图片块）。没有 ⇒ 缺。
 */
attachments?: Array<Attachment>, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
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
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "unread", why: UnreadWhy, 
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
type StepBody = { "kind": "changes", diff: LineDiff, } | { "kind": "command", line: string, };
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
 * 这次改动算好的行差（照结果里的逐段改动，带真行号；[`LineDiff`]）。新建整份文件 · 没有改动 · 一段也放不下 ⇒ 缺，
 * 两端画调用那一份（[`StepBody::Changes`]）。
 */
patch?: LineDiff, 
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
 * 说明（命令那一句注一类）。
 */
note?: string, 
/**
 * 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
 */
file?: string, 
/**
 * 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
 */
body?: StepBody, 
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
type AgentKind = string;
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
type Attachment = { name: string, 
/**
 * 相对会话工作目录的路径。
 */
rel: string, 
/**
 * 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好）：chip 上的类别字。
 */
classText: string, 
/**
 * 那一份此刻多大；不在了 ⇒ 缺。
 */
bytes?: number, 
/**
 * 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
 */
sizeText?: string, 
/**
 * 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
 */
gone?: boolean, };
type Block = { "type": "text", text: string, 
/**
 * 正文多少字节（UTF-8）：界面写「展开 3.0 KB」照它，不自己量。
 */
bytes: number, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, 
/**
 * 内容多少字节（各正文块的 UTF-8 字节 ＋ 各图的原图字节）：出口省掉内容之后照样有，界面写大小照它。
 */
bytes: number, } | { "type": "image", 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 原图多少字节（base64 解开之后）。
 */
bytes: number, 
/**
 * 图在哪一行：那一行在记录里的起点字节偏移（翻译那一下写）。
 */
at: number, 
/**
 * 这一行里第几张（从 0 起，按块的先后深度优先，工具结果里的也数）。
 */
n: number, };
type ChildRunTag = { label: string, kind?: string, };
type Draw = "card" | "steps" | "strip" | "none";
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
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
type LineDiff = { rows: Array<LineDiffRow>, 
/**
 * 收着时先画前几行；`rows` 比它多 ⇒ 其余等展开。
 */
shown: number, 
/**
 * 行没给全（超了字数上界，在段的边界停下）；`added` / `removed` 仍是整份的数。
 */
truncated: boolean, 
/**
 * 整份加了几行 · 删了几行。
 */
added: number, removed: number, 
/**
 * 「+N −M」那一句（与过程那一行右侧那一句同一处写）；没有增删 ⇒ 空串。
 */
stat: string, 
/**
 * 降级了的那一句：改前改后行数太多、没逐行比，只画整删 ＋ 整增（两端照摆）；逐行比了 ⇒ 缺。
 */
notice?: string, };
type LineDiffKind = "add" | "del" | "ctx" | "head";
type LineDiffRow = { kind: LineDiffKind, 
/**
 * 行正文（不带头字；过长的一行截了以「…」收尾）。段头 ⇒ 写好的那一格。
 */
text: string, 
/**
 * 改之前 · 改之后的行号（1 起）：加的行没有改前，删的行没有改后；段头 · 从入参算的那一份（不知道在文件第几行）⇒ 缺。
 */
oldNo?: number, newNo?: number, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: AgentKind, 
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
atMs?: number, 
/**
 * 这条在消息流里画成哪一形（后端从这条通用记录算一次；界面照它摆，不再判画不画）。
 */
draw: Draw, 
/**
 * 人在这条里说了话（人打的字 · 斜杠命令 · `!` 命令）：切 tab 那一道认它。
 */
human: boolean, } & ({ "t": "said", 
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
cwd?: string, 
/**
 * 随这一句发的附件（适配层从正文里认出、正文里那几行引用已去掉；图片附件照旧是 `blocks` 里的图片块）。没有 ⇒ 缺。
 */
attachments?: Array<Attachment>, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
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
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "unread", why: UnreadWhy, 
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
 * 这一行（含 `\n`）之后那个字节；相邻认不出的几行并成这一条时 ＝ 最后那一行之后。没 `\n` 收尾的残尾 ⇒ `null`。
 */
end: number | null, 
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
type StepBody = { "kind": "changes", diff: LineDiff, } | { "kind": "command", line: string, };
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
 * 这次改动算好的行差（照结果里的逐段改动，带真行号；[`LineDiff`]）。新建整份文件 · 没有改动 · 一段也放不下 ⇒ 缺，
 * 两端画调用那一份（[`StepBody::Changes`]）。
 */
patch?: LineDiff, 
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
 * 说明（命令那一句注一类）。
 */
note?: string, 
/**
 * 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
 */
file?: string, 
/**
 * 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
 */
body?: StepBody, 
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
type FoldFail = { uuid: string, tool: string | null, line: number, };
type FoldSegment = { 
/**
 * 这一段跟在哪一条后面：这一轮你那句，或露着的某一条（uuid）。一段只有一个 `after`，界面按它认段。
 */
after: string, 
/**
 * `after` 那一条的行号（界面按它与 `untilLine` 在骨架账本里取这一段的那几行）。
 */
afterLine: number, 
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
 */
until: string | null, 
/**
 * `until` 那一条的行号（没有 ⇒ 没有）。
 */
untilLine: number | null, 
/**
 * 此刻：只有还没收尾的那一轮的尾巴那一段会是在跑 / 在等你（[`dress_live`]）。
 */
phase: TurnPhase, 
/**
 * 这一段里第一处失败落在哪（「跳到第一处失败」照它走，界面不自己找）；这一段没有失败 ⇒ 没有。
 */
firstFail: FoldFail | null, };
type HistoryTurns = { 
/**
 * 起点字节（＝ 请求的 `from`；问的是 `cover` ⇒ 包着它的那一轮的 `at`）。
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
 * 任一字节：从包着它的那一轮起（只拉了记录后半截的一端拿已拉到的最上面那一行问）；与 `from` 都给以它为准。
 */
cover?: number | null, 
/**
 * 只出开头在这个字节之前的轮（缺 ⇒ 到文件尾）；截在这里 ⇒ 没有下一页、`end` ＝ 它。
 */
to?: number | null, 
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
 * 你那句的行号（与实时行的 `seq` 同一个空间；跳过去时按它物化）。
 */
line: number, 
/**
 * 你那句的 uuid（跳过去时按它找卡）。
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
folds: Array<FoldSegment>, };
type TurnSpan = { text: string, from: number | null, to: number | null, };
```

码：`bad_args` · `failed` · `too_large`

#### `sessions-needs`

这台上需手动的会话清单。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --sessions-needs`

入参 `SessionsNeedsReq` · 回包 `NeedsList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type AnsweredRow = { sid: string, said: Words, };
type AskKind = "approve" | "answer" | "plan";
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type LoginStartReq = { "kind": "account", agent: AgentKind, account: string, } | { "kind": "mcp", agent: AgentKind, account: string | null, server: string, projectDir: string | null, };
type ManualFix = { command: string, args: LoginStartReq, };
type ManualItem = { "kind": "accountLogin", agent: AgentKind, account: string, said: string, fix: ManualFix, } | { "kind": "mcpLogin", 
/**
 * 在哪个号里；没设账号的那一份 ⇒ `null`。
 */
account: string | null, server: string, scope: string, said: string, fix: ManualFix | null, 
/**
 * `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
 */
why?: string, } | { "kind": "secretFile", said: string, fix: ManualFix | null, why?: string, };
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
 * 批准：那一步的主参数（命令取说得出用意的那一段）；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
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
 * 先答序（小的先答，出口只按它排）：危险度那一档在高位（顶上那个框先答，再按危险度），同一档里等得越久越小，
 * 不知道等了多久的排这一档最后。算进的是这一份答出那一刻已等多久 ⇒ 会走的钟那一侧从收到起又等了多久照减（`waitedMs` 为 `null` 的不减）。
 */
rank: number, 
/**
 * 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
 * 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：各出口从收到那一刻起按本机钟接着加（时长读口对同一份金样）。
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
 * 子运行（agent）发的 ⇒ 是哪一趟 ＋ 写好的那一句；主会话 ⇒ `null`。
 */
run: NeedsAskRun | null, 
/**
 * 一行摘要（命令里说得出用意的那一段 · 路径 · 第一道题）；说不出 ⇒ `null`。
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
type NeedsAskRun = { 
/**
 * 那一趟的 id（那一家原样）。
 */
id: string, 
/**
 * 谁要批，写好的那一句：名（派出时的标签）· 类别（那一家原样，如 `general-purpose`），缺哪样少哪样；出口照抄。
 */
text: Words, };
type NeedsCell = { id: string, label: string, };
type NeedsChoice = { 
/**
 * 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
 */
id: string, text: Words, tone: Tone, 
/**
 * 要不要附一句话：none · optional · required。
 */
note: NeedsNote, 
/**
 * 排在哪一段（出口照它分段，不按 id 认）。
 */
row: NeedsChoiceRow, };
type NeedsChoiceRow = "main" | "hold" | "release";
type NeedsItem = { "kind": "session", rank: number, session: NeedsRow, } | { "kind": "plan", rank: number, workspace: string, slice: string, cell?: NeedsCell, } | { "kind": "manual", rank: number, manual: ManualItem, };
type NeedsKind = "approve" | "answer" | "plan" | "network" | "worker" | "goal" | "choose" | "unknown";
type NeedsList = { 
/**
 * 依次：此刻活着、那台说在等人的会话 · 计划里要人看的格 · 会话外的（需登录的号 · MCP · 权限太宽的自家秘密文件）；一项都没有 ⇒ 空。
 */
items: Array<NeedsItem>, 
/**
 * 提问没人答、已替人答过的会话（各留最近一次，那条会话下一次挂上提问才换掉），按会话 id 排。
 */
answered: Array<AnsweredRow>, 
/**
 * 会话外那一截读不出（账号库清单在、却读不懂）⇒ 那一句；那一截这时不列（空清单看起来像「没事」）。读得出 ⇒ 缺。
 */
manualUnreadable?: Words, };
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
type NeedsRow = { sid: string, 
/**
 * 叫这条会话什么（同清单那一行的标题）。
 */
label: Words, 
/**
 * 此刻这条会话的整份脸。
 */
face: SessionFace, needs: Needs, 
/**
 * 隐藏那一格（照常列在这里：隐藏不影响需手动）；不隐藏 ⇒ `null`（格恒在）。
 */
hidden: Hidden | null, };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type SessionsNeedsReq = Record<symbol, never>;
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args`

#### `sessions-needs-merge`

问到各台的需手动、合成一个先后。

收 `args` · 可撤 · CLI：`ccm -- --sessions-needs-merge`

入参 `NeedsMergeReq` · 回包 `NeedsMerged`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type AskKind = "approve" | "answer" | "plan";
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type LoginStartReq = { "kind": "account", agent: AgentKind, account: string, } | { "kind": "mcp", agent: AgentKind, account: string | null, server: string, projectDir: string | null, };
type ManualFix = { command: string, args: LoginStartReq, };
type ManualItem = { "kind": "accountLogin", agent: AgentKind, account: string, said: string, fix: ManualFix, } | { "kind": "mcpLogin", 
/**
 * 在哪个号里；没设账号的那一份 ⇒ `null`。
 */
account: string | null, server: string, scope: string, said: string, fix: ManualFix | null, 
/**
 * `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
 */
why?: string, } | { "kind": "secretFile", said: string, fix: ManualFix | null, why?: string, };
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
 * 批准：那一步的主参数（命令取说得出用意的那一段）；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
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
 * 先答序（小的先答，出口只按它排）：危险度那一档在高位（顶上那个框先答，再按危险度），同一档里等得越久越小，
 * 不知道等了多久的排这一档最后。算进的是这一份答出那一刻已等多久 ⇒ 会走的钟那一侧从收到起又等了多久照减（`waitedMs` 为 `null` 的不减）。
 */
rank: number, 
/**
 * 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
 * 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：各出口从收到那一刻起按本机钟接着加（时长读口对同一份金样）。
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
 * 子运行（agent）发的 ⇒ 是哪一趟 ＋ 写好的那一句；主会话 ⇒ `null`。
 */
run: NeedsAskRun | null, 
/**
 * 一行摘要（命令里说得出用意的那一段 · 路径 · 第一道题）；说不出 ⇒ `null`。
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
type NeedsAskRun = { 
/**
 * 那一趟的 id（那一家原样）。
 */
id: string, 
/**
 * 谁要批，写好的那一句：名（派出时的标签）· 类别（那一家原样，如 `general-purpose`），缺哪样少哪样；出口照抄。
 */
text: Words, };
type NeedsAt = { origin: Origin, item: NeedsItem, };
type NeedsCell = { id: string, label: string, };
type NeedsChoice = { 
/**
 * 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
 */
id: string, text: Words, tone: Tone, 
/**
 * 要不要附一句话：none · optional · required。
 */
note: NeedsNote, 
/**
 * 排在哪一段（出口照它分段，不按 id 认）。
 */
row: NeedsChoiceRow, };
type NeedsChoiceRow = "main" | "hold" | "release";
type NeedsDown = { origin: Origin, said: string, };
type NeedsItem = { "kind": "session", rank: number, session: NeedsRow, } | { "kind": "plan", rank: number, workspace: string, slice: string, cell?: NeedsCell, } | { "kind": "manual", rank: number, manual: ManualItem, };
type NeedsKind = "approve" | "answer" | "plan" | "network" | "worker" | "goal" | "choose" | "unknown";
type NeedsMergeReq = { machines: Array<Origin>, };
type NeedsMerged = { items: Array<NeedsAt>, total: number, unreadable: Array<NeedsUnreadable>, down: Array<NeedsDown>, };
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
type NeedsRow = { sid: string, 
/**
 * 叫这条会话什么（同清单那一行的标题）。
 */
label: Words, 
/**
 * 此刻这条会话的整份脸。
 */
face: SessionFace, needs: Needs, 
/**
 * 隐藏那一格（照常列在这里：隐藏不影响需手动）；不隐藏 ⇒ `null`（格恒在）。
 */
hidden: Hidden | null, };
type NeedsUnreadable = { origin: Origin, said: Words, };
type Origin = string;
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args`

#### `needs-ask`

答题 hook 挂一条待答请求、等人答（hook 客户端 `--answer-hook` 用；本连接断了 ⇒ 撤）。

收 `args` · 连接内就地做完 · 只在流上

入参 `AskReq` · 回包 `AskReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type AskReply = { decision: unknown, };
type AskReq = { agent: AgentKind, hook: unknown, };
```

码：`bad_args`

#### `needs-answer`

答一条挂着的待答请求（`needs.ask`）：第一个答案落下，其余回「别处已答」（`gone` ＋ 那一句）。

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
type AnswerResult = "answered" | "gone";
type Answered = { result: AnswerResult, 
/**
 * `gone` 时写好的那一句（已有人答过 · 已在终端答过或已关闭 · 替人答过的那一句）；`answered` ⇒ 不上线。
 */
said?: Words, };
type AskPick = { 
/**
 * 选项 id（`needs.ask.questions[].options[].id`）；缺 ⇒ 没选。
 */
options?: Array<string>, 
/**
 * 自由文本；缺 / 空白 ⇒ 没写。
 */
text?: string | null, };
type Words = string;
```

码：`bad_args` · `not_found` · `bad_choice` · `note_required`

#### `history-facts`

会话事实。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（整份是 `facts`） · CLI：`ccm -- --history-facts`

入参 `HistoryFactsReq` · 回包 `FactsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
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
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
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
type FactsByPath = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, };
type FactsBySid = { 
/**
 * 会话 id（那台按它在自己的记录树里找，同 `session-commands`）。
 */
sid: string, };
type FactsClock = { text: Words, from: number, };
type FactsCost = { micros: number, partial: boolean, text: Words, };
type FactsLine = { for: WireSessionActivity, text: Words, clock: FactsClock | null, peek: Words | null, };
type FactsReply = { 
/**
 * 按 sid 找到的那份记录（全路径）；按路径问的 ⇒ 缺。
 */
path?: string, 
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
 * `usage.model` 写给人看的名字（那一家的模型那一处给：`agents::model_text`）；没有 usage · 认不出是哪一家 · 这一家认不出这个值 ⇒ `null`
 * （界面照抄，不自己拼）。每次现给，不累加。
 */
modelText: Words | null, 
/**
 * 此刻是换模型清单里哪一项（原值，抽屉勾它）：终端没了时记下的那一项；否则那一家把 `usage.model` 认成清单里唯一对得上的那一项
 * （`agents::model_pick`）；认不出 · 对上不止一项 ⇒ `null`。每次现给，不进续算表。
 */
modelPick: string | null, 
/**
 * 会话的项目目录（会话起在哪个目录）：适配层读记录开头给（`agents::project_dir_of`），读到即锁定；
 * 开头里还没有 ⇒ `null`，下一次再读。与会话宣告那一帧的 `project_dir` 同一个函数。
 */
projectDir: string | null, 
/**
 * 这份记录是哪一家的（线上的 kind，适配层按记录认：`agents::record_kind_of`）；认不出 ⇒ `null`（界面要分家的那几项灰着，不落哪一家）。
 */
agent: AgentKind | null, 
/**
 * 此刻持着这条会话的活进程 pid（`accounts_query::session_holders`：照那一家声明的判活方式查），升序；不累加，每次现查。
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
 * **需手动**：那台说在等、等的是什么（不累加，每次现查）。不在等 ⇒ `null`。
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
 * 同一档写给人看的那一句（那一家的权限档那一处给：`agents::permission_mode_text`，切档回包的 `modeText` 同一处）；
 * 没有档 · 认不出是哪一家 · 这一家认不出这一档 ⇒ `null`（界面照抄，不自己拼）。每次现给，不累加。
 */
permissionModeText: Words | null, 
/**
 * 全会话用量（按请求去重）；一条带用量的回复都没有 ⇒ `null`。
 */
tokens: TokenUse | null, 
/**
 * 全会话花费（记录里那一家自己记的花费那一条，最后一条为准）；记录里没有 ⇒ `null`（不按定价自己算）。
 */
cost: FactsCost | null, 
/**
 * 还没收场的后台命令（文件序）：累加的那一份账。
 */
bgTasks: Array<BgTask>, 
/**
 * **后台任务运行中**：那台说一轮停了、后台命令还在跑 ⇒ 状态一句（不累加，每次现查）。不是这一态 ⇒ `null`。
 */
background: FactsBackground | null, 
/**
 * **答题口没接上**：ccm 起这条会话时挂不上答题 hook（用户自己给的会话级设置读不出 / 形状不认）· 不是 ccm 起的、持着它的进程身上看不出挂过 ⇒ 写好的那一句
 * （「这条会话只能在终端里答批准」；不累加，每次按持着它的进程现查）。接上了 / 判不了 ⇒ `null`。
 */
answerOff: Words | null, 
/**
 * **没人答时自动答**：会话注解里那一格设的几分钟（`control::asks` 判只管提问）；关 ⇒ `null`。不累加，每次现读。
 */
autoAnswer: number | null, 
/**
 * 上一条提问没人答、已替人答过 ⇒ 写好的那一句（「10 分钟没人答，已自动答：…」，这条会话下一次挂上提问才换掉）；没有 ⇒ `null`。不累加，每次现查。
 */
autoAnswered: Words | null, 
/**
 * 这个会话里那一家说有毛病的 MCP 服务器（要登录 · 断开 · 还在连），按名字排；没列的不等于连上了。
 */
mcp: Array<McpTrouble>, 
/**
 * 由谁启动（会话血缘里它的主会话）；不是谁起的 ⇒ `null`。不累加，每次现查。
 */
startedBy: Kin | null, 
/**
 * 它起的子会话（按起的先后）；不累加，每次现查。
 */
children: Array<Kin>, 
/**
 * **状态一句里会走的那一截**：pidfile 此刻说的那一态 ＋ 记录里的事实（在跑的那一步 · 等的种类与起点 · 最后一句的时刻），[`line_of`] 写。
 * 不活 ⇒ `null`。不累加，每次现查。
 */
line: FactsLine | null, 
/**
 * 认不出、又不成卡的行（那一家的簿记行：没见过的类型、没有链上身份）：不在消息流里画，在这里照实说几行、哪几种。一行都没有 ⇒ `null`。
 */
quietUnread: QuietUnread | null, };
type Hidden = { by: HiddenBy, said: string, };
type HiddenBy = "self" | "alias" | "launch" | "child" | "bg";
type HistoryFactsReq = FactsByPath | FactsBySid;
type Kin = { sid: string, title: string, 
/**
 * 此刻这条会话的整份脸。
 */
face: SessionFace, 
/**
 * 隐藏那一格；不隐藏 ⇒ `null`（格恒在）。
 */
hidden: Hidden | null, };
type LastSay = { text: string, 
/**
 * 那条记录的 `timestamp` 原样；没有 ⇒ `null`。
 */
at: string | null, 
/**
 * `at` 的毫秒（同 [`PendingCall::at_ms`]：「完成多久前」交那一个读口走字）。
 */
atMs: number | null, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
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
 * 批准：那一步的主参数（命令取说得出用意的那一段）；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
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
 * 先答序（小的先答，出口只按它排）：危险度那一档在高位（顶上那个框先答，再按危险度），同一档里等得越久越小，
 * 不知道等了多久的排这一档最后。算进的是这一份答出那一刻已等多久 ⇒ 会走的钟那一侧从收到起又等了多久照减（`waitedMs` 为 `null` 的不减）。
 */
rank: number, 
/**
 * 到这一份答出的那一刻已经等了多久（毫秒）：`sinceMs` 与读那份 pidfile 的时刻**同在那台的钟上**相减，不跨机器减；
 * 起点比那一刻还晚（那台的钟往回拨过）⇒ 0。没有起点 ⇒ `null`。会走的钟：各出口从收到那一刻起按本机钟接着加（时长读口对同一份金样）。
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
 * 子运行（agent）发的 ⇒ 是哪一趟 ＋ 写好的那一句；主会话 ⇒ `null`。
 */
run: NeedsAskRun | null, 
/**
 * 一行摘要（命令里说得出用意的那一段 · 路径 · 第一道题）；说不出 ⇒ `null`。
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
type NeedsAskRun = { 
/**
 * 那一趟的 id（那一家原样）。
 */
id: string, 
/**
 * 谁要批，写好的那一句：名（派出时的标签）· 类别（那一家原样，如 `general-purpose`），缺哪样少哪样；出口照抄。
 */
text: Words, };
type NeedsChoice = { 
/**
 * 答的时候交回的那个词（闭集见 `control::asks::verdict_of`）。
 */
id: string, text: Words, tone: Tone, 
/**
 * 要不要附一句话：none · optional · required。
 */
note: NeedsNote, 
/**
 * 排在哪一段（出口照它分段，不按 id 认）。
 */
row: NeedsChoiceRow, };
type NeedsChoiceRow = "main" | "hold" | "release";
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
 * 主参数一行（那一家的「工具 → 主参数」表；提问取第一问）；没登记 / 入参里没有 ⇒ `null`。
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
 * 这一步此刻的样子（[`settle_pending`] 每次现判，不累加）。
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
type QuietUnread = { 
/**
 * 「无法识别 N 行（类型：a · b）」。
 */
text: Words, 
/**
 * 几行。
 */
count: number, 
/**
 * 哪几种（原文里的 `type`，首见序，至多 [`QUIET_KINDS_KEEP`] 种）。
 */
kinds: Array<string>, };
type RetryOutcome = "retrying" | "recovered" | "failed" | "interrupted";
type RetryRun = { 
/**
 * 这一串首条重试记录的 `uuid`（界面那条细条就挂在它上面）。
 */
id: string, outcome: RetryOutcome, };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type StepWait = "running" | "awaiting" | "unclear";
type TokenUse = { input: number, output: number, cacheRead: number, cacheWrite5m: number, cacheWrite1h: number, 
/**
 * 算进来的请求数。
 */
requests: number, 
/**
 * 写好的串（输入 · 输出 · 读缓存 · 写缓存）。
 */
text: Words, };
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
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed` · `too_large` · `no_such_session`

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
 * 命中的那条记录的 uuid（界面按它找卡）。
 */
uuid: string, 
/**
 * 它的行号（与实时行的 `seq` 同一个空间；界面按它物化那一段）。
 */
line: number, 
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
type Draw = "card" | "steps" | "strip" | "none";
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
classes: Array<RecordClass>, 
/**
 * 本份里出现过的画法（`dr` 列的号从 1 起指它；不画 ⇒ 0）。
 */
draws: Array<Draw>, o: Array<number>, n: Array<number>, t: Array<number>, dr: Array<number>, fd: Array<number>, body: IndexBody, 
/**
 * 人说的那几行（行号差）。
 */
hu: Array<number>, inputs: IndexInputs, };
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
at: Array<number>, u: Array<string>, x: Array<string>, ts: Array<string>, };
type RecordClass = "said" | "reply" | "retry" | "title" | "unread";
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
at: number, 
/**
 * 这一句的行号（与实时行的 `seq` 同一个空间；跳过去时按它物化）。同 `at`，由扫的那一处填。
 */
line: number, uuid: string, 
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

一份会话的尾段：手上那一份怎么接，连第一页正文。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 收 `view`（`rows[]` 是 `read_row`，`rows[].record` 是 `record`） · CLI：`ccm -- --history-tail`

入参 `HistoryTailReq` · 回包 `HistoryTail`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type Attachment = { name: string, 
/**
 * 相对会话工作目录的路径。
 */
rel: string, 
/**
 * 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
 */
class: FileClass, 
/**
 * 那一种的词（那台写好）：chip 上的类别字。
 */
classText: string, 
/**
 * 那一份此刻多大；不在了 ⇒ 缺。
 */
bytes?: number, 
/**
 * 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
 */
sizeText?: string, 
/**
 * 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
 */
gone?: boolean, };
type Block = { "type": "text", text: string, 
/**
 * 正文多少字节（UTF-8）：界面写「展开 3.0 KB」照它，不自己量。
 */
bytes: number, } | { "type": "thinking", text: string, } | { "type": "tool_use", id: string, name: string, input: unknown, } | { "type": "tool_result", 
/**
 * 对的是哪次工具调用。
 */
for: string, 
/**
 * 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
 */
content: Array<Block>, isError: boolean, 
/**
 * 内容多少字节（各正文块的 UTF-8 字节 ＋ 各图的原图字节）：出口省掉内容之后照样有，界面写大小照它。
 */
bytes: number, } | { "type": "image", 
/**
 * 图片类型（`image/png` …）。
 */
mediaType: string, 
/**
 * 原图多少字节（base64 解开之后）。
 */
bytes: number, 
/**
 * 图在哪一行：那一行在记录里的起点字节偏移（翻译那一下写）。
 */
at: number, 
/**
 * 这一行里第几张（从 0 起，按块的先后深度优先，工具结果里的也数）。
 */
n: number, };
type ChildRunTag = { label: string, kind?: string, };
type Draw = "card" | "steps" | "strip" | "none";
type FileClass = "folder" | "text" | "code" | "image" | "archive" | "pdf" | "link" | "other";
type Held = { 
/**
 * 那份记录的路径（换了路径 ⇒ 续不上）。
 */
path: string, 
/**
 * 客户端缺的第一行的行号。
 */
next: number, 
/**
 * 第 `next` 行从哪个字节起。
 */
next_byte: number, 
/**
 * 后端自己核的记号（那一刻文件最后一行的位置与摘要）：客户端只存、原样交回，不解。空串 ＝ 那一刻一行都没有。
 */
mark: string, };
type HistoryTail = { 
/**
 * 怎么接。
 */
how: TailHow, 
/**
 * `rows` 第一行的行号（续传 ＝ 交来的 `held.next`；`handoff` ＝ `total`；其余 ＝ `tail_from`）。
 */
from: number, 
/**
 * 第一页里每个可计行一条（同 `history-read.rows[]`）。
 */
rows: Array<ReadRow>, 
/**
 * 下一页从这里起（同 `history-read.next`）。
 */
next: number, 
/**
 * 这一段读完了。
 */
eof: boolean, 
/**
 * 读到 `end` 之后存的那一份（下一次原样交回）。
 */
held: Held, 
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
type HistoryTailReq = { 
/**
 * jsonl 路径（围栏同 `history-read`）。
 */
path: string, 
/**
 * 要最新几行（也是「缺得比这还多就不读那一截」的那条线）。
 */
n: number, 
/**
 * 上一次回的那一份（缺 ⇒ 手上什么都没有）。
 */
held?: Held, };
type LineDiff = { rows: Array<LineDiffRow>, 
/**
 * 收着时先画前几行；`rows` 比它多 ⇒ 其余等展开。
 */
shown: number, 
/**
 * 行没给全（超了字数上界，在段的边界停下）；`added` / `removed` 仍是整份的数。
 */
truncated: boolean, 
/**
 * 整份加了几行 · 删了几行。
 */
added: number, removed: number, 
/**
 * 「+N −M」那一句（与过程那一行右侧那一句同一处写）；没有增删 ⇒ 空串。
 */
stat: string, 
/**
 * 降级了的那一句：改前改后行数太多、没逐行比，只画整删 ＋ 整增（两端照摆）；逐行比了 ⇒ 缺。
 */
notice?: string, };
type LineDiffKind = "add" | "del" | "ctx" | "head";
type LineDiffRow = { kind: LineDiffKind, 
/**
 * 行正文（不带头字；过长的一行截了以「…」收尾）。段头 ⇒ 写好的那一格。
 */
text: string, 
/**
 * 改之前 · 改之后的行号（1 起）：加的行没有改前，删的行没有改后；段头 · 从入参算的那一份（不知道在文件第几行）⇒ 缺。
 */
oldNo?: number, newNo?: number, };
type LineRecord = { 
/**
 * 哪一家（注册表里那一家的 `kind`）。
 */
agent: AgentKind, 
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
atMs?: number, 
/**
 * 这条在消息流里画成哪一形（后端从这条通用记录算一次；界面照它摆，不再判画不画）。
 */
draw: Draw, 
/**
 * 人在这条里说了话（人打的字 · 斜杠命令 · `!` 命令）：切 tab 那一道认它。
 */
human: boolean, } & ({ "t": "said", 
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
cwd?: string, 
/**
 * 随这一句发的附件（适配层从正文里认出、正文里那几行引用已去掉；图片附件照旧是 `blocks` 里的图片块）。没有 ⇒ 缺。
 */
attachments?: Array<Attachment>, } | { "t": "reply", blocks: Array<Block>, 
/**
 * 型号名原样。
 */
model?: string, 
/**
 * 代理那一侧自动写的应答（不是模型说的，不建卡）。
 */
autoReply: boolean, 
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
error?: ReplyError, } | { "t": "retry", reason: ApiReason, attempt?: number, max?: number, } | { "t": "title", text: string, by: TitleBy, } | { "t": "unread", why: UnreadWhy, 
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
type StepBody = { "kind": "changes", diff: LineDiff, } | { "kind": "command", line: string, };
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
 * 这次改动算好的行差（照结果里的逐段改动，带真行号；[`LineDiff`]）。新建整份文件 · 没有改动 · 一段也放不下 ⇒ 缺，
 * 两端画调用那一份（[`StepBody::Changes`]）。
 */
patch?: LineDiff, 
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
type TailHow = "tail" | "resume" | "handoff" | "truncated" | "rewritten";
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
 * 说明（命令那一句注一类）。
 */
note?: string, 
/**
 * 这一步说的那份文件：绝对路径原样（界面「在文件窗口里定位」按它）；不是路径 · 相对路径 ⇒ 缺。
 */
file?: string, 
/**
 * 展开那一下画什么（改动 · 命令）；没有 ⇒ 界面照原文画入参。
 */
body?: StepBody, 
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
unavailable: Array<Unavailable>, };
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
 * 文件里有没有 cc-bus 钩子程序的字样。读不到 → `None`（**不猜**）。
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
go: UndoAt | null, howUrl: string | null, action: ChoreAction, };
type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
type ChoreCounts = { 
/**
 * 进角标的：要做 ＋ 待定 ＋ 要装，还没做完的（没做 · 过期）。
 */
badge: number, 
/**
 * 还没做完的：不是已做、也不是不用了。
 */
open: number, 
/**
 * 还没做完的要做（过期的本来就升成要做）。以下四类合起来就是 `open`。
 */
must: number, 
/**
 * 还没做完的待定。
 */
decide: number, 
/**
 * 还没做完的要装。
 */
install: number, 
/**
 * 还没做完的要装 · 可选与可选（含先决没满足的）。
 */
optional: number, 
/**
 * 已做。
 */
done: number, };
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
 * 「待办」各类的件数。
 */
counts: ChoreCounts, 
/**
 * 几台排先后的键，小的在前：有进角标的件 0 · 只剩别的没做完的 1 · 没事 2。
 */
rank: number, own: Array<OwnItem>, };
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
 * 文件的大小，写好的字（`copy_core::size_text`）；目录 / 不在 ⇒ `null`。
 */
size_text: Words | null, };
type UndoAt = { page: string, tab?: string, anchor?: string, };
type Words = string;
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
type AccountRow = { 
/**
 * `configDir` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）；账号 0 ⇒ `null`。
 */
configDirShown: string | null, 
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
badge: Badge, 
/**
 * 号牌上那一两个字（号色那一格里画的，唯一写法在后端）。
 */
plate: Words, 
/**
 * 这个号「5h 那一格」写好的字（同 `quota-read` 那一格：账读不出 ⇒ `5h 读不到`）；没出过数 · 按量号 ⇒ `null`。
 */
quota: Words | null, 
/**
 * 选不了（`selectable: false`）的那一句：需登录 · 没配 key · 不隔离 · 号目录不在；选得了 ⇒ `null`。
 * 账号 0 不经选号（起它走「不指定号」那一条）⇒ 恒 `null`。
 */
unselectable: Words | null, 
/**
 * 给不给［登录…］：需登录、而这一家能在 app 里登录 ⇒ `true`（需登录却登不了的，`unselectable` 那一句说为什么）。
 */
login: boolean, 
/**
 * 进不进新建会话的选号表：隔离、有配置目录 ⇒ `true`（需登录的照样列、灰着，选不选得了看 `selectable`）；
 * 账号 0（不经选号）· 原地模式 ⇒ `false`。两个出口照它列，不自己判。
 */
newSession: boolean, name: string, 
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
type Badge = { text: Words, warn: boolean, title: Words, };
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
go: UndoAt | null, howUrl: string | null, action: ChoreAction, };
type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
type ChoreCounts = { 
/**
 * 进角标的：要做 ＋ 待定 ＋ 要装，还没做完的（没做 · 过期）。
 */
badge: number, 
/**
 * 还没做完的：不是已做、也不是不用了。
 */
open: number, 
/**
 * 还没做完的要做（过期的本来就升成要做）。以下四类合起来就是 `open`。
 */
must: number, 
/**
 * 还没做完的待定。
 */
decide: number, 
/**
 * 还没做完的要装。
 */
install: number, 
/**
 * 还没做完的要装 · 可选与可选（含先决没满足的）。
 */
optional: number, 
/**
 * 已做。
 */
done: number, };
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
 * 「待办」各类的件数。
 */
counts: ChoreCounts, 
/**
 * 几台排先后的键，小的在前：有进角标的件 0 · 只剩别的没做完的 1 · 没事 2。
 */
rank: number, own: Array<OwnItem>, };
type DiffOp = "same" | "del" | "add";
type DiffRow = { n: number | null, op: DiffOp, text: string, };
type LastSeen = { accounts: Seen<AccountsList> | null, data: Seen<DataReport> | null, };
type LastSeenReadReq = { 
/**
 * 哪台（机器名）。
 */
origin: Origin, };
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
effectiveDefault: string | null, };
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
 * 文件的大小，写好的字（`copy_core::size_text`）；目录 / 不在 ⇒ `null`。
 */
size_text: Words | null, };
type Seen<T> = { "state": "kept", seen: SeenAt, value: T, } | { "state": "unreadable", seen: SeenAt, };
type Seen<T> = { "state": "kept", seen: SeenAt, value: T, } | { "state": "unreadable", seen: SeenAt, };
type SeenAt = { 
/**
 * 记下的时刻（毫秒）。
 */
atMs: number, 
/**
 * 「采样于 12:03」（后端照文案表与那一台的钟写）。
 */
said: Words, };
type UndoAt = { page: string, tab?: string, anchor?: string, };
type Words = string;
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

#### `chores-merge`

问到各台的「文件与数据」、件数合成一份。

收 `args` · 可撤 · CLI：`ccm -- --chores-merge`

入参 `ChoresMergeReq` · 回包 `ChoresMerged`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

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
go: UndoAt | null, howUrl: string | null, action: ChoreAction, };
type ChoreAction = "copyCommand" | "copySnippet" | "decide" | "locate" | "how" | "installFirst";
type ChoreCounts = { 
/**
 * 进角标的：要做 ＋ 待定 ＋ 要装，还没做完的（没做 · 过期）。
 */
badge: number, 
/**
 * 还没做完的：不是已做、也不是不用了。
 */
open: number, 
/**
 * 还没做完的要做（过期的本来就升成要做）。以下四类合起来就是 `open`。
 */
must: number, 
/**
 * 还没做完的待定。
 */
decide: number, 
/**
 * 还没做完的要装。
 */
install: number, 
/**
 * 还没做完的要装 · 可选与可选（含先决没满足的）。
 */
optional: number, 
/**
 * 已做。
 */
done: number, };
type ChoreKind = "must" | "decide" | "install" | "installOptional" | "optional";
type ChoreState = "todo" | "done" | "expired" | "blocked" | "declined";
type ChoresAt = { origin: Origin, report?: DataReport, said?: string, };
type ChoresMergeReq = { machines: Array<Origin>, client?: FootprintClient | null, };
type ChoresMerged = { total: ChoreCounts, machines: Array<ChoresAt>, };
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
 * 「待办」各类的件数。
 */
counts: ChoreCounts, 
/**
 * 几台排先后的键，小的在前：有进角标的件 0 · 只剩别的没做完的 1 · 没事 2。
 */
rank: number, own: Array<OwnItem>, };
type DiffOp = "same" | "del" | "add";
type DiffRow = { n: number | null, op: DiffOp, text: string, };
type FootprintClient = { home: string, path?: string | null, };
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
 * 文件的大小，写好的字（`copy_core::size_text`）；目录 / 不在 ⇒ `null`。
 */
size_text: Words | null, };
type UndoAt = { page: string, tab?: string, anchor?: string, };
type Words = string;
```

码：`bad_args`

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

入参 `AssetsCatalogReq` · 回包 `CatalogReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AssetsCatalogReq = Record<symbol, never>;
type CatalogAsset = { kind: ExtKind, name: string, 
/**
 * 那台上的项目目录（项目级）；`None` = 用户级。
 */
project?: string, 
/**
 * 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
 */
dir?: string, digest: string, 
/**
 * 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义；插件：市场 · 版本 · 说明 · 组成 · 开关出处）。
 */
summary: unknown, 
/**
 * 装了、关着（只有插件有这一态；缺 ＝ 开着）。
 */
off?: boolean, };
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
type CatalogRow = { kind: ExtKind, name: string, state: string, from: Array<CatalogFrom>, };
type ExtKind = "skill" | "mcp" | "plugin";
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `assets-catalog-merge`

把另一台后端的整份目录并进来。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --assets-catalog-merge`

入参 `AssetsCatalogMergeReq` · 回包 `CatalogReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AssetsCatalogMergeReq = { catalog: CatalogIn, };
type CatalogAsset = { kind: ExtKind, name: string, 
/**
 * 那台上的项目目录（项目级）；`None` = 用户级。
 */
project?: string, 
/**
 * 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
 */
dir?: string, digest: string, 
/**
 * 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义；插件：市场 · 版本 · 说明 · 组成 · 开关出处）。
 */
summary: unknown, 
/**
 * 装了、关着（只有插件有这一态；缺 ＝ 开着）。
 */
off?: boolean, };
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
type CatalogRow = { kind: ExtKind, name: string, state: string, from: Array<CatalogFrom>, };
type ExtKind = "skill" | "mcp" | "plugin";
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `assets-sync`

本机常驻后端沿池里那条 SSH 同步资产目录。

收 `args` · 可撤 · CLI：`ccm -- --assets-sync`

入参 `AssetsSyncReq` · 回包 `AssetsSynced`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AssetsSyncReq = { 
/**
 * 那台的名字。
 */
origin?: string, };
type AssetsSyncRow = { 
/**
 * 那台的名字。
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
 * 「连这台」的那几台 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）。
 */
reach: Array<ReachRow>, };
type Origin = string;
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
 * 写的时候 `Door::put` 用的 `root`。
 */
base: string, 
/**
 * 写的时候 `Door::put` 用的相对前缀：`rel` = `<prefix>/<path>`。
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

#### `mcp-read`

这台机器的 MCP 列表成品。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --mcp-read`

入参 `McpReadReq` · 回包 `McpReadReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type LoginStartReq = { "kind": "account", agent: AgentKind, account: string, } | { "kind": "mcp", agent: AgentKind, account: string | null, server: string, projectDir: string | null, };
type ManualFix = { command: string, args: LoginStartReq, };
type McpFailedMark = { said: string, tip: string, detail: string | null, };
type McpLoginFix = { 
/**
 * 在哪个号里；没设账号的那一份 ⇒ `null`。
 */
account: string | null, 
/**
 * 在 app 里登：`login-start {kind: mcp}`；那一家不在 app 里登 MCP ⇒ `null`。
 */
fix: ManualFix | null, 
/**
 * `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
 */
why?: string, };
type McpLoginMark = { said: string, tip: string, fix: Array<McpLoginFix>, };
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
type ExtKind = "skill" | "mcp" | "plugin";
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
type ExtKind = "skill" | "mcp" | "plugin";
type ExtLoc = { "level": "user" } | { "level": "project", dir: string, };
type ExtTokens = { source: string, target: string | null, };
type HubApplied = { machines: Array<HubMachineDone>, };
type HubApplyReq = { kind: ExtKind, name: string, to: Array<string | null>, place: ExtLoc | null, tokens: { [key in string]: ExtTokens }, fill: { [key in string]: { [key in string]: string } }, };
type HubMachineDone = { to: string | null, name: string, done: ExtDone | null, error: string | null, detail: string | null, };
```

码：`bad_args` · `missing` · `catalog_unreadable` · `io_failed`

#### `ext-list`

设置「扩展」页那张表。

收 `args` · 可撤 · 只在流上

入参 `ExtListReq` · 回包 `ExtList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtBring = { from: string | null, 
/**
 * 来源给人看的名字（本机 · 别的台 · cc-monitor 自带）。
 */
fromName: string, scope: ExtScope, targets: Array<ExtTarget>, };
type ExtCell = { 
/**
 * 表上那个点。
 */
state: ExtState, 
/**
 * 那个点那一句（悬停 · 抽屉里那台那一行；后端写好）。
 */
said: string, 
/**
 * 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目。
 */
places: Array<ExtPlace>, bring: ExtBring | null, 
/**
 * 没有「装到…」时为什么（现状 ＋ 能做什么）。
 */
note: string | null, };
type ExtDetail = { label: string, value: string, };
type ExtKind = "skill" | "mcp" | "plugin";
type ExtKnown = { 
/**
 * 名字底下那一句：从哪来（cc-monitor 自带 · 发布处）。
 */
from: string, 
/**
 * 内置备注（有才有）。
 */
note: string | null, 
/**
 * 装上之后要在那台的 agent 配置里加钩子：抽屉里每台一行钩子状态（问那台的 `hooks-diag`）。
 */
hooks: boolean, };
type ExtLegend = { state: ExtState, said: string, };
type ExtList = { machines: Array<ExtMachine>, rows: Array<ExtRow>, problems: Array<string>, 
/**
 * 表下图例：列哪几态 ＋ 各一句（「关着」只在有一台装着插件时列）。
 */
legend: Array<ExtLegend>, 
/**
 * 表顶「认得」那一组的组头（带 `known` 的那几行排在最前，界面在它们前面放这一句）。
 */
knownTitle: string, };
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
projects: Array<string>, 
/**
 * 那台的 MCP 列表（那台 `mcp-read` 的成品原样：本机后端问到再交，`faces/ext_list_face.rs::with_mcp`）；
 * 表里没有 MCP 行 · 那台没连上 · 问不到 ⇒ 缺（界面那台的 MCP 格不画小标）。这一层只转交、不读它：
 * 形状是 `mcp-read` 的回包（金样 `tests/__fixtures__/mcp-read.golden.json` 钉），界面按那个生成类型收（`ext-reads.ts::mcpOf`）。
 */
mcp?: unknown, };
type ExtPlace = { at: ExtLoc, 
/**
 * 全局那一行：`same` · `differs` · `missing`；项目那几行：`same` · `differs`（与「这一版」比）。
 */
state: ExtState, 
/**
 * 这一处那一句（后端写好）。
 */
said: string, 
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
note: string | null, 
/**
 * 这一处自己的一句（插件：版本 · 开着 / 关着 · 出处）；别的种类没有。
 */
line?: string, };
type ExtRow = { kind: ExtKind, 
/**
 * 种类那一格的字（后端写好）。
 */
kindSaid: string, name: string, about: string | null, detail: Array<ExtDetail>, new: boolean, 
/**
 * cc-monitor 认得的那几样才有（表顶一组，排在最前）。
 */
known: ExtKnown | null, 
/**
 * 用户写的备注（随目录在后端之间同步）。
 */
note: string | null, cells: Array<ExtCell>, };
type ExtScope = { from: ExtLoc, to: ExtLoc, };
type ExtState = "same" | "differs" | "missing" | "project" | "off";
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
type ExtCell = { 
/**
 * 表上那个点。
 */
state: ExtState, 
/**
 * 那个点那一句（悬停 · 抽屉里那台那一行；后端写好）。
 */
said: string, 
/**
 * 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目。
 */
places: Array<ExtPlace>, bring: ExtBring | null, 
/**
 * 没有「装到…」时为什么（现状 ＋ 能做什么）。
 */
note: string | null, };
type ExtDetail = { label: string, value: string, };
type ExtKind = "skill" | "mcp" | "plugin";
type ExtKnown = { 
/**
 * 名字底下那一句：从哪来（cc-monitor 自带 · 发布处）。
 */
from: string, 
/**
 * 内置备注（有才有）。
 */
note: string | null, 
/**
 * 装上之后要在那台的 agent 配置里加钩子：抽屉里每台一行钩子状态（问那台的 `hooks-diag`）。
 */
hooks: boolean, };
type ExtLegend = { state: ExtState, said: string, };
type ExtList = { machines: Array<ExtMachine>, rows: Array<ExtRow>, problems: Array<string>, 
/**
 * 表下图例：列哪几态 ＋ 各一句（「关着」只在有一台装着插件时列）。
 */
legend: Array<ExtLegend>, 
/**
 * 表顶「认得」那一组的组头（带 `known` 的那几行排在最前，界面在它们前面放这一句）。
 */
knownTitle: string, };
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
projects: Array<string>, 
/**
 * 那台的 MCP 列表（那台 `mcp-read` 的成品原样：本机后端问到再交，`faces/ext_list_face.rs::with_mcp`）；
 * 表里没有 MCP 行 · 那台没连上 · 问不到 ⇒ 缺（界面那台的 MCP 格不画小标）。这一层只转交、不读它：
 * 形状是 `mcp-read` 的回包（金样 `tests/__fixtures__/mcp-read.golden.json` 钉），界面按那个生成类型收（`ext-reads.ts::mcpOf`）。
 */
mcp?: unknown, };
type ExtPlace = { at: ExtLoc, 
/**
 * 全局那一行：`same` · `differs` · `missing`；项目那几行：`same` · `differs`（与「这一版」比）。
 */
state: ExtState, 
/**
 * 这一处那一句（后端写好）。
 */
said: string, 
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
note: string | null, 
/**
 * 这一处自己的一句（插件：版本 · 开着 / 关着 · 出处）；别的种类没有。
 */
line?: string, };
type ExtRow = { kind: ExtKind, 
/**
 * 种类那一格的字（后端写好）。
 */
kindSaid: string, name: string, about: string | null, detail: Array<ExtDetail>, new: boolean, 
/**
 * cc-monitor 认得的那几样才有（表顶一组，排在最前）。
 */
known: ExtKnown | null, 
/**
 * 用户写的备注（随目录在后端之间同步）。
 */
note: string | null, cells: Array<ExtCell>, };
type ExtScope = { from: ExtLoc, to: ExtLoc, };
type ExtState = "same" | "differs" | "missing" | "project" | "off";
type ExtTarget = { at: ExtLoc, ok: boolean, note: string | null, };
```

码：`bad_args` · `catalog_unreadable` · `io_failed`

#### `ext-note-set`

写 / 改 / 清一个扩展的备注。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ext-note-set`

入参 `ExtNoteReq` · 回包 `ExtNote`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExtKind = "skill" | "mcp" | "plugin";
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
type ExtKind = "skill" | "mcp" | "plugin";
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
type ExtKind = "skill" | "mcp" | "plugin";
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
type HooksDiagnosis = { stop: HookState, note: string, };
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
 * `path` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）。
 */
shown: string, 
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
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, 
/**
 * 标签页：显示 / 隐藏（`null` ＝ 继承）。
 */
tab: Tab | null, 
/**
 * 起出来的会话进哪个标签页组（`null` ＝ 继承）。
 */
tabGroup: string | null, 
/**
 * 开口那句：落在 tmux 里起新会话时交给 agent（`null` ＝ 继承）。
 */
say: string | null, };
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
accountShape: ProfileAccountShape | null, 
/**
 * 在这台终端里起它的那一行（[`super::start_line`]）：终端屏照抄、敲进去。
 */
start: string, };
type ProfileSlot = { id: ProfileSlotId, label: string, };
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "tab" | "tabGroup" | "say";
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxNaming = { mode: ProfileTmuxMode, text: string, };
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesBook = { 
/**
 * 配置文件的路径。
 */
path: string, 
/**
 * `path` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）。
 */
pathShown: string, 
/**
 * 配置文件在不在。
 */
exists: boolean, 
/**
 * 盘上那份的版本（不在 ⇒ `null`），存的时候当 `ifRev` 交回 `profiles-write`。
 */
rev: string | null, 
/**
 * 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按看的那一台的时区写好）；没改过 ⇒ `null`。
 */
editedAt: string | null, 
/**
 * TOML 本身写坏 ⇒ 那一处（这时 `profiles` 为空、不能按条目改）；否则 `null`。
 */
fileProblem: ProfileProblem | null, profiles: Array<ProfileRow>, 
/**
 * 这台存好的几组会话（`[set."组名"]`，照写下的先后）；一组写错只那一组带 `problems`。
 */
sets: Array<SetOut>, 
/**
 * 配置文件不在时首建那几条的预览（在 ⇒ 空）：两条（`cc` · `cct`），没有 tmux 的那台只一条。
 */
seed: Array<ProfileRow>, 
/**
 * 空态那一块的说明与按钮字，随 `seed` 的条数（没有 tmux 的那台只建一条）；`seed` 空 ⇒ 空串。
 */
seedHint: string, seedCreate: string, 
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
tmux: boolean | null, 
/**
 * 表单各格的标签，按表单从上到下的顺序（表单 · 删一段时「会丢哪几项」那一句照它写格名）。
 */
slots: Array<ProfileSlot>, 
/**
 * 「在哪起」那三种 tmux 取名各一句（怎么取名 ＋ 撞名时怎样），按 auto · fixed · base。
 */
tmuxNaming: Array<ProfileTmuxNaming>, };
type SetOut = { name: string, rows: Array<SetRowOut>, problems: Array<ProfileProblem>, };
type SetRowOut = { cwd: string, alias: string, n: number, tabGroup: string | null, line: number, };
type Tab = "show" | "hide";
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
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, 
/**
 * 标签页：显示 / 隐藏（`null` ＝ 继承）。
 */
tab: Tab | null, 
/**
 * 起出来的会话进哪个标签页组（`null` ＝ 继承）。
 */
tabGroup: string | null, 
/**
 * 开口那句：落在 tmux 里起新会话时交给 agent（`null` ＝ 继承）。
 */
say: string | null, };
type ProfileMergeRow = { key: string, slot: ProfileSlotId, label: string, vals: Array<string>, said: string, from: string, 
/**
 * 「来自 {from}」（新建框单个页那几格照摆）。
 */
fromSaid: string, overriddenBy: string | null, };
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
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "tab" | "tabGroup" | "say";
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
type Tab = "show" | "hide";
```

码：`bad_args` · `refused`

#### `profiles-from-line`

一条自由命令行折成别名表单（头一个词是哪一家的启动器 · ccm 照它自己的解析器折）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --profiles-from-line`

入参 `ProfilesFromLineReq` · 回包 `ProfileForm`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ProfileAccountPick = { "kind": "account", name: string, } | { "kind": "base" };
type ProfileCwdCase = { at: string, to: string, };
type ProfileForm = { name: string, from: string | null, account: ProfileAccountPick | null, tmux: ProfileTmuxPick | null, cwdIf: Array<ProfileCwdCase> | null, cwd: string | null, agent: string | null, 
/**
 * 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
 */
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, 
/**
 * 标签页：显示 / 隐藏（`null` ＝ 继承）。
 */
tab: Tab | null, 
/**
 * 起出来的会话进哪个标签页组（`null` ＝ 继承）。
 */
tabGroup: string | null, 
/**
 * 开口那句：落在 tmux 里起新会话时交给 agent（`null` ＝ 继承）。
 */
say: string | null, };
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesFromLineReq = { name: string, 
/**
 * 那一条命令原样（写法同 shell：空白分词、引号里原样）。
 */
line: string, cwd?: string | null, 
/**
 * 起在这个固定名的 tmux 会话里（缺 ⇒ 照命令行）。
 */
sessionName?: string | null, };
type Tab = "show" | "hide";
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
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, 
/**
 * 标签页：显示 / 隐藏（`null` ＝ 继承）。
 */
tab: Tab | null, 
/**
 * 起出来的会话进哪个标签页组（`null` ＝ 继承）。
 */
tabGroup: string | null, 
/**
 * 开口那句：落在 tmux 里起新会话时交给 agent（`null` ＝ 继承）。
 */
say: string | null, };
type ProfileOp = { "op": "set", was: string | null, form: ProfileForm, } | { "op": "remove", name: string, children?: ProfileChildren, } | { "op": "init", seed: boolean, } | { "op": "saveSet", was: string | null, set: SetEdit, } | { "op": "removeSet", name: string, };
type ProfileSlotChange = { slot: ProfileSlotId, label: string, before: string, after: string, };
type ProfileSlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "tab" | "tabGroup" | "say";
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesImpact = { affected: Array<ProfileAffected>, };
type ProfilesImpactReq = { changes: Array<ProfileOp>, };
type RowEdit = { cwd: string, alias: string, n: number, tabGroup: string | null, };
type SetEdit = { name: string, rows: Array<RowEdit>, };
type Tab = "show" | "hide";
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
args: string | null, launcher: string | null, tmuxSize: string | null, detach: boolean, 
/**
 * 标签页：显示 / 隐藏（`null` ＝ 继承）。
 */
tab: Tab | null, 
/**
 * 起出来的会话进哪个标签页组（`null` ＝ 继承）。
 */
tabGroup: string | null, 
/**
 * 开口那句：落在 tmux 里起新会话时交给 agent（`null` ＝ 继承）。
 */
say: string | null, };
type ProfileOp = { "op": "set", was: string | null, form: ProfileForm, } | { "op": "remove", name: string, children?: ProfileChildren, } | { "op": "init", seed: boolean, } | { "op": "saveSet", was: string | null, set: SetEdit, } | { "op": "removeSet", name: string, };
type ProfileTmuxMode = "auto" | "fixed" | "base";
type ProfileTmuxPick = { mode: ProfileTmuxMode, name: string, };
type ProfilesWriteDone = { wrote: boolean, rev: string | null, reload: string | null, };
type ProfilesWriteReq = { changes: Array<ProfileOp>, ifRev: string | null, };
type RowEdit = { cwd: string, alias: string, n: number, tabGroup: string | null, };
type SetEdit = { name: string, rows: Array<RowEdit>, };
type Tab = "show" | "hide";
```

码：`bad_args` · `refused` · `stale`

#### `set-plan`

补足一组会话的预览：每行已有几个 · 再起几个 · 进哪个组 · 起不了的原因。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --set-plan` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SetPlanReq` · 回包 `SetPlan`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type RowEdit = { cwd: string, alias: string, n: number, tabGroup: string | null, };
type RowPlan = { cwd: string, alias: string, 
/**
 * 要补足到几个。
 */
want: number, 
/**
 * 此刻已有几个。
 */
have: number, 
/**
 * 这一下再起几个（起不了 ⇒ 0）。
 */
more: number, 
/**
 * 起出来的进哪个标签页组。
 */
group: string, 
/**
 * 这一行起不了的原因（能起 ⇒ `null`）。
 */
blocked: string | null, 
/**
 * 这一行写成一段：「目录 组名 已有/要的」，起不了 ⇒「目录 原因」（组菜单那一项悬停侧卡里的一行）。
 */
said: string, 
/**
 * 「已有 n」。
 */
haveSaid: string, 
/**
 * 右侧那一格：「再起 n」· 够了「—」· 起不了 ⇒ 原因。
 */
moreSaid: string, 
/**
 * 这一行的个数还能不能再加一（一行上限 · 一组合计上限都没到）。
 */
canMore: boolean, 
/**
 * 不能再加的那一句（能加 ⇒ `null`）。
 */
moreWhy: string | null, };
type SetPlan = { rows: Array<RowPlan>, more: number, 
/**
 * 组菜单那一项的第二行：「N 个目录 · M 个会话」（有起不了的行 ⇒ 再写「K 行起不了」）。
 */
short: string, 
/**
 * 右侧那一格：「再起 N」· 都够了「已齐」。
 */
moreSaid: string, 
/**
 * 预览末行：合计再起几个。
 */
totalSaid: string, 
/**
 * 主按钮的字：「启动 N」· 都够了「已齐」。
 */
startSaid: string, };
type SetPlanReq = { name?: string, rows?: Array<RowEdit>, };
```

码：`bad_args` · `no_tmux` · `no_such_set` · `refused`

#### `set-launch`

照存好的一组补足、逐条起：起之前写好组注解，一条不行不挡别的。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --set-launch` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SetLaunchReq` · 回包 `SetLaunched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
type ArriveAsk = { 
/**
 * 认它用的那一格。
 */
by: ArriveBy, 
/**
 * 起之前这台报到的序号：只认这之后报到的。
 */
since: number, 
/**
 * 起在哪个 tmux 会话里（没见到时抓那一屏当原话）；开窗起的 ⇒ `null`。
 */
session: string | null, };
type ArriveBy = { "kind": "sid", sid: Sid, } | { "kind": "cwd", cwd: string, };
type GroupRef = { id: string, name: string, };
type Landed = "started" | "in_place" | "running" | "idle" | "window";
type LaunchFail = { code: string, message: string, };
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type Opened = { 
/**
 * 落在哪：`started` · `in_place` · `running` · `idle` · `window`。
 */
landed: Landed, 
/**
 * 落在哪个 tmux 会话（`window` ⇒ `null`）。
 */
session: string | null, 
/**
 * 要开终端跑的那一行（那台渲好的）：`window` ⇒ 起会话那一行（不跑就没起）或进目录那一行；其余 ⇒ 接进那个 tmux 会话那一行。
 */
line: string | null, 
/**
 * 恢复 / 接回 ⇒ 那一条；新起 ⇒ 先定好的 sid（那一家不认先定 ⇒ `null`）。
 */
sid: string | null, 
/**
 * 起的是哪一家（接回不起 agent ⇒ `null`）。
 */
agent: AgentKind | null, 
/**
 * 起在哪个目录（`~` 已按这台的家目录展开；开窗的起始目录、认报到都用它；接回 · 接上已有的 shell ⇒ `null`）。
 */
cwd: string | null, 
/**
 * 实际用的号（账号 0 / 不表态 / 没另起 ⇒ `null`）。
 */
account: LaunchedAccount | null, 
/**
 * 那台要附带说的一句（命中几个在跑的、接了第一个）；没有 ⇒ `null`。
 */
note: string | null, 
/**
 * 第一句交进去的结局（同 `terminal-input` 的回包）；没给第一句 ⇒ `null`。第一句作为启动参数交给那一家 ⇒ 起好了就是送到了（起不成整条失败、走不到这里）。
 */
prompt: Sent | null, 
/**
 * 起了一家（或开窗那一行跑起来就起）⇒ 等它报到那一问（`session-arrive` 的入参，原样交回）；没起 ⇒ `null`。
 */
arrival: ArriveAsk | null, };
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
type RowLaunched = { cwd: string, alias: string, group: GroupRef | null, opened: Array<Opened>, failed: Array<LaunchFail>, blocked: string | null, };
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
 * 这一结局之后出口可以做的下一步（后端照结局与原因判好，出口照摆、不按 `why` 自己挑）；没有 ⇒ 不带这一格。
 */
acts?: Array<SentAct>, };
type SentAct = "retry" | "recapture" | "relist";
type SentResult = "delivered" | "unsure" | "refused";
type SetLaunchReq = { name: string, };
type SetLaunched = { rows: Array<RowLaunched>, started: number, failed: number, 
/**
 * 起完那一句：全成「已启动 n」· 有没成的「启动 成/全」。
 */
said: string, 
/**
 * 没成的逐条一行「别名 启动失败 · 原因」（全成 ⇒ 空串）。
 */
detail: string, };
type Sid = string;
```

码：`bad_args` · `no_tmux` · `no_such_set` · `refused`

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

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-list`

入参 `NoArgs` · 回包 `BusList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusAgent = { 
/**
 * 会话 id（收件人就写它）。
 */
id: string, 
/**
 * 叫它什么（出口照摆）：这台有它的记录 ⇒ 会话标题；没有 ⇒ 照实就是会话 id。
 */
title: string, 
/**
 * 待读条数。
 */
unread: number, 
/**
 * 这条会话此刻活着没有：`true` / `false` / `null`（判不了，**不是**「不在」）。
 */
live: boolean | null, };
type BusList = { agents: Array<BusAgent>, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args` · `failed`

#### `bus-send`

发一封信：核信封、照规矩收进收件箱。

收 `args` · 可撤 · CLI：`ccm -- --bus-send`

入参 `BusEnvelopeReq` · 回包 `BusSent`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusAct = "ask" | "answer" | "request" | "inform" | "handback" | "correct";
type BusEnvelope = { 
/**
 * 消息 id（`re` · `msg:` 引的就是它）。
 */
id: string, 
/**
 * 时间（UTC，`YYYY-MM-DDTHH:MM:SSZ`）。
 */
ts: string, 
/**
 * 发件人（会话 id）。
 */
from: string, 
/**
 * 收件人（会话 id）。
 */
to: string, 
/**
 * 1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。
 */
tier: number, 
/**
 * 这一封是做什么的（线上码）。
 */
act: BusAct, 
/**
 * 标题（一行）。
 */
title: string, 
/**
 * 接在哪一条后面（消息 id），没有 ⇒ `null`。
 */
re: string | null, 
/**
 * 引用（路径 · `pb:` · `msg:` · `commit:` · URL）。
 */
refs: Array<string>, 
/**
 * 正文。
 */
text: string, };
type BusEnvelopeReq = { 
/**
 * 发件人：会话 id。
 */
from: string, 
/**
 * 收件人：会话 id。
 */
to: string, 
/**
 * 收件会话在哪台（机器表里的名字）。缺 ⇒ 本机；别台 ⇒ 经跨台那一跳交给那台收（`dial::remote_ask::hop`）。
 */
machine?: Origin, 
/**
 * 1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。缺 ⇒ 2。
 */
tier?: number, 
/**
 * 这一封是做什么的（线上码；写错的码在解入参那一下就拒）。
 */
act: BusAct, 
/**
 * 一行，不超过 80 字。
 */
title: string, 
/**
 * 接在哪一条后面（消息 id）；答 · 交回 · 更正必带。
 */
re?: string | null, 
/**
 * 引用，只核前缀形状。缺 ⇒ 没有引用。
 */
refs?: Array<string> | null, 
/**
 * 正文，不超过 1200 字节（长的写成文件，给路径）。缺 ⇒ 只有标题。
 */
text?: string | null, };
type BusSent = { envelope: BusEnvelope, knock: boolean, };
type Origin = string;
```

码：`bad_args` · `bad_from` · `bad_to` · `bad_tier` · `bad_title` · `need_re` · `bad_re` · `bad_ref` · `too_long` · `not_same_group` · `no_roster` · `duplicate` · `throttled` · `io_failed` · `offline`

#### `bus-inbox`

只读看一个 agent 收件箱的尾巴。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-inbox`

入参 `BusInboxReq` · 回包 `BusInbox`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusAct = "ask" | "answer" | "request" | "inform" | "handback" | "correct";
type BusInbox = { messages: Array<BusInboxItem>, 
/**
 * 读不懂的行数（不是规范信封的也算）。
 */
skipped: number, 
/**
 * 回显超过 4 MiB ⇒ 保尾，`true`。
 */
truncated: boolean, };
type BusInboxItem = { 
/**
 * 动作码显示成的那个词（问 · 答 · 请求 · 告知 · 交回 · 更正）。
 */
act_said: string, 
/**
 * 消息 id（`re` · `msg:` 引的就是它）。
 */
id: string, 
/**
 * 时间（UTC，`YYYY-MM-DDTHH:MM:SSZ`）。
 */
ts: string, 
/**
 * 发件人（会话 id）。
 */
from: string, 
/**
 * 收件人（会话 id）。
 */
to: string, 
/**
 * 1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。
 */
tier: number, 
/**
 * 这一封是做什么的（线上码）。
 */
act: BusAct, 
/**
 * 标题（一行）。
 */
title: string, 
/**
 * 接在哪一条后面（消息 id），没有 ⇒ `null`。
 */
re: string | null, 
/**
 * 引用（路径 · `pb:` · `msg:` · `commit:` · URL）。
 */
refs: Array<string>, 
/**
 * 正文。
 */
text: string, };
type BusInboxReq = { 
/**
 * 先过形状判定，不过 ⇒ `bad_id`。
 */
id: string, 
/**
 * 1..=2000，缺 ⇒ 200。
 */
lines?: number, };
```

码：`bad_args` · `bad_id` · `failed`

#### `bus-peek`

读新信（不推已读位置）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-peek`

入参 `BusPeekReq` · 回包 `BusPeeked`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusPeekReq = { 
/**
 * 会话 id（先过形状判定）。
 */
id: string, 
/**
 * 这一段最多几封（分段排空积压）；缺 ⇒ 不分段。
 */
max_lines?: number, };
type BusPeeked = { text: string, token: string | null, 
/**
 * 这一段几封。
 */
count: number, 
/**
 * 读不懂、跳过的几行（它们不会再出现）。
 */
skipped: number, };
```

码：`bad_args` · `bad_id` · `failed`

#### `bus-commit`

推已读位置（比令牌）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-commit`

入参 `BusCommitReq` · 回包 `BusCommitted`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusCommitReq = { id: string, 
/**
 * `bus-peek` 给的令牌。
 */
token: string, };
type BusCommitted = { read: number, };
```

码：`bad_args` · `bad_id` · `bad_token` · `stale` · `changed` · `io_failed`

#### `bus-unread`

未读几封。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --bus-unread`

入参 `BusUnreadReq` · 回包 `BusUnread`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BusUnread = { unread: number, };
type BusUnreadReq = { id: string, };
```

码：`bad_args` · `bad_id` · `failed`

### 4.8 终端与会话

#### `terminal-ssh`

给一台远端开终端要跑的那一串。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminal-ssh`

入参 `TerminalSshReq` · 回包 `SshCommand`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Origin = string;
type Sid = string;
type SshCommand = { 
/**
 * 只给 cc-monitor 自己开的窗、接在 `line` 前面跑的一段（跑它的 shell 把窗口标签记进本机后端的家；里面是本机数据目录的
 * 绝对路径与这台终端方言的写法）。入参没给 `sid` · 记录目录不在 ⇒ 空串。
 */
prelude: string, 
/**
 * 这台电脑终端里跑的那一行（Windows：PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'`；别处：同一组参数的 POSIX shell 一行）。
 */
line: string, };
type TerminalSshReq = { 
/**
 * 那台的名字。
 */
machine: Origin, 
/**
 * 要在那台跑的命令。
 */
command?: string, 
/**
 * 在那台的这个目录开登录 shell（串，或非 UTF-8 的 `{"b16": "<hex>"}`）。
 */
cwd?: string | { b16: string }, 
/**
 * 这个终端显示的那条会话（只跟 `command`）：给了 ⇒ 那一行前面接一段，跑它的那个 shell 把自己的窗口标签记进本机后端的家，↗ 先照它在本机找。
 */
sid?: Sid, };
```

码：`bad_args` · `bad_jump` · `refused` · `no_ssh_client` · `unobservable`

#### `session-window`

这条会话在这台电脑上的哪儿（窗口标签 · 进程链，或原因）。

收 `args` · 可撤 · 只在流上

入参 `SessionWindowReq` · 回包 `SessionWindow`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FrontAct = { "kind": "connect" } | { "kind": "open-in-terminal" } | { "kind": "open-here" } | { "kind": "copy", detail: string, } | { "kind": "update" } | { "kind": "retry" } | { "kind": "reconnect" };
type FrontFace = { title: Words, body: Words, hint?: Words, tone: Tone, acts: Array<FrontAct>, };
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
type Origin = string;
type SessionWindow = { 
/**
 * 显示它的终端带的窗口标签（`<shell 进程号>-<起始时刻>`），最近动静在前；没带 ⇒ 空。
 */
labels: Array<string>, 
/**
 * 这台上显示它的那个终端进程往上的进程链（自下而上）；空 ⇔ 有 `face`。
 */
chain: Array<Link>, 
/**
 * 会话在 tmux 里。
 */
tmux: boolean, 
/**
 * 链为什么是空的：写好的那一张（出口照摆；壳按标签也没找到窗口时摆它）。
 */
face?: FrontFace, };
type SessionWindowReq = { 
/**
 * 会话所在那台（本机 ⇒ `<local>`）。
 */
machine: Origin, 
/**
 * 会话 id。
 */
sid: Sid, };
type Sid = string;
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type Words = string;
```

码：`bad_args` · `failed` · `child_timed_out`

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
 * 一个都没有时为什么。
 */
why?: ShownWhy, 
/**
 * 会话在 tmux 里（问它的那台照带给出口：窗口关了时给不给［在终端里打开］）。旧版那台不带 ⇒ 当不在。
 */
tmux: boolean, };
type ShownWhy = "detached" | "no-terminal" | "unreadable";
type Sid = string;
type SidReq = { 
/**
 * 会话 id。
 */
sid: Sid, };
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

这台的终端名单（两种宿主同一形，每一行带宿主）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 13 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminals-list`

入参 `TerminalsListReq` · 回包 `TerminalsList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentKind = string;
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
type HolderWho = "terminal-window" | "desktop" | "mobile" | "none";
type InputHolder = { 
/**
 * 哪一端。
 */
who: HolderWho, 
/**
 * 是不是这次请求自报的那个前端。
 */
mine: boolean, 
/**
 * 给人看的那一句（「输入在此处」·「输入在手机」…，后端写好，出口照抄）。
 */
said: string, };
type StartedBy = { 
/**
 * 会话上的 `@ccm_client`（没声明 ⇒ `null`）。
 */
client: string | null, 
/**
 * 这个调用方过不过身份门（与 `can.input` 是不是 `true` 同一个判定；结束还要看 `can.end`）。
 */
mine: boolean, };
type TermInput = { "kind": "shared" } | { "kind": "held", holder: InputHolder, };
type TermSession = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 哪一家（没标就缺）。
 */
agent?: AgentKind, };
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
 * 终端宿主。
 */
host: WireTerminalHost, 
/**
 * 给人看的名（tmux：会话名；托管：`ccm --attach <名>` 认的那个名）。
 */
name: string, 
/**
 * 里面跑着会话、前台是 agent 时是窗格标题；否则是前台程序名。
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
 * 输入怎么进：`shared`（tmux：各端都能打字）· `held`（托管：只有持有者的进）。
 */
input: TermInput, 
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
type WireTerminalHost = "tmux" | "managed";
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `terminals-board-here`

这台「终端」视图那几行（只列 cc-monitor 起的，排好、字写好）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --terminals-board-here`

入参 `BoardHereReq` · 回包 `BoardHere`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BoardHere = { rows: Array<BoardRow>, 
/**
 * `false` ＝ 名单里有读不懂的行。
 */
complete: boolean, };
type BoardHereReq = { 
/**
 * 自报的前端（决定每行的 `can_end`）。
 */
client?: string, };
type BoardRow = { 
/**
 * 这一行在整张名单里的键（不透明、跨几次问稳定）：出口按它认行。
 */
key: string, 
/**
 * 终端句柄（`terminals-list` 那一格；抓屏 · 订画面 · 送字时交回）。
 */
terminal: string, 
/**
 * 终端的名字（`terminals-list` 那一行的 `name`：tmux 是会话名，托管是 `ccm --attach <名>` 认的那个名）。
 */
name: string, 
/**
 * 里面跑着的会话。
 */
sid?: string, dot: SessionDot, 
/**
 * 点的名字（读屏 · 悬停）。
 */
dot_name: Words, title: Words, 
/**
 * 单独终端窗口的系统标题（标题 · 终端 · 那台）：各台合成那一步写（那台叫什么合成时才知道），这台那一份是空的。
 */
window_title: Words, 
/**
 * 标题照命令原文、等宽画（没有会话的终端）。
 */
mono: boolean, 
/**
 * 标题旁的徽标（在等人 ⇒ 那一态的短名）。
 */
badge?: Words, 
/**
 * 第二行（号或「没有会话」· 目录）。
 */
meta: Words, 
/**
 * 第二行尾的小签（后台 · 终端窗口 ×n · 已退出）。
 */
chips: Array<Words>, 
/**
 * 行尾那一格（现在 · 1m · 2h）。
 */
at_text: Words, 
/**
 * 它算进哪几格筛选（不含 `all`）。
 */
tags: Array<BoardTag>, 
/**
 * 找的时候比对的那一串（标题 · 目录 · 终端名，已小写）。
 */
find: string, 
/**
 * 能不能结束。
 */
can_end: true | { no: "not_yours" | "not_managed" | "other_windows", said: string }, };
type BoardTag = "all" | "session" | "bare" | "background";
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type Words = string;
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `terminals-board`

「终端」视图那张名单（问到各台、合成一份：每台一组 · 筛选四格的数）。

收 `args` · 可撤 · CLI：`ccm -- --terminals-board`

入参 `BoardReq` · 回包 `TerminalsBoard`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type BoardDown = { said: Words, };
type BoardFilter = { tag: BoardTag, label: Words, count: number, };
type BoardGroup = { machine: Origin, name: Words, 
/**
 * 组头那一句（n 个终端 · m 个后台）。
 */
sub: Words, 
/**
 * 这台没答 ⇒ 那一句；`rows` 是上次答的那份（点一律 unknown）。
 */
down?: BoardDown, 
/**
 * `false` ＝ 那台名单里有读不懂的行。
 */
complete: boolean, rows: Array<BoardRow>, };
type BoardReq = { machines: Array<Origin>, client?: string, };
type BoardRow = { 
/**
 * 这一行在整张名单里的键（不透明、跨几次问稳定）：出口按它认行。
 */
key: string, 
/**
 * 终端句柄（`terminals-list` 那一格；抓屏 · 订画面 · 送字时交回）。
 */
terminal: string, 
/**
 * 终端的名字（`terminals-list` 那一行的 `name`：tmux 是会话名，托管是 `ccm --attach <名>` 认的那个名）。
 */
name: string, 
/**
 * 里面跑着的会话。
 */
sid?: string, dot: SessionDot, 
/**
 * 点的名字（读屏 · 悬停）。
 */
dot_name: Words, title: Words, 
/**
 * 单独终端窗口的系统标题（标题 · 终端 · 那台）：各台合成那一步写（那台叫什么合成时才知道），这台那一份是空的。
 */
window_title: Words, 
/**
 * 标题照命令原文、等宽画（没有会话的终端）。
 */
mono: boolean, 
/**
 * 标题旁的徽标（在等人 ⇒ 那一态的短名）。
 */
badge?: Words, 
/**
 * 第二行（号或「没有会话」· 目录）。
 */
meta: Words, 
/**
 * 第二行尾的小签（后台 · 终端窗口 ×n · 已退出）。
 */
chips: Array<Words>, 
/**
 * 行尾那一格（现在 · 1m · 2h）。
 */
at_text: Words, 
/**
 * 它算进哪几格筛选（不含 `all`）。
 */
tags: Array<BoardTag>, 
/**
 * 找的时候比对的那一串（标题 · 目录 · 终端名，已小写）。
 */
find: string, 
/**
 * 能不能结束。
 */
can_end: true | { no: "not_yours" | "not_managed" | "other_windows", said: string }, };
type BoardTag = "all" | "session" | "bare" | "background";
type Origin = string;
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type TerminalsBoard = { filters: Array<BoardFilter>, groups: Array<BoardGroup>, };
type Words = string;
```

码：`bad_args`

#### `monitor-board-here`

监控板这台那几条（序 · 活不活 · 在跑几个 agent；不认得的不答）。

收 `args` · 可撤 · CLI：`ccm -- --monitor-board-here`

入参 `MonitorHereReq` · 回包 `MonitorHere`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MonitorHere = { rows: Array<MonitorHereRow>, };
type MonitorHereReq = { sids: Array<string>, };
type MonitorHereRow = { sid: string, 
/**
 * 这台发出去的那张脸上的序（`face.order`）。
 */
order: number, 
/**
 * 那张脸的一生是「活」。
 */
live: boolean, 
/**
 * 这台运行簿里这条会话此刻在跑的子 agent 数。
 */
running: number, };
```

码：`bad_args`

#### `monitor-board`

监控板（问到各台、合成一份：分组 · 组内先后 · 组头 · 汇总那一句）。

收 `args` · 可撤 · CLI：`ccm -- --monitor-board`

入参 `MonitorBoardReq` · 回包 `MonitorBoard`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MonitorAsk = { machine: Origin, sid: string, };
type MonitorBoard = { 
/**
 * 几台 · 几条活会话 · 几个 agent 在跑；一组都没有 ⇒ 空。
 */
summary: Words, groups: Array<MonitorGroup>, };
type MonitorBoardReq = { sessions: Array<MonitorAsk>, };
type MonitorGroup = { machine: Origin, 
/**
 * 组头那一句（那台的名字 ＋ 几条）。
 */
title: Words, 
/**
 * 组内的先后（排好了）。
 */
sids: Array<string>, 
/**
 * 这台没答 ⇒ 那台离线那一句（`sids` 照请求先后）。
 */
down?: Words, };
type Origin = string;
type Words = string;
```

码：`bad_args`

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
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
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
 * 这一结局之后出口可以做的下一步（后端照结局与原因判好，出口照摆、不按 `why` 自己挑）；没有 ⇒ 不带这一格。
 */
acts?: Array<SentAct>, };
type SentAct = "retry" | "recapture" | "relist";
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

#### `terminal-take-input`

替自报的前端接管托管终端的输入。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminal-take-input`

入参 `TerminalHoldReq` · 回包 `InputNow`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HolderWho = "terminal-window" | "desktop" | "mobile" | "none";
type InputHolder = { 
/**
 * 哪一端。
 */
who: HolderWho, 
/**
 * 是不是这次请求自报的那个前端。
 */
mine: boolean, 
/**
 * 给人看的那一句（「输入在此处」·「输入在手机」…，后端写好，出口照抄）。
 */
said: string, };
type InputNow = { input: TermInput, };
type TermInput = { "kind": "shared" } | { "kind": "held", holder: InputHolder, };
type TerminalHoldReq = { 
/**
 * 名单里的不透明句柄（托管那一形）。
 */
terminal: string, 
/**
 * 自报的前端（`desktop` · `mobile`；缺省按桌面）。
 */
client?: string, };
```

码：`bad_args` · `bad_target` · `not_known` · `ambiguous` · `not_managed` · `not_yours` · `no_such_session` · `unobservable` · `child_timed_out`

#### `terminal-release-input`

交还托管终端的输入。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminal-release-input`

入参 `TerminalHoldReq` · 回包 `InputNow`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type HolderWho = "terminal-window" | "desktop" | "mobile" | "none";
type InputHolder = { 
/**
 * 哪一端。
 */
who: HolderWho, 
/**
 * 是不是这次请求自报的那个前端。
 */
mine: boolean, 
/**
 * 给人看的那一句（「输入在此处」·「输入在手机」…，后端写好，出口照抄）。
 */
said: string, };
type InputNow = { input: TermInput, };
type TermInput = { "kind": "shared" } | { "kind": "held", holder: InputHolder, };
type TerminalHoldReq = { 
/**
 * 名单里的不透明句柄（托管那一形）。
 */
terminal: string, 
/**
 * 自报的前端（`desktop` · `mobile`；缺省按桌面）。
 */
client?: string, };
```

码：`bad_args` · `bad_target` · `not_known` · `ambiguous` · `not_managed` · `not_yours` · `no_such_session` · `unobservable` · `child_timed_out`

#### `session-interrupt`

中断一条会话这一轮（不进输入队、不换持有者）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --session-interrupt` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `SessionInterruptReq` · 回包 `Sent`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
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
 * 这一结局之后出口可以做的下一步（后端照结局与原因判好，出口照摆、不按 `why` 自己挑）；没有 ⇒ 不带这一格。
 */
acts?: Array<SentAct>, };
type SentAct = "retry" | "recapture" | "relist";
type SentResult = "delivered" | "unsure" | "refused";
type SessionInterruptReq = { 
/**
 * 会话 id。
 */
sid: string, 
/**
 * 自报的前端。
 */
client?: string, };
```

码：`bad_args` · `bad_target` · `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `terminal-end`

结束名单里的一个终端。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 18 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --terminal-end`

入参 `TerminalEndReq` · 回包 `TerminalEnded`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type TerminalEndReq = { 
/**
 * 目标：名单里的不透明句柄（与 `sid` 恰给一个）。
 */
terminal?: string, 
/**
 * 目标：会话 id（与 `terminal` 恰给一个）。
 */
sid?: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维）。
 */
client?: string, };
type TerminalEnded = { terminal: string, };
```

码：`bad_args` · `bad_target` · `not_known` · `ambiguous` · `no_such_session` · `wrong_owner` · `too_many_windows` · `kill_failed` · `unobservable` · `child_timed_out`

#### `session-commands`

一条会话能用的斜杠命令与 Skill（内置 ＋ 本项目 ＋ 它用的号的配置目录）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-commands`

入参 `SessionCommandsReq` · 回包 `SessionCommands`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type CommandScope = "builtin" | "project" | "user";
type SessionAct = { 
/**
 * 经 `session-queue-put` 投进会话的那一句（那一家声明的）。
 */
say: string, 
/**
 * 送之前先确认（清空 ⇒ 是；压缩 ⇒ 否）。
 */
confirm: boolean, };
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
confirm: boolean, 
/**
 * `needsArgs` 为真时那一格的字（「带参数」）；为假 ⇒ `null`。
 */
needsArgsText: string | null, 
/**
 * `confirm` 为真时那一格的字（「先确认」）；为假 ⇒ `null`。
 */
confirmText: string | null, };
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
problems: Array<string>, 
/**
 * ［+］面板分隔线下「压缩上下文」：送进会话的那一句（经 `session-queue-put` 投进队）；这一家没有 ⇒ `null`（那一行不画）。
 */
compact: SessionAct | null, 
/**
 * 同上「清空会话」。
 */
clear: SessionAct | null, };
type SessionCommandsReq = { 
/**
 * 会话 id。
 */
sid: Sid, };
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
 * 点它时发进会话的那一句（后端写好，经 `session-queue-put` 投进队）。
 */
say: string, };
type Sid = string;
```

码：`bad_args` · `no_such_session`

#### `session-model`

换模型（开着 ⇒ 送那一家的那一行、从记录里读回换成了什么；在等人不送；终端没了 ⇒ 记下、下次恢复照它起）。

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
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
type SessionModelReq = { 
/**
 * 会话 id。
 */
sid: Sid, 
/**
 * 要换成的模型（原值；必须是 `session-open-facts` 那一家 `models` 里的一个）。
 */
model: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
 */
client?: string, };
type Sid = string;
```

码：`bad_args` · `unsupported` · `no_such_session` · `no_tmux` · `unobservable` · `child_timed_out`

#### `session-permission`

切权限档（开着 ⇒ 按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按；终端没了 ⇒ 记下、下次恢复照它起）。

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
 * 同一档写给人看的字（同会话事实 `permissionModeText` 那一处：`PermissionFace::text_of`）；没读到 · 这一档没有字 ⇒ `null`。
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
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
type SessionPermissionReq = { 
/**
 * 会话 id。
 */
sid: Sid, 
/**
 * 要切到的档（原值；必须是 `session-open-facts` 那一家 `permissionModes` 里的一个）。
 */
mode: string, 
/**
 * 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
 */
client?: string, };
type Sid = string;
```

码：`bad_args` · `unsupported` · `launch_only` · `not_known` · `ambiguous` · `no_tmux` · `tmux_too_old` · `no_server` · `no_such_session` · `capture_failed` · `unobservable` · `child_timed_out`

#### `terminal-follow`

订阅一个终端的画面（有变化推一整屏，一帧在途等回执；tmux 那一形订着时那台多一个只读客户端，用户自己配的 client-attached / client-detached 钩子会被它触发）。

收 `args` · 连接内就地做完 · 只在流上 · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `FollowReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FollowLive = "snapshot_only" | "stopped";
type FollowRefused = { live: FollowLive, };
type FollowReq = { 
/**
 * 订阅票（客户端铸的不透明串，至多 128 字节，只用一次）；之后的 `terminal_screen` / `terminal_follow_end` 帧带它。
 */
ticket: string, 
/**
 * 目标：名单里的句柄（与 `sid` 恰给一个）。
 */
terminal?: string | null, 
/**
 * 目标：会话 id（与 `terminal` 恰给一个）。
 */
sid?: string | null, };
```

码：`bad_args` · `bad_target` · `not_known` · `ambiguous` · `no_tmux` · `tmux_too_old` · `too_many_follows` · `unobservable` · `child_timed_out`

失败带 `data` 的码：`bad_target` ⇒ `FollowRefused` · `not_known` ⇒ `FollowRefused` · `ambiguous` ⇒ `FollowRefused` · `no_tmux` ⇒ `FollowRefused` · `tmux_too_old` ⇒ `FollowRefused` · `too_many_follows` ⇒ `FollowRefused` · `unobservable` ⇒ `FollowRefused` · `child_timed_out` ⇒ `FollowRefused`

#### `terminal-follow-ack`

订阅的第 `seq` 帧画完了（画面有变化就推下一帧）。

收 `args` · 连接内就地做完 · 只在流上

入参 `AckReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AckReq = { ticket: string, 
/**
 * 画完的那一帧的序号。
 */
seq: number, };
```

码：`bad_args` · `not_known`

#### `terminal-unfollow`

退订一个终端的画面（幂等，不发收尾帧）。

收 `args` · 连接内就地做完 · 只在流上

入参 `UnfollowReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type UnfollowReq = { 
/**
 * 退不在册的也回 `ok`（记下它：订阅那一问若还在路上，到了也不起）。
 */
ticket: string, };
```

码：`bad_args`

#### `sessions-stop`

停一批会话（逐个答，一个不成不挡下一个）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 392 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --sessions-stop`

入参 `SidsReq` · 回包 `Batched`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Batched = { results: Array<SessionOutcome>, };
type Origin = string;
type SessionOutcome = { 
/**
 * 哪台（每台那一跳答的是它自己 ⇒ 本机那一个；本机合好时换成那一项的那台）。
 */
machine: Origin, sid: Sid, outcome: SessionState, 
/**
 * 跳过 / 失败的原因码（挑按钮用）；做成了 ⇒ `null`。
 */
why: string | null, 
/**
 * 那台写好的那一句；做成了 ⇒ `null`。
 */
said: string | null, 
/**
 * 失败那一项的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
 */
copyDetail: string, 
/**
 * 那台写的称呼（项目 ＋ 名字）。
 */
title: string, 
/**
 * 落在哪个终端会话上。
 */
session: string | null, };
type SessionState = "done" | "skipped" | "failed";
type Sid = string;
type SidsReq = { 
/**
 * 那几个会话（1–64 个，不重复）。
 */
sids: Array<Sid>, 
/**
 * 自报的前端（「哪个前端的会话」那一维，同 `terminal-end`）。
 */
client?: string, };
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `sessions-where`

一批会话各在这台哪个终端里。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 8 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --sessions-where`

入参 `SidsReq` · 回包 `Whereabouts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Launch = "managed" | "tmux" | "direct";
type Sid = string;
type SidsReq = { 
/**
 * 那几个会话（1–64 个，不重复）。
 */
sids: Array<Sid>, 
/**
 * 自报的前端（「哪个前端的会话」那一维，同 `terminal-end`）。
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
terminals: Array<TerminalAt>, 
/**
 * 这一条恢复默认放在哪（这台的偏好「恢复时放在哪」× 那条起在哪 × 有没有 tmux · 能不能托管）。
 */
run: Launch, };
type Whereabouts = { results: Array<Whereabout>, };
```

码：`bad_args` · `unobservable` · `child_timed_out`

#### `session-open`

起会话 · 恢复 · 接回（一个请求：落在哪 ＋ 要开终端就跑的那一行）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 总期限上限 23 秒（带 `within_ms` / `--within-ms` ⇒ 收紧到它减 2 秒） · CLI：`ccm -- --session-open`

入参 `SessionOpenReq` · 回包 `Opened`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

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
type AgentKind = string;
type ArriveAsk = { 
/**
 * 认它用的那一格。
 */
by: ArriveBy, 
/**
 * 起之前这台报到的序号：只认这之后报到的。
 */
since: number, 
/**
 * 起在哪个 tmux 会话里（没见到时抓那一屏当原话）；开窗起的 ⇒ `null`。
 */
session: string | null, };
type ArriveBy = { "kind": "sid", sid: Sid, } | { "kind": "cwd", cwd: string, };
type ForkFrom = { 
/**
 * 源会话 id。
 */
sid: string, 
/**
 * 从哪条消息处分叉。
 */
uuid: string, };
type Landed = "started" | "in_place" | "running" | "idle" | "window";
type Launch = "managed" | "tmux" | "direct";
type LaunchedAccount = { name: string, configDir: string, 
/**
 * 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
 */
model: string | null, };
type NewOpen = { 
/**
 * 哪一家（线上的 kind）。必填。
 */
agent: AgentKind, 
/**
 * 工作目录（开头的 `~` 按这台的家目录读）。
 */
cwd: string, 
/**
 * 用户改过的终端名（只跟 tmux）；缺 ⇒ 这台铸（分叉不收：名字从源会话的终端名铸）。
 */
tmuxName?: string, 
/**
 * 这一趟手填的启动命令（空 / 缺 ⇒ 这台偏好里那一家的启动命令）。
 */
command?: string, 
/**
 * 分叉自哪一条（只在这一步写分支记录）。
 */
fork?: ForkFrom, 
/**
 * 第一句：起的时候交给那一家；只跟 tmux。
 */
prompt?: string, 
/**
 * 这一趟用的模型（压过偏好表）；缺 ＝ 跟随。
 */
model?: string, 
/**
 * 起在哪一个权限档（那一家声明的档之一）；缺 ＝ 这台偏好里那一家的那一档，没设 ⇒ 那一家自己的默认。
 */
permissionMode?: string, 
/**
 * 轮换：缺 ＝ 用「新会话用」的那份策略 · `{strategy: id}` ＝ 起之前按定好的 sid 写成用那一份。
 */
rotation?: RotationPick, 
/**
 * 经哪条别名起（一类 ＝ 一条别名：那一行 `ccm @别名`，别名合下来的几格在 ccm 那一侧合、这一趟给的盖过它）；缺 ⇒ 不经别名。
 */
alias?: string, };
type OpenField = "agent" | "command" | "cwd" | "account" | "place" | "tmuxName";
type OpenRefusal = { 
/**
 * 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
 */
field: OpenField | null, 
/**
 * 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
 */
unavailable: AccountUnavailable | null, };
type OpenWhat = { "kind": "new" } & NewOpen | { "kind": "resume", sid: Sid, } | { "kind": "look", sid?: string, session?: string, } | { "kind": "shell", session?: string, cwd?: string, };
type Opened = { 
/**
 * 落在哪：`started` · `in_place` · `running` · `idle` · `window`。
 */
landed: Landed, 
/**
 * 落在哪个 tmux 会话（`window` ⇒ `null`）。
 */
session: string | null, 
/**
 * 要开终端跑的那一行（那台渲好的）：`window` ⇒ 起会话那一行（不跑就没起）或进目录那一行；其余 ⇒ 接进那个 tmux 会话那一行。
 */
line: string | null, 
/**
 * 恢复 / 接回 ⇒ 那一条；新起 ⇒ 先定好的 sid（那一家不认先定 ⇒ `null`）。
 */
sid: string | null, 
/**
 * 起的是哪一家（接回不起 agent ⇒ `null`）。
 */
agent: AgentKind | null, 
/**
 * 起在哪个目录（`~` 已按这台的家目录展开；开窗的起始目录、认报到都用它；接回 · 接上已有的 shell ⇒ `null`）。
 */
cwd: string | null, 
/**
 * 实际用的号（账号 0 / 不表态 / 没另起 ⇒ `null`）。
 */
account: LaunchedAccount | null, 
/**
 * 那台要附带说的一句（命中几个在跑的、接了第一个）；没有 ⇒ `null`。
 */
note: string | null, 
/**
 * 第一句交进去的结局（同 `terminal-input` 的回包）；没给第一句 ⇒ `null`。第一句作为启动参数交给那一家 ⇒ 起好了就是送到了（起不成整条失败、走不到这里）。
 */
prompt: Sent | null, 
/**
 * 起了一家（或开窗那一行跑起来就起）⇒ 等它报到那一问（`session-arrive` 的入参，原样交回）；没起 ⇒ `null`。
 */
arrival: ArriveAsk | null, };
type Refused = "not_yours" | "not_managed" | "other_windows" | "not_known" | "ambiguous" | "ended" | "screen_changed" | "waiting" | "held_elsewhere" | "host_outdated";
type RotationPick = { 
/**
 * 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）；`null` ⇒ 这个会话不轮换（新会话用的那份不管它）。
 */
strategy: string | null, };
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
 * 这一结局之后出口可以做的下一步（后端照结局与原因判好，出口照摆、不按 `why` 自己挑）；没有 ⇒ 不带这一格。
 */
acts?: Array<SentAct>, };
type SentAct = "retry" | "recapture" | "relist";
type SentResult = "delivered" | "unsure" | "refused";
type SessionOpenReq = { 
/**
 * 起新的 · 恢复某条 · 接回 · 开一个 shell。
 */
what: OpenWhat, 
/**
 * 缺 ＝ 跟随：恢复 ⇒ 这条会话上次的号；新起 ⇒ 这台的默认号；分叉 ⇒ 源会话的号。
 */
account?: AccountAsk, 
/**
 * 放在哪；缺 ＝ 这台定（新起：有 tmux 就 tmux、分叉照源会话那一种；恢复：照这台的偏好）。接回不收。
 */
run?: Launch, 
/**
 * 开窗的那台就是这台（本机 Windows 渲 PowerShell；本机直路不带工作目录）。
 */
local: boolean, 
/**
 * 这一趟的票（每次点一下一张；期限到了带同一张再问 ⇒ 认出同一趟，不起第二个）。缺 ⇒ 不认。
 */
ticket?: string, 
/**
 * 自报的前端（就地键入过身份门，同 `kill`）。
 */
client?: string, };
type Sid = string;
```

码：`bad_args` · `unknown_agent` · `bad_command` · `no_dir` · `account_unavailable` · `place_unavailable` · `bad_tmux_name` · `tmux_taken` · `record_gone` · `already_live` · `not_hosted` · `wrong_owner` · `unobservable` · `fork_failed` · `refused` · `start_failed` · `child_timed_out` · `launch_pending` · `no_strategy`

失败带 `data` 的码：`bad_args` ⇒ `OpenRefusal` · `unknown_agent` ⇒ `OpenRefusal` · `bad_command` ⇒ `OpenRefusal` · `no_dir` ⇒ `OpenRefusal` · `account_unavailable` ⇒ `OpenRefusal` · `place_unavailable` ⇒ `OpenRefusal` · `bad_tmux_name` ⇒ `OpenRefusal` · `tmux_taken` ⇒ `OpenRefusal` · `record_gone` ⇒ `OpenRefusal` · `already_live` ⇒ `OpenRefusal` · `not_hosted` ⇒ `OpenRefusal` · `wrong_owner` ⇒ `OpenRefusal` · `unobservable` ⇒ `OpenRefusal` · `fork_failed` ⇒ `OpenRefusal` · `refused` ⇒ `OpenRefusal` · `start_failed` ⇒ `OpenRefusal` · `child_timed_out` ⇒ `OpenRefusal` · `launch_pending` ⇒ `OpenRefusal` · `no_strategy` ⇒ `OpenRefusal`

#### `session-arrive`

等起好的会话报到。

收 `args` · 可撤 · 只在流上

入参 `ArriveAsk` · 回包 `ArriveEnd`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ArriveAsk = { 
/**
 * 认它用的那一格。
 */
by: ArriveBy, 
/**
 * 起之前这台报到的序号：只认这之后报到的。
 */
since: number, 
/**
 * 起在哪个 tmux 会话里（没见到时抓那一屏当原话）；开窗起的 ⇒ `null`。
 */
session: string | null, };
type ArriveBy = { "kind": "sid", sid: Sid, } | { "kind": "cwd", cwd: string, };
type ArriveEnd = { "kind": "arrived", sid: string, } | { "kind": "missed", 
/**
 * 第一行是标题，其余是正文（含那一屏末几行）。
 */
said: string, 
/**
 * 起在的那个 tmux 会话此刻还在 ⇒ 它在终端名单里的句柄；不在 · 读不到 · 开窗起的 ⇒ `null`。
 */
terminal: string | null, };
type Sid = string;
```

码：`bad_args`

#### `session-open-facts`

起新会话那个框要的事实：最近目录 · 能放在哪几种 · 有哪几家 agent · 分叉时原会话的起法。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-open-facts`

入参 `OpenFactsReq` · 回包 `OpenFacts`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type AgentFacts = { 
/**
 * 线上的 kind（交回 `session-open {what: new}` 的 `agent` 那一格）。
 */
kind: string, 
/**
 * 这一家写给人看的名字。
 */
text: string, 
/**
 * 起会话可选的模型（不选 ＝ 跟着那一家自己的默认）；这一家没声明 ⇒ 空。
 */
models: Array<AgentPick>, 
/**
 * 换模型抽屉的第一项「默认」：会话开着时交 `session-model` 这个值 ⇒ 回到那一家自己的缺省模型（起会话不交它，不选就是默认）；
 * 这一家没声明 ⇒ `null`。
 */
modelDefault: AgentPick | null, 
/**
 * 抽屉底那一行：换模型的副作用（那一家自己的行为，例如也改这个号以后新会话的默认）；没有 ⇒ `null`。
 */
modelNote: string | null, 
/**
 * 起会话可选的权限档（不给越过批准那一档）；这一家没有 ⇒ 空。
 */
permissionModes: Array<AgentPick>, 
/**
 * 从这台起新会话默认的那一档（这台偏好里设了、且还在上面那张清单里）；没设 ⇒ `null`（跟随那一家自己的默认）。
 */
permissionDefault: AgentPick | null, };
type AgentKind = string;
type AgentPick = { 
/**
 * 原值（`session-open` 的 `model` / `permissionMode` 收的就是它）。
 */
value: string, 
/**
 * 写给人看的字。
 */
text: string, };
type ForkFacts = { 
/**
 * 源会话是哪一家（说不出 ⇒ 空串）。
 */
agent: AgentKind, 
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
type Launch = "managed" | "tmux" | "direct";
type OpenFacts = { 
/**
 * 这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个）。
 */
recent: Array<RecentDir>, 
/**
 * 这台能放在哪几种（`run` 那一格可选的值，照 `Launch` 的序；只有 `direct` ⇒ 只能开终端窗口）。
 */
hosts: Array<Launch>, 
/**
 * 新建框默认「运行于」：同 `session-open {what: new}` 不给 `run` 时落的那一形（能托管就托管，分叉的源会话在 tmux 里 ⇒ tmux）。
 */
run: Launch, 
/**
 * 这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序），各带名字与可选的模型 · 权限档。
 */
agents: Array<AgentFacts>, 
/**
 * 没给 `forkOf` ⇒ `null`。
 */
fork: ForkFacts | null, };
type OpenFactsReq = { 
/**
 * 可缺：分叉源会话 sid。
 */
forkOf?: string, 
/**
 * 可缺（随 `forkOf`）：从哪条消息处分叉（框顶说第几轮、几点）。
 */
at?: string, };
type RecentDir = { 
/**
 * 工作目录。
 */
cwd: string, 
/**
 * 那台按自己的家目录写好的显示形（`~/…`），两端照抄。
 */
shown: string, 
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

#### `session-open-dir`

起新会话前核一个目录：在不在 · 会用哪个终端名。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --session-open-dir`

入参 `OpenDirReq` · 回包 `DirChecked`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DirChecked = { 
/**
 * 这个目录在不在。
 */
exists: boolean, 
/**
 * 这台按自己的家目录写好的显示形（`~/…`），两端照抄。
 */
shown: string, 
/**
 * 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
 */
tmuxName: string | null, };
type OpenDirReq = { 
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
type AgentKind = string;
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
sid: Sid, 
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
 * 这台是不是界面所在那台（同 `session-open`）。
 */
local: boolean, 
/**
 * 哪一家（线上的 kind）。启动命令 · 模型表这台自己读。
 */
agent: AgentKind, 
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
type Sid = string;
```

码：`bad_args` · `unobservable` · `account_unavailable` · `not_in_terminal` · `ambiguous` · `already_live` · `stop_failed` · `start_failed` · `child_timed_out`

失败带 `data` 的码：`account_unavailable` ⇒ `AccountUnavailable` · `ambiguous` ⇒ `RestartRefused` · `already_live` ⇒ `RestartRefused` · `stop_failed` ⇒ `RestartRefused` · `start_failed` ⇒ `RestartRefused`

### 4.9 会话输入队

#### `session-queue-put`

往一条会话投一句（排进那条会话的队；同 id 只入队一次，同一端按端序号保序）。

收 `args` · 可撤 · CLI：`ccm -- --session-queue-put`

入参 `SessionQueuePutReq` · 回包 `QueuePut`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Origin = string;
type QueuePut = { outcome: "queued" | "held" | "seen", 
/**
 * 那一条的序号（收着的还没有）。
 */
seq?: number, 
/**
 * 排第几（排着的那几条里，从 1 数；不是排着 ⇒ 不在）。
 */
place?: number, 
/**
 * 那一条此刻的样子，会话那台写好（排队 · 第几 · 已送进 · 已撤），发件端贴在那一句下面。
 */
said: string, };
type Rewind = { 
/**
 * 被编辑的那一句在记录里的 id（`history-user-inputs` 那一行的 `uuid`）：退到它之前，改好的那句接在它原来的位置上。
 */
to: string, how: "conversation" | "code_and_conversation", when: "after_turn" | "now", };
type SessionQueuePutReq = { 
/**
 * 会话所在那台（本机 ⇒ `<local>`）。
 */
machine: Origin, sid: Sid, 
/**
 * 发件端给的唯一 id（这一端待发箱里那一句的编号）：同 id 再投只入队一次。
 */
id: string, 
/**
 * 发自哪一端。
 */
end: string, 
/**
 * 这一端自己的递增序号（从 1 数）：给了 ⇒ 同一端先后两条保序（前一号没到先收着）；不给 ⇒ 照到达先后。
 */
n?: number, 
/**
 * 几点发的（发件端的 unix 毫秒）：会话那台只照记在盘上，名单不带、不参与排序。
 */
atMs: number, text: string, 
/**
 * 随这一句发的附件编号（按先后）。会话那台当场按会话工作目录找：有一份不在 ⇒ 拒（`attachment_gone`），一个字节不写。缺 ⇒ 不带。
 */
attachments?: Array<string>, 
/**
 * 编辑已发的那一句（确认那一下才带）：投递时先在会话里退到 `rewind.to` 那一句之前，再说 `text`。普通的一句 ⇒ 不带。
 */
rewind?: Rewind, };
type Sid = string;
```

码：`bad_args` · `io_failed` · `queue_unreadable` · `queue_full` · `text_too_long` · `attachment_gone` · `offline`

#### `session-queue-cancel`

撤队里一条还没送的。

收 `args` · 可撤 · CLI：`ccm -- --session-queue-cancel`

入参 `SessionQueueCancelReq` · 回包 `QueueCancelled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Origin = string;
type QueueCancelled = { 
/**
 * `false` ⇒ 那一条已不在队里（送过 · 撤过 · 没有这一条），什么都没动。
 */
cancelled: boolean, queue: QueueList, };
type QueueItem = { 
/**
 * 主人发的序号（收着的那几条还没有）。先后只看它。
 */
seq?: number, 
/**
 * 发件端给的唯一 id（重发去重）。
 */
id: string, 
/**
 * 发自哪一端。
 */
end: string, 
/**
 * 那一句。
 */
text: string, 
/**
 * 随这一句发的附件编号（`session-attachment-commit` 回的，按先后）；没有 ⇒ 不在。
 */
attachments?: Array<string>, 
/**
 * 这一句是编辑已发的那一句（发送那一下选的怎么退）：投递时先退到那一句之前再说它；普通的一句 ⇒ 不在。
 */
rewind?: Rewind, state: "queued" | "held" | "undelivered", 
/**
 * 那一条此刻的样子，会话那台写好（排队 · 第 N · 排队 · 失败 · 未送进会话；同回执那一句）：各端照摆，不按 `state` 自己挑字。
 */
said: string, 
/**
 * 发自哪一端，会话那台写好的那个词（认得的端写名字，别的照 `end` 原样）。
 */
from: string, };
type QueueList = { items: Array<QueueItem>, 
/**
 * 最近送进 agent 的那几条的 id（先送的在前，只留最近几条）：发件端据它把那一句标「已送进」；不在名单也不在这里 ⇒ 撤了。
 */
sent: Array<string>, };
type Rewind = { 
/**
 * 被编辑的那一句在记录里的 id（`history-user-inputs` 那一行的 `uuid`）：退到它之前，改好的那句接在它原来的位置上。
 */
to: string, how: "conversation" | "code_and_conversation", when: "after_turn" | "now", };
type SessionQueueCancelReq = { machine: Origin, sid: Sid, id: string, };
type Sid = string;
```

码：`bad_args` · `io_failed` · `queue_unreadable` · `offline`

#### `session-queue-read`

一条会话此刻的队列名单。

收 `args` · 可撤 · CLI：`ccm -- --session-queue-read`

入参 `SessionQueueReadReq` · 回包 `QueueList`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Origin = string;
type QueueItem = { 
/**
 * 主人发的序号（收着的那几条还没有）。先后只看它。
 */
seq?: number, 
/**
 * 发件端给的唯一 id（重发去重）。
 */
id: string, 
/**
 * 发自哪一端。
 */
end: string, 
/**
 * 那一句。
 */
text: string, 
/**
 * 随这一句发的附件编号（`session-attachment-commit` 回的，按先后）；没有 ⇒ 不在。
 */
attachments?: Array<string>, 
/**
 * 这一句是编辑已发的那一句（发送那一下选的怎么退）：投递时先退到那一句之前再说它；普通的一句 ⇒ 不在。
 */
rewind?: Rewind, state: "queued" | "held" | "undelivered", 
/**
 * 那一条此刻的样子，会话那台写好（排队 · 第 N · 排队 · 失败 · 未送进会话；同回执那一句）：各端照摆，不按 `state` 自己挑字。
 */
said: string, 
/**
 * 发自哪一端，会话那台写好的那个词（认得的端写名字，别的照 `end` 原样）。
 */
from: string, };
type QueueList = { items: Array<QueueItem>, 
/**
 * 最近送进 agent 的那几条的 id（先送的在前，只留最近几条）：发件端据它把那一句标「已送进」；不在名单也不在这里 ⇒ 撤了。
 */
sent: Array<string>, };
type Rewind = { 
/**
 * 被编辑的那一句在记录里的 id（`history-user-inputs` 那一行的 `uuid`）：退到它之前，改好的那句接在它原来的位置上。
 */
to: string, how: "conversation" | "code_and_conversation", when: "after_turn" | "now", };
type SessionQueueReadReq = { machine: Origin, sid: Sid, };
type Sid = string;
```

码：`bad_args` · `io_failed` · `queue_unreadable` · `offline`

#### `session-rewind-options`

编辑已发的一句时，发送那一下给哪几项（怎么退 · 什么时候退）。

收 `args` · 可撤 · CLI：`ccm -- --session-rewind-options`

入参 `SessionRewindOptionsReq` · 回包 `RewindOptions`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Origin = string;
type RewindChoiceRow = { how: "conversation" | "code_and_conversation", 
/**
 * 那一项的字（会话那台写好）。
 */
text: string, 
/**
 * 缺省选它（只落在能选的那一项上）。
 */
default: boolean, 
/**
 * 这一刻能不能选；不能 ⇒ 界面灰着摆、写 [`Self::why`]。
 */
enabled: boolean, 
/**
 * 不能选的那一句（会话那台写好）；能选 ⇒ 不在。
 */
why?: string, };
type RewindOptions = { 
/**
 * 「怎么退」那几项（通用的几种都列，不能选的灰着带原因）；空 ⇒ 这一家不支持从这里回退（为什么见 `said`）。
 */
how: Array<RewindChoiceRow>, 
/**
 * 「什么时候退」那几项；空 ⇒ 主 agent 此刻空着（只有后台子 agent 在跑也算空着），不给这一行。
 */
when: Array<RewindWhenRow>, 
/**
 * 给人看的那一句（会话那台写好）：`how` 空 ⇒ 为什么退不了；有项 ⇒ 退之前要知道的（这句之后派出的子 agent 还在跑）；都没有 ⇒ 不在。
 */
said?: string, };
type RewindWhenRow = { when: "after_turn" | "now", text: string, default: boolean, };
type SessionRewindOptionsReq = { machine: Origin, sid: Sid, 
/**
 * 被编辑的那一句在记录里的 id（同 [`Rewind::to`]）：哪一项能选按那一句之后的事判。
 */
to: string, };
type Sid = string;
```

码：`bad_args` · `no_such_session` · `offline`

### 4.10 机器

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

这台的 `ccm` 会哪些、终端里敲它走到哪。

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
build: string, 
/**
 * 终端里敲 `ccm` 走到的是不是这一份，与那一句。
 */
onPath: OnPath, };
type NoArgs = Record<symbol, never>;
type OnPath = { verdict: OnPathVerdict, said: string, };
type OnPathVerdict = "ours" | "other" | "absent" | "unknown";
```

码：`bad_args`

#### `ccm-user-path`

`ccm` 所在目录在不在用户级 `PATH` 上 · 加 · 撤。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --ccm-user-path`

入参 `UserPathReq` · 回包 `UserPath`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type UserPath = { state: UserPathState, said: string, 
/**
 * 问不出 / 改没成时的复制详情（原话在里面）；顺利 ⇒ `null`。
 */
detail: string | null, 
/**
 * 在 ⇒ 撤那一段；不在 ⇒ 加那一段；判不了 ⇒ `null`。
 */
command: string | null, };
type UserPathOp = "read" | "add" | "remove";
type UserPathReq = { op: UserPathOp, };
type UserPathState = "on" | "off" | "unknown" | "not_windows";
```

码：`bad_args`

#### `deploy-install`

给那台装 / 升级后端。

收 `args` · 可撤 · 只在流上

入参 `DeployInstallReq` · 回包 `Installed`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ActionTag = "skip" | "deploy" | "keep";
type DeployInstallReq = { slots: Array<SlotRow>, 
/**
 * 那台的名字（非空；机器表按它查 · 状态成品按它记）。
 */
machine: string, };
type Installed = { action: ActionTag, 
/**
 * 这一趟做了什么那一句（`deploy::Plan::said`）。
 */
said: string, 
/**
 * 落点上现在那一份自报的身份（`keep` ⇒ 那台上那一份的；否则这一版的）。
 */
build: string, 
/**
 * 那台的 OS（表 A 的词）。
 */
os: string, };
type SlotRow = { os: string, arch: string, id: string, 
/**
 * 本机上那一格字节的文件（绝对路径；后端读它，不经帧传字节）。
 */
path: string, };
```

码：`bad_args` · `io_failed` · `refused` · `unreachable` · `undecidable` · `not_carried` · `fenced` · `place_failed`

#### `deploy-uninstall`

卸那台的后端。

收 `args` · 可撤 · 只在流上

入参 `DeployUninstallReq` · 回包 `Uninstalled`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DeployUninstallReq = { slots: Array<SlotRow>, machine: string, };
type SlotRow = { os: string, arch: string, id: string, 
/**
 * 本机上那一格字节的文件（绝对路径；后端读它，不经帧传字节）。
 */
path: string, };
type Uninstalled = { 
/**
 * 删了落点那一份 ⇒ `true`；本来就不在 ⇒ `false`。
 */
removed: boolean, said: string, };
```

码：`bad_args` · `io_failed` · `refused` · `unreachable` · `not_carried` · `fenced` · `place_failed`

#### `machine-read`

机器表里每台此刻的状态。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `MachineReadReq` · 回包 `MachineRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DialWhy = "resolve" | "unreachable" | "timeout" | "host_key" | "auth" | "password" | "key_unreadable" | "jump" | "session_denied" | "other";
type EmptyFace = { 
/**
 * 大字那一句（「无会话」· 「本机无会话」· 一台都没报完那一句）。
 */
line: string, 
/**
 * 下面那一行（怎么让它有 · 哪几台还在连 · 哪几台未计入）。
 */
hint: string, };
type MachineFix = "retry" | "conn_settings" | "push_key" | "compare_fingerprint" | "update" | "connect" | "install";
type MachineFixButton = { fix: MachineFix, label: string, };
type MachineRead = { machines: { [key in string]: MachineState }, empty: EmptyFace, };
type MachineReadReq = Record<symbol, never>;
type MachineStage = "deploy" | "attach";
type MachineState = { 
/**
 * 给人看的那台的名字（问题行那一句里说的就是它）。
 */
name: string, state: MachineStateKind, 
/**
 * 算不算离线（页头「N 台离线」数的就是这一格）：没连上 · 指纹不对 · 做不了 ⇒ 是；登录过了、只是拒开会话 ⇒ 不是。
 */
offline: boolean, 
/**
 * 没连上的原因码（`down` · `host_key_changed` 才有）。
 */
reason: DialWhy | null, 
/**
 * 正在连时到哪一步。
 */
stage: MachineStage | null, 
/**
 * 那台 cc-monitor 的产品版本号：与这一版同一份字节时就是这一版的号；别的报不出 ⇒ 空。
 */
version: string | null, 
/**
 * 与这一版的版本关系（握手过才有）。
 */
versionRelation: VersionRelation | null, 
/**
 * 那台的系统（部署问到过才有）。
 */
os: string | null, 
/**
 * 名字旁那个词（连着时空）。
 */
word: string, 
/**
 * 问题行那一句（空 ⇒ 不出问题行）。
 */
said: string, 
/**
 * 那台离线那一句（恒有，连接层写的；哪一态都带着）。出口要说那台离线时照摆这一句，不各自取字。
 */
offlineSaid: string, fixes: Array<MachineFixButton>, 
/**
 * 指纹不对时那台这一次出示的主机指纹（`host_key_changed` 才有）。
 */
seenHostKey: string | null, 
/**
 * 指纹不对时库里记下的那一枚（`host_key_changed` 才有；库里没有同类型的 ⇒ 空串）。拨号核心说的，不在这里算。
 */
expectedHostKey: string | null, 
/**
 * 复制详情：这一轮没连上 / 做不了时那一份（界面照原样跟在问题行那句后面）；连着时末尾接每条连接那一段（卡头 ⋯ 里复制）。
 */
detail: string | null, 
/**
 * 到这台怎么连过去的那一句（「经跳板 devbox · 经代理」；多跳外到内「经跳板 a → b」）：跳板照配置，经代理连上了才知道。
 * 直连又不经代理 ⇒ 省略。界面有就摆一行，不拼字；每条连接的读数不上卡头，在 `detail` 末尾「连接：」那一段。
 */
route?: string, 
/**
 * 机器条上那一行（缺 ⇒ 不进条：连着的 · 比这一版新的 · 停用的 · 还不知道的）。桌面与手机的条都只照这一格画。
 */
strip?: MachineStripLine, };
type MachineStateKind = "up" | "connecting" | "installing" | "updating" | "down" | "host_key_changed" | "needs_update" | "newer" | "disabled" | "unsupported" | "unknown";
type MachineStripLine = { tone: StripTone, progress: boolean, };
type StripTone = "busy" | "offline" | "warn" | "fault";
type VersionRelation = "same" | "older" | "newer" | "incomparable";
```

码：`bad_args`

#### `machine-find`

打的字是不是恰好一台（名字 · 本机 · ssh 配置的 Host · 名字后跟第几条）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machine-find`

入参 `MachineFindReq` · 回包 `MachineFind`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineFind = { hit: MachineHit | null, };
type MachineFindReq = { query: string, };
type MachineFrom = "local" | "table" | "ssh_config";
type MachineHit = { 
/**
 * 那台的名字（本机 ⇒ `<local>`；机器表里的写法）。
 */
machine: Origin, from: MachineFrom, 
/**
 * 名字后跟的数字：那台的第几条会话（从 1 起）；没跟 ⇒ 空。
 */
nth: number | null, };
type Origin = string;
```

码：`bad_args`

#### `machine-menu`

那台此刻能做的几样（打开终端 · 新建会话 · 文件 · 机器状态 · 加入机器表）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · 只在流上

入参 `MachineMenuReq` · 回包 `MachineMenu`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DialWhy = "resolve" | "unreachable" | "timeout" | "host_key" | "auth" | "password" | "key_unreadable" | "jump" | "session_denied" | "other";
type MachineFix = "retry" | "conn_settings" | "push_key" | "compare_fingerprint" | "update" | "connect" | "install";
type MachineFixButton = { fix: MachineFix, label: string, };
type MachineFrom = "local" | "table" | "ssh_config";
type MachineMenu = { from: MachineFrom, 
/**
 * 那台的机器状态成品（同设置 › 机器卡那一份）；只在 ssh 配置里的那一种没有。
 */
head: MachineState | null, actions: Array<MenuAction>, };
type MachineMenuReq = { machine: Origin, };
type MachineStage = "deploy" | "attach";
type MachineState = { 
/**
 * 给人看的那台的名字（问题行那一句里说的就是它）。
 */
name: string, state: MachineStateKind, 
/**
 * 算不算离线（页头「N 台离线」数的就是这一格）：没连上 · 指纹不对 · 做不了 ⇒ 是；登录过了、只是拒开会话 ⇒ 不是。
 */
offline: boolean, 
/**
 * 没连上的原因码（`down` · `host_key_changed` 才有）。
 */
reason: DialWhy | null, 
/**
 * 正在连时到哪一步。
 */
stage: MachineStage | null, 
/**
 * 那台 cc-monitor 的产品版本号：与这一版同一份字节时就是这一版的号；别的报不出 ⇒ 空。
 */
version: string | null, 
/**
 * 与这一版的版本关系（握手过才有）。
 */
versionRelation: VersionRelation | null, 
/**
 * 那台的系统（部署问到过才有）。
 */
os: string | null, 
/**
 * 名字旁那个词（连着时空）。
 */
word: string, 
/**
 * 问题行那一句（空 ⇒ 不出问题行）。
 */
said: string, 
/**
 * 那台离线那一句（恒有，连接层写的；哪一态都带着）。出口要说那台离线时照摆这一句，不各自取字。
 */
offlineSaid: string, fixes: Array<MachineFixButton>, 
/**
 * 指纹不对时那台这一次出示的主机指纹（`host_key_changed` 才有）。
 */
seenHostKey: string | null, 
/**
 * 指纹不对时库里记下的那一枚（`host_key_changed` 才有；库里没有同类型的 ⇒ 空串）。拨号核心说的，不在这里算。
 */
expectedHostKey: string | null, 
/**
 * 复制详情：这一轮没连上 / 做不了时那一份（界面照原样跟在问题行那句后面）；连着时末尾接每条连接那一段（卡头 ⋯ 里复制）。
 */
detail: string | null, 
/**
 * 到这台怎么连过去的那一句（「经跳板 devbox · 经代理」；多跳外到内「经跳板 a → b」）：跳板照配置，经代理连上了才知道。
 * 直连又不经代理 ⇒ 省略。界面有就摆一行，不拼字；每条连接的读数不上卡头，在 `detail` 末尾「连接：」那一段。
 */
route?: string, 
/**
 * 机器条上那一行（缺 ⇒ 不进条：连着的 · 比这一版新的 · 停用的 · 还不知道的）。桌面与手机的条都只照这一格画。
 */
strip?: MachineStripLine, };
type MachineStateKind = "up" | "connecting" | "installing" | "updating" | "down" | "host_key_changed" | "needs_update" | "newer" | "disabled" | "unsupported" | "unknown";
type MachineStripLine = { tone: StripTone, progress: boolean, };
type MenuAct = "open_terminal" | "new_session" | "files" | "machine_status" | "add_machine";
type MenuAction = { act: MenuAct, sub: string | null, blocked: string | null, note: string | null, };
type Origin = string;
type StripTone = "busy" | "offline" | "warn" | "fault";
type VersionRelation = "same" | "older" | "newer" | "incomparable";
```

码：`bad_args` · `no_such_machine`

#### `machine-wake`

叫醒那台的常驻流（不再停着、断着就马上再接）。

收 `args` · 可撤 · 只在流上

入参 `MachineWakeReq` · 回包 `MachineState`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type DialWhy = "resolve" | "unreachable" | "timeout" | "host_key" | "auth" | "password" | "key_unreadable" | "jump" | "session_denied" | "other";
type MachineFix = "retry" | "conn_settings" | "push_key" | "compare_fingerprint" | "update" | "connect" | "install";
type MachineFixButton = { fix: MachineFix, label: string, };
type MachineStage = "deploy" | "attach";
type MachineState = { 
/**
 * 给人看的那台的名字（问题行那一句里说的就是它）。
 */
name: string, state: MachineStateKind, 
/**
 * 算不算离线（页头「N 台离线」数的就是这一格）：没连上 · 指纹不对 · 做不了 ⇒ 是；登录过了、只是拒开会话 ⇒ 不是。
 */
offline: boolean, 
/**
 * 没连上的原因码（`down` · `host_key_changed` 才有）。
 */
reason: DialWhy | null, 
/**
 * 正在连时到哪一步。
 */
stage: MachineStage | null, 
/**
 * 那台 cc-monitor 的产品版本号：与这一版同一份字节时就是这一版的号；别的报不出 ⇒ 空。
 */
version: string | null, 
/**
 * 与这一版的版本关系（握手过才有）。
 */
versionRelation: VersionRelation | null, 
/**
 * 那台的系统（部署问到过才有）。
 */
os: string | null, 
/**
 * 名字旁那个词（连着时空）。
 */
word: string, 
/**
 * 问题行那一句（空 ⇒ 不出问题行）。
 */
said: string, 
/**
 * 那台离线那一句（恒有，连接层写的；哪一态都带着）。出口要说那台离线时照摆这一句，不各自取字。
 */
offlineSaid: string, fixes: Array<MachineFixButton>, 
/**
 * 指纹不对时那台这一次出示的主机指纹（`host_key_changed` 才有）。
 */
seenHostKey: string | null, 
/**
 * 指纹不对时库里记下的那一枚（`host_key_changed` 才有；库里没有同类型的 ⇒ 空串）。拨号核心说的，不在这里算。
 */
expectedHostKey: string | null, 
/**
 * 复制详情：这一轮没连上 / 做不了时那一份（界面照原样跟在问题行那句后面）；连着时末尾接每条连接那一段（卡头 ⋯ 里复制）。
 */
detail: string | null, 
/**
 * 到这台怎么连过去的那一句（「经跳板 devbox · 经代理」；多跳外到内「经跳板 a → b」）：跳板照配置，经代理连上了才知道。
 * 直连又不经代理 ⇒ 省略。界面有就摆一行，不拼字；每条连接的读数不上卡头，在 `detail` 末尾「连接：」那一段。
 */
route?: string, 
/**
 * 机器条上那一行（缺 ⇒ 不进条：连着的 · 比这一版新的 · 停用的 · 还不知道的）。桌面与手机的条都只照这一格画。
 */
strip?: MachineStripLine, };
type MachineStateKind = "up" | "connecting" | "installing" | "updating" | "down" | "host_key_changed" | "needs_update" | "newer" | "disabled" | "unsupported" | "unknown";
type MachineStripLine = { tone: StripTone, progress: boolean, };
type MachineWakeReq = { machine: string, };
type StripTone = "busy" | "offline" | "warn" | "fault";
type VersionRelation = "same" | "older" | "newer" | "incomparable";
```

码：`bad_args` · `unknown_machine` · `disabled` · `busy`

失败带 `data` 的码：`busy` ⇒ `MachineState`

#### `machine-stop`

停那台的常驻流、再停那台的常驻后端。

收 `args` · 可撤 · 只在流上

入参 `MachineStopReq` · 回包 `MachineStopped`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type MachineStopReq = { machine: string, };
type MachineStopped = { stopped: string, pid: number | null, };
```

码：`bad_args` · `stop_failed`

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
type PubkeyPushReq = { 
/**
 * 那台的配置（设置页表单里那台，可能还没保存）。
 */
machine: Host, 
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

入参 `ExitPolicyReadReq` · 回包 `ExitPolicy`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExitPolicy = { 
/**
 * `absent`（没设过）· `chosen` · `unreadable`（读不出：也照答，不是失败）—— 后端 `exit_policy::Read::state` 那三个词。
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
type ExitPolicyReadReq = Record<symbol, never>;
```

码：`bad_args`

#### `exit-policy-set`

写「退出行为」那个值，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --exit-policy-set`

入参 `ExitPolicySetReq` · 回包 `ExitPolicy`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ExitPolicy = { 
/**
 * `absent`（没设过）· `chosen` · `unreadable`（读不出：也照答，不是失败）—— 后端 `exit_policy::Read::state` 那三个词。
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
type ExitPolicySetReq = { killOnExit: boolean, };
```

码：`bad_args` · `io_failed`

#### `prefs-read`

读这台的偏好。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --prefs-read`

入参 `NoArgs` · 回包 `PrefsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileState = "absent" | "ok" | "unreadable";
type NoArgs = Record<symbol, never>;
type Prefs = { 
/**
 * 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
 */
launchers: { [key in string]: string }, 
/**
 * 恢复会话不说放在哪时落在哪：托管 · tmux · 照那条会话起时那一种。
 */
resumeWhere: ResumeWhere, 
/**
 * 这台每个号默认用哪个模型（号 → 模型，原值）。
 */
models: { [key in string]: string }, 
/**
 * 从这台起新会话默认用哪个权限档（哪一家 → 那一家声明的档的原值）；恢复不读它。
 */
permissionModes: { [key in string]: string }, 
/**
 * 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
 */
contextLimits: { [key in string]: number }, 
/**
 * 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
 * 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
 */
agentHomes: { [key in string]: string }, 
/**
 * planned-build 的插件根目录（绝对路径）；设了 ⇒ 计划那一族只认它，没设 ⇒ 认这台装着的插件。用的那一刻现读。
 */
pbDir: string | null, 
/**
 * 子会话（在会话里用 ccm 起的）进不进标签页；缺省隐藏。
 */
childTab: Tab, 
/**
 * 分身会话（那一家在后台起的主会话副本）进不进标签页；缺省显示。
 */
bgTab: Tab, };
type PrefsReply = { 
/**
 * `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: FileState, 
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
type ResumeWhere = "managed" | "tmux" | "as_started";
type Tab = "show" | "hide";
```

码：`bad_args`

#### `prefs-set`

改这台的偏好，写完读回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --prefs-set`

入参 `PrefsSet` · 回包 `PrefsReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileState = "absent" | "ok" | "unreadable";
type Prefs = { 
/**
 * 每一家（线上的 kind）用哪条启动命令（原值）；没有那一家 ⇒ 那一家自己的默认启动器。
 */
launchers: { [key in string]: string }, 
/**
 * 恢复会话不说放在哪时落在哪：托管 · tmux · 照那条会话起时那一种。
 */
resumeWhere: ResumeWhere, 
/**
 * 这台每个号默认用哪个模型（号 → 模型，原值）。
 */
models: { [key in string]: string }, 
/**
 * 从这台起新会话默认用哪个权限档（哪一家 → 那一家声明的档的原值）；恢复不读它。
 */
permissionModes: { [key in string]: string }, 
/**
 * 上下文上限表：模型名子串（大小写不敏感，最长匹配胜）→ 上限 tokens。
 */
contextLimits: { [key in string]: number }, 
/**
 * 每一家（线上的 kind）的数据目录（绝对路径）；没有那一家 ⇒ 照那一家自己的解析（环境变量 → 默认目录）。
 * 后端起来时读一次（[`apply_agent_homes`]），改了要重启后端才生效。
 */
agentHomes: { [key in string]: string }, 
/**
 * planned-build 的插件根目录（绝对路径）；设了 ⇒ 计划那一族只认它，没设 ⇒ 认这台装着的插件。用的那一刻现读。
 */
pbDir: string | null, 
/**
 * 子会话（在会话里用 ccm 起的）进不进标签页；缺省隐藏。
 */
childTab: Tab, 
/**
 * 分身会话（那一家在后台起的主会话副本）进不进标签页；缺省显示。
 */
bgTab: Tab, };
type PrefsReply = { 
/**
 * `absent`（没设过）· `ok` · `unreadable`（读不懂：也照答，不是失败）。
 */
state: FileState, 
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
launchers?: { [key in string]: string }, 
/**
 * 恢复时放在哪（三选）。
 */
resumeWhere?: ResumeWhere, 
/**
 * 某几个号的模型：`{号: 模型 | null}`，`null` ⇒ 删掉那一条。只动点到的那几个号。
 */
models?: { [key in string]: string | null }, 
/**
 * 某几家新会话的权限档：`{kind: 档 | null}`，`null` ⇒ 删掉那一条。写的时候核在那一家的清单里。
 */
permissionModes?: { [key in string]: string | null }, 
/**
 * 整张上限表（整张换掉）。
 */
contextLimits?: { [key in string]: number }, 
/**
 * 某一家的数据目录：`{kind: 路径}`，空串 ⇒ 删掉那一家（回到它自己的解析）。只动点到的那几家；改了要重启后端才生效。
 */
agentHomes?: { [key in string]: string }, 
/**
 * 子会话进不进标签页。
 */
childTab?: Tab, 
/**
 * 分身会话进不进标签页。
 */
bgTab?: Tab, 
/**
 * pb 的插件根目录：空串 ⇒ 删掉（回到认装着的插件）。
 */
pbDir?: string, };
type ResumeWhere = "managed" | "tmux" | "as_started";
type Tab = "show" | "hide";
```

码：`bad_args` · `unreadable` · `io_failed`

#### `machines-read`

读这一端的机器表。

不收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machines-read`

入参 `NoArgs` · 回包 `MachinesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileState = "absent" | "ok" | "unreadable";
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
type MachineRow = { 
/**
 * 那台的名字（寻址用的那一个：`label`，空 ⇒ `host`，都去掉首尾空白）。
 */
name: string, 
/**
 * 那一台原样。
 */
host: Host, };
type MachinesReply = { 
/**
 * 没有这份 · 读得懂 · 读不懂（读不懂也照答，不是失败）。
 */
state: FileState, 
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
 * 照用的那一张：一台一行，带后端算好的名字（`{name, host}`；名字的算法只在拨号核心 `Host::name`，界面 · 壳都不再算）。
 */
hosts: Array<MachineRow>, 
/**
 * 每台记下的主机指纹（名字 → 指纹，库里没有的那台不出现；拨号核心那份库现读）。
 */
hostKeys: { [key in string]: Array<string> }, };
type NoArgs = Record<symbol, never>;
```

码：`bad_args`

#### `machines-try`

这串补丁若现在落盘，机器表那一道过不过（盘上不动）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --machines-try`

入参 `MachinesEdits` · 回包 `MachinesTried`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type ElemKey = { fields: Array<string>, equals: string, };
type MachineEdit = { "op": "setin", where: Array<ElemKey>, field: string, value: unknown, } | { "op": "insertin", where: Array<ElemKey>, value: unknown, } | { "op": "removein", where: Array<ElemKey>, };
type MachineFault = { 
/**
 * `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名 ·
 * `leading_dash` 地址 / 用户以 `-` 打头（开终端时会被 ssh 当成选项）。
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
type MachineFaultCode = "no_host" | "no_user" | "port" | "name_taken" | "leading_dash";
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
type FileState = "absent" | "ok" | "unreadable";
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
type MachineEdit = { "op": "setin", where: Array<ElemKey>, field: string, value: unknown, } | { "op": "insertin", where: Array<ElemKey>, value: unknown, } | { "op": "removein", where: Array<ElemKey>, };
type MachineFault = { 
/**
 * `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名 ·
 * `leading_dash` 地址 / 用户以 `-` 打头（开终端时会被 ssh 当成选项）。
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
type MachineFaultCode = "no_host" | "no_user" | "port" | "name_taken" | "leading_dash";
type MachineRow = { 
/**
 * 那台的名字（寻址用的那一个：`label`，空 ⇒ `host`，都去掉首尾空白）。
 */
name: string, 
/**
 * 那一台原样。
 */
host: Host, };
type MachinesEdits = { edits: Array<MachineEdit>, };
type MachinesReply = { 
/**
 * 没有这份 · 读得懂 · 读不懂（读不懂也照答，不是失败）。
 */
state: FileState, 
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
 * 照用的那一张：一台一行，带后端算好的名字（`{name, host}`；名字的算法只在拨号核心 `Host::name`，界面 · 壳都不再算）。
 */
hosts: Array<MachineRow>, 
/**
 * 每台记下的主机指纹（名字 → 指纹，库里没有的那台不出现；拨号核心那份库现读）。
 */
hostKeys: { [key in string]: Array<string> }, };
type MachinesSet = { machines: MachinesReply, };
```

码：`bad_args` · `unreadable` · `element_gone` · `element_exists` · `bad_machine` · `io_failed`

失败带 `data` 的码：`element_gone` ⇒ `Gone` · `bad_machine` ⇒ `MachineFault`

#### `host-keys-forget`

忘掉一台记下的主机指纹，读回机器表、推 `changed {machines}`。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --host-keys-forget`

入参 `HostKeysForgetReq` · 回包 `MachinesReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type FileState = "absent" | "ok" | "unreadable";
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
type HostKeysForgetReq = { machine: Origin, };
type MachineRow = { 
/**
 * 那台的名字（寻址用的那一个：`label`，空 ⇒ `host`，都去掉首尾空白）。
 */
name: string, 
/**
 * 那一台原样。
 */
host: Host, };
type MachinesReply = { 
/**
 * 没有这份 · 读得懂 · 读不懂（读不懂也照答，不是失败）。
 */
state: FileState, 
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
 * 照用的那一张：一台一行，带后端算好的名字（`{name, host}`；名字的算法只在拨号核心 `Host::name`，界面 · 壳都不再算）。
 */
hosts: Array<MachineRow>, 
/**
 * 每台记下的主机指纹（名字 → 指纹，库里没有的那台不出现；拨号核心那份库现读）。
 */
hostKeys: { [key in string]: Array<string> }, };
type Origin = string;
```

码：`bad_args` · `unreadable` · `element_gone` · `io_failed`

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

#### `drift-merge`

本机与各台的漂移账。

不收 `args` · 可撤 · CLI：`ccm -- --drift-merge`

入参 `NoArgs` · 回包 `DriftMerge`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

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
type DriftMerge = { machines: Array<DriftOn>, };
type DriftOn = { 
/**
 * 那台的键（本机 `<local>`，别的台是机器表里的名字）。
 */
origin: string, report: DriftReport | null, };
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
 * 那台的名字。
 */
origin: Origin, 
/**
 * 绑本机 `127.0.0.1:localPort`：用户敲进去的原文，这里解（界面不判是不是端口）。
 */
localPort: string, 
/**
 * 每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip。
 */
remoteHost: string, 
/**
 * 同 `localPort`：原文。
 */
remotePort: string, };
type ForwardStarted = { 
/**
 * `fwd-<n>`，本进程内单调。
 */
id: string, };
type Origin = string;
```

码：`bad_args` · `bad_spec` · `unreachable` · `bad_jump` · `full` · `failed`

#### `remote-probe`

测试连接。

收 `args` · 连接内就地做完 · 只在流上

入参 `ProbeReq` · 回包 `null`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

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
type ProbeReq = { 
/**
 * 1..=64 个 `[A-Za-z0-9-]`；进度帧 `probe` 原样回填。
 */
ticket: string, 
/**
 * 那台的配置。
 */
machine: Host, };
```

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
 * 那份文件的总大小，写好的字（`copy_core::size_text`）；没装 ⇒ `null`。
 */
size_text: Words | null, 
/**
 * 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行。
 */
text: string, 
/**
 * 前面还有没回的字节。
 */
truncated: boolean, };
type Words = string;
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
type SshImported = { groups: Array<SshImportGroup>, 
/**
 * 有别名在期限内没读完（整批的期限用完 · 那一发卡住）：`groups` 只是读到的那一部分。
 */
partial: boolean, };
```

码：`bad_args`

### 4.11 计划

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
type PlanListSlice = { name: string, domain: string | null, current: boolean, progress: PlanProgress | null, needCount: number, error: string | null, stale: PlanStale | null, 
/**
 * 同那一片成品上的那一份（照抄）。
 */
said: PlanSliceSaid, };
type PlanListWorkspace = { workspace: string, repo: string | null, auto: boolean, rev: string, stale: PlanStale | null, needCount: number, bySession: { [key in string]: PlanSessionBlock }, slices: Array<PlanListSlice>, };
type PlanPb = { state: "ok" | "missing" | "unsupported", said: string | null, version?: string, };
type PlanProgress = { done: number, of: number, };
type PlanSessionBlock = { slice: string, block: string, 
/**
 * 根节点（编号；点会话头那一枚标选中它）。
 */
cell: string | null, 
/**
 * 根节点的标题；最上面那一个 ⇒ `null`。
 */
title: string | null, top: boolean, phase: string | null, at: string | null, atTitle: string | null, via: "session" | "subagent", said: PlanSessionSaid, };
type PlanSessionSaid = { 
/**
 * 块名：同那一块的 [`PlanBlockSaid::name`]。
 */
name: string, 
/**
 * 会话头那一枚标：片 · 块名 · 阶段。
 */
mark: string, };
type PlanSliceSaid = { 
/**
 * 读不成 ＋ 过期那一句：整份读不成在先，这一片读不成在后；都没有 ⇒ `null`。
 */
strip: string | null, 
/**
 * 这一片读不成、只剩名字那一句；读得成 ⇒ `null`。
 */
bare: string | null, 
/**
 * 读到的时刻那一句；时刻的字还没写 ⇒ `null`。
 */
readAt: string | null, 
/**
 * 顶层签了几格（总览头）；没有收的值 ⇒ `null`。
 */
top: string | null, 
/**
 * 大纲根那一行右端的「几/几」；没有收的值 ⇒ `null`。
 */
root: string | null, };
type PlanStale = { said: string | null, raw: string | null, since: number, sinceText?: string, };
```

码：`bad_args`

#### `plan-read`

一个工作区的成品（几个仓库的图 · 状态 · 权力范围 · 校验；负责人与 signer 对到会话；不带 agent_view）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-read`

入参 `PlanReadReq` · 回包 `PlanRead`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type PlanArchived = { id: string, title: string | null, kind: string | null, replacedBy: string | null, };
type PlanBlock = { id: string, 
/**
 * 根节点（最上面那一个 ⇒ `project`）。
 */
cells: Array<string>, 
/**
 * 根节点底下算活的直接子签收了几格；根节点不在片里 · 底下没有算活的 ⇒ `null`。
 */
inside: PlanKids | null, dir: string | null, owner: PlanWho | null, phase: string | null, 
/**
 * 站在哪一格（编号）；`null` ⇒ 起点。
 */
at: string | null, 
/**
 * 划出它的那一项还在（false ⇒ 删了，下一趟 update 并回）。
 */
listed: boolean, 
/**
 * 交接说明（pb 原样，不带认领码：码的位置写着 `<认领码>`，句末说码在哪拿）；给不了 ⇒ `null`。
 */
handoff: string | null, 
/**
 * 为什么给不了；给了 ⇒ `null`。
 */
handoffError: string | null, 
/**
 * 「新会话接手」那一格（后端判，界面照画；字在 `handoff` · `handoffError`）。
 */
takeover: PlanTakeover, 
/**
 * 接手的会话此刻在哪几组（帧面答之前现标，见 后端 `plan::groups`；pb 不知道有组）。
 */
groups: PlanBlockGroups, said: PlanBlockSaid, };
type PlanBlockGroups = { 
/**
 * 组模块答了 ⇒ 那条会话在的几组（照组的先后；不在组里 ⇒ 空）；没有接手会话 · 组模块没答 ⇒ 空。
 */
groups: Array<PlanGroup>, 
/**
 * 那一格的字：组名照排；一组都没有（不在组里 · 没有接手会话 · 没答）⇒「—」。
 */
text: string, 
/**
 * 组模块没答 ⇒ 那一句（「读取组失败 · …」，悬停看）；答了 · 不用问 ⇒ `null`。
 */
unread: string | null, };
type PlanBlockSaid = { 
/**
 * 块名：最上面那一块 ⇒ 顶层那一句；否则根节点的标题（没有 ⇒ 根节点编号 ⇒ 块编号）。
 */
name: string, 
/**
 * 根节点底下签了几格（块表「里面」那一格 · 手机此会话那一块）；`inside` 为 `null` ⇒ `null`。
 */
inside: string | null, };
type PlanCell = { id: string, title: string | null, kind: string | null, body: string | null, parent: string | null, children: Array<string>, edges: PlanEdges, pointedBy: PlanEdges, files: Array<PlanFile>, 
/**
 * 状态里记着的值，一个值一项带笔数（pb 原样，照类上登的先后）。
 */
state: Array<PlanState>, 
/**
 * 状态那一行字（「已签收 2 · decided」，同 `pb render`）；什么都没记 ⇒ `null`。
 */
stateLine: string | null, 
/**
 * 依据它的是哪几格（pb 给的）。
 */
evidencedBy: Array<string>, 
/**
 * 算活的：记着收的值 ⇒ `done`，没记 ⇒ `open`；类不许记收的值 ⇒ `null`（不进任何分母）。
 */
statusCode: "done" | "open" | null, 
/**
 * 类是待定、还没记 decided。
 */
undecided: boolean, 
/**
 * 画哪一种（图标 · 颜色）：照上面两格判好，两端照它挑。
 */
look: PlanLook, 
/**
 * 行右端那一句：状态那一行 ＋ 原因短词，用分隔连好；都没有 ⇒ `null`。
 */
end: string | null, 
/**
 * 状态那一节什么都没有（没记值、也没有指着它没执行的 sets）时那一句：算活、底下有格 ⇒ 跟着底下走；别的 ⇒ 没记状态。
 * 有东西可摆 ⇒ `null`。
 */
quiet: string | null, 
/**
 * 没签收（或待定没人定）的原因；签收了 · 不算活 ⇒ `null`。
 */
whyCode: PlanWhyCode | null, 
/**
 * 行尾那一个原因短词；没有原因 ⇒ `null`。
 */
whyShort: PlanWhyShort | null, 
/**
 * 它自己写的、还没执行的 sets。
 */
sets: Array<PlanSetsTo>, 
/**
 * 在它底下加节点要在哪个权力范围里做（pb 的 `children_in`；最上面那一个 ⇒ `project`）。
 */
block: string | null, 
/**
 * 那个权力范围的负责人（要改它，该说给的那一位）。
 */
owner: PlanWho | null, 
/**
 * 写得动它元数据的那一位（pb 的 `signer`）。
 */
signer: PlanWho | null, 
/**
 * 它写的 covers（格的编号或 `project`）。
 */
covers: Array<string>, 
/**
 * covers 管着它的那几格（pb 算的）。
 */
coveredBy: Array<string>, refs: PlanRefs, 
/**
 * 退回过 ⇒ 那一次的状态；没有 ⇒ `null`。
 */
returned: PlanReturned | null, said: PlanCellSaid, };
type PlanCellSaid = { 
/**
 * 上面几层的「编号 标题」，从顶层往下连好；顶层 ⇒ `null`。
 */
trail: string | null, 
/**
 * 它是一块的根节点 ⇒ 那一句（插它的「编号 标题」）；不是 ⇒ `null`。
 */
blockRoot: string | null, };
type PlanCheck = { unreadable: Array<PlanReadError>, 
/**
 * 元数据坏了、编号还认得出的节点：判不了，不在 `cells` 里。
 */
unparsed: Array<string>, red: Array<PlanRed>, undecidable: Array<PlanUndecidable>, };
type PlanDomain = { name: string | null, error: string | null, kinds: Array<PlanKind>, phases: Array<PlanPhase>, };
type PlanEdgeWords = { to: string | null, with: string | null, after: string | null, replaces: string | null, };
type PlanEdges = { to: Array<string>, with: Array<string>, after: Array<string>, replaces: Array<string>, };
type PlanFile = { path: string, 
/**
 * 对账的码；pb 给了认不得的 ⇒ `null`。
 */
stateCode: "ok" | "missing" | "empty" | "broken" | null, 
/**
 * pb 的那一句（长度 · 坏在哪）。
 */
note: string | null, 
/**
 * 文件核对那一格写的字（在 · 缺 · 空 · 坏，后端照码写好；认不出的码 ⇒ `null`）。
 */
checkText: string | null, 
/**
 * 同一份文件还挂在哪几格（编号）。
 */
alsoBy: Array<string>, };
type PlanGroup = { id: string, title: string, };
type PlanKids = { done: number, of: number, };
type PlanKind = { name: string | null, edgeWords: PlanEdgeWords, 
/**
 * 这一类许记的状态值（照表里的先后）。
 */
values: Array<string>, };
type PlanLook = "done" | "open" | "undecided" | "none";
type PlanNeed = { key: string, kind: "top" | "red" | "ended" | "ask", block: string | null, cell: string | null, 
/**
 * 那一条牵着的会话（问人 · 停了的那一个）。
 */
sid: string | null, 
/**
 * 不通过那一条在 `check.red` 里的位置。
 */
red?: number, acked: boolean, 
/**
 * 那一条的标题（后端照种类写好）。
 */
title: string, 
/**
 * 标题底下那几行（顶层签了几格 · pb 那一句与修法 · 底下签了几格 · 子 agent 由父会话收）；负责人那一枚由出口挂，不在这里。
 */
lines: Array<string>, 
/**
 * 「第几条 / 共几条」（照认可记录标过之后、没认可的那几条照先后写）；认可过的 ⇒ `null`。
 */
pos: string | null, };
type PlanPendingSet = { source: string | null, target: string | null, value: string | null, op: "add" | "remove", 
/**
 * pb 那一句。
 */
blockedReason: string | null, 
/**
 * pb 给程序的代号（`source_delegated` …）。
 */
blockedCode: string | null, };
type PlanPhase = { name: string | null, does: string | null, };
type PlanProgress = { done: number, of: number, };
type PlanRead = { pb: string | null, workspace: string, 
/**
 * 当前仓库（pb 的 `current_repo`）。
 */
repo: string | null, 
/**
 * 自动继续开没开（pb 的 `auto_continue`）。
 */
auto: boolean, 
/**
 * 能选的领域（`pb repo` 认得的每一档）。
 */
domains: Array<PlanDomain>, slices: Array<PlanSlice>, bySession: { [key in string]: PlanSessionBlock }, 
/**
 * 这一份输出的摘要：变了才算计划变了。
 */
rev: string, readAt: number, readAtText?: string, 
/**
 * 上次读数那一格（与 `readAtText` 同一把钟写）：那台离线时计划条照它说这一份是什么时候读到的。
 */
seen?: SeenAt, stale: PlanStale | null, needCount: number, };
type PlanReadError = { file: string | null, line: number | null, message: string | null, };
type PlanReadReq = { 
/**
 * 工作区根（`plan-list` 给的那个）。
 */
workspace: string, };
type PlanRed = { rule: string | null, what: string | null, 
/**
 * 该哪个权力范围改。
 */
block: string | null, fix: string | null, 
/**
 * `what` 里提到的格（UTF-16 下标）。
 */
refs: Array<PlanRef>, };
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
 * 根节点（编号；点会话头那一枚标选中它）。
 */
cell: string | null, 
/**
 * 根节点的标题；最上面那一个 ⇒ `null`。
 */
title: string | null, top: boolean, phase: string | null, at: string | null, atTitle: string | null, via: "session" | "subagent", said: PlanSessionSaid, };
type PlanSessionSaid = { 
/**
 * 块名：同那一块的 [`PlanBlockSaid::name`]。
 */
name: string, 
/**
 * 会话头那一枚标：片 · 块名 · 阶段。
 */
mark: string, };
type PlanSetsTo = { target: string | null, value: string | null, 
/**
 * 加一笔 · 去一笔。
 */
op: "add" | "remove", };
type PlanSlice = { name: string, domain: string | null, 
/**
 * 仓库相对工作区根在哪（pb 给的，照用；pb 没给 ⇒ `null`）。
 */
path: string | null, current: boolean, 
/**
 * 只做计划：人还没 `pb adopt`（`plan-command adopt` 接）。
 */
docsOnly: boolean, 
/**
 * 这一刻读不成的那一句（pb 原样）。
 */
error: string | null, stale: PlanStale | null, bare: boolean, kinds: Array<PlanKind>, phases: Array<PlanPhase>, 
/**
 * 收的值（领域表 update 规则加 · 去的那个，软件是「已签收」）；表里没有 ⇒ `null`（不出进度）。
 */
doneValue: string | null, top: Array<string>, 
/**
 * 顶层进度；没有收的值 ⇒ `null`。
 */
progress: PlanProgress | null, 
/**
 * 算活、还没签收的格数。
 */
openCells: number, blocks: Array<PlanBlock>, 
/**
 * 划出了、还没建的。
 */
unbuiltBlocks: Array<PlanUnbuilt>, 
/**
 * 无主文件（仓库读不出 ⇒ `paths` 为 null、`why` 说为什么）。
 */
unowned: PlanUnowned, 
/**
 * 还没执行的 sets（画虚线，为什么写在目标那一格上）。
 */
pendingSets: Array<PlanPendingSet>, check: PlanCheck, 
/**
 * 编号 ⇒「编号 标题」（图上每个节点，连排期的 ＋ 归档里的；后端拼，界面凡出现编号照它写）。
 */
labels: { [key in string]: string }, cells: Array<PlanCell>, archived: Array<PlanArchived>, needs: Array<PlanNeed>, 
/**
 * 这一片要你看的数（没认可的，不含问人）。
 */
needCount: number, 
/**
 * 最上面那个权力范围（`project`）退回过 ⇒ 那一次此刻的状态；没退回过 ⇒ `null`（缺 ⇒ 同）。
 */
returned: PlanReturned | null, said: PlanSliceSaid, };
type PlanSliceSaid = { 
/**
 * 读不成 ＋ 过期那一句：整份读不成在先，这一片读不成在后；都没有 ⇒ `null`。
 */
strip: string | null, 
/**
 * 这一片读不成、只剩名字那一句；读得成 ⇒ `null`。
 */
bare: string | null, 
/**
 * 读到的时刻那一句；时刻的字还没写 ⇒ `null`。
 */
readAt: string | null, 
/**
 * 顶层签了几格（总览头）；没有收的值 ⇒ `null`。
 */
top: string | null, 
/**
 * 大纲根那一行右端的「几/几」；没有收的值 ⇒ `null`。
 */
root: string | null, };
type PlanStale = { said: string | null, raw: string | null, since: number, sinceText?: string, };
type PlanState = { value: string, count: number, };
type PlanTakeover = "open" | "no_line" | "running" | "unsure" | "merging";
type PlanUnbuilt = { id: string | null, cells: Array<string>, };
type PlanUndecidable = { rule: string | null, why: string | null, };
type PlanUnowned = { paths: Array<string> | null, why: string | null, };
type PlanWho = { id: string, kind: "session" | "subagent" | "unknown", 
/**
 * 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。
 */
sid: string | null, 
/**
 * 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
 */
needs: string | null, 
/**
 * 此刻那条会话的整份脸（认不出 ⇒ 状态不明）。
 */
face: SessionFace, 
/**
 * 写好的那一个名字：子 agent ⇒「{父会话} 的 agent」；认不出 ⇒ 认不出那一句（id 截成前后几位）；
 * 会话本身 ⇒ `null`（两端写那条会话的名字）。
 */
label: string | null, };
type PlanWhyCode = { "kind": "nosign" } | { "kind": "inside", done: number, of: number, } | { "kind": "upper" } | { "kind": "undecided" };
type PlanWhyShort = { text: string, wait: boolean, };
type SeenAt = { 
/**
 * 记下的时刻（毫秒）。
 */
atMs: number, 
/**
 * 「采样于 12:03」（后端照文案表与那一台的钟写）。
 */
said: Words, };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `failed` · `no_pb` · `not_workspace` · `pb_unsupported`

#### `plan-cell-view`

一个节点的 agent 视角（agent 站在那个节点时 pb 印给它的那一段，原样；点到才起 `pb dump --node`）。

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

码：`bad_args` · `failed` · `no_pb` · `no_view` · `not_workspace` · `pb_unsupported`

#### `plan-command`

以人的身份代敲 pb 的命令：`continue` · `pause`（开关自动继续，管整个工作区）· `adopt`（接过 `slice` 那个仓库，不是当前仓库先切过去）· `render`（pb 画整张图写进系统临时目录，回页面路径）· `repo`（在 `workspace` 那个目录登记 `path` 那一片，带 `domain`；撞上已有的一片 ⇒ 只带名字切过去）；这一侧带着 agent 身份 ⇒ 用户命令不代敲。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-command`

入参 `PlanCommandReq` · 回包 `PlanCmdReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanAdopted = { slice: string, 
/**
 * 仓库相对工作区根在哪。
 */
path: string, 
/**
 * `true` ⇒ 先把当前仓库切成了它（pb 的 `adopt` 只接当前仓库）。
 */
switched: boolean, };
type PlanCmdReply = { rc: number, said: string | null, path: string | null, 
/**
 * `adopt` 才有：接过来的是哪一个仓库。
 */
adopted: PlanAdopted | null, };
type PlanCommandReq = { 
/**
 * 工作区根。
 */
workspace: string, 
/**
 * `continue` · `pause` · `adopt` · `render` · `repo`（`repo` 的 `workspace` 是新计划框里选的目录）。
 */
cmd: "continue" | "pause" | "adopt" | "render" | "repo", 
/**
 * `adopt` 接过哪一个仓库（仓库名）；别的动词不带。
 */
slice?: string | null, 
/**
 * `repo` 登记的仓库相对工作区根在哪；别的动词不带。
 */
path?: string | null, 
/**
 * `repo` 选的那一档领域（缺 ⇒ 不带）；别的动词不带。
 */
domain?: string | null, };
```

码：`bad_args` · `failed` · `no_pb` · `no_such_slice` · `not_workspace` · `pb_unsupported` · `refused`

#### `plan-new`

新计划框问的那一份：那个目录能选的领域（`pb dump --domains`）· pb 往上找到的工作区 · 仓库落在哪 · 路径最后一段撞上的那一片 · 要敲的那一行 `pb repo …`；pb 不在 · 太旧写在回包里。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-new`

入参 `PlanNewReq` · 回包 `PlanNewForm`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanDomain = { name: string | null, error: string | null, kinds: Array<PlanKind>, phases: Array<PlanPhase>, };
type PlanEdgeWords = { to: string | null, with: string | null, after: string | null, replaces: string | null, };
type PlanKind = { name: string | null, edgeWords: PlanEdgeWords, 
/**
 * 这一类许记的状态值（照表里的先后）。
 */
values: Array<string>, };
type PlanNewExisting = { slice: string, domain: string | null, };
type PlanNewForm = { 
/**
 * 找不到 pb ⇒ `missing`；pb 没有 `dump --domains` ⇒ `unsupported`（两样都带那一句）。
 */
pb: PlanPb, 
/**
 * pb 往上找到的工作区根；找不到 ⇒ `null`（`root` 就成工作区根）。
 */
workspace: string | null, 
/**
 * `pb repo` 在哪个目录敲（＝ `workspace`，没有 ⇒ 问的那个目录）。
 */
root: string, 
/**
 * 登记之后那一片叫什么（路径最后一段 ＝ 仓库名；撞上 ⇒ 那一片）；路径空 ⇒ `null`。
 */
slice: string | null, 
/**
 * 能选的几档（形状同 `PlanRead.domains`）。
 */
domains: Array<PlanDomain>, 
/**
 * 路径最后一段撞上的那一片（领域建了不改）。
 */
existing: PlanNewExisting | null, 
/**
 * 仓库会落在哪（绝对路径）；路径空 ⇒ `null`。
 */
repoDir: string | null, 
/**
 * 路径底下那一句（「仓库 …」· 撞上 ⇒「已有此片 · 软件 · 只切换」）；路径空 ⇒ `null`。
 */
note: string | null, 
/**
 * 要敲的那一行；路径空 ⇒ `null`。
 */
line: string | null, 
/**
 * 起 pb 没成（超时 · 读不懂 · 别的退出码）：那一句。
 */
failed: string | null, };
type PlanNewReq = { 
/**
 * 在哪个目录开（那台机器上的绝对路径；还不在也行）。
 */
dir: string, 
/**
 * 仓库相对工作区根在哪（最后一段是片名）；空 ⇒ 只问领域。
 */
path: string, 
/**
 * 选的那一档领域。
 */
domain?: string | null, };
type PlanPb = { state: "ok" | "missing" | "unsupported", said: string | null, version?: string, };
type PlanPhase = { name: string | null, does: string | null, };
```

码：`bad_args`

#### `plan-pb-check`

一个路径能不能当 pb 用（入口在 · 起得来），回给人看的那一句；设置里存 pb 路径之前问。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-pb-check`

入参 `PlanPbCheckReq` · 回包 `PlanPbCheck`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanPbCheck = { state: "ok" | "missing" | "no_entry" | "not_runnable", said: string, };
type PlanPbCheckReq = { 
/**
 * 要校验的插件根目录（那台机器上的绝对路径）。
 */
dir: string, };
```

码：`bad_args`

#### `plan-files`

文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb）。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-files`

入参 `PlanFilesReq` · 回包 `PlanFiles`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanFileDup = { id: string, 
/**
 * 「编号 标题」，后端拼好。
 */
label: string, title: string | null, };
type PlanFileOwner = { 
/**
 * 文件名（目录里的那一项）。
 */
name: string, 
/**
 * 作主的那一格（排期上先声明它的那一格）。
 */
id: string, 
/**
 * 「编号 标题」（没有标题 ⇒ 只写编号），后端拼好。
 */
label: string, title: string | null, 
/**
 * 类不许记收的值 ⇒ `null`（不算活）。
 */
statusCode: "done" | "open" | null, 
/**
 * 状态那一行字（后端照 pb 记着的值写好，如「已签收 2」）；什么都没记 ⇒ `null`。
 */
status: string | null, 
/**
 * 那一格住的那一块（块根格的标题）；住顶块 ⇒ `null`。
 */
block: string | null, 
/**
 * 对账码；认不出 ⇒ `null`。
 */
fileState: "ok" | "missing" | "empty" | "broken" | null, fileNote: string | null, 
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
 * 这个目录里无主的那几份（文件名）；不在任何一片里 · 那一片读不成 · 仓库读不出 ⇒ `null`。
 */
unowned: Array<string> | null, 
/**
 * 仓库读不出、判不了无主的那一句（pb 原样）；判得了 ⇒ `null`。
 */
unownedWhy: string | null, };
type PlanFilesReq = { 
/**
 * 那台机器上的绝对路径。
 */
dir: string, };
```

码：`bad_args`

#### `plan-refs`

会话消息里提到的格：照会话接手的那块（否则工作目录所在的那片）认片，几段正文一次交 pb 的 `dump --refs`，回每段的几处（UTF-16 下标 ＋ 跟在编号后面的标题）；认不出片 · pb 不在 ⇒ 每段空。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-refs`

入参 `PlanRefsReq` · 回包 `PlanMsgRefs`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanMsgRef = { start: number, end: number, ids: Array<string>, label: string, };
type PlanMsgRefs = { workspace: string | null, slice: string | null, refs: Array<Array<PlanMsgRef>>, };
type PlanRefsReq = { sid: string, texts: Array<string>, };
```

码：`bad_args`

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

把人的话投给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，认不出 ⇒ 只给复制，已结束 ⇒ 只给复制或（`resume`）照投（投递时在这台恢复它），能投就投进它的输入队；排进去了记一条已退回。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-return`

入参 `PlanReturnReq` · 回包 `PlanReturnReply`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type Allow = { "can": "yes" } | { "can": "no", why: Words, };
type CanClose = { "can": "quiet" } | { "can": "menu" } | { "can": "no", why: Words, };
type CanSay = { "can": "yes", hint: Words, } | { "can": "no", why: Words, };
type CanTerm = { "can": "yes", host: Launch, } | { "can": "no", why: Words, };
type Launch = "managed" | "tmux" | "direct";
type Life = { "is": "live", activity?: WireSessionActivity, } | { "is": "reconnectable" } | { "is": "ended", by?: string, } | { "is": "gone" } | { "is": "unclear" };
type PlanReturnReply = { line: string, to: PlanWho | null, 
/**
 * `queued`（投进那条会话的输入队、那台已落盘；投递在它能收时送，已结束的先恢复它）· `copy`（投不了，只给这一行去复制）。
 */
result: "queued" | "copy", 
/**
 * `copy` 的原因：`unknown`（认不出接手）· `ended`（已结束、没点恢复）。
 */
why: string | null, said: string | null, };
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
 * 自报的前端（投进队的那一条记作发自它；投递时过 `terminal-input` 那一道身份门）。
 */
client?: string | null, 
/**
 * `true` ⇒ 目标会话已结束时照投（投递时在这台恢复它、这一行当它的第一句；人点了［恢复并退回］）。
 */
resume?: boolean | null, };
type PlanWho = { id: string, kind: "session" | "subagent" | "unknown", 
/**
 * 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。
 */
sid: string | null, 
/**
 * 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
 */
needs: string | null, 
/**
 * 此刻那条会话的整份脸（认不出 ⇒ 状态不明）。
 */
face: SessionFace, 
/**
 * 写好的那一个名字：子 agent ⇒「{父会话} 的 agent」；认不出 ⇒ 认不出那一句（id 截成前后几位）；
 * 会话本身 ⇒ `null`（两端写那条会话的名字）。
 */
label: string | null, };
type SessionCan = { 
/**
 * 关掉它的标签页。
 */
close: CanClose, 
/**
 * 恢复（起一个新进程接着那份记录）。
 */
resume: Allow, 
/**
 * 还有终端可去（拉前 · 停 · 杀 · 终端页）。
 */
term: CanTerm, 
/**
 * 程序退了、tmux 会话还在 ⇒ 接回。
 */
reattach: Allow, 
/**
 * 换模型（`session-model`）。
 */
model: Allow, 
/**
 * 切权限档（`session-permission`）。
 */
permission: Allow, 
/**
 * 停下这一轮（往那条会话送 Esc）：只在一轮在跑时能。
 */
stop: Allow, 
/**
 * 说一句（消息流底下那个输入框，`session-queue-put`）：在终端宿主里活着 · 那台看不见（成待发）· 已结束能恢复（先恢复再说）⇒ 能、
 * `hint` 是框里那一句占位；不在任何宿主里（非 cc-monitor 起的）· 程序退了 · 记录已不在 · 说不清 ⇒ 不能、`why` 写好。界面照摆，不另判。
 */
say: CanSay, 
/**
 * 结束它（右键 · 多选 · 终端名单 · 占位同一个确认框，交 `sessions-stop`）：活着且住在终端宿主里（住哪判不了 ⇒ 交了由那台认）·
 * 程序退了 tmux 会话还在 ⇒ 能；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、`why` 写好。
 */
end: Allow, };
type SessionDot = "running" | "needs-you" | "background" | "idle" | "exited" | "ended" | "gone" | "unknown";
type SessionFace = { dot: SessionDot, 
/**
 * 这条会话的一生（在干什么只住在「活」那一支里）。
 */
life: Life, 
/**
 * 那台看不见时照旧作数（已结束 · 记录已不在）。
 */
settled: boolean, 
/**
 * 短名（运行中 · 需手动 · 已结束 …）。
 */
name: Words, 
/**
 * 悬停那一句。
 */
tip: Words, 
/**
 * 没有会话事实时的状态一句。
 */
line: Words, tone: Tone, 
/**
 * 监控板 · 组内的序（小的在前）：等人 0 · 在干活 1 · 后台 2 · 闲着 / 说不清 3 · Claude 已退出 4 · 已结束 / 记录已不在 5。
 */
order: number, can: SessionCan, 
/**
 * 跑在账号库里哪个号下：那台后端宣告时读会话进程环境判一次，这条会话之后的脸（含离开「活」的那几张）都带它。
 * 那一家没有账号库 · 判不了 · 冷读出来的脸（清单行 · 需手动行 · 血缘）⇒ 不上线。
 */
account?: string, };
type Tone = "plain" | "fail" | "now" | "need" | "warn" | "busy";
type WireSessionActivity = "working" | "needs_you" | "idle" | "background_work";
type Words = string;
```

码：`bad_args` · `attachment_gone` · `child_timed_out` · `failed` · `io_failed` · `no_pb` · `no_such_cell` · `no_target` · `not_workspace` · `pb_unsupported` · `queue_full` · `queue_unreadable` · `review_unreadable` · `text_too_long`

#### `plan-ask`

问图开框前那一问：后端拼开口那句（别名的那句 ＋「看 {编号}「{标题}」：」），别名预选这台记着的问图用的那条；给了别名 ⇒ 记成问图用的。

收 `args` · 撤不动（阻塞档，`cancel` 回 `not_cancellable`） · CLI：`ccm -- --plan-ask` · 没有 tmux 的机器上做不到（hello `unavailable` 会列它）

入参 `PlanAskReq` · 回包 `PlanAsk`（类型从 Rust 生成，`generated/TypedCommands.ts`）：

```ts
type PlanAsk = { 
/**
 * 框顶那一句（「问图 · {标题}」）。
 */
title: string, 
/**
 * 预选的别名（这一问给的，或记着的那条）；没有 ⇒ `null`（框里那一格空着，选了才能起）。
 */
alias: string | null, 
/**
 * 开口那句：别名的那句 ＋ 换行 ＋「看 {编号}「{标题}」这一格：」；没有别名 / 别名没写 ⇒ 只有后一半。
 */
line: string, 
/**
 * 起在哪那一句（「tmux 里 · 号 lab（来自 look）· 进分组「看图」」，照别名合下来的写）；没有别名 ⇒ `null`。
 */
runs: string | null, };
type PlanAskReq = { 
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
id: string, 
/**
 * 改选的那条别名（记成这台问图用的）；缺 ⇒ 用记着的。
 */
alias?: string | null, };
```

码：`bad_args` · `child_timed_out` · `failed` · `io_failed` · `no_pb` · `no_such_alias` · `no_such_cell` · `no_tmux` · `not_workspace` · `pb_unsupported` · `refused` · `review_unreadable`

## 5. CLI 子命令

一次性调用：`ccm -- --子命令 …`（打头的 `--` 之后才归后端）；收 `args` 的那一段 JSON 从 stdin 读，或经 `--args-b64 <base64>` 走 argv（二选一，上限与码见 `IPC-PROTOCOL.md` §8），回一行 JSON 到 stdout。
「＝ 帧命令」的那几条与流上同名命令是同一个处理器。本表随二进制走的那一份：`ccm -- --commands`（同一个口，`stream/catalog.rs`）。

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
| `--accounts-set-default` | ＝ 帧命令 `accounts-set-default`：设默认号 |
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
| `--bookmarks-edit` | ＝ 帧命令 `bookmarks-edit`：切换 · 删 · 加几条书签，回改完的整份 |
| `--bookmarks-read` | ＝ 帧命令 `bookmarks-read`：这台的书签 |
| `--bus-commit` | ＝ 帧命令 `bus-commit`：推已读位置（比令牌） |
| `--bus-inbox` | ＝ 帧命令 `bus-inbox`：只读看一个 agent 收件箱的尾巴 |
| `--bus-list` | ＝ 帧命令 `bus-list`：谁在线 + 各自待读多少 |
| `--bus-peek` | ＝ 帧命令 `bus-peek`：读新信（不推已读位置） |
| `--bus-send` | ＝ 帧命令 `bus-send`：发一封信：核信封、照规矩收进收件箱 |
| `--bus-unread` | ＝ 帧命令 `bus-unread`：未读几封 |
| `--cc-bus-install` | ＝ 帧命令 `cc-bus-install`：把这台二进制带着的 cc-bus 装到这台 |
| `--cc-bus-install-state` | ＝ 帧命令 `cc-bus-install-state`：装 cc-bus 到这台之前看一眼 |
| `--ccm-user-path` | ＝ 帧命令 `ccm-user-path`：`ccm` 所在目录在不在用户级 `PATH` 上 · 加 · 撤 |
| `--chores-mark` | ＝ 帧命令 `chores-mark`：记下「待办」里的一个选择 |
| `--chores-merge` | ＝ 帧命令 `chores-merge`：问到各台的「文件与数据」、件数合成一份 |
| `--commands` | 列全部命令与用途：stdout 一行 `{commands: [{cmd?, cli?, usage?, what, args?}]}`（`cmd` 帧命令名 · `cli` 子命令 · `what` 一句话 · `args` 收不收参数；只在流上的没有 `cli`，CLI 独有的没有 `cmd`） |
| `--data-report` | ＝ 帧命令 `data-report`：「文件与数据」那一份成品 |
| `--drift-merge` | ＝ 帧命令 `drift-merge`：本机与各台的漂移账 |
| `--drift-report` | ＝ 帧命令 `drift-report`：这台后端的漂移账 |
| `--exit-policy-read` | ＝ 帧命令 `exit-policy-read`：读「退出行为」那个值（值住后端所在那台） |
| `--exit-policy-set` | ＝ 帧命令 `exit-policy-set`：写「退出行为」那个值，写完读回 |
| `--ext-list-here` | ＝ 帧命令 `ext-list-here`：设置「扩展」页那张表的本机那一半（不问可达表） |
| `--ext-note-set` | ＝ 帧命令 `ext-note-set`：写 / 改 / 清一个扩展的备注 |
| `--ext-uninstall-apply` | ＝ 帧命令 `ext-uninstall-apply`：从这台卸一个扩展 |
| `--ext-uninstall-preview` | ＝ 帧命令 `ext-uninstall-preview`：从这台卸一个扩展之前那张卡 |
| `--files-batch-stop` | ＝ 帧命令 `files-batch-stop`：撤一组复制 · 删除 · 改权限 |
| `--files-chmod` | ＝ 帧命令 `files-chmod`：一组改成同一个 unix 权限位 |
| `--files-commit-text` | ＝ 帧命令 `files-commit-text`：按块读回、拼起来、覆盖写 |
| `--files-commit-upload` | ＝ 帧命令 `files-commit-upload`：把暂存区里一份传完的上传件挪进目标 |
| `--files-copy` | ＝ 帧命令 `files-copy`：同根内复制一组 |
| `--files-create` | ＝ 帧命令 `files-create`：在文件管理目标根底下新建一份此前不存在的文件 |
| `--files-delete` | ＝ 帧命令 `files-delete`：删一组（文件 · 空目录 · 显式递归的整棵） |
| `--files-delete-session` | ＝ 帧命令 `files-delete-session`：删一份历史会话 |
| `--files-extract` | ＝ 帧命令 `files-extract`：解压到一个新目录 |
| `--files-find` | ＝ 帧命令 `files-find`：在常驻索引里查 |
| `--files-grep` | ＝ 帧命令 `files-grep`：在一个目录底下按内容搜 |
| `--files-home` | ＝ 帧命令 `files-home`：后端这个用户的 home |
| `--files-index-rebuild` | ＝ 帧命令 `files-index-rebuild`：重建常驻文件名索引：走一遍，做完返回 |
| `--files-index-status` | ＝ 帧命令 `files-index-status`：索引的新鲜度 / 条目数 / 常驻字节 |
| `--files-ls` | ＝ 帧命令 `files-ls`：列一个目录的直接子项 |
| `--files-mkdir` | ＝ 帧命令 `files-mkdir`：新建一个目录 |
| `--files-read-chunk` | ＝ 帧命令 `files-read-chunk`：按字节寻址分块读回 |
| `--files-read-text` | ＝ 帧命令 `files-read-text`：读一份文本进编辑器 |
| `--files-rename` | ＝ 帧命令 `files-rename`：改名 / 同根内移动 |
| `--files-size` | ＝ 帧命令 `files-size`：算一个目录有多大 |
| `--files-stage-chunk` | ＝ 帧命令 `files-stage-chunk`：存盘的一块进暂存区 |
| `--files-stat` | ＝ 帧命令 `files-stat`：一个路径的元数据 |
| `--files-write-text` | ＝ 帧命令 `files-write-text`：覆盖写一份已经在的普通文件 |
| `--first-run` | ＝ 帧命令 `first-run`：首次运行「开始用」三步各自打没打勾 |
| `--footprint-report` | ＝ 帧命令 `footprint-report`：「足迹」由这台后端出整份成品 |
| `--groups-here` | ＝ 帧命令 `groups-here`：这台的组那一半：这台有组员的那几片 ＋ 带组或固定的会话 |
| `--groups-read` | ＝ 帧命令 `groups-read`：问到各台的组那一半、并成一份 |
| `--groups-write` | ＝ 帧命令 `groups-write`：建组 · 进出组 · 改名与解散扇出给各片 |
| `--history-annotate` | ＝ 帧命令 `history-annotate`：改一条历史注解 |
| `--history-branch` | ＝ 帧命令 `history-branch`：主线外清单 |
| `--history-browse` | ＝ 帧命令 `history-browse`：历史页那一列（几台并成一列） |
| `--history-facts` | ＝ 帧命令 `history-facts`：会话事实 |
| `--history-find` | ＝ 帧命令 `history-find`：会话内查找 |
| `--history-forget` | ＝ 帧命令 `history-forget`：删一条历史注解 |
| `--history-image` | ＝ 帧命令 `history-image`：按引用取回一张图 |
| `--history-index` | ＝ 帧命令 `history-index`：会话骨架索引 |
| `--history-lines` | ＝ 帧命令 `history-lines`：按行号取回一段 |
| `--history-list` | ＝ 帧命令 `history-list`：历史页的平铺会话清单 |
| `--history-listing` | ＝ 帧命令 `history-listing`：这台的清单（按记号只给变了的） |
| `--history-page` | ＝ 帧命令 `history-page`：按字节分页读，出记录行 |
| `--history-read` | ＝ 帧命令 `history-read`：按字节分页读一份会话 |
| `--history-record` | ＝ 帧命令 `history-record`：这条会话的记录还在不在 |
| `--history-run` | ＝ 帧命令 `history-run`：一个子运行的记录 |
| `--history-search` | ＝ 帧命令 `history-search`：全文搜索 |
| `--history-search-merge` | ＝ 帧命令 `history-search-merge`：问到各台搜一遍、合成一份 |
| `--history-tail` | ＝ 帧命令 `history-tail`：一份会话的尾段：手上那一份怎么接，连第一页正文 |
| `--history-turns` | ＝ 帧命令 `history-turns`：一轮的摘要 |
| `--history-user-inputs` | ＝ 帧命令 `history-user-inputs`：「人说过的话」清单 |
| `--hooks-diag` | ＝ 帧命令 `hooks-diag`：cc-bus 钩子诊断 |
| `--host-keys-forget` | ＝ 帧命令 `host-keys-forget`：忘掉一台记下的主机指纹，读回机器表、推 `changed {machines}` |
| `--last-seen-read` | ＝ 帧命令 `last-seen-read`：读离线那台的上次值 |
| `--last-seen-write` | ＝ 帧命令 `last-seen-write`：记下一台这一次读成的那一份（或清掉那台） |
| `--machine-find` | ＝ 帧命令 `machine-find`：打的字是不是恰好一台（名字 · 本机 · ssh 配置的 Host · 名字后跟第几条） |
| `--machine-interrupts` | ＝ 帧命令 `machine-interrupts`：停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前，会打断什么 |
| `--machines-read` | ＝ 帧命令 `machines-read`：读这一端的机器表 |
| `--machines-set` | ＝ 帧命令 `machines-set`：改机器表（按键认那一台），写完读回、推 `changed {machines}` |
| `--machines-try` | ＝ 帧命令 `machines-try`：这串补丁若现在落盘，机器表那一道过不过（盘上不动） |
| `--mcp-read` | ＝ 帧命令 `mcp-read`：这台机器的 MCP 列表成品 |
| `--mcp-sync-apply` | ＝ 帧命令 `mcp-sync-apply`：在要被写的那一台把那一条写进去 |
| `--mcp-sync-preview` | ＝ 帧命令 `mcp-sync-preview`：在要被写的那一台看装上之后那一条 |
| `--mcp-sync-source` | ＝ 帧命令 `mcp-sync-source`：装到别的机器时来源那一条 |
| `--monitor-board` | ＝ 帧命令 `monitor-board`：监控板（问到各台、合成一份：分组 · 组内先后 · 组头 · 汇总那一句） |
| `--monitor-board-here` | ＝ 帧命令 `monitor-board-here`：监控板这台那几条（序 · 活不活 · 在跑几个 agent；不认得的不答） |
| `--ping` | ＝ 帧命令 `ping`：问活：零载荷，回 `ok` |
| `--place-self` | 部署那一趟：暂存区里新传上去的这一份读自己的字节，自报是这一版才 `rename(2)` 盖掉落点 `~/.cc-monitor/bin/ccm`；回 `{placed: true, build_id}`；不在暂存区 ⇒ `outside_staging`；自报不是这一版 ⇒ `not_self`、暂存件当场删 |
| `--place-verdict` | ＝ 帧命令 `place-verdict`：本机那一份放不放 |
| `--plan-ack` | ＝ 帧命令 `plan-ack`：认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `changed {plan}` 带新的数 |
| `--plan-ask` | ＝ 帧命令 `plan-ask`：问图开框前那一问：后端拼开口那句（别名的那句 ＋「看 {编号}「{标题}」：」），别名预选这台记着的问图用的那条；给了别名 ⇒ 记成问图用的 |
| `--plan-cell-view` | ＝ 帧命令 `plan-cell-view`：一个节点的 agent 视角（agent 站在那个节点时 pb 印给它的那一段，原样；点到才起 `pb dump --node`） |
| `--plan-command` | ＝ 帧命令 `plan-command`：以人的身份代敲 pb 的命令：`continue` · `pause`（开关自动继续，管整个工作区）· `adopt`（接过 `slice` 那个仓库，不是当前仓库先切过去）· `render`（pb 画整张图写进系统临时目录，回页面路径）· `repo`（在 `workspace` 那个目录登记 `path` 那一片，带 `domain`；撞上已有的一片 ⇒ 只带名字切过去）；这一侧带着 agent 身份 ⇒ 用户命令不代敲 |
| `--plan-files` | ＝ 帧命令 `plan-files`：文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb） |
| `--plan-list` | ＝ 帧命令 `plan-list`：这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区） |
| `--plan-new` | ＝ 帧命令 `plan-new`：新计划框问的那一份：那个目录能选的领域（`pb dump --domains`）· pb 往上找到的工作区 · 仓库落在哪 · 路径最后一段撞上的那一片 · 要敲的那一行 `pb repo …`；pb 不在 · 太旧写在回包里 |
| `--plan-pb-check` | ＝ 帧命令 `plan-pb-check`：一个路径能不能当 pb 用（入口在 · 起得来），回给人看的那一句；设置里存 pb 路径之前问 |
| `--plan-read` | ＝ 帧命令 `plan-read`：一个工作区的成品（几个仓库的图 · 状态 · 权力范围 · 校验；负责人与 signer 对到会话；不带 agent_view） |
| `--plan-refs` | ＝ 帧命令 `plan-refs`：会话消息里提到的格：照会话接手的那块（否则工作目录所在的那片）认片，几段正文一次交 pb 的 `dump --refs`，回每段的几处（UTF-16 下标 ＋ 跟在编号后面的标题）；认不出片 · pb 不在 ⇒ 每段空 |
| `--plan-return` | ＝ 帧命令 `plan-return`：把人的话投给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，认不出 ⇒ 只给复制，已结束 ⇒ 只给复制或（`resume`）照投（投递时在这台恢复它），能投就投进它的输入队；排进去了记一条已退回 |
| `--plan-unack` | ＝ 帧命令 `plan-unack`：撤掉一条认可（没有也不算错）；推一帧 `changed {plan}` 带新的数 |
| `--powershell-policy-set` | ＝ 帧命令 `powershell-policy-set`：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned` |
| `--prefs-read` | ＝ 帧命令 `prefs-read`：读这台的偏好 |
| `--prefs-set` | ＝ 帧命令 `prefs-set`：改这台的偏好，写完读回 |
| `--profiles-bases` | ＝ 帧命令 `profiles-bases`：「基于」下拉能选的几段（选了不成圈） |
| `--profiles-from-line` | ＝ 帧命令 `profiles-from-line`：一条自由命令行折成别名表单（头一个词是哪一家的启动器 · ccm 照它自己的解析器折） |
| `--profiles-impact` | ＝ 帧命令 `profiles-impact`：这几处改动会让哪几段合下来变（改前改后） |
| `--profiles-read` | ＝ 帧命令 `profiles-read`：读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填 |
| `--profiles-resolve` | ＝ 帧命令 `profiles-resolve`：一段合下来的合并表与「等于」那一行（可按未存的表单算） |
| `--profiles-write` | ＝ 帧命令 `profiles-write`：按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数 |
| `--pubkey-push` | ＝ 帧命令 `pubkey-push`：把本机公钥推进那台的 `authorized_keys` |
| `--quota-probe` | ＝ 帧命令 `quota-probe`：用某个号查一次额度 |
| `--quota-read` | ＝ 帧命令 `quota-read`：这台的额度账 |
| `--quota-warm` | ＝ 帧命令 `quota-warm`：给一个号开窗 |
| `--resident-attach` | 小中继：连这台家里常驻后端的套接字，stdin → 套接字、套接字 → stdout 原样对拷，任一边断就退；断开 ⇒ stdout 一行 `{attach:"refused", reason: absent\|unreachable}`、退出 2 |
| `--resident-call` `[--stdin-line] --within-ms <毫秒> [--tz <IANA 名>]` | 问这台的常驻后端一条帧命令（stdin 一行 `{cmd, args}`）：连套接字、握手（在看名单交空的）、发请求、等那一条应答，stdout 一行回包、退出 0；常驻答了失败 ⇒ stderr 一行它那一份失败信封、退出 2；没人在听 ⇒ `resident_absent`（不起冷进程顶上）；`--within-ms` 必给（等多久归发起方，缺 ⇒ `bad_args`），到期限没回 ⇒ `timed_out` |
| `--resident-ensure` `[--replace]` | 确保这台的常驻后端在听：已在 ⇒ `{pid, replaced: false}`（pid 是套接字对端）；没在 ⇒ 起一个脱离的自己、回 `{pid, replaced}`；`--replace` 在听那一位比自己旧才停掉再起（只升不降） |
| `--resident-stop` `[--grace <秒>]` | 停这台的常驻后端：问套接字在听的是谁 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful\|killed\|not_running, pid}` |
| `--rotation-facts` | ＝ 帧命令 `rotation-facts`：每个号此刻的事实 |
| `--rotation-for-new` | ＝ 帧命令 `rotation-for-new`：新会话用哪份策略 |
| `--rotation-reload` | ＝ 帧命令 `rotation-reload`：在用旧版的会话都载入新版 |
| `--rotation-settings` | ＝ 帧命令 `rotation-settings`：改一份策略的设置 |
| `--rotation-strategies` | ＝ 帧命令 `rotation-strategies`：策略目录 |
| `--rotation-strategy-add` | ＝ 帧命令 `rotation-strategy-add`：加一份策略 |
| `--rotation-strategy-copy` | ＝ 帧命令 `rotation-strategy-copy`：复制一份策略 |
| `--rotation-strategy-delete` | ＝ 帧命令 `rotation-strategy-delete`：删一份策略 |
| `--rotation-strategy-rename` | ＝ 帧命令 `rotation-strategy-rename`：给策略改名 |
| `--rotation-strategy-tz` | ＝ 帧命令 `rotation-strategy-tz`：给策略设时区 |
| `--rotation-temp` | ＝ 帧命令 `rotation-temp`：会话侧的临时动作 |
| `--rotation-try` | ＝ 帧命令 `rotation-try`：试跑一份策略 |
| `--rotation-use` | ＝ 帧命令 `rotation-use`：会话载入一份策略 |
| `--rotation-use-now` | ＝ 帧命令 `rotation-use-now`：现在用这个号 |
| `--rotation-view` | ＝ 帧命令 `rotation-view`：一个会话或一份策略此刻的轮换 |
| `--session-account` | ＝ 帧命令 `session-account`：一条会话的号 |
| `--session-attachment-check` | ＝ 帧命令 `session-attachment-check`：开传前问一次：这一条的附件逐份收不收（上限 · 目录 · 工作目录可写） |
| `--session-attachment-commit` | ＝ 帧命令 `session-attachment-commit`：把暂存区里传完的一份落进会话工作目录下一个新的附件目录 |
| `--session-attachment-drop` | ＝ 帧命令 `session-attachment-drop`：删掉会话工作目录下的一个附件目录（草稿上撤掉那一份） |
| `--session-commands` | ＝ 帧命令 `session-commands`：一条会话能用的斜杠命令与 Skill（内置 ＋ 本项目 ＋ 它用的号的配置目录） |
| `--session-fork` | ＝ 帧命令 `session-fork`：从某条消息处分叉出一个新会话 |
| `--session-interrupt` | ＝ 帧命令 `session-interrupt`：中断一条会话这一轮（不进输入队、不换持有者） |
| `--session-model` | ＝ 帧命令 `session-model`：换模型（开着 ⇒ 送那一家的那一行、从记录里读回换成了什么；在等人不送；终端没了 ⇒ 记下、下次恢复照它起） |
| `--session-open` | ＝ 帧命令 `session-open`：起会话 · 恢复 · 接回（一个请求：落在哪 ＋ 要开终端就跑的那一行） |
| `--session-open-dir` | ＝ 帧命令 `session-open-dir`：起新会话前核一个目录：在不在 · 会用哪个终端名 |
| `--session-open-facts` | ＝ 帧命令 `session-open-facts`：起新会话那个框要的事实：最近目录 · 能放在哪几种 · 有哪几家 agent · 分叉时原会话的起法 |
| `--session-permission` | ＝ 帧命令 `session-permission`：切权限档（开着 ⇒ 按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按；终端没了 ⇒ 记下、下次恢复照它起） |
| `--session-queue-cancel` | ＝ 帧命令 `session-queue-cancel`：撤队里一条还没送的 |
| `--session-queue-put` | ＝ 帧命令 `session-queue-put`：往一条会话投一句（排进那条会话的队；同 id 只入队一次，同一端按端序号保序） |
| `--session-queue-read` | ＝ 帧命令 `session-queue-read`：一条会话此刻的队列名单 |
| `--session-rewind-options` | ＝ 帧命令 `session-rewind-options`：编辑已发的一句时，发送那一下给哪几项（怎么退 · 什么时候退） |
| `--session-self` | ＝ 帧命令 `session-self`：我是哪条会话 · 在哪几组 · 组员有谁 |
| `--sessions-needs` | ＝ 帧命令 `sessions-needs`：这台上需手动的会话清单 |
| `--sessions-needs-merge` | ＝ 帧命令 `sessions-needs-merge`：问到各台的需手动、合成一个先后 |
| `--sessions-stop` | ＝ 帧命令 `sessions-stop`：停一批会话（逐个答，一个不成不挡下一个） |
| `--sessions-where` | ＝ 帧命令 `sessions-where`：一批会话各在这台哪个终端里 |
| `--set-launch` | ＝ 帧命令 `set-launch`：照存好的一组补足、逐条起：起之前写好组注解，一条不行不挡别的 |
| `--set-plan` | ＝ 帧命令 `set-plan`：补足一组会话的预览：每行已有几个 · 再起几个 · 进哪个组 · 起不了的原因 |
| `--skill-install-apply` | ＝ 帧命令 `skill-install-apply`：在要被写的那一台把勾的那几个 skill 文件写进去 |
| `--skill-install-plan` | ＝ 帧命令 `skill-install-plan`：在要被写的那一台判 skill 装不装得过来 |
| `--skill-read` | ＝ 帧命令 `skill-read`：读来源那台上的一个 skill |
| `--ssh-config-import` | ＝ 帧命令 `ssh-config-import`：批量导入预览 |
| `--tasks-list` | ＝ 帧命令 `tasks-list`：一个会话的任务列表 |
| `--terminal-end` | ＝ 帧命令 `terminal-end`：结束名单里的一个终端 |
| `--terminal-input` | ＝ 帧命令 `terminal-input`：往一个终端送字或送键（过身份门） |
| `--terminal-preview` | ＝ 帧命令 `terminal-preview`：抓一个终端的一屏（只抓一次，轮询归调用方） |
| `--terminal-release-input` | ＝ 帧命令 `terminal-release-input`：交还托管终端的输入 |
| `--terminal-ssh` | ＝ 帧命令 `terminal-ssh`：给一台远端开终端要跑的那一串 |
| `--terminal-take-input` | ＝ 帧命令 `terminal-take-input`：替自报的前端接管托管终端的输入 |
| `--terminals-board` | ＝ 帧命令 `terminals-board`：「终端」视图那张名单（问到各台、合成一份：每台一组 · 筛选四格的数） |
| `--terminals-board-here` | ＝ 帧命令 `terminals-board-here`：这台「终端」视图那几行（只列 cc-monitor 起的，排好、字写好） |
| `--terminals-list` | ＝ 帧命令 `terminals-list`：这台的终端名单（两种宿主同一形，每一行带宿主） |
| `--tmux-notify` `<backend_pid> <backend_starttime>` | tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统 |
| `--unplace-self` | 卸载那一趟：暂存区里传上去的这一份删掉落点 `~/.cc-monitor/bin/ccm`（不认身份）、再删自己；回 `{removed: true\|false}`；不在暂存区 ⇒ `outside_staging` |

## 6. 几条命令的出入参类型

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

### 骨架索引的一行（`history-index` 的 `rows` 装运之前）

#### `IndexRow`

骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（第一格 · `§2.5b 路 D`）。

| 字段 | 类型 | 说明 |
|---|---|---|
| `o` | number | 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)` |
| `n` | number | 行字节长（**含**结尾 `\n`） |
| `t` | RecordClass? | 这一行翻成的那条通用记录的类（`said` · `reply` · `retry` · `title` · `unread`，与读正文时那条记录的 `t` 同一个词）；不进界面（元数据 · 解析不出）⇒ 省略 |
| `u` | string? | 只在用户输入那张小表里带；扫描图按它认一条记录在第几轮） |
| `sc` | bool? | 这条记录属于某个子运行（适配层 `RecordFace::run_of` 答得出） |
| `dr` | Draw? | 画成哪一形（就是那条通用记录的 `draw`）；不画 ⇒ 省略 |
| `hu` | bool? | 人在这条里说了话（就是那条通用记录的 `human`） |
| `ch` | number? | 正文字符数（**代码块之外**；Unicode 标量计，不含换行） |
| `cj` | number? | 其中 CJK/全宽字符数（口径 = `height-estimate.ts::fallbackTextHeight` 的 `> 0x2E80`） |
| `pl` | number? | 正文**非空**硬行数（代码块之外） |
| `cb` | number? | 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。 |
| `cl` | number? | 代码块内总行数 |
| `fd` | number? | 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块 |
| `x` | string? | 这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略 |
| `ts` | string? | 同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席） |

### `history-record` 的出参

#### `RecordProbe`

`record_in` 的答案。

| 字段 | 类型 | 说明 |
|---|---|---|
| `present` | bool | 那条会话的记录在这台找得到（哪一家的都算） |
| `root` | string | 找到 ⇒ 那份所在的记录根；找不到 ⇒ 查的那棵记录树的根 |
