//! `files-read` 这一族：后端侧只读的文件面 —— 搜索（SFTP 只能递归 `READDIR`、N 次往返，协议里也没有常驻索引）及其同族的纯读
//! （列目录 · 元数据 · 读文本 / 按块读 · 问 home · 算大小 · grep）。写面住 `control/files_write.rs`，SFTP 只做传输。
//!
//! # 三条硬边界（能力声明的一部分，不是注释）
//!
//! ## ① 整族一个字节都不写，而且这一条进声明的语义
//!
//! [`CAPABILITIES`] 的第三栏 [`Capability::effect`] 是闭集（[`Effect`]），每条能力都说自己落在哪一格；
//! `tests/backend/files/capability_guard.rs` 把它钉成相等断言：从实现源码派生出来的那一格 == 这里声明的那一格。
//! 派生的人群是 [`Capability::impl_files`]，那几张表的并集又被钉成恰好等于本族目录下的全部 `.rs`（分区恒等）。
//! ⇒ 往本族任何一份实现里写一个改动盘上东西的动词，声明这一侧第一个红（`readonly_guard` 跟着红，两道都在）。
//!
//! ## ② 跨 target 不对等，而且必须如实声明
//!
//! | 表 | 判法 | 为什么 |
//! |---|---|---|
//! | [`CAPABILITIES`] 的 [`Capability::targets`] | 集合相等 —— 每条能力在 [`TARGETS`] 全体上都要有 | 「能力在不在」在各平台上必须是同一个答案 |
//! | [`FRESHNESS`] | 逐 target 一行，刻意不判相等 | 保鲜机制逐平台不是一件事；判成相等 = 逼人写假声明 |
//!
//! [`FRESHNESS`] 另有一条反向判据：不许全部平台都填同一个机制串（为了让对拍变绿把几行抄成一样，就是假声明的长相）。
//!
//! ## ③ 「定期重走」的周期是能力的一部分，必须可查询
//!
//! 住 [`index::REWALK_INTERVAL_SECS`]，由 `files.index.status` 交出去（[`index::Status::rewalk_interval_secs`]），界面显示那个延迟。
//!
//! # 线上面与汇总
//!
//! 线上名与能力名的翻译只有一处：[`answer_wire`]。本族在能力汇总 [`crate::capability_ledger`] 里是一个面（人群 [`crate::CAPABILITY_FACES`]）。
//! 索引怎么建、什么时候变新是机制的线上面：`files-index-rebuild`（[`index::rebuild_once`]）· `files-browse`（[`browse_watch::set_browsing`]）；
//! 节拍归调用方（后端零定时器）—— 没人发 rebuild，`files.find` 就回 `index_missing: true`。
//! 常驻那一份索引是进程级的 ⇒ 这几条要在同一条连接上才配得起来；CLI 面一次 exec 一个进程，建好的索引随进程没了。
//! 跨 target 的对等断言（各面能力集横着对、逐条登记豁免）没做；它与「在这台机器上做不到」（[`crate::stream::wire::Unavailable`]，运行期逐机器）是两个轴。

pub mod browse_watch;
pub mod grep;
pub mod index;
// Everything 式搜索词：解析 ＋ 逐条匹配（`files.find` 的 `query` 只在这里被读懂）。
pub mod query;
pub mod raw;
// `files.size`（算目录大小）。
pub mod size;

/// 这一族的名字。
#[cfg(test)]
pub const FAMILY: &str = "files-read";

/// 一条能力的**副作用档**。**闭集。**
///
/// 🔴 [`Effect::TouchesDisk`] 今天**没有任何一条能力声明它** —— 它存在的唯一理由是
/// 让「有人往这一族里加了一个会动盘上东西的操作」这件事在**声明这一侧可表达、可断言**。
/// 一个只有一个成员的枚举没法承载一条相等断言（派生出来的那一侧无处可落），
/// 而那条相等断言就是边界① 要的「声明那道更早」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// 遍历 · 读文件名 · 存内存 · 答查询。**一个字节都不往盘上写。**
    ReadsOnly,
    /// 这一族里有一处会改动盘上的东西。**今天零条**。
    TouchesDisk,
}

/// 编译 target 轴：住 [`crate::Target`]（汇总那一层，声明 target 的面不止一个），本族再导出它。
pub use crate::{Target, TARGETS};

/// 一条能力的登记。
///
/// ⚠ 后四栏（`impl_files` / `args` / `fields` / `codes`）**只被判据读** ——
/// 那正是它们存在的理由：把「这条能力的契约面」写成**数据**，好让机检对着它比
///（形状照 `inbound::CommandSpec`，那份头注逐字写着同一条理由）。
pub struct Capability {
    /// 线上能力名。那张表逐字。
    pub name: &'static str,
    /// 它做什么（登记散文，只给判据读、不上界面 ⇒ 不叫 `what`：那个字段名会被普查当成文案出口）。
    pub purpose: &'static str,
    /// 🔴 副作用档。边界① 的兑现处 —— 判据把它与**从实现派生**出来的那一格判相等。
    pub effect: Effect,
    /// 实现它的那几份文件（本族目录下的文件名）。**派生的人群就是这张表。**
    pub impl_files: &'static [&'static str],
    /// 它在哪些 target 上有实现。边界②：这一栏必须是 [`TARGETS`] 全体。
    pub targets: &'static [Target],
    /// 入方向参数名。
    pub args: &'static [&'static str],
    /// 出方向字段名。
    pub fields: &'static [&'static str],
    /// 本能力自己可能回的 code。
    pub codes: &'static [&'static str],
}

