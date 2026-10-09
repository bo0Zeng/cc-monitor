//! `24e` 数据面：**列一个目录**。只有一侧 —— 远端；经通道问那台机器上的后端。
//!
//! # 🪦 本机侧退役（2026-09-23）：「本机」那一侧存在过，现在不在了
//!
//! 〔散文墓碑〕曾经这个模块有两侧：`Source` 是个两格枚举（`Local` / `Remote`），
//! 列目录本机走 `read_dir`、远端走 SFTP；`FileWindow` 上有 `go_local` / `go_remote`
//! 那一对跳转与 `return_label` 那颗按钮标签，工具栏上一颗「本机」⇄「回 <机器名>」。
//!
//! 判据侧**随功能一起走掉的那七条**，逐条点名（点名是有账的：每一行都挂着
//! `PROSE_NAME_TOMBSTONE` 标记并登记进 `structural_scan_tests::TOMBSTONED`）：
//!
//! - 〔散文墓碑〕`a_window_that_started_local_has_nowhere_to_go_back_to`（阴性对照：没有来处时那条路不该通）
//! - 〔散文墓碑〕`the_button_shows_up_exactly_when_the_jump_would_work`（按钮在不在 ＝ 跳得成不成）
//! - 〔散文墓碑〕`a_real_click_on_the_go_back_button_walks_the_whole_chain`（真合成一次点击，不跳任何一跳）
//! - 另外四条专测「本机侧出声拒」的（复制 / 四条写 / 拖入 / 往外拖），
//!   以及两条专测本机列目录的（列得出一屏 / 列不出要出声）。
//!   ⚠ 这六条**刻意不逐条点名**：它们的名字里逐字带着 `the_local_side_…`，
//!   点出来只是把同一句话说六遍；而上面那三条各自钉着一件**不重复**的事，
//!   点名是为了让「随它们一起走掉的检出力」有一份可读的账。
//!
//! ## 为什么退役 —— 两句，都是引的，不是推的
//!
//! ① 用户 2026-09-23 逐字裁决：「**本地文件句柄不用. 本地不需要文件管理器**」。
//!
//! ② 而这条裁决**与本仓既有的设计裁决一致，不是新方向**。`src/doc/INVARIANTS.md`
//!    的「§40 追加（用户 2026-07-29）：本地的功能要和远端一致」那一节里，
//!    「**天然不对称白名单（不是欠账，不必补）**」第一条逐字就是：
//!
//!    > **SFTP 文件面板** —— 本地有操作系统的文件管理器，不需要它
//!
//!    ⇒ 本机那一侧从落地第一天起就在白名单上。它存在过这件事本身是**越界**，
//!    不是「做了一半的功能」。删它不欠任何人一条平价缺口。
//!
//! ## ⚠ 退役**没有**顺带做掉什么（别读宽）
//!
//! - **[`list_local`] 还在**，而且它今天在**生产路径上零消费者** —— 留着的唯一理由
//!   不是「以后可能用得上」，逐条写在它自己的头注上（一条递减棘轮不许测试段裸遍历目录）。
//! - **上传与往外拖照旧碰本机盘**（`transfer` 读本机文件、`download` 往本机盘写、
//!   `shell::local_home` 给「存到哪儿」一个缺省值）。那是**传输**，不是文件管理器：
//!   要求是「本地不需要文件管理器」，不是「窗口不许碰本机盘」。
//! - `src/sftp/` 那块**旧面板**一个字节没动（它的退役是另一刀，排在补齐 7 项功能之后）。
//!
//! # 🔴列目录**只经通道问后端**（逐条理由住下面「窗口进程只说 `call`」那一节）
//!
//! 上一版这里写的是「远端这一侧直接 `await` 池子那条列目录命令（同进程、同一个连接池）」。
//! 窗口改成独立进程之后那句话早就只对一半（换进程就换一份池），F2 这一拍把那条路整条摘了。
//!
//! # ⚠ 排序：一个契约，盘上有两份实现 —— 而**显示序**这一半只有一个家
//!
//! 「目录在前，再按名称小写排」这条契约，生产侧的落点**曾经**是池子里那个私有排序函数
//! （`sort_entries`〔散文墓碑〕随池子那条列目录命令一起删了）⇒ 今天只剩 [`sort_rows`] 一份。
//!
//! 🔴 **如实登记这个缝**：后端不排（它答的是目录项，不是一屏）⇒ 窗口这一侧的显示序
//! 只经 [`sort_rows`]；池子那一份今天只服务旧面板。
//! [`tests::the_two_orderings_agree_on_a_synthetic_set`] 把两份契约对拍成相等，
//! 所以「两份漂开」这件事照旧有判据；
//! 而「排两次会不会把序搅了」有一条结构上的答案：`slice::sort_by` 是**稳定**的，
//! 把同一个比较器再作用一次在已经有序的那一摞上是恒等
//! （[`tests::sorting_an_already_sorted_screenful_by_name_changes_nothing`] 现打钉着）。
//!
//! ⚠ **池子那一份没退役**（它排的是 `SftpEntry`，不是这一侧的行），
//! 提级成共用件仍然要等 `sftp*.rs` 那棵树 —— **本刀不动它**（另一路在改那棵树）。
//!
//! # 🔴 排序**可选**（`名称 / 大小 / 类型`）—— 默认那一档与今天逐字节相同
//!
//! 旧面板表头上那个下拉（`src/sftp/paths.ts::sortEntries` 的三档）窗口这侧此前一格都没有：
//! [`sort_rows`] 是一个**写死**的纯函数。这一刀把「按什么排」提成 [`SortBy`] 一个入参，
//! 而 [`SortBy::Name`]（缺省）那一支**逐字节等于改之前那一份**：
//! 多出来的只是一个在 `Name` 那档返回 `Equal` 的 `then_with`，它对结果零影响。
//! 这句话不是推的 —— [`tests::the_default_order_is_byte_for_byte_what_it_was_before`]
//! 把**基线那一趟真跑出来的序**压成一个指纹钉在判据里（那个数是在基线提交上量的）。

use copy_core::copy_text;
use std::path::Path;

