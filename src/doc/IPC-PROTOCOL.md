# 跨进程协议

两件事：**后端的线上协议**（monitor 与第二个前端都按它接后端）与**本机进程之间的几份文件**（PowerShell 绑窗 · 设置 · 历史注解）。

本页只写不随命令变的总述。每一帧、每一条命令、每一个子命令的逐格形状在 **[IPC-COMMANDS.md](IPC-COMMANDS.md)** —— 那一份从代码生成
（`tests/backend/protocol_doc_gen.rs`，判据「重新生成 == 仓里那份」），改了帧字段或命令登记之后在 `src/backend` 下跑
`CCM_REGEN_PROTOCOL_DOC=yes cargo test --lib -- protocol_doc_gen` 重生成。

## 1. 载体

| 载体 | 谁用 | 怎么接 |
|---|---|---|
| SSH exec | 远端（monitor 经本机常驻后端的连接池 · 第二个前端直连） | `ccm -- --stream …`：后端写 stdout、读 stdin |
| 常驻套接字 | 本机常驻后端 · 远端常驻后端（经 `link-open` 的 `use:"stream"` 在那台跑 `ccm -- --resident-attach` 小中继，原样对拷） | Unix 套接字 `<家>/run/backend.sock`（家 = `~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`；目录只给本人，收连接时核对端 uid，没有钥匙）；先读 hello，再交一行 `{"attach":true}`（可带 `"flags":[…]`，起流旗标的子集），回 `{"attach":"ok"}` 或 `{"attach":"refused","reason":"malformed-attach"}`；中继连不上时第一行就是 `{"attach":"refused","reason":"absent"|"unreachable"}`；每条连接各一份 watcher / 入方向 / writer |
| 一次性 exec | CLI 子命令（脚本 · skill · 第二个前端的查询） | `ccm -- --子命令 …`，干完即退、不进流 |

后端二进制叫 `ccm`（`~/.cc-monitor/bin/ccm`）：零参数是「起会话」，打头的 `--` 之后才归后端（`<交给 claude 的…> -- <ccm 自己的…>`）。

**线约束**：每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内没有裸 `\n` / `\r`。帧用 `kind` 做标签（snake_case）。

## 2. 起流旗标

monitor 只对 hello 里**声明了对应能力**（`capabilities`）的后端传对应旗标，不按版本号猜。

| 旗标 | 意思 |
|---|---|
| `--stream` | 「我是流模式」的显式词，不改行为 |
| `--with-bg` | 放行 `kind:"bg"` 的后台任务会话 |
| `--tail-only` | 不重放历史：各文件从当前行数起只尾随新行；历史由客户端另取（`history-read` / `--read-session-tail`） |
| `--with-pid` | `session_added` 带 `pid`（只本机那条流发；默认关，关时字节与加它之前一字不差） |
| `--with-raw` | `line` 带 `raw`（那一行记录的原文）。**过渡格**：前端读的是成品 `record`，手机那一侧的缺格补齐之后删。默认关 |

## 3. hello：先读它，再说话

连上之后后端先发一帧 `hello`，**客户端读到它之前不许写命令**（后端侧 reader 要一个只有 flush 过 hello 才拿得到的见证才起得来）。几个面各管一件事：

- `capabilities` —— 后端认得哪些起流旗标（每个 token 对应一条它会先剥掉的旗标）；缺 ＝ 按最小能力集待它。
- `emits` —— 后端会发哪些帧 kind；客户端据此决定依不依赖某种帧。
- `commands` —— 后端接哪些入方向命令；不在里面的命令客户端**不发**。缺 ＝ 这个后端不读 stdin。
- `unavailable` —— 接得下、但**这台机器上做不到**的命令与原因码（如没有 tmux ⇒ `no_tmux`）。缺 ＝ 没有把握，照发、看回话；列出来的 ＝ 别画那个按钮。它是提示，后端自己绝不拿它拒命令（同一帧会被这个进程后来的连接共用，读数可以很旧）。
- `uncancellable` —— `commands` 里撤不动的那几条（阻塞档）。

