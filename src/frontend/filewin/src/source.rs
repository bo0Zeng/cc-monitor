//! 数据面：**列一个目录**。经通道问那台机器上的后端 —— 本机也是一台（`<local>`），同一条路。
//!
//! # 🔴列目录**只经通道问后端**（逐条理由住下面「窗口进程只说 `call`」那一节）
//!
//! # 排序在后端
//!
//! 一屏的先后（四列 · 目录在前 · 反过来只反那一列）由 `files-ls` 的 `sort` 排好送来，窗口照摆、不重排。

use cmd_wire::files::WirePath;
use copy_core::copy_text;
/// 本机时区（偏移读不出 ⇒ 按 UTC 排）：后端与壳同一份。
use platform_core::tz;

/// 文件列表里的一行。**故意比 `SftpEntry` 窄** —— 列表只画得下这些。
///
/// 🔴**它要过一次进程边界**，所以带 serde 那一对（逐条理由住 [`super::proc`]）。⚠ 这一对 derive **不是** rust↔TS 那条边界上的
/// （没有 `#[ts(export)]`，前端一个字段都不消费它）—— 它只走
/// 「monitor → 窗口进程」这一跳，两头是**同一份代码**编出来的。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Row {
    pub name: String,
    /// 绝对路径。远端恒用 `/`；本机用本机分隔符。
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// 非 UTF-8 文件名有损显示 —— 本机侧同样会有（`to_string_lossy`）。
    pub lossy_name: bool,
}

/// 一行 ＋ **后端在送、而 [`Row`] 那五格装不下的那两格**（是不是链接 · 什么时候改的）。
///
/// # 🔴 窄的那个不动，外面套一层
///
/// [`Row`] 只是那五格（它的结构体字面量盘上约 30 处），`Listed` 是**窗口里的一行**。⚠ 刻意**只给 `Deref`、不给 `DerefMut`**：
/// 经这一层改不了里面那一行 ⇒ 「两格与那一行漂开」这件事**写都写不出来**。
///
/// # 第一屏
///
/// 第一屏由窗口进程自己列（`proc::first_screen`），链接与时间两格直接从 [`row_from_ls_entry`] 来。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Listed {
    /// 那五格。
    pub row: Row,
    /// 🔴 这一项**是不是符号链接**。后端 `files.ls` 的 `kind` 与 SFTP 的 `is_symlink`
    /// 两条路都送得出它，窗口照它画链接（见 [`row_from_ls_entry`] 那张表）。
    pub link: bool,
    /// 链接指向的是目录（后端 `files-ls` 的 `link_dir`）：点得进去，但删 / 复制仍按链接本身算（不递归）。
    pub link_dir: bool,
    /// 最后修改时间（epoch 秒）。
    ///
    /// 🔴 `None` 逐字是「**这条路没送这一格**」，不是「这个文件没有时间」。
    /// 两者刻意不混：SFTP 那条退路交不出它（`SftpEntry` 里没有这一格），
    /// 而「后端送了一个 0」是 1970 年，那是一个真时间。
    pub mtime_secs: Option<u64>,
    /// 修改时间画出来的样子：列里那一格（短）与悬停 / 属性里的完整那一格 —— 那台后端按它的本地钟写好（`files-ls` 的
    /// `mtime_text` · `mtime_full`），窗口照抄、不换算。没送 ⇒ `None`（同 `mtime_secs`：不画编出来的字）。
    #[serde(default)]
    pub mtime_text: Option<String>,
    #[serde(default)]
    pub mtime_full: Option<String>,
    /// **有损名的原始字节**（名字那一段，不是整条路径）；名字是合法 UTF-8 ⇒ `None`。
    ///
    /// 窗口的路径是字符串，非 UTF-8 的名字经有损解码之后**寻址不到**（U+FFFD 不是那个字节）。
    /// 后端 `files-ls` 送的本来就是字节（`{"b16": …}`），[`row_from_ls_entry`] 把它留住 ——
    /// 写操作（改名 · 删除 · 改权限）就能对乱码名动手（写面的相对段按这几个字节发，`cmd_wire::files::WirePath`）。
    /// ⚠ `serde(default)`：开窗那一跳（`proc::OpenRequest`）两头版本不齐时缺这一格 ⇒ `None`
    /// ⇒ 那一行退回「有损名不许写」，不会拿显示串去寻址。
    #[serde(default)]
    pub raw_name: Option<Vec<u8>>,
    /// 断了的链接（后端 `files-ls` 的 `link_to: "missing"`：指向的东西不在 / 读不到）。
    #[serde(default)]
    pub link_broken: bool,
    /// 画哪种图标（后端判的 `class`）。
    pub class: cmd_wire::files::FileClass,
    /// 「类型」那一列的字（后端写好的；问不出 ⇒ 空）。
    pub type_text: String,
    /// 预览 · 看图 · 编辑那几道门（后端判好的；目录与问不出的路 ⇒ `None`）。
    pub gates: Option<cmd_wire::files::Gates>,
    /// 大小那一格的字（后端写好的；问不出 ⇒ `None`，那一格空着）。
    #[serde(default)]
    pub size_text: Option<String>,
}

impl Listed {
    /// 从一条**只交得出那五格**的路来的一行（链接与时间两格都**不知道**）。
    ///
    /// ⚠ 名字里的 `plain` 说的就是那件事：它不是「这一行不是链接、没有时间」，
    /// 是「问不出来」。⇒ 画的时候两格都不画，而不是画一个编出来的值。
    pub fn plain(row: Row) -> Self {
        Self {
            link: false,
            link_dir: false,
            mtime_secs: None,
            mtime_text: None,
            mtime_full: None,
            raw_name: None,
            link_broken: false,
            class: if row.is_dir {
                cmd_wire::files::FileClass::Folder
            } else {
                cmd_wire::files::FileClass::Other
            },
            type_text: String::new(),
            gates: None,
            size_text: None,
            row,
        }
    }

    /// 点得进去：那台画成文件夹的那几行（目录 · 指向目录的链接，后端 `files/class.rs::class_of_entry` 判；同手机 `Files.kt::entryOf`）。
    pub fn opens_as_dir(&self) -> bool {
        self.class == cmd_wire::files::FileClass::Folder
    }

    /// 这一行在 `files-ls` 回包里的那几格，原样交回本机后端问菜单（`files-menu`；认链接 · 断链 · 有损名在核心那一处）：
    /// 名字的字节 · 种类 · 链接指向 · 种类 · 门 · 大小与时间那几格照收到的抄；路径与显示形是窗口拼好的那一串。
    pub fn ls_row(&self) -> cmd_wire::files::LsEntry {
        use cmd_wire::files::{FileKind, LinkTo, LsEntry};
        let at = WirePath(self.row.path.as_bytes().to_vec());
        LsEntry {
            path: at.clone(),
            name: WirePath(
                self.raw_name
                    .clone()
                    .unwrap_or_else(|| self.row.name.as_bytes().to_vec()),
            ),
            kind: if self.link {
                FileKind::Symlink
            } else if self.row.is_dir {
                FileKind::Dir
            } else {
                FileKind::File
            },
            size: (!self.row.is_dir).then_some(self.row.size),
            size_text: self.size_text.clone(),
            mtime_secs: self.mtime_secs,
            mtime_text: self.mtime_text.clone(),
            mtime_full: self.mtime_full.clone(),
            link_dir: self.link.then_some(self.link_dir),
            link_to: self.link.then_some(if self.link_broken {
                LinkTo::Missing
            } else if self.link_dir {
                LinkTo::Dir
            } else {
                LinkTo::File
            }),
            class: self.class,
            type_text: self.type_text.clone(),
            gates: self.gates.clone(),
            shown: at,
        }
    }
}

