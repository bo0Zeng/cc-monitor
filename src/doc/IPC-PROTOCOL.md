# 跨进程协议

两件事：**后端的线上协议**（monitor 与第二个前端都按它接后端）与**本机进程之间的几份文件**（PowerShell 绑窗 · 设置 · 历史注解）。

本页只写不随命令变的总述。每一帧、每一条命令、每一个子命令的逐格形状在 **[IPC-COMMANDS.md](IPC-COMMANDS.md)** —— 那一份从代码生成
（`tests/backend/protocol_doc_gen.rs`，判据「重新生成 == 仓里那份」），改了帧字段或命令登记之后那条判据红、
现打的那份落在 `.build/golden/`，对过后 `bash tests/scripts/bless-golden.sh` 拷回。

## 1. 载体

| 载体 | 谁用 | 怎么接 |
|---|---|---|
| stdio | 本机：Windows 上 monitor 监护的子进程（管道） | `ccm -- --stream …`：后端写 stdout、读 stdin。脱离起的常驻后端（monitor 在 Linux 上起它 · `--resident-ensure`）也以这一行起，但只在下一行的套接字上说话 |
| 常驻套接字 | 本机常驻后端 · 远端常驻后端（本机后端经池里那条 SSH 在那台跑 `ccm -- --resident-attach` 小中继，原样对拷） | Unix 套接字 `<家>/run/backend.sock`（家 = `~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`；目录只给本人，收连接时核对端 uid，没有钥匙）；连上就交一行 `{"attach":true}`（可带 `"tz"` · `"mine"` · `"deflate"` · `"watch"`，见 §2；别的键一律当 malformed；不必等 hello，hello 写在分档之前），再读 hello、读回话：`{"attach":"ok"}`（带了 `mine` ⇒ 多一格 `"relation":"same"|"older"|"newer"|"incomparable"`，这台相对 `mine`，序键是 `BUILD_ID` 的；带了 `"deflate":true` ⇒ 多一格 `"deflate":true`，这一行之后这台写过去的字节整条是一个 raw deflate 流、每帧 full flush（帧之间不回指），客户端 → 这台照原样；远端那条流都要、本机的不要）或 `{"attach":"refused","reason":"malformed-attach"}`（认不得这一形的就是比这一形早的：桌面当旧、装上这一版再接；手机不带 `deflate` 再接一次，接上了照不压走）；中继找不到或够不着那台常驻时第一行就是 `{"attach":"refused","reason":"absent"|"unreachable"}`；这台一份 watcher（会话账本 · 记录读者 · 运行簿整台一份，记录整台只预读一次），每条连接各一份投影（出方向通道 · 名单 · 丢帧账）/ 入方向 / writer |
| 一次性 exec | CLI 子命令（脚本 · skill · 第二个前端的查询） | `ccm -- --子命令 …`，干完即退、不进流 |

后端二进制叫 `ccm`（`~/.cc-monitor/bin/ccm`）：零参数是「起会话」，打头的 `--` 之后才归后端（`<交给 claude 的…> -- <ccm 自己的…>`）。

**线约束**：每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内没有裸 `\n` / `\r`。帧用 `kind` 做标签（snake_case）。

## 2. 起流：时区与声明

一条流要什么只由两样定：看的那一台的时区，与这条流的出口声明。没有别的起流旗标。
流恒这样发：**历史不重放**（各文件从当前行数起只尾随新行；`[0, 当前)` 由客户端经骨架 / 尾段另取：`history-index` · `history-tail` · `history-read`）；
**bg 会话照宣告**（`session_added.background: true`，成不成 tab 由客户端定）；`session_added` 带 `pid`；`line` 不带原文（两个前端都读成品 `record`）。

| 载体 | 怎么交 |
|---|---|
| stdio（CLI 那一臂） | `ccm -- --stream [--tz <IANA 名>] [--view <声明>]`。`--stream` 是「我是流模式」的显式词；`--view` 的值同 §8 CLI 面那一格（JSON 原样或 base64）。声明认不出 ⇒ 不起流：stderr 一行失败信封（`bad_args`）、退出 2 |
| 常驻那条连接 | attach 行 `{"attach":true,"tz":"Asia/Shanghai","mine":"<BUILD_ID>","deflate":true,"watch":{"sids":[…],"branch":[…],"kept":[…]}}`：四格都可缺；`deflate` 见上表「常驻套接字」；`watch` 与 `stream-watch` 的入参同形，这条流从第一帧起照它发；别的键 ⇒ `{"attach":"refused","reason":"malformed-attach"}` |

- **`tz`**：这条流**推**出去的钟面（`line.record.timeText` · `session_runs.runs[].startedText`）按它写。缺 · 认不得 ⇒ UTC。见 §4「时刻按谁的钟」。
- **`view`**（只 CLI 那一臂）：与请求信封的 `view` 同一套词、同一处解（§4）；只许点推送帧里住着的成品：`record`（`line.record`）· `session_added` · `session_status` · `session_left`（整帧就是那件成品，`kind` 不受裁）。
  点了别的成品 ⇒ 当认不出。缺 ＝ 全量。应答不受它管（应答各按各的请求信封）。
- **`mine`**（只常驻那条）：手上这一版的 `BUILD_ID`；回话里那一格 `relation` 由它算，客户端不另问新旧。
- **`watch`**（只常驻那条）：在看名单 `{sids, branch?, kept?}`（同 `stream-watch` 的入参，下一节）；那台接上那一刻就照它定这条连接的名单，宣告之后的运行表 · 行 · 活卡只发名单里的，`branch` 里的会话接上即补清单。缺 ＝ 全看（不报名单的载体）。

### 流只发要看的会话

一条流此刻在看哪几个会话（整份换）：常驻那条连接在 attach 行的 `watch` 里带上，从第一帧起生效；之后变了用 `stream-watch {sids, branch, kept}` 报（同形）；stdio 载体没有 attach 行，接上之后报一次（报之前 ＝ 全看）。客户端名单从空开始（还没人看 ＝ 一个都不看），每台一张名单台（`resident-client` 的 `watch::Desk`）管报、重报与 `from[]`，桌面与手机同一份。不在名单里的会话，它的 `line` · `session_runs` · `session_branch` 不上这条流（后端照读照数，行号不断）；宣告 · 状态 · 去向 · 文件重读 / 不在了 · `changed` 照发。`session_branch` 另要订：只给 `branch` 里列了的会话（`[{sid, path?, at, sum?}]`，交回手上那份的条数与摘要），后端照它只补缺的那一截；不列的会话一帧都不收（手机给每条在看的会话各交一格、手上什么都没有，从头整份收）。`kept`（可缺）：手上留着记录、要那台说清去向的会话 id（桌面 ＝ 重放留存里那台有行的）；那台清单报完了、会话账上没有它（不活 · 不可重连 · 不在等裁）⇒ 只给这条连接一帧 `session_left`（`face.life` 已结束）（清单还没报完 ⇒ 放出 `sessions_replayed` 那一刻答，排在它后面），一条只答一次。「已结束」只由那台答，客户端不自己推。
应答 `from[]` 给刚进名单的会话每份记录一格 `{sid, path, seq}`：这条流从第 `seq` 行起发它，`[0, seq)` 由客户端按骨架补（`history-index` 从上次的末端续）；同时补发一帧整份的运行表与主线外清单。两段合起来不重复也不漏（重叠处按 `(sid, seq)` 去重）。