`build_id` 是这份二进制的身份；monitor 按它判远端后端要不要换。`host_env` 回显起它的宿主交给它的那几格环境（monitor 据此拒接住在别的家的后端）。

## 4. 入方向：请求 · 应答 · 取消

```text
→ {"id":"<不透明串>","cmd":"<命令名>","args":{…},"within_ms":10000}          一行一个；args 缺 ＝ null；within_ms 可缺
← {"kind":"reply","id":"…","ok":true,"data":{…}}                              无返回值时没有 data
← {"kind":"reply","id":"…","ok":false,"code":"…","message":"…","detail":"…"}  少数码另带 data（形状见各命令）
→ {"id":"…","cmd":"cancel","args":{"target":"<要撤的 id>"}}
← {"kind":"cancelled","id":"<被撤的 id>"}
```

- **`id`** 由客户端发号、后端只回显（不解析、不校验）。monitor 的发法是 `<连接 nonce>-<单调序号>`，重连不撞号。同一个 `id` 的命令还在跑 ⇒ `duplicate_id`。
- **`within_ms`**：发起方愿意等多久。后端从收到这一行起减 2 秒余量换成截止时刻，阻塞档命令里装总期限的各处都收紧到它（到点回 `child_timed_out`，见 §CLI 面「期限」那一条）。不是正整数 ⇒ 当没带。
- **超时归客户端**：后端零定时器，不替客户端掐表；客户端的期限覆盖「写入 ＋ 等应答」两段，到点就撤单（`cancel`）。
- **取消是一条普通命令**。撤一个不存在的 `id` 也回 `ok`。阻塞档的命令开跑之后打不断，`cancel` 回 `not_cancellable`（去等它自己的应答），不会回一条假的 `cancelled`。
- **应答走独立的小通道**（256 条），与出方向的实时帧（10 000 条）分开：丢一条内容帧可恢复，丢一条应答客户端会永远等下去。writer 有界地优先应答。
  同走这条通道的还有几种「丢了就停住」的帧：链路字节（`link_data` / `link_end`）· 传输进度（`transfer`）· 测试连接进度（`probe`）· 终端实时预览的画面与收尾（`terminal_screen` / `terminal_follow_end`）。
  终端画面每条订阅至多一帧在途（客户端回执才推下一帧）、每条连接至多 8 条订阅，单帧至多 512 KiB，所以占不满这条通道。
- **信任边界**：单行上限 1 MiB（超了整行丢、回 `line_too_long`，`id` 从行首至多 4 KiB 里尽力抠）· 坏 JSON 回 `bad_request` 并继续读 · 任何失败都不结束读循环、不结束进程。
- **码分两层**：协议级码（闭集，见 IPC-COMMANDS.md 第 3 节）与命令无关、只有入方向那一层发得出；命令自己的码列在各命令下面。客户端拿到协议级码 ＝ 客户端代码写错了，别重试。
- **失败应答三格**：`code` 给程序认（分支 · 重试判断）；`message` 是给人看的那一句（后端按共享文案表 `src/shared/copy/table.json` 写好，前端原样上屏）；
  `detail` 是「复制详情」那几行（时刻 · 机器 · 命令 · 码 · 原话，排法只住 `copy_core::detail`），下层原话只进这里、不进 `message`。

## 5. 会话流：帧的先后与续传

1. 连上：`hello` → 每个活会话一帧 `session_added`（宣告时带初始 `activity` / `waiting_for`；后台会话带 `background: true`）→ `sessions_replayed`（「清单报完了」：分清「还没说完」与「说完了、里面没有它」）。
2. 内容：每条记录一帧 `line`，带成品 `record`（通用记录，见下面「通用记录」一节；缺 ＝ 不进界面、照占号）、`seq`（本条流里按文件单调递增）与 `byte_offset`（这一行末尾在文件里的累计字节）。
   **续传用 `byte_offset`，不用 `seq`**：断线重连后拿它当偏移再读（`history-read` / `--read-session-from-offset`）。