/// 夹具与「只交得出那五格」的路用的那一形 —— 与 [`Listed::plain`] 同义。
impl From<Row> for Listed {
    fn from(row: Row) -> Self {
        Self::plain(row)
    }
}

impl std::ops::Deref for Listed {
    type Target = Row;
    fn deref(&self) -> &Row {
        &self.row
    }
}

/// 这个窗口现在在看哪儿 —— **一台机器**（本机也是一台：`<local>`，与远端同一条路，窗口里没有「本机那一侧」的分支）。
///
/// 它装的是那台机器**寻址用的名字**（本机后端机器表里算好的那个名字，monitor 开窗时随种子交来）。
/// 窗口只拿它取名字；「在此打开终端」由 monitor 接，所以窗口进程不认识 monitor 的配置类型
/// （`filewin` 边界判据那一格「开窗配置的类型」是零）。
#[derive(Clone, Debug)]
pub struct Source(String);

impl Source {
    /// 造一个 —— **这是唯一的造法**。
    pub fn remote(name: String) -> Self {
        Source(name)
    }

    /// 给窗口标题/面包屑用的短名。
    pub fn label(&self) -> String {
        self.0.clone()
    }

    /// 这一趟问的是**哪台机器** —— 走 [`Origin`]（`chan::wire` 再导出的全仓那一个类型）。
    ///
    /// 🔴 它与 [`Source::label`] **刻意分开两个函数**：`label` 是给人看的，
    /// 而这一个是**寻址用的**。两者今天由同一个 `origin_label()` 喂
    /// （本机那一侧退役之前不是这样：那时 `label` 回的是「本机」两个中文字，
    /// 而寻址要的是 `cmd_wire::LOCAL`）。
    /// ⇒ **仍然不合并**：它们回的是同一个串是**今天的实况**，不是契约。
    ///
    /// ⚠ **刻意不回 `String`**：`origin_tests::no_new_raw_string_origin_parameters`
    /// 是一条递减棘轮 —— 「origin 归一 —— 这是地基」，
    /// 新代码一律用这个类型，不许再给这个概念造一种表达。
    /// ⚠ 那个名字由 monitor 开窗时用 `origin_label()` 算（`filewin/entry.rs`），与 `stream_source` 的 `on_attached` 登记时
    /// 用的是**同一个函数** —— 两处漂开的症状是「命令发给了一个谁都没登记过的
    /// origin，而且不报错」。
    pub fn origin(&self) -> Origin {
        Origin::named(self.0.clone())
    }
}

// 远端路径按字节切只住线上类型那一份（`cmd_wire::files::WirePath::{parent, tail, child}`：只认 `/`，尾 `/` 全去掉，根的上一级是根）。
// 下面三个是窗口里持**串**那一侧的适配：合法 UTF-8 进、合法 UTF-8 出（切点都在 `/` 上，不会切进一个字符中间），自己一个字节都不切。

/// 上一级目录（[`WirePath::parent`]）。到顶了回 `/`：调用方靠「回来的和给出去的相等」判断已经在顶上了。
pub fn parent_dir(cwd: &str) -> String {
    String::from_utf8_lossy(&WirePath::parent(cwd.as_bytes()).0).into_owned()
}

/// 最后一段（[`WirePath::tail`]）。
pub fn remote_basename(path: &str) -> &str {
    std::str::from_utf8(WirePath::tail(path.as_bytes())).unwrap_or_default()
}

/// 目录 ＋ 名字（[`WirePath::child`]）。
pub fn child_of(dir: &str, name: &str) -> String {
    String::from_utf8_lossy(&WirePath::child(dir.as_bytes(), name.as_bytes()).0).into_owned()
}

/// 一屏按哪一列排、反没反：线上那一形，窗口状态（关窗即没）。
pub use cmd_wire::files::{LsSort as Sort, LsSortBy as SortBy};

/// 点了表头 `by` 那一列之后怎么排：同一列 ⇒ 反过来；换了一列 ⇒ 那一列的第一下。
pub fn after_click(now: Sort, by: SortBy) -> Sort {
    Sort {
        by,
        reverse: now.by == by && !now.reverse,
    }
}

/// 一条远端绝对路径 → **可点的那几段**：`(这一段叫什么, 点它去哪儿)`，根排在最前。
///
/// # 🔴 它为什么住在这儿（而不是 `shell.rs` 里画面包屑的那一行旁边）
///
/// 切远端路径的算法**多一份就多一种「Windows 上 `\` 被当分隔符」的机会**。面包屑要的是 [`WirePath`] 那几样之外的
/// 另一种切法（前缀那一摞，不是单个前缀、也不是尾段），守同一句：**只认 `/`，一次都不碰 `std::path`**。
///
/// 盘上每一处按 `/` 切的地方由 `source_tests::every_place_that_splits_a_remote_path_is_declared`
/// 逐条钉成一张相等的表。
///
/// ⚠ 到顶（`/`）时回的是**恰好一格**（根自己）。空串同理 ——
/// 面包屑那一行永远至少画得出一个可点的根，不会出现「一行都没有」那种空态。
pub fn breadcrumbs(path: &str) -> Vec<(String, String)> {
    let mut out = vec![("/".to_string(), "/".to_string())];
    let mut acc = String::new();
    for seg in path.trim_end_matches('/').split('/') {
        // 空段（开头那个、或者 `//` 中间那个）跳过 —— 它不是一级目录。
        if seg.is_empty() {
            continue;
        }
        acc.push('/');
        acc.push_str(seg);
        out.push((seg.to_string(), acc.clone()));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// **有损名全寻址**：一条远端路径的原始字节（「整条寻址链要换成字节」）
// ═══════════════════════════════════════════════════════════════════════
//
// 窗口里显示用的一律是有损串；**发出去的**（列目录 · 读 / 存文本 · 复制 · 算大小 · 写面的根）经 [`RemotePath::wire`]：
// 路径是合法 UTF-8 ⇒ 字符串；不是 ⇒ `{"b16": …}`（后端 `files-*` 那一族早就收这一形）。
// 上一级 · 尾段 · 拼一项照 [`WirePath`] 那一份切（恒用 `/`，尾斜杠不算一段）。

/// 一条远端路径：显示串 ＋（路径不是合法 UTF-8 时）原始字节。`raw == None` ⇒ `shown` 就是真字节。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemotePath {
    pub shown: String,
    pub raw: Option<Vec<u8>>,
}

impl RemotePath {
    /// 一条合法 UTF-8 的路径。
    pub fn plain(s: &str) -> Self {
        Self {
            shown: s.to_string(),
            raw: None,
        }
    }

    /// 从字节来：合法 UTF-8 ⇒ 等于 [`Self::plain`]；否则留住字节、显示有损串。
    pub fn from_bytes(b: &[u8]) -> Self {
        match std::str::from_utf8(b) {
            Ok(s) => Self::plain(s),
            Err(_) => Self {
                shown: String::from_utf8_lossy(b).to_string(),
                raw: Some(b.to_vec()),
            },
        }
    }

    /// 显示串 ＋ 可能有的字节（行上那两格的形状）。
    pub fn of(shown: &str, raw: Option<&[u8]>) -> Self {
        match raw {
            Some(b) => Self::from_bytes(b),
            None => Self::plain(shown),
        }
    }

    /// 真字节。
    pub fn bytes(&self) -> Vec<u8> {
        self.raw
            .clone()
            .unwrap_or_else(|| self.shown.as_bytes().to_vec())
    }

    /// 发出去的那一形（字符串或 `{"b16": …}`）。
    pub fn wire(&self) -> serde_json::Value {
        wire_bytes(&self.bytes())
    }

    /// 发出去的那一格（线上类型：原始字节）。
    pub fn path(&self) -> cmd_wire::files::WirePath {
        cmd_wire::files::WirePath(self.bytes())
    }

    /// 尾段那一格（线上类型，[`WirePath::tail`]）。
    pub fn tail(&self) -> WirePath {
        WirePath(WirePath::tail(&self.bytes()).to_vec())
    }

    /// 上一级（[`WirePath::parent`]）。
    pub fn parent(&self) -> Self {
        Self::from_bytes(&WirePath::parent(&self.bytes()).0)
    }

    /// 底下名叫 `name` 的那一项（[`WirePath::child`]）。
    pub fn child(&self, name: &[u8]) -> Self {
        Self::from_bytes(&WirePath::child(&self.bytes(), name).0)
    }

    /// 这条路径是有损的（显示串寻址不到真字节）。
    pub fn is_lossy(&self) -> bool {
        self.raw.is_some()
    }
}

/// 字节 → 发出去的那一形：合法 UTF-8 ⇒ 字符串；否则 ⇒ `{"b16": …}`（小写十六进制）。
pub fn wire_bytes(b: &[u8]) -> serde_json::Value {
    serde_json::json!(cmd_wire::files::WirePath::from(b))
}

/// 一格 `mtime_secs` 画出来是什么：列里那一格（短）与悬停 / 属性里的完整时间。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MtimeText {
    /// 当天写 `15:01`，今年写 `10-02`，往年写 `2025-12-31`。
    pub short: String,
    /// 完整时间（`2026-10-02 15:01:23`）。
    pub full: String,
}

