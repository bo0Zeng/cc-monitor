//! 命令表 · 文件管理：`files-*`（读 · 写 · 上传的提交 · 解压 · 删历史会话）。

use crate::wire::spec::CommandSpec;
use crate::wire::typed;

pub(super) const SPECS: &[CommandSpec] = &[
    // ── `files-read` 这一族上线 ────────────────────────────
    //
    // 🔴 **处理器不住 `control/`，这是本仓第一条** —— 而且它必须不住那里：
    //   `control/` 的定义是「**会改变世界**」（`§1.1` 第二条线），而这一族整族纯读
    //   （边界①，`readonly_guard` 4259 行一行没动）；
    //   而读面 `observe/` 又被入方向头注那条硬约束挡着（`inbound` 不许出现 `observe::`，
    //   `inbound_structure_guards::inbound_never_reaches_into_the_observe_layer` 在钉）。
    //   ⇒ 它住顶层 `files/`（`lib.rs` 一条 `pub mod`）。入方向头注那句「每条命令的处理器属于 `control/`」
    //   由此收窄成「**不在入方向里实现**」——那才是它真正保的东西。
    //
    // 🔴 **四条的 `run` 长得一模一样，那是刻意的**：命令名从 `r.cmd` 来，
    //   而 `r.cmd` 正是 `lookup()` 用来选中这条 spec 的那个串
    //   （CLI 面同理，`stream::cli` 拿 `spec.name` 填它）⇒ 「登记的名字」与
    //   「真正被调的能力」**在类型上是同一个值**，抄错一条能力名这件事不可表示。
    //   翻译（`-` → `.`）只有 `files::answer_wire` 一处，理由整段在那个函数的头注。
    //
    // ⚠ 四条全在 `Run::Blocking`：`files::answer` 是**同步**函数，前两条真的做文件系统
    //   I/O，`files-find` 在 64 万条量纲上的现打外推是 20–50 ms——
    //   那是不该占住 worker 的时长；而把「哪条够快可以走 `Run::Async`」拆成两档，
    //   等于给同一个同步入口记两份账。⇒ 一族一档。
    //   代价如实写：它们因此**取消不掉**，`cancel` 命中时回 `not_cancellable`（不撒谎）。
    // ── 裁出来的第五、第六条 ──────────────
    //
    // 🔴 **它们补的是那两段「机制」的线上面** —— 那张三段表
    //   （建索引 / 保鲜 / 查询）里，第二刀只把「查」那一段接上了线。
    //   在这两条之前，`files::index::rebuild_once` 与 `files::browse_watch::set_browsing`
    //   **零生产调用方** ⇒ 真机上 `files-find` 恒回 `index_missing: true`。
    //
    // 🔴 **节拍仍然不归后端**，一个字没松：这两条与抓屏那条原语**同一形** ——
    //   「**只做一次**……『隔多久再做一次』留在调用方」（`K37`：后端只给机制，不给偏好），
    //   `no_timer_guard` 在后端侧零容忍地钉着。
    //   ⚠ **别把这一刀读成「那个缺口填上了」**：调用方不发这条命令，
    //   索引照旧永远不会自己变新，而「调用方到底发不发」后端这棵树的判据钉不住。
    //
    // ⚠ 两条的 `run` 与同族那四条逐字同形，理由同上一段（名字从 `r.cmd` 来 ⇒
    //   「登记的名字」与「真被调的能力」在类型上是同一个值）。
    // ── 把那份零消费者的写原语接上 ──
    //
    // 🔴 **这一条是本族第一条会往盘上写的命令，而它不花用户那句「允许」**：
    //   `control/files_write.rs` 09-19 那时在 `readonly_guard` 的写白名单上（今天写经 `Face`），
    //   射程逐字是「`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录」
    //   ⇒ 接上它**没有放宽任何一层判据**（`readonly_guard` 一行没动）。
    //   它此前的形状与 `files.ls` 同一形：**能力在那儿，没人接**（那份文件头注逐字
    //   「它今天没有调用方」）。本条就是那个调用方。
    //
    // 🔴 **处理器住 `control/`，与 `files-read` 那六条刻意不同层**：`control/` 的定义是
    //   「**会改变世界**」（`§1.1` 第二条线）—— 这一条真的改变世界，所以它回到了那一层；
    //   而 `files/` 那一族整族纯读（边界①），**一个字节都不许被这一条带脏**。
    //   ⇒ 两面分家：`files-*` 这个线上前缀底下从此有两族，
    //   由 `inbound_structure_guards` 那两条**互不相交**的相等断言各钉一族。
    //
    // ⚠ **CLI 面是自动来的，不是选的**：`stream::cli::cli_exposed` = 非 `Run::Builtin`
    //   ⇒ 这一条同拍上了 `lib.rs::SUBCOMMANDS`（不加就当未知 flag、静默进流模式）。
    //   ⇒ 「入口窄」这件事不靠命令面，靠写句柄：本族用 typed 的 `Facing` 那一档，分派那一层才递 `Face`，
    //   别族拿不到它。理由整段在 `control/files_write` 头注「写怎么被挡住」那一节。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesCreate>(
        "在文件管理目标根底下新建一份此前不存在的文件",
    ),
    // ── 改动既有数据的那五条 ──────────
    //
    // 🔴 **这五条才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
    //   处理器同住 `control/files_write.rs`，写都经分派递进来的 `Face`（`Facing` 那一档只许本族用）。
    //   ⚠ 本段五条与上面 `files-create` 的 `run` 逐字同形（名字从 `r.cmd` 来）。
    // ⚠ 全在阻塞档：同步文件系统 I/O（外加围栏那几次 `canonicalize`），开跑之后打不断
    //   ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesMkdir>("新建一个目录"),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesRename>("改名 / 同根内移动"),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesDelete>(
        "删一组（文件 · 空目录 · 显式递归的整棵）",
    ),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesChmod>(
        "一组改成同一个 unix 权限位",
    ),
    // 写面第七条：同根内复制。与上面五条同住一个模块、
    //   同一扇门、同档（同步文件 I/O，取消不掉）。它**不给 `Face` 添动词**：由 `O_EXCL` 新建 ＋
    //   换名 ＋ 删自己刚建的那一份拼出来（理由住 `control/files_write.rs::copy_entry`）。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesCopy>("同根内复制一组"),
    // 撤一组（复制 · 删除 · 改权限那三条带票的那一组）：只立旗，不碰盘。
    typed::asynchronous::<crate::stream::inbound::files_write_face::FilesBatchStop>(
        "撤一组复制 · 删除 · 改权限",
    ),
    // 解压：处理器住
    //   `control/files_extract.rs`，写经 `Face`。阻塞档（同步读包 ＋ 落盘）。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesExtract>("解压到一个新目录"),
    // 建一条链接（`files_extract.rs::land_link` 那一个动词，FILES2 已在写面闭集里）：链接那条路径过根底下的解析，
    //   目标文本原样（同 `cp -P`）。阻塞档（一次 `symlink`）。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesLink>("建一条符号链接"),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesWriteText>(
        "覆盖写一份已经在的普通文件",
    ),
    // ── 用户文件的读改写 ＋ 删历史会话 ─────────────────────────
    //
    // 🔴 只有后端的文件管理部分写**用户的文件**，本机也算 ⇒ monitor 进程
    //   不再直接写用户文件；本机与远端都经这三条（`call(origin, …)`，同一条路）。处理器同住
    //   `control/files_write.rs`，写经 `Face`。阻塞档（同步文件 I/O）。
    //   `files-delete-session` 只收 sid（理由住那个模块的 `delete_session`）。
    // 上一版说它是「会话文件围栏**唯一的例外**」—— FN1 之后文件管理写面已不设会话文件围栏，
    //   这一条「只许删会话形状那一份」的限制是它自己的，不是谁的例外。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesPeek>("读改写的读那一半"),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesPut>("整份替换一份文本文件"),
    typed::blocking::<crate::stream::inbound::files_session_face::FilesDeleteSession>(
        "删一份历史会话",
    ),
    // ── 上传的**提交** ──────────────────────
    //
    // 🔴 SFTP 缩成只做传输之后，上传只写暂存区（`~/.cc-monitor/staging/<key>.part`）；
    //   把它挪进用户目标的**那一下**在这里 —— 用户逐字「现在只允许后端的文件管理部分写文件」。
    //   处理器住 `control/files_commit.rs`，写经 `Face`。阻塞档：同步文件系统 I/O（路径解析的 `canonicalize` ＋ 改名）。
    // ── 存盘装不进一条请求行时：逐块进暂存区 ＋ 读回拼起来原地覆盖 ──────────
    //   同住 `control/files_commit.rs`，阻塞档理由同上一条。
    typed::blocking::<crate::stream::inbound::files_write_face::FilesStageChunk>(
        "存盘的一块进暂存区",
    ),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesCommitText>(
        "按块读回、拼起来、覆盖写",
    ),
    typed::blocking::<crate::stream::inbound::files_write_face::FilesCommitUpload>(
        "把暂存区里一份传完的上传件挪进目标",
    ),
    typed::blocking::<crate::stream::inbound::files_face::FilesBrowse>(
        "告诉后端「用户现在在看哪几个目录」",
    ),
    typed::blocking::<crate::stream::inbound::files_face::FilesIndexRebuild>(
        "重建常驻文件名索引：走一遍，做完返回",
    ),
    // **按内容搜**：`{path, needle, ignore_ascii_case?, limit?}` ⇒ 命中的那几份（第一处命中那一行 ＋ 命中几行）
    //   ＋ 走了多少。**可撤** ⇒ 异步档：走那一趟放进阻塞线程池，`cancel` 丢掉这个 future 时守卫置取消位、那一趟随即停
    //   （`files/mod.rs::answer_grep_cancellable`）。纯读。
    typed::asynchronous::<crate::stream::inbound::files_face::FilesGrep>("在一个目录底下按内容搜"),
    typed::blocking::<crate::stream::inbound::files_face::FilesFind>("在常驻索引里查"),
    typed::blocking::<crate::stream::inbound::files_face::FilesIndexStatus>(
        "索引的新鲜度 / 条目数 / 常驻字节",
    ),
    typed::blocking::<crate::stream::inbound::files_face::FilesLs>("列一个目录的直接子项"),
    typed::blocking::<crate::stream::inbound::files_face::FilesStat>("一个路径的元数据"),
    // ── 窗口换走通道的那两问 ────────────────
    //
    // 🔴 **同一族（`files-read`）的第七、第八条，整族照旧纯读**：编辑器读一份文本 ·
    //   开窗前「那台机器的 home 在哪」。此前窗口为这两问各拨一条 SFTP（
    //   那张欠账表），现在经通道问后端 —— 窗口进程够后端**只剩通道**这一条路。
    // ⚠ `run` 与同族那六条逐字同形（名字从 `r.cmd` 来）；同在 `Run::Blocking`、同样取消不掉。
    typed::blocking::<crate::stream::inbound::files_face::FilesReadText>("读一份文本进编辑器"),
    // 读族第十条：按字节寻址分块读回（非 UTF-8 名的下载）。同族同形、同在阻塞档。
    typed::blocking::<crate::stream::inbound::files_face::FilesReadChunk>("按字节寻址分块读回"),
    // 读族第九条：算目录大小。与同族那几条逐字同形、同在阻塞档。
    typed::blocking::<crate::stream::inbound::files_face::FilesSize>("算一个目录有多大"),
    typed::blocking::<crate::stream::inbound::files_face::FilesHome>("后端这个用户的 home"),
];

// 文件窗口读的那几条回包的线上字节钉在夹具里（换形前后逐字节比）。
#[cfg(test)]
#[path = "../../../../../tests/backend/stream/file_manager_wire_tests.rs"]
mod wire_tests;