桌面那一侧（monitor）：每台的名单 ＝ 主窗口当前那个 tab ∪ 开着查看窗跟着的会话；名单一变、或那台换了一条新连接（新连接 ＝ 没报过），就在那台的长连接上重报一次（`src/frontend/shell/src/stream_watch.rs`）。主窗口还没报过之前一台都不报（全看）。
`from[]` 原样进会话流（`watch` 格，没 credit 也照交）：tab 与查看窗补上 `[已有, seq)`（缺得不多按行号取到 `seq`；缺得多 ⇒ tab 整份重来、只取 `seq` 之下一截尾巴，更早的往上翻再取）；壳的留存同一拍修成连着的一段（F5 重放不出洞）。

## 3. hello：先读它，再说话

连上之后后端先发一帧 `hello`，**客户端读到它之前不许写命令**（后端侧 reader 要一个只有 flush 过 hello 才拿得到的见证才起得来）。几个面各管一件事：

- `commands` —— 后端接哪些入方向命令；不在里面的命令客户端**不发**。缺 ＝ 这个后端不读 stdin。
- `unavailable` —— 接得下、但**这台机器上做不到**的命令与原因码（如没有 tmux ⇒ `no_tmux`）；每项带本机后端写好的置灰句 `said`，界面照抄。缺 ＝ 没有把握，照发、看回话；列出来的 ＝ 别画那个按钮。它是提示，后端自己绝不拿它拒命令（同一帧会被这个进程后来的连接共用，读数可以很旧）。
- `shapes` —— 每条 typed 命令的形状指纹（入参 · 回包 · 码 · 撤不撤得动合起来）；跟客户端编进来的那一张对不上的命令不发。撤不撤得动客户端只看自己那一张（指纹对得上 ⇒ 两边一致）。

`build_id` 是这份二进制的身份；monitor 按它判远端后端要不要换。`version` 是这份二进制出自的那一版（仓根 `package.json`），本机后端照它写机器页那台的版本号（缺 ⇒ 说不出）。`host_env` 回显起它的宿主交给它的那几格环境（monitor 据此拒接住在别的家的后端）。

## 4. 入方向：请求 · 应答 · 取消

```text
→ {"id":"<不透明串>","cmd":"<命令名>","args":{…},"within_ms":10000,"tz":"Asia/Shanghai","view":{…},"machine":"<那台>","called":"<这一端对它的称呼>"}   一行一个；args 缺 ＝ null；within_ms · tz · view · machine · called 可缺
← {"kind":"reply","id":"…","ok":true,"data":{…}}                              无返回值时没有 data
← {"kind":"reply","id":"…","ok":false,"code":"…","said":"…","detail":"…"}  少数码另带 data（形状见各命令）
→ {"id":"…","cmd":"cancel","args":{"target":"<要撤的 id>"}}
← {"kind":"cancelled","id":"<被撤的 id>"}
```

- **`id`** 由客户端发号、后端只回显（不解析、不校验）。monitor 的发法是 `<连接 nonce>-<单调序号>`，重连不撞号。同一个 `id` 的命令还在跑 ⇒ `duplicate_id`。
- **`within_ms`**：发起方愿意等多久。后端从收到这一行起减 2 秒余量换成截止时刻，阻塞档命令里装总期限的各处都收紧到它（到点回 `child_timed_out`，见 §CLI 面「期限」那一条）。不是正整数 ⇒ 当没带。
- **`called`**：发这一行的那一端怎么称呼答它的这台（连它的客户端登记的名字：远端是机器表里那台的名字，本机是「本机」那个词）。「在哪」那一句（会话事实 `whereLine`「<目录> · <机器>」）照它写；
  缺 · 不是非空串 ⇒ 当没带，那一句只写目录、不编名字。带了 `machine` 的那一问不带它：转过去那一跳由本后端连那台的客户端带它自己的那一份。
- **`machine`**：这一问是问哪台的（机器表里那台的名字）。缺 · `null` · 空串 ＝ 问这个后端自己；给了 ⇒ 这个后端不自己答，经它持有的那台的常驻流原样转过去
  （命令 · 入参 · `view` 交那台，那台的回包 · 码 · 原话 · `data` 原样回来，`control/feeds.rs::forward`）；那台没有在跑的一条 · 期限内没接上 ⇒ `unreachable`。撤单照本连接的 `id` 撤，转过去那一问随之撤。
- **`view`**：出口的声明 —— 要哪几格、哪几格不要（缺 ＝ 全量）。词表两个（`faces/project.rs::WORDS`）：`cells: {<成品>: [<路径>…]}` 只要这几格（连同通往它们的那几层）·
  `omit: {<成品>: [<路径>…]}` 这几格不要；同一件成品两个都给 ⇒ 先挑后去。成品名与路径照格目录（金样 `tests/__fixtures__/cells-catalog.golden.json`）：`a.b` 嵌套 · `a[]` 列表每项 ·
  `a.*` 以 id 为键的表每项 · `a[k=v]` 列表里按判别格挑的那一种 · `a{k=v}` 非列表的那一种（根上写 `{k=v}`）；**不写挑法 ＝ 每一种都算**
  （`blocks[type=tool_use].input` 同时管 `said` 与 `reply` 两类记录的 `blocks`）。点到某一格的上一层 ＝ 那一层整个；以 `a[]` / `a.*` 收尾 ＝ `a` 这一格整个；
  去掉的格是**删掉**，不置空。认不出的词 · 成品 · 格（往透传的一团里再点格也算）· 或点了这条命令不出的成品 ⇒ 开跑之前就回 `bad_args`，
  认不出的那几处写进 `detail`。出成品、收 `view` 的命令与成品住处（`stream/inbound/views.rs::PLACES`）：`history-read`（`rows[]` 是 `read_row`，其中 `record` 是 `record`）·
  `history-page` / `history-lines`（`lines[].record`）· `history-run`（`rows[].record`）· `history-facts`（整份应答是 `facts`）；别的命令带 `view` 一律 `bad_args`。
  投影只是「这一次不发」：成品与格目录（冻结的那几件也一样）一格不动。