/// 纯函数：`offset` 是那一刻本机时区与 UTC 的差（秒，含夏令时），`today` 是本机此刻的年月日。
pub fn mtime_text_at(secs: u64, offset: i64, today: (i64, i64, i64)) -> MtimeText {
    let t = secs as i64 + offset;
    let (y, m, d) = copy_core::civil_from_days(t.div_euclid(86_400));
    let rem = t.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let short = if (y, m, d) == today {
        format!("{hh:02}:{mm:02}")
    } else if y == today.0 {
        format!("{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    };
    MtimeText {
        short,
        full: format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}"),
    }
}

/// 按**本机**的本地时间画 —— 只给**这台自己的事**用：窗口自己记下的时刻（保存于 · 断线于 · 一件传输收尾于）
/// 与这台盘上的文件（上传撞名那张表「这台」那一格）。
///
/// ⚠ 那台后端送来的文件时间**不走这里**：列表 · 搜索结果 · 属性 · 预览 · 撞名表「那台」那一格都照抄后端写好的
/// `mtime_text` / `mtime_full`。
pub fn mtime_text(secs: u64) -> MtimeText {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let today =
        copy_core::civil_from_days((now + tz::offset_secs(now).unwrap_or(0)).div_euclid(86_400));
    mtime_text_at(secs, tz::offset_secs(secs as i64).unwrap_or(0), today)
}

/// 那台机器答出来的 home 能不能当**起点**用。
///
/// # 🔴 抽成纯函数的理由
///
/// 问 home 那一趟（窗口进程的 `proc::first_screen`，问后端 `files-home`）要一个起着的后端才跑得起来
/// ⇒ 那条路上**有逻辑的一段**就是这里（连同 [`home_from_reply`] 的解字节），抽出来它就有判据了。
///
/// # 为什么要判一道，而不是把回值直接拿去开窗
///
/// 入口那条命令吃的是一条**绝对路径**。后端那一侧对 home 也判过「必须是绝对路径」，
/// 但那是**对面的承诺**，不是我们的不变量（对面可能是一台旧后端、也可能是 Windows 上
/// 那种不以 `/` 起头的绝对路径）——
/// 空串 / 相对路径 / 一串空白，任一种直接拿去开窗都是「窗口出来了、里面是空的」，
/// 而那正是 [`super::entry::open_file_window`] 头注花一整节要避免的那一形。
/// ⇒ 不合格就**带着原文回错**，让 webview 那侧照旧弹它的失败提示。
///
/// ⚠ 末尾的 `/` 会被剥掉（根 `/` 除外）—— 那不是洁癖：
/// [`parent_dir`] 靠「回来的和给出去的相等」判「已经在顶上了」，
/// 而 `/srv/` 与 `/srv` 在那个算法里是两个不同的输入。
pub fn start_dir_from_home(answer: &str) -> Result<String, String> {
    let t = answer.trim();
    if t.is_empty() {
        return Err(copy_text("rsFilewinSource.home.empty", &[]).into());
    }
    if !t.starts_with('/') {
        return Err(copy_text(
            "rsFilewinSource.home.relative",
            &[("path", &t.to_string())],
        ));
    }
    let trimmed = t.trim_end_matches('/');
    Ok(if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    })
}

/// 问那台的家目录（`files-home`，按类型发、按类型解）→ 开窗的起点。窗口里问 home 的都经这一处。
pub async fn home_of(
    line: &Line,
    origin: &Origin,
    t: std::time::Duration,
) -> Result<String, String> {
    let r = ask_typed(line, origin, &cmd_wire::files::HomeReq {}, t)
        .await
        .map_err(|f| f.said)?;
    home_from_reply(r)
}