/// 文件列表里的一行。**故意比 `SftpEntry` 窄** —— 列表只画得下这些。
///
/// 🔴**它现在要过一次进程边界**，所以多了 serde 那一对。
/// 开窗改成起一个独立进程之后，入口那条命令先列好的那一屏得**交给另一个进程**
/// （逐条理由住 [`super::proc`]）。⚠ 这一对 derive **不是** rust↔TS 那条边界上的
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
/// # 🔴 为什么不是「往 [`Row`] 上再加两个字段」—— 理由不是设计，是并行开发，如实记
///
/// 直觉写法是给 `Row` 加两格。**那一改今天落不下去**：`Row` 的结构体字面量盘上约 30 处，
/// 其中 `tests/frontend/shell/filewin/{writeops,editor,proc}_tests.rs` 三份在本刀的红线里
/// （另两路正在改它们对应的生产文件）。给 `Row` 加一格 ⇒ 那三份当场编不过；
/// 而且**另两路新写的每一处 `Row { … }` 会在合并那天一起编不过** ——
/// 那不是一次可能的冲突，是一次**必然**的冲突。
///
/// ⇒ 形状换成「窄的那个一个字节不动，外面套一层」：
/// [`Row`] 仍然是那五格，`Listed` 是**窗口里的一行**。⚠ 刻意**只给 `Deref`、不给 `DerefMut`**：
/// 经这一层改不了里面那一行 ⇒ 「两格与那一行漂开」这件事**写都写不出来**。
///
/// # 过进程边界的是它，不是 [`Row`]
///
/// 上一版这里登记着一条代价：交给窗口进程的**第一屏**走 `Vec<Row>`，于是那一屏
/// 没有链接与时间两格。那份文件（`proc.rs`）后来进了写区 ⇒ 种子里那一屏
/// 换成了 `Vec<Listed>`，那条代价没了。〔09-28 裁 3〕那一屏今天不过进程边界了（窗口进程自己列，`proc::first_screen`），
/// 链接与时间两格直接从 [`row_from_ls_entry`] 来。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Listed {
    /// 那五格。
    pub row: Row,
    /// 🔴 这一项**是不是符号链接**。后端 `files.ls` 的 `kind` 与 SFTP 的 `is_symlink`
    /// 两条路都送得出它 —— 此前窗口这一侧把它**压掉了**（见 [`row_from_ls_entry`] 那张表）。
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
    /// 后端 `files-ls` 送的本来就是字节（`{"b16": …}`），此前在 [`row_from_ls_entry`] 里被丢掉了 ——
    /// 留住它，写操作（改名 · 删除 · 改权限）就能对乱码名动手（`writeops::rel_json` 发 b16）。
    /// ⚠ `serde(default)`：开窗那一跳（`proc::OpenRequest`）两头版本不齐时缺这一格 ⇒ `None`
    /// ⇒ 那一行退回「有损名不许写」，不会拿显示串去寻址。
    #[serde(default)]
    pub raw_name: Option<Vec<u8>>,
    /// 断了的链接（后端 `files-ls` 的 `link_to: "missing"`：指向的东西不在 / 读不到）。
    #[serde(default)]
    pub link_broken: bool,
}

impl Listed {
    /// 从一条**只交得出那五格**的路来的一行（链接与时间两格都**不知道**）。
    ///
    /// ⚠ 名字里的 `plain` 说的就是那件事：它不是「这一行不是链接、没有时间」，
    /// 是「问不出来」。⇒ 画的时候两格都不画，而不是画一个编出来的值。
    pub fn plain(row: Row) -> Self {
        Self {
            row,
            link: false,
            link_dir: false,
            mtime_secs: None,
            mtime_text: None,
            mtime_full: None,
            raw_name: None,
            link_broken: false,
        }
    }

    /// 点得进去：目录，或指向目录的链接。
    pub fn opens_as_dir(&self) -> bool {
        self.row.is_dir || self.link_dir
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

/// 这个窗口现在在看哪儿 —— **一台远端**，而且只可能是一台远端。
///
/// 🔴 **它是一个 newtype 而不是一个单格枚举，这一条是承重的。**
///
/// 「本机那一侧不存在」这件事有两种落法：① 留着枚举、把 `Local` 那一格删掉；
/// ② 连枚举一起收成一个 newtype。两者对今天的行为**完全等价**，但可判性差一整级：
/// 走 ① 的话 `is_remote()` 恒回 `true`、那七处 `if !is_remote() { 出声拒 }`
/// 变成永远走不到的死支 —— 而**死支上的错误文案与一条真判据长得一模一样**
/// （本仓那条「判据不在执行链上就等于不存在」的同形）。走 ②，
/// 「窗口看着本机」这句话**连写都写不出来**：编译器兜着，不靠任何一条判据兜。
/// 形状上的先例逐字住 `src/frontend/shell/Cargo.toml`（`creds-core` 那条 `harden` feature）：
/// 「『backend 写不了这份文件』是**编译器**兜的，不是一条判据兜的」。
///
/// 它装的是那台机器**寻址用的名字**（`RemoteConfig::origin_label` 的口径，monitor 开窗时算好随种子交来）。
/// 从前装的是整份 `RemoteConfig`：窗口只拿它取名字、再给「在此打开终端」算机器事实 —— 开终端改由 monitor 接之后只剩名字，
/// 窗口进程从此不认识 monitor 的配置类型（`filewin` 边界判据那一格「开窗配置的类型」清零）。
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
    /// 而寻址要的是 `origin::LOCAL` 那个 `"<local>"`）。
    /// ⇒ **仍然不合并**：它们回的是同一个串是**今天的实况**，不是契约。
    ///
    /// ⚠ **刻意不回 `String`**：`origin_tests::no_new_raw_string_origin_parameters`
    /// 是一条递减棘轮 —— 「origin 归一 —— 这是地基」，
    /// 新代码一律用这个类型，不许再给这个概念造一种表达。
    /// ⚠ 那个名字由 monitor 开窗时用 `origin_label()` 算（`filewin/entry.rs`），与 `stream_source` 的 `stream_loop` 登记时
    /// 用的是**同一个函数** —— 两处漂开的症状是「命令发给了一个谁都没登记过的
    /// origin，而且不报错」（`inbound_client::LOCAL_ORIGIN` 的头注记过同一形）。
    pub fn origin(&self) -> Origin {
        Origin(self.0.clone())
    }
}

// 远端路径的「上一级」与「最后一段」（`parent_dir` · `remote_basename`）逐字搬进了契约 crate `filewin-contract`：
//   monitor 开窗入口（`filewin/entry.rs::plan_target`：「跳到这个文件」切成目录 ＋ 名字）与窗口两边都切，切法只许一份。
pub use filewin_contract::{parent_dir, remote_basename};

/// 按哪一列排（详情视图的四列，闭集）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    /// 名称（小写不敏感）。**缺省**。
    #[default]
    Name,
    /// 修改时间，**新的在前**。
    Mtime,
    /// 「类型」那一列（种类，再扩展名）。
    Type,
    /// 大小，**大的在前**。
    Size,
}

impl SortBy {
    /// 四列全体（表头按它画）。
    pub const ALL: [SortBy; 4] = [SortBy::Name, SortBy::Mtime, SortBy::Type, SortBy::Size];

    /// 表头那一格写什么。
    pub fn label(self) -> String {
        match self {
            SortBy::Name => copy_text("rsFilewinSource.sort.name", &[]),
            SortBy::Mtime => copy_text("rsFilewinSource.sort.mtime", &[]),
            SortBy::Size => copy_text("rsFilewinSource.sort.size", &[]),
            SortBy::Type => copy_text("rsFilewinSource.sort.type", &[]),
        }
    }