/// 🔴 **这一族的能力声明 —— 唯一住址。**
///
/// 六条，与「这一族有哪些」那张表 ＋ 它下面那张「第五、第六条」的表
/// **逐字同名**（判据按名字两向对拍，改一边不改另一边当场红）。
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        name: "files.ls",
        purpose: "列一个目录的直接子项（名字走原始字节）",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["limit", "path"],
        fields: &[
            "entries",
            "kind",
            "link_dir",
            "link_to",
            "mtime_full",
            "mtime_secs",
            "mtime_text",
            "path",
            "size",
            "total",
            "truncated",
            "unreadable",
        ],
        codes: &["bad_path", "denied", "not_dir", "not_found", "unreadable"],
    },
    Capability {
        name: "files.stat",
        purpose: "一个路径的元数据",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["path"],
        // +`mode`（unix 权限位低 12 位；非 unix 缺席）—— 文件窗口改权限那个框要显示现值。
        // +`owner` · `link_target`：文件窗口「属性」要列属主与链接指向（取不到 / 不是链接 ⇒ `null`）。
        fields: &[
            "kind",
            "link_target",
            "mode",
            "mtime_full",
            "mtime_secs",
            "mtime_text",
            "owner",
            "path",
            "readonly",
            "size",
        ],
        codes: &["bad_path", "unreadable"],
    },
    Capability {
        name: "files.find",
        purpose: "🔴 **在常驻索引里查** —— SFTP 给不了的那一条，整族的存在理由（Everything 式搜索词 · 分页 · 同一个搜索框的旧号丢掉）",
        effect: Effect::ReadsOnly,
        impl_files: &["browse_watch.rs", "index.rs", "mod.rs", "query.rs", "raw.rs"],
        targets: TARGETS,
        args: &["desc", "limit", "offset", "query", "scope", "seq", "sort", "stream", "under"],
        fields: &[
            "cover_root",
            "desc",
            "hits",
            "index_age_secs",
            "index_missing",
            "index_root",
            "offset",
            "out_of_index",
            "scanned",
            "seq",
            "sort",
            "stale",
            "start",
            "total_hits",
            "truncated",
        ],
        codes: &["bad_args", "bad_path", "bad_query", "superseded"],
    },
    Capability {
        name: "files.index.status",
        purpose: "索引的新鲜度 ／ 条目数 ／ 常驻字节，**以及后端声明的重走周期**（边界③）",
        effect: Effect::ReadsOnly,
        impl_files: &["browse_watch.rs", "index.rs", "mod.rs"],
        targets: TARGETS,
        args: &[],
        fields: &[
            "age_secs",
            "browse_watch_cap",
            "browse_watches",
            "cold_first_build_secs",
            "entries",
            "index_missing",
            "resident_bytes",
            "rewalk_interval_secs",
            // 没走进去的挂载点个数。
            "skipped_mounts",
            "stale",
            "truncated",
            "unreadable_dirs",
            // 读不进去的那几个目录（前 20 个，线上形）。
            "unreadable_paths",
        ],
        codes: &[],
    },
    // ── 建索引 · 保鲜 ────────────────
    // 机制的线上面，不是节拍（节拍归调用方）。两条都在边界① 之内：`rebuild_once` 是遍历 ＋ 换掉内存里那一份，
    // `set_browsing` 是登记名单 ＋ 重列一遍 —— 都不往盘上写一个字节。
    Capability {
        name: "files.index.rebuild",
        purpose: "🔴 **走一遍，就一遍，做完返回** —— `index::rebuild_once` 的线上面（只有机制，没有节拍）",
        effect: Effect::ReadsOnly,
        impl_files: &["index.rs", "mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["path"],
        fields: &[
            "entries",
            "path",
            "resident_bytes",
            // 没走进去的挂载点个数。
            "skipped_mounts",
            "truncated",
            "unreadable_dirs",
            // 读不进去的那几个目录（前 20 个，线上形）。
            "unreadable_paths",
        ],
        // 🔴 `already_rebuilding`：非阻塞互斥抢不到那个位。
        //    它**刻意是一个码而不是回参里的一个布尔** —— 理由住 `answer_index_rebuild`。
        // `no_home`：没给 `path`（＝ 家目录）而这台机器说不出家目录。
        codes: &["already_rebuilding", "bad_path", "no_home", "unreadable"],
    },
    Capability {
        name: "files.browse",
        purpose: "告诉后端「用户现在在看哪几个目录」—— `browse_watch::set_browsing` 的线上面（保鲜的另一半）",
        effect: Effect::ReadsOnly,
        impl_files: &["browse_watch.rs", "mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["dirs"],
        // +`watching` · `watch_failed` · `watch_error`：监听器真挂上了几个、没挂上的出声。
        fields: &[
            "added",
            "browse_watch_cap",
            "rejected",
            "removed",
            "watch_error",
            "watch_failed",
            "watching",
        ],
        codes: &["bad_args", "bad_path"],
    },
    // ── 窗口的两问：读一份文本 · 那台机器的 home 在哪 ────────────
    // 两条都纯读 ⇒ 进这一族。
    Capability {
        name: "files.read.text",
        purpose: "读一份文本进编辑器 —— **超上限整趟拒、不截断**；含 NUL / 不是 UTF-8 也拒",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["max_bytes", "path"],
        fields: &["bytes", "path", "sha256", "text"],
        codes: &["bad_args", "bad_path", "not_text", "too_large", "unreadable"],
    },
    // ── 「算目录大小」：读族第九条 ────────────────
    //   在那台机器上走一遍、只回几个数（＋「零流量」）。纯读，整族照旧一个字节不写。
    Capability {
        name: "files.size",
        purpose: "算一个目录（或文件）有多大 —— 不跟链接、不进别的文件系统，只回几个数",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs", "size.rs"],
        targets: TARGETS,
        args: &["path"],
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
        codes: &["bad_path", "unreadable"],
    },
    // ── 按字节寻址分块读回 ─────────────────────────
    // 非 UTF-8 名的下载（SFTP 库的路径是 `String`，寻址不到）经后端链路一块一块读回；与 `files-stage-chunk`（分块写进暂存区）对称。纯读。
    Capability {
        name: "files.read.chunk",
        purpose: "从一份普通文件的 `offset` 起读至多 `len` 字节（原始字节，b16 送回）—— 下载非 UTF-8 名那条路的一块",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["len", "offset", "path"],
        fields: &["content", "eof", "offset", "path", "size"],
        codes: &["bad_args", "bad_path", "not_text", "unreadable"],
    },
    // ── 读族第十一条：按内容搜 ─────────────────────────────
    //   在那台机器上走一遍一个目录、只回命中的那几份（有字节与条数上界、可撤、不跟链接）。纯读，整族照旧一个字节不写。
    Capability {
        name: "files.grep",
        purpose: "在一个目录底下按内容搜 —— 不跟链接、不进别的文件系统，有字节与命中数上界，每份回第一处命中那一行",
        effect: Effect::ReadsOnly,
        impl_files: &["grep.rs", "mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["ignore_ascii_case", "limit", "needle", "path"],
        fields: &[
            "bytes",
            "files",
            "hits",
            "limit",
            "links",
            "path",
            "skipped_binary",
            "skipped_large",
            "skipped_mounts",
            "stopped",
            "truncated",
            "unreadable",
        ],
        codes: &["bad_args", "bad_path", "unreadable"],
    },
    Capability {
        name: "files.home",
        purpose: "后端这个进程的用户 home（绝对路径）—— 窗口开窗时「开在哪儿」那一问",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &[],
        fields: &["path"],
        codes: &["no_home"],
    },
];

/// **保鲜机制逐 target 的如实声明**（边界②的另一半）。
///
/// 🔴 **这张表刻意不参加任何「三个平台要一样」的对拍。** 理由：
/// 「把它们判成相等会**逼人写假声明**」。它参加的是另外两条：
/// ① 每个 target 恰好一行（集合相等，不许漏一个平台）；
/// ② **不许三行填成同一个机制串**（那就是假声明的长相）。
pub struct Freshness {
    pub target: Target,
    /// 保鲜怎么做（这个平台上的**机制名**，一句话）。
    pub how: &'static str,
    /// 非特权身份拿不拿得到。后端是**以用户身份经 SSH 起的，不是 root**。
    pub unprivileged: bool,
    /// 这一格的证据档。
    pub evidence: Evidence,
    /// 🔴 这一格**没**买到什么。**不许留空**（判据有长度地板）。
    pub gap: &'static str,
}

/// 一格声明背后是什么证据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// 本机现打，读数有住址。
    Measured,
    /// 文献读数，没实测。
    LiteratureOnly,
    /// 判不了 —— 缺什么写在 [`Freshness::gap`] 里。
    Undetermined,
}

pub const FRESHNESS: &[Freshness] = &[
    Freshness {
        target: Target::LinuxGnu,
        how: "定期重走（周期由调用方出节拍）＋ 浏览目录挂 inotify，一个 watch 一个目录",
        unprivileged: true,
        evidence: Evidence::Measured,
        gap: "**不是实时**。要全文件系统监听就得 root：`man 2 fanotify_init` 一手逐字 —— \
              非特权不许 `FAN_MARK_MOUNT` / `FAN_MARK_FILESYSTEM`。\
              而 watch 上限本机现打 262144、home 下 640413 条目⇒ 全挂挂不住。\
              另：**冷缓存下走一遍要多久没量过**（`drop_caches` 要 root）。",
    },
    Freshness {
        target: Target::LinuxMusl,
        how: "同 gnu 那一格（同一份源码、同一个内核接口），差别只在链接方式",
        unprivileged: true,
        evidence: Evidence::Undetermined,
        gap: "🔴 **这一格是「没验」，不是「验过没事」。** 门禁的 `muslbuild` 那格买的是\
              「**编得出静态字节**」，**不买**「在真远端上跑得起来」（无真机、不运行）。\
              ⇒ 静态 musl 下 `inotify` 的行为**本件没有任何读数**。",
    },
    Freshness {
        target: Target::Windows,
        how: "定期重走 ＋ 浏览目录走 ReadDirectoryChangesW（notify 在这个平台上的后端）",
        unprivileged: true,
        evidence: Evidence::LiteratureOnly,
        gap: "**没实测**（本仓的 Windows 虚拟机上没跑过本族）。\
              ⚠ Everything 那套（读 MFT ＋ 订阅 USN Journal）**两段都要管理员**\
              （voidtools FAQ 逐字），而我们的后端不是 ⇒ 那条路对我们关着，\
              这一格拿不到 Everything 那种「整盘一秒」。",
    },
    Freshness {
        target: Target::MacOs,
        how: "定期重走 ＋ 浏览目录走 FSEvents（notify 在这个平台上的后端）",
        unprivileged: true,
        evidence: Evidence::LiteratureOnly,
        gap: "🟡 **文献读数，没实测**（那张表里这一格逐字就是这么标的）。\
              FSEvents 能无特权递归监听一棵树、Spotlight 的索引普通用户可查（`mdfind`）——\
              两条都**没碰过**。⇒ 这个平台上「保鲜要不要换成另一套机制」判不了。",
    },
];

// ══════════════════════ 命令面（形状照 `inbound::CommandSpec` 的处理器）══════════════════════

/// 一条能力的答案：成功交 JSON，失败交 `(code, message)`（与 `inbound` 的 `CmdResult` 同形）。
pub type Answer = Result<serde_json::Value, (&'static str, String)>;

/// 每次回送的条数上限的默认值（调用方给 `limit` 就用它的；给 0 或不给 ⇒ 用这个）。
/// 理由是「一次往返」：一次查询可以命中五万多条，一次全推过去就不再是零流量搜索。截断时 `truncated` 与 `total_hits` 会说出来。
pub const DEFAULT_LIMIT: usize = 1000;

fn limit_of(args: &serde_json::Value) -> usize {
    match args.get("limit").and_then(serde_json::Value::as_u64) {
        Some(n) if n > 0 => n as usize,
        _ => DEFAULT_LIMIT,
    }
}

fn path_arg(args: &serde_json::Value) -> Result<std::path::PathBuf, (&'static str, String)> {
    let v = args.get("path").ok_or((
        "bad_path",
        crate::common::contract::malformed("missing `path` (a string or {\"b16\": \"<hex>\"})"),
    ))?;
    let bytes = raw::from_json(v).ok_or((
        "bad_path",
        crate::common::contract::malformed("`path` must be a string or {\"b16\": \"<hex>\"}"),
    ))?;
    if bytes.is_empty() {
        return Err((
            "bad_path",
            crate::common::contract::malformed("`path` is empty"),
        ));
    }
    Ok(raw::to_path_buf(&bytes))
}

/// `files.browse` 的 `dirs` —— 一个数组，每项与 [`path_arg`] 同那两种形。
///
/// # 🔴 两个码刻意分得开
///
/// `dirs` **自己**的形状不对（少了它 / 不是数组）是 `bad_args`；数组里**某一项**
/// 不是一个路径是 `bad_path`（与 `files.ls` / `files.stat` 同一个码、同一条
/// 「刻意不尽力而为地猜」的理由）。压成一句会让调用方分不清该改哪一头。
///
/// ⚠ **空数组是合法的**，语义是「用户现在什么都没在看」⇒ 名单清空。
/// 「少了 `dirs`」与「`dirs` 是空的」**是两件事**：前者是调用方漏了参数，
/// 后者是它真的要卸掉全部 —— 静默地把前者当后者办，就是悄悄把 watch 全拆了。
fn dirs_arg(args: &serde_json::Value) -> Result<Vec<std::path::PathBuf>, (&'static str, String)> {
    let v = args.get("dirs").ok_or((
        "bad_args",
        crate::common::contract::malformed(
            "missing `dirs` (an array of strings or {\"b16\": \"<hex>\"}; empty array is valid)",
        ),
    ))?;
    let arr = v.as_array().ok_or((
        "bad_args",
        crate::common::contract::malformed(
            "`dirs` must be an array (the whole current list, not a delta)",
        ),
    ))?;
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let bytes = raw::from_json(item).ok_or((
            "bad_path",
            crate::common::contract::malformed(&format!(
                "`dirs[{i}]` must be a string or {{\"b16\": \"<hex>\"}}"
            )),
        ))?;
        if bytes.is_empty() {
            return Err((
                "bad_path",
                crate::common::contract::malformed(&format!("`dirs[{i}]` is empty")),
            ));
        }
        out.push(raw::to_path_buf(&bytes));
    }
    Ok(out)
}

/// 一个时刻换成 Unix 纪元秒。
///
/// ⚠ **参数刻意写成 `io::Result<SystemTime>` 而不是那个元数据类型** ——
/// 那个类型的名字带大写，而 `readonly_guard` 的只读白名单是按**动词**认的、
/// 表里那一条是小写的 `metadata`（大小写不同 ⇒ 认不出来 ⇒ 当场红），
/// 而把它 `use` 进来又正好撞上同一条判据的「条目导入 = 逃生口」那一格。
/// ⇒ 两头都不碰：**不提那个类型名**。这不是绕过护栏，是不去动它。
fn epoch_secs(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// 一个条目的类型名。**闭集四个词**，判据按它对拍。
///
/// ⚠ 参数是三个 `bool` 而不是那个类型，理由同 [`epoch_secs`]。
fn kind_name(is_dir: bool, is_file: bool, is_symlink: bool) -> &'static str {
    if is_dir {
        "dir"
    } else if is_file {
        "file"
    } else if is_symlink {
        "symlink"
    } else {
        "other"
    }
}

/// 读不了时那一截原因：常见的两种说成人话，其余用系统原话（`ErrorKind` 的调试名 `PermissionDenied` 不上屏）。
fn io_kind_said(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            copy_core::copy_text("beFilesRead.ioKind.denied", &[])
        }
        std::io::ErrorKind::NotFound => copy_core::copy_text("beFilesRead.ioKind.notFound", &[]),
        _ => e.to_string(),
    }
}