- **`tz`（时刻按谁的钟）**：看的那一台（桌面 · 手机）的时区，IANA 名。回包里写成字的时刻（`…Text` · `timeText` · `tsText` · `startText` · `mtime_text` · 今天 / 昨天那几格）**一律按它写**，不按答话那台后端自己的钟；远端那一跳（这台替看的人问另一台）回来的原始时刻也由这台按它写。
  不是串 · 时区库里没有这个名 · 没带 ⇒ **按 UTC 写**（不拒），但当成「看的那一台说不出自己的时区」：要那一台时区名的地方不冒充成 `UTC`
  （`rotation-view` 的「时区不明」那一条不带 `zone`、句子里照实说一句）。`local` ＝ 看的人就在答话这一台：按这台系统的钟，要那一台时区名时给这台系统的时区名（说不出同上）。替看的人问别台时照算的那一个转过去（没说 ⇒ 明说 `UTC`）。偏移按**那一刻**算（夏令时跟着那一刻，冬天看夏天的记录照样对）；时区库随后端二进制带，各平台同一份。
  为什么是 IANA 名而不是偏移：偏移只说得准「此刻」，历史页 · 记录里的时刻一跨夏令时就差一小时；看的那一台本来就叫得出自己的名（`Intl` · `ZoneId` · 系统设置），给名不多花一个字节的心思。
  原始时刻（`at` · `resetsAt` · `tsMs` · `mtime_secs` …）照旧是数，要它跟着走的「已经多久」由出口按此刻自己走字（见 §7 的时长约定）。
  复制详情（`detail`）里那一行时刻同样按它写，并带偏移（`2026-10-10 04:30:15 +08:00`，跟哪台的日志都对得上）：详情在深处拼好时先留空位，出去那一下（流的写者按这条连接看的那一台的时区 · CLI 按 `--tz`）填。
  轮换策略的分时段按**策略文件头里的时区**判（`tz`，IANA 名；播种 · 升级那一下写上那台的本地时区，取不到标 `tzUnknown`、按 UTC 判），跟此刻有没有人在看都无关。请求信封的 `tz` 只管回包里时刻的字。
- **超时归客户端**：后端零定时器，不替客户端掐表；客户端的期限覆盖「写入 ＋ 等应答」两段，到点就撤单（`cancel`）。
- **取消是一条普通命令**。撤一个不存在的 `id` 也回 `ok`。阻塞档的命令开跑之后打不断，`cancel` 回 `not_cancellable`（去等它自己的应答），不会回一条假的 `cancelled`。
- **应答走独立的小通道**（256 条），与出方向的实时帧（每条连接按字节封顶）分开：丢一条内容帧可恢复，丢一条应答客户端会永远等下去。writer 有界地优先应答。
  同走这条通道的还有几种「丢了就停住」的帧：传输进度（`transfer`）· 测试连接进度（`probe`）· 装后端那一趟的进度（`deploy`）· 终端实时预览的画面与收尾（`terminal_screen` / `terminal_follow_end`）。
  终端画面每条订阅至多一帧在途（客户端回执才推下一帧）、每条连接至多 8 条订阅，单帧至多 512 KiB，所以占不满这条通道。
- **信任边界**：单行上限 1 MiB（超了整行丢、回 `line_too_long`，`id` 从行首至多 4 KiB 里尽力抠）· 坏 JSON 回 `bad_request` 并继续读 · 任何失败都不结束读循环、不结束进程。
- **码分两层**：协议级码（闭集，见 IPC-COMMANDS.md 第 3 节）与命令无关、只有入方向那一层发得出；命令自己的码列在各命令下面。客户端拿到协议级码 ＝ 客户端代码写错了，别重试。
- **登录失败还能怎么办**：`login-start` · `login-finish` 失败时（`bad_args` 除外）`data` 带 `again`：`paste`（作业还在，接着粘）· `restart`（作业没了，重起一趟 `login-start`）·
  `none`（再来也是同样结果）。值由后端照码定、写在码表那一行上（`faces/login_face.rs`）：`not_found` · `unsupported` ⇒ `none`；`failed` · `refused` · `no_job` ⇒ `restart`；
  `bad_paste` · `child_timed_out` ⇒ `paste`。界面照 `again` 画，不按码去猜。
- **失败应答三格**（就是交出去那一形 `cmd_wire::failed::Failed`）：`code` 给程序认（分支 · 重试判断）；`said` 是给人看的那一句（后端按共享文案表 `src/shared/copy/table.json` 写好，前端原样上屏）；
  `detail` 是「复制详情」那几行（时刻 · 机器 · 命令 · 码 · 原话，排法只住 `copy_core::detail`），下层原话只进这里、不进 `said`。

## 5. 会话流：帧的先后与续传

1. 连上：`hello` → 每个活会话一帧 `session_added`（宣告时带那条会话的整份脸 `face`；在等什么只由 `session_status` 带；后台会话带 `background: true`）→ `sessions_replayed`（「清单报完了」：分清「还没说完」与「说完了、里面没有它」）。
   `session_added.agent_kind` 每条都带（那一家的 kind）；脸上的 `account` 是这条会话跑在账号库里哪个号下（宣告时读会话进程环境里的配置目录判一次，之后那几帧的脸照带；判不了 ⇒ 缺），界面的账号徽章只读它，起停不回头问。哪几家进流、各自怎么判活由适配层在注册表那一行声明（`Adapter::live`：认 pidfile · 认开着写 · 判不了），
   冷读（`history-facts` 的 `writers` 与「状态不明」的原因 · `history-turns` 还在跑那一轮）问同一个声明（`accounts_query::session_holders`）：
   - 有 pidfile 的那一家（Claude Code）：活会话 ＝ pidfile 在、进程在。
   - 没有 pidfile 的那一家（Codex）：活会话 ＝ **有进程开着它的那份记录写**（看 `/proc/<pid>/fdinfo` 的打开方式，不看进程名）；
     那个进程退了（pidfd）或把记录写完关闭、复核已没人开着写 ⇒ 摘下它，去向由会话账本裁好发 `session_left`。「开着它写」是系统给的事实、再加 pidfd 看守，
     与 pidfile 那一路同一档。
     ⚠ 「Codex 在世期间一直开着这份 rollout」只读过源码（codex-rs 的 rollout 写者任务持有文件），**没在真机上实测过**。
     它要是不成立（中途关了文件），后果是漏认 / 提早摘掉，不会把死的认成活的。
     扫进程表只在三种时候：记录根下新建 / 打开了一份还没认的记录（按事件攒批：一次醒来读完的那一批里开了又关的当场抵掉，读的人不引出扫描）·
     起步与「重新对齐」· 内核事件队列溢出。已知口子：一份很大的记录正被人读到一半时恰好赶上那一批，会多扫一趟、什么帧都不出（不误报）。
     只在 Linux 上判得了；别的平台照实判不了，这一家的会话不宣告。
