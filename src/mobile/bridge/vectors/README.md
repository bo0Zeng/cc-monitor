# golden vectors

真实录制的输出，不是手写样例。Python（`bridge/tests/`）与 Kotlin（`core-claude` 等模块的测试）吃同一份文件。

这些文件随 git 走，不挪去缓存目录，也不进 `.gitignore`。

## 文件

| 文件 | 是什么 |
|---|---|
| `<case>.ndjson` | SDK 消息流。每行 `{t_ns, msg}`，`msg` 保 `__type__` 与全部字段 |
| `<case>.frames.ndjson` | 帧。由 `bridge/tools/reference_encoder.py` 从上者生成，每行 `{t_ns, frame}` |
| `cli-*.cli.ndjson` | CLI 原生 stream-json（管道真实产出的形状），每行 `{t_ns, line}`，由 `bridge/tools/record_cli_vectors.py` 录 |
| `daemon-wire.ndjson` | cc-monitor 后端的 `hello` 与错误形状样本，新旧两版各一份 |
| `daemon-query-projects.ndjson` · `daemon-query-sessions.ndjson` | 后端项目列表与会话列表查询的应答：形状、条数、会话数、时间戳是真实录制的，项目路径、目录名、会话标题与首句换成了造出来的中性值 |

带 `__meta__` 的首行记录录制环境与说明，不是数据。`t_ns` 是相对首行的单调纳秒，供重放时的虚拟时钟用。

## case

| case | 录到什么 |
|---|---|
| `text-short` | 最小纯文本：`init`/`bs`/`d`/`at`/`be`/`res` 齐 |
| `tool-call` | `tu` → `tr` 完整链，含 `ij` |
| `thinking` | 7 个 `td`，共 560 字符（开了 `display: summarized`） |
| `auth-fail` | `err` 的 `code=authentication_failed`，`res.ok=false` 而 `subtype` 仍是 `success` |
| `interrupt` | `res.ok=false`、`why=aborted_streaming`；`bs` 两个而 `be` 一个，留下一个永不收口的块 |
| `long-reply` | 31 个 `d`、2575 字符，平均一块约 83 字符；`signature_delta` 出现在这里 |
| `demo-debug` | 演示素材：一次「排查并修 bug」，7 次工具调用（`Bash`·`Read`·`Edit`） |
| `cli-text-short` | 与 `text-short` 同 prompt 的 CLI 形状，供逐帧对照 |
| `cli-two-turns` | 常驻管道连发两轮：同一个 `session_id`、两轮都没有用户消息回显、第二轮没有 hook 事件但仍有 `init` |

没覆盖到的：Task 子 agent（`p` 非空）、超过 4096 字符的工具结果（`trunc`）、限流拒绝、工具被拒。
这些只有构造输入的单测。

## 两类文件，规矩相反

| | 技术 golden（除 `demo-debug` 外） | 演示素材（`demo-debug`） |
|---|---|---|
| 用途 | 喂测试断言 | 随 app 分发，用户会看到 |
| 编辑 | 逐字节忠实，绝不编辑 | 必须脱敏后编辑 |

- `demo-debug` 已脱敏：命令清单换成内建通用集，路径换成 `/home/you`、`~/projects/inventory`，
  命令输出里的属主用户名也替换了（`__meta__` 里有说明）。脱敏要扫值，不只扫路径形状。
  它不进逐字节断言：拿编辑过的东西当 golden 等于断言自己的编辑。
- 技术 golden 只换值、不动形状：录制机的绝对路径（`init.cwd`、`raw.memory_paths`）里的家目录布局与账号目录换成了中性路径，
  录制账号自己装的 skill 与 MCP 换成了造出来的 `demo-skill-a…h` · `demo-tools`（工具名 `mcp__demo-tools__t01…t15`），
  内建命令原样，条数与顺序不变，没有凭据。`daemon-wire.ndjson` 有脱敏（路径、家目录、`dirName` 都换了，
  `dirName` 是路径的编码形式，最容易漏）。`daemon-query-*` 的项目清单与会话摘录换成了造出来的中性值，条数与数对得上。

## 录到的、影响实现的事实

- 中断时 `be` 不为在飞的块到达，但 `assistant` 照样带着全文到达 ⇒ 正文以 `at`/`tt` 为准，`res` 封口所有开着的块。
  钉住它的测试断言的是「确实会这样」，别当 bug 修掉。
- 思考只能拿到摘要：`display` 不给就是 `omitted`，`td` 照来但文本为空，只有 `estimated_tokens`。
  光靠 prompt 措辞不会触发 extended thinking，要显式配置。
- delta 有四种：`text_delta` · `input_json_delta` · `thinking_delta` · `signature_delta`，后两种没有 `text` 字段。
- `rate_limit_event` 在正常路径上就会来。
- 远端 `settings.json` 里的 hook 会在 `init` 之前产出 hook 事件；还有 `system` 的 `status`、`thinking_tokens`。
  这些都靠 `ev` 透传，每条输入至少出一帧。
- 后端遇到不认识的子命令：stdout 空、错误走 stderr、退出码 2。`hello` 缺 `capabilities` 时按空集处理。
- 流式拼出的助手文本与磁盘会话记录逐字一致（`bridge/tools/cross_validate.py`）。

## 重录 / 加 case

```sh
<sdkvenv>/bin/python bridge/tools/record_vectors.py --case <name>   # 会产生真实 API 调用
python3 bridge/tools/reference_encoder.py --case <name>
```

新 case 在 `record_vectors.py` 的 `CASES` 里加一条，并在上表照实写录到了什么，而不是想录什么。