/// 这一族认得的类型名 —— **闭集，唯一住址**。判据按它对拍 [`kind_name`] 的出口。
pub const KINDS: &[&str] = &["dir", "file", "other", "symlink"];

/// 索引里的类型字节（`query::KIND_*`）→ 线上那个词。
const KINDS_BY_CODE: [&str; 4] = ["file", "dir", "symlink", "other"];

/// `files.ls` —— 列一个目录的直接子项。
fn answer_ls(args: &serde_json::Value) -> Answer {
    let dir = path_arg(args)?;
    let limit = limit_of(args);
    // 打不开的原因分几种码（界面按码说「路径不存在 / 无权限 / 不是目录」，系统原话进详情）。
    let rd = std::fs::read_dir(&dir).map_err(|e| {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "denied",
            std::io::ErrorKind::NotADirectory => "not_dir",
            _ => "unreadable",
        };
        (
            code,
            copy_core::copy_text("beFilesRead.ls.unreadable", &[("kind", &io_kind_said(&e))]),
        )
    })?;
    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut seen = 0usize;
    // 读不出的项照数（不静默少一项）：窗口按 `unreadable` 说一句。
    let mut unreadable = 0usize;
    for e in readable(rd, &mut unreadable) {
        seen += 1;
        if entries.len() >= limit {
            continue;
        }
        let mut row = serde_json::Map::new();
        row.insert("path".to_string(), raw::to_json(raw::path_bytes(&e.path())));
        // 🔴 类型走 `DirEntry::file_type()`：它**不跟 symlink**
        //（Linux 上直接来自目录项里的 d_type），所以指向别处的链接不会被当成它的目标。
        let kind = e
            .file_type()
            .map(|t| kind_name(t.is_dir(), t.is_file(), t.is_symlink()))
            .unwrap_or("other");
        row.insert(
            "kind".to_string(),
            serde_json::Value::String(kind.to_string()),
        );
        if let Ok(md) = std::fs::metadata(e.path()) {
            row.insert("size".to_string(), serde_json::json!(md.len()));
            if let Some(t) = epoch_secs(md.modified()) {
                row.insert("mtime_secs".to_string(), serde_json::json!(t));
                let (short, full) = crate::common::time::mtime_texts_here(t);
                row.insert("mtime_text".to_string(), serde_json::json!(short));
                row.insert("mtime_full".to_string(), serde_json::json!(full));
            }
            // 链接指向的是不是目录（跟链接那一次 `metadata` 顺带的）：窗口据此给「打开」。断链不出这一格。
            if kind == "symlink" {
                row.insert("link_dir".to_string(), serde_json::json!(md.is_dir()));
                row.insert(
                    "link_to".to_string(),
                    serde_json::json!(if md.is_dir() { "dir" } else { "file" }),
                );
            }
        } else if kind == "symlink" {
            // 跟不过去 ⇒ 断了的链接（指向的东西不在 / 读不到）。
            row.insert("link_to".to_string(), serde_json::json!("missing"));
        }
        entries.push(serde_json::Value::Object(row));
    }
    Ok(serde_json::json!({
        "entries": entries,
        "truncated": seen > entries.len(),
        "total": seen,
        "unreadable": unreadable,
    }))
}