    /// 这一列第一次点下去是不是从大到小（时间 · 大小：新的 / 大的在前；名称 · 类型：A 在前）。
    pub fn starts_descending(self) -> bool {
        matches!(self, SortBy::Mtime | SortBy::Size)
    }
}

/// 一屏怎么排：哪一列 ＋ 有没有被再点一次反过来。窗口状态，关窗即没。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sort {
    pub by: SortBy,
    /// 点过同一列第二次 ⇒ 反过来。
    pub reversed: bool,
}

impl From<SortBy> for Sort {
    fn from(by: SortBy) -> Self {
        Self {
            by,
            reversed: false,
        }
    }
}

impl Sort {
    /// 这一屏是不是从大到小。
    pub fn descending(self) -> bool {
        self.by.starts_descending() != self.reversed
    }

    /// 点了表头 `by` 那一列之后怎么排：同一列 ⇒ 反过来；换了一列 ⇒ 那一列的第一下。
    pub fn after_click(self, by: SortBy) -> Self {
        if self.by == by {
            Self {
                by,
                reversed: !self.reversed,
            }
        } else {
            by.into()
        }
    }
}

/// 目录在前，再按那一列排；相持一律回落到名称小写。反过来时只反那一列（目录照旧在前，同资源管理器）。
///
/// 🔴 **这是「屏幕上那一屏是什么序」的唯一住址。**「类型」那一列按 [`super::kind`] 判的种类，再扩展名（与那一列写的字同源）。
pub fn sort_rows(v: &mut [Listed], sort: impl Into<Sort>) {
    let sort: Sort = sort.into();
    // 排序键每行先算一次（小写名 · 扩展名），比较时不再分配。
    v.sort_by_cached_key(|r| {
        let lower = r.name.to_lowercase();
        let key = match sort.by {
            SortBy::Name => By::Name(lower.clone()),
            SortBy::Mtime => By::Num(r.mtime_secs.unwrap_or(0)),
            SortBy::Size => By::Num(r.size),
            SortBy::Type => By::Type(super::kind::kind_of(r), super::kind::ext_of(&r.name)),
        };
        let key = if sort.descending() {
            Order::Desc(std::cmp::Reverse(key))
        } else {
            Order::Asc(key)
        };
        (std::cmp::Reverse(r.is_dir), key, lower)
    });
}

/// [`sort_rows`] 那一列的排序键（一次排序里每行同一形）。
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum By {
    Name(String),
    Num(u64),
    Type(super::kind::Kind, Option<String>),
}

/// 那一列正着排还是反着排（一次排序里每行同一形）。
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Order {
    Asc(By),
    Desc(std::cmp::Reverse<By>),
}

/// 一条远端绝对路径 → **可点的那几段**：`(这一段叫什么, 点它去哪儿)`，根排在最前。
///
/// # 🔴 它为什么住在这儿（而不是 `shell.rs` 里画面包屑的那一行旁边）
///
/// [`parent_dir`] / [`remote_basename`] 那一对的头注逐字说了：切远端路径的算法
/// **多一份就多一种「Windows 上 `\` 被当分隔符」的机会**。面包屑要的是第三种切法
/// （前缀那一摞，不是单个前缀、也不是尾段）⇒ 它**加进那一对所在的地方**，
/// 与它们共享同一句理由：**只认 `/`，一次都不碰 `std::path`**。
///
/// 盘上每一处按 `/` 切的地方由 `source_tests::every_place_that_splits_a_remote_path_is_declared`
/// 逐条钉成一张相等的表 —— 「不许写第四份」这句话因此不再是散文。
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
// 路径是合法 UTF-8 ⇒ 字符串（与此前逐字相同）；不是 ⇒ `{"b16": …}`（后端 `files-*` 那一族早就收这一形）。
// 按字节切的三个函数与 [`parent_dir`] / [`remote_basename`] / [`breadcrumbs`] 同住这里、同一个口径（恒用 `/`，尾斜杠不算一段）。

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

    /// 上一级（按字节切）。
    pub fn parent(&self) -> Self {
        Self::from_bytes(&parent_bytes(&self.bytes()))
    }

    /// 尾段发出去的那一形。
    pub fn tail_wire(&self) -> serde_json::Value {
        wire_bytes(tail_bytes(&self.bytes()))
    }

    /// 这条路径是有损的（显示串寻址不到真字节）。
    pub fn is_lossy(&self) -> bool {
        self.raw.is_some()
    }
}

/// 字节 → 发出去的那一形：合法 UTF-8 ⇒ 字符串；否则 ⇒ `{"b16": …}`（小写十六进制）。
pub fn wire_bytes(b: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(b) {
        Ok(s) => serde_json::Value::String(s.to_string()),
        Err(_) => serde_json::json!({
            super::find::HEX_KEY: b.iter().map(|x| format!("{x:02x}")).collect::<String>()
        }),
    }
}

fn trim_slashes(b: &[u8]) -> &[u8] {
    let mut end = b.len();
    while end > 0 && b[end - 1] == b'/' {
        end -= 1;
    }
    &b[..end]
}

/// [`parent_dir`] 的字节版（同一个口径：根的上一级是根）。
pub fn parent_bytes(b: &[u8]) -> Vec<u8> {
    let t = trim_slashes(b);
    match t.iter().rposition(|x| *x == b'/') {
        Some(0) | None => b"/".to_vec(),
        Some(i) => t[..i].to_vec(),
    }
}

/// [`remote_basename`] 的字节版。
pub fn tail_bytes(b: &[u8]) -> &[u8] {
    let t = trim_slashes(b);
    match t.iter().rposition(|x| *x == b'/') {
        Some(i) => &t[i + 1..],
        None => t,
    }
}

