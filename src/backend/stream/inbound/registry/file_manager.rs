//! 命令表 · 文件管理：`files-*`（读 · 写 · 上传的提交 · 解压 · 删历史会话）。

use crate::stream::inbound::spec::{CommandSpec, Run};

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由本门递给文件管理写面 —— 写面自己一家 agent 的布局都不认（`files/module_boundary_guard.rs` 围栏那一类为零）。
pub(in crate::stream::inbound) const SESSION_PORT: crate::control::files_write::SessionPort =
    crate::control::files_write::SessionPort {
        locate: crate::agents::locate_session_for_delete,
        is_record: crate::agents::is_session_record,
    };

pub(super) const SPECS: &[CommandSpec] = &[
    // ── 〔步 `24f` 第二刀 09-20〕`files-read` 这一族上线 ────────────────────────────
    //
    // 🔴 **处理器不住 `control/`，这是本仓第一条** —— 而且它必须不住那里：
    //   `control/` 的定义是「**会改变世界**」（`§1.1` 第二条线），而这一族整族纯读
    //   （边界①，`readonly_guard` 4259 行一行没动）；
    //   而读面 `observe/` 又被入方向头注那条硬约束挡着（`inbound` 不许出现 `observe::`，
    //   `inbound_structure_guards::inbound_never_reaches_into_the_observe_layer` 在钉）。
    //   ⇒ 它住顶层 `files/`（`lib.rs` 一条 `pub mod`，`readonly_guard::BACKEND_CORE_MODULES`
    //   里也是一条独立登记）。入方向头注那句「每条命令的处理器属于 `control/`」
    //   由此收窄成「**不在入方向里实现**」——那才是它真正保的东西。
    //
    // 🔴 **四条的 `run` 长得一模一样，那是刻意的**：命令名从 `r.cmd` 来，
    //   而 `r.cmd` 正是 `lookup()` 用来选中这条 spec 的那个串
    //   （CLI 面同理，`cli_control` 拿 `spec.name` 填它）⇒ 「登记的名字」与
    //   「真正被调的能力」**在类型上是同一个值**，抄错一条能力名这件事不可表示。
    //   翻译（`-` → `.`）只有 `files::answer_wire` 一处，理由整段在那个函数的头注。
    //
    // ⚠ 四条全在 `Run::Blocking`：`files::answer` 是**同步**函数，前两条真的做文件系统
    //   I/O，`files-find` 在 64 万条量纲上的现打外推是 20–50 ms——
    //   那是不该占住 worker 的时长；而把「哪条够快可以走 `Run::Async`」拆成两档，
    //   等于给同一个同步入口记两份账。⇒ 一族一档。
    //   代价如实写：它们因此**取消不掉**，`cancel` 命中时回 `not_cancellable`（不撒谎）。
    // ── 〔步 `24f` 第三刀 09-21〕裁出来的第五、第六条 ──────────────
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
    // ── 〔波 5 ㈠〕 **第 2 步**：把那份零消费者的写原语接上 ──
    //
    // 🔴 **这一条是本族第一条会往盘上写的命令，而它不花用户那句「允许」**：
    //   `control/files_write.rs` 自 09-19 起就在 `readonly_guard` 的写白名单上，
    //   射程逐字是「`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录」
    //   ⇒ 接上它**没有放宽任何一层判据**（`readonly_guard` 这一拍一行没动）。
    //   它此前的形状与 `files.ls` 同一形：**能力在那儿，没人接**（那份文件头注逐字
    //   「它今天没有调用方」）。本条就是那个调用方。
    //
    // 🔴 **处理器住 `control/`，与 `files-read` 那六条刻意不同层**：`control/` 的定义是
    //   「**会改变世界**」（`§1.1` 第二条线）—— 这一条真的改变世界，所以它回到了那一层；
    //   而 `files/` 那一族整族纯读（边界①），**一个字节都不许被这一条带脏**。
    //   ⇒ 两面分家：`files-*` 这个线上前缀底下从此有两族，
    //   由 `inbound_structure_guards` 那两条**互不相交**的相等断言各钉一族。
    //
    // ⚠ **CLI 面是自动来的，不是选的**：`cli_control::cli_exposed` = 非 `Run::Builtin`
    //   ⇒ 这一条同拍上了 `lib.rs::SUBCOMMANDS`（不加就当未知 flag、静默进流模式）。
    //   ⇒ 「入口窄」这件事不靠命令面，靠 `readonly_guard` 第三层那条
    //   「**谁引用得到 `control/files_write`**」。理由整段在那个模块的命令面那一节。
    CommandSpec {
        name: "files-create",
        doc_anchor: Some("#### `files-create`"),
        codes: &["bad_args", "bad_path", "exists", "io_failed", "refused"],
        fields: &["bytes", "content", "path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔波 5 ㈡〕 **第 3 步**：改动既有数据的那五条 ──────────
    //
    // 🔴 **这五条才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
    //   处理器同住 `control/files_write.rs`（`readonly_guard` 第三层唯一登记的模块），
    //   而本文件是那一层登记的**唯一一扇门** —— 后端生产树里别处引用那个模块 ⇒ 红。
    //   ⚠ 本段五条与上面 `files-create` 的 `run` 逐字同形（名字从 `r.cmd` 来）。
    // ⚠ 全在阻塞档：同步文件系统 I/O（外加围栏那几次 `canonicalize`），开跑之后打不断
    //   ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
    CommandSpec {
        name: "files-mkdir",
        doc_anchor: Some("#### `files-mkdir`"),
        codes: &["bad_args", "bad_path", "exists", "io_failed", "refused"],
        fields: &["path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-rename",
        doc_anchor: Some("#### `files-rename`"),
        codes: &["bad_args", "bad_path", "exists", "io_failed", "refused"],
        fields: &["from", "path", "root", "to"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-delete",
        doc_anchor: Some("#### `files-delete`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        // `recursive`（入）· `removed`（出）：显式才删整棵树，逐条目过围栏。
        // `expect`（入）：给了 ⇒ 盘上逐字节等于它才删一份普通文件，否则 `stale`。
        // `limit`（入）· `remaining`（出）：递归删一趟至多删几条、还剩几条（调用方接着发）。
        fields: &[
            "expect",
            "limit",
            "path",
            "recursive",
            "rel",
            "remaining",
            "removed",
            "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-chmod",
        doc_anchor: Some("#### `files-chmod`"),
        // `no_unix_mode`：这个平台没有 unix 权限位（target 轴从这一格现推 Windows 那一格）。
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "no_unix_mode",
            "refused",
        ],
        fields: &["before", "mode", "path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 写面第七条：同根内复制。与上面五条同住一个模块、
    //   同一扇门、同档（同步文件 I/O，取消不掉）。它**不给第三层添动词**：由 `O_EXCL` 新建 ＋
    //   换名 ＋ 删自己刚建的那一份拼出来（理由住 `control/files_write.rs::copy_entry`）。
    CommandSpec {
        name: "files-copy",
        doc_anchor: Some("#### `files-copy`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        // `recursive`（入）· `files` / `dirs`（出）：显式才复制目录。
        fields: &[
            "bytes",
            "dirs",
            "files",
            "from",
            "links",
            "overwrite",
            "path",
            "recursive",
            "root",
            "to",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 解压：处理器住
    //   `control/files_extract.rs`（第三层第四个登记的模块），本文件照旧是那一层唯一的门。阻塞档（同步读包 ＋ 落盘）。
    CommandSpec {
        name: "files-extract",
        doc_anchor: Some("#### `files-extract`"),
        codes: &[
            "bad_args",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
            "unsupported",
        ],
        fields: &[
            "bytes", "dirs", "files", "fresh", "links", "path", "rel", "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_extract::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 建一条链接（`files_extract.rs::land_link` 那一个动词，FILES2 已在写面闭集里）：链接那条路径过根底下的解析，
    //   目标文本原样（同 `cp -P`）。阻塞档（一次 `symlink`）。
    CommandSpec {
        name: "files-link",
        doc_anchor: Some("#### `files-link`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        fields: &["path", "rel", "root", "target"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_extract::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-write-text",
        doc_anchor: Some("#### `files-write-text`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes", "content", "expect", "path", "rel", "root", "sha256",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 用户文件的读改写 ＋ 删历史会话 ─────────────────────────
    //
    // 🔴 只有后端的文件管理部分写**用户的文件**，本机也算 ⇒ monitor 进程
    //   不再直接写用户文件；本机与远端都经这三条（`call(origin, …)`，同一条路）。处理器同住
    //   `control/files_write.rs`（第三层），本文件照旧是那一层唯一的门。阻塞档（同步文件 I/O）。
    //   `files-delete-session` 只收 sid（理由住那个模块的 `delete_session`）。
    // 上一版说它是「会话文件围栏**唯一的例外**」—— FN1 之后文件管理写面已不设会话文件围栏，
    //   这一条「只许删会话形状那一份」的限制是它自己的，不是谁的例外。
    CommandSpec {
        name: "files-peek",
        doc_anchor: Some("#### `files-peek`"),
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "not_text",
            "refused",
            "too_large",
        ],
        fields: &["exists", "path", "rel", "root", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-put",
        doc_anchor: Some("#### `files-put`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "backup", "bytes", "changed", "content", "created", "expect", "parents", "path", "rel",
            "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-delete-session",
        doc_anchor: Some("#### `files-delete-session`"),
        codes: &["bad_args", "io_failed", "refused"],
        fields: &["path", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ──：上传的**提交** ──────────────────────
    //
    // 🔴 SFTP 缩成只做传输之后，上传只写暂存区（`~/.cc-monitor/staging/<key>.part`）；
    //   把它挪进用户目标的**那一下**在这里 —— 用户逐字「现在只允许后端的文件管理部分写文件」。
    //   处理器住 `control/files_commit.rs`（`readonly_guard` 第三层第二个登记的模块），
    //   本文件照旧是那一层唯一的门。阻塞档：同步文件系统 I/O（围栏的 `canonicalize` ＋ 改名）。
    // ── 存盘装不进一条请求行时：逐块进暂存区 ＋ 读回拼起来原地覆盖 ──────────
    //   同住 `control/files_commit.rs`（第三层第二个模块），阻塞档理由同上一条。
    CommandSpec {
        name: "files-stage-chunk",
        doc_anchor: Some("#### `files-stage-chunk`"),
        codes: &["bad_args", "io_failed", "refused"],
        fields: &["bytes", "content", "key", "seq"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-commit-text",
        doc_anchor: Some("#### `files-commit-text`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes", "chunks", "expect", "key", "path", "rel", "root", "sha256",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-commit-upload",
        doc_anchor: Some("#### `files-commit-upload`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes",
            "chunks",
            "expect",
            "key",
            "overwrite",
            "path",
            "rel",
            "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-browse",
        doc_anchor: Some("#### `files-browse`"),
        codes: &["bad_args", "bad_path"],
        // +`watching` · `watch_failed` · `watch_error`（进程里那一个监听器跟上名单）。
        fields: &[
            "added",
            "browse_watch_cap",
            "dirs",
            "rejected",
            "removed",
            "watch_error",
            "watch_failed",
            "watching",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-rebuild",
        doc_anchor: Some("#### `files-index-rebuild`"),
        // `already_rebuilding`：非阻塞互斥抢不到那个位。`no_home`：没给根而家目录说不出。
        codes: &["already_rebuilding", "bad_path", "no_home", "unreadable"],
        fields: &[
            "entries",
            "path",
            "resident_bytes",
            "skipped_mounts",
            "truncated",
            "unreadable_dirs",
            "unreadable_paths",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **按内容搜**：`{path, needle, ignore_ascii_case?, limit?}` ⇒ 命中的那几份（第一处命中那一行 ＋ 命中几行）
    //   ＋ 走了多少。**可撤** ⇒ 异步档：走那一趟放进阻塞线程池，`cancel` 丢掉这个 future 时守卫置取消位、那一趟随即停
    //   （`files/mod.rs::answer_grep_cancellable`）。纯读。
    CommandSpec {
        name: "files-grep",
        doc_anchor: Some("#### `files-grep`"),
        codes: &["bad_args", "bad_path", "unreadable"],
        fields: &[
            "bytes",
            "files",
            "hits",
            "ignore_ascii_case",
            "limit",
            "links",
            "needle",
            "path",
            "skipped_binary",
            "skipped_large",
            "skipped_mounts",
            "stopped",
            "truncated",
            "unreadable",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::files::answer_grep_cancellable(r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "files-find",
        doc_anchor: Some("#### `files-find`"),
        codes: &["bad_args", "bad_path", "bad_query", "superseded"],
        fields: &[
            "cover_root",
            "desc",
            "hits",
            "index_age_secs",
            "index_missing",
            "index_root",
            "limit",
            "offset",
            "out_of_index",
            "query",
            "scanned",
            "scope",
            "seq",
            "sort",
            "stale",
            "start",
            "stream",
            "total_hits",
            "truncated",
            "under",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-status",
        doc_anchor: Some("#### `files-index-status`"),
        codes: &[],
        fields: &[
            "age_secs",
            "browse_watch_cap",
            "browse_watches",
            "cold_first_build_secs",
            "entries",
            "index_missing",
            "resident_bytes",
            "rewalk_interval_secs",
            "skipped_mounts",
            "stale",
            "truncated",
            "unreadable_dirs",
            "unreadable_paths",
        ],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-ls",
        doc_anchor: Some("#### `files-ls`"),
        codes: &["bad_path", "denied", "not_dir", "not_found", "unreadable"],
        fields: &[
            "entries",
            "kind",
            "limit",
            "link_dir",
            "link_to",
            "mtime_secs",
            "path",
            "size",
            "total",
            "truncated",
            "unreadable",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-stat",
        doc_anchor: Some("#### `files-stat`"),
        codes: &["bad_path", "unreadable"],
        // +`mode`（能力 `files.stat` 同拍加的那一格；非 unix 缺席）· `owner` · `link_target`（文件窗口「属性」）。
        fields: &[
            "kind",
            "link_target",
            "mode",
            "mtime_secs",
            "owner",
            "path",
            "readonly",
            "size",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ──：窗口换走通道的那两问 ────────────────
    //
    // 🔴 **同一族（`files-read`）的第七、第八条，整族照旧纯读**：编辑器读一份文本 ·
    //   开窗前「那台机器的 home 在哪」。此前窗口为这两问各拨一条 SFTP（
    //   那张欠账表），现在经通道问后端 —— 窗口进程够后端**只剩通道**这一条路。
    // ⚠ `run` 与同族那六条逐字同形（名字从 `r.cmd` 来）；同在 `Run::Blocking`、同样取消不掉。
    CommandSpec {
        name: "files-read-text",
        doc_anchor: Some("#### `files-read-text`"),
        codes: &[
            "bad_args",
            "bad_path",
            "not_text",
            "too_large",
            "unreadable",
        ],
        fields: &["bytes", "max_bytes", "path", "sha256", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 读族第十条：按字节寻址分块读回（非 UTF-8 名的下载）。同族同形、同在阻塞档。
    CommandSpec {
        name: "files-read-chunk",
        doc_anchor: Some("#### `files-read-chunk`"),
        codes: &["bad_args", "bad_path", "not_text", "unreadable"],
        fields: &["content", "eof", "len", "offset", "path", "size"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 读族第九条：算目录大小。与同族那几条逐字同形、同在阻塞档。
    CommandSpec {
        name: "files-size",
        doc_anchor: Some("#### `files-size`"),
        codes: &["bad_path", "unreadable"],
        fields: &[
            "bytes",
            "dirs",
            "files",
            "links",
            "other",
            "path",
            "skipped_mounts",
            "unreadable_dirs",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-home",
        doc_anchor: Some("#### `files-home`"),
        codes: &["no_home"],
        fields: &["path"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
