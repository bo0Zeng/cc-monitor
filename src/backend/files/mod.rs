//! 〔步 24f · 2026-09-20〕**`files-read` 这一族** —— 后端侧**只读**的文件面，
//! 登记住 `设计/96 §2.9`、设计正文住 `设计/60 §3.5`、秤住 `设计/17 §6.9` 的 `F2`。
//!
//! # 这一族为什么存在（只有一条理由，别读宽）
//!
//! `设计/60 §2 档①` 那张表**只有一行**：
//!
//! > **远端搜索** —— SFTP 只能递归 `READDIR`，N 次往返；
//! > 而且**协议里没有「放一份常驻索引」这个概念**。
//!
//! ⇒ 删 / 改名 / 建目录 / **复制**（`copy-data` 零流量）**全部留在 SFTP**
//!（`设计/60 §6.6 ①`）。后端买到的**只有搜索**，而搜索**纯读**
//! ⇒ `tests/backend/readonly_guard.rs` 那 4259 行**一行都不用改**。
//! 本件整件事的前提就是不动那条铁律。
//!
//! # 🔴 三条硬边界（`设计/96 §2.9`）—— 它们是**能力声明的一部分**，不是注释
//!
//! ## ① 整族一个字节都不写，而且**这一条要进声明的语义**
//!
//! `设计/96 §2.9` 边界① 逐字：
//!
//! > 这一条要进 `CAPABILITIES` 的语义，**不是注释**：将来谁往这一族里加一个写操作，
//! > **能力声明这一侧就该先红**，而不是靠 `readonly_guard` 兜底 ——
//! > 两道都要，但声明那道更早。
//!
//! 兑现处是下面那张 [`CAPABILITIES`] 的第三栏 [`Capability::effect`]：
//! 它是一个**闭集**（[`Effect`]），每一条能力都要说自己落在哪一格。
//! 而 `tests/backend/files/capability_guard.rs` 把那一栏钉成一条**相等**断言：
//!
//! ```text
//! 从实现源码里**派生**出来的那一格  ==  这里**声明**的那一格
//! ```
//!
//! 派生的人群是 [`Capability::impl_files`]，而那几张表的并集又被钉成
//! 「**恰好等于**本族目录下现打的全部 `.rs`」（分区恒等）——
//! 新加一份文件想躲开派生，那条恒等当场红。
//!
//! ⇒ 往本族任何一份实现里写一个改动盘上东西的动词，**声明这一侧第一个红**
//!（`readonly_guard` 那一侧会跟着红，两道都在）。
//! 🔴 这一条**有死值验**：把一个写盘动词塞进 `index.rs` 再跑，
//! `the_declared_effect_equals_the_effect_derived_from_the_implementation` 逐字点名。
//!
//! ## ② 跨 target 不对等，而且必须如实声明
//!
//! `设计/96 §2.9` 边界② 逐字：
//!
//! > 本篇 `§2` 那条跨 target 对拍断言，对这一族要判的是「**能力在不在**」，
//! > **不是「新鲜度一样」** —— 后者在三个平台上本来就不同。
//! > **把它们判成相等会逼人写假声明。**
//!
//! ⇒ 本模块把这两件事拆成**两张表**，各自一套判法：
//!
//! | 表 | 判法 | 为什么 |
//! |---|---|---|
//! | [`CAPABILITIES`] 的 [`Capability::targets`] | 🔴 **集合相等** —— 每条能力在 [`TARGETS`] 全体上都要有 | 「能力在不在」在三个平台上必须是同一个答案 |
//! | [`FRESHNESS`] | **逐 target 一行，刻意不判相等** | 保鲜机制逐平台不是一件事；判成相等 = 逼人写假声明 |
//!
//! 而 [`FRESHNESS`] 那张表另有一条**反向**判据接着：它不许**全部**平台都填同一个
//! 机制串。那一条挡的正是「为了让某条对拍变绿，把三行抄成一样」——
//! 也就是上面那句「假声明」的具体长相。
//!
//! ## ③ 「定期重走」的周期是能力的一部分，必须可查询
//!
//! 住 [`index::REWALK_INTERVAL_SECS`]（那里逐条写了这个数怎么定的、依据是什么），
//! 由 `files.index.status` 交出去（[`index::Status::rewalk_interval_secs`]）。
//! `设计/60 §3.5.3` 逐字要求那个延迟**显示在界面上**，不许让用户猜为什么搜不到。
//!
//! # ⚠ 本件**没有**做到的（`设计/96 §2` 那三层里的第 2、3 层，以及线上那一跳）
//!
//! 1. **`CAPABILITIES` 的汇总没接**。`设计/96 §2` 第 2 层要求「能力清单从实现派生，
//!    `CAPABILITIES` 由它们汇总而来」。本族把自己那一份声明成了**数据**
//!   （下面这张表），但**没有**把它汇进 `lib.rs::CAPABILITIES` ——
//!    那一处的语义今天是「**会在一次性查询判定前剥离对应 flag** 的流能力」
//!   （`§26` 死循环护栏逐字，`main_stream_flag_tests` 在钉），本族四条**都不是那种东西**，
//!    硬塞进去会当场红，而且会是**红对了**。⇒ 汇总要先有第 2 层那个派生机制，那是另一件活。
//! 2. ✅〔`24f` 第二刀 · 2026-09-20〕**线上那一跳已经接上**：四条能力同拍进了
//!    `inbound::REGISTRY` / `inbound::COMMANDS`（帧面）· `lib::SUBCOMMANDS`（CLI 面）·
//!    `src/doc/IPC-PROTOCOL.md §10`（四个小节 ＋ CLI 那一半那一段），并 bump 了 `BUILD_ID`。
//!    [`answer`] 一个字节没改 —— 它的签名本来就是照着 `inbound::CommandSpec` 的处理器形状
//!    做的（一进一出、`(code, message)` 的错误信封），接线那一拍加的是四条登记，不是重写。
//!    线上名与能力名的翻译**只有一处**：[`answer_wire`]。
//!    🔴 **但这不等于「搜索能用了」，两条如实登记：**
//!    ① **索引今天没有任何线上办法叫它建。** `设计/60 §3.5.2a` 把节拍留给调用方，
//!       而「重走」那条命令**不在** `设计/96 §2.9` 那张四条的表里
//!       ⇒ [`index::rebuild_once`] 与 [`browse_watch::set_browsing`] 至今**零生产调用方**
//!       ⇒ 真机上 `files.find` 恒回 `index_missing: true`（那**不是**「没搜到」）。
//!       这是设计面的一个缺口，不是接线漏了一条：要补它得先在 `设计/96 §2.9` 那张表上
//!       裁出第五条能力。**报备，不在本刀里自己长出来。**
//!    ② **消费侧还没有**：`src/bridge` 那一头一个字节都没动（`files.index.status`
//!       回的 `age_secs` / `rewalk_interval_secs` / `stale` 还没显示在界面上 ——
//!       `设计/60 §3.5.3` 那条 ⬜ 仍然是 ⬜）。
//! 3. **按内容搜 / 模糊匹配 / 排序** —— `设计/60 §3.5.3` 逐字「一条都没设计」，本件也没做。