3. 状态：`session_status`（红绿灯变了才发，天然稀疏；`activity` ＝ `working` · `needs_you` · `idle` · `background_work`（一轮停了、它在后台起的命令还在跑），后端适配层从那一家的进程状态翻过来，翻不出就不带）· `turn_end`（一轮结束）· `session_runs`（子运行表，整份）· `tasks_changed` / `rotation_changed` / `rotation_rules_changed` / `accounts_changed` / `quota_changed` / `plan_changed`（只带 sid、工作区与摘要，或不带载荷：客户端收到就重问那条查询，清单本身不在帧里）。
4. 离开：`session_removed`（`cause`：`gone` 真没了 · `superseded` 同一个 pidfile 原地换了 sid，后者客户端直接归档、别去查 tmux）→ `session_state`（这台后端自己裁：`reconnectable` 容器还在、接得回去 · `ended` 只能 resume）。
5. 记录文件被动过：`session_file_gone`（不见了，不误判结束）· `session_file_reread`（被截短 / 改写过、已从头重读；紧排在重读出来的 `line` 之前）。
6. 背压：实时通道满时丢帧，排空后发一帧 `overflow`（丢了几帧 ＋ 不可恢复的那些帧的身份 `lost`）。`line` / `turn_end` 丢了可以从记录文件补；`session_added` / `session_removed` / `session_status` / `session_state` / `tasks_changed` 这类一次性结论丢了别处没有，客户端按 `lost` 重同步。

`--tail-only` 时客户端先取快照（尾部优先：`--read-session-tail` / `history-tail`），`session_added.lines` 是宣告那一刻的完整行数，用来核快照拉全了没有。

## 6. 会话归属与身份门

- **`@ccm_sid`**：tmux 窗格上的会话标记。一个 tmux 会话可挂几个窗格、各挂各的 sid；送字 / 抓屏 / 结束都落在挂着那个 sid 的窗格上，身份也按那个窗格判（活动窗格是谁不算数）。
- **`@ccm_client`**：会话由谁起。用户在终端里敲的 ccm / 别名起的写 `ccm`（谁都能动）；某个前端为自己内部起的写那个前端的名字（1–32 个 `[a-z0-9-]`）。
  请求可带 `args.client` 自报是哪个前端：没声明 / 声明成 `ccm` ⇒ 这一维不拦；声明的就是自报的 ⇒ 放；声明了别的 ⇒ `wrong_owner`（别的前端起的，这里只能看）。
  `kill` · `launch` 的 `send-into` · `sessions-*` · `terminal-input` · `session-restart` 共用这一维；它防误动，不是安全边界。
- **破坏性动作三道门**：名字精确匹配（`=name:`，不许含 `:` / `=` / 控制字符）⇒ 名字像我们铸的（`cc-*` / `<X>-cc`）或已挂 `@ccm_sid` ⇒ 只有一个窗口（只给杀会话）。杀的是 `#{session_id}` 句柄，不是名字。
- **抓屏不过身份门、恒可用**：`terminal-preview` 只读，谁问都给（身份门只管送字 / 结束）⇒ `terminals-list` 每行的 `can` 里只有会变的 `input` · `end`，没有 `preview` 那一格。
  `terminal-input` 只收 `terminal` | `sid` · `text` · `enter` · `key` · `seen_screen` · `client`，多送一格 ⇒ `bad_args`。
- **终端句柄**：`terminals-list` 每行的 `terminal` 是不透明句柄，前端不拼、不解析；后端不收任意 tmux 目标串，句柄 / sid 先在那一刻的名单里对上才动手。
- **送字的回话不带画面**：`terminal-input` 回 `delivered` 时**不带** `screen`（`screen` 只跟 `refused` ＋ `screen_changed` 一起回，是那一刻的新指纹）。
  要连着按，要么订 `terminal-follow`（只在帧面），要么每按一下之前问一次 `terminal-preview` 拿新指纹。重抓的节拍归前端。