2. 内容：每条记录一帧 `line`，带成品 `record`（通用记录，见下面「通用记录」一节；缺 ＝ 不进界面、照占号）、`seq`（本条流里按文件单调递增）与 `byte_offset`（这一行末尾在文件里的累计字节）。
   **续传用 `byte_offset`，不用 `seq`**：断线重连后拿它当偏移再读（`history-read`）。
3. 状态：`session_status`（红绿灯变了才发，天然稀疏；带整份脸 `face`：点 · 短名 · 悬停句 · 状态一句 · 序 · 能做什么，在干什么在 `face.life.activity` ＝ `working` · `needs_you` · `idle` · `background_work`（一轮停了、它在后台起的命令还在跑），后端适配层从那一家的进程状态翻过来；没有进程状态文件的那一家照它记录里的事件翻（一轮开始 ⇒ `working`，一轮完了 / 中止 ⇒ `idle`），翻不出就不带；一轮在跑时 `face.can.stop` 是 `yes`，停止键照它画）· `session_runs`（子运行表，整份）· `changed`（「这台的某样东西变了」，下面「`changed` 主题表」）。
4. 离开：只一帧 `session_left {sid, face}`，去向在 `face.life`（`reconnectable` 容器还在、接得回去 · `ended` 只能 resume，同一个 pidfile 原地换了 sid ⇒ `ended {by}` 带顶替它的那条 · `gone` 记录也不在了）。这台后端自己裁：宣告时就知道不在 tmux 里 ⇒ 当场发；在 tmux 里 · 判不了 ⇒ 等摘除之后那份 tmux 观测回来再发。
5. 记录文件被动过：`session_file_gone`（不见了，不误判结束）· `session_file_reread`（被截短 / 改写过、已从头重读；重读的行不逐行上流，帧带读完的行数，客户端照宣告那样从尾段取）。两帧都带后端写好的那一句 `said`，两端照摆、不照 `why` 自己映。
6. 背压：实时通道满时丢帧，排空后发一帧 `overflow`，只带那台写好的一句 `said`（丢了约几行 · 会话状态已重新对齐 · 两样都有）。`line` 丢了可以从记录文件补；会话状态那一类（`session_added` / `session_status` / `session_left`）与不可丢主题的 `changed` 丢了，那台紧跟着在同一条连接上把此刻的样子整份重报一次（`changed` 再发一帧、不带体，客户端重问），客户端不判、不重接。
7. 别台：各台的常驻流归本机后端（`control/feeds.rs`），那台流上的事原样转给连着本机后端的每条流连接，一种帧 `relayed`：`machine` 说是哪一台，`what` 分三形 ——`line`（那台流上的一行原样，字符串；每条连接先来它的 hello）· `gap`（那台流上超长丢了一行，或这一端挤丢了一行）· `ended`（那台这一条连接断了，下一条从 hello 起）。不进 `overflow`：挤丢了 ⇒ 这条连接在那台下一帧之前先收那台一格 `gap`（断了的那一帧挤丢 ⇒ 补 `ended`）。

### `changed` 主题表

「X 变了 ⇒ 重读」只一种帧：`{"kind":"changed","topic":…, "key"?, "rev"?, "body"?}`。`topic` 是哪一样；`key` 是哪一个；`rev` 是变成了哪一版；
`body` 是那一样的小成品（带了就不用重问，超了上限就不带）。没拿到 `body` ⇒ 重问表里那条查询（那一份的唯一出口仍是那条查询）。
表只住后端一处（`wire/topic.rs`）；壳不认主题，按 `topic` 原样扇给订了 `changed/<topic>` 的界面订阅（格体 ＝ 帧去掉 `kind` / `topic`：`{key?, rev?, body?}`）。

<!-- topic-table:begin -->
| topic | 可丢 | key | rev | body | 重问 |
|---|---|---|---|---|---|
| `accounts` | 否 | — | — | — | `accounts-list` |
| `profiles` | 否 | — | — | — | `profiles-read` |
| `quota` | 是 | — | — | 带（≤ 8192 B） | `quota-read` |
| `rotation` | 是 | 带 | — | — | `rotation-view` |
| `rotation_strategies` | 是 | — | — | 带（≤ 8192 B） | `rotation-strategies` |
| `plan` | 是 | 带 | 带 | 带（≤ 64 B） | `plan-read` |
| `tasks` | 否 | 带 | — | 带（≤ 8192 B） | `tasks-list` |
| `needs` | 否 | 带 | — | — | `sessions-needs` |
| `login` | 否 | 带 | — | 带（≤ 8192 B） | `sessions-needs` |
| `machines` | 是 | — | — | — | `machines-read` |
| `machine` | 是 | 带 | — | 带（≤ 8192 B） | `machine-read` |
| `empty` | 是 | — | — | 带（≤ 1024 B） | `machine-read` |
| `session_hidden` | 否 | 带 | — | 带（≤ 1024 B） | `history-list` |
| `files_dir` | 是 | 带 | — | — | `files-ls` |
| `history_list` | 是 | — | — | — | `history-list` |
| `groups` | 否 | — | — | — | `groups-read` |
| `session_queue` | 否 | 带 | — | 带（≤ 16384 B） | `session-queue-read` |
<!-- topic-table:end -->