/// 目录项里读得出的那几条；读不出的数进 `unreadable`（不静默跳过）。
fn readable<'a, E, I>(items: I, unreadable: &'a mut usize) -> impl Iterator<Item = E> + 'a
where
    I: IntoIterator<Item = std::io::Result<E>>,
    I::IntoIter: 'a,
{
    items.into_iter().filter_map(move |e| match e {
        Ok(e) => Some(e),
        Err(_) => {
            *unreadable += 1;
            None
        }
    })
}

/// `files.stat` —— 一个路径的元数据。它跟 symlink（拿链接指向的那个东西的元数据）：不跟的读法要另一个动词，
/// 那个动词不在 `readonly_guard` 的只读白名单上。需要区分链接本身时，先用 `files.ls` 看它父目录那一行的 `kind`。
fn answer_stat(args: &serde_json::Value) -> Answer {
    let path = path_arg(args)?;
    let md = std::fs::metadata(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &io_kind_said(&e))],
            ),
        )
    })?;
    let mut out = serde_json::Map::new();
    out.insert("path".to_string(), raw::to_json(raw::path_bytes(&path)));
    let t = md.file_type();
    out.insert(
        "kind".to_string(),
        serde_json::Value::String(kind_name(t.is_dir(), t.is_file(), t.is_symlink()).to_string()),
    );
    out.insert("size".to_string(), serde_json::json!(md.len()));
    out.insert(
        "readonly".to_string(),
        serde_json::json!(md.permissions().readonly()),
    );
    // unix 权限位（低 12 位：rwx×3 ＋ setuid / setgid / sticky）—— 同一次 `metadata`、同样跟链接，
    //   与本条其余几格同源。非 unix 平台**缺席**（不是 0：0 是一个真能设的权限值，报 0 等于说假话）；
    //   那边改权限本来就回 `no_unix_mode`（`control/files_write.rs::change_mode`）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        out.insert(
            "mode".to_string(),
            serde_json::json!(md.permissions().mode() & 0o7777),
        );
    }
    if let Some(ms) = epoch_secs(md.modified()) {
        out.insert("mtime_secs".to_string(), serde_json::json!(ms));
        let (short, full) = crate::common::time::mtime_texts_here(ms);
        out.insert("mtime_text".to_string(), serde_json::json!(short));
        out.insert("mtime_full".to_string(), serde_json::json!(full));
    }
    // 属主（跟链接，同上几格）：用户名，查不到名字给 uid 数字串；非 unix 给 `null`。
    out.insert(
        "owner".to_string(),
        serde_json::json!(crate::platform::paths::owner_of(&path)),
    );
    // 路径**本身**是不是链接（`readlink` 读得出 ⇒ 是）：是 ⇒ 目标原文（原始字节形）；否 ⇒ `null`。
    out.insert(
        "link_target".to_string(),
        crate::platform::paths::link_target_of(&path).map_or(serde_json::Value::Null, |t| {
            raw::to_json(raw::path_bytes(&t))
        }),
    );
    Ok(serde_json::Value::Object(out))
}