/// 一趟 `files-home` 的回包 → 开窗的起点。
///
/// 🔴 起点问的是后端 `files-home`（窗口进程经通道问，`proc::first_screen`），monitor 那一侧不碰 SFTP。
///
/// 两道：① `path` 解成字节 —— **不是合法 UTF-8 就拒**（窗口的路径是字符串，有损解码之后
/// 寻址不到那个目录，开出来的是别处）；② 能不能当起点，交 [`start_dir_from_home`]。
pub fn home_from_reply(r: cmd_wire::files::FilesPathReply) -> Result<String, String> {
    let s =
        String::from_utf8(r.path.0).map_err(|_| copy_text("rsFilewinSource.home.notUtf8", &[]))?;
    start_dir_from_home(&s)
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴**窗口进程只说 `call`** —— 经通道
// ═══════════════════════════════════════════════════════════════════════
//
// 窗口是独立前端；后端分原生后端与文件管理后端，monitor 打开文件管理器的前端。后端是给定的，没有退路。
//
// # 形状
//
// 窗口进程（`proc::child_main`）从 stdin 第一行拿到种子（含帧长），在自己的 stdin / stdout 上
// 起一个 `chan::client::Client`（[`Line`]）—— 起它的 monitor 在那对管子的另一头（父子管道，没有钥匙）。此后读侧与写面**每一条**都经 [`ask`] 说
// `call(origin, op, payload, budget)`，由 monitor 那一侧的路由器转给 `inbound_client`
// ——本机与远端同一条路。
//
// # 🔴 没有退路
//
// 问不到就是错，原话画在窗口上；窗口进程里列目录只有一条路（排序在后端）。守它的是
// `source_tests::listing_has_no_second_road_when_the_backend_refuses`（行为）＋
// `boundary_tests` 的窗口侧登记表（池子那条列目录命令不在表里 ⇒ 写上就红）。
//
// # ⚠ 各条路走哪
//
// - **跨机传输**：上传 / 下载 / 复制到另一台是寻址本机的一组 `transfer-copy` ＋ 订阅 `files-batch/<票>`（撤 = `files-batch-stop`），
//   编排与 SFTP 住本机常驻后端；往 OS 拖出去不做。窗口进程零 SFTP。
//   「读一份文本进编辑器」问后端 `files-read-text`。
// - **同机复制**问后端 `files-copy`。
// - **开窗时解 home** 问后端 `files-home`（[`home_from_reply`]），monitor 那一侧开窗不拨 SFTP。
// - **`Row` 持字符串不持字节**，字节走**发出去的那一跳**：
//   窗口记住当前目录的字节（`FileWindow::cwd_raw`）与行上名字的字节（`Listed::raw_name`），发路径一律经 [`RemotePath`]
//   （进目录 · 列目录 · 上一级 · 读 / 存文本 · 复制 · 算大小 · 新建 · 写面的根）；显示照旧用有损串。
//   仍然做不了、会出声的：有损目录里上传 / 搜索 / 开终端 / 书签。

/// 一趟 `files-ls` 回来的 `entries` 里的**一条** → [`Listed`]。
///
/// 🔴 **抽成具名函数是为了让它可判**：[`list_via_backend`] 整条路要一条通道才跑得起来，
/// 而那条路上**唯一有逻辑的一段**就是这里。
///
/// # 🔴 逐格说明 —— **这张表是一条判据，不是散文**
///
/// | 后端送的 | 窗口这一行的哪一格 | 怎么来的 |
/// |---|---|---|
/// | `path`（字符串 或 `{"b16":…}`） | `path` · `name` · `lossy_name` | 先解成**字节**，再取尾段作名字；有损与否看那串字节 |
/// | `kind`（`dir`/`file`/`symlink`/`other`） | `is_dir` ＋ `link` | `dir` ⇒ 目录；`symlink` ⇒ 链接。**两格，不是一格** |
/// | `link_dir`（只在链接上、可能缺） | `link_dir` | 链接且它为真 ⇒ 指向目录；缺 ⇒ 否 |
/// | `size`（可能缺） | `size` | 缺就是 0（同本机那条：读不到 stat 不整趟失败） |
/// | `size_text`（可能缺） | `size_text` | 后端写好的大小那一格，原样；缺 ⇒ `None` |
/// | `mtime_secs`（可能缺） | `mtime_secs` | 原样带上来；缺就是 `None`（＝**没送**，不是 1970） |
/// | `mtime_text`（可能缺） | `mtime_text` | 后端按那台本地钟写好的短写法，原样；缺 ⇒ `None` |
/// | `mtime_full`（可能缺） | `mtime_full` | 后端按那台本地钟写好的完整写法，原样；缺 ⇒ `None` |
/// | `class` | `class` | 原样（画哪种图标 · 点不点得进去：[`Listed::opens_as_dir`]） |
/// | `shown`（头与每一行） | —— | 窗口不读（给手机选目录那一屏的显示形） |
/// | `type_text` | `type_text` | 原样（「类型」那一列） |
/// | `gates`（目录缺） | `gates` | 原样（预览 · 看图 · 编辑许不许、许多少） |
/// | `order` · `up` | 那一屏有几行 | 照回包换进手上那一份（[`Shown`]），由 [`screen_of`] 照先后摊开 |
/// | `truncated` | 界面上那句「只拿到了前 N 条」 | 同上 |
/// | `unreadable` | 界面上那句「有 n 项读不出来」 | 同上；缺 ⇒ 0 |
///
/// 🔴 守这张表的是 `source_tests::the_field_by_field_table_between_backend_and_window_is_a_judge`：
/// 左栏的人群**现读** `src/backend/files/mod.rs` 里 `files.ls` 那条能力声明的 `fields`
/// （两侧不同源），与表里声明的那一摞**逐格相等**；右栏那句「怎么来的」
/// 每一行各挂一条**行为**断言。加一格 / 丢一格 / 说法漂了，三种都红。
pub fn row_from_ls_entry(v: &cmd_wire::files::LsEntry) -> Listed {
    // 画链接 · 断链 · 有损名照那一行那几格（`kind` · `link_dir` · `link_to` · 名字的字节）；能做什么由本机后端照原行判（`files-menu`）。
    // 名字那一段的字节留着寻址（有损名的路径由窗口按当前目录的字节拼）。
    use cmd_wire::files::{FileKind, LinkTo};
    let link = v.kind == FileKind::Symlink;
    let lossy = std::str::from_utf8(v.name.bytes()).is_err();
    Listed {
        row: Row {
            path: String::from_utf8_lossy(&v.path.0).to_string(),
            is_dir: v.kind == FileKind::Dir,
            size: v.size.unwrap_or(0),
            lossy_name: lossy,
            name: String::from_utf8_lossy(v.name.bytes()).into_owned(),
        },
        link,
        link_dir: link && v.link_dir == Some(true),
        // 🔴 原样带上来。缺了就是 `None`（＝**后端没送**）—— 不许兜底成 0，
        //    那是 1970-01-01，一个看起来很像真读数的假时间。
        mtime_secs: v.mtime_secs,
        mtime_text: v.mtime_text.clone(),
        mtime_full: v.mtime_full.clone(),
        raw_name: lossy.then(|| v.name.0.clone()),
        link_broken: link && v.link_to == Some(LinkTo::Missing),
        class: v.class,
        type_text: v.type_text.clone(),
        gates: v.gates.clone(),
        size_text: v.size_text.clone(),
    }
}

/// 一屏之外的那几格：被截断了吗 · 有几项读不出来（后端照数、没列出）· 是不是家 / 顶。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cut {
    pub truncated: bool,
    pub unreadable: u64,
    /// 目录里一共读到几项（含没回送的；后端没给 ⇒ 0）。
    pub total: u64,
    /// 列的这一级就是那台的家目录（后端判的 `at_home`；左栏「家」那一项照它亮）。
    pub at_home: bool,
    /// 列的这一级到顶了（后端答的 `up` 为空；左栏「根」那一项照它亮）。
    pub at_root: bool,
    /// 表头箭头画在哪一列、朝哪（回包头答的 `by` · `descending`）；头还没到过 ⇒ `None`，不画箭头。
    pub caret: Option<Caret>,
}

/// 表头上的箭头：那台实际排的那一列 ＋ 那一列是不是从大到小（都照回包头，窗口不自己算）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caret {
    pub by: SortBy,
    pub descending: bool,
}

/// 一个标签页手上那一份目录：上一次回包的记号 ＋ 排好的那几行（线上那一形，[`cmd_wire::sync::Hand`]）。
/// 不带记号问 ⇒ 整份；带着它问 ⇒ 没变 / 只给变了的那几行（判在那台后端，这里只照回包机械地换）。
pub type Shown = cmd_wire::sync::Hand<cmd_wire::files::LsHead, cmd_wire::files::LsEntry>;