`key`：`rotation` · `tasks` · `needs` 是会话 id，`plan` 是工作区根。`needs`：需手动可能变了，进程里只一个铃（`history/needs_changed.rs`），从 watcher 的出方向发、不可丢。带 `key` ＝ 那个会话（挂着的请求挂上 · 撤掉 · 答掉 · 「没人答时自动答」改了 · 那台说进出等人 · 离开「活」），重问 `sessions-needs` 与那个会话的 `history-facts`；不带 `key` ＝ 会话外那几项（账号清单动了 · 某个号记「MCP 要登录」的那份文件翻了 · 一份登录作业收了 · 续令牌没成）或计划要人看的数变了，重问 `sessions-needs`（会外那一截在它的整份里）。`rev`：`plan` 是新的输出摘要（同 `plan-read` 的 `rev`），与手上那一份相同 ⇒ 不用问。
`body`：`plan` 是 `{needs}`（这个工作区此刻需手动看的数：没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）；其余带 `body` 的就是「重问」那条命令的应答（`tasks` 问 `{sid: key}`、另两样不带参数；`rotation` 不带，客户端照 `{target: {sid: key}}` 重问 `rotation-view`），由同一个处理器在发帧时现算，超了上限或答不成就不带。
账号清单不带（`accounts-list` 要客户端说是哪一家）；配置文件不带（整份大）。
`login`：一份登录作业自己收了（本机浏览器回调登完 · 登录子命令先退了），`key` 是作业号，`body` 是 `{state: "done"}` 或 `{state: "failed", said, detail}`；只推给发起它（`login-start`）的那条连接。`machines`：这一端的机器表被改写（`machines-set` · 换形那一次），重问 `machines-read`；monitor 照它对齐远端流。`machine`：这一端连的某台远端此刻的状态变了（接常驻那一轮 · 部署走到哪一步 · 「连接这台」开关），`key` 是那台的名字，`body` 是那台的状态成品（同 `machine-read` 里那一格）；本机常驻后端才推。
`session_hidden`：这台某个活会话隐不隐藏变了（注解 · 这台偏好 `childTab` / `bgTab` · 会话血缘动了之后重判的），`key` 是会话 id，`body` 是 `{hidden}`（那一格 `{by, said}`，不隐藏 ⇒ `null`；判定只住后端一处，`session_added` · `history-list` 每行 · `sessions-needs` 每行带的是同一格），收到就换、不重读整份清单；万一没收到，重问 `history-list {sid}` 那一行。
`files_dir`：这台某个正被浏览的目录（名单由 `files-browse` 交）里有了动静，`key` 是那个目录（UTF-8 ⇒ 路径本身；不是 ⇒ `\0` ＋ 十六进制小写）；正开着那个目录的那一屏带着手上的记号（`held`）重问 `files-ls`，回包只带变了的那几行。
`history_list`：这台的会话清单可能动了（会话来去 · 名字 / 状态变 · 记录长了 · 注解 · 隐藏规则）；同一个会话在这台答下一份清单之前只说一次（记录每长一行不是每行一帧），连着几下没取走也只一帧。正看着清单的那一屏带着手上的记号（`held`）重问 `history-list`，回包只带变了的那几行；看的是远端那台的，本机后端带 `fresh` 先去同步副本。
`groups`：这台的组那一半动了（这台那几片的组建 · 改名 · 解散，这台的会话进出组 · 固定变了；注解文件重读时由 watcher 判，只改名字 · 星标不推；本机后端那几条常驻流里哪台接上 · 断开也推一声）；看着组的那一屏重问本机后端 `groups-read`（它自己问到各台再并）。
`session_queue`：这台某条会话的输入队动了（投进 · 撤 · 送进 · 标未送达，哪一端、哪个进程写的都算；watcher 盯 `~/.cc-monitor/session-queue/`，一批里动过的每条会话各推一次）；`key` 是会话 id，`body` 是整份名单（同 `session-queue-read` 问这条会话的应答，由那条命令在这台那一半现算）；各端只照这一份画队、不各自记，超了上限或读不出就不带、重问 `session-queue-read`。
可丢的走 tap 那条（丢了下一次变化或重问就补上），不可丢的走 watcher 的出方向（丢了那台就地重报时再发一帧）；`login` 走发起它的那条连接的应答通道（不丢）。带了 `body` 不改可丢性。

历史不重放 ⇒ 客户端先取快照（尾部优先：`history-tail`）。宣告不等记录：`session_added` 一认出来就发，记录在后台预读，读完发 `session_primed {sid, path?, lines?}` —— `session_primed.lines` 是那一刻那份记录的完整行数，这条会话的 `line` 从这一行起来（之前一帧都不发，`session_runs` · `session_branch` 同），客户端在这一帧排快照、拿它核快照拉全了没有。半路接上来的连接先收一份此刻的样子：每个活会话一帧 `session_added`（现值）、读完了的再一帧 `session_primed`（行数是此刻已计完的）、运行表 · 可重连的 `session_left`（主线外清单不在里面：报名单时照 `branch` 给） · 清单已报完就一帧 `sessions_replayed`。

## 6. 会话归属与身份门

- **`@ccm_sid`**：tmux 窗格上的会话标记。一个 tmux 会话可挂几个窗格、各挂各的 sid；送字 / 抓屏 / 结束都落在挂着那个 sid 的窗格上，身份也按那个窗格判（活动窗格是谁不算数）。
- **`@ccm_client`**：会话由谁起。用户在终端里敲的 ccm / 别名起的写 `ccm`（谁都能动）；某个前端为自己内部起的写那个前端的名字（1–32 个 `[a-z0-9-]`）。
  请求可带 `args.client` 自报是哪个前端：没声明 / 声明成 `ccm` ⇒ 这一维不拦；声明的就是自报的 ⇒ 放；声明了别的 ⇒ `wrong_owner`（别的前端起的，这里只能看）。
  `kill` · `sessions-*` · `session-open`（就地键入那一形）· `terminal-input` 共用这一维；它防误动，不是安全边界。
- **破坏性动作三道门**：名字精确匹配（`=name:`，不许含 `:` / `=` / 控制字符）⇒ 名字像我们铸的（`cc-*` / `<X>-cc`）或已挂 `@ccm_sid` ⇒ 只有一个窗口（只给杀会话）。杀的是 `#{session_id}` 句柄，不是名字。
- **抓屏不过身份门、恒可用**：`terminal-preview` 只读，谁问都给（身份门只管送字 / 结束）⇒ `terminals-list` 每行的 `can` 里只有会变的 `input` · `end`，没有 `preview` 那一格。
  `terminal-input` 只收 `terminal` | `sid` · `text` · `enter` · `key` · `seen_screen` · `client`，多送一格 ⇒ `bad_args`。
- **终端句柄**：`terminals-list` 每行的 `terminal` 是不透明句柄，前端不拼、不解析；后端不收任意 tmux 目标串，句柄 / sid 先在那一刻的名单里对上才动手。
- **送字的回话不带画面**：`terminal-input` 回 `delivered` 时**不带** `screen`（`screen` 只跟 `refused` ＋ `screen_changed` 一起回，是那一刻的新指纹）。
  要连着按，要么订 `terminal-follow`（只在帧面），要么每按一下之前问一次 `terminal-preview` 拿新指纹。重抓的节拍归前端。

## 7. 两个前端共吃的冻结面

桌面端（monitor）与第二个前端（手机端）吃同一个后端。第二个前端按下面这些格读，缺一格就把整帧当坏帧丢 ⇒ 它们**冻结**：不许改名、删、换类型，只许加新字段；非改不可就两边同拍。

- 帧：`hello` `build_id` `host_arch` · `line` `session_id` `path` `seq` `byte_offset` `record` ·
  `overflow` `said` · `live` `sid` `run` `card` `n` `op` `rid` `phase` `clipped` `blocks` ·
  `reply` `id` `ok` `code` `said` `detail` `data` · `cancelled` `id` · 请求信封 `id` `cmd` `args` `within_ms`（可缺）`tz`（可缺）`view`（可缺）`machine`（可缺）。