## 7. 两个前端共吃的冻结面

桌面端（monitor）与第二个前端（手机端）吃同一个后端。第二个前端按下面这些格读，缺一格就把整帧当坏帧丢 ⇒ 它们**冻结**：不许改名、删、换类型，只许加新字段；非改不可就两边同拍。

- 帧：`hello` `v` `build_id` `host_arch` `claude_dir` `capabilities` `emits` · `line` `session_id` `path` `seq` `byte_offset` `raw`（`--with-raw`）`record` ·
  `overflow` `dropped` `lost` `lost_truncated` · `turn_end` `session_id` `uuid` · `tap` `stream` `run` `resp` `n` `ev` ·
  `reply` `id` `ok` `code` `message` `detail` `data` · `cancelled` `id` · 请求信封 `id` `cmd` `args` `within_ms`（可缺）。
- 会话三帧与成品面在格目录里：每格带 `frozen`。`session_added`（`sid` `path` `cwd` `name` `lines` `waiting_for` `agent_kind` `liveness_confidence` `attachable` `activity` `activity_text` `activity_tone` `background`）·
  `session_status`（`sid` `waiting_for` `liveness_confidence` `activity` `activity_text` `activity_tone`）· `session_removed`（`sid` `cause`）只冻这几格；
  `line.record` 与 `history-read` 的 `rows`（`end` `hash` `record` `cwd`）整件冻结 —— 通用记录的公共格、六类各自的格、`who`、`error`、各种内容块逐格冻结（见下面「通用记录」一节）；
  `history-page` / `history-lines` / `history-run` 的 `record` 是同一形。新加一格随格目录金样重写（`cells_catalog_tests::the_golden_is_what_the_command_writes`，删 / 改名 / 换类型 / 解冻重写也不放行）。
  `line.raw` 逐字节等于记录文件里那一行，去掉行尾（`\n`；CRLF 行连 `\r` 一起去）。
- **改了冻结的格就得打版本号**（同加子命令）：`build_id_guard` 的指纹带一行冻结格的摘要 ⇒ 第二个前端连上只核 `BUILD_ID` 这一处，不用读 `cells-catalog` 逐格核。
- 一次性子命令：第二个前端只调 `--backend-probe` · `--resident-ensure`（可带 `--replace`）· `--resident-attach`，其余都走常驻流上的帧命令。
- 会话是不是后台、此刻在干什么只看后端判好的 `background` · `activity`；那一家的原词不上线。
- 判据：`wire_tests::the_shapes_the_second_frontend_reads_stay_put`（帧那张表，类型逐格对）· `cells_catalog_tests::the_golden_is_what_the_command_writes`（格目录里冻结的格）· `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump`（子命令集或冻结格变了 ⇒ 打版本号）· `wire_tests::the_subcommands_the_second_frontend_calls_stay_put`（子命令那三条）。
- 跨语言金样：`tests/__fixtures__/session-stream.golden.jsonl`，每种帧两行（「全格」与「最少格」），由后端真序列化器写；最少格里的格就是必填格。
  终端管理 `tests/__fixtures__/terminals.golden.json` · `--resolve` `tests/__fixtures__/resolve-contract.golden.json` · 换号重启 `tests/__fixtures__/rotation-switch-restart.golden.json`。
- 部署：第二个前端从 GitHub Release 下后端字节（两个 musl 目标），按 `SHA256SUMS-linux.txt` 与字节里的身份戳校验；资产名登记在 [RELEASING.md](RELEASING.md)。

### 通用记录

