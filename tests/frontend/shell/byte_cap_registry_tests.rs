use std::path::{Path, PathBuf};

/// 超限之后怎么办。**刻意是个封闭集合** —— 多出第四种就得回来论证。
///
/// ⚠ 里面**没有「静默截断」**：报告 B-5 点名的那一处（`remote_history`）
/// 已由 F06 上半改成「截断 + 说清」。这个集合就是那次修复的护栏。
const ALLOWED_SEMANTICS: &[&str] = &[
    "硬报错",
    "截断+说清",
    "拒收+回错",
    // 🔴 `K-R59`（09-11）：这里原来还有第四种 **「分轮续读」** —— 它的**唯一**用户是
    //    远端会话流那一段里那条 8 MiB 的「单文件单轮读满即停、下轮续读」上限
    //    〔散文墓碑〕它当年叫 `BACKENDLESS_READ_CAP`，与 `BACKENDLESS_DISCOVER_CAP` 同批。
    //    那一整段随定框 `K35` 删除 ⇒ 这一项当场变成「没人用的名字」，
    //    而本文件那条判据逐字要求「登记表里每一种都得真有人用……别留一个谁都能往里塞的口子」
    //    ⇒ **一起摘掉，不是顺手，是被那条判据逼下来的**（它先红，摘了才绿）。
    //    ⚠ 真有人重新做「读一大块、分轮续」的形状，请**回来重新论证**再加，别照抄这段墓碑。
    // ⚠ 第五种是本轮**论证后**加的，不是顺手加的：搜索索引的字符封顶截掉的是
    // **可搜性**，不是数据 —— 原始 jsonl 一个字节没动，只是超过封顶的那段搜不到。
    // 它与「静默截断」的分界就在这里：**有没有丢掉用户的东西**。
    "索引截断（不丢数据）",
    // ⚠ 第六种同样是**论证后**加的〔audit-0805 §5 1x，08-06〕：
    // `accounts_query::session_accounts` 读单个会话 pidfile 超限时**跳过那一条**，
    // 既不是硬报错（不会让整份账号清单失败 —— 一个坏文件不该毁掉全部归属），
    // 也不是截断（没有把半份数据当成完整的用）。
    // ★ 它与被刻意排除的「静默截断」的分界**就在那个 `warn!`**：
    // 跳过本身不是问题，**跳过而不说是谁**才是（定框 E4）。
    // ⇒ 名字里带「说清」不是修辞：没有 warn 的跳过**不许**登记成这一项。
    "跳过+说清",
    // ⚠ 第七种，同样**论证后**加〔audit-0805 §5 1x，08-06 对拍查出〕：
    // `local_accounts::load` 读 manifest 超限时**整份结果降级返回**
    // （`available: true`、账号清单为空、错误塞进 `meta`）—— 既不是硬报错
    // （不 `?` 上去），也不是跳过一条（降的是整份结果）。
    // ★ 它与「静默」的分界同样在**错误有没有被带出去**：这里带在 `meta` 里，看得见。
    // ⚠ 顺带如实登记一处**没修**的：同一个 `Err` 臂同时处理「manifest 不存在」（正常）
    // 与「manifest 超限」（异常），两者被合并成同一种降级 —— 分开报要动账号错误面的
    // UX，超出 1x 的范围，进。
    "降级+说清",
    // ⚠ 第八种，同样**论证后**加：backend 出方向单行超限时
    // **丢掉那一行**，并往 `REMOTE_HEALTH` 发一条带 origin 的说明。
    // 今天告用户的那一层换成**原位** `Gap`（`to_seq` 缺：知道丢了、不知道丢到哪）——
    //   与行同一条流交给订阅，前端照往后补；旁路那条健康提示删了。分界照旧是「告用户 vs 告日志」。
    //
    // ★ 它与「跳过+说清」的分界是**谁被告知**：那一档告的是**日志**（`warn!`），
    // 而这一档告的是**用户**（前端能看见的类型化事件）。为什么必须多这一档：
    // 帧读跑在一个后台 task 里，**没有调用方可以回错**（回 `Err` 会被主循环当成
    // 致命错误去重连，而超长行只是这一行坏了）；而只写日志的话用户看到的是
    // 「这条会话少了一行」且无从得知为什么。⇒ 两者都不够，需要一个新名字。
    "丢弃+带身份报告",
    // ⚠ 第九种，**论证后**加：窗口存盘时一条请求行的上限（`editor::SAVE_LINE_CAP`，
    // 与后端入方向一行同值）。越过它**不拒、不截、不丢**：整份切成几块、每块一行、逐块送进暂存区，
    // 后端读回拼起来 —— 用户的字节一个不少。它与「拒收+回错」的分界就在这里：越界的那一份**照样存成了**。
    "分块（不丢数据）",
];