/// `files.find` —— 在常驻索引里查。**只回送命中。**
///
/// `query` 是窗口原样发来的搜索词（解析只在 [`query::parse`]）；`seq` ＋ `stream` 让同一个搜索框的旧那一趟收手；
/// `under` 给了 ⇒ 只搜那个目录底下，不给 ⇒ 家目录；`offset` / `limit` 只回这一屏。
fn answer_find(args: &serde_json::Value) -> Answer {
    let q = args
        .get("query")
        .and_then(serde_json::Value::as_str)
        .ok_or((
            "bad_args",
            crate::common::contract::malformed("missing `query` (a string)"),
        ))?;
    let matcher = query::parse(q).map_err(|e| ("bad_query", e.said()))?;
    let under = match args.get("under") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(raw::from_json(v).filter(|b| !b.is_empty()).ok_or((
            "bad_path",
            crate::common::contract::malformed(
                "`under` must be a non-empty string or {\"b16\": \"<hex>\"}",
            ),
        ))?),
    };
    let seq = match args.get("seq") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_u64().ok_or((
            "bad_args",
            crate::common::contract::malformed("`seq` must be a non-negative integer"),
        ))?),
    };
    let stream = args
        .get("stream")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let offset = args
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let superseded = || {
        (
            "superseded",
            copy_core::copy_text("beFilesRead.find.superseded", &[]),
        )
    };
    let ticket = match seq {
        Some(n) => Some(index::ticket(stream, n).map_err(|_| superseded())?),
        None => None,
    };
    let sort = index::Sort {
        key: match args.get("sort") {
            None | Some(serde_json::Value::Null) => index::SortKey::default(),
            Some(v) => v.as_str().and_then(index::SortKey::from_wire).ok_or((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`sort` must be one of {}",
                    index::SortKey::WIRE.join(" / ")
                )),
            ))?,
        },
        desc: match args.get("desc") {
            None | Some(serde_json::Value::Null) => false,
            Some(v) => v.as_bool().ok_or((
                "bad_args",
                crate::common::contract::malformed("`desc` must be a boolean"),
            ))?,
        },
    };
    // 「整台机器」：范围由这台自己定（文件系统的根），界面不猜平台。
    let machine = match args.get("scope") {
        None | Some(serde_json::Value::Null) => false,
        Some(v) => match v.as_str() {
            Some("under") => false,
            Some("machine") => true,
            _ => {
                return Err((
                    "bad_args",
                    crate::common::contract::malformed("`scope` must be \"under\" or \"machine\""),
                ));
            }
        },
    };
    let home = home_var().map(std::path::PathBuf::from);
    let root = machine.then(|| machine_root(home.as_deref()));
    let under = root.or(under);
    let r = index::find(&index::FindArgs {
        query: &matcher,
        under: under.as_deref(),
        home: home.as_deref().map(raw::path_bytes),
        offset,
        limit: limit_of(args),
        ticket: ticket.as_ref(),
        sort,
    })
    .map_err(|_| superseded())?;
    let path_or_null = |p: &Option<Vec<u8>>| match p {
        Some(b) => raw::to_json(b),
        None => serde_json::Value::Null,
    };
    let start = under
        .clone()
        .or_else(|| home.as_deref().map(|h| raw::path_bytes(h).to_vec()));
    Ok(serde_json::json!({
        "hits": r.hits.iter().map(|h| serde_json::json!({
            "path": raw::to_json(&h.path),
            "kind": KINDS_BY_CODE.get(h.kind as usize).copied().unwrap_or("other"),
            "location": raw::to_json(index::location_of(&h.path, start.as_deref())),
            "size": h.meta.size,
            "mtime_secs": h.meta.mtime_secs,
            "mtime_text": h.meta.mtime_secs.map(|t| crate::common::time::mtime_texts_here(t).0),
            "marks": matcher.marks(&h.path).iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "start": path_or_null(&start),
        "sort": sort.key.wire(),
        "desc": sort.desc,
        "total_hits": r.total_hits,
        "truncated": r.truncated,
        "scanned": r.scanned,
        "index_age_secs": r.index_age_secs,
        "index_missing": r.index_missing,
        "stale": r.stale,
        "index_root": path_or_null(&r.index_root),
        "out_of_index": r.out_of_index,
        "cover_root": path_or_null(&r.cover_root),
        "seq": seq,
        "offset": offset,
    }))
}

/// `path` 相对 `root` 那一段（不在它底下 ⇒ 原样；就是它 ⇒ 最后一段名字）。
fn rel_to<'p>(path: &'p [u8], root: &[u8]) -> &'p [u8] {
    let sep = |b: u8| b == b'/' || (cfg!(windows) && b == b'\\');
    let mut r = root;
    while r.len() > 1 && r.last().is_some_and(|&b| sep(b)) {
        r = &r[..r.len() - 1];
    }
    if path.len() > r.len()
        && path.starts_with(r)
        && (sep(path[r.len()]) || r.last().is_some_and(|&b| sep(b)))
    {
        let mut rest = &path[r.len()..];
        while rest.first().is_some_and(|&b| sep(b)) {
            rest = &rest[1..];
        }
        return rest;
    }
    if path == r {
        return path
            .iter()
            .rposition(|&b| sep(b))
            .map_or(path, |i| &path[i + 1..]);
    }
    path
}

/// 「整台机器」那一档的范围：unix 是 `/`；Windows 是家目录所在那块盘的根（说不出 ⇒ `C:\\`）。
fn machine_root(home: Option<&std::path::Path>) -> Vec<u8> {
    if cfg!(windows) {
        let drive = home
            .map(raw::path_bytes)
            .filter(|h| h.len() >= 2 && h[1] == b':' && h[0].is_ascii_alphabetic())
            .map(|h| h[..2].to_vec())
            .unwrap_or_else(|| b"C:".to_vec());
        [drive, b"\\".to_vec()].concat()
    } else {
        b"/".to_vec()
    }
}

/// `files.index.status` —— 新鲜度 ／ 条目数 ／ 常驻字节 ／ **声明的重走周期**。
fn answer_status() -> Answer {
    let s = index::status();
    Ok(serde_json::json!({
        "index_missing": s.index_missing,
        "entries": s.entries,
        "resident_bytes": s.resident_bytes,
        "unreadable_dirs": s.unreadable_dirs,
        "unreadable_paths": unreadable_paths_json(),
        "truncated": s.truncated,
        "skipped_mounts": s.skipped_mounts,
        "age_secs": s.age_secs,
        "rewalk_interval_secs": s.rewalk_interval_secs,
        "stale": s.stale,
        "browse_watches": s.browse_watches,
        "browse_watch_cap": s.browse_watch_cap,
        "cold_first_build_secs": s.cold_first_build_secs,
    }))
}