/// 两个目录按**段**比的最长公共前缀（字节版；都只在根下相交 ⇒ `/`）—— `workspace::common_dir` 的字节版。
pub fn common_dir_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let segs = |x: &[u8]| -> Vec<Vec<u8>> {
        x.split(|c| *c == b'/')
            .filter(|s| !s.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    let (sa, sb) = (segs(a), segs(b));
    let mut out = Vec::new();
    for (x, y) in sa.iter().zip(sb.iter()) {
        if x != y {
            break;
        }
        out.push(b'/');
        out.extend_from_slice(x);
    }
    if out.is_empty() {
        out.push(b'/');
    }
    out
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

use host_core::local_offset_at;

/// 按**本机**的本地时间画 —— 只给**这台自己的事**用：窗口自己记下的时刻（保存于 · 断线于 · 一件传输收尾于）
/// 与这台盘上的文件（上传撞名那张表「这台」那一格）。
///
/// ⚠ 那台后端送来的文件时间**不走这里**：列表 · 搜索结果 · 属性 · 预览 · 撞名表「那台」那一格都照抄后端写好的
/// `mtime_text` / `mtime_full`。
pub fn mtime_text(secs: u64) -> MtimeText {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let today = copy_core::civil_from_days((now + local_offset_at(now)).div_euclid(86_400));
    mtime_text_at(secs, local_offset_at(secs as i64), today)
}

/// 列一个**本机**目录。
///
/// # 🔴 它在生产路径上**零消费者**，而它还在盘上 —— 逐条理由，不是「以后可能用得上」
///
/// 本机那一侧退役（见本模块头注那块墓碑）之后，它唯一的消费者是
/// `tests/frontend/filewin/find_testing.rs::walk` —— 搜索那一族判据用的**合成后端**
/// 要一个「列一个目录」原语去走一棵临时目录树。
///
/// 为什么不把它搬进那份测试文件（那才是「该删就删」的写法）：
/// `scanning_guard_registry::no_new_guard_walks_the_tree_without_excluding_itself`
/// 的人群是 `tests/` 整棵树，而它的针里逐字有 `read_dir(`
/// ⇒ 搬过去就是往那条**递减棘轮**的存量清单里**新加一个文件**，
/// 而那张清单逐字「**只许变短**，不许往里加」，且
/// `the_pending_ratchet_never_turns_backwards` 拿 git 历史当权威
/// ——「抬上限让今天好过」提交了也不会绿。
///
/// ⇒ 三条路里选了代价最小的一条，**并且把代价写在这里**：
/// ① 搬进测试文件 ⇒ 顶破棘轮（禁）；② 抽进 `guard-core` ⇒ 动共享 crate 登记表
/// （不在本刀写区）；③ **留在原处、降级申报**（选它）。
///
/// ⚠ **它不是「本机文件管理器」的一部分了。** 「本机也能浏览」那条路径整条不在了：
/// 没有 `Source` 的本机格、没有那颗按钮、[`list_dir`] 的退路里也不再提它。
/// 谁要把它接回一条用户可达的路径上 —— 那是在推翻一条产品裁决，先去读那块墓碑。
///
/// ⚠ 单个条目 `metadata()` 失败（权限 / 竞态删除）时**不整趟失败**：
/// 那一行按「非目录、大小 0」记下来 —— 列目录的用处是「看得见有什么」，
/// 为一个读不到 stat 的条目把整屏打掉不划算。
pub fn list_local(dir: &Path) -> Result<Vec<Listed>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| {
        copy_text(
            "rsFilewinSource.local.readDirFailed",
            &[("e", &e.to_string())],
        )
    })?;
    let mut out: Vec<Listed> = Vec::new();
    for entry in rd.flatten() {
        let raw = entry.file_name();
        let name = raw.to_string_lossy().to_string();
        let lossy_name = name.contains('\u{FFFD}');
        // `metadata()` 跟随符号链接；跟不动（悬空链接）就退回 `symlink_metadata`。
        let meta = entry
            .metadata()
            .or_else(|_| std::fs::symlink_metadata(entry.path()))
            .ok();
        // ⚠ 这一侧**刻意不接那两格**（链接 / 时间）：它在生产路径上零消费者
        //   （见上面那一节），给它加两格只会让「本机那一侧还活着」看起来更像真的。
        out.push(Listed::plain(Row {
            path: entry.path().to_string_lossy().to_string(),
            is_dir: meta.as_ref().is_some_and(|m| m.is_dir()),
            size: meta.as_ref().map_or(0, |m| m.len()),
            lossy_name,
            name,
        }));
    }
    sort_rows(&mut out, SortBy::default());
    Ok(out)
}

/// 那台机器答出来的 home 能不能当**起点**用。
///
/// # 🔴 抽成纯函数的理由
///
/// 问 home 那一趟（〔09-28 裁 3〕窗口进程的 `proc::first_screen`）要一个起着的后端才跑得起来
/// ⇒ 那条路上**有逻辑的一段**就是这里（连同 [`home_from_reply`] 的解字节），抽出来它就有判据了。
///
/// 改过名（旧名带着 SFTP 那一问的字眼）：第七刀那一版问的是
/// SFTP 的 `realpath(".")`；现在问的是后端 `files-home`，判的这一道一个字没变。
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

/// 问 home 那条线上命令的名字（后端 `files-read` 族第八条）。
pub const CMD_HOME: &str = "files-home";