会话记录里一行在界面里是什么，由那一家的适配层翻成这一形（`agents/record.rs`；Claude 的翻译表 `agents/claudecode/record_of.rs`，Codex 的 `agents/codex/record.rs`）。
两个前端都只按 `t` 与格排版，不认任何一家的盘上格式。每类全格 ＋ 最少格的样本：`tests/__fixtures__/record.golden.jsonl`（后端真序列化器写，`record_of_tests` 钉着）。

公共格（每类都有）：

| 格 | 类型 | 说明 |
|---|---|---|
| `agent` | string | 哪一家（`claude` · `codex` …） |
| `id` | string | 这条记录在本会话里的身份：**不透明串**（非空、会话内唯一；主线外清单、分叉、跳转都按它认），前端不解析、不拼。那一家的记录自己有身份就用它（Claude 的 `uuid`）；没有的合成「`@<这一行起点的字节偏移>`」 |
| `at` | string? | 记录时刻（ISO-8601 原样）；插进正在跑的那一轮的 `queued` 是打字那一刻 |
| `timeText` | string? | `at` 在那台本地钟上的钟面 `HH:MM`（界面照抄、不换算） |
| `t` | string | 类别：`said` · `reply` · `retry` · `title` · `queued` · `unread` |

各类的格：

| 类 | 格 |
|---|---|
| `said`（人那一侧说的：人 · 工具结果 · 派活 · 来话 · 通知 …） | `who`（`UserText`：`speaker` 谁说的，`kind` 闭集 `human` `slashCommand` `bashInput` `bashOutput` `commandOutput` `taskNotification` `agentMessage` `peerSession` `coordinator` `agentTask` `system` `compactSummary` `interrupt` `toolResult`；`text` 要显示的正文；`pasted?` 粘贴块）· `blocks` · `results?`（工具调用 id ⇒ 结果一句：`ok` `rejected?` `lines?` `added?` `removed?` `files?` `answer?` `exitCode?` `file?` `patch?` `patchTruncated?`；`patch` 至多 32 KiB、整段不劈）· `cwd?` |
| `reply`（代理的回复） | `blocks` · `model?` · `autoReply`（代理那一侧自动写的应答，不建卡）· `endsTurn`（一轮的结束，与帧 `turn_end` 同一个判定）· `cards?`（工具调用 id ⇒ 卡型 `agent` `interactive` `diff` `md` `command`）· `steps?`（工具调用 id ⇒ 过程一行：`tool` · `arg?` · `path?` · `note?` · `known`）· `runs?`（工具调用 id ⇒ 派出的子运行 `{label, kind?}`）· `error?`（上游最终失败写的报错：`reason` 闭集 `overloaded` `quota` `network` `auth` `context` `unknown`，`status?` HTTP 状态码；报错正文在 `blocks` 里） |
| `retry`（上游失败、将重试） | `reason`（同上闭集）· `attempt?` · `max?` |
| `title`（会话标题） | `text` · `by`（`agent` 代理起的 · `user` 人起的） |
| `queued`（人在一轮跑着时插进去的一句） | `who`（同 `said`） |
| `unread`（这一家的这一行认不出：没见过的类型 · 见过的类型形状变了） | `why`（闭集 `unknownType` `parseFailed`）· `type`（那一行自己说的类型原样，只当标签、出口不按它取字；没有 ⇒ `null`）· `text`（写好的一句）· `tone`（恒 `warn`）· `excerpt`（原文截到 400 字节、按字符截，给［复制详情］，出口照抄不解析）· `count`（并了几条：出记录页那一遍把相邻、原因与类型都一样的并成一条；实时 `line` 帧每行一条、恒 1） |

内容块 `blocks[]`（两家共有的词，按 `type`）：`text {text}` · `thinking {text}` · `tool_use {id, name, input}` · `tool_result {for, content: [块], isError}` · `image {source}`。