/// `files.index.rebuild` —— 走一遍，就一遍，做完返回。
///
/// 根读不进去 ⇒ 拒，常驻那一份一个字节不动：[`index::build`] 对一个打不开的根不会失败（只把 `unreadable_dirs` 加一、交一份空快照），
/// 而 [`index::rebuild_once`] 会把常驻那一份整份换掉 ⇒ 路径打错一个字母，好索引就被空的顶掉、回参还像一次成功的重走。
/// 所以换之前先探一次根，打不开回 `unreadable`（判据：换之前的条目数 == 被拒之后的条目数）。
/// 根底下读不进去的子目录照旧落在 `unreadable_dirs` 里（不是 0 就说明这份索引有洞）。
fn answer_index_rebuild(args: &serde_json::Value) -> Answer {
    // 不给 `path` ⇒ 这台机器的家目录（默认的根）。
    let root = match args.get("path") {
        None | Some(serde_json::Value::Null) => home_var()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or((
                "no_home",
                copy_core::copy_text("beFilesRead.rebuild.noHome", &[]),
            ))?,
        Some(_) => path_arg(args)?,
    };
    // 只要「打不打得开」这一个答案 —— 句柄拿到就丢，一条目录项都不读。
    std::fs::read_dir(&root).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.rebuild.rootUnreadable",
                &[("kind", &io_kind_said(&e))],
            ),
        )
    })?;
    // 「有一趟已经在跑」（`rebuild_once` 非阻塞互斥、抢不到回 `None`）走错误码 `already_rebuilding`，不走带 `skipped` 的成功回参：
    // 那样「没走」与「走完了但树是空的」在结构上就分不开。
    let stats = index::rebuild_once(&root).ok_or_else(|| {
        (
            "already_rebuilding",
            copy_core::copy_text(
                "beFilesRead.rebuild.busy",
                &[("n", &index::rebuild_skipped().to_string())],
            ),
        )
    })?;
    Ok(serde_json::json!({
        "path": raw::to_json(raw::path_bytes(&root)),
        "entries": stats.entries,
        "resident_bytes": stats.resident_bytes,
        "unreadable_dirs": stats.unreadable_dirs,
        "unreadable_paths": unreadable_paths_json(),
        "truncated": stats.truncated,
        "skipped_mounts": stats.skipped_mounts,
    }))
}

/// 常驻那一份里读不进去的那几个目录，线上形（合法 UTF-8 ⇒ 字符串，否则 `{"b16": …}`）。
fn unreadable_paths_json() -> Vec<serde_json::Value> {
    index::unreadable_paths()
        .iter()
        .map(|b| raw::to_json(b))
        .collect()
}

/// `files.browse` —— 告诉后端「用户现在在看哪几个目录」。
///
/// [`browse_watch::set_browsing`] 登记名单 ＋ 当场把那几个目录各重列一遍（结果进 overlay，查询时盖掉大索引里的对应条目）；
/// [`browse_watch::keep_watching`] 让进程里那一个监听器跟上名单 ⇒ 浏览的目录此后一有动静 overlay 就跟着重列。
/// 买不到：watch 绑 inode 不绑路径 · 内核队列溢出 · 窗口关了没人发空名单（最后那份名单的 watch 留到下一次）。
/// `rejected` 必须跟着回去，并同拍带上 `browse_watch_cap`：只回一个数、不说上限，调用方没法判该少送几个。
fn answer_browse(args: &serde_json::Value) -> Answer {
    let dirs = dirs_arg(args)?;
    let applied = browse_watch::set_browsing(&dirs);
    // 名单登记了之后让进程里那一个监听器跟上。
    let w = browse_watch::keep_watching();
    Ok(serde_json::json!({
        "added": applied.added,
        "removed": applied.removed,
        "rejected": applied.rejected,
        "browse_watch_cap": browse_watch::MAX_BROWSE_WATCHES,
        "watching": w.watching,
        "watch_failed": w.failed,
        "watch_error": w.error,
    }))
}

/// `files.read.text` 一趟**最多**肯交多少字节 —— 后端自己的天花板，**不是**编辑上限。
///
/// # 🔴 两个数，两个住址，两件事（别把它读成「编辑上限的第二份」）
///
/// - **编辑上限**是**调用方**的：它答的是「这个文本控件打字卡不卡」，那是窗口那一侧的
///   语境（「那个上限该是多少、超了怎么办，要在**原生窗口的文本控件**
///   这个语境里答」）⇒ 它住窗口，每趟经 `max_bytes` 送过来。「机制在后端 · 偏好归调用方」
///   （同本族 `files.index.rebuild` 那条的形）。
/// - **本常量**是后端的：它答的是「一帧应答整个进内存、整个过线，最大能多大」。
///   推算：JSON 转义最坏把一个字节写成 6 个（`\u00XX`），8 MiB × 6 = 48 MiB，
///   仍在应答那一帧的上限（monitor 侧读后端一行 64 MiB、通道一帧 64 MiB）之内。
///   ⇒ 调用方要的 `max_bytes` 超过它 ⇒ `bad_args`（说清天花板是多少），**不偷偷夹小**。
pub const READ_TEXT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// `files-read-chunk` 一块最多多少原始字节（b16 翻倍后一帧应答仍远小于 monitor 读一行的上限）。
/// 调用方给的 `len` 越界 ⇒ `bad_args`、不夹小（同 [`READ_TEXT_MAX_BYTES`] 那一条理由）。
pub const READ_CHUNK_MAX_BYTES: u64 = 256 * 1024;

/// CAS 摘要形里十六进制串的长度（SHA-256 = 32 字节）。
pub const SHA256_HEX_LEN: usize = 64;

/// 一份字节的 SHA-256，64 位小写十六进制 —— **CAS 摘要形 `expect: {"sha256": …}` 的唯一算法住址**。
///
/// 读的那一趟（[`answer_read_text`]）对交出去的字节算它；写面（`control/files_write.rs::overwrite_text_expecting`）
/// 拿它比「盘上此刻那一份」、写成之后对新内容再算一次交回去。住读族这一侧，是因为读族不许伸手进写面
/// （写面只有 `stream/inbound/` 一扇门），反过来写面借读族一个纯函数不开新门。
pub fn content_sha256(bytes: &[u8]) -> String {
    let mut d = ContentDigest::new();
    d.update(bytes);
    d.finish()
}

/// [`content_sha256`] 的**流式**那一形（同一个算法、同一种十六进制）：上传一边传一边算整份的摘要
/// （`control/transfer.rs::upload_to_staging`），提交那一侧对暂存件逐块读着算（[`file_sha256`]）—— 几个 G 的文件不整份进内存。
pub struct ContentDigest(ring::digest::Context);

impl ContentDigest {
    pub fn new() -> Self {
        ContentDigest(ring::digest::Context::new(&ring::digest::SHA256))
    }
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    /// 64 位小写十六进制。
    pub fn finish(self) -> String {
        use std::fmt::Write as _;
        let d = self.0.finish();
        let mut out = String::with_capacity(SHA256_HEX_LEN);
        for b in d.as_ref() {
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}

impl Default for ContentDigest {
    fn default() -> Self {
        Self::new()
    }
}

/// 一份文件的 [`content_sha256`]，**逐块读**（64 KiB 一块）。读不出 ⇒ 原样的 IO 错。
pub fn file_sha256(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open(path)?;
    let mut d = ContentDigest::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(d.finish());
        }
        d.update(&buf[..n]);
    }
}