/// 列的这一级在那台上的绝对路径（回包面包屑最后一段，那台展开 `~`、按它的规则切好的）；还没到过头 · 不是 UTF-8 ⇒ `None`。
pub fn landed_dir(h: &Shown) -> Option<String> {
    let last = h.head.as_ref()?.info.crumbs.last()?;
    String::from_utf8(last.path.0.clone()).ok()
}

/// 手上那一份 → **一屏**（后端排好的先后照摆）＋ 截断那几格（头还没到过 ⇒ 全是 0）。
pub fn screen_of(h: &Shown) -> (Vec<Listed>, Cut) {
    let cut = h.head.as_ref().map_or_else(Cut::default, |hd| Cut {
        truncated: hd.info.truncated,
        unreadable: hd.info.unreadable,
        total: hd.info.total,
        at_home: hd.info.at_home,
        at_root: hd.info.up.is_none(),
        caret: Some(Caret {
            by: hd.info.by,
            descending: hd.info.descending,
        }),
    });
    (h.in_order().map(row_from_ls_entry).collect(), cut)
}

/// 问**那台机器上的后端**要一个目录（整份：不带记号）。
///
/// 回 `(行, 被截断了吗 · 有几项读不出来)`。⚠ 截断要**画出来** —— 「目录里就这么多」与
/// 「后端截断了」在屏幕上长得一样，而那正是本仓的头号病形。
///
/// ⚠ 本函数**自己没有逻辑**，判的那一段住 [`row_from_ls_entry`]。
pub async fn list_via_backend(
    line: &Line,
    origin: &Origin,
    dir: &str,
    limit: usize,
    sort: Sort,
) -> Result<(Vec<Listed>, Cut), String> {
    let mut h = Shown::default();
    let p = list_via_backend_at(
        line,
        origin,
        cmd_wire::files::WirePath::from(dir),
        limit,
        sort,
        None,
    )
    .await
    .map_err(|f| f.said)?;
    h.apply(p);
    Ok(screen_of(&h))
}

/// 〔有损名全寻址〕问一个目录，目录按字节给（[`RemotePath::path`]），带着手上那一份的记号（`None` ⇒ 整份）。
/// 按 `LsReq` 发、按回包类型解；回包交调用方照它换手上那一份（[`Shown::apply`]）。
pub async fn list_via_backend_at(
    line: &Line,
    origin: &Origin,
    dir: cmd_wire::files::WirePath,
    limit: usize,
    sort: Sort,
    held: Option<cmd_wire::sync::Held>,
) -> Result<cmd_wire::files::LsReply, Failed> {
    let req = cmd_wire::files::LsReq {
        path: dir,
        limit: Some(limit as u64),
        dirs_only: None,
        sort: Some(sort),
        held,
        range: None,
    };
    ask_typed(line, origin, &req, std::time::Duration::from_secs(20)).await
}

/// 目录打不开的那几个码（后端 `files-ls` 拒时给的）：是 ⇒ 列表顶上那一条带出路「回上一级 · 回主目录」；那一句照后端写好的画，不按码另挑。
pub const OPEN_FAIL_CODES: &[&str] = &["not_found", "denied", "not_dir", "unreadable"];

/// 这一趟列不出来是不是「目录打不开」（给两颗出路）；别的码（入参不对 · 通道没走通 …）只说那一句。
pub fn opens_fail(code: &str) -> bool {
    OPEN_FAIL_CODES.contains(&code)
}

/// 一屏最多要多少行。
///
/// ⚠ 后端那侧有自己的默认值（`files/mod.rs::DEFAULT_LIMIT`）；这里**显式给**，
/// 因为「一屏多少」是调用方的事（同那条「机制在后端 ·
/// 节拍归调用方」的形）。取这个数是因为窗口是**虚拟滚动**的
/// （`ScrollArea::show_rows`），一次拿多少不影响帧时，只影响一次往返的大小。
pub const LS_LIMIT: usize = 50_000;

/// 列一个目录 —— **只问后端**（没有退路）。
///
/// 回手上那一份（整份，带着记号：开窗之后头一次重问就只拿变了的那几行）。问不到 ⇒ 那句原话（[`comms_inward::chan::said::said`] 翻过的）原样交出去。
pub async fn list_dir(
    line: &Line,
    source: &Source,
    dir: &str,
    sort: Sort,
) -> Result<Shown, String> {
    let mut h = Shown::default();
    let p = list_via_backend_at(
        line,
        &source.origin(),
        cmd_wire::files::WirePath::from(dir),
        LS_LIMIT,
        sort,
        None,
    )
    .await
    .map_err(|f| f.said)?;
    h.apply(p);
    Ok(h)
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴窗口进程够后端的**唯一一处** `call`
// ═══════════════════════════════════════════════════════════════════════

/// 寻址键：`chan::wire` 再导出的全仓那一个「哪台机器」类型。
pub use comms_inward::chan::wire::Origin;

/// 窗口进程手里那条通道（克隆便宜，共享同一条回环连接）。
pub type Line = comms_inward::chan::client::Client;

/// 发一条命令、拿它的 `data`（JSON）。
///
/// 🔴 **窗口进程里说 `call` 的只有这一处**：
/// 期限由调用方给（`t`），这里只把它换成**绝对时刻**的 `Budget`——
/// 一个期限常量都不住在这儿。
///
/// ⚠ 载荷在这里才被当成 JSON：通道对它不透明（`C1`），宿主那一侧把它原样交给
/// `inbound_client`，回来的那一份也是 JSON。`null` ⇒ 当成契约不符（写面与读侧那几条
/// 都回一个对象）。
pub async fn ask(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: &serde_json::Value,
    t: std::time::Duration,
) -> Result<serde_json::Value, String> {
    ask_coded(line, origin, cmd, args, t)
        .await
        .map_err(|f| f.said)
}

pub(crate) use cmd_wire::failed::Failed;

/// 这一侧的错（问之前拼不出参数 · 回来的读不懂）：窗口进程写详情（时刻 · 机器 · 本机 · 命令 · 原话）。
pub(crate) fn failed_here(said: String, origin: &Origin, cmd: &str, raw: Option<&str>) -> Failed {
    use copy_core::detail::{Detail, Label};
    Failed {
        detail: Detail::new()
            .item(Label::At, now_stamp())
            .item(Label::Machine, origin.to_string())
            .maybe(Label::Local, local_line_for(origin))
            .item(Label::Command, cmd)
            .maybe(Label::Raw, raw)
            .render(),
        ..Failed::local(said)
    }
}

/// 复制详情「本机」那一项的值（开窗种子交来的，monitor 算好；`proc::child_main` 起来时记一次）。
static LOCAL_LINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// 记下「本机」那一项的值（开窗种子里那一格；只记一次）。
pub(crate) fn set_local_line(line: &str) {
    LOCAL_LINE.get_or_init(|| line.to_string());
}

/// 看 `origin` 那台时窗口自己写的详情要不要带「本机」那一行：远端 ⇒ 带（值是种子交来的）；本机 ⇒ 不带（出错的就是本机）。
fn local_line_for(origin: &Origin) -> Option<String> {
    (!origin.is_local())
        .then(|| LOCAL_LINE.get().cloned())
        .flatten()
        .filter(|l| !l.trim().is_empty())
}

/// 此刻（本机本地时间 ＋ 偏移）。
fn now_stamp() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    copy_core::detail::stamp(t, tz::offset_secs(t).unwrap_or(0))
}