/// 一趟 `files-home` 的 `data` → 开窗的起点。
///
/// 🔴**这一问从 SFTP 换到了后端。** 上一版这里是一个
/// async 函数（monitor 为「`.` 是哪儿」单拨一条 SFTP，走池子那条 `realpath` 命令），
/// 它是 monitor 那一侧**最后一处**碰 SFTP 的地方；
/// 现在问的是后端 `files-home`（〔09-28 裁 3〕窗口进程经通道问，`proc::first_screen`）。
///
/// 两道：① `path` 解成字节 —— **不是合法 UTF-8 就拒**（窗口的路径是字符串，有损解码之后
/// 寻址不到那个目录，开出来的是别处）；② 能不能当起点，交 [`start_dir_from_home`]。
pub fn home_from_reply(d: &serde_json::Value) -> Result<String, String> {
    let raw = d
        .get("path")
        .ok_or_else(|| copy_text("rsFilewinSource.home.noPath", &[]))?;
    let bytes = super::find::decode_path(raw)
        .ok_or_else(|| copy_text("rsFilewinSource.said.badReply", &[]))?;
    let s = String::from_utf8(bytes).map_err(|_| copy_text("rsFilewinSource.home.notUtf8", &[]))?;
    start_dir_from_home(&s)
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴〔波二〕**窗口进程只说 `call`** —— 经 F3 那条通道
// ═══════════════════════════════════════════════════════════════════════
//
// 用户逐字：「甲, 窗口变成独立前端. 我说了后端要模块化, 即原生后端+文件管理后端.
// 现在先解耦清楚. 然后 monitor 可以打开文件管理器的前端」；「D11：后端是给定的，不要退路」。
//
// # 形状
//
// 窗口进程（`proc::child_main`）从 stdin 拿到交接件（`chan::host::Handoff`：回环地址 ＋
// 钥匙 ＋ 帧长），用 `chan::dial::dial` 连上 monitor 那个通道口，换一个
// `chan::client::Client`（[`Line`]）。此后读侧与写面**每一条**都经 [`ask`] 说
// `call(origin, op, payload, budget)`，由 monitor 那一侧的路由器转给 `inbound_client`
// ——本机与远端同一条路。
//
// # 🔴 没有退路（`D11`）
//
// 上一版这里是「先问后端，问不到就退回 SFTP 并出声」（那一格的裁决类型叫 `ListVerdict`，
// 退路走池子那条列目录命令）。**整条拿掉了**：问不到就是错，原话画在窗口上。
// 窗口进程里列目录这件事从此只有一条路、一份排序（[`sort_rows`]）。
// 那条退路的判据（「退路在、排在问后端之后、只有一支」）随它一起换成了反面：
// `source_tests::listing_has_no_second_road_when_the_backend_refuses`（行为）＋
// `boundary_tests` 的窗口侧登记表（池子那条列目录命令不在表里 ⇒ 写上就红）。
//
// # ⚠ 这一步**没有**做到什么（别读宽）
//
// - ✅〔审计 F 🔴-3 订正〕**跨机传输**：这里原先写「仍走 SFTP（上传 · 往外拖 · 取消），未拍」。
//   今天上传 / 下载是窗口说 `transfer-upload` / `transfer-download` ＋ 订阅 `transfer/<id>`（取消 = 停订），
//   monitor 只中继，SFTP 住本机常驻后端；往 OS 拖出去不做。窗口进程零 SFTP。
//   ✅「读一份文本进编辑器」那一条已换成后端 `files-read-text`。
// - ✅**同机复制**此前仍走 SFTP（后端没有 `files-copy`，
//   登记在 `boundary_tests` 那一类「后端缺命令」里）—— 现在问后端 `files-copy`，那一类清零删了。
// - ✅**开窗时解 home** 此前仍走 SFTP（住 monitor 那一侧，
//   后端没有这一问）—— 现在问后端 `files-home`（[`home_from_reply`]），monitor 那一侧
//   开窗一个 SFTP 都不拨了。
// - **`Row` 仍然持字符串不持字节** —— 那「单独一刀」没去改 `Row`，改的是**发出去的那一跳**：
//   窗口记住当前目录的字节（`FileWindow::cwd_raw`）与行上名字的字节（`Listed::raw_name`），发路径一律经 [`RemotePath`]
//   （进目录 · 列目录 · 上一级 · 读 / 存文本 · 复制 · 算大小 · 新建 · 写面的根）；显示照旧用有损串。
//   仍然做不了、会出声的：有损目录里上传 / 搜索 / 开终端 / 书签；有损名下载（SFTP 库寻址不到）。

/// 那条线上命令的名字。⚠ 能力名是 `files.ls`，线上名是 `files-ls`
/// （两者刻意不同形，同 `find.rs` 头注那条）。
pub const CMD_LS: &str = "files-ls";

/// 一趟 `files-ls` 回来的 `entries` 里的**一条** → [`Listed`]。
///
/// 🔴 **抽成具名函数是为了让它可判**：[`list_via_backend`] 整条路要一条通道才跑得起来，
/// 而那条路上**唯一有逻辑的一段**就是这里。
///
/// # 🔴 逐格说明 —— **这张表现在是一条判据，不是散文**
///
/// | 后端送的 | 窗口这一行的哪一格 | 怎么来的 |
/// |---|---|---|
/// | `path`（字符串 或 `{"b16":…}`） | `path` · `name` · `lossy_name` | 先解成**字节**，再取尾段作名字；有损与否看那串字节 |
/// | `kind`（`dir`/`file`/`symlink`/`other`） | `is_dir` ＋ `link` | `dir` ⇒ 目录；`symlink` ⇒ 链接。**两格，不是一格** |
/// | `link_dir`（只在链接上、可能缺） | `link_dir` | 链接且它为真 ⇒ 指向目录；缺 ⇒ 否 |
/// | `size`（可能缺） | `size` | 缺就是 0（同本机那条：读不到 stat 不整趟失败） |
/// | `mtime_secs`（可能缺） | `mtime_secs` | 原样带上来；缺就是 `None`（＝**没送**，不是 1970） |
/// | `mtime_text`（可能缺） | `mtime_text` | 后端按那台本地钟写好的短写法，原样；缺 ⇒ `None` |
/// | `mtime_full`（可能缺） | `mtime_full` | 后端按那台本地钟写好的完整写法，原样；缺 ⇒ `None` |
/// | `entries` | 那一屏有几行 | 由 [`rows_from_ls_data`] 摊开 |
/// | `truncated` | 界面上那句「只拿到了前 N 条」 | 同上 |
/// | `unreadable` | 界面上那句「有 n 项读不出来」 | 同上；缺 ⇒ 0 |
///
/// 🔴〔补齐五项 2026-09-23〕**这张表此前是散文，而它自陈的两格正是两条真缺口。**
/// 上一版逐字写着「`kind`（四值）⇒ `is_dir`：只有 `dir` 算目录」与
/// 「`mtime_secs` ⇒ ——：窗口今天不画它」。前者把一个四值**压成一个布尔**
/// （于是屏幕上分不出符号链接），后者整格丢掉（于是看不到修改时间）；
/// 而**没有任何一条判据在守这张表** —— 谁把 `size` 那一行也悄悄丢掉，
/// 这张表照旧写着「原样带上来」，一个数都不会动。
///
/// ⇒ 这一刀把它变成 `source_tests::the_field_by_field_table_between_backend_and_window_is_a_judge`：
/// 左栏的人群**现读** `src/backend/files/mod.rs` 里 `files.ls` 那条能力声明的 `fields`
/// （两侧不同源），与表里声明的那一摞**逐格相等**；右栏那句「怎么来的」
/// 每一行各挂一条**行为**断言。加一格 / 丢一格 / 说法漂了，三种都红。
pub fn row_from_ls_entry(v: &serde_json::Value) -> Result<Listed, String> {
    let raw = v
        .get("path")
        .ok_or_else(|| copy_text("rsFilewinSource.ls.noPath", &[]))?;
    let bytes = super::find::decode_path(raw)
        .ok_or_else(|| copy_text("rsFilewinSource.said.badReply", &[]))?;
    // 🔴 有损与否看**字节**，不看转出来的那个串里有没有 U+FFFD。
    //    后者是一个猜：真叫 `\u{FFFD}` 的文件会被误判成有损。
    // 只看**名字那一段**（最后一个 `/` 之后；远端路径恒用 `/`）：目录有损而名字干净的那一行
    //   名字照样能用（整条路径的字节由窗口按当前目录的字节拼，`FileWindow::row_path`）。
    let cut = bytes.iter().rposition(|b| *b == b'/').map_or(0, |k| k + 1);
    let lossy_name = std::str::from_utf8(&bytes[cut..]).is_err();
    let path = String::from_utf8_lossy(&bytes).to_string();
    let name = remote_basename(&path).to_string();
    // 有损名留住名字那一段的原始字节。
    let raw_name = lossy_name.then(|| bytes[cut..].to_vec());
    // 🔴 `kind` 落**两格**，不是一格：压成一个布尔就是「看不出哪个是符号链接」。
    let kind = v.get("kind").and_then(|k| k.as_str());
    let is_dir = kind == Some("dir");
    let link = kind == Some("symlink");
    let link_dir = link && v.get("link_dir").and_then(serde_json::Value::as_bool) == Some(true);
    let size = v
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Ok(Listed {
        row: Row {
            name,
            path,
            is_dir,
            size,
            lossy_name,
        },
        link,
        link_dir,
        // 🔴 原样带上来。缺了就是 `None`（＝**后端没送**）—— 不许兜底成 0，
        //    那是 1970-01-01，一个看起来很像真读数的假时间。
        mtime_secs: v.get("mtime_secs").and_then(serde_json::Value::as_u64),
        mtime_text: v
            .get("mtime_text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        mtime_full: v
            .get("mtime_full")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        raw_name,
        link_broken: link
            && v.get("link_to").and_then(serde_json::Value::as_str) == Some("missing"),
    })
}

/// 一屏之外的那两格：被截断了吗 · 有几项读不出来（后端照数、没列出）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cut {
    pub truncated: bool,
    pub unreadable: u64,
    /// 目录里一共读到几项（含没回送的；后端没给 ⇒ 0）。
    pub total: u64,
}

/// 问**那台机器上的后端**要一个目录。
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
    by: impl Into<Sort>,
) -> Result<(Vec<Listed>, Cut), String> {
    list_via_backend_at(
        line,
        origin,
        serde_json::Value::String(dir.to_string()),
        limit,
        by,
    )
    .await
    .map_err(|f| f.said)
}