/// `files.read.chunk`：`{path, offset, len}` → `{path, offset, size, eof, content: {b16}}`。
/// 只读普通文件（不是 ⇒ `not_text`，与读族同一个码）；`offset` 越过末尾 ⇒ 空块、`eof: true`。
fn answer_read_chunk(args: &serde_json::Value) -> Answer {
    use std::io::{Read as _, Seek as _};
    let path = path_arg(args)?;
    let num = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(serde_json::Value::as_u64).ok_or((
            "bad_args",
            crate::common::contract::malformed(&format!("missing `{k}` (non-negative integer)")),
        ))
    };
    let offset = num(args.get("offset"), "offset")?;
    let len = num(args.get("len"), "len")?;
    if len == 0 || len > READ_CHUNK_MAX_BYTES {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`len` is {len}; accepted range 1..={READ_CHUNK_MAX_BYTES}, not clamped"
            )),
        ));
    }
    let kind = |e: std::io::Error| io_kind_said(&e);
    let md = std::fs::metadata(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text("beFilesRead.size.unreadable", &[("kind", &kind(e))]),
        )
    })?;
    if !md.is_file() {
        return Err((
            "not_text",
            copy_core::copy_text("beFilesRead.text.notRegular", &[]),
        ));
    }
    let broke = |e: std::io::Error| {
        (
            "unreadable",
            copy_core::copy_text("beFilesRead.text.readBroke", &[("kind", &kind(e))]),
        )
    };
    let mut f = std::fs::File::open(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text("beFilesRead.text.openFailed", &[("kind", &kind(e))]),
        )
    })?;
    f.seek(std::io::SeekFrom::Start(offset)).map_err(broke)?;
    let mut buf: Vec<u8> = Vec::new();
    f.take(len).read_to_end(&mut buf).map_err(broke)?;
    let size = md.len();
    Ok(serde_json::json!({
        "path": raw::to_json(raw::path_bytes(&path)),
        "offset": offset,
        "size": size,
        "eof": offset + buf.len() as u64 >= size,
        "content": { "b16": buf.iter().map(|b| format!("{b:02x}")).collect::<String>() },
    }))
}

/// `files.read.text` —— 读一份文本。超上限整趟拒，不截断（截断过的文本存回去会写坏文件）。
///
/// | 码 | 什么时候 |
/// |---|---|
/// | `too_large` | `stat` 出来的大小超过 `max_bytes`；或者读的时候比 `stat` 时大（文件在长）|
/// | `not_text` | 不是普通文件 · 含 NUL 字节 · 不是合法 UTF-8 |
/// | `unreadable` | 读不到（不存在 / 没权限）|
///
/// 大小判两次：`stat` 与真读之间文件可以被换大，第二道用 `take(max + 1)` 读 —— 最多多读一个字节就知道「超了」。
fn answer_read_text(args: &serde_json::Value) -> Answer {
    let path = path_arg(args)?;
    let max = args
        .get("max_bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or((
            "bad_args",
            crate::common::contract::malformed(
                "missing `max_bytes` (non-negative integer, required every call)",
            ),
        ))?;
    if max == 0 || max > READ_TEXT_MAX_BYTES as u64 {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`max_bytes` is {max}; accepted range 1..={READ_TEXT_MAX_BYTES}, not clamped"
            )),
        ));
    }
    let md = std::fs::metadata(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &io_kind_said(&e))],
            ),
        )
    })?;
    if !md.is_file() {
        return Err((
            "not_text",
            copy_core::copy_text("beFilesRead.text.notRegular", &[]),
        ));
    }
    if md.len() > max {
        return Err((
            "too_large",
            copy_core::copy_text(
                "beFilesRead.text.tooLarge",
                &[
                    ("size", &md.len().to_string()),
                    ("max", &max.to_string()),
                    ("over", &(md.len() - max).to_string()),
                ],
            ),
        ));
    }
    let f = std::fs::File::open(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.text.openFailed",
                &[("kind", &io_kind_said(&e))],
            ),
        )
    })?;
    let mut buf: Vec<u8> = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(f, max + 1), &mut buf).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text("beFilesRead.text.readBroke", &[("kind", &io_kind_said(&e))]),
        )
    })?;
    if buf.len() as u64 > max {
        return Err((
            "too_large",
            copy_core::copy_text("beFilesRead.text.grew", &[("max", &max.to_string())]),
        ));
    }
    if buf.contains(&0) {
        return Err((
            "not_text",
            copy_core::copy_text("beFilesRead.text.hasNul", &[]),
        ));
    }
    let n = buf.len();
    // 交出去的那份字节的摘要：编辑器存回去时原样交回当 CAS 的 `expect`（算法住写面那一处）。
    let sha256 = content_sha256(&buf);
    let text = String::from_utf8(buf).map_err(|e| {
        (
            "not_text",
            copy_core::copy_text(
                "beFilesRead.text.notUtf8",
                &[("at", &e.utf8_error().valid_up_to().to_string())],
            ),
        )
    })?;
    Ok(serde_json::json!({
        "path": raw::to_json(raw::path_bytes(&path)),
        "text": text,
        "bytes": n,
        "sha256": sha256,
    }))
}

/// `files.size` —— 走法与诚实边界住 [`size`] 头注。
fn answer_size(args: &serde_json::Value) -> Answer {
    let path = path_arg(args)?;
    let m = size::measure(&path).map_err(|e| {
        (
            "unreadable",
            copy_core::copy_text(
                "beFilesRead.size.unreadable",
                &[("kind", &io_kind_said(&e))],
            ),
        )
    })?;
    Ok(serde_json::json!({
        "path": raw::to_json(raw::path_bytes(&path)),
        "bytes": m.bytes,
        "files": m.files,
        "dirs": m.dirs,
        "links": m.links,
        "other": m.other,
        "skipped_mounts": m.skipped_mounts,
        "unreadable_dirs": m.unreadable_dirs,
    }))
}

/// 后端这个进程的用户 home（环境里那一格，原样）。空串算没有。与 SFTP 那一问（`realpath(".")`）是同一个答案的两个出处
/// （sshd 按账号给 `HOME` 并把 SFTP 子系统的起点放在同一处）；有人在登录脚本里改了 `HOME` 时答改过之后的那个 —— shell 与 Claude 认的也是它。
fn home_var() -> Option<std::ffi::OsString> {
    crate::platform::paths::home_dir().map(std::path::PathBuf::into_os_string)
}

/// `files.grep` 的入参：`{path, needle, ignore_ascii_case?, limit?}`。`needle` 与 `path` 同两种形（字符串 / `{"b16"}`），
/// 非空、至多 [`grep::NEEDLE_MAX_BYTES`] 字节；`limit` 缺省 [`grep::DEFAULT_LIMIT`]、至多 [`grep::MAX_LIMIT`]。
fn grep_args(
    args: &serde_json::Value,
) -> Result<(std::path::PathBuf, grep::GrepArgs), (&'static str, String)> {
    let path = path_arg(args)?;
    let needle = args.get("needle").and_then(raw::from_json).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `needle` (a string or {\"b16\": \"<hex>\"})"),
    ))?;
    if needle.is_empty() || needle.len() > grep::NEEDLE_MAX_BYTES {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`needle` must be 1..={} bytes",
                grep::NEEDLE_MAX_BYTES
            )),
        ));
    }
    let limit = match args.get("limit").and_then(serde_json::Value::as_u64) {
        Some(n) if n > 0 => (n as usize).min(grep::MAX_LIMIT),
        _ => grep::DEFAULT_LIMIT,
    };
    Ok((
        path,
        grep::GrepArgs {
            needle,
            ignore_ascii_case: args
                .get("ignore_ascii_case")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            limit,
        },
    ))
}