pub mod browse_watch;
pub mod index;
pub mod raw;

/// 这一族的名字。`设计/96 §2.9` 的标题逐字。
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

/// 编译 target —— `设计/96 §2` 那条跨 target 对拍的人群。
///
/// ⚠ 与 `设计/01 §7.2` 那条「两个壳」的对等断言**不是同一条断言**
///（`设计/96 §2` 开头逐字分过这两个轴：壳答「折进去会不会改变它能干什么」，
/// target 答「每个平台编不编得过」）。本族登记的是后者。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// `x86_64-unknown-linux-gnu`（本机原生构建，也是 in-process 那条路的宿主）。
    LinuxGnu,
    /// `*-unknown-linux-musl`（部署到被观测机器的那份静态字节，两个 arch）。
    LinuxMusl,
    /// `x86_64-pc-windows-*`。
    Windows,
    /// `*-apple-darwin`。
    MacOs,
}

/// 全体 target。**这一族的能力集在它们之上必须相等**（边界②）。
pub const TARGETS: &[Target] = &[
    Target::LinuxGnu,
    Target::LinuxMusl,
    Target::Windows,
    Target::MacOs,
];

/// 一条能力的登记。
///
/// ⚠ 后四栏（`impl_files` / `args` / `fields` / `codes`）**只被判据读** ——
/// 那正是它们存在的理由：把「这条能力的契约面」写成**数据**，好让机检对着它比
///（形状照 `inbound::CommandSpec`，那份头注逐字写着同一条理由）。
pub struct Capability {
    /// 线上能力名。`设计/96 §2.9` 那张表逐字。
    pub name: &'static str,
    /// 它做什么。
    pub what: &'static str,
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
/// 四条，与 `设计/96 §2.9`「这一族有哪些」那张表**逐字同名**
///（判据按名字两向对拍，改一边不改另一边当场红）。
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        name: "files.ls",
        what: "列一个目录的直接子项（名字走原始字节）",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        // 🔴 〔`24f` 第二刀 09-20 订正〕这里原先还有一条 `"ignore_ascii_case"` ——
        //    **[`answer_ls`] 一次都没读它**（现打：它只调 [`path_arg`] 与 [`limit_of`]）。
        //    那是从下面 `files.find` 那条抄过来的一个鬼影：本表没有任何判据拿 `args`
        //    去对真解析器，所以它一直没红。接线那一拍必须先把它摘掉 ——
        //    不然 `src/doc/IPC-PROTOCOL.md §10` 那份**冻结的线上契约**里就会多出一个
        //    「写了也不起作用」的参数，而那份文档的读者在仓外。
        args: &["limit", "path"],
        fields: &["entries", "kind", "mtime_secs", "path", "size", "truncated"],
        codes: &["bad_path", "unreadable"],
    },
    Capability {
        name: "files.stat",
        what: "一个路径的元数据",
        effect: Effect::ReadsOnly,
        impl_files: &["mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["path"],
        fields: &["kind", "mtime_secs", "path", "readonly", "size"],
        codes: &["bad_path", "unreadable"],
    },
    Capability {
        name: "files.find",
        what: "🔴 **在常驻索引里查** —— SFTP 给不了的那一条，整族的存在理由",
        effect: Effect::ReadsOnly,
        impl_files: &["browse_watch.rs", "index.rs", "mod.rs", "raw.rs"],
        targets: TARGETS,
        args: &["ignore_ascii_case", "limit", "needle"],
        fields: &[
            "hits",
            "index_age_secs",
            "index_missing",
            "scanned",
            "total_hits",
            "truncated",
        ],
        codes: &["bad_args"],
    },
    Capability {
        name: "files.index.status",
        what: "索引的新鲜度 ／ 条目数 ／ 常驻字节，**以及后端声明的重走周期**（边界③）",
        effect: Effect::ReadsOnly,
        impl_files: &["browse_watch.rs", "index.rs", "mod.rs"],
        targets: TARGETS,
        args: &[],
        fields: &[
            "age_secs",
            "browse_watch_cap",
            "browse_watches",
            "entries",
            "index_missing",
            "resident_bytes",
            "rewalk_interval_secs",
            "stale",
            "truncated",
            "unreadable_dirs",
        ],
        codes: &[],
    },
];

