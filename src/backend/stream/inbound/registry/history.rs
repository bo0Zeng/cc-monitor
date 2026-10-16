//! 命令表 · 历史会话与它的读面：`history-*` · `session-fork` · `resolve` · `tasks-list` · `resync`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // **历史注解**（星标 / 改名 / 隐藏）的读写者换成本机常驻后端 ——
    //   「文件留在原处、同一路径，不迁移、一条不丢」：文件住家里（`<家>/history-metadata.json`，后端按家推），
    //   写是自家状态，经 `Own`（`history_annotations.rs`，读不懂就拒写、只改那一条、认不出的键原样留着）。三条都是阻塞档（读写一份小文件）。
    crate::wire::typed::blocking::<crate::history::history_annotations::HistoryAnnotateCmd>(
        "改一条历史注解",
    ),
    crate::wire::typed::blocking::<crate::history::history_annotations::HistoryForgetCmd>(
        "删一条历史注解",
    ),
    // 「这台上每条会话上次用哪个号起的」（起会话账号记录，会话所在那台各问一次）。读不懂 ⇒ `unreadable`。
    crate::wire::typed::blocking::<crate::control::launch_account::HistoryLastAccountsCmd>(
        "sid → 上次用哪个号起",
    ),
    // ── 只读查询面上线 —— 层 1 ＋ ──────────
    //
    // 🔴 **这八条此前全是一次性子命令**：monitor 每问一次就新拨一条 SSH（握手 ＋ 鉴权 ＋ exec），
    //   而这条长连接明明已经在那儿。账号那两条还被一个 10 秒的轮询按台数翻倍。
    //   ⇒ 登记上来，monitor 改走已有的 `inbound_client`，那些逐次拨号与轮询一起删。
    //
    // 🔴 **处理器住顶层 `read_face`，与 `files/` 同一个理由**：入方向不许出现 `observe::`
    //   （`inbound_never_reaches_into_the_observe_layer`），而查询本体住 `observe/`。
    //   `read_face` 只做换壳 —— 查询本体住 `observe/`，`out` 是内存。
    //   每条有一个派生的 CLI 面（`--history-read` …），已进 `lib.rs::SUBCOMMANDS`（不进就静默进流模式）。
    //
    // ⚠ 全在 `Run::Blocking`：它们都做文件 I/O（`history-search` 扫全库）。代价同 `files-*`：
    //   `cancel` 命中时回 `not_cancellable`（不撒谎）。
    // **历史页的平铺清单**（`history_list.rs`）：跨项目一次出成品 —— 每行的状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名。
    //   远端那一份由界面经已开着的长连接问那台常驻 `raw`（那台热缓存）、带着交进来（`listing`），本进程记着给敲字用；并注解、筛、排都在这台。
    //   这台不替界面去问那台（原先经 `remote_ask` 在那台起一次性进程冷扫）。
    crate::wire::typed::asynchronous::<crate::history::history_list::HistoryListCmd>(
        "历史页的平铺会话清单",
    ),
    // 全文搜索那两条：入参 · 回包 · 错误码都是类型（`faces/search_face.rs`）。搜索扫全库 ⇒ 阻塞档；合并是纯计算 ⇒ 异步档（同 `ping`）。
    crate::wire::typed::blocking::<crate::faces::search_face::HistorySearch>("全文搜索"),
    crate::wire::typed::asynchronous::<crate::faces::search_face::HistorySearchMerge>(
        "把各台的搜索结果合成一份",
    ),
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryRunCmd>("一个子运行的记录"),
    // resume 之前问「这条会话的记录还在不在」。同族同档（一次目录枚举 ⇒ 阻塞档）、
    // 同一个只读宿主。**只收 sid**（找文件那一步与分叉 / 删会话同一份 `agents::find_session_file`）。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryRecordCmd>(
        "这条会话的记录还在不在",
    ),
    // 按**行号**取回（没接骨架的会话丢掉的正文从这里要回来）。同族同档、同一个只读宿主。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryLinesCmd>("按行号取回一段"),
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryReadCmd>(
        "按字节分页读一份会话",
    ),
    // 按字节分页读、出记录行（`read_face.rs` 那一臂 ＋ `observe/record_page.rs`）。同族同档、同一个只读宿主。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryPageCmd>(
        "按字节分页读，出记录行",
    ),
    // 骨架索引与大纲清单上帧面（此前它们在远端走逐次拨号 —— `STILL_DIALED` 那两行）。
    // 同族同档（同步文件 I/O ⇒ 阻塞档）、同一个只读宿主（`read_face::answer`）。
    // 会话内查找上帧面（此前走逐次拨号 —— `STILL_DIALED` 那一行）。同族同档。
    // 一轮的摘要（`read_face.rs` 那一臂 ＋ `observe/turns.rs`）。同族同档、同一个只读宿主；`from` 是某一轮的 `at`。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryTurnsCmd>("一轮的摘要"),
    crate::wire::typed::blocking::<crate::faces::history_face::SessionsNeedsCmd>(
        "这台上需手动的会话清单",
    ),
    // 各台需手动的清单合成一个先后（纯计算 ⇒ 异步档，同 `history-search-merge`）：界面逐台问 `sessions-needs`，本机后端排。
    crate::wire::typed::asynchronous::<crate::faces::history_face::NeedsMergeCmd>(
        "把各台的需手动清单合成一个先后",
    ),
    // 答批准 / 提问 / 批计划（`control/asks/`）：会话级答题 hook 经 `needs-ask` 挂一条、阻塞等；任一出口经 `needs-answer` 答，第一个答案落下。
    //   挂着的请求住常驻进程里 ⇒ 两条都只在帧面（`needs-ask` 要本连接的号 ⇒ 硬臂；`needs-answer` 在 `stream::cli::STREAM_ONLY`）。
    crate::wire::typed::frame::<crate::control::asks::NeedsAsk>(
        "答题 hook 挂一条待答请求、等人答（hook 客户端 `--answer-hook` 用；本连接断了 ⇒ 撤）",
    ),
    // 锁一下登记表、交给 hook 那一头的通道：不碰盘、不起进程、不等 ⇒ 普通异步档（没有 await 点，也就没什么可撤的）。
    crate::wire::typed::asynchronous::<crate::control::asks::NeedsAnswerCmd>(
        "答一条挂着的待答请求（`needs.ask`）：第一个答案落下，其余回 `already_answered`",
    ),
    // 会话事实（`observe/facts_query.rs`）。`prior` 是调用方上一次拿到的应答里的 `{end, rev}`（续传令牌，那台续算表 `observe::facts_cache`）；带 `view` 时这两格照样在（`views::TOKENS`）。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryFactsCmd>("会话事实"),
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryBranchCmd>("主线外清单"),
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryFindCmd>("会话内查找"),
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryIndexCmd>("会话骨架索引"),
    crate::wire::typed::blocking::<crate::faces::history_face::UserInputsCmd>("「人说过的话」清单"),
    // 尾段那张图 ＋ 手上那一份（`held`）续不续得上 ＋ 第一页正文，一问交回（续传的判定只在这里）。
    crate::wire::typed::blocking::<crate::faces::history_face::HistoryTailCmd>(
        "一份会话的尾段：手上那一份怎么接，连第一页正文",
    ),
    // 分叉：本体 `control/fork_write.rs::run_inner`（
    //   读 → 适配层的分叉变换（`agents::build_branch_records`）→ `O_EXCL` 新建）。本机远端同一条长连接；读整份 jsonl ⇒ 阻塞档。
    //   回复多一格 `launch`：分叉之后起要的三格事实（`control/fork_launch.rs`，宿主 `faces/fork_face.rs` 收齐）。
    crate::wire::typed::blocking::<crate::faces::fork_face::SessionFork>(
        "从某条消息处分叉出一个新会话",
    ),
    // 手动对齐：整机（或 `sid` 只对一个会话）重跑起步那套对齐，回差异。阻塞档：等每份 watcher 做完。
    crate::wire::typed::blocking::<crate::stream::resync_face::ResyncCmd>(
        "手动对齐：重发这条流上的会话状态",
    ),
    // ── 功能侧只读查询 —— 远端会话的任务列表（`parity_ledger` `session.tasks`）──
    //
    // 🔴 此前只有 monitor 直读**本机** `tasks/<sid>/` 那一条路，远端 tab 永远拿不到任务。
    //   本机后端与远端后端是同一个二进制 ⇒ 读法搬到这里，monitor 按 origin 问（本机也走这里）。
    // ⚠ 宿主是 `feature_face`，**不是** `read_face`：monitor 侧有一条两向判据数的正是
    //   「交给 `read_face::answer` 的 == `C1` 那八条」，本族不在其中（理由全文在 `feature_face` 头注）。
    // ⚠ 阻塞档：读一个目录 ＋ 每个任务文件各一次。`cancel` 命中回 `not_cancellable`（不撒谎）。
    crate::wire::typed::blocking::<crate::faces::feature_face::TasksListCmd>("一个会话的任务列表"),
    // U6b-3：第一条**真业务命令**：按 `ResumeSpec` 推恢复命令（本体 `control/resolve_query.rs`）。CLI 面照派生规则有 `--resolve`。
    crate::wire::typed::asynchronous::<crate::control::resolve_query::ResolveCmd>(
        "按 `ResumeSpec` 推出恢复命令 `CommandPlan`",
    ),
];
