# aterm 聊天通道线协议

手机与远端 `claude` 之间的聊天通道。远端没有常驻的 bridge 程序：一条跑在 tmux 里的常驻管道
把 CLI 的 stream-json 落进文件，手机 tail 这个文件，在本机把每一行编成紧凑帧。

- 帧的字段以 `tools/reference_encoder.py` 为准，它是可执行的规格；本文的帧字段块由
  `tests/test_encoder.py::test_protocol_doc_matches_encoder` 与 golden 对账，改了编码器忘了改这里会红。
- 手机侧的实现在 `core-claude` 的 `bridge/` 包：`CliFrameEncoder`（stream-json → 帧）、
  `BridgeCodec`（帧 ↔ JSON）、`PipeSession`（下行与上行两头）、`ChatTurnAssembler`（帧 → 渲染单元）。

---

## 1. 会话目录与管道

每个会话一个目录，路径相对登录 cwd：

```
~/.aterm/s/<sid>/
  in.ndjson      # 上行：手机追加，管道的 tail -f 读
  events.ndjson  # 下行：管道写，手机 tail
  bridge.log     # 管道的 stderr
  att/           # 上行附件（up-<名字>）
```

管道在 tmux 里起，形状是：

```
mkdir -p <dir> && touch <dir>/in.ndjson && {
  printf '%s\n' '<META_LINE>';
  tail -n 0 -f <dir>/in.ndjson | claude <PIPE_FLAGS> … | <信封循环>;
} >> <dir>/events.ndjson 2>> <dir>/bridge.log
```

- `PIPE_FLAGS` = `--input-format stream-json --output-format stream-json --include-partial-messages --verbose`。
  `--include-partial-messages` 不能省：没有它就没有 `stream_event`，也就没有任何增量帧。
- 不传 `setting_sources=[]` 之类的东西：远端的 `CLAUDE.md`、settings、skills、自定义命令要照常加载，
  `init` 的命令清单全靠它们。
- 一个会话只许一条管道。两条管道往同一个 `events.ndjson` 写，同一条消息会被处理两遍。

## 2. 传输

- NDJSON，UTF-8，一行一条，`\n` 结尾。
- 下行：`events.ndjson` 只追加。手机用 `tail -c +<offset+1> -F <path>` 读，字节 offset 就是续读的位置，
  下行行不带序号。注意：读的通道绝不分配 PTY，PTY 的 ONLCR 会让每行多一个字节，offset 静默错位。
- 上行：手机在已有的 SSH 连接上开一次性 exec，往 `in.ndjson` 追加一行。
- 上行单行上限 56KB（量的是转义后的整条命令）。sshd 把整条命令当成单个 argv 元素，
  内核 `MAX_ARG_STRLEN` = 131070B，128KB 就「参数列表过长」；56KB 给 shell 引号与 JSON 转义的膨胀留了余量。
  纯文本也会超，大段内容走文件传输。

### 2.1 下行文件的信封

```
{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}   ← 首行
{"t_ns":"<启动秒数><9 位行序>","event":{…CLI stream-json 的一行…}}             ← 其后每行
```

- 下游只认这个信封，不认产出方式。`__meta__` 行跳过。
- `t_ns` 是排序键，不是时钟：管道启动那一刻的秒数接上行序。实时下行没有消费方读它；
  golden 里的 `t_ns` 是录制时的真纳秒，供重放用。
- 解析不了的行（`tail -F` 在截断或轮转时可能给出半行）跳过，不弄崩整条流。

## 3. 上行

### 3.1 用户消息

```json
{"type":"user","aterm_id":"aterm-<origin>-local#<n>","message":{"role":"user","content":"…"}}
```

- `content` 是字符串。附件先上传到 `att/`，正文里用 `@路径` 引用，不内联 content-block 数组。
- `aterm_id` 是幂等标识，CLI 容忍这个未知键。键名带 `aterm_` 前缀，免得撞上 CLI 自己的键。
- 追加是幂等的：同一条 exec 里先 `grep -qF '"aterm_id":"<id>"' in.ndjson`，在就答 `ATERM_UP_DUP`，
  不在才追加并答 `ATERM_UP_ADD`。针是整个键值对而不是裸标识，正文里恰好有同一串时不会误判成重复。
  两个哨兵都认不出来的答复一律当失败。
- 追加成功不等于管道在听：管道不在时文件还在，追加照样成功。所以追加前先确认那条 tmux 会话在，
  不在就重建。