/// 一趟 `call` 没成的复制详情：对端拒了、那台写了详情 ⇒ 原样（远端时「本机」那一行 monitor 的通道宿主已补）；
/// 别的（通道没走通 · 对端不认 · 本侧）⇒ 窗口进程写时刻 · 机器（没够着那台标「未连接」）· 本机 · 命令 · 断在 · 码
/// （取法住通信层 `HopFacts`，排法住 `copy_core::detail::channel`，与 monitor 壳同一份）。
pub fn detail_of(at: &Origin, cmd: &str, e: &comms_inward::chan::wire::CallError) -> String {
    let origin = &at.to_string();
    use comms_inward::chan::wire::{CallError, PeerFault};
    // 拒绝体读不懂（不是 JSON）⇒ 那段原文进「原话」（句子只说被拒，[`comms_inward::chan::said::said`]）。
    let mut raw_body: Option<String> = None;
    if let CallError::Peer {
        why: PeerFault::Refused { body },
    } = e
    {
        match serde_json::from_slice::<serde_json::Value>(&body.0) {
            Ok(v) => {
                let wrote = v.get("detail").and_then(|d| d.as_str()).unwrap_or_default();
                if !wrote.trim().is_empty() {
                    return wrote.to_string();
                }
            }
            Err(_) => raw_body = Some(String::from_utf8_lossy(&body.0).into_owned()),
        }
    }
    let f = e.hop_facts();
    copy_core::detail::channel(
        &now_stamp(),
        origin,
        f.unreached,
        local_line_for(at).as_deref(),
        cmd,
        f.hop.as_deref(),
        &f.code,
    )
    .maybe(copy_core::detail::Label::Raw, raw_body.as_deref())
    .render()
}

/// 与 [`ask`] 同一件事，失败时**把对端的码一起交出来**（[`Failed`]）。
///
/// 🔴 **窗口进程里说 `call` 的仍然只有一处 —— 就是这里**：[`ask`] 是它的一层薄壳
/// （只把码丢掉）。「期限由调用方给、这里只换成绝对时刻」那一条同 [`ask`] 头注。
pub async fn ask_coded(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: &serde_json::Value,
    t: std::time::Duration,
) -> Result<serde_json::Value, Failed> {
    ask_coded_cancellable(
        line,
        origin,
        cmd,
        args,
        t,
        comms_inward::chan::wire::CancelToken::new(),
    )
    .await
}

/// 照类型问一条：名字 · 入参 · 回包都挂在入参类型上（`cmd_wire::Call`，后端登记那一行用的是同一份）。
/// 入参按类型发，回包按类型解；解不出 ⇒ 这一侧的那一句（解码器原话进复制详情）。两端形状对不对得上，由握手时的形状指纹先管。
pub async fn ask_typed<C: cmd_wire::Call>(
    line: &Line,
    origin: &Origin,
    req: &C,
    t: std::time::Duration,
) -> Result<C::Reply, Failed> {
    let args = serde_json::to_value(req).map_err(|e| {
        failed_here(
            copy_text("reason.code.bug", &[]),
            origin,
            C::NAME,
            Some(&e.to_string()),
        )
    })?;
    let d = ask_coded(line, origin, C::NAME, &args, t).await?;
    reply_of::<C>(d).map_err(|e| unparsable(origin, C::NAME, &e))
}

/// `cmd` 那一条的回包解不出来（`why` ＝ 解码器那一句，进复制详情不进句子）。
pub fn unparsable(origin: &Origin, cmd: &str, why: &str) -> Failed {
    failed_here(
        copy_text("reason.content.unparsable", &[]),
        origin,
        cmd,
        Some(why),
    )
}

/// 一组只一项的回包 → 那一项：做成了 ⇒ 那一项的结局；没做成 ⇒ 那一项的码与那一句（复制详情是那一项的原话）。
pub fn one_of(r: cmd_wire::batch::BatchReply) -> Result<cmd_wire::batch::BatchOutcome, Failed> {
    use cmd_wire::batch::ItemState;
    let Some(o) = r.outcomes.into_iter().next() else {
        return Err(Failed::local(r.said));
    };
    if o.state == ItemState::Done {
        return Ok(o);
    }
    let detail = o.detail.clone().unwrap_or_default();
    let (said, rest) = match detail.split_once('\n') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None if detail.is_empty() => (r.said, String::new()),
        None => (detail, String::new()),
    };
    Err(Failed {
        code: Failed::code_or_fallback(o.code.or_else(|| {
            serde_json::to_value(o.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
        })),
        said,
        detail: rest,
        data: None,
    })
}

/// 一份回包按那条命令的回包类型解（解不出 ⇒ 解码器那一句）。
pub fn reply_of<C: cmd_wire::Call>(d: serde_json::Value) -> Result<C::Reply, String> {
    serde_json::from_value(d).map_err(|e| e.to_string())
}

/// 〔「可撤」〕同 [`ask_coded`]，撤单手柄由调用方握着（按内容搜那颗「停」拨它）：
/// 拨下去 ⇒ 这一问当场回收（`Ours{Cancelled}`），通道把撤单转给那台后端（对端停不停是尽力）。
/// 🔴 窗口进程里说 `call` 的仍然只有一处 —— 就在本函数里（[`ask_coded`] 是它的薄壳）。
pub async fn ask_coded_cancellable(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: &serde_json::Value,
    t: std::time::Duration,
    cancel: comms_inward::chan::wire::CancelToken,
) -> Result<serde_json::Value, Failed> {
    ask_raw(line, origin, cmd, args, t, cancel)
        .await
        .map_err(|r| r.failed)
}

/// 一组一条那几条（复制 · 删除 · 改权限 · 两台之间的复制）：同 [`ask_coded`]，没成时多交两样 ——
/// 对端带的 `data`（`exists` ⇒ 同名清单）与「通道那一跳断了」（那台可能已经在做 ⇒ 同一张票再问）。
pub async fn ask_batch(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: &serde_json::Value,
    t: std::time::Duration,
) -> Result<serde_json::Value, batch_core::Rebuff> {
    ask_raw(
        line,
        origin,
        cmd,
        args,
        t,
        comms_inward::chan::wire::CancelToken::new(),
    )
    .await
}

/// 🔴 窗口进程里说 `call` 的那一处（[`ask_coded_cancellable`] · [`ask_batch`] 都是它的薄壳）。
async fn ask_raw(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: &serde_json::Value,
    t: std::time::Duration,
    cancel: comms_inward::chan::wire::CancelToken,
) -> Result<serde_json::Value, batch_core::Rebuff> {
    use comms_inward::chan::wire::{Body, Budget, CallError, Comms, Op};
    let plain = |failed: Failed| batch_core::Rebuff { failed, cut: false };
    let budget = Budget::new(std::time::Instant::now() + t, cancel);
    let payload = Body(serde_json::to_vec(args).map_err(|e| {
        plain(failed_here(
            copy_text("reason.code.bug", &[]),
            origin,
            cmd,
            Some(&e.to_string()),
        ))
    })?);
    let op = Op(cmd.to_string());
    let body = line
        .call(origin, &op, payload, budget)
        .await
        .map_err(|e| batch_core::Rebuff {
            failed: Failed {
                code: Failed::code_or_fallback(refused_code(&e)),
                said: said_here(&e, origin),
                detail: detail_of(origin, cmd, &e),
                data: refused_data(&e),
            },
            cut: matches!(e, CallError::Hop { .. }),
        })?;
    let v: serde_json::Value = serde_json::from_slice(&body.0).map_err(|e| {
        plain(failed_here(
            copy_text("rsFilewinSource.ask.unreadable", &[]),
            origin,
            cmd,
            Some(&e.to_string()),
        ))
    })?;
    if v.is_null() {
        return Err(plain(failed_here(
            copy_text("reason.content.unparsable", &[]),
            origin,
            cmd,
            None,
        )));
    }
    Ok(v)
}