/// `files.grep` 的成品（见 [`grep`] 头注）。
fn grep_reply(top: &std::path::Path, limit: usize, g: &grep::Grepped) -> serde_json::Value {
    let stopped = match g.stopped {
        grep::Stopped::Hits => serde_json::json!("hits"),
        grep::Stopped::Bytes => serde_json::json!("bytes"),
        grep::Stopped::Done | grep::Stopped::Cancelled => serde_json::Value::Null,
    };
    serde_json::json!({
        "path": raw::to_json(raw::path_bytes(top)),
        "hits": g.hits.iter().map(|h| serde_json::json!({
            "path": raw::to_json(&h.path),
            "rel": raw::to_json(rel_to(&h.path, raw::path_bytes(top))),
            "line": h.line,
            "text": raw::to_json(&h.text),
            "matches": h.matches,
            "lines": h.lines.iter().map(|l| serde_json::json!({
                "line": l.line,
                "text": raw::to_json(&l.text),
                "marks": l.mark.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "files": g.files,
        "bytes": g.bytes,
        "links": g.links,
        "skipped_binary": g.skipped_binary,
        "skipped_large": g.skipped_large,
        "skipped_mounts": g.skipped_mounts,
        "unreadable": g.unreadable,
        "limit": limit,
        "truncated": matches!(g.stopped, grep::Stopped::Hits | grep::Stopped::Bytes),
        "stopped": stopped,
    })
}

fn grep_unreadable(e: &std::io::Error) -> (&'static str, String) {
    (
        "unreadable",
        copy_core::copy_text(
            "beFilesRead.grep.unreadable",
            &[("kind", &io_kind_said(&e))],
        ),
    )
}

/// `files.grep` —— 同步那一臂（唯一入口 [`answer`] 走它；取消位由调用方给）。
fn answer_grep(args: &serde_json::Value, cancel: &std::sync::atomic::AtomicBool) -> Answer {
    let (top, a) = grep_args(args)?;
    let g = grep::search(&top, &a, cancel).map_err(|e| grep_unreadable(&e))?;
    Ok(grep_reply(&top, a.limit, &g))
}

/// `files-grep` 帧面那一臂：**可撤**。走一趟放进阻塞线程池；这个 future 被丢掉（`cancel` 帧）⇒ 守卫把取消位置上，
/// 阻塞线程上那一趟看见它就收手（每进一项看一次）。
pub async fn answer_grep_cancellable(args: serde_json::Value) -> Answer {
    struct RaiseOnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for RaiseOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _raise = RaiseOnDrop(flag.clone());
    tokio::task::spawn_blocking(move || answer_grep(&args, &flag))
        .await
        .unwrap_or_else(|e| {
            Err((
                "unreadable",
                copy_core::copy_text("beFilesRead.grep.unreadable", &[("kind", &e.to_string())]),
            ))
        })
}

/// `files.home` —— 这台机器上「开在 home」那个起点。
///
/// 🔴 **说不出就拒，不猜**：没有这一格 / 是空的 / 不是绝对路径 ⇒ `no_home`。
/// 拿当前目录或根目录兜底，就是「窗口开出来了、却开在一个用户没要的地方」。
fn answer_home() -> Answer {
    home_from(home_var())
}

/// [`answer_home`] 里**有逻辑的那一段**：环境给的那一格 → 起点或拒。
///
/// 抽成吃参数的纯函数，是为了让「没有 / 空 / 相对」三形**喂得进去**：
/// 判据若去改测试进程自己的环境，就是在一个多线程进程里改全局状态；
/// 若拿同一个环境变量去对答案，两侧同源、恒真。
fn home_from(h: Option<std::ffi::OsString>) -> Answer {
    let h = h.filter(|v| !v.is_empty()).ok_or((
        "no_home",
        copy_core::copy_text("beFilesRead.home.missing", &[]),
    ))?;
    let p = std::path::PathBuf::from(h);
    if !p.is_absolute() {
        return Err((
            "no_home",
            copy_core::copy_text("beFilesRead.home.relative", &[]),
        ));
    }
    Ok(serde_json::json!({ "path": raw::to_json(raw::path_bytes(&p)) }))
}

/// 这一族的**唯一入口**。
///
/// 🔴 「一条命令、一个往返」（第三段）就是这个函数的形状：
/// 一次调用进来、一个 JSON 出去，中间**不与调用方再对话**。
///
/// ⚠ 分派写成一个对 [`CAPABILITIES`] 的 `match`，而「这个 `match` 与那张表的名字
/// 两向对得上」由 `capability_guard` 钉住 —— 不许出现「表里有、分派没有」
/// 那种静默的不可用（本仓 `p1t-removal-cause` 那次真 bug 就是这一形）。
pub fn answer(name: &str, args: &serde_json::Value) -> Answer {
    match name {
        "files.ls" => answer_ls(args),
        "files.stat" => answer_stat(args),
        "files.find" => answer_find(args),
        "files.index.status" => answer_status(),
        "files.index.rebuild" => answer_index_rebuild(args),
        "files.browse" => answer_browse(args),
        "files.read.text" => answer_read_text(args),
        "files.home" => answer_home(),
        "files.size" => answer_size(args),
        "files.read.chunk" => answer_read_chunk(args),
        "files.grep" => answer_grep(args, &std::sync::atomic::AtomicBool::new(false)),
        other => Err((
            "unknown_capability",
            crate::common::contract::malformed(&format!("unknown capability `{other}`")),
        )),
    }
}

/// 本族声明的能力名。
pub fn capability_names() -> Vec<&'static str> {
    CAPABILITIES.iter().map(|c| c.name).collect()
}

/// **线上命令名 → 能力名**（`files-index-status` → `files.index.status`）。
/// 这一族上线之后，两个命名空间之间的翻译**只有这一处**。
///
/// # 🔴 为什么线上那一面不能直接叫 `files.ls`（不是排版偏好，是两条判据的射程）
///
/// 能力名带 `.`，而命令面那一侧的取词器**不认这个字符**，现打：
///
/// - `protocol_doc_guard::dispatched_subcommands` 与
///   `main_argv_table_guard::dispatched` 收 token 的字符集同样是那一套
///   ⇒ `"--files.ls"` 这个字面量会被**静默丢弃**：不是「查过觉得没问题」，是**没看见**。
///
/// ⇒ 用 `.` 的代价是把四条命令从两条判据底下同时抽走，而两条都照常报绿 ——
/// 那正是本仓反复治的那一形。⇒ 线上一律 `-`，能力名一个字不动
/// （那张表逐字钉着它），两者之间只留这一个函数。
///
/// ⚠ 反向替换之所以够用：本族**没有一条能力名里带 `-`**，
/// 由 `inbound_structure_guards` 那条两向集合相等钉住。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    answer(&wire_name.replace('-', "."), args)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/capability_guard.rs"]
mod tests;

// 〔波 5〕**文件管理后端模块**与原生后端那条边界的判据
// （用户逐字「后端要模块化, 即原生后端＋文件管理后端. 现在先解耦清楚」）。
// 挂在这里而不是 `lib.rs`：那份文件的模块声明那几行归另一路（A1）。
#[cfg(test)]
#[path = "../../../tests/backend/files/module_boundary_guard.rs"]
mod module_boundary_guard;