- **过程一行的主参数 `steps.*.arg` 是定长一行**：换行与连串空白压成一个空格，至多 200 字（按字符，不切半个字），截了以 `…` 收尾；两个前端都不再截。完整内容照旧在 `blocks` 的入参里。
- 出口开关：`history-read` / `history-page` 的 `summaryOnly` 剥掉 `blocks` 与 `results` 里的 `patch` / `patchTruncated`，折起那一行要的格照给；`--with-raw` 才带 `line.raw`。
- 主线外清单（ESC 回退掉的那几条记录的 `id`）不在记录上，单独给：实时帧 `session_branch`（整份）· 冷读 `history-branch`。没有链的那一家恒空。

## 8. CLI 一次性调用

- `ccm -- --子命令 [位置参数…]`。从帧命令派生的那些（`--files-ls` 之类）与流上同名命令是同一个处理器，stdout 一行应答 JSON。
- **入参**（收 `args` 的那些；一段 JSON，空 ＝ `{}`）有两个口，**二选一**，每条派生子命令都有，两个修饰词都认**任意位置**：
  - **stdin**：默认读到 EOF；带 `--stdin-line` ⇒ 读到第一个换行就动手（给关不掉 stdin 的调用方）。上限 1 MiB（1048576 字节）。
    stdin 开着、却 1000 ms 内一个字节都没来 ⇒ 立即回 `no_input`（不挂住）；第一个字节到了之后不再计时。stdin 是 EOF ⇒ 当 `{}`，缺哪格由命令自己回 `bad_args`。
  - **argv**：`--args-b64 <base64 的 JSON>`（标准字母表、带补位）。给了它就不碰 stdin（写不了 stdin 的调用方用它，例如只有 stdout 的执行通道）。
    值上限 131068 字节（编码后，约 96 KiB 的 JSON）。系统先卡一道：Linux 单个参数 ≤ 131072 字节（含结尾 NUL），经 ssh 时整行命令是登录 shell `-c` 的**一个**参数，
    同一个上限管整行；Windows 整行 ≤ 32767 个字符。超过系统那一道，后端根本起不来，调用方看到的是 shell / sshd 那一层的错（如 `Argument list too long`），不是下面的信封 ⇒ 更大的载荷走 stdin。
  - 两个都给（`--args-b64` 与 `--stdin-line`）· `--args-b64` 缺值 / 给两次 · 不收入参的命令带 `--args-b64` ⇒ `bad_args`；base64 坏 / 解出来不是 UTF-8 / 不是 JSON ⇒ `bad_request`；
    超上限（哪一个口都一样）⇒ `args_too_large`，拒收、不截断。
- **期限**：`--within-ms <毫秒>`（任意位置）与帧面请求信封的 `within_ms` 同名同义：减 2 秒余量换成截止时刻，装总期限的命令都收紧到它，到点回的码与帧面相同（`child_timed_out`）。
  哪几条装总期限、上限多少，协议参考里逐条写（「总期限上限 N 秒」）；没写的那几条不装，带了期限对它不起作用。到点只回 `child_timed_out` 这一个码，三条例外不整条失败：`aliases-read` · `powershell-policy-set` 落在成品的 `policy.error`，`ssh-config-import` 交已解析的那几个。
  缺值 · 给两次 ⇒ `bad_args`；值不是正整数 ⇒ 当没带（同帧面那一格）。
- **stderr 是协议通道**：成功 ⇒ stderr 一个字节都没有；失败 ⇒ 正好一行信封（下一条）。后端自己的诊断日志一行都不写 stderr，
  整行追加进那台后端的 stderr 诊断文件（宿主交了 `CCM_BACKEND_STDERR_LOG` 就是那一份，否则 `~/.cc-monitor/logs/backend/stderr.log`，常驻后端的 stderr 落的也是它；
  那一层目录不在 ⇒ 诊断丢掉，不替它建）。调用方可以整段 JSON-parse stderr。