/// 扫到了但**不是体量上限**的，逐条写清为什么排除。
///
/// ⚠ 排除表也会腐：下面第一条会检查每条排除**真的还扫得到**，
/// 否则它就是一条永远不匹配的死规则，而死规则会在下次有人往这个名字上写真上限时悄悄放行。
const NOT_A_SIZE_CAP: &[(&str, &str)] = &[
    (
        "ERROR_CHARS",
        "`backend/observe/runs.rs`：运行表里失败收场的报错原话至多留几个**字**（字符数，不是字节；运行表一变就整份重发，留短一点）",
    ),
    (
        "TH32CS_SNAPPROCESS",
        "**标志位**不是体量：`CreateToolhelp32Snapshot` 那一问「要进程表」的那个位（`platform/win_tables.rs`）。",
    ),
    (
        "TREE_SLICE",
        "**条数**不是字节：文件窗口删一整棵树时一趟让后端至多删几条（`filewin/writeops.rs`）；删够了后端停在两条之间、回还剩几条，窗口接着发下一趟，不限任何读写的体量。",
    ),
    (
        "STALE_AFTER",
        "**秒数**不是字节：额度显示态里「最后一次看到距今超过多久算数旧」（`accounts/quota/show.rs`，30 分钟）；只决定 `stale` 那一格，不限任何读写的体量。",
    ),
    (
        "MARKED_SESSIONS_KEEP",
        "**会话个数**不是字节：中转看见的请求标记至多记多少个会话（`observe/relay_marks.rs`）；超了丢最早记下的那个，它再发请求就重新记。",
    ),
    (
        "CONTEXT_STANDARD",
        "**模型的上下文窗口**（tokens），不是字节：会话事实判上下文上限时的标准那一档（`observe/facts_query.rs::context_limit`）；不限任何读写的体量。",
    ),
    (
        "CONTEXT_EXTENDED",
        "**模型的上下文窗口**（tokens），不是字节：扩展那一档（1M），判不出时按它算；不限任何读写的体量。",
    ),
    (
        "PRIME_BLOCK",
        "**一次读多少**不是上界：冷接宣告会话时主记录按这么大一块一块读（一行比它长就接着读完那一行），只为不把整份读进内存；什么都不因它被截掉。",
    ),
    (
        "ENDED_KEEP",
        "**条数**不是字节：运行簿每个会话记住几个被挤出运行表的已收场子运行（id ＋ 终态，先进先出）；超了丢最旧的那条，它日后再被说到就按新的立起来。",
    ),
    (
        "IMAGE_MAX_SIDE",
        "**像素边长**不是字节：文件窗口图片预览解出来之后最长边超过它就先缩小再交给界面；什么都不丢，只是画小一点。",
    ),
    (
        "CHECK_EVERY",
        "**步长**不是体量：文件名搜索在索引里每扫多少条看一次「这个搜索框是不是来了更新的号」（来了 ⇒ 这一趟收手）；什么都不因它被截掉。",
    ),
    (
        "BINARY_SNIFF_LEN",
        "**看多远**不是上界：按内容搜判二进制时看每份开头多少字节（里面有 NUL ⇒ 当二进制跳过、记数进 `skipped_binary`）；什么都不因它被截掉。",
    ),
    (
        "STOP_GRACE_MS",
        "**时间**不是体量：一次性子命令 `--resident-stop` 请常驻后端收尾之后等它退的宽限期（毫秒）；比后端自己的排空上限长（`resident_tests` 钉）。",
    ),
    (
        "MARGIN_MS",
        "**时间**不是体量：发起方期限里留给回程与收尾的那一段（毫秒）；后端装总期限时从发起方给的期限里减掉它。",
    ),
    (
        "KILL_WAIT_MS",
        "**时间**不是体量：强杀之后再等它没了的上限（毫秒）；还在 ⇒ 出声说没停掉。",
    ),
    (
        "GRACE_MAX_SECS",
        "**时间**不是体量：`--grace <秒>` 收下的上界（越界 ⇒ `bad_args` 拒）。",
    ),
    (
        "WAIT_TIMEOUT",
        "Win32 **返回码**不是体量：`WaitForSingleObject` 的「期限到了、对象没被触发」（`0x102`）。",
    ),
    (
        "SIO_TCP_INFO",
        "Win32 **控制码**不是体量：`WSAIoctl` 问一条 TCP 的往返时间那一问（`_WSAIORW(IOC_VENDOR, 39)`，`platform/tcp_rtt.rs`）。",
    ),
    (
        "PROCESS_TERMINATE",
        "Win32 **访问掩码位**不是体量：开进程句柄时要「能强杀」那一位。",
    ),
    (
        "INBOX_LINES_MAX",
        "**行数**不是体量：后端 `bus-inbox` 的 `lines` 入参上界（看收件箱尾巴最多几行，越界 ⇒ `bad_args`）；\
             限字节总量的是同文件的 `INBOX_CAP`。",
    ),
    (
        "READ_FLOOR_BPS",
        "**速率**（字节 / 秒）不是体量：`frame_query·rs::read_budget` 拿它把「要读多少字节」折成分页读那一件的\
             总时限（一件事一个绝对时刻）。它不限任何字节总量、不截任何东西 —— 限总量的是各自的字节上限\
             （`SNAPSHOT_MAX_BYTES` / `MAX_SESSION_BYTES`）。",
    ),
    (
        "CHUNK_RAW",
        "**步长**不是上限：文件窗口上传块形每一块读多少原始字节（`filewin/chunk_upload.rs`，b16 翻倍后一行仍远小于后端入方向一行 \
             `MAX_LINE_BYTES`）。它不限任何字节总量（整份文件多大都一块一块送完）、不截任何东西。",
    ),
    (
        "PULL_CHUNK",
        "**步长**不是上限：非 UTF-8 名的下载每一块向那台后端要多少原始字节（`filewin/lossy_pull.rs`，== 后端 \
             `READ_CHUNK_MAX_BYTES`，读两侧源码钉相等）。它不限整份多大（一块一块读到 `eof`）、不截任何东西。",
    ),
    (
        "STAGING_MODE",
        "**权限位**不是体量：文件窗口经那台后端自建暂存区时收成的 unix 权限（0o700，与后端 `PRIVATE_DIR_MODE` 同值），\
             不限任何字节总量、不截任何东西。",
    ),
    (
        "OWN_FILE_MODE",
        "**权限位**不是体量：后端文件管理面在 `~/.cc-monitor` 里新建文件时给的 unix 权限（0o600，只给本人），\
             不限任何字节总量、不截任何东西。",
    ),
    (
        "PRIVATE_DIR_MODE",
        "**权限位**不是体量：后端 `control/files_commit·rs` 建 `~/.cc-monitor` 与暂存区那一下给的 unix 权限（0o700，只给本人），\
             不限任何字节总量、不截任何东西。",
    ),
    (
        "COMPRESS_RTT_FLOOR_US",
        "**时间门槛**（微秒）不是体量：后端 `dial/connect·rs::compression_for` 判「这一跳远不远」的往返时间门槛\
             （内核 `TCP_INFO` 量到的握手往返 ≥ 它才压）。它不限任何字节总量、不截任何东西。",
    ),
    (
        "STATUS_CONTROL_C_EXIT",
        "**退出码**不是体量：Windows 的 NTSTATUS \
             `0xC000013A`（被控制台事件打死）。`backend_policy·rs::exit_status` 拿它认出那一种死法、说人话，\
             不限任何东西的大小。",
    ),
    (
        "LINK_STEP",
        "**步长**不是体量：monitor 往链路里送上行字节时一次切多大（`link_mux·rs::LinkStream` 的 \
             `poll_write`）。多出来的留给调用方下一次写 —— **不丢、不截**，它不限任何总量。\
             与后端那一块的上限（`dial/link·rs` 的 `LINK_CHUNK_BYTES`，本表 `CAPS` 里「拒收+回错」那一行）同一个数，\
             跨 crate 对拍住 `link_mux_tests::the_chunk_cap_is_the_same_number_on_both_sides`。",
    ),
    (
        "PIPE_BUFFER",
        "**缓冲**不是体量：后端一条链路内部那两根内存管子（`tokio::io::duplex`）各自的缓冲。\
             写满了写方**等**（背压），不丢、不截 —— 它不限任何总量，只决定一次能攒多少没被读走。",
    ),
    (
        "TREE_ENTRY_CAP",
        "**条数**不是体量：一趟递归删最多动多少条（含目标自己，\
             后端 `files_write·rs::plan_tree_within` 的计划趟）。超了**整趟拒、一个字节不动**\
             （`refused`，话里带上限是多少）—— 不做半截。它管的是「一次手势删多少条」与计划表的内存，\
             不限任何字节量。",
    ),
    (
        "FIVE_HOURS",
        "**时间**不是体量：claude 那份「用满」回包说不出卡在哪个窗口时，重置时刻离此刻多少秒以内标 5 小时那个窗口\
             （后端 `agents/claudecode/quota·rs::limit_reply`；更远标 7 天那个）。不限任何字节量。",
    ),
    (
        "LEFTOVER_STALE_SECS",
        "**时间**不是体量：部署残件（`put_atomic` 的临时件 / 备份件）多久没动过才算没人要（秒）；远大于 monitor 等一次 put 的 600 秒。",
    ),
    (
        "STAGING_STALE_SECS",
        "**时间**不是体量：暂存区里一份上传件多少秒没动过才算孤儿\
             （后端 `files_commit·rs::sweep_stale`，只在一次提交成功时顺手扫，不是节拍器）。\
             它不限任何字节量 —— 暂存区能长多大今天**没有上限**，孤儿只按时间收。",
    ),
    (
        "TRIM_SLACK",
        "**条数**不是体量：重放缓冲里一个会话要**多出**这么多条才修剪一次\
             （`event_replay·rs::push_and_trim`）—— 量的是摊还节奏，不是容量。\
             容量那一格是 `REPLAY_TAIL_KEEP`（也是条数），两者之和就是单会话的上界\
             （分档取消之后对**每个**会话都成立）。",
    ),
    (
        "CHANNEL_CAPACITY",
        "**条数**不是体量（mpsc 通道能排多少帧）。它的溢出语义由 `Overflow` 帧管，见 F03。",
    ),
    (
        "LOCAL_LINES_CAPACITY",
        "**件数**不是体量：本机内容通道（`local_lines·rs`）能排多少帧。与 `CHANNEL_CAPACITY` 同族 ——\
             量的是队列深度。满了是**背压**（读循环停在送上 ⇒ 不再读 ⇒ 本机后端写阻塞），**不丢、不截**，\
             它不限任何一段用户数据的字节数（一帧的字节上限是 `BACKEND_FRAME_LINE_CAP`，本表 `CAPS` 那一行）。",
    ),
    (
        "LIMIT_MAX",
        "**条数**不是体量：`search-core` 里 `--limit` / IPC `limit`              能取的最大值 = 一次搜索最多构造多少条 **snippet**，不是任何字节量。             它「超了怎么办」有答案、但不在本表的语义里 —— `clamp_limit` 直接把它夹住，             而被夹掉的那部分由 `SnippetBudget::starved()` ⇒ `SessionHits::hits_truncated`              ⇒ `SearchResponse::truncated` 一路说出来（那正是 `KR100D3` 治的东西）。",
    ),
    (
        "BUILD_STAMP_LEN",
        "🔴 **一段定长数据的长度，不是任何东西的上限**：它逐字等于\
             `STAMP_OPEN.len() + BUILD_ID.len() + STAMP_CLOSE.len()`（界标住契约 crate），\
             是 `CC_MONITOR_BUILD_STAMP` 那个 `static [u8; N]` 的 N。\
             它**没有「超了怎么办」这一格** —— 编译期算出来多少就是多少，\
             `BUILD_ID` 长一个字符它跟着长一个字符。\
             ⚠ 它之所以必须是**定长数组**而不是 `&str`：`#[used] static [u8; N]` 有地址、\
             进 `.rodata`、字节按定义连续，编译器拆不成立即数 —— 那正是「拿到一份二进制\
             扫得出它是谁」这条性质的支点（理由全文住 backend `main.rs` 的 `build_stamp`）。",
    ),
    // ── 默认拒绝上线后，把「名字没关键词的尺寸类常量」逐个判过。
    // **七个全都不是字节上限** —— 也就是说旧的名字关键词过滤今天恰好完整；
    // 本表登记的是**这个判断本身**，好让下一个不叫 `*_CAP` 的真上限没法悄悄溜过去。
    (
        "CHUNK",
        "**传输步长**不是上限：SFTP 每次读写多少字节，读完继续读，没有「超了怎么办」。\
             ⚠ 它是这七个里最像字节量的一个 —— 正因如此才要写下来为什么不算。",
    ),
    (
        "PROGRESS_EVERY",
        "**进度上报间隔**（每传够这么多字节报一次进度），量的是节奏不是容量。",
    ),
    (
        "PORT_SPAN",
        "**端口个数**不是体量（`K-P1` 的常驻监听口从家目录 hash 出来，落在 \
             `49152..=65535` 这 16384 个动态口里）。它没有「超了怎么办」这一格 —— \
             那个 hash 按定义落在区间内。",
    ),
    // `NET_EPOCH_TO_WIN32_FILETIME_TICKS`（.NET 与 FILETIME 的纪元差）那一行摘了：它住的那个换算一个调用方都没有，连同常量删了。
    // `PROC_START_TOLERANCE_TICKS` 那一行摘了：它住 monitor 自己那份判活（`session_map.rs` 的 Windows
    //   进程身份核对），本机判活改由本机后端的帧来，那份实现连同这个常量一起删了。
    (
        "STARTTIME_IDX_AFTER_COMM",
        "**字段下标**（`/proc/<pid>/stat` 里 `starttime` 在 `comm` 之后的第几个字段）。不是量。",
    ),
    (
        "TTY_NR_IDX_AFTER_COMM",
        "**字段下标**（`/proc/<pid>/stat` 里 `tty_nr` 在 `comm` 之后的第几个字段）。不是量。",
    ),
    (
        "CREATE_NEW_CONSOLE",
        "**Win32 进程创建标志位**（`CreateProcess` 的 flag）。是位掩码不是尺寸。",
    ),
    (
        "CREATE_NO_WINDOW",
        "同上：Win32 进程创建标志位。",
    ),
    // ── HTTP 中转带进来的。**刻意不并进上面那组「七个」**：
    // 那一组是 08-06 那次普查的读数，往里塞新的会让「七个全都不是字节上限」变成假话。
    // ⚠ 上面的 `CHUNK` 一条**盖不住它** —— `every_size_typed_constant_…` 里的
    // `excused` 是**名字全等**（`n == name`），不是子串。
    (
        "READ_CHUNK",
        "它是**一次 read 的缓冲区长度**（读多少给多少的上限，**不是任何东西的体量上限**）；\
             它不截断、不拒收。与上面的 `CHUNK` 同族：`relay/server.rs` 的 `pump` 拿它开一个 \
             `vec![0u8; READ_CHUNK]` 反复读到 EOF，**没有「超了怎么办」这一支**；\
             减小它只是把同一批字节分更多次搬。",
    ),
    // `TEE_QUEUE_LINES`（NDJSON 行落点的队列深度）随那个落点删了。
    (
        "ID_SNIFF_BYTES",
        "**一段嗅探窗口**，不是上限：后端读循环遇到超长行（整行丢弃）时，只看行首这么多字节\
             去抠信封的 `id`、好让 `line_too_long` 带回请求方的 id（`inbound·rs::sniff_id`）。\
             越过它什么都不丢、不截 —— `id` 不在这一段里就回空串，即改动之前的行为。",
    ),
    (
        "REQUEST_ID_ROOM",
        "**一个字段最长多少字节**（请求行里 `id` 那一格的位子），不是上限：\
             窗口量存盘那一行时按最长的 id 算，只会比真发的那一行长、不会短。\
             它没有「超了怎么办」—— id 是 monitor 按固定形状造的，判据钉着那个最长形状恰好等于它。",
    ),
    // ── 大文件模式（`filewin/bigfile.rs`）带进来的三个 ──
    (
        "BIG_LINE_BYTES",
        "**进大文件模式的门槛**，不是上限：最长一行超过它，编辑面就换成「只排视口内的行、\
             长行只排可见段」那一面。**没有任何东西被拒、被截** —— 越过它的文件照样整份打开、\
             照样能改能存；编辑的真上限是 `filewin/editor.rs::MAX_EDIT_BYTES`（下面 `CAPS` 那一行）。\
             数由 `bigfile::derive_threshold(LINE_READING)` 推出，判据钉「推算式 == 常量」。",
    ),
    (
        "BIG_TOTAL_BYTES",
        "同上，全文那一维的门槛（全文超过它进大文件模式）。按真敲键重打读数后 1 MiB → 256 KiB；\
             编辑上限同拍抬到 1 MiB（`editor::MAX_EDIT_BYTES`；再抬到 8 MiB）⇒ 这一条从此**在打开时就会开火**。",
    ),
    (
        "FRAME_BUDGET_US",
        "**时间**（一帧的预算，微秒），不是字节。两个门槛从它推出来。",
    ),
    // ── 后端 Windows 判活（`platform/win_proc.rs` · `platform/proc.rs`）带进来的五个 ──
    //    全是 Win32 常量与时间换算，没有一个量字节。
    (
        "PROCESS_QUERY_LIMITED_INFORMATION",
        "**Win32 访问掩码**（`OpenProcess` 的「只读查询」权限位，`0x1000`）。名字里的 LIMITED 是\
             「受限的查询权限」，不是任何东西的上限 —— 是位掩码不是尺寸（同 `CREATE_NO_WINDOW` 那一族）。",
    ),
    (
        "SYNCHRONIZE",
        "**Win32 访问掩码**（允许等一个进程句柄的权限位）。是位掩码不是尺寸。",
    ),
    (
        "WAIT_FOREVER",
        "**「一直等」的哨兵值**（`WaitForSingleObject` 的 `INFINITE`，全 1）。不是时长上限、更不是字节量 —— \
             它的意思恰恰是「没有上限」；`pidwatch_windows_shape_tests` 钉着它必须是全 1（零定时器）。",
    ),
    (
        "WAIT_ABANDONED",
        "**Win32 等待结果码**（`WaitForSingleObject` 回「上一个持有者没放就没了」—— `platform/lock.rs` 那把命名互斥量）。\
             是一个返回码，不是尺寸。",
    ),
    (
        "FILETIME_TICKS_BEFORE_UNIX_EPOCH",
        "**时间纪元差**（Win32 FILETIME 的 1601 起点与 Unix 1970 起点相差多少个 100ns tick）。\
             单位是时间不是字节。",
    ),
    (
        "FILETIME_TICKS_PER_SEC",
        "**时间刻度**（一秒里有多少个 100ns 的 FILETIME tick）。单位换算，不是任何东西的上限。",
    ),
    // ⚠ `MIN_SCANNED_CODE_BYTES` 那条已删（08-06）：它是**测试段里的地板**，
    //    本表原来扫整份文件才需要排它；扫描面收窄到生产段之后它成了死规则，
    //    而本表自己的 `the_exclusion_list_is_not_dead_wood` 当场要求删。
];