- 会话三帧与成品面在格目录里：每格带 `frozen`。`session_added`（`sid` `path` `cwd` `name` `agent_kind` `attachable` `background` 与整份脸 `face` 里手机读的那几格）·
  `session_status`（`sid` `waiting_for` 与 `face` 那几格）· `session_left`（`sid` 与 `face` 那几格，连 `face.life.by`）只冻这几格；
  `line.record` 与 `history-read` 的 `rows`（`end` `hash` `record` `cwd`）整件冻结 —— 通用记录的公共格、六类各自的格、`who`、`error`、各种内容块逐格冻结（见下面「通用记录」一节）；
  `history-page` / `history-lines` / `history-run` 的 `record` 是同一形；
  `quota-read`（`quota_read`：每号的 `rows` · `fiveHour` · `warm` 与号名表 `names` 等）· `history-list`（`history_list`：每行的标题 · 时刻 · 段头 · 注解三格 · 整份脸那几格等）只冻手机读的那几格。新加一格随格目录金样重写（`cells_catalog_tests::the_golden_is_what_the_command_writes`，删 / 改名 / 换类型 / 解冻重写也不放行）。
  `line.raw` 逐字节等于记录文件里那一行，去掉行尾（`\n`；CRLF 行连 `\r` 一起去）。
- **改了冻结的格就得打版本号**（同加子命令）：`build_id_guard` 的指纹带一行冻结格的摘要 ⇒ 冻结格一变 `BUILD_ID` 就变。第二个前端连上不逐格核：attach 行带手上这一版，那台在回话里说相对它的新旧，能不能用逐条命令按 hello 里的形状指纹判。
- 一次性子命令：第二个前端只调 `--resident-ensure`（可带 `--replace`）· `--resident-attach`，其余都走常驻流上的帧命令。
- 会话是不是后台、此刻在干什么只看后端判好的 `background` · `face`；那一家的原词不上线。
- 判据：`wire_tests::the_shapes_the_second_frontend_reads_stay_put`（帧那张表，类型逐格对）· `cells_catalog_tests::the_golden_is_what_the_command_writes`（格目录里冻结的格）· `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump`（子命令集或冻结格变了 ⇒ 打版本号）· `wire_tests::the_subcommands_the_second_frontend_calls_stay_put`（子命令那两条）。
- 跨语言金样：`tests/__fixtures__/session-stream.golden.jsonl`，每种帧两行（「全格」与「最少格」），由后端真序列化器写；最少格里的格就是必填格。
  终端管理 `tests/__fixtures__/terminals.golden.json`。
- 部署：第二个前端不在重连循环里自己装。它连上一台走 `--resident-attach`，attach 回话说那台相对自己这一版的新旧：比自己旧 ⇒ 照接、只发形状指纹对得上的命令、提示那台要更新，人按［安装］/［更新］才从包里那两份 Linux 后端装上去（同桌面那一份 `deploy` crate）；桌面那一端的本机后端在它那几条常驻流接不上或那台不是这一版时自己走 `deploy-install` 那一趟。Release 资产名（含两个 musl 后端与 `SHA256SUMS-linux.txt`）登记在 [RELEASING.md](RELEASING.md)。

### 通用记录

会话记录里一行在界面里是什么，由那一家的适配层翻成这一形（形状 `src/common/cmd-wire/src/record.rs`，两侧同一份；Claude 的翻译表 `agents/claudecode/record_of.rs`，Codex 的 `agents/codex/record.rs`）。
两个前端都只按 `t` 与格排版，不认任何一家的盘上格式。每类全格 ＋ 最少格的样本：`tests/__fixtures__/record.golden.jsonl`（后端真序列化器写，`record_of_tests` 钉着）。Codex 那一家的样本是一整份造的中性 rollout（`tests/__fixtures__/codex-rollout.sample.jsonl`）走读页那一遍写成的 `tests/__fixtures__/record-codex.golden.jsonl`（`agents/codex` 的 `record_tests` 钉着；界面 `codex-record.vitest.ts`、手机 `CodexRecordGoldenTest` 解同一份）；它的 `function_call` 入参盘上是 `arguments` 那个 JSON 串，适配层解成对象放进 `tool_use.input`（解不出 ⇒ 原串包成 `{input}`）。

公共格（每类都有）：

| 格 | 类型 | 说明 |
|---|---|---|
| `agent` | string | 哪一家（`claude` · `codex` …） |
| `id` | string | 这条记录在本会话里的身份：**不透明串**（非空、会话内唯一；主线外清单、分叉、跳转都按它认），前端不解析、不拼。那一家的记录自己有身份就用它（Claude 的 `uuid`）；没有的合成「`@<这一行起点的字节偏移>`」 |
| `at` | string? | 记录时刻（ISO-8601 原样）；一轮中途插进去的那一句（`said`）是打字那一刻 |
| `timeText` | string? | `at` 在那台本地钟上的钟面 `HH:MM`（界面照抄、不换算） |
| `t` | string | 类别：`said` · `reply` · `retry` · `title` · `unread` |

各类的格：

| 类 | 格 |
|---|---|
| `said`（人那一侧说的：人 · 工具结果 · 派活 · 来话 · 通知 …；人在一轮跑着时插进去的一句也是它，在它自己那一行的位置上） | `who`（`UserText`：`speaker` 谁说的，`kind` 闭集 `human` `slashCommand` `bashInput` `bashOutput` `commandOutput` `taskNotification` `agentMessage` `peerSession` `coordinator` `agentTask` `system` `compactSummary` `interrupt` `toolResult`；`text` 要显示的正文；`pasted?` 粘贴块）· `blocks` · `results?`（工具调用 id ⇒ 结果一句：`ok` `rejected?` `lines?` `added?` `removed?` `files?` `answer?` `exitCode?` `file?` `patch?`（算好的行差 `{rows: [{kind: add|del|ctx|head, text, oldNo?, newNo?}], shown, truncated, added, removed, stat}`：照 structuredPatch、带真行号，至多 32 KiB、整段不劈）· `cwd?` |
| `reply`（代理的回复） | `blocks` · `model?` · `autoReply`（代理那一侧自动写的应答，不建卡）· `cards?`（工具调用 id ⇒ 卡型 `agent` `interactive` `diff` `md` `command`）· `steps?`（工具调用 id ⇒ 过程一行：`tool` · `arg?` · `path?` · `note?` · `known`；展开画的 `body?`：改动 `{kind: changes, diff: 行差}`（同上形，照入参改前改后算，行号缺；结果带着 `patch` ⇒ 两端改画结果那份） · 命令 `{kind: command, line}`；可在文件窗口定位的绝对路径 `file?`。界面只按这几格画，不读 `tool_use.input` 的键）· `runs?`（工具调用 id ⇒ 派出的子运行 `{label, kind?}`）· `error?`（上游最终失败写的报错：`status?` HTTP 状态码 · `title` 写好的卡头那一句；报错正文在 `blocks` 里） |
| `retry`（上游失败、将重试） | `text?`（写好的那一行「{原因} · 重试 a/b · {钟面} 起」，出口照看的那一台的时区写） |
| `title`（会话标题） | `text` · `by`（`agent` 代理起的 · `user` 人起的） |
| `unread`（这一家的这一行认不出：没见过的类型 · 见过的类型形状变了） | `why`（闭集 `unknownType` `parseFailed`）· `type`（那一行自己说的类型原样，只当标签、出口不按它取字；没有 ⇒ `null`）· `text`（写好的一句）· `tone`（恒 `warn`）· `excerpt`（原文截到 400 字节、按字符截，给［复制详情］，出口照抄不解析）· `count`（并了几条：出记录页那一遍把相邻、原因与类型都一样的并成一条；实时 `line` 帧每行一条、恒 1） |