- 失败：stderr 一行 `{code, message, detail}`（少数码另带 `data`）、退出 2 —— 与帧面失败应答出自同一份：同一个失败两个面上 `code` · `message` · `detail` · `data` 逐字相同（`detail` 恒在）。
  带 `--text` ⇒ stderr 是那一句 ＋ 下面原样接 `detail` 那几行（同界面「复制详情」复制出去的那一段），不是 JSON。
  手写的几条一次性子命令（`--list-projects` · `--fork-session` · `--account-trust` · `--account-trust-zero`）失败也是这一份（详情里的「命令」是那条子命令名，不收 `--text`）；
  只有 `--resolve` 另是一形（`{code, message}` 两格，冻结给第二个前端，见 §10）。
- 出清单的那几条（骨架索引 · 你说过的话 · 会话内查找）是**三段**：首行头（认得出对面会出这份东西）· 每条一行 · 尾行带 `count` 与续点。**没有尾行 ⇒ 输出被截断**，调用方不许当全量。
  选项写在位置参数**前面**：老后端不认新选项时会快速失败（stdout 0 字节、退出 2），而不是把整份会话透传回来；客户端认「首行不是那个头」⇒ 诚实降级。
- 路径参数过同一套路径围栏（只许落在那台的会话记录树里）。
- **`--resolve`**（`ResumeSpec` → stdout `CommandPlan`）与仓外 aterm 的契约冻结，字节以金样为准。入参与上面同一套口：stdin（可带 `--stdin-line`）或 `--args-b64`，同一套上限。
  错误码全集：`bad_request` · `invalid_session_id` · `unsafe_launch_candidate` · `serialize_failed`（流上的 `resolve` 也回这几个）·
  `stdin_read_failed` · `args_too_large` · `no_input` · `bad_args`（只在一次性这条）。它的三个出参可信度不同：`command`（候选启动器 ＋ `--resume <sid>`）可当事实用；
  `sessionName` 纯从 sid 派生、没查过 tmux；`capabilities` 是典型档、不是这台此刻的探测结果 —— 要判某个 tmux 会话在不在，问 `terminals-list`。

## 9. 本机进程之间的文件

全在 `~/.cc-monitor/` 下（路径越界一律不许）。共同约束：**UTF-8 无 BOM**（PowerShell 5.1 的 `Out-File -Encoding utf8` 会写 BOM；写端用 `[IO.File]::WriteAllText(…, UTF8Encoding($false))`，读端也剥一次）·
**原子写**（Rust：临时文件 → 原子替换；PowerShell：单次 `WriteAllText`）· 目录不在就建 · 未知字段忽略。

| 文件 | 写 | 读 | 是什么 |
|---|---|---|---|
| `config.json` | monitor（唯一写口 `config.rs::patch_config_at`：交「改哪几条路径」，进程级锁里现读、逐条应用、原子替换；盘上读不懂就拒写） | monitor 前端 · `logging` · 起本机后端时取 `claudeDir` | 主题 · 字体 · `claudeDir` 覆盖 · 诊断 · 机器表 `remote.hosts`（改认人的那几格时整张表要成立：`config.rs::check_machine_table`） |
| `ps-await/<PID>.tty` | Linux bash / zsh 接入块（`src/shared/ccm-aliases.sh`，本机桌面上开的 shell） | monitor `bind.rs` | `{shell_pid, proc_start, tty}`：「去认这个终端的窗口」；认上 / 认不出就删 |
| `ps-registry/<PID>.json` | monitor `bind.rs` | monitor 拉前 | `{ps_pid, hwnd, owner_pid, owner_proc_start, ps_proc_start, title_at_bind, registered_at}`：那个 shell 显示在哪个窗口；与 shell 进程同寿 |
| `auto-launch.json` | monitor 设置 · 启动时写自己的路径 | PowerShell 接入块 `__ccm_bind` | `{auto_launch_enabled, monitor_exe_path}`：用 `cc` 起 claude 时要不要顺手开 monitor |
| `history-metadata.json` | 本机常驻后端（`history-annotate` / `history-forget`；读不懂就拒写、只改那一条） | 本机常驻后端（`history-list` 并进成品） | `{<sid>: {starred, custom_title, hidden}}` |
| `logs/monitor/` · `logs/backend/` | monitor · 常驻后端 | 人 · `backend-log` | 诊断日志（按天滚 · 有上限） |
| `backend.json` | `exit-policy-set` | `exit-policy-read` · monitor 退出时 | 「退出行为」：monitor 退出时结束不结束本机常驻后端 |