/// 〔有损名全寻址〕同 [`list_via_backend`]，目录由调用方给线上那一形（字符串或 `{"b16": …}`，[`RemotePath::wire`]）。
pub async fn list_via_backend_at(
    line: &Line,
    origin: &Origin,
    dir: serde_json::Value,
    limit: usize,
    by: impl Into<Sort>,
) -> Result<(Vec<Listed>, Cut), Failed> {
    let args = serde_json::json!({ "path": dir, "limit": limit });
    let d = ask_coded(
        line,
        origin,
        CMD_LS,
        &args,
        std::time::Duration::from_secs(20),
    )
    .await?;
    rows_from_ls_data(&d, by).map_err(Failed::local)
}

/// 目录打不开的那几种（后端的码；界面按它说一句、给出路）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenFail {
    NotFound,
    Denied,
    NotDir,
    Other,
}

impl OpenFail {
    /// 后端拒绝时给的码 → 哪一种（不是这几个码 ⇒ `None`：那不是「打不开」，是别的失败）。
    pub fn of_code(code: Option<&str>) -> Option<Self> {
        Some(match code? {
            "not_found" => Self::NotFound,
            "denied" => Self::Denied,
            "not_dir" => Self::NotDir,
            "unreadable" => Self::Other,
            _ => return None,
        })
    }
}

/// 一趟 `files-ls` 的整份 `data` → **一屏**。
///
/// 🔴 **把整段抽出来（而不是只抽一条 entry）是为了让这三件都变成行为可判的**：
/// ① 排序真的做了；② 解不出的一行**整趟报错**而不是被悄悄跳过；③ 截断那一格带回来。
/// 第一版只抽了 `row_from_ls_entry`，于是 ②③ 只能靠源码代理钉 ——
/// 而死值验当场量到 ② **一条判据都不红**（把 `?` 换成 `if let Ok` 静悄悄地过）。
///
/// # ⚠ 为什么「解不出一行」要整趟失败
///
/// 悄悄跳过的后果：目录里少一个文件，屏幕上没有任何提示
/// ⇒ 与「这个文件不存在」**分不开**。而用户会据此以为文件丢了。
pub fn rows_from_ls_data(
    d: &serde_json::Value,
    by: impl Into<Sort>,
) -> Result<(Vec<Listed>, Cut), String> {
    let arr = d
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| copy_text("rsFilewinSource.ls.notArray", &[]))?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, one) in arr.iter().enumerate() {
        out.push(row_from_ls_entry(one).map_err(|e| {
            copy_text(
                "rsFilewinSource.ls.badEntry",
                &[("i", &i.to_string()), ("e", &e.to_string())],
            )
        })?);
    }
    // 🔴 **排序在这一侧，而且它只有一份** —— 后端不排（它答的是目录项，不是一屏）。
    //    「按什么排」由调用方给（用户在工具栏上选的那一档），算法只有 `sort_rows` 这一个家。
    sort_rows(&mut out, by);
    let truncated = d
        .get("truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let unreadable = d
        .get("unreadable")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let total = d
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Ok((
        out,
        Cut {
            truncated,
            unreadable,
            total,
        },
    ))
}

/// 一屏最多要多少行。
///
/// ⚠ 后端那侧有自己的默认值（`files/mod.rs::DEFAULT_LIMIT`）；这里**显式给**，
/// 因为「一屏多少」是调用方的事（同那条「机制在后端 ·
/// 节拍归调用方」的形）。取这个数是因为窗口是**虚拟滚动**的
/// （`ScrollArea::show_rows`），一次拿多少不影响帧时，只影响一次往返的大小。
pub const LS_LIMIT: usize = 50_000;

/// 列一个目录 —— **只问后端**（`D11`：没有退路）。
///
/// 回 `(行, 截断了吗)`。问不到 ⇒ 那句原话（[`said`] 翻过的）原样交出去。
pub async fn list_dir(
    line: &Line,
    source: &Source,
    dir: &str,
    by: impl Into<Sort>,
) -> Result<(Vec<Listed>, Cut), String> {
    list_via_backend(line, &source.origin(), dir, LS_LIMIT, by).await
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
/// 🔴 **窗口进程里说 `call` 的只有这一处**（`X6` 的 Rust 那一侧人群就是它）：
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

/// 一趟 [`ask_coded`] 没成：**对端说的码**（只有「对端拒了」那一形有）＋ 那句人话。
///
/// 🔴为什么要把码留下来：编辑器读文本那一问，后端的
/// `too_large` / `not_text` 与「连不上」是**两件事**（前者说「这份不是一份能编辑的文本」，
/// 后者说「这一趟没走通」），而 [`said`] 翻完之后只剩一句话 —— 分它们就只能猜字符串前缀。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Failed {
    /// 对端拒绝时它给的那个码（`None` = 不是对端拒的：没走通 / 这一侧拼错 / 对端不认这条命令）。
    pub code: Option<String>,
    /// 给人看的那句话（[`said`] 翻过的，或这一侧自己的那句）。
    pub said: String,
    /// 「复制详情」那几行（首行之外）：对端拒了 ⇒ 那台后端写好的那份；通道没走通 / 这一侧的错 ⇒ 窗口进程自己写（[`detail_of`]）。
    /// 空 ⇒ 不出按钮。
    pub detail: String,
}

impl Failed {
    fn local(said: String) -> Self {
        Self {
            code: None,
            said,
            detail: String::new(),
        }
    }

    /// 这一侧的错（问之前拼不出参数 · 回来的读不懂）：窗口进程写详情（时刻 · 机器 · 命令 · 原话）。
    pub(crate) fn here(said: String, origin: &Origin, cmd: &str, raw: Option<&str>) -> Self {
        Self {
            code: None,
            said,
            detail: copy_core::detail::Detail::new()
                .item(copy_core::detail::Label::At, now_stamp())
                .item(copy_core::detail::Label::Machine, &origin.0)
                .item(copy_core::detail::Label::Command, cmd)
                .maybe(copy_core::detail::Label::Raw, raw)
                .render(),
        }
    }