/// `(相对仓根的路径, 常量名, 它管的是什么量, 超限怎么办)`。值只在源码里写一份。
///
/// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面第一条红。
const CAPS: &[(&str, &str, &str, &str)] = &[
    // ---- monitor 侧 ----
    // ---- P8a：插件面只读枚举 ----
    // 这两条随读实现搬去了后端，后来随插件只读列表一起删了。
    // `local_accounts.rs` 的 `MANIFEST_CAP`（本机读账号 manifest，「降级+说清」）那一行出列：
    //   那份零生产调用方的本机参照实现删了，读 manifest 只剩后端一处（`MAX_MANIFEST_BYTES`，登记在后端那一段）。
    // 住址 `remote_history.rs` → `history.rs`；→ 后端 `read_face.rs::WHOLE_SESSION_MAX_BYTES`
    //   （「整份读进查看器」那一件的上限判定随记录解释进了后端，登记在后端那一段）。
    // 🔴**它管的不是字节，是条目数** —— 如实登记这一点。
    //    本表的抽取器按「具名常量 ＋ 一个够大的数」取人群，不看单位；
    //    而「一屏最多多少行」与本表别的那几项**不是同一种量**，
    //    所以这一栏写清楚，别让下一个人按字节去读它。
    //    ⚠ 超限那一档走的是**后端回 `truncated: true` ＋ 窗口画一句话**
    //    （`shell.rs` 那一行橙字逐字「没看见的文件不代表它不在」）
    //    ⇒ 正是本表允许的「截断+说清」，而**不是**它明令排除的「静默截断」。
    // 重放留存的总量（全部会话合起来的正文字节）。超了整条丢：先丢已经不活的会话、再丢最久没进过行的；
    //   F5 之后界面按骨架 / 行号把那一段取回来（`event_replay·rs` 头注那张表）⇒ 不是静默丢，是「截断+说清」。
    (
        "src/frontend/shell/src/event_replay.rs",
        "HELD_BYTES_CAP",
        "monitor 重放留存里全部会话合起来的正文字节（单会话另有条数上界 `REPLAY_TAIL_KEEP` ＋ `TRIM_SLACK`）",
        "截断+说清",
    ),
    (
        "src/frontend/filewin/src/source.rs",
        "LS_LIMIT",
        "一趟 `files-ls` 一屏最多几条目录项（**条目数，不是字节**）",
        "截断+说清",
    ),
    // 池子那一份同值的 `MAX_EDIT_BYTES`（「SFTP 在线编辑的文件体量」）随那条读文本命令一起走了。
    // 文件窗口编辑器的上限**搬回窗口**（它答「文本控件打字卡不卡」，
    //   是窗口的偏好）：每趟经 `max_bytes` 送给后端 `files-read-text`，后端按它整趟拒、不截断。
    // 🔴256 KiB → 1 MiB（那时存盘整份装一行，上限钉成后端入方向一行）。
    // 🔴1 MiB → **8 MiB**：装不进一行的分块走暂存区（下面 `SAVE_LINE_CAP` 那一行），
    //   能存多大改由后端 `files-commit-text` 的天花板定，而它就是 `files/mod.rs::READ_TEXT_MAX_BYTES`
    //   （存得回的要读得回来）⇒ 对 E 改钉这一对。
    (
        "src/frontend/filewin/src/editor.rs",
        "MAX_EDIT_BYTES",
        "文件窗口编辑器能打开、能存回的文本体量 ＝ 后端 `files-read-text` / `files-commit-text` 的天花板",
        "拒收+回错",
    ),
    // 预览自己的上限（**刻意不借编辑上限**：预览跟着光标走，↑↓ 一路按下去
    //   每一步都是一趟 `files-read-text`；编辑上限是「点了编辑」那一下的量）。
    //   超了：**不读**，面板上说「有 N，预览只看 M 以内的文件」；不截一半来预览（截断的文本会被当成全文）。
    (
        "src/frontend/filewin/src/preview.rs",
        "PREVIEW_MAX_BYTES",
        "文件窗口预览一份文本的体量（每挪一次光标一趟）",
        "拒收+回错",
    ),
    // 图片预览一张图读多少（逐块经 `files-read-chunk` 读回）。超了：**不读**，面板上说「图片只预览 N 以内的」；不截一半来解码。
    (
        "src/frontend/filewin/src/preview.rs",
        "IMAGE_MAX_BYTES",
        "文件窗口预览一张图片的字节数（窗口自己解码）",
        "拒收+回错",
    ),
    // 存盘时一条请求行最多多长 ＝ 后端入方向一行（对 F 读两侧源码钉相等）。
    (
        "src/frontend/filewin/src/editor.rs",
        "SAVE_LINE_CAP",
        "窗口存盘时一条请求行（`files-write-text` 整份 / `files-stage-chunk` 一块）序列化后的长度",
        "分块（不丢数据）",
    ),
    (
        "src/frontend/shell/src/stream_source/snapshot.rs",
        "SNAPSHOT_MAX_BYTES",
        "首连快照单会话体量",
        "截断+说清",
    ),
    // ── `K-P1`：常驻监听口的握手（**两侧各一条，方向不同**）───────────
    (
        "src/frontend/shell/src/local_backend_host.rs",
        "LISTEN_HANDSHAKE_LINE_CAP",
        "宿主读常驻口那一行（hello / attach 应答）—— hello 帧本机实测 ~1.1 KB",
        "拒收+回错",
    ),
    // 历史页的平铺清单（`history_list.rs`）：一台一次最多回多少行；多出的不回、`truncated` ⇒ 列表底「更早的用搜索找」。
    (
        "src/backend/history/history_list.rs",
        "DEFAULT_LIMIT",
        "一台的平铺会话清单一次回的行数（调用方给 `limit` 可改，1–20000）",
        "截断+说清",
    ),
    // 常驻进程里那张「一份记录扫出的那一行」的记账表（`history_query.rs`）：超了整张清掉重来，下一问照常整份扫 —— 只丢记账，不丢数据。
    (
        "src/backend/observe/history_query.rs",
        "SESSION_META_CAP",
        "常驻进程记着的会话记录摘要条数（按长度 · 修改时刻认没变，没变不再整份扫）",
        "索引截断（不丢数据）",
    ),
    (
        "src/backend/stream/listen.rs",
        "ATTACH_LINE_CAP",
        "backend 读一行 attach 请求 —— `{\"attach\":\"<32 位十六进制>\"}` 本机实测 51 字节。\
             ⚠ 对端是**同机任何进程**，不是我们自己的子进程",
        "拒收+回错",
    ),
    (
        "src/frontend/shell/src/stream_source/exec.rs",
        "EXEC_CAPTURE_MAX_BYTES",
        "一次 exec 的 stdout/stderr 各自收集量",
        "截断+说清",
    ),
    // ──：**本机后端的 stderr 接进滚动日志**那一格 ───────
    //
    // 那一行先前是 `Stdio::null()`（后端整层 91 处诊断的唯一出口被无条件丢弃）。
    // 接出来就多了一个量：**一条子进程一生往日志里搬多少字节**。
    //
    // 🔴 这两条的超限语义都**只砍「记」，不砍「读」** —— 读的一侧停下来，
    //    管道写满之后子进程下一次 `write(2)` 就阻塞，那会把「诊断没人看」
    //    升级成「**打印诊断会把后端挂住**」。判据看不见这个区别，写在这里。
    (
        "src/frontend/shell/src/local_backend.rs",
        "STDERR_LOG_BUDGET_BYTES",
        "一条被监护子进程**一生**往 monitor 滚动日志里搬的 stderr 字节数。\
             ⚠ 它**不是**「读多少」：超了照旧读到 EOF（见上），只是不再逐行记；\
             收尾按 pid 报一句「另有 N 行没记进来」。\
             为什么要有它：后端崩溃循环时 stderr 可以很吵，而日志按天滚 —— \
             没有上界的话，一次崩溃循环能把当天那份撑爆，\
             于是**下一次故障的线索反而被这一次的噪音淹掉**",
        "丢弃+带身份报告",
    ),
    (
        "src/frontend/shell/src/local_backend.rs",
        "STDERR_MAX_LINE_BYTES",
        "子进程 stderr **一条**的上界（对端一个 `\\n` 都不发时，`read_until` 会一直吃内存）。\
             ⚠ **不丢字节**：超了这一段照记，断口贴一句 `STDERR_CUT_MARK` 说「在此切开，\
             下一条接着它」，余下的字节成为下一条 ⇒ 「说清」那一半是承重的 —— \
             没有那句标记，日志里会出现一条**看起来完整、其实是半句**的诊断",
        "截断+说清",
    ),
    // 🔴 `K-R59`（09-11，定框 `K35`）：这里原来有**两条** ——
    //    `BACKENDLESS_READ_CAP`（8 MiB，单文件单轮读，超了分轮续读）与
    //    `BACKENDLESS_DISCOVER_CAP`（4 MiB，`find` 发现命令的 stdout，超了截断+说清）。
    //    它们随 `stream_source/` 那一整段 `daemonless` 轮询读一起退役 ——
    //    **不是「上限放宽了」，是被它们限住的那条读根本不存在了。**
    (
        "src/frontend/shell/src/launch.rs",
        "MAX_REMOTE_CMD",
        "本机那条送法的命令串长度（字节）",
        "拒收+回错",
    ),
    // 〔「有字节与条数上界」〕按内容搜那三道。
    (
        "src/backend/files/grep.rs",
        "FILE_MAX_BYTES",
        "按内容搜时单份文件的表观大小（字节）",
        "跳过+说清",
    ),
    (
        "src/backend/files/grep.rs",
        "TOTAL_MAX_BYTES",
        "按内容搜一趟累计读进来的字节",
        "截断+说清",
    ),
    // 会话的项目目录只读记录开头：读满了还没找到 ⇒ 没有这一格（标题退到 aiTitle / sid），日志里说是哪份文件。
    (
        "src/backend/agents/mod.rs",
        "HEAD_CAP",
        "找会话的项目目录时，会话记录开头至多读多少字节",
        "跳过+说清",
    ),
    // ⚠ 判二进制看开头多远那个数（`BINARY_SNIFF_LEN`）不是上界 —— 什么都不因它被截掉，不登记。
    // 远端那条送法（ssh 外壳）随渲染进了本机后端，上限同值搬过去。
    (
        "src/backend/dial/terminal.rs",
        "MAX_COMMAND",
        "开终端那一行里要在远端跑的命令串长度（字节）",
        "拒收+回错",
    ),
    // 🔴 `K-R100`：这两条**原本两侧各登记一份**（`src/frontend/shell/src/search.rs` 与
    // `src/backend/observe/search_query.rs`），靠下面「对 D」那条判据
    // 钉住它们相等。收口之后它们只有一个家 ⇒ **「两侧漂开」这件事在结构上没了**，
    // 那条对拍随之删掉（见 `the_cross_crate_twins_are_machine_checked_not_hand_copied`）。
    // `search-core` 拆进后端：两条封顶随通用口径住 `observe/search_rules.rs`（仍只此一份）。
    (
        "src/backend/observe/search_rules.rs",
        "MAIN_CAP",
        "单条 main 文本进索引的**字符**数（只此一份）",
        "索引截断（不丢数据）",
    ),
    (
        "src/backend/observe/search_rules.rs",
        "TOOL_CAP",
        "单条 tool 文本进索引的**字符**数（只此一份）",
        "索引截断（不丢数据）",
    ),
    (
        "src/backend/observe/search_query.rs",
        "FIND_MAX_LIMIT",
        "会话内查找（`--find-in-session`）一次最多**列**多少条命中（条数，不是字节）；\
             `--limit` 要得再多也按它算。⚠ 只砍「列」不砍「数」：尾行的 `total` 恒为全量，\
             面板据 `total > 条数` 说「只列了前 N 条」",
        "截断+说清",
    ),
    // ── 以下七条此前**全都不在本表的扫描面里**。
    // 前六条是**内联字面量**（`.take(32 * 1024 * 1024)` 这种），本表头注把那一族
    // 划在范围外、交给一条「前提触发器」盯着别长大 —— 而那条触发器**一直在假绿**
    // （它抠 `.take(` 之后的连续数字再要求紧跟 `.read_to_end`，
    // `32 * 1024 * 1024` 抠出 `"32"`、后面是 `" * 1024"` ⇒ 形态不匹配 ⇒ 不计数）。
    // ⇒ F10b 把六处提成具名常量（于是自然进主扫描面），并把触发器改成按事实取样。
    //
    // ★ 更要紧的是：它们的超限语义此前**全是「静默截断」** —— `.take(N).read_to_end()`
    // 读满就停、缓冲区里是半份数据，而调用方拿它当完整的用。那正是本表这个封闭集合
    // **刻意排除**的那一种。⇒ 六处一律改成「多读一个字节 + 超了就回错」
    // （形态照抄 `src/backend/common/fs.rs` 那条既有注释）。
    // 这里原先登记着 `cc_bus.rs` 读两份登记表的 `CC_BUS_TSV_CAP`〔散文墓碑〕（32 MiB，拒收+回错）：
    //   驾驶舱读名册改由界面经通道问后端 `bus-state`（转调 `cc-list --tsv`），monitor 这边不再读流 ⇒ 常量随那条读删了。
    // 🔴 `K-R112`（09-13）：**cc-bus 查在线那条读上限删了，不是「忘了」。**
    //    它是老那条按名字探在线的 shell 串（`tmux has-session`）的读上限，
    //    而查在线整条改走后端的 `bus-list` 帧之后**没有一条流要读** ——
    //    帧应答是结构化的，上限由入方向通道自己那一层管。
    //    ⇒ 常量不存在了，留着这一行就是**僵尸账**（本表自己那条反向锚点会当场逮住）。
    // monitor 读收件箱那条 `INBOX_READ_CAP`〔散文墓碑〕（4 MiB，截断+说清）搬进后端 `bus-inbox`（下一行）。
    (
        "src/backend/control/cc_bus.rs",
        "INBOX_CAP",
        "`bus-inbox` 交回的收件箱尾巴（`cc-log` 的回显）",
        // 这是回显不是清单：超了保尾、成品里 `truncated: true` 说清（与 monitor 旧那条同档）。
        "截断+说清",
    ),
    // 这里原先登记着 `cc_bus.rs` 的控制类回显上限（发消息 / spawn 的回显，64 KiB，截断+说清）。
    //   发消息 `K-R98`、派生 BS1b 先后改走后端原语，那两条回显不再经 SSH 读 ⇒ 常量随最后一个用户删了。
    //   「截断没毒、回 Err 有毒（用户会重试、再起一个 agent）」那条理由今天住在 `OnOverflow::Truncate` 的头注里。
    // `mcp.rs` 读远端 `.claude.json` 那条 `REMOTE_CLAUDE_JSON_CAP`〔散文墓碑〕（32 MiB）删了：改问那台后端 `mcp-read`（后端那份读上限是 `MAX_CONFIG_BYTES`）。
    // `hooks_diag.rs` 读远端 `settings.json` 那条 `REMOTE_SETTINGS_CAP`〔散文墓碑〕（4 MiB）删了：改经那台后端 `files-peek`（上限归后端 `PEEK_MAX_BYTES`，已在表里）。
    // 第七条不是内联字面量，是**压根没有上限**：backend 出方向单行此前走无界 `read_line`。
    // ⚠ 它的数**刻意不等于** backend 侧的 `MAX_LINE_BYTES`（1 MiB，入方向命令信封）——
    // 实测本机 525,132 行 jsonl 里有 78 行超过 1 MiB、最长 2.97 MiB，
    // 抄过去就是丢真实数据。理由全文在 `stream_source/exec.rs::BACKEND_FRAME_LINE_CAP` 头注。
    (
        "src/frontend/shell/src/stream_source/exec.rs",
        "BACKEND_FRAME_LINE_CAP",
        "backend **出方向单行**（一帧 = 一条 Claude jsonl 行）",
        "丢弃+带身份报告",
    ),
    // ---- backend 侧 ----
    // 从 monitor `history·rs` 那个 `MAX_SESSION_BYTES`〔散文墓碑〕搬来：「整份读进查看器」那一件读过它就明拒、那句话说读到了哪（F06）。
    (
        "src/backend/faces/read_face.rs",
        "WHOLE_SESSION_MAX_BYTES",
        "查看器读一整份会话 jsonl（`history-page` 带 `whole`；本机远端同一条）",
        "拒收+回错",
    ),
    (
        "src/backend/control/fork_write.rs",
        "MAX_SESSION_JSONL_BYTES",
        "分叉时读源会话 jsonl",
        "硬报错",
    ),
    (
        "src/backend/control/launch.rs",
        "MAX_FIELD_BYTES",
        "启动请求单字段（载荷/名字/cwd）",
        "拒收+回错",
    ),
    (
        "src/backend/control/session_restart.rs",
        "MAX_WAIT_MS",
        "换号重启发起方给的两个期限（毫秒，不是字节）",
        "拒收+回错",
    ),
    (
        "src/backend/control/terminals.rs",
        "MAX_SCROLLBACK",
        "终端预览往回多要的行数（行，不是字节）",
        "截断+说清",
    ),
    (
        "src/backend/control/resolve_query.rs",
        "MAX_RESOLVE_STDIN",
        "`--resolve` 的 stdin",
        "截断+说清",
    ),
    (
        "src/backend/control/cli_control.rs",
        "MAX_CLI_STDIN",
        "控制面 CLI 子命令（`--launch`/`--kill`/…）的 stdin args JSON",
        "拒收+回错",
    ),
    (
        "src/backend/stream/inbound/mod.rs",
        "MAX_LINE_BYTES",
        "入方向单行",
        "拒收+回错",
    ),
    // 链路（本机常驻后端替界面持有并复用 SSH 连接，`dial/link.rs` · `link_mux.rs`）。
    (
        "src/backend/dial/link.rs",
        "LINK_CHUNK_BYTES",
        "一块链路字节（`link-data` 上行一块 / `link_data` 下行一块，解码后）",
        "拒收+回错",
    ),
    (
        "src/backend/dial/link.rs",
        "MAX_WINDOW",
        "一条链路手里的信用（= 在途下行字节）：`link-open` 的初始窗口与累计 `link-credit`",
        "拒收+回错",
    ),
    // 部署读版本标记 / 入口 shim 那一问的上限（经本机后端 `files` 链路的 `read`，`max` 就是它）。
    // `sftp.rs` 读版本标记那一读的上限那一行删了：那一读随 cc-acct-iso 的部署命令退役。
    // 公钥推送（本机常驻后端）读本机那份 `.pub` 的上限：先问大小，超了就不读、原话拒。
    (
        "src/backend/assets/pubkey.rs",
        "PUB_READ_MAX",
        "本机那份 `.pub`（公钥一行几百字节；超了就不是公钥）",
        "拒收+回错",
    ),
    // 部署计划（本机常驻后端）认从前那份三行入口时读落点那一份的上限：先问大小、大了不读。
    (
        "src/backend/control/deploy_plan.rs",
        "ENTRY_READ_MAX",
        "部署计划读回落点那一份（只在它不说自己是谁时，认从前那份几十字节的三行入口）",
        "跳过+说清",
    ),
    // SFTP 住本机常驻后端：部署链路（`use:"files"`）一问一答的两个界。
    (
        "src/backend/dial/sftp.rs",
        "REQUEST_LINE_CAP",
        "files 链路上一行请求（短 JSON；`put` 的字节不走行）",
        "拒收+回错",
    ),
    (
        "src/backend/dial/sftp.rs",
        "MAX_PUT_BYTES",
        "一次 `put` 收进内存的字节（后端二进制今天 MB 级）。超了 ⇒ 那几个字节照收照丢（别让下一行请求读到半截二进制）、回 `too_big`",
        "拒收+回错",
    ),
    (
        "src/frontend/shell/src/link_mux.rs",
        "LINK_WINDOW_BYTES",
        "一条链路「后端发过来、还没还信用」的下行字节（对端守约时它就是界上的在途量）。\
         超了 = 对端不守约 ⇒ **那条链路整条丢掉**，读端拿到一句带链路 id 的错（调用方据它按断线处置、\
         用户看得见那条远端连接断了），同时 `warn!` 一行 —— 不涨内存，而且出声",
        "丢弃+带身份报告",
    ),
    // `files-read-text` 一趟最多肯交多少 —— **后端的天花板，不是编辑上限**
    //   （编辑上限是调用方的，每趟经 `max_bytes` 送过来）。推算：JSON 转义最坏 ×6 ⇒ 48 MiB，
    //   仍在 monitor 读后端一行（`BACKEND_FRAME_LINE_CAP` 64 MiB）与通道一帧（64 MiB）之内。
    (
        "src/backend/files/mod.rs",
        "READ_TEXT_MAX_BYTES",
        "`files-read-text` 调用方给的 `max_bytes` 最大能多大（一帧应答整份进内存、整份过线）",
        "拒收+回错",
    ),
    // 用户文件读改写的**读那一半**一趟肯交多少。写那一半要把新内容与
    //   读到的那一份装进同一行请求（后端一行 `MAX_LINE_BYTES` = 1 MiB）⇒ 两份各 256 KiB、给转义留余量。
    // 非 UTF-8 名的下载逐块读回：一块多少原始字节（调用方的 `len` 越界 ⇒ `bad_args`、不夹小）。
    (
        "src/backend/files/mod.rs",
        "READ_CHUNK_MAX_BYTES",
        "`files-read-chunk` 一块读回的原始字节（b16 翻倍后一帧应答）",
        "拒收+回错",
    ),
    // 解压：zip 里一条链接的目标文本（那一条的正文）最多读多少；超了整趟拒、不截一半当目标。
    (
        "src/backend/control/files_extract.rs",
        "LINK_TARGET_MAX_BYTES",
        "`files-extract` 读 zip 里一条符号链接的目标文本（那一条的正文）",
        "拒收+回错",
    ),
    (
        "src/backend/control/files_write.rs",
        "PEEK_MAX_BYTES",
        "`files-peek` 一趟读回的文本（读改写的读那一半；写回时与新内容同装一行请求）",
        "拒收+回错",
    ),
    // 只读查询的帧面宿主那三个数（一帧应答要整个进内存、整个过线）。
    (
        "src/backend/faces/read_face.rs",
        "READ_PAGE_BYTES",
        "`history-read` 一页（一帧应答）的正文字节数",
        "索引截断（不丢数据）",
    ),
    // `backend-log` 一帧回多少：只回诊断文件的尾部，`truncated: true` 说清前面还有。
    (
        "src/backend/faces/read_face.rs",
        "LOG_TAIL_BYTES",
        "`backend-log` 一帧回的后端诊断文件尾部字节数",
        "截断+说清",
    ),
    (
        "src/backend/faces/read_face.rs",
        "LINE_CAP_BYTES",
        "`history-read` 里单独一行比一页还长时最多续读多长",
        "拒收+回错",
    ),
    (
        "src/backend/faces/read_face.rs",
        "LINES_CAP_BYTES",
        "按行那几条帧查询（`history-search` 等）整份输出",
        "拒收+回错",
    ),
    // 插件市场枚举那两个上限随插件只读列表整块删了（界面上没有可做的事）。
    // 任务列表搬进后端（`tasks-list`）：单个任务文件的读上限。
    (
        "src/backend/observe/tasks_query.rs",
        "TASK_FILE_CAP_BYTES",
        "`tasks-list` 读单个任务文件（本机实测几百字节量级）",
        "跳过+说清",
    ),
    // 别名预览（`ccm-print`）入参里一个参数最长多少。
    (
        "src/backend/control/ccm/mod.rs",
        "PRINT_MAX_WORD_BYTES",
        "`ccm-print` 交来的一条别名里一个参数的字节数（别名表单产出的远小于它）",
        "拒收+回错",
    ),
    // 账号库管理读盘那一步：清单与撤销清单各一个上限（读的是这台自己的账号库，超了就读不动、如实报）。
    (
        "src/backend/accounts/manage/scan.rs",
        "MAX_MANIFEST_BYTES",
        "改账号库之前读账号清单（与只读那一侧 `accounts_query.rs` 同一个量、同一个数）",
        "硬报错",
    ),
    (
        "src/backend/accounts/manage/scan.rs",
        "MAX_UNDO_BYTES",
        "回滚时读一份备份里的撤销清单（每一行一条路径）",
        "跳过+说清",
    ),
    // 账号之间同步用户级 MCP 读盘那一步：清单 · 共享集合一个上限，各号配置文件一个上限（同适配层读那份文件的量级）。
    (
        "src/backend/accounts/manage/mcp_share_exec.rs",
        "MAX_SMALL_BYTES",
        "同步之前读账号清单与各号共用的 MCP 那份文件",
        "硬报错",
    ),
    (
        "src/backend/accounts/manage/mcp_share_exec.rs",
        "MAX_CONFIG_BYTES",
        "同步之前读一个号的配置文件（会被项目历史与 MCP 配置撑大）",
        "跳过+说清",
    ),
    // 〔RM1a → MIG-3b 续〕「足迹」由那台后端出成品（`footprint-report`，只读）那两个数（两拍之后 monitor 不再 stat，它那两个数与收 `client.stat` 那一个随之删）。
    (
        "src/backend/footprint/mod.rs",
        "MAX_ENTRIES",
        "`footprint-report` 这台自己列一个目录最多几个名字（数 glob 那一族；超了 ⇒ 列不动，不截断）",
        "跳过+说清",
    ),
    (
        "src/backend/footprint/mod.rs",
        "MAX_HOOK_FILE_BYTES",
        "`footprint-report` 查钩子字样时读的那份 settings 文件多大",
        "跳过+说清",
    ),
    // cc-bus 钩子诊断（`hooks-diag`，只读）读那台自己的 settings 文件多大。
    (
        "src/backend/observe/cc_bus_hooks.rs",
        "SETTINGS_CAP_BYTES",
        "`hooks-diag` 读那台 agent 配置根下的 `settings.json` 多大",
        "降级+说清",
    ),
    // 「直接敲的也走中转」（`relay-optin`，只读）读那台用户级设置文件多大；超了按「读不了」说，不当没装。
    (
        "src/backend/agents/claudecode/paths.rs",
        "SETTINGS_CAP_BYTES",
        "`relay-optin` 读那台 `~/.claude/settings.json` 多大",
        "降级+说清",
    ),
    // 资产目录那六个数（`agents/claudecode/assets.rs` · `asset_catalog.rs` · `asset_sync.rs`）。
    (
        "src/backend/agents/claudecode/assets.rs",
        "MAX_PROJECT_MCP_BYTES",
        "资产目录扫描时读一份项目 `.mcp.json`",
        "跳过+说清",
    ),
    (
        "src/backend/agents/claudecode/assets.rs",
        "SKILL_DOC_MAX_BYTES",
        "资产目录扫描时读一个 skill 的 `SKILL.md`（只为取 `description:`）—— 读它的函数把错交给调用方，调用方记进 `problems`",
        "硬报错",
    ),
    (
        "src/backend/assets/asset_catalog.rs",
        "CATALOG_MAX_BYTES",
        "后端自有的资产目录文件 `~/.cc-monitor/assets-catalog.json`（读不出来就不覆盖）",
        "拒收+回错",
    ),
    // 后端自有状态文件的读上限（`common/own_state::read_bytes` 收它）：超了当读不出来 ⇒ 不覆盖、照实回错。
    (
        "src/backend/accounts/oauth/store.rs",
        "MAX_BYTES",
        "订阅号的凭据文件 `.credentials.json`（续令牌前读；读不出来就不续、不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/accounts/quota/ledger.rs",
        "MAX_BYTES",
        "后端自有的额度账 `~/.cc-monitor/quota.json`（读不出来就不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/accounts/quota/rotation.rs",
        "MAX_BYTES",
        "后端自有的账号轮换 `~/.cc-monitor/rotation.json`（读不出来就不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/accounts/upstream_select/file_face.rs",
        "KEY_FILE_READ_CAP",
        "上游选择的凭据文件 `apikey-credentials.json`（写 key 前读；读不出来就 `io_failed`、不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/control/exit_policy.rs",
        "MAX_BYTES",
        "后端自有的退出行为 `~/.cc-monitor/backend.json`（一格布尔；读不出来 ⇒ `unreadable` 带原因回出去，按缺省办）",
        "拒收+回错",
    ),
    (
        "src/backend/control/launch_account.rs",
        "MAX_BYTES",
        "后端自有的「会话用的号」记录 `~/.cc-monitor/launch-accounts.json`（读不出来就不覆盖、`unreadable`）",
        "拒收+回错",
    ),
    (
        "src/backend/dial/known_hosts.rs",
        "MAX_BYTES",
        "后端自有的主机钥匙 `~/.cc-monitor/known_hosts`（读不出来 ⇒ 不覆盖、回错，记钥匙那一处留一行日志）",
        "拒收+回错",
    ),
    // skill 装记录那份文件：超了当读不懂 ⇒ 不覆盖、`ledger_unreadable`（读的人也不许把它说成「什么都没装过」）。
    // 「要你动手」收事实时读用户的启动文件 / 设置文件：超了跳过那一份（`warn!` 带路径），那一件不出改法。
    (
        "src/backend/footprint/chores/gather.rs",
        "READ_CAP",
        "「要你动手」读一份用户启动文件 / 设置文件（`~/.bashrc` · `~/.claude/settings.json`）",
        "跳过+说清",
    ),
    // 「要你动手」记下的选择：超了当读不懂 ⇒ 不覆盖、`marks_unreadable`；判的时候照没记算。
    (
        "src/backend/footprint/chores/marks.rs",
        "MAX_BYTES",
        "后端自有的「要你动手」选择 `~/.cc-monitor/chores.json`（读不出来就不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/assets/skill_ledger.rs",
        "MAX_BYTES",
        "后端自有的 skill 装记录 `~/.cc-monitor/skill-installs.json`（读不出来就不覆盖）",
        "拒收+回错",
    ),
    (
        "src/backend/assets/asset_catalog.rs",
        "SKILL_MAX_FILE_BYTES",
        "算一个 skill 的摘要时读其中一个文件（超了只记长度，`summary.truncated` 说出来）",
        "降级+说清",
    ),
    // 历史注解那一份文件（读写者换成本机常驻后端）。超了当读不懂：读回「不知道」、拒写、一个字节不动。
    (
        "src/backend/history/history_annotations.rs",
        "MAX_BYTES",
        "读一份历史注解文件（星标 / 改名 / 隐藏 / 上次账号；monitor 从前读写的那一份，路径由它交）",
        "拒收+回错",
    ),
    // 〔第二问〕全文搜索常驻索引的上界：按最近优先留，留不下的那几份照搜、只是不留（下一问再读）。
    (
        "src/backend/observe/record_scan.rs",
        "MAX_BYTES",
        "冷开会话那几问共用的扫描图留在内存里的估算字节（超了先淘汰最久没用的；单张比它还大就不留、每次现扫）",
        "索引截断（不丢数据）",
    ),
    (
        "src/backend/observe/search_query.rs",
        "RESIDENT_MAX_BYTES",
        "全文搜索常驻索引留在内存里的可搜文本（估算字节）；本机远端同一条",
        "索引截断（不丢数据）",
    ),
    // 住址随「问远端那一跳」搬家（`asset_sync.rs` → `remote_ask.rs`，逻辑一字不改）；
    //   量从「拉回来的那一份目录」放宽成「经那一跳问回来的任何一份 stdout」（资产目录 · 历史项目 / 会话清单）。
    (
        "src/backend/stream/remote_ask.rs",
        "PULL_MAX_BYTES",
        "本机后端经池里那条 SSH 在远端跑一条一次性子命令、拿回来的 stdout（资产目录 · 历史清单；capture）",
        "拒收+回错",
    ),
    // 测试连接进本机后端：探活链路上的每一行（阶段行 · ack · 那台后端的 hello · 应答）。
    (
        "src/backend/dial/probe.rs",
        "LINE_CAP",
        "测试连接那条探活链路上的一行（阶段行 · ack · 那台后端的首行 hello · ping 应答；hello 是后端出方向单行，同量级）",
        "拒收+回错",
    ),
    // 端口转发账进本机常驻后端：读链路那一侧的 ack 与计数行。
    (
        "src/backend/dial/forwards.rs",
        "ACK_CAP",
        "起一条端口转发时链路那一侧回的 ack 那一行（同 `remote_ask` 读 ack 的上限）",
        "拒收+回错",
    ),
    (
        "src/backend/dial/forwards.rs",
        "COUNT_CAP",
        "端口转发链路每接进一条连接报的那一行计数（`{\"accepted\":n}`）",
        "降级+说清",
    ),
    // 脱离常驻那条载体的 stderr 诊断文件：满了换份，旧的留一份，再早的丢 ——
    //   丢要带身份：新那份第一行写「上一份挪去了哪、再早的那一份丢了」（`stderr_log::roll_note`）。
    (
        "src/backend/stderr_log.rs",
        "CAP_BYTES",
        "脱离常驻那条载体的本机后端 stderr 诊断文件每一份的大小（当前 ＋ 旧的一份 ⇒ 盘上 ≤ 两倍）",
        "丢弃+带身份报告",
    ),
    (
        "src/backend/assets/ext.rs",
        "NOTE_MAX_CHARS",
        "扩展页上用户写的一条备注（字数；记进资产目录、随目录同步到别的后端）",
        "拒收+回错",
    ),
    (
        "src/backend/assets/asset_sync.rs",
        "PUSH_MAX_BYTES",
        "同步时一趟推给远端的载荷（管进一条 `sh -c` 命令，受 `MAX_ARG_STRLEN` 限）；多台切块，单台超了不推、说出来",
        "跳过+说清",
    ),
    // ⚠这条**由本护栏当场逮出来的**：backend 的 Claude 知识搬进
    // `agents/claudecode/` 之后，常量跟着换了住址与名字，而本表按「文件+常量名」定位 ⇒
    // 两格同时红（「有上限没登记」+「登记的那个算不出值」）。**登记表的键随搬迁同轮改。**
    (
        "src/backend/agents/claudecode/accounts.rs",
        "MAX_CONFIG_BYTES",
        "读 Claude 的账号配置文件",
        "硬报错",
    ),
    (
        "src/backend/observe/accounts_query.rs",
        "MAX_MANIFEST_BYTES",
        "backend 侧读账号 manifest",
        "硬报错",
    ),
    (
        "src/backend/observe/accounts_query.rs",
        "MAX_SESSION_FILE_BYTES",
        "读单个 `sessions/<PID>.json`",
        "跳过+说清",
    ),
    // ⚠HTTP 中转带进来的第一处上限。它被用在**两个方向**上，
    // 两处的处置**都是**「拒收+回错」，只是回的码不同 —— 逐字记下来，免得下一个人
    // 以为它只管请求那一侧：
    //   读**下游请求**头 ⇒ `read_head` 返回 `None` ⇒ 回 `400 Bad Request`
    //   读**上游响应**头 ⇒ 同一个 `read_head`  ⇒ 回 `502 Bad Gateway`
    // ⚠ 订正两格〔回修轮之五 08-25〕：
    //   ① 上面先前逐字点着 `relay/server.rs:97` / `:134` 两个**行号**〔行号墓碑〕，两个今天都不对了
    //      （那两处随后被改动顶走）。⇒ 改成点**函数名**：`server.rs::serve_one` 里那两个调用点。
    //      行号是**每一轮都会变的量**，写进登记表下一轮自动变成假话。
    //   ② 先前那句「同一份代码里 `relay/http1.rs:39`〔行号墓碑〕 的头注写的是「回 431」」**今天也不成立**：
    //      `read_head` 的头注早在回修轮（08-25）就订正过，它现在逐字写的是
    //      「超了返回 `None`」并说明两个调用点各自回 400 / 502 —— 与本表一致，没有分歧了。
    (
        "src/comms/outward/server.rs",
        "HEAD_CAP",
        "一次 HTTP 请求/响应的**头部**字节数（不是体）",
        "拒收+回错",
    ),
    // ⚠〔`K-H1` 回修轮之五 08-25，D3 `阻-1(D3)`〕**这一处是补一个真缺陷，不是补一条登记**：
    // 先前请求体的长度来自 `Content-Length`，值域是 `usize` 全域、**没有任何上界**
    // （`HEAD_CAP` 只管头字节数，管不到它）。一条 `Content-Length: 1000000000000`
    // ⇒ `vec![0u8; n]` 分配失败 ⇒ `handle_alloc_error` ⇒ **abort（SIGABRT）**，
    // 不走 unwind ⇒ `catch_unwind` 接不住；而中转是「一个进程服务 N 个会话」
    // ⇒ 打掉的是**当时所有会话的在途流**。
    (
        "src/comms/outward/server.rs",
        "BODY_CAP",
        "一条下游 HTTP 请求的**请求体**字节数（`Content-Length` 那个值）",
        "拒收+回错",
    ),
    // ⚠〔同上一轮〕与 `BODY_CAP` **不同族**：那一条防「拿外部给的一个数去分配」，
    // 这一条防「按真实收到的字节无界增长」（上游发一条永不换行的 `data:` 行 /
    // 永不结束的块长度行）。丢的只是 **tee 那一路**，下游拿到的字节一个不少。
    (
        "src/comms/outward/server.rs",
        "TEE_DECODE_CAP",
        "tee 侧解码缓冲攒着的那截（SSE 半行 / chunked 还没成形的块长度行）",
        "丢弃+带身份报告",
    ),
    // tee 交给 tap 口的**一个 SSE 事件**的原文字节数。超了这一件不交、位置号照占 ⇒
    // 接收侧看见 `n` 的缺口（身份 = 哪个响应的第几号）；下游的字节一个不少（tap 是抄一份）。
    (
        "src/comms/outward/tee.rs",
        "TAP_DATA_CAP",
        "tee 交给 tap 口的一个 SSE 事件（`data:` 后那段原文）的字节数",
        "丢弃+带身份报告",
    ),
    // 续订阅号登录令牌那一发：令牌端点回包的头与体各自的上限（经中转的一问一答原语读，超了整发作错、不截断）。
    (
        "src/backend/accounts/oauth/mod.rs",
        "ANSWER_CAP",
        "续登录令牌那一发令牌端点的回包（头 · 体各自）字节数",
        "硬报错",
    ),
    // 🔴 **〔条 67〕`ASSET_BYTE_CAP` 与 `RESPONSE_HEAD_BYTE_CAP` 这一对摘了。**
    // 它们管的是「backend 给自己拉一个可执行文件」那一跳，而那一跳随 `sidecars/` 整棵删了
    // （用户逐字「不在现在设计里的全部删掉」）。
    //
    // ⚠ **那一对买到的道理是本表最值钱的一条，删代码不许连它一起删**：
    //   头一版头与体**共用一个上限**，喂 100 字节上限 ＋ 200 字节响应体，读回来的是
    //   `Got(42)` —— 头把体的额度吃掉 158 字节，剩下 42 字节被当成一份完整的资产。
    //   那正是 `ALLOWED_SEMANTICS` 刻意排除的「静默截断」。
    //   ⇒ **一个上限罩着两个量，超限那一刻分不出被截的是谁。两个量就要两个数。**
    //   下一次再有「一趟 HTTP 里同时读头和体」的路，照这条办。
];

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// ★ **小到这个数以下的就不当体量看**〔audit-0805 §5 1y，08-06 给它一个名字〕。
///
/// 这条过滤本身是个取舍：名字里带 `MAX`/`CAP`/`LIMIT`/`BYTES` 的小整数，绝大多数是
/// 「条数 / 次数 / 重试上限」而不是字节体量；真出现一个比它还小的**体量**上限，
/// 会被**静默跳过**（1y 逐字登记着这一点，并说明只有 `the_exclusion_list_is_not_dead_wood`
/// 间接盯着它）。
///
/// ⚠ 从裸字面量提成常量，是为了让 §5 那一行**只留名字、不抄值** ——
/// 抄进计划里的值没有任何东西读它，改了这里它就成了假陈述（`plan-lint` 判据 3.9）。
/// 名字**刻意不含** `MAX`/`CAP`/`LIMIT`/`BYTES`：那几个词正是上面 `scan()` 的钩子，
/// 含了就会让本常量把自己当成一处待登记的上限。
const SMALLEST_PLAUSIBLE_SIZE: u64 = 1024;