设置窗机器列表那一格的成品（`state` · `reason` · `fixes` …）判定只在 `machine_state.rs`，形状以金样 `tests/__fixtures__/machine-state.golden.json` 为准。

**Claude Code 自己写、后端只读的两份**（集成方要自己写 pidfile 时看这里）：

- `<claude_dir>/sessions/<PID>.json`：活会话登记表。`kind` 是**排他式**的 —— 缺席或 `"interactive"` 放行、`"bg"` 只在 `--with-bg` 时放行、**其它任何值都被隐藏**（想让会话可见，`kind` 要么不写、要么写 `interactive`；同一个 pidfile 的 `kind` 翻成别的值等于让会话消失）。
  `attachable: false` ⇒ 不提供 attach / 拉前 / 「杀死空 tmux」，缺席 ＝ `true`。`procStart` 参与 PID 复用检测：缺了就退化成只看进程在不在。
- `<claude_dir>/tasks/<sid>/<id>.json`：任务清单（`<digits>.json` 才算，`.lock` / `.highwatermark` 忽略；读到半截 JSON 单条跳过）。后端盯这棵树，变了发 `tasks_changed{sid}`，客户端重问 `tasks-list`。

## 切到终端：此刻显示这个会话的是哪个窗口

点 ↗ 那一刻现查，不缓存：本机会话从 agent 进程往上走进程链；远端会话先问那台此刻谁连着它（带窗口标签 `LC_CCM_WINDOW` 就先按标签），
再问本机后端开着那条连接的是哪串进程（`terminal-processes`）。最后一跳在 monitor（`bind.rs::pick_chain_window`）：沿链从下往上，
每一级先问「它显示在哪个窗口」，再看它名下的可见顶层窗口，碰到终端本身就停（恰好一个窗口 ⇒ 它，几个 ⇒ 照实说分不清）。

「它显示在哪个窗口」：
- **Windows**：借那个进程的控制台问一句（`platform::console::console_window`：`AttachConsole` → `GetConsoleWindow` → 属主）。
  Windows Terminal 每个标签的伪控制台窗口的属主就是承载它的那个窗口；经典控制台就是控制台窗口自己。不要接入块、不要登记、不改标题。
  只认到窗口，认不到窗口里的哪个标签。
- **Linux（X11）**：bash / zsh 接入块在本机桌面上开的 shell 里留 `ps-await/<PID>.tty`，monitor 往那个终端写改标题序列挂记号、按标题找窗口，
  写进 `ps-registry/`（标题出栈还原）。

## 加一条协议

- **线上命令**：在 `src/backend/stream/inbound/registry/` 对应那一族加一条 `CommandSpec`（名字 · 一句话 · 码 · 字段 ＋ 向 ＋ 一句话 · 收不收载荷 · 怎么跑），CLI 面自动派生（还要进 `lib.rs` 的 `SUBCOMMANDS`）；重生成 `IPC-COMMANDS.md`。
- **帧字段**：加在 `stream/wire.rs` 的 `Frame` 上，写一句 `///`；只加不改；进金样；重生成。
- **CLI 独有子命令**：进 `SUBCOMMANDS` 与 `stream/inbound/cli_only.rs` 的 `CLI_ONLY_DOCS`（用法 ＋ 一句话）。
- **文件**：只许在 `~/.cc-monitor/` 下；UTF-8 无 BOM、原子写、未知字段忽略；写明短暂还是持久，短暂的写明超时；在上面那张表里加一行。