/// 订一条只出快照格的流（`files-batch/<票>`）到它关掉为止，每一格的 JSON 交给 `on`。撤 ＝ 调用方丢掉这个 future。
pub async fn watch_cells(
    line: &Line,
    origin: &Origin,
    kind: &str,
    mut on: impl FnMut(serde_json::Value),
) {
    use comms_inward::chan::wire::{Comms, Item, Kind, Sub};
    use futures::StreamExt as _;
    let mut sub = line.subscribe(origin, &Kind(kind.to_string()), None, WATCH_CREDIT);
    while let Some(item) = sub.next().await {
        match item {
            Item::Frame { body, .. } => {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body.0) {
                    on(v);
                }
            }
            Item::Closed { .. } => return,
            Item::Gap { .. } | Item::Seen { .. } | Item::Unseen { .. } => {}
        }
        sub.want(1);
    }
}

/// 等那台连回来（那台的成品说「连着」为止；那条流不在 · 和 monitor 断了 ⇒ 当场回，让调用方照旧再问一次）。
pub async fn link_up(line: &Line, origin: &Origin) {
    watch_link_until(line, origin, |s| {
        matches!(s, LinkSeen::Up | LinkSeen::Severed)
    })
    .await
}

/// 「那台怎么了」那条流：本机后端那一台的 `changed/machine`（各台的状态成品住本机后端，`key` ＝ 那台的名字）。
const MACHINE_KIND: &str = "changed/machine";

/// 那台的状态成品 ⇒ 那台此刻的样子（纯函数）：连着 ⇒ `Up`；正在连 · 装 · 换 ⇒ `Reconnecting`；别的 ⇒ `Down`。
pub fn seen_of_state(state: cmd_wire::machine::MachineStateKind) -> LinkSeen {
    use cmd_wire::machine::MachineStateKind as K;
    match state {
        K::Up => LinkSeen::Up,
        K::Connecting | K::Installing | K::Updating => LinkSeen::Reconnecting,
        _ => LinkSeen::Down,
    }
}

/// `changed/machine` 那条流的一格 ⇒ `origin` 那台此刻的样子（纯函数）；`None` ＝ 这一格不说它。
/// 「看得见 / 看不见」说的是本机后端那一条：本机那台照它（[`link_seen`]）；别台只在看不见时跟着断（本机后端不在 ⇒ 问不到那台）。
/// 带状态的一格：`key` 是那台的名字 ⇒ 照它的成品（[`seen_of_state`]）。
pub fn machine_seen(origin: &Origin, item: &comms_inward::chan::wire::Item) -> Option<LinkSeen> {
    use comms_inward::chan::wire::Item;
    match item {
        Item::Frame { body, .. } if !origin.is_local() => {
            let v: serde_json::Value = serde_json::from_slice(&body.0).ok()?;
            if v.get("key").and_then(serde_json::Value::as_str) != origin.host_name() {
                return None;
            }
            let state = serde_json::from_value(v.get("body")?.get("state")?.clone()).ok()?;
            Some(seen_of_state(state))
        }
        Item::Seen { .. } if !origin.is_local() => None,
        other => link_seen(other),
    }
}

/// 别台此刻的样子：问本机后端一次那台的成品（`machine-read`）；问不到 · 表里没有它 ⇒ `None`（窗口当作说不清）。
async fn machine_now(line: &Line, origin: &Origin) -> Option<LinkSeen> {
    let got = ask_typed(
        line,
        &Origin::local(),
        &cmd_wire::machine::MachineReadReq {},
        std::time::Duration::from_secs(10),
    )
    .await
    .ok()?;
    Some(seen_of_state(got.machines.get(&origin.to_string())?.state))
}

/// 订「那台怎么了」那条流，每一格交给 `on`，`on` 回真就停；流关了 ⇒ 停。别台先问一次此刻的成品（那条流只在变了时才来一格）。
async fn watch_link_until(line: &Line, origin: &Origin, mut on: impl FnMut(LinkSeen) -> bool) {
    use comms_inward::chan::wire::{Comms, Item, Kind, Sub};
    use futures::StreamExt as _;
    let mut sub = line.subscribe(
        &Origin::local(),
        &Kind(MACHINE_KIND.to_string()),
        None,
        WATCH_CREDIT,
    );
    if !origin.is_local() {
        if let Some(s) = machine_now(line, origin).await {
            if on(s) {
                return;
            }
        }
    }
    while let Some(item) = sub.next().await {
        if let Item::Closed { .. } = item {
            return;
        }
        if let Some(s) = machine_seen(origin, &item) {
            if on(s) {
                return;
            }
        }
        sub.want(1);
    }
}

/// 订阅流一开始给的 credit（格数）。之后每吃一格还一格。
///
/// ⚠ 格子是**整份快照**（那一侧合并中间几格不丢信息）⇒ credit 小也不会漏读数，
/// 它只决定「窗口一口气最多攒几格没画」。
pub const WATCH_CREDIT: u32 = 8;

/// 分块读写那一圈（`chunk_core`：存盘 · 看图）问那台的那一口：经通道 [`ask_typed`]，每问 [`CHUNK_BUDGET`]。
pub struct Over<'a> {
    pub line: &'a Line,
    pub origin: &'a Origin,
}

/// 一块的往返上限（与写面一趟同一个上界）。
pub const CHUNK_BUDGET: std::time::Duration = super::writeops::WRITE_BUDGET;

impl chunk_core::Ask for Over<'_> {
    type Err = Failed;
    async fn ask<C: cmd_wire::Call + Sync>(&self, req: &C) -> Result<C::Reply, Self::Err> {
        ask_typed(self.line, self.origin, req, CHUNK_BUDGET).await
    }
}

/// 那台此刻连没连着（本机后端那台的成品说了算，窗口不猜）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSeen {
    Up,
    /// 刚断、正在重连。
    Reconnecting,
    /// 重连一轮没连上（或从没连上）。
    Down,
    /// 窗口自己和 monitor 之间那对管子断了（monitor 退了 / 重启了）：没有谁在重连，这扇窗接不回去
    /// （通道是起它的那一方给的父子管道，没有可再拨的地址）。
    Severed,
}

/// 一趟命令没成 ⇒ 给人看的那一句：断在窗口与 monitor 之间那对管子（[`monitor_hop`]）⇒ 说与 cc-monitor 断开，不说那台离线；
/// 别的照通道那一句（[`comms_inward::chan::said::said`]）。
pub fn said_here(e: &comms_inward::chan::wire::CallError, origin: &Origin) -> String {
    use comms_inward::chan::wire::{CallError, HopFault, Reach};
    match e {
        CallError::Hop {
            at,
            reach,
            why: HopFault::Unreachable | HopFault::Dropped,
        } if monitor_hop(at) => match reach {
            Reach::NotSent => copy_text("rsFilewinSource.hop.severed", &[]),
            Reach::Sent | Reach::Unknown => copy_text("rsFilewinSource.hop.severedUnknown", &[]),
        },
        _ => comms_inward::chan::said::said(e, origin),
    }
}