内容块 `blocks[]`（两家共有的词，按 `type`）：`prose {doc, more}`（助手正文：后端解析好的块树 ＋ 写好的「展开 {大小}」那一格，两端照放）· `text {text, bytes}` · `thinking {text}` · `tool_use {id, name, input}` · `tool_result {for, content: [块], isError, bytes}` · `image {source}`（`bytes` ＝ UTF-8 字节，结果块是内容各块之和）。

- **过程一行的主参数 `steps.*.arg` 是定长一行**：换行与连串空白压成一个空格，至多 200 字（按字符，不切半个字），截了以 `…` 收尾；两个前端都不再截。完整内容照旧在 `blocks` 的入参里。
- 要哪几格由出口交的声明定（请求信封的 `view`，见 §4）：折起那一行只要 `omit: {record: ["blocks", "results.*.patch"]}`；
  正文按需（工具卡展开时再取那一行全文）交 `omit: {record: ["blocks[type=tool_use].input", "blocks[type=tool_result].content", "results.*.patch"]}`。`line` 不带原文。
- 主线外清单（ESC 回退掉的那几条记录的 `id`）不在记录上，单独给：实时帧 `session_branch {sid, path, at, off, sum}` ——「这份记录的清单前 `at` 条照旧，其后换成 `off`」，`sum` ＝ 到这一帧末尾那一截的摘要（FNV-1a 64，客户端只记、在 `stream-watch.branch` 里交回）；变了只发变了的那一截；刚订上时照交回的那份补缺的，对不上 · 换了记录 · 这条连接丢过它一帧 ⇒ 从 `at = 0` 起整份，按页切几帧接着发 · 冷读 `history-branch`（同样分页）。没有链的那一家恒空。
- 分页（`history-index` · `-user-inputs` · `-turns` · `-branch`）：一条应答至多 `REPLY_PAGE_BYTES` 字节（一项单独超了 ⇒ 那一页只有它）；有下一页 ⇒ `next` 是下一页的续点（前三条是字节、`-branch` 是条数），没有 ⇒ `null`；入参可缺的 `limit` 管一页至多几项。写出方向写完一页应答就先让实时帧走 ⇒ 分页取到一半时别的会话来的行至多再等一页。

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
- **声明**：`--view <声明>`（任意位置）与帧面请求信封的 `view` 同名同义、同一处解、同一处拒、同一处裁（§4）。值以 `{` 打头 ＝ JSON 原样，否则当 base64（标准字母表）解；
  缺值 · 给两次 · 解不出 · 声明认不出 ⇒ `bad_args`，在读入参之前就拒。
- **时区**：`--tz <IANA 名>`（任意位置）与帧面请求信封的 `tz` 同名同义：回包里写成字的时刻按看的那一台的时区写；缺值 · 认不得 ⇒ UTC。
  另认一个词 `local`：看的人就在答话这一台上 ⇒ 按这台系统的钟（帧面信封的 `tz` 同样认它）。**没带 `--tz` 就是它**（一次性命令与 `--stream` 起参同此）：
  argv 只有这台上的进程起得了（终端里敲的 `ccm -- --<命令> --text`、本机脚本、壳起本机后端）；替看的人问别台的那一跳恒交 `--tz <IANA 名>`（「这一台」交这台系统的名，说不出交 `UTC`）。
- **期限**：`--within-ms <毫秒>`（任意位置）与帧面请求信封的 `within_ms` 同名同义：减 2 秒余量换成截止时刻，装总期限的命令都收紧到它，到点回的码与帧面相同（`child_timed_out`）。
  哪几条装总期限、上限多少，协议参考里逐条写（「总期限上限 N 秒」）；没写的那几条不装，带了期限对它不起作用。到点只回 `child_timed_out` 这一个码，三条例外不整条失败：`aliases-read` · `powershell-policy-set` 落在成品的 `policy.error`，`ssh-config-import` 交已解析的那几个。
  缺值 · 给两次 ⇒ `bad_args`；值不是正整数 ⇒ 当没带（同帧面那一格）。
- **stderr 是协议通道**：成功 ⇒ stderr 一个字节都没有；失败 ⇒ 正好一行信封（下一条）。后端自己的诊断日志一行都不写 stderr，
  整行追加进那台后端的 stderr 诊断文件（宿主交了 `CCM_BACKEND_STDERR_LOG` 就是那一份，否则 `~/.cc-monitor/logs/backend/stderr.log`，常驻后端的 stderr 落的也是它；
  那一层目录不在 ⇒ 诊断丢掉，不替它建）。调用方可以整段 JSON-parse stderr。
- 失败：stderr 一行 `{code, said, detail}`（少数码另带 `data`）、退出 2 —— 与帧面失败应答出自同一份：同一个失败两个面上 `code` · `said` · `detail` · `data` 逐字相同（`detail` 恒在）。
  带 `--text` ⇒ stderr 是那一句 ＋ 下面原样接 `detail` 那几行（同界面「复制详情」复制出去的那一段），不是 JSON。
  手写的几条（`--resident-*` · `--tmux-notify`）失败也是这一份；不认识的子命令 ⇒ `unknown_command`，同一份信封。
- 路径入参（`path`）过同一套路径围栏（只许落在那台的会话记录树里）。

## 9. 本机进程之间的文件

全在 `~/.cc-monitor/` 下（路径越界一律不许）。共同约束：**UTF-8 无 BOM**（PowerShell 5.1 的 `Out-File -Encoding utf8` 会写 BOM；写端用 `[IO.File]::WriteAllText(…, UTF8Encoding($false))`，读端也剥一次）·
**原子写**（Rust：临时文件 → 原子替换；PowerShell：单次 `WriteAllText`）· 目录不在就建 · 未知字段忽略。

`~/.cc-monitor/` 下有哪些文件、谁写、删了会不会丢，只看 [INVARIANTS.md](INVARIANTS.md) §2.1 那一张（判据两向对拍）；后端自己那几份的形状写在写它的模块头注里（历史注解：`src/backend/history/history_annotations.rs`），版本与读法见 INVARIANTS §2.2；`config.json` 的顶层键全集是 `src/frontend/ui/config.ts` 的 `CONFIG_KEY_OWNERS`。
这里只写几个进程之间交接的那几份的格式：