/// **保鲜机制逐 target 的如实声明**（边界②的另一半）。
///
/// 🔴 **这张表刻意不参加任何「三个平台要一样」的对拍。** 理由是 `设计/96 §2.9` 逐字的：
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
    /// 文献读数，**没实测**。
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
              而 watch 上限本机现打 262144、home 下 640413 条目（`真相源/98 §3.2`）⇒ 全挂挂不住。\
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
        gap: "🟡 **文献读数，没实测**（`设计/96 §2.9` 那张表里这一格逐字就是这么标的）。\
              FSEvents 能无特权递归监听一棵树、Spotlight 的索引普通用户可查（`mdfind`）——\
              两条都**没碰过**。⇒ 这个平台上「保鲜要不要换成另一套机制」判不了。",
    },
];

// ══════════════════════ 命令面（形状照 `inbound::CommandSpec` 的处理器）══════════════════════

/// 一条能力的答案：成功交 JSON，失败交 `(code, message)`。
///
/// ⚠ 与 `inbound` 那一侧**逐字同形**（它的 `CmdResult` 是
/// `Result<Option<serde_json::Value>, (String, String)>`）——接线那一拍才不用改形状。
pub type Answer = Result<serde_json::Value, (&'static str, String)>;

/// 每次回送的条数上限的**默认值**。
///
/// 调用方给 `limit` 就用它的。给 0 或不给 ⇒ 用这个。
/// ⚠ 上限存在的理由不是省内存，是「一次往返」这句话要成立：
/// `真相源/98 §3.3` 那趟现打里有一次查询命中 **52 666** 条 ——
/// 把它们一次全推过去，「零流量搜索」那句话就只剩半句。
/// 回送被截断时 `truncated` 与 `total_hits` 两个字段都会说出来。
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
        "少了 `path` —— 它要么是一个字符串，要么是 `{\"b16\": \"<十六进制>\"}`".to_string(),
    ))?;
    let bytes = raw::from_json(v).ok_or((
        "bad_path",
        "`path` 的形状不对 —— 只认字符串或 `{\"b16\": \"<十六进制>\"}`；\
         这里刻意不「尽力而为」地猜，猜错一个字节就是去看另一个文件"
            .to_string(),
    ))?;
    if bytes.is_empty() {
        return Err(("bad_path", "`path` 是空的".to_string()));
    }
    Ok(raw::to_path_buf(&bytes))
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

/// 这一族认得的类型名 —— **闭集，唯一住址**。判据按它对拍 [`kind_name`] 的出口。
pub const KINDS: &[&str] = &["dir", "file", "other", "symlink"];