/// 这一跳是不是窗口与 monitor 之间那对管子（第 0 跳：通道客户端在那一跳原位出「看不见」）。断线条（[`link_seen`]）与失败那一句（[`said_here`]）同读这一处。
fn monitor_hop(at: &comms_inward::chan::wire::HopId) -> bool {
    at.idx == 0
}

/// 那条流的一格 ⇒ 那台此刻的样子（纯函数）；`None` ＝ 这一格不说连没连着。
/// 「看不见」出在第 0 跳 ⇒ 是窗口与 monitor 之间那对管子断了（通道客户端在那一跳原位出这一格），不是那台在重连。
pub fn link_seen(item: &comms_inward::chan::wire::Item) -> Option<LinkSeen> {
    use comms_inward::chan::wire::{HopFault, Item};
    match item {
        Item::Seen { .. } => Some(LinkSeen::Up),
        Item::Unseen { at, .. } if monitor_hop(at) => Some(LinkSeen::Severed),
        Item::Unseen {
            why: HopFault::Dropped,
            ..
        } => Some(LinkSeen::Reconnecting),
        Item::Unseen { .. } => Some(LinkSeen::Down),
        Item::Frame { .. } | Item::Gap { .. } | Item::Closed { .. } => None,
    }
}

/// 订「那台怎么了」那条流（本机后端的 `changed/machine`，[`machine_seen`]）到它关掉为止；每一格交给 `on`。
/// 对端说「没有这条流」/ 流关了 ⇒ 回（窗口当作说不清：不画断线条）。
pub async fn watch_link(line: &Line, origin: &Origin, mut on: impl FnMut(LinkSeen)) {
    watch_link_until(line, origin, |s| {
        on(s);
        false
    })
    .await
}

/// 订「那台的计划变了」那条流（[`super::plan::PLAN_CHANGED_KIND`]）到它关掉为止；来一格（或丢了几格）就调一次 `on`。
/// 对端说「没有这条流」/ 流关了 ⇒ 回（窗口照旧只在列目录时问一次）。
pub async fn watch_plan(line: &Line, origin: &Origin, mut on: impl FnMut()) {
    use comms_inward::chan::wire::{Comms, Item, Kind, Sub};
    use futures::StreamExt as _;
    let mut sub = line.subscribe(
        origin,
        &Kind(super::plan::PLAN_CHANGED_KIND.to_string()),
        None,
        WATCH_CREDIT,
    );
    while let Some(item) = sub.next().await {
        match item {
            Item::Frame { .. } | Item::Gap { .. } => on(),
            Item::Seen { .. } | Item::Unseen { .. } => {}
            Item::Closed { .. } => return,
        }
        sub.want(1);
    }
}

/// 那台正被浏览的目录有了动静那条流：后端的 `changed` 帧、主题 `files_dir`（monitor 按主题原样扇出；格体 `{key}`）。
pub const DIR_CHANGED_KIND: &str = "changed/files_dir";

/// 那台哪几个目录动过（整扇窗一份；[`DIR_CHANGED_KIND`] 那条流写进来，窗口每帧取走一次）。
#[derive(Clone, Default)]
pub struct DirWatch(std::sync::Arc<std::sync::Mutex<DirWatchInner>>);

#[derive(Default)]
struct DirWatchInner {
    moved: Moved,
    /// 已经起了那条订阅。
    watching: bool,
}

/// 上次取走之后动过的那几个目录（[`cmd_wire::files::WirePath::key`]）；`all` ＝ 丢过几格、说不清是哪几个。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub dirs: std::collections::BTreeSet<String>,
    pub all: bool,
}

impl Moved {
    /// 这个目录要不要重问。
    pub fn has(&self, dir: &RemotePath) -> bool {
        self.all || self.dirs.contains(&dir.path().key())
    }
}

impl DirWatch {
    fn lock(&self) -> std::sync::MutexGuard<'_, DirWatchInner> {
        match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// 流上来一格：带 `key` ⇒ 记那个目录；不带 · 丢过几格（`None`）⇒ 说不清是哪几个。
    pub fn note(&self, key: Option<String>) {
        let mut g = self.lock();
        match key {
            Some(k) => {
                g.moved.dirs.insert(k);
            }
            None => g.moved.all = true,
        }
    }

    /// 取走攒下的那几个（取完清空）。
    pub fn take(&self) -> Moved {
        std::mem::take(&mut self.lock().moved)
    }

    /// 起那条订阅（整扇窗一次；流关了就停）。
    pub fn watch(
        &self,
        rt: &tokio::runtime::Handle,
        line: Line,
        origin: Origin,
        ctx: Option<egui::Context>,
    ) {
        {
            let mut g = self.lock();
            if g.watching {
                return;
            }
            g.watching = true;
        }
        let me = self.clone();
        rt.spawn(async move {
            use comms_inward::chan::wire::{Comms, Item, Kind, Sub};
            use futures::StreamExt as _;
            let mut sub = line.subscribe(
                &origin,
                &Kind(DIR_CHANGED_KIND.to_string()),
                None,
                WATCH_CREDIT,
            );
            while let Some(item) = sub.next().await {
                match item {
                    Item::Frame { body, .. } => me.note(
                        serde_json::from_slice::<serde_json::Value>(&body.0)
                            .ok()
                            .and_then(|v| v.get("key")?.as_str().map(str::to_string)),
                    ),
                    Item::Gap { .. } => me.note(None),
                    Item::Seen { .. } | Item::Unseen { .. } => {}
                    Item::Closed { .. } => return,
                }
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
                sub.want(1);
            }
        });
    }
}

/// 「重新连接」：叫醒本机后端手上那台的常驻流（`machine-wake`：断着的不等退避马上再接）。连没连上看 [`watch_link`]；
/// 本机那一台没有可叫醒的（壳自己接本机后端）。这一下本身没送到 ⇒ 记一行（断线条照旧摆着，人可以再点）。
pub async fn kick_link(line: &Line, origin: &Origin) {
    if origin.is_local() {
        return;
    }
    let req = cmd_wire::machine::MachineWakeReq {
        machine: origin.to_string(),
    };
    if let Err(why) = ask_typed(
        line,
        &Origin::local(),
        &req,
        std::time::Duration::from_secs(5),
    )
    .await
    {
        tracing::warn!("machine-wake not delivered: {}", why.said);
    }
}

/// 对端拒了的那一形里，它带的 `data`（`exists` ⇒ 同名清单）。别的形一律 `None`。
pub fn refused_data(e: &comms_inward::chan::wire::CallError) -> Option<serde_json::Value> {
    use comms_inward::chan::wire::{CallError, PeerFault};
    match e {
        CallError::Peer {
            why: PeerFault::Refused { body },
        } => serde_json::from_slice::<serde_json::Value>(&body.0)
            .ok()?
            .get("data")
            .cloned(),
        _ => None,
    }
}

/// 对端拒了的那一形里，它给的那个码。别的形一律 `None`（**不猜**）。
pub fn refused_code(e: &comms_inward::chan::wire::CallError) -> Option<String> {
    use comms_inward::chan::wire::{CallError, PeerFault};
    match e {
        CallError::Peer {
            why: PeerFault::Refused { body },
        } => serde_json::from_slice::<serde_json::Value>(&body.0)
            .ok()?
            .get("code")?
            .as_str()
            .map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/source_tests.rs"]
mod tests;