| 文件 | 格式 |
|---|---|
| `ps-await/<PID>.tty` | `{shell_pid, proc_start, tty}`：Linux bash / zsh 接入块（`src/shared/ccm-aliases.sh`）请 monitor 去认这个终端的窗口；认上 / 认不出就删 |
| `ps-registry/<PID>.json` | `{ps_pid, hwnd, owner_pid, owner_proc_start, ps_proc_start, title_at_bind, registered_at}`：那个 shell 显示在哪个窗口；与 shell 进程同寿 |
| `auto-launch.json` | `{auto_launch_enabled, monitor_exe_path}`：PowerShell 接入块 `__ccm_bind` 读，用 `cc` 起 claude 时要不要顺手开 monitor |
| `opened-windows/<sid>` | 两行：开那个终端的 shell 的标签 `<进程号>-<起始时刻>`、那台的名字（下面「切到终端」） |

设置窗机器列表那一格的成品（`state` · `word` · `said` · `fixes` …）：各台（含本机，键 `<local>`）判定只在后端 `deploy/machine.rs`（本机常驻后端答 `machine-read`、推 `changed {machine}`；各台的常驻流由本机常驻后端自己持有（`control/feeds.rs`），每一轮的结论它自己记，接不上或那台不是这一版时它先 `deploy-install` 再请那台换；各处［重新连接］与机器页「起」·［重试］是 `machine-wake`（回那台此刻的成品；拒码 `unknown_machine` · `disabled` · `busy`（带那一刻的成品）），「停」是 `machine-stop`）。本机后端没起来时没有成品，壳只说它起没起来（`local-backend` 事件 · `backend_status` 的 `local`：`up` · `starting` · `down` · `older` · `newer`）；成品形状以金样 `tests/__fixtures__/machine-state.golden.json` 为准。

**Claude Code 自己写、后端只读的两份**（集成方要自己写 pidfile 时看这里）：

- `<claude_dir>/sessions/<PID>.json`：活会话登记表。`kind` 是**排他式**的 —— 缺席或 `"interactive"` 放行、`"bg"` 照宣告（`session_added.background: true`）、**其它任何值都被隐藏**（想让会话可见，`kind` 要么不写、要么写 `interactive`；同一个 pidfile 的 `kind` 翻成别的值等于让会话消失）。
  `attachable: false` ⇒ 不提供 attach / 拉前 / 「杀死空 tmux」，缺席 ＝ `true`。`procStart` 参与 PID 复用检测：缺了就退化成只看进程在不在。
- `<claude_dir>/tasks/<sid>/<id>.json`：任务清单（`<digits>.json` 才算，`.lock` / `.highwatermark` 忽略；读到半截 JSON 单条跳过）。后端盯这棵树，变了发 `changed {tasks, key: sid}`，客户端重问 `tasks-list`。

## 切到终端：此刻显示这个会话的是哪个窗口

点 ↗ 那一刻本机后端答一次这条会话在这台电脑上的哪儿（`session-window`：显示它的终端带的窗口标签 `LC_CCM_WINDOW` · 那个终端进程往上的进程链，或写好的那一张；
本机会话的链从连着它的 tmux 客户端 · 不在 tmux 里就是它自己起，远端会话由本机后端问那台谁连着它、拿这台的连接表对出开着那条连接的进程）。
远端会话的终端是 cc-monitor 开的 ⇒ 先不问那台：开的那一刻（`terminal-ssh` 带了 `sid`）那一行前面接一段，跑它的那个 shell 把自己的标签
（`<进程号>-<起始时刻>`）与那台的名字写进 `<家>/opened-windows/<sid>`（两行，原子写；短暂：那个 shell 一退，下一次 ↗ 就删）；
↗ 时这台的进程表里那个 shell 还是它、它自己或底下开着 ssh ⇒ 从那个 ssh 往上的链就是答话，不问那台。对不上才照上面问那台。
最后一跳在 monitor（`bind.rs::bring_session_window`）：先按标签，再沿链从下往上（`pick_chain_window`），
每一级先问「它显示在哪个窗口」，再看它名下的可见顶层窗口，碰到终端本身就停（恰好一个窗口 ⇒ 它，几个 ⇒ 照实说分不清）。

「它显示在哪个窗口」：
- **Windows**：借那个进程的控制台问一句（`AttachConsole` → `GetConsoleWindow` → 属主）。借的是本二进制的一次性小助手（隐藏子命令
  `--ccm-console windows <进程号…>`，每个进程答一行 `<pid> shown 0x<窗口>` · `hidden` · `denied` · `none`），monitor 自己不挂别人的控制台。
  Windows Terminal 每个标签的伪控制台窗口的属主就是承载它的那个窗口；经典控制台就是控制台窗口自己。不要接入块、不要登记。
  Windows Terminal 的窗口里再切到那个标签页：小助手（`--ccm-console tab <进程号> 0x<窗口>`，答一行 `selected` · `single` · `miss <原因>`）
  借它的控制台挂记号标题，UI Automation 找名字带记号的标签页选中，原标题写回。小助手自己有 2 秒预算：到点（UI Automation 卡住）就先写回标题、
  答 `miss no-uia`、退出；monitor 至多等它 3 秒，到点只是不再等、不掐它（掐断会把记号留在标题上）。
- **Linux（X11）**：bash / zsh 接入块在本机桌面上开的 shell 里留 `ps-await/<PID>.tty`，monitor 往那个终端写改标题序列挂记号、按标题找窗口，
  写进 `ps-registry/`（标题出栈还原）。

## 加一条协议

- **线上命令**：在 `src/backend/stream/inbound/registry/` 对应那一族加一条 `CommandSpec`（名字 · 一句话 · 码 · 字段 ＋ 向 ＋ 一句话 · 收不收载荷 · 怎么跑），CLI 面自动派生（子命令闸门 `stream::cli::is_subcommand` 跟着认）；重生成 `IPC-COMMANDS.md`。
- **帧字段**：加在 `src/common/cmd-wire/src/frame.rs` 的 `Frame` 那一臂的类型上，写一句 `///`；只加不改；进金样；重生成。
- **CLI 独有子命令**：进 `stream/oneshot.rs` 的 `ONESHOTS`（子命令 ＋ 用法 ＋ 一句话 ＋ 实现），闸门照它认。
- **文件**：只许在 `~/.cc-monitor/` 下；UTF-8 无 BOM、原子写、未知字段忽略；写明短暂还是持久，短暂的写明超时；在 INVARIANTS §2.1 那张表里登它是真相还是缓存。后端自己的那份先在 `relay_route_core` 加一个 `Decl` 常量、排进 `OWN_FILES`，读写经 `Doc::of` 取的那一个（INVARIANTS §2.2）；几个进程之间交接的，再在 §9 那张表里写它的格式。