/// 从源码里把 `const NAME: ty = <expr>;` 的字节数算出来。
///
/// 只认本仓在用的两种写法：`A * 1024 * 1024` 与 `1 << N`。
/// ⚠ 认不出来就返回 `None` 让调用方红 —— **不许猜**，猜错会让对拍变成一句空话。
fn eval_cap(expr: &str) -> Option<u64> {
    let e = expr
        .split("//")
        .next()
        .unwrap_or(expr)
        .trim()
        .trim_end_matches(';')
        .trim();
    let e = e.replace('_', "");
    if let Some((l, r)) = e.split_once("<<") {
        let (l, r) = (l.trim().parse::<u64>().ok()?, r.trim().parse::<u32>().ok()?);
        return l.checked_shl(r);
    }
    let mut acc: u64 = 1;
    for part in e.split('*') {
        acc = acc.checked_mul(part.trim().parse::<u64>().ok()?)?;
    }
    Some(acc)
}

/// 扫两棵树，抠出所有「像字节上限」的常量：名字含 MAX/CAP/LIMIT/BYTES 且值里有 1024 或 `<<`。
///
/// ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir`（`scanning_guard_registry` 逼的）。
/// ⚠ 先前这里写着「它**按构造摘除调用者自己那份**…本文件的 `CAPS`
/// 表里就写着一堆 `MAX_*` 名字；今天它们的类型不是 `u64/usize` 所以扫不中，
/// 但那是**运气**不是设计」。那一刀**在这一处不生效**（判据由 `#[path]` 挂载
/// ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中）。
/// 今天挡住那张 `CAPS` 表自匹配的是**住址**：本文件住 `tests/frontend/shell/`，而下面三棵根是
/// `src/frontend/shell/src` · `src/backend` · `src/common` —— 它一份都不在里面。
/// 〔audit-0805 **F23**：这一族已实测栽过五次〕
/// 生产段里**全部**尺寸类 const：`const NAME: u64|usize|u32 = <expr>`。
///
/// **抽成共享量具**：`scan()` 是它按名字关键词过滤后的子集，
/// 而下面那条默认拒绝判据量的是它本身 —— 两者共用同一份遍历，
/// 免得「自检量了一份副本」（本会话在 `session_name_registry` 上刚踩过）。
fn size_typed_consts() -> Vec<(String, String, Option<u64>)> {
    let root = repo_root();
    let mut out = Vec::new();
    // `src/common` 也要扫：共享 crate 里同样住着上限。
    for sub in ["src/frontend/shell/src", "src/backend", "src/common"] {
        for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let body = guard_core::production_source(&raw);
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            for line in body.lines() {
                let t = line.trim();
                let Some(rest) = t
                    .strip_prefix("pub(crate) const ")
                    .or_else(|| t.strip_prefix("pub const "))
                    .or_else(|| t.strip_prefix("const "))
                else {
                    continue;
                };
                let Some((name, tail)) = rest.split_once(':') else {
                    continue;
                };
                let Some((ty, expr)) = tail.split_once('=') else {
                    continue;
                };
                if !matches!(ty.trim(), "u64" | "usize" | "u32") {
                    continue;
                }
                match eval_cap(expr) {
                    Some(v) if v >= SMALLEST_PLAUSIBLE_SIZE => {
                        out.push((rel.clone(), name.trim().to_string(), Some(v)))
                    }
                    Some(_) => {}
                    None => out.push((rel.clone(), name.trim().to_string(), None)),
                }
            }
        }
    }
    out.sort();
    out
}