    /// 一行汇总（「上传失败 · n 项」）底下几件各自的失败 ⇒ 复制出去的整段：首行是那一行，每件一段（名字 · 那一句 ＋ 它的详情），
    /// 段间空一行（与主界面合流 ×N 同一排法）；没有一件带详情 ⇒ `None`（不出按钮）。
    pub fn copy_many(head: &str, items: &[(String, Failed)]) -> Option<String> {
        let segs: Vec<String> = items
            .iter()
            .filter(|(_, f)| !f.detail.trim().is_empty())
            .map(|(name, f)| {
                format!(
                    "{}\n{}",
                    copy_text(
                        "rsFilewinProgress.detail.failedOne",
                        &[("name", name), ("why", &f.said)]
                    ),
                    f.detail
                )
            })
            .collect();
        (!segs.is_empty()).then(|| format!("{head}\n\n{}", segs.join("\n\n")))
    }

    /// 屏上那一句 `shown`（可能套了别的字）＋ 详情 ⇒ 复制出去的整段；没详情 ⇒ `None`（不出按钮）。
    pub fn copy_body(shown: &str, detail: &str) -> Option<String> {
        (!detail.trim().is_empty()).then(|| format!("{shown}\n{detail}"))
    }
}

/// 只有一句话（这一侧说的，没有详情 ⇒ 不出按钮）。
impl From<String> for Failed {
    fn from(said: String) -> Self {
        Failed::local(said)
    }
}

impl From<&str> for Failed {
    fn from(said: &str) -> Self {
        Failed::local(said.to_string())
    }
}

/// 写出来就是给人看的那一句（详情另取）。
impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

/// 此刻（本机本地时间 ＋ 偏移）。
fn now_stamp() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    copy_core::detail::stamp(t, local_offset_at(t))
}

/// 一趟 `call` 没成的复制详情：对端拒了、那台写了详情 ⇒ 原样；别的（通道没走通 · 对端不认 · 本侧）⇒ 窗口进程写
/// 时刻 · 机器（没发出去标「未连上」）· 命令 · 断在 · 码（那几项的取法住通信层 `HopFacts`，与 monitor 壳同一份）。
pub fn detail_of(origin: &Origin, cmd: &str, e: &comms_inward::chan::wire::CallError) -> String {
    let origin = origin.0.as_str();
    use comms_inward::chan::wire::{CallError, PeerFault};
    use copy_core::detail::{Detail, Label};
    if let CallError::Peer {
        why: PeerFault::Refused { body },
    } = e
    {
        let wrote = serde_json::from_slice::<serde_json::Value>(&body.0)
            .ok()
            .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
            .unwrap_or_default();
        if !wrote.trim().is_empty() {
            return wrote;
        }
    }
    let f = e.hop_facts();
    let machine = if f.not_sent {
        format!(
            "{origin}（{}）",
            copy_text("detail.value.notConnected", &[])
        )
    } else {
        origin.to_string()
    };
    Detail::new()
        .item(Label::At, now_stamp())
        .item(Label::Machine, machine)
        .item(Label::Command, cmd)
        .maybe(Label::Hop, f.hop)
        .item(Label::Code, f.code)
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
    use comms_inward::chan::wire::{Body, Budget, Comms, Op};
    let budget = Budget {
        until: std::time::Instant::now() + t,
        cancel,
    };
    let payload = Body(serde_json::to_vec(args).map_err(|e| {
        Failed::here(
            copy_text("rsFilewinSource.ask.badArgs", &[("e", &e.to_string())]),
            origin,
            cmd,
            None,
        )
    })?);
    let op = Op(cmd.to_string());
    let body = line
        .call(origin, &op, payload, budget)
        .await
        .map_err(|e| Failed {
            code: refused_code(&e),
            said: said(cmd, &e),
            detail: detail_of(origin, cmd, &e),
        })?;
    let v: serde_json::Value = serde_json::from_slice(&body.0).map_err(|e| {
        Failed::here(
            copy_text("rsFilewinSource.ask.unreadable", &[("e", &e.to_string())]),
            origin,
            cmd,
            None,
        )
    })?;
    if v.is_null() {
        return Err(Failed::here(
            copy_text("rsFilewinSource.said.badReply", &[]),
            origin,
            cmd,
            None,
        ));
    }
    Ok(v)
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴窗口进程里说 `subscribe` 的**唯一一处**：看一趟传输
// ═══════════════════════════════════════════════════════════════════════

/// 进度流一开始给的 credit（格数）。之后每吃一格还一格。
///
/// ⚠ 格子是**整份快照**（传输台那一侧合并中间几格不丢信息）⇒ credit 小也不会漏读数，
/// 它只决定「窗口一口气最多攒几格没画」。
pub const WATCH_CREDIT: u32 = 8;

/// 看一趟传输（`kind` = `transfer/<id>`）到收场。回传完的字节数；失败 / 撤 ⇒ 那句原话。
///
/// - `stop` 被拨下 ⇒ **停订**（`Sub::stop`）⇒ 传输台那一侧「停订即撤」；本函数回「撤了」。
/// - 每一格进度交给 `on(已传, 总共)`。
///
/// 🔴 **窗口进程里说 `subscribe` 的只有这一处**（`X6` 的 Rust 那一侧人群里 `subscribe` 那一格）。
/// ⚠ `subscribe` 没有期限参数（订阅是长期意向）—— 这一趟多久算完由传输台说了算，
/// 窗口能做的是撤。
pub async fn watch(
    line: &Line,
    origin: &Origin,
    kind: &str,
    stop: &comms_inward::chan::wire::CancelToken,
    on: impl FnMut(u64, u64),
) -> Result<Watched, Failed> {
    watch_coded(line, origin, kind, stop, on).await
}

/// 同 [`watch`]（名字留着：上传据失败里的码 `sftp_home_mismatch` 换路）。失败带传输台说的码与复制详情
/// （传输台写了 ⇒ 原样；没写 · 这一侧收场的 ⇒ 窗口进程自己写）。
pub async fn watch_coded(
    line: &Line,
    origin: &Origin,
    kind: &str,
    stop: &comms_inward::chan::wire::CancelToken,
    mut on: impl FnMut(u64, u64),
) -> Result<Watched, Failed> {
    let plain = Failed::local;
    use comms_inward::chan::wire::{By, Comms, Item, Kind, Sub};
    use futures::StreamExt as _;
    let mut sub = line.subscribe(origin, &Kind(kind.to_string()), None, WATCH_CREDIT);
    let json = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap_or_default();
    loop {
        let next = tokio::select! {
            i = sub.next() => i,
            () = stop.cancelled() => {
                sub.stop();
                return Err(plain(super::transfer::CANCELLED.to_string()));
            }
        };
        let Some(item) = next else {
            return Err(plain(copy_text("rsFilewinSource.watch.cutShort", &[])));
        };
        match item {
            Item::Frame { body, .. } => {
                let v = json(&body.0);
                on(
                    v.get("got")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                    v.get("total")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                );
                sub.want(1);
            }
            Item::Gap { .. } | Item::Seen { .. } => sub.want(1),
            Item::Unseen { .. } => {
                return Err(plain(copy_text("rsFilewinSource.watch.hostGone", &[])))
            }
            Item::Closed { by: By::Ours(why) } => {
                return Err(plain(copy_text(
                    "rsFilewinSource.watch.localBreak",
                    &[("why", &format!("{:?}", why))],
                )))
            }
            Item::Closed { by: By::Peer(body) } => {
                let v = json(&body.0);
                let text = |k: &str| v.get(k).and_then(serde_json::Value::as_str).unwrap_or("");
                return match text("state") {
                    "done" => Ok(Watched {
                        bytes: v
                            .get("bytes")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        sha256: v.get("sha256").and_then(|s| s.as_str()).map(str::to_string),
                    }),
                    "failed" => {
                        let said = text("why").to_string();
                        let detail = text("detail");
                        let mut f = if detail.trim().is_empty() {
                            Failed::here(said, origin, kind, None)
                        } else {
                            Failed {
                                code: None,
                                said,
                                detail: detail.to_string(),
                            }
                        };
                        f.code = v.get("code").and_then(|c| c.as_str()).map(str::to_string);
                        Err(f)
                    }
                    "cancelled" => Err(plain(super::transfer::CANCELLED.to_string())),
                    _ => Err(plain(super::find::refusal(
                        kind,
                        text("code"),
                        text("message"),
                    ))),
                };
            }
        }
    }
}

/// 那台此刻连没连着（`link` 那条流的一格；monitor 的连接循环说了算，窗口不猜）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSeen {
    Up,
    /// 刚断、正在重连。
    Reconnecting,
    /// 重连一轮没连上（或从没连上）。
    Down,
}