/// `files.ls` —— 列一个目录的直接子项。
fn answer_ls(args: &serde_json::Value) -> Answer {
    let dir = path_arg(args)?;
    let limit = limit_of(args);
    let rd = std::fs::read_dir(&dir)
        .map_err(|e| ("unreadable", format!("这个目录打不开：{:?}", e.kind())))?;
    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut seen = 0usize;
    for e in rd {
        let Ok(e) = e else { continue };
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
            }
        }
        entries.push(serde_json::Value::Object(row));
    }
    Ok(serde_json::json!({
        "entries": entries,
        "truncated": seen > entries.len(),
    }))
}

/// `files.stat` —— 一个路径的元数据。
///
/// ⚠ **它跟 symlink**（拿的是链接指向的那个东西的元数据）。
/// 不跟的那个读法要另一个动词，而那个动词**不在** `readonly_guard` 的只读白名单上
///（那张表在另一棵树上、不在本件写区；往它加动词是**放宽**一条红线，
/// 不是本件该顺手做的事）。⇒ **这是一条真实的局限，如实登记。**
/// 需要区分链接本身时，先用 `files.ls` 看它父目录那一行的 `kind`
///（那一栏走的是不跟链接的读法）。
fn answer_stat(args: &serde_json::Value) -> Answer {
    let path = path_arg(args)?;
    let md = std::fs::metadata(&path)
        .map_err(|e| ("unreadable", format!("这个路径读不到：{:?}", e.kind())))?;
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
    if let Some(ms) = epoch_secs(md.modified()) {
        out.insert("mtime_secs".to_string(), serde_json::json!(ms));
    }
    Ok(serde_json::Value::Object(out))
}

/// `files.find` —— 在常驻索引里查。**只回送命中。**
fn answer_find(args: &serde_json::Value) -> Answer {
    let v = args.get("needle").ok_or((
        "bad_args",
        "少了 `needle` —— 它要么是一个字符串，要么是 `{\"b16\": \"<十六进制>\"}`".to_string(),
    ))?;
    let needle = raw::from_json(v).ok_or((
        "bad_args",
        "`needle` 的形状不对 —— 只认字符串或 `{\"b16\": \"<十六进制>\"}`".to_string(),
    ))?;
    let r = index::find(&index::FindArgs {
        needle,
        ignore_ascii_case: args
            .get("ignore_ascii_case")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        limit: limit_of(args),
    });
    Ok(serde_json::json!({
        "hits": r.hits.iter().map(|h| raw::to_json(h)).collect::<Vec<_>>(),
        "total_hits": r.total_hits,
        "truncated": r.truncated,
        "scanned": r.scanned,
        "index_age_secs": r.index_age_secs,
        "index_missing": r.index_missing,
    }))
}

/// `files.index.status` —— 新鲜度 ／ 条目数 ／ 常驻字节 ／ **声明的重走周期**。
fn answer_status() -> Answer {
    let s = index::status();
    Ok(serde_json::json!({
        "index_missing": s.index_missing,
        "entries": s.entries,
        "resident_bytes": s.resident_bytes,
        "unreadable_dirs": s.unreadable_dirs,
        "truncated": s.truncated,
        "age_secs": s.age_secs,
        "rewalk_interval_secs": s.rewalk_interval_secs,
        "stale": s.stale,
        "browse_watches": s.browse_watches,
        "browse_watch_cap": s.browse_watch_cap,
    }))
}

/// 这一族的**唯一入口**。
///
/// 🔴 「一条命令、一个往返」（`设计/60 §3.5.2` 的第三段）就是这个函数的形状：
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
        other => Err(("unknown_capability", format!("`{other}` 不是这一族的能力"))),
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
/// 能力名带 `.`，而命令面那一侧**两个取词器都不认这个字符**，现打：
///
/// - `protocol_doc_guard::code_span_identifiers` 按「字母数字 / `_` / `-`」切词
///   ⇒ 文档里写多全，`documented.contains("files.ls")` 都**恒假**
///   （`bus-list` 当年是因为少了 `-` 才被补进去的，同一条射程问题）；
/// - `protocol_doc_guard::dispatched_subcommands` 与
///   `main_argv_table_guard::dispatched` 收 token 的字符集同样是那一套
///   ⇒ `"--files.ls"` 这个字面量会被**静默丢弃**：不是「查过觉得没问题」，是**没看见**。
///
/// ⇒ 用 `.` 的代价是把四条命令从三条判据底下同时抽走，而三条都照常报绿 ——
/// 那正是本仓反复治的那一形。⇒ 线上一律 `-`，能力名一个字不动
/// （`设计/96 §2.9` 那张表逐字钉着它），两者之间只留这一个函数。
///
/// ⚠ 反向替换之所以够用：本族**没有一条能力名里带 `-`**，
/// 由 `inbound_structure_guards` 那条两向集合相等钉住。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    answer(&wire_name.replace('-', "."), args)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/capability_guard.rs"]
mod tests;