/// ★**默认拒绝**：尺寸类常量要么进人群，要么登记为「不是体量」。
///
/// # 它补的洞
///
/// `scan()` 的人群靠**名字里有没有 MAX/CAP/LIMIT/BYTES**。那是**按怎么写取样** ——
/// 一个叫 `TRUNCATE_AT` / `CEILING` / `ROOM` 的字节上限整条隐形，而没有任何信号。
///
/// ⚠ 实测**今天没有活的漏网**：名字不含关键词的尺寸类常量只有 4 个，
/// 逐个读过都不是字节上限（Win32 时间纪元 · SFTP 传输步长 · 进度上报间隔 · 时间容差）。
/// 所以本条钉的是**明天**。
///
/// # 为什么不按「用法」派生（量过之后否掉的）
///
/// 试过「被拿去和长度比较 / 截断的常量才算」：实测它**漏掉最像字节量的那个**
///（`CHUNK`，因为它用在 `read_exact` 的缓冲区长度上而不是比较里），
/// 却**圈进两个时间常量**（`PROGRESS_EVERY` / `PROC_START_TOLERANCE_TICKS` 都用 `>=` 比）。
/// ⇒ 派生错人群比手写清单更糟（本会话第四次量到同一条），改走默认拒绝：
/// **人群取可机判的超集**（所有 ≥1024 的尺寸类 const），
/// 「是不是字节上限」这个语义判断**登记成人的答案**，而不是猜。
#[test]
fn every_size_typed_constant_is_either_a_cap_or_registered_as_not_one() {
    let all = size_typed_consts();
    // 抽取器自检：登记的每一处上限都得被这一份遍历看见（两侧不同源：表是人写的，人群是现扫的）。
    for (f, n, ..) in CAPS {
        assert!(
            all.iter().any(|(g, m, _)| g == f && m == n),
            "尺寸类常量的遍历没看见登记的 `{f}::{n}` —— 抽取器坏了，本条会零命中地绿"
        );
    }
    let mut unclassified = Vec::new();
    for (rel, name, v) in &all {
        let in_population = ["MAX", "CAP", "LIMIT", "BYTES"]
            .iter()
            .any(|k| name.contains(k));
        let excused = NOT_A_SIZE_CAP.iter().any(|(n, _)| n == name);
        if !in_population && !excused {
            unclassified.push(format!("  {rel}: {name} = {v:?}"));
        }
    }
    assert!(
        unclassified.is_empty(),
        "这些尺寸类常量既不在字节上限人群里（名字没有 MAX/CAP/LIMIT/BYTES），\n\
             也没登记为「不是体量」：\n{}\n\
             ⚠ **名字不是判据** —— 一个叫 `TRUNCATE_AT` 的字节上限同样是字节上限。\n\
             要么改名进人群并在 `CAPS` 里说清「限什么 / 超了怎么办」（定框 E5 要求成对），\n\
             要么加进 `NOT_A_SIZE_CAP` 并写明它量的是什么（条数 / 时间 / 步长 …）。",
        unclassified.join("\n")
    );
}