- CLI 不回声用户消息：用户的话不会作为帧回到下行。

### 3.2 中断

```json
{"type":"control_request","request_id":"req_<n>_<8hex>","request":{"subtype":"interrupt"}}
```

- 与用户消息走同一个文件、同一条 `tail -f`，不另开通道。三个键名一个都不能改，改了 CLI 会当普通行忽略。
- 不能用 `C-c`：pane 的前台进程组是整条管道，`C-c` 会把管道打掉。
- 不去重、不等答复。连点两次会发两条，第二条对端答 `still_queued`，无害。
- 中断后的那一轮照样以 `res` 收口，`why` = `aborted_streaming`。

## 4. 帧

一行 CLI stream-json 编成 0..n 条帧。每帧带 `t`，值为 `None` 的键不发。

```frame-fields
# 由 tests/test_encoder.py::test_protocol_doc_matches_encoder 钉住。
# 格式：<帧 tag>: <该帧在 golden 里出现过的全部字段（不含 t）>
init: agents cmds cwd mcp model plugins raw sid skills tools
bs: bt i id m name
d: i m x
td: i m x
ij: i m x
be: i m
at: i m x
tt: i m x
tu: i id inp m name
tr: id ok x
res: cost dur mu ok raw sid turns usage why
rl: raw
err: code m
ev: k raw
```

这块只列 golden 里出现过的字段。编码器还会发 golden 没覆盖到的：所有块级帧与 `tr`/`ut` 的 `p`、
`tr.trunc`、`res.api`/`res.den`，以及 `ut` 整个帧型。

| t | 来源 | 字段与规则 |
|---|---|---|
| `init` | `system` / `init` | 命令清单，见 4.1。没被吸收的字段全留在 `raw` |
| `bs` | `content_block_start` | `bt` = `text`/`thinking`/`tool_use`；`id`/`name` 只有 tool_use 块有 |
| `d` | `text_delta` | 正文增量 |
| `td` | `thinking_delta` | 思考增量，最多是摘要，见 4.3 |
| `ij` | `input_json_delta` | 工具入参增量，拼完才是合法 JSON |
| `be` | `content_block_stop` | 块收口。不保证到达，见 4.4 |
| `at` | `assistant` 的 text 块 | 正文全文，覆盖 `m#i` 那个块 |
| `tt` | `assistant` 的 thinking 块 | 思考全文，同上 |
| `tu` | `assistant` 的 tool_use 块 | `inp` 保原始结构，不预解析 |
| `tr` | `user` 的 tool_result 块 | `ok` = `!is_error`；`x` 超 4096 字符截断，带 `trunc.n`（全文长度） |
| `ut` | `user` 且 `content` 是字符串 | `x` = 那段文本 |
| `res` | `result` | 一轮的唯一收口，见 4.5 |
| `rl` | `rate_limit_event` | 正常路径上也会来，不只是被限流时 |
| `err` | `assistant.error` | `code` 原样转。认证失败在这里暴露 |
| `ev` | 其余一切 | 透传，见 4.6 |

### 4.1 `init`：命令清单

```json
{"t":"init","sid":"…","model":"…","cwd":"…",
 "cmds":["compact","model",…], "skills":[…], "tools":["Task","Bash",…,"mcp__x__y"],
 "agents":[…], "plugins":[…],
 "mcp":[{"name":"…","status":"connected","source":"…"}],
 "raw":{…}}
```

- 来自 CLI `init` 的顶层字段 `slash_commands` / `skills` / `tools` / `mcp_servers` / `agents` / `plugins`。
- 条目是裸字符串，没有描述、参数、来源和分组。命令名不带 `/`，要显示 `/xxx` 由客户端加。
- `skills` 是 `cmds` 的前缀子集：同一个东西在两处各出现一次，界面上合成一个条目。
- `mcp` 是唯一带结构的：`status` 有 `connected`·`failed`·`needs-auth`·`pending`·`disabled` 五个值，
  `source` 可选、可以整个缺席。两格原样透传，不折叠、不填默认值。
- 每轮都重发 `init`；登录态不同时清单会缩水。不把第一次拿到的清单当权威缓存。
- 斜杠命令当普通文本投进 `in.ndjson`，CLI 自己解析。需要 TUI 交互的命令在这种模式下会怎样不确定，
  只能靠超时兜底。

### 4.2 块的归属：`m#i` 与 `p`