/// 订「那台连没连着」那条流（[`filewin_contract::LINK_KIND`]，monitor 自己接）到它关掉为止；每一格交给 `on`。
/// 对端说「没有这条流」/ 流关了 ⇒ 回（窗口当作说不清：不画断线条）。
pub async fn watch_link(line: &Line, origin: &Origin, mut on: impl FnMut(LinkSeen)) {
    use comms_inward::chan::wire::{Comms, HopFault, Item, Kind, Sub};
    use futures::StreamExt as _;
    let mut sub = line.subscribe(
        origin,
        &Kind(filewin_contract::LINK_KIND.to_string()),
        None,
        WATCH_CREDIT,
    );
    while let Some(item) = sub.next().await {
        match item {
            Item::Seen { .. } => on(LinkSeen::Up),
            Item::Unseen {
                why: HopFault::Dropped,
                ..
            } => on(LinkSeen::Reconnecting),
            Item::Unseen { .. } => on(LinkSeen::Down),
            Item::Frame { .. } | Item::Gap { .. } => {}
            Item::Closed { .. } => return,
        }
        sub.want(1);
    }
}

/// 「重新连接」：叫醒那台的连接循环（[`filewin_contract::LINK_RETRY_OP`]，monitor 自己接；回 `{}`）。连没连上看 [`watch_link`]，
/// 这一下本身没送到 ⇒ 记一行（断线条照旧摆着，人可以再点）。
pub async fn kick_link(line: &Line, origin: &Origin) {
    if let Err(why) = ask(
        line,
        origin,
        filewin_contract::LINK_RETRY_OP,
        &serde_json::json!({}),
        std::time::Duration::from_secs(5),
    )
    .await
    {
        tracing::warn!("link-retry not delivered: {why}");
    }
}

/// 一趟传输看完了（[`watch`] 的成功那一形）：字节数 ＋上传那一路的整份摘要（提交时的 `expect`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watched {
    pub bytes: u64,
    pub sha256: Option<String>,
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

/// 通道那三层失败→ 窗口上那一句话。**穷尽 `match`，不许 `_ =>`。**
///
/// 🔴 对端说了话（`Peer{Refused}`）时 body 是宿主那一侧拼的 `{"code","message"}`
/// （`backend_route::layer_call_error` 逐字），翻成人话走 [`super::find::refusal`] ——
/// 与从前经 `inbound_client` 直连时**同一个翻译**，一句都没换。
///
/// ⚠ `reach` 那一格要说出来：`Sent` / `Unknown` 的意思是「对面可能已经做了」，
/// 对写面那几条这一句是承重的（用户据此决定要不要再点一次）。
pub fn said(cmd: &str, e: &comms_inward::chan::wire::CallError) -> String {
    use comms_inward::chan::wire::{CallError, HopFault, OursFault, PeerFault, Reach};
    match e {
        CallError::Peer { why } => match why {
            PeerFault::Refused { body } => {
                match serde_json::from_slice::<serde_json::Value>(&body.0) {
                    Ok(v) => super::find::refusal(
                        cmd,
                        v.get("code")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(""),
                        v.get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(""),
                    ),
                    Err(_) => copy_text(
                        "rsFilewinSource.said.refused",
                        &[("detail", &(String::from_utf8_lossy(&body.0)).to_string())],
                    ),
                }
            }
            PeerFault::Unsupported => copy_text("rsFilewinSource.said.unknownCmd", &[]),
        },
        CallError::Hop { at, reach, why } => {
            let what = match why {
                HopFault::Unreachable => &copy_text("rsFilewinSource.said.unreachable", &[]),
                HopFault::Dropped => &copy_text("rsFilewinSource.said.broken", &[]),
                HopFault::Overrun => &copy_text("rsFilewinSource.said.timeout", &[]),
            };
            let did = match reach {
                Reach::NotSent => &copy_text("rsFilewinSource.said.notSent", &[]),
                Reach::Sent | Reach::Unknown => &copy_text("rsFilewinSource.said.mayHaveRun", &[]),
            };
            let leg = if at.idx == 0 {
                &copy_text("rsFilewinSource.said.legWindow", &[])
            } else {
                &copy_text("rsFilewinSource.said.legHost", &[])
            };
            copy_text(
                "rsFilewinSource.said.hopFault",
                &[
                    ("leg", &leg.to_string()),
                    ("what", &what.to_string()),
                    ("did", &did.to_string()),
                ],
            )
        }
        CallError::Ours { why, runs_on } => match why {
            // 那台对这一条不认撤 ⇒ 说它可能还在跑。
            OursFault::Cancelled if *runs_on => copy_text("rsChanWire.ours.cancelledRunsOn", &[]),
            OursFault::Cancelled => copy_text("rsFilewinSource.said.cancelled", &[]),
            OursFault::Misuse => copy_text("rsFilewinSource.said.internal", &[]),
            OursFault::Broken => copy_text("rsFilewinSource.said.badReply", &[]),
        },
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/source_tests.rs"]
mod tests;