fn scan() -> Vec<(String, String, Option<u64>)> {
    let root = repo_root();
    let mut out = Vec::new();
    // 🔴 `K-R100` 09-13：`src/common` 这一条与 `size_typed_consts()` 那边同源同理
    // ——搜索的两条封顶已收进共享 crate `search-core`。
    // ⚠ 上面 `size_typed_consts` 的头注说「两者共用同一份遍历」，**盘上不是**：
    // 这里自己又走了一遍。⇒ **改扫描面要两处一起改**（本轮就是漏了这一处才红的）。
    for sub in ["src/frontend/shell/src", "src/backend", "src/common"] {
        for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            // ★ 只扫**生产段**：本条原来扫整份文件，于是**测试里的夹具常量**
            // 也被当成生产上限（`common/fs.rs` 的顺序判据里那个 `const CAP` 当场被误报）。
            // 判据扫生产段是本仓通行做法（`guard_core` 头注那一族）—— 这里此前是例外。
            let body = guard_core::production_source(&raw);
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            for line in body.lines() {
                let t = line.trim();
                let Some(rest) = t
                    .strip_prefix("pub(crate) const ")
                    .or_else(|| t.strip_prefix("pub const "))
                    .or_else(|| t.strip_prefix("const "))
                else {
                    continue;
                };
                let Some((name, tail)) = rest.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if !["MAX", "CAP", "LIMIT", "BYTES"]
                    .iter()
                    .any(|k| name.contains(k))
                {
                    continue;
                }
                let Some((ty, expr)) = tail.split_once('=') else {
                    continue;
                };
                if !matches!(ty.trim(), "u64" | "usize" | "u32") {
                    continue;
                }
                if NOT_A_SIZE_CAP.iter().any(|(n, _)| *n == name) {
                    continue;
                }
                // 下界见 `SMALLEST_PLAUSIBLE_SIZE`：更小的多半是「条数 / 次数」而不是体量。
                // ⚠ 这条过滤本身是个取舍，`the_exclusion_list_is_not_dead_wood` 盯着它。
                match eval_cap(expr) {
                    Some(v) if v >= SMALLEST_PLAUSIBLE_SIZE => {
                        out.push((rel.clone(), name.to_string(), Some(v)))
                    }
                    Some(_) => {}
                    None => out.push((rel.clone(), name.to_string(), None)),
                }
            }
        }
    }
    out.sort();
    out
}

/// ★ 正题一：**每一处字节上限都得登记它管什么量、超限怎么办**。
#[test]
fn every_byte_cap_says_what_it_bounds_and_what_happens_past_it() {
    let found = scan();
    // 抽取器坏了扫不到东西 ⇒ 下面反向那一条（登记的必须真在）红。
    let missing: Vec<String> = found
        .iter()
        .filter(|(f, n, _)| !CAPS.iter().any(|(g, m, ..)| g == f && m == n))
        .map(|(f, n, v)| format!("  {f}  {n} = {v:?}"))
        .collect();
    assert!(
        missing.is_empty(),
        "有字节上限没登记：\n{}\n\n\
             登记时要回答两件事，**都不许留空**：\n\
             ① 它管的是**什么量**（账号清单？一整份会话？一次 exec 的输出？）——\n\
                报告 B-5 说「三处上限该收成一个常量」，核实之后是 **14 处、至少 9 种不同的量**，\n\
                硬收成一个就是把不同的东西按数字凑到一起。\n\
             ② **超限怎么办**，且只能是 {ALLOWED_SEMANTICS:?} 之一 —— \n\
                **没有「静默截断」这一项**（那正是报告 B-5 点名、F06 上半修掉的那处）。",
        missing.join("\n")
    );
    // 反向：登记的必须真在（改名/删掉了就该删条目）。
    for (f, n, ..) in CAPS {
        assert!(
            found.iter().any(|(g, m, _)| g == f && m == n),
            "登记表里的 `{f}::{n}` 已经不在源码里了 —— 删掉这条，别留僵尸账"
        );
    }
}

/// ★ 正题三：**跨 crate 那几对「与另一侧同值」的注释，变成机检**（账本 S3 的钉法）。
///
/// ⚠ 三对的**口径不一样**，不能一把梭都钉「相等」：
/// 第三对的注释写的是「同**量级**」，而它们**今天就不等**（8 KiB vs 4 KiB）——
/// 钉「相等」会造出一条假判据。
#[test]
fn the_cross_crate_twins_are_machine_checked_not_hand_copied() {
    let by = |f: &str, n: &str| -> u64 {
        scan()
            .into_iter()
            .find(|(g, m, _)| g == f && m == n)
            .and_then(|(_, _, v)| v)
            .unwrap_or_else(|| panic!("抠不到 `{f}::{n}` —— 本条会零命中地绿"))
    };

    // 对 A 🔴 **这一对没了，是被解决掉的**：monitor 那一侧（`local_accounts.rs` 的
    //   `MANIFEST_CAP`，本机账号 manifest 参照实现）随那份实现删了 ⇒ 读 manifest 的上限只剩后端
    //   `observe/accounts_query.rs::MAX_MANIFEST_BYTES` 一处，「两侧漂开」在结构上不再可能（同对 D 的处置）。

    // 对 B：两边都是「一整份会话 jsonl」这同一个量 ⇒ 钉相等。
    let b1 = by("src/backend/faces/read_face.rs", "WHOLE_SESSION_MAX_BYTES");
    let b2 = by(
        "src/backend/control/fork_write.rs",
        "MAX_SESSION_JSONL_BYTES",
    );
    assert_eq!(
        b1, b2,
        "「一整份会话 jsonl」的上限两处漂开了（查看器整份读 {b1} / 分叉 {b2}）。\
             ⚠ `fork_write.rs` 的注释写的是「同一**量级**」，而本条钉的是**相等** —— \
             因为它们是同一个量。要刻意分开就把这条判据与那句注释**一起**改。"
    );

    // 对 D（搜索索引封顶）🔴 **这一对没了，是被解决掉的，不是被删掉的**。
    //
    // 原文：`src/frontend/shell/src/search.rs` 与 `src/backend/observe/search_query.rs`
    // 各写一个 `MAIN_CAP`/`TOOL_CAP`，本条钉它们相等。收口后两个字面量只剩一份
    // （`search-core`；今天住后端 `observe/search_rules.rs`）⇒ **「两侧漂开」在结构上不再可能**，一条对拍相等的判据也就无从谈起
    // （它会变成「同一个数等于它自己」，恒绿）。
    // 把这件事焊住的判据换了个形状，住（随家搬进后端）
    // `tests/backend/observe/search_rules_tests.rs` 的 `the_search_kou_jing_has_exactly_one_home`：
    // 它断言搜索那一侧（今天只剩后端）生产段**不许**再出现 `const MAIN_CAP` / `const TOOL_CAP` 之类的定义，
    // monitor 生产树里零处再搜由 `search_kou_jing_guard.rs::the_monitor_grows_no_search_helpers` 断言。
    // ⇒ 这两个数搬回任何一侧，当场红。

    // 对 E〔F9 续 09-24 立；F9c〕：窗口的编辑上限**就是**后端读 / 提交存盘的天花板 ⇒ 钉相等。
    //   窗口多给 ⇒ 打得开、改得了，存的时候后端 `files-commit-text` 拒（`bytes` 越过天花板）、读的时候
    //   `files-read-text` 拒 `max_bytes`；窗口少给 ⇒ 存得回的文件被本地冤拒。两个方向都是错。
    // 上一版这一对钉的是「编辑上限 == 后端入方向一行」（那时存盘整份装一行）；那一对换成下面的对 F。
    let e1 = by("src/frontend/filewin/src/editor.rs", "MAX_EDIT_BYTES");
    let e2 = by("src/backend/files/mod.rs", "READ_TEXT_MAX_BYTES");
    assert_eq!(
        e1, e2,
        "窗口编辑上限与后端读 / 提交存盘的天花板漂开了（窗口 {e1} / 后端 {e2}）。\
             存得回的要读得回来：`files-read-text` 的 `max_bytes` 与 `files-commit-text` 的 `bytes` 都按它拒。"
    );

    // 对 F：窗口存盘时一行的上限 ＝ 后端入方向一行的上限 ⇒ 钉相等。
    //   窗口多给一个字节 ⇒ 后端整行丢弃（`line_too_long`）；少给 ⇒ 只是多切几块（不错，但两份数漂了就该有人看）。
    let f1 = by("src/frontend/filewin/src/editor.rs", "SAVE_LINE_CAP");
    let f2 = by("src/backend/stream/inbound/mod.rs", "MAX_LINE_BYTES");
    assert_eq!(
        f1, f2,
        "窗口存盘一行的上限与后端入方向一行上限漂开了（窗口 {f1} / 后端 {f2}）。\
             分块那一支按它切（`editor::plan_chunks`），每块那一行都得装进后端的一行。"
    );

    // 对 C：注释写的是「同**量级**」，而且**今天就不等** ⇒ 钉比值，不钉相等。
    let c1 = by("src/backend/control/launch.rs", "MAX_FIELD_BYTES");
    let c2 = by("src/frontend/shell/src/launch.rs", "MAX_REMOTE_CMD");
    let (hi, lo) = if c1 >= c2 { (c1, c2) } else { (c2, c1) };
    assert!(
        lo > 0 && hi <= lo * 4,
        "「一条人能读的启动命令」两侧差得太远（backend {c1} / monitor {c2}）。\
             ⚠ 这一对**刻意不同值**（今天 8192 vs 4096），注释写的是「同量级」；\
             这里把「量级」形式化成**不超过 4 倍** —— 那是个约定，不是实测阈值。"
    );
}

/// ★ **排除表不许长草**（上面 `scan()` 的注释引了这条 —— 铁律 14：
/// 指向不存在的判据比没有注释更坏，所以它必须真的存在）。
///
/// 排除掉的名字必须**真的还在源码里**：否则那条排除是一条永远不匹配的死规则，
/// 而死规则会在下一次有人往这个名字上写一个**真的**体量上限时**悄悄放行**。
#[test]
fn the_exclusion_list_is_not_dead_wood() {
    let root = repo_root();
    let mut all = String::new();
    // 🔴 `K-R100` 09-13 加 `src/common`：排除表的语料面必须与**产生上限的那个语料面**
    // 一致（`size_typed_consts()` / `scan()` 都已含它）。不一致的话，一条针对共享 crate
    // 里常量的排除会被本条判成「死木」—— 而它其实活着，只是本条看不见它。
    for sub in ["src/frontend/shell/src", "src/backend", "src/common"] {
        for (_, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let body = guard_core::production_source(&raw);
            all.push_str(&body);
        }
    }
    for (name, why) in NOT_A_SIZE_CAP {
        assert!(
            all.contains(&format!("const {name}")),
            "排除表里的 `{name}` 已经不在源码里了 —— 删掉这条，别留死规则（{why}）"
        );
        assert!(
            why.chars().count() > 15,
            "`{name}` 的排除理由太短，像是占位：「{why}」。\
                 排除一个名字要说清**它为什么不是体量上限**（条数？地板？次数？）"
        );
    }
}