- 块级帧（`bs`/`d`/`td`/`ij`/`be`/`at`/`tt`/`tu`）都带 `m`（message_id）＋ `i`（块序号），块的键是 `m#i`。
  `i` 每条消息从 0 重来，只用 `i` 会撞键。
- message_id 只在 `message_start` 里给一次，之后的块事件都没有，编码器必须记住它。
- `at`/`tt`/`tu` 的 `i` 不能取 `content` 的下标：CLI 每个块单独发一条 `assistant`，`content` 长度恒为 1。
  要按「这条消息已交付几个块」计数。
- `p` = `parent_tool_use_id`，在 stream-json 行的外层（与 `event` 同级）。主流里它是 null，
  只有 Task 子 agent 的行非空。当前 message_id 按 `p` 分桶记，否则子 agent 的 `message_start`
  会冲掉主流的 message_id，主块已显示的正文被清掉重来。

流不是逐 token 的：录到的长回复平均一块约 83 字符、块间隔中位约 470ms，即约 176 字符/秒。
打字机式的缓释速率要不低于这个数，否则越播越落后。

### 4.3 `td`：思考最多是摘要

- 要开 extended thinking 得显式配置，光靠 prompt 措辞不会触发。
- `display` 只有 `summarized` / `omitted`，没有原文。`omitted` 时 `td` 照来但文本为空，只有 `estimated_tokens`。
  所以思考折叠按「摘要」设计，空的时候显示「思考中」加 token 估计。

### 4.4 全文是权威，`be` 只是渲染时机

中断时 `content_block_stop` 不会为在飞的块到达（`interrupt` golden：`bs` 两个、`be` 一个），
而那一轮的 `assistant` 照样带着完整正文到达，且与磁盘上的会话记录逐字一致。装配规则：

1. 正文以 `at`/`tt` 为准，无条件覆盖增量拼出来的内容。增量丢了不用补。
2. `be` 只决定「这段可以切成 Markdown 渲染了」，不能用来判定内容完整。
3. `res` 到达时封口所有还开着的块，否则被中断的会话里永远挂着一个转圈的块。

### 4.5 `res`：一轮的收口

```json
{"t":"res","sid":"…","ok":true,"why":"completed","cost":0.0421,"turns":2,
 "dur":48211,"usage":{…},"mu":{…},"raw":{…}}
```

- `ok` = `!is_error`，绝不看 `subtype`：未登录时 `subtype` 仍是 `success`。
- `why` = `terminal_reason`：`completed` · `max_turns` · `aborted_streaming`（中断后）· `api_error`（认证失败）。
  「主动停」与「出错了」靠它区分。
- `api` = `api_error_status`；`den` = `permission_denials`（非空时才发）；`mu` 取 `modelUsage`（或 `model_usage`）。
- 只有见到 `res` 才把这一轮置为结束。`be` 只收块不收轮；`res` 之后还可能飘来 `ev`/`rl`。

### 4.6 `ev`：透传

```json
{"t":"ev","k":"stream:message_start","raw":{…}}
{"t":"ev","k":"delta:signature_delta","raw":{…}}
```

- `k` 的前缀标明来源层：`stream:` · `delta:` · `block:` · `system:` · `user-block:` · `cli:` 等。
- 未知的帧型、字段、delta 种类一律：不崩、留原文、照样出帧。每行输入至少出一帧，
  不然界面会永远等不到结果、日志里什么都没有。
- 会落进 `ev` 的已知东西：`signature_delta`（没有 `text` 字段）、`system` 的 `status` 与 hook 事件
  （远端 `settings.json` 里的 hook 会触发，出现在 `init` 之前）。
- 解码端遇到不认识的 `t` 另出 `Unknown`，不与 `ev` 混：`ev` 是编码器认得但没特化，`Unknown` 是对面比自己新。

## 5. 流与磁盘记录

磁盘上的会话记录是正文的权威副本：流式拼出的助手文本与它逐字一致（`tools/cross_validate.py`）。
翻历史读的是磁盘记录，不 tail 正在聊的那份。

| 只在流里有 | 只在磁盘有 |
|---|---|
| `d`/`td`/`ij` 增量、`rl`、`stream:*`、`system:thinking_tokens` | `parentUuid`、`isSidechain`、`gitBranch` |

注意：断线期间的限流事件与思考 token 数不落盘，补不回来。界面不假装它们完整。

## 6. 演进

- 帧内只做加法，新字段带默认值；不改已有字段的语义，要改就换字段名。
- CLI 的新消息先走 `ev`，确实需要特化才加新 `t`。