/// 每条登记的超限语义必须在封闭集合里，且**不许出现「静默」**。
#[test]
fn no_cap_is_allowed_to_fail_silently() {
    for (f, n, what, sem) in CAPS {
        assert!(
            ALLOWED_SEMANTICS.contains(sem),
            "`{f}::{n}`（{what}）的超限语义写的是「{sem}」，不在 {ALLOWED_SEMANTICS:?} 里。\
                 多出一种就得回来论证 —— 定框 **E5**：上限与超限语义**成对定义**。"
        );
        assert!(
            !sem.contains("静默") && !what.contains("静默"),
            "`{f}::{n}` 出现了「静默」。报告 B-5 点名的就是这一种（`remote_history` 那处），\
                 F06 上半已经修掉。**这个集合就是那次修复的护栏。**"
        );
    }
    // 语义集合不许长草：登记表里每一种都得真有人用。
    for sem in ALLOWED_SEMANTICS {
        assert!(
            CAPS.iter().any(|(_, _, _, s)| s == sem),
            "允许集合里的「{sem}」今天一处都没有人用 —— 删掉它，别留一个谁都能往里塞的口子"
        );
    }
}
/// ★ **登记的语义要和调用点的处置对得上**〔audit-0805 §5 1x，08-06〕。
///
/// # 1x 说的「登记的不是验过的」，这条是它可判定的那一半
///
/// 本表此前只查「语义在封闭集合里」，**不查代码真的那么做了**。
/// 08-06 逐条对之后当场查出一处不符：`MAX_SESSION_FILE_BYTES` 登记「硬报错」，
/// 而调用点是 `Err(_) => continue` —— **静默跳过**，而「静默」正是这张封闭集合
/// **刻意排除**的那一项（F06 上半就是为它建的）。
///
/// ⇒ 本条钉：登记为 **硬报错** 的上限，它的调用点**不许把错误吞掉**。
/// 吞掉的形态（`Err(_) =>` / `.ok()` / `unwrap_or_default` / `unwrap_or(`）出现在
/// 用到该常量的那几行里就红。
///
/// ⚠ **只查「硬报错」这一档**：其余四档（截断/拒收/分轮/索引截断/跳过）各有各的正当
/// 处置形态，一条规则套不下 —— 硬做就是把判据的匹配面画得比事实大（本区反复记的那族）。
/// 其余档的行为对拍**如实留在 1x**，不假装覆盖了。
#[test]
fn a_cap_registered_as_hard_error_is_not_swallowed_at_its_call_site() {
    let root = repo_root();
    let mut bad = Vec::new();
    let mut checked_sems = std::collections::BTreeSet::new();
    for (file, name, _what, sem) in CAPS {
        if !["硬报错", "跳过+说清", "降级+说清"].contains(sem) {
            continue;
        }
        let raw = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("{file} 读不到：{e} —— 文件搬了就把登记一起改"));
        // 剥**注释 + 测试段**：
        // ① 注释里常常逐字写着这些形态（本区第四次踩这个坑就不该再踩）；
        // ② 测试段里也会用这些常量（`accounts_query.rs` 的 symlink 用例就是），
        //    而测试里怎么处置错误与生产语义无关 —— 本条第一次跑就在那里误报。
        let src = guard_core::production_code(&raw);
        let lines: Vec<&str> = src.lines().collect();
        let mut checked = false;
        for (i, l) in lines.iter().enumerate() {
            if !guard_core::contains_word(l, name) || l.contains("const ") {
                continue;
            }
            checked = true;
            // ⚠ 窗口 **10 行是个启发式参数**，不是量出来的分界。
            //
            // 它要覆盖的是「`match` 的 `Err` 臂里返回一个结构体」这种最长的正当形态
            // （`local_accounts` 那处占 7 行）。第一版给 3 行、第二版给 6 行，
            // 都是**差一行**误报 —— 与其一路加，不如把它写成启发式并交代失效模式：
            //   · 臂特别长 ⇒ **假红**（marker 落在窗口外）；
            //   · 紧邻的**别的**语句里恰好有 marker ⇒ **假绿**。
            //     ⚠ 这一条**不再是假想的**，实测撞上了：把
            //     `plugins.rs` 的降级臂改成 `Err(_why) => {}`（真·静默降级），
            //     本条**照样绿** —— 因为**下一个** `match` 臂里的 `declared_error`
            //     落进了同一个窗口。⇒ 本条对「同一个 `match` 里有多条臂」这种形状
            //     **钉不住**，别读成「降级一定被说清了」；那一层今天靠站点自己的
            //     行为判据兜（`plugins.rs` 那两条当场红）。
            // 那个站点连同那两条行为判据随读实现搬去了后端，后来随插件只读列表一起删了。
            // 两种都靠人读诊断分辨。已登记进。
            const WINDOW: usize = 10;
            let window = lines[i..(i + WINDOW).min(lines.len())].join("\n");
            // ★ **正向要求**，不是黑名单。
            //
            // 第一版列了吞掉的形态（`Err(_) =>` / `.ok()` / …）。变异当场证明它没用：
            // 把 `Err(_)` 写成 `Err(e)` 就绕过去了 —— **匹配单位（一种拼法）比事实
            // （把错误吞了）小**，本区那一族的又一次。
            // 正向问「传上去了吗」只有一个答案形态：`?`。拼写变体绕不过去。
            let (need, why) = if *sem == "硬报错" {
                ("?", "把错误传上去")
            } else if *sem == "降级+说清" {
                // 降级也得**把错误带出去**（塞进 `meta`/`notice`/`error` 任一）。
                // 不带 = 静默降级，那正是这张封闭集合刻意排除的东西。
                //
                // ⚠⚠ 上面这句话**从第一天起就写着「任一」，
                // 而代码只认 `meta`** —— 那是本表只有一个降级点（`local_accounts`）时
                // 留下的 **n=1 的针**：它钉的是那一处**碰巧用的字段名**，不是「把错误带出去」
                // 这个性质。第二个降级点一来就误报。
                // ⇒ 改成**认它自己承诺的那三个词**，不是为放行谁而放宽。
                // ★ 如实登记失效模式：`error` 这个词比 `meta` 常见，
                // 窗口里恰好出现别的 `error` 会**假绿** —— 与本条窗口启发式的
                // 既有失效模式同档，靠人读诊断分辨。
                ("meta|notice|error", "把错误带进返回值")
            } else {
                // 「跳过+说清」那一档：跳过本身不是问题，**跳过而不说是谁**才是（E4）。
                // 变异实测：去掉那句 `warn!`，此前**没有任何判据会红** —— 这一档
                // 当时只是个名字。
                ("warn!", "说清跳过的是哪一个")
            };
            // ★ **「硬报错」那档改用「同一条语句」而不是窗口**〔变异逼出来的〕。
            //
            // 窗口版当场被证伪：把 `…?` 换成 `.unwrap_or_default()`，**判据照样绿** ——
            // 因为**下一条语句**的 `?` 落进了窗口。那正是上面注释里刚写下的假绿模式，
            // 写完立刻就撞上了。
            // ⇒ 传播是**语句级**的性质：从常量所在行扫到**第一条以 `;` 收尾的行**为止。
            // 其余两档的 marker 在 `match` 臂**块内**，语句级切不到，仍用窗口。
            let scope = if *sem == "硬报错" {
                let mut k = i;
                while k < lines.len() && !lines[k].trim_end().ends_with(';') {
                    k += 1;
                }
                // ⚠⚠ **这个扫描原来没有上界，而那让本条对一整类站点失灵**。
                //
                // 变异实测：把一处**真降级**的站点登记成「硬报错」⇒ 本条**照样绿**。
                // 原因是常量落在 `match … {` 那一行的**臂头**上，而臂们都以 `,` 收尾 ——
                // 扫描一路穿过整个 `match`、穿出函数，直到几十行外某条真以 `;` 收尾的
                // 语句，而那里的 `?` 把它喂绿了。
                // ⇒ 「同一条语句」这个意图本来就该有界：**封到与另外两档同一个窗口**。
                // 失效模式如实登记：语句真的长过 `WINDOW` 行 ⇒ **假红**（同窗口档）。
                lines[i..(k + 1).min(lines.len()).min(i + WINDOW)].join("\n")
            } else {
                window.clone()
            };
            // 针可以是**一组备选**（`|` 分隔）：命中任一即可。
            if !need.split('|').any(|n| scope.contains(n)) {
                bad.push(format!(
                    "  {file}:{} 用 {name}（登记「{sem}」）却没有 `{need}` —— 没有{why}",
                    i + 1
                ));
            }
        }
        if checked {
            checked_sems.insert(*sem);
        }
    }
    // 抽取器正控：这三档里表上登记了的每一档，都至少有一处使用点被看见。
    for sem in ["硬报错", "跳过+说清", "降级+说清"] {
        assert!(
            !CAPS.iter().any(|(.., s)| *s == sem) || checked_sems.contains(sem),
            "登记「{sem}」的上限一处使用点都没找到 —— 抽取器坏了，本条此刻是空转的"
        );
    }
    assert!(
        bad.is_empty(),
        "登记的语义与调用点对不上：\n{}\n\n\
             ★ 登记「硬报错」而调用点把错误吞掉 = **登记是假的**。\n\
             08-06 实测过一处（`MAX_SESSION_FILE_BYTES` 登记硬报错、实为静默跳过）。\n\
             两条路：把调用点改成真的传上去；或者把登记改成实话 —— \n\
             若真实处置是跳过，**必须同时给它身份**（`warn!` 说清是哪一个），\n\
             才够得上「跳过+说清」那一档。**没有身份的跳过不许登记。**",
        bad.join("\n")
    );
}

/// 内联读上限里**实参不是具名常量**的那些。
/// `(相对仓根的路径, `take` 的实参逐字, 为什么它不必进 `CAPS`)`
///
/// 只有一种正当情况：上限是**参数**，真值由调用方给（而调用方给的是具名常量）。
const PARAMETRIC_READ_CAPS: &[(&str, &str, &str)] = &[
    // `files-read-chunk` 一块读多少是入参 `len`，入口先判 `1..=READ_CHUNK_MAX_BYTES`（已在 `CAPS` 里）、越界 `bad_args`。
    // ⚠ 不是静默截断：回 `offset` / `size` / `eof`，调用方循环读到 `eof` —— 一个字节都不丢。
    (
        "src/backend/files/mod.rs",
        "len",
        "`files-read-chunk` 的 `len` 是入参，入口先判 `1..=READ_CHUNK_MAX_BYTES`（已在 `CAPS` 里）；回 `eof` / `size`，\
             调用方读到 `eof` 为止 —— 不是截断后当完整的用。",
    ),
    // `history_query::read_page` 的一页上限是入参；唯一调用点
    // （`read_face.rs`）给的是具名常量 `READ_PAGE_BYTES`（已在 `CAPS` 里）。
    // ⚠ 不是静默截断：读满一页就停、**回续点 `next`**，调用方循环到 `eof` —— 一个字节都不丢。
    (
        "src/backend/observe/history_query.rs",
        "page as u64",
        "`read_page` 的一页上限是入参，调用方给 `read_face::READ_PAGE_BYTES`（已在 `CAPS` 里）；\
             读满即停并回续点，由调用方翻下一页 —— 分页，不是截断。",
    ),
    // `read_face::log_tail` 的上限是入参 `max`，唯一调用点给的是 `min(调用方要的, LOG_TAIL_BYTES)`（已在 `CAPS` 里）。
    // ⚠ 不是静默截断：从 `size − max` 起读、应答带 `truncated`（前面还有没回的）。
    (
        "src/backend/faces/read_face.rs",
        "max",
        "`log_tail` 的上限是入参，`backend-log` 那一臂给的是封顶 `LOG_TAIL_BYTES`（已在 `CAPS` 里）；\
             读的是尾部，应答 `truncated` 说清前面还有 —— 截断且说清。",
    ),
    (
        "src/backend/common/fs.rs",
        "cap + 1",
        "`read_file_capped` 是后端侧共用的有界读助手，上限是入参；\
             调用方给的是 `MAX_CONFIG_BYTES` / `MAX_MANIFEST_BYTES` 等具名常量。",
    ),
    // ⚠〔`K-H1` 回修轮之五 08-25，D3 `阻-1(D3)`〕这一处的 `n` 与上面两条**不同族**，
    //    差别要写清楚，否则下一个人会以为它也是「调用方给具名常量」那一档：
    //    `n` 是**下游 `Content-Length` 给的、敌手可控的**值，**不是**任何具名常量。
    //    真正的上限是**它旁边那个 `cap` 参数**，而调用点给的 `cap` 是具名常量 `BODY_CAP`
    //    （已在 `CAPS` 里，拒收+回错 ⇒ 413）。
    //    ★ 顺序是这一格的**全部**：`if n > cap { return Ok(None); }` 在 `take(n)` **之前** ——
    //    颠倒过来 `take` 就会按敌手给的数去长。死值验 `MU2` 打的正是这个顺序（1 红）。
    (
        "src/comms/outward/http1.rs",
        "n as u64",
        "`read_exact_body` 的 `n` 是**请求体声明长度**（下游 `Content-Length`，敌手可控），\
             它**不是上限**；上限是同一函数的 `cap` 入参，唯一调用点给的是具名常量 `BODY_CAP`\
             （已在 `CAPS` 里登记，超限语义「拒收+回错」⇒ 413）。\
             `n > cap` 的判断在 `take(n)` **之前**，所以这里读不满 `cap` 就停 —— \
             不是静默截断：超限那一支一个字节都不读，直接回 413。",
    ),
    // files 链路的 `put`：`size` 是请求里**声明**的字节数（monitor 给），不是上限；
    //   上限是具名常量 `MAX_PUT_BYTES`（已在 `CAPS` 里），`size > MAX_PUT_BYTES` 的判断在 `take(size)` **之前**。
    (
        "src/backend/dial/sftp.rs",
        "size",
        "`put` 的 `size` 是请求里声明的长度（对端给的），**不是上限**；上限是具名常量 `MAX_PUT_BYTES`\
             （已在 `CAPS` 里，超限「拒收+回错」）。`size > MAX_PUT_BYTES` 那一支在 `take(size)` **之前**：\
             超了就把那几个字节照收照丢、回 `too_big`；没超才读进内存，读不满回 `bad_request` —— 不是静默截断。",
    ),
    // 🔴 〔条 67〕按需拉取那一跳的「一趟别读过头」那条摘了 —— 随 `sidecars/` 整棵走。
    //    它记的是「那个上限是同一趟里两个量的和，它只负责『一趟别读过头』，
    //    头与体读进来之后**各自**再对自己那个上限判一次」。那个形状仍值得照抄。
];

/// ★ **前提触发器的重写**。
///
/// # 它替掉的那条判据**一直在假绿**
///
/// 旧条叫 `inline_literal_byte_caps_are_still_just_the_one`，自述逐字：
/// 「本表的扫描面只认具名常量…范围要成立，得有个东西盯着『范围外那一族别长大』…
/// 实测人群 **1**…人群变 2 的那天这个判断就该重做 —— **本条就是那个闹钟**」。
///
/// 它抠 `.take(` 之后的 `is_ascii_digit()` 连续段，再要求**紧接着**是 `.read_to_end`。
/// 于是 `.take(32 * 1024 * 1024)` 抠出 `"32"`、后面是 `" * 1024 …"` ⇒ 形态不匹配 ⇒ **不计数**。
/// 实测那一族当时已有 **6 处**，它只看得见 1 处。**闹钟响过五次，一次都没听见。**
///
/// ★ 病根与本仓反复记的那一条同族：**它按「一种拼法」取样，而事实是「有没有上限」**。
/// 而它自己的注释里逐字写过这个失效模式（「匹配单位（一种拼法）比事实小」）——
/// 写下那句话的判据，自己犯了那句话说的错。
///
/// # 重写后的取样：按**事实**
///
/// 人群 = 生产段里每一处「`.take(<任意实参>)` 紧邻一个字节读」。实参怎么拼不影响入群。
/// 入群之后**默认拒绝**：实参要么是已登记的具名常量（`CAPS` 管它），
/// 要么在 [`PARAMETRIC_READ_CAPS`] 里说清为什么不必进表。
#[test]
fn every_inline_read_cap_resolves_to_something_registered() {
    // 抽取器正控：一处紧邻字节读的 `.take(…)` 要被看见，不紧邻的不算。
    assert_eq!(
        take_args_before_a_read(
            "let n = r.take(FOO_CAP + 1).read_to_end(&mut b)?; it.take(3).count();"
        ),
        vec!["FOO_CAP + 1".to_string()],
        "内联读上限的抽取器坏了，下面的默认拒绝此刻是空转的"
    );
    let sites = inline_read_cap_sites();
    let mut unresolved = Vec::new();
    for (file, arg) in &sites {
        // 剥掉 `+ 1`（「多读一个字节好分辨刚好读满与其实还有」那个惯用形态）。
        let bare = arg.split('+').next().unwrap_or(arg).trim().to_string();
        let is_named_const = !bare.is_empty()
            && bare
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && bare.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        if is_named_const {
            if CAPS.iter().any(|(_, n, ..)| *n == bare) {
                continue;
            }
            unresolved.push(format!(
                "  {file}: .take({arg}) —— `{bare}` 是具名常量但**不在 CAPS 里**"
            ));
            continue;
        }
        if PARAMETRIC_READ_CAPS
            .iter()
            .any(|(f, a, _)| f == file && a == arg)
        {
            continue;
        }
        unresolved.push(format!(
            "  {file}: .take({arg}) —— 实参不是具名常量，也没登记"
        ));
    }
    assert!(
        unresolved.is_empty(),
        "这些内联读上限没法追溯到任何登记：\n{}\n\n\
             ★ **裸字面量上限是本表五次假绿的病灶** —— 它不在主扫描面里，\n\
             所以「限什么量 / 超限怎么办」两问都没人回答，而实测那六处的答案全是\n\
             **静默截断**（`.take(N).read_to_end()` 读满就停、半份数据被当完整的用），\n\
             那正是 `ALLOWED_SEMANTICS` 刻意排除的那一种。\n\
             两条路：① 提成具名常量（那样它自然进主表，两问必须回答）；\n\
             ② 若上限真是**入参**，登记进 `PARAMETRIC_READ_CAPS` 并写清调用方给的是哪些具名常量。",
        unresolved.join("\n")
    );
    // 反向：登记不许留死行。
    for (file, arg, _) in PARAMETRIC_READ_CAPS {
        assert!(
            sites.iter().any(|(f, a)| f == file && a == arg),
            "`PARAMETRIC_READ_CAPS` 里的 `{file}: .take({arg})` 已经不在源码里了 —— \
                 删掉这条，别留死规则"
        );
    }
}

/// 抠出生产段里所有「`.take(<实参>)` 紧邻一个字节读」的位置。
///
/// 返回 `(相对仓根的路径, 实参逐字)`。
///
/// ⚠ 两个坑是旧版注释里逐字记过的，这里照样避开：
/// ① **不能按字节切**（中文注释里会切在多字节字符中间直接 panic）⇒ 走 `Vec<char>`；
/// ② **不能用宽窗口**（会把邻近另一行的读算进来，旧版实测抓出三个假阳）⇒ 要求紧邻。
/// ⚠ 但**不再按拼法取实参** —— 走括号配平，`32 * 1024 * 1024` / `cap + 1` / `FOO` 一视同仁。
fn inline_read_cap_sites() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out = Vec::new();
    for sub in ["src/frontend/shell/src", "src/backend"] {
        for (path, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let prod = guard_core::production_code(&src);
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for arg in take_args_before_a_read(&prod) {
                out.push((rel.clone(), arg));
            }
        }
    }
    out.sort();
    out
}

/// 一份生产段里每一处「`.take(<实参>)` 紧邻一个字节读」的实参逐字。
fn take_args_before_a_read(prod: &str) -> Vec<String> {
    const READS: &[&str] = &[".read_to_end", ".read_exact", ".read_to_string"];
    let mut out = Vec::new();
    let cs: Vec<char> = prod.chars().collect();
    let needle: Vec<char> = ".take(".chars().collect();
    let mut i = 0usize;
    while i + needle.len() <= cs.len() {
        if cs[i..i + needle.len()] != needle[..] {
            i += 1;
            continue;
        }
        // 括号配平取实参。
        let mut j = i + needle.len();
        let mut depth = 1usize;
        let mut arg = String::new();
        while j < cs.len() && depth > 0 {
            match cs[j] {
                '(' => {
                    depth += 1;
                    arg.push('(');
                }
                ')' => {
                    depth -= 1;
                    if depth > 0 {
                        arg.push(')');
                    }
                }
                c => arg.push(c),
            }
            j += 1;
        }
        // 紧邻：跳过空白之后必须直接是一个字节读。
        let mut k = j;
        while k < cs.len() && cs[k].is_whitespace() {
            k += 1;
        }
        let tail: String = cs[k..(k + 16).min(cs.len())].iter().collect();
        if READS.iter().any(|r| tail.starts_with(r)) {
            out.push(arg.trim().to_string());
        }
        i = j.max(i + 1);
    }
    out
}

/// ★ **异步流整读必须有上限**：对端是另一个进程，无界读就是无界堆分配。
///
/// 人群取「把一整条**流**读进内存」这个事实（读的动作紧邻 `.await`）；
/// 同步的 `std::fs::read_to_string(path)` 读的是本机文件、体量由磁盘兜着，不同族。
#[test]
fn no_async_stream_read_is_uncapped() {
    // 抽取器正控：一处无界的异步流整读要被看见，带 `.take(` 的不算。
    let sample = "stdout.read_to_end(&mut b).await?;\n\n\n\n\n\n\nx.take(CAP + 1).read_line(&mut s).await?;\n";
    assert_eq!(
        uncapped_stream_reads(sample).len(),
        1,
        "异步流整读的抽取器坏了，本条此刻是空转的"
    );
    let mut orphans = Vec::new();
    let root = repo_root();
    for sub in ["src/frontend/shell/src", "src/backend"] {
        for (path, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let prod = guard_core::production_code(&src);
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for line in uncapped_stream_reads(&prod) {
                orphans.push(format!("  {rel}:{line}"));
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "这些地方把一整条**流**读进内存，没有上限：\n{}\n\n\
             ★ 对端是**远端进程** —— 它坏掉、或者压根不是我们的后端，都会让\n\
             「无界读」变成「无界堆分配」。backend 侧为此栽过一次实测：\n\
             喂 512 MiB 无换行的流 ⇒ RSS 从 6 MiB 涨到 518 MiB\n\
             （见 `src/backend/stream/inbound/mod.rs` 头注）。\n\
             加上限：`.take(CAP + 1)` + 超了回错，形态见 `common/fs.rs`。",
        orphans.join("\n")
    );
}

/// 一份生产段里无界的异步流整读（1 起的行号）：读的动作紧邻 `.await`、前后几行没有 `.take(`。
///
/// 针按**读的动作**取（`read_to_end` / `read_to_string` / `read_line` / `read_until`），不按某一个方法名取。
/// ⚠ 失效模式：先 `let fut = …;` 再 `await` 就漏出人群；「附近有 `.take(`」是窗口启发式。
fn uncapped_stream_reads(prod: &str) -> Vec<usize> {
    let reads: Vec<String> = ["to_end", "to_string", "line", "until"]
        .iter()
        .map(|m| format!(".read_{m}("))
        .collect();
    let lines: Vec<&str> = prod.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !reads.iter().any(|r| l.contains(r.as_str())) {
            continue;
        }
        let lo = i.saturating_sub(4);
        let hi = (i + 3).min(lines.len());
        let window = lines[lo..hi].join("\n");
        if window.contains(".await") && !window.contains(".take(") {
            out.push(i + 1);
        }
    }
    out
}

/// ★ **「丢弃+带身份报告」那一档的行为对拍**。
///
/// # 为什么它需要一条**专属**判据
///
/// 隔壁 `a_cap_registered_as_hard_error_is_not_swallowed_at_its_call_site` 是
/// 「从常量被提到的那一行往下看 N 行找 marker」。那个形状对本档**不成立**：
/// `BACKEND_FRAME_LINE_CAP` 在三个地方被提到（有界读的判断处 · 措辞函数 · 消费点的
/// `warn!`），而报告只发生在**第三处** —— 按每处提及去要求 marker 会造出两条假红。
///
/// ⇒ 换个取样单位：**处置分支本身**。人群 = `stream_source/` 整个目录生产段里每一处
/// `CappedLine::TooLong` 的处置臂。要求：
/// ① 每一臂都得说点什么（至少 `warn!`）—— 定框 **E4**：静默失败要给身份；
/// ② **至少有一臂**把它抬到用户能看见的那一层（原位 `Gap`：主帧读那一臂交 `Ok(None)`）——
///    那正是本档与「跳过+说清」的分界（告日志 vs 告用户）。
///
/// ⚠ 失效模式如实登记：臂体窗口是 **12 行**的启发式。臂特别长 ⇒ 假红；
/// 紧邻的别的语句里恰好有 marker ⇒ 假绿。与隔壁那条同源取舍。
#[test]
fn the_drop_and_report_semantics_is_honoured_at_every_over_limit_arm() {
    let prod = crate::guard_support::stream_source_production();
    let lines: Vec<&str> = prod.lines().collect();
    let mut arms = 0usize;
    let mut silent = Vec::new();
    let mut reported = 0usize;
    for (i, l) in lines.iter().enumerate() {
        // ⚠ **必须同时要求 `=>`**〔本条首跑就红在这里〕：`CappedLine::TooLong` 既是
        // **构造**（有界读函数里 `return Ok(… CappedLine::TooLong(seen) …)`）也是
        // **模式**（处置臂 `Ok(CappedLine::TooLong(bytes)) => {`）。
        // 第一版只按名字取样，于是把有界读里那两处构造当成了「什么都没说的处置臂」——
        // **匹配单位（名字出现）比事实（这是一处处置）大**，本仓那一族的又一次。
        if !l.contains("CappedLine::TooLong") || !l.contains("=>") {
            continue;
        }
        arms += 1;
        const WINDOW: usize = 12;
        let body = lines[i..(i + WINDOW).min(lines.len())].join("\n");
        if !body.contains("warn!") {
            silent.push(format!("  stream_source/ 生产段第 {} 行", i + 1));
        }
        if body.contains("frame_tx.send(Ok(None))") {
            reported += 1; // 原位 `Gap` 那一路（主循环 `LineIntake::lost`）
        }
    }
    assert!(
        arms > 0,
        "一处超限处置臂都没找到 —— 抽取器坏了，本条此刻是空转的"
    );
    assert!(
        silent.is_empty(),
        "这些超限处置臂什么都没说：\n{}\n\n\
             ★ 丢一行**不说**就是静默失败，而 `ALLOWED_SEMANTICS` 刻意排除了那一种。\n\
             用户看到的会是「这条会话少了一行」且无从得知为什么。",
        silent.join("\n")
    );
    assert!(
        reported >= 1,
        "没有任何一处超限把话说到**用户**那一层（原位 `Gap`，`frame_tx.send(Ok(None))`）。\n\
             ★ 那是「丢弃+带身份报告」与「跳过+说清」的**唯一分界** —— \n\
             只写 `warn!` 的话本档就该改登记成「跳过+说清」，别占一个更强的名字。"
    );
}
