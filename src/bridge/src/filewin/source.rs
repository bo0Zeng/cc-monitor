//! `24e` 数据面：**列一个目录**。只有一侧 —— 远端，走已有的 SFTP 那一套。
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
//!   用户裁的是「本地不需要文件管理器」，不是「窗口不许碰本机盘」。
//! - `src/sftp/` 那块**旧面板**一个字节没动（它的退役是另一刀，排在补齐 7 项功能之后）。
//!
//! # 🔴 远端这一侧刻意**不新写传输代码**
//!
//! [`list_remote`] 直接 `await` [`crate::sftp_pool::sftp_list_dir`] ——
//! 那是个 `#[tauri::command]`，但它同时就是一个普通的 `pub async fn`。
//! **同进程**（见 `super` 的头注）⇒ 这里是一次普通函数调用，**不过 IPC、不过 serde**，
//! 而且走的是**同一个进程级连接池**（`sftp_pool.rs::pool`）⇒ 不会多拨一条 SSH。
//!
//! # ⚠ 排序：一个契约，盘上有两份实现 —— 而**显示序**这一半在本刀收成了一份
//!
//! 「目录在前，再按名称小写排」这条契约，生产侧的落点是
//! `sftp_pool.rs::sort_entries`。它是**私有**的 ⇒ 本模块调不到，
//! 只能再写一份 [`sort_rows`]。
//!
//! 🔴 **如实登记这个缝，连同它这一刀变窄到什么程度**：
//! 退路那条（`sftp_list_dir`）回来的东西**在池子里已经排过一次**（生产实现），
//! 而 [`list_dir`] 现在**两支都再过一遍 [`sort_rows`]** ——
//! 于是「**屏幕上那一屏是什么序**」这件事在窗口这一侧只有一个家。
//! [`tests::the_two_orderings_agree_on_a_synthetic_set`] 把**两条路的真实输出**
//! 对拍成相等，所以「两份漂开」这件事照旧有判据；
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

use std::path::Path;

use crate::ssh_source::RemoteConfig;

/// 文件列表里的一行。**故意比 `SftpEntry` 窄** —— 列表只画得下这些。
///
/// 🔴〔第十三刀 2026-09-23〕**它现在要过一次进程边界**，所以多了 serde 那一对。
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
/// 其中 `tests/bridge/filewin/{writeops,editor,proc}_tests.rs` 三份在本刀的红线里
/// （另两路正在改它们对应的生产文件）。给 `Row` 加一格 ⇒ 那三份当场编不过；
/// 而且**另两路新写的每一处 `Row { … }` 会在合并那天一起编不过** ——
/// 那不是一次可能的冲突，是一次**必然**的冲突。
///
/// ⇒ 形状换成「窄的那个一个字节不动，外面套一层」：
/// [`Row`] 仍然是**过进程边界的那五格**（`proc::OpenRequest` 持的就是它，而那份文件在红线里），
/// `Listed` 是**窗口里的一行**。⚠ 刻意**只给 `Deref`、不给 `DerefMut`**：
/// 经这一层改不了里面那一行 ⇒ 「两格与那一行漂开」这件事**写都写不出来**。
///
/// # ⚠ 它的代价，就一条，写在这儿而不是藏着
///
/// 入口那条命令先列好、交给窗口进程的**第一屏**走的是 `Vec<Row>`（`proc.rs` 那个载荷）
/// ⇒ 那一屏**没有这两格**（画出来就是「没有箭头、没有时间」），
/// 第一次刷新 / 换目录之后就有了。这一格由 [`Listed::plain`] 的名字说出来，
/// 并由 `shell_tests::the_seeded_first_screenful_admits_it_has_no_link_or_time_column` 钉着。
/// 补它只要把 `proc::OpenRequest::rows` 换成 `Vec<Listed>`（一行），
/// 而**那份文件不在本刀写区**。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Listed {
    /// 那五格。
    pub row: Row,
    /// 🔴 这一项**是不是符号链接**。后端 `files.ls` 的 `kind` 与 SFTP 的 `is_symlink`
    /// 两条路都送得出它 —— 此前窗口这一侧把它**压掉了**（见 [`row_from_ls_entry`] 那张表）。
    pub link: bool,
    /// 最后修改时间（epoch 秒）。
    ///
    /// 🔴 `None` 逐字是「**这条路没送这一格**」，不是「这个文件没有时间」。
    /// 两者刻意不混：SFTP 那条退路交不出它（`SftpEntry` 里没有这一格），
    /// 而「后端送了一个 0」是 1970 年，那是一个真时间。
    pub mtime_secs: Option<u64>,
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
            mtime_secs: None,
        }
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
/// 形状上的先例逐字住 `src/bridge/Cargo.toml`（`creds-core` 那条 `harden` feature）：
/// 「『backend 写不了这份文件』是**编译器**兜的，不是一条判据兜的」。
///
/// 🔴〔第十三刀〕serde 那一对的理由同 [`Row`]：开窗那一跳要把它交给另一个进程。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Source(Box<RemoteConfig>);

impl Source {
    /// 造一个 —— **这是唯一的造法**。
    pub fn remote(cfg: RemoteConfig) -> Self {
        Source(Box::new(cfg))
    }

    /// 那台机器的配置。
    ///
    /// ⚠ 回引用而**不是**克隆：调用方里有一半只是要读 `origin_label()`，
    /// 而另一半要 `clone()` 一份丢进 tokio —— 让后者自己写那一声 `clone`，
    /// 别在这里替所有人付。
    pub fn cfg(&self) -> &RemoteConfig {
        &self.0
    }

    /// 给窗口标题/面包屑用的短名。
    pub fn label(&self) -> String {
        self.0.origin_label()
    }

    /// 这一趟问的是**哪台机器** —— 走 [`crate::origin::Origin`]，全仓那一个类型。
    ///
    /// 🔴 它与 [`Source::label`] **刻意分开两个函数**：`label` 是给人看的，
    /// 而这一个是**寻址用的**。两者今天由同一个 `origin_label()` 喂
    /// （本机那一侧退役之前不是这样：那时 `label` 回的是「本机」两个中文字，
    /// 而寻址要的是 `origin::LOCAL` 那个 `"<local>"`）。
    /// ⇒ **仍然不合并**：它们回的是同一个串是**今天的实况**，不是契约。
    ///
    /// ⚠ **刻意不回 `String`**：`origin_tests::no_new_raw_string_origin_parameters`
    /// 是一条递减棘轮 —— `设计/00 §2.5 ①` 逐字「origin 归一 —— 这是地基」，
    /// 新代码一律用这个类型，不许再给这个概念造一种表达。
    /// ⚠ 这里用 `origin_label()`，与 `ssh_source` 的 `stream_loop` 登记时
    /// 用的是**同一个函数** —— 两处漂开的症状是「命令发给了一个谁都没登记过的
    /// origin，而且不报错」（`inbound_client::LOCAL_ORIGIN` 的头注记过同一形）。
    pub fn origin(&self) -> crate::origin::Origin {
        crate::origin::Origin(self.0.origin_label())
    }
}

/// 上一级目录。
///
/// 🔴 **它只吃一条字符串，不吃 [`Source`]** —— 而那是本机那一侧退役买到的东西之一：
/// 远端路径**恒用 `/`**（SFTP 协议就是这么定的，对面是 Windows 也一样）
/// ⇒ 只剩一个算法。⚠ 别为了「看起来通用」把 `std::path` 换回来：
/// 它在 Windows 上会把 `\` 也当分隔符 ⇒ 远端一个名字里含反斜杠的目录会被切成两级。
///
/// ⚠ 到顶了就**返回原值**（不是空串、不是 `None`）—— 调用方靠「回来的和给出去的相等」
/// 判断「已经在顶上了」，这样「到顶」这件事不需要第二个返回通道。
pub fn parent_dir(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    if trimmed.is_empty() {
        // `/` 或空串：都已经在根上。
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

/// 远端路径的**最后一段**（basename）。
///
/// 🔴 抽成具名函数是因为盘上已经有**三处** `rsplit('/')` 各写了一份
/// （`corpus.rs` · `shell.rs` 那两处），而这一刀要的是第四处。
/// ⇒ 不再加第四份。它与 [`parent_dir`] 是**一对**（一个给前缀、一个给尾段），
/// 所以住同一处。
///
/// ⚠ **只用 `/`**，理由与 [`parent_dir`] 逐字相同：SFTP 协议恒用 `/`，
/// 拿 `std::path` 去切远端路径在 Windows 上会把 `\` 也当分隔符。
/// ⚠ 那三处旧写法**本刀不动**（它们各在自己的语境里，改它们是另一件活）——
/// 如实登记在这儿，别以为这个概念只有一个住址。
pub fn remote_basename(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(i) => &t[i + 1..],
        None => t,
    }
}

/// 按什么排。**闭集三档，与旧面板那个下拉逐格同名**
/// （`src/sftp/paths.ts::SortBy` 的 `"name" | "size" | "type"`）。
///
/// 🔴 **缺省是 [`SortBy::Name`]，而那一档就是本刀之前那个写死的纯函数** ——
/// 逐字节相同这件事怎么证的，住本模块头注最后那一节。
///
/// ⚠ **刻意没有第四档「按时间」**，尽管这一刀刚把 `mtime_secs` 接进来：
/// 用户裁的是「旧面板有、新窗口没有」那五项，而旧面板那个下拉只有三档
/// ⇒ 加第四档是一次**谁都没要求的行为变更**。如实登记为**没做**，
/// 接它只要在这里加一格 ＋ 在 [`sort_rows`] 里加一支（`mtime_secs` 已经在行上了）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    /// 名称（小写不敏感）。**缺省**。
    #[default]
    Name,
    /// 大小，**大的在前**（同旧面板：`b.size - a.size`）。
    Size,
    /// 扩展名。
    Type,
}

impl SortBy {
    /// 三档全体 —— **界面那个下拉的唯一人群**，别在 `shell.rs` 里另写一份名单。
    pub const ALL: [SortBy; 3] = [SortBy::Name, SortBy::Size, SortBy::Type];

    /// 下拉里那一格写什么。
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Name => "名称",
            SortBy::Size => "大小",
            SortBy::Type => "类型",
        }
    }
}

/// 一个名字的**扩展名**（小写）。
///
/// ⚠ **逐字照旧面板那一份**（`paths.ts::sortEntries` 里那个 `ext`：
/// `n.slice(n.lastIndexOf(".") + 1).toLowerCase()`）—— 包括它那个怪处：
/// 名字里**没有点**的时候它回的是**整个名字**（`lastIndexOf` 回 `-1`，`slice(0)`）。
/// 那不是我抄错了，那是「按类型排」在旧面板上今天的实况；
/// 换掉它是一次行为变更，不在本刀射程里。判据 `sorting_by_type_matches_the_old_panel`
/// 里那一档阴性对照钉的就是这个怪处。
fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) => name[i + 1..].to_lowercase(),
        None => name.to_lowercase(),
    }
}

/// 目录在前，再按 `by` 排。`by` 相持时**一律回落到名称小写**（同旧面板那个 `|| cmpName`）。
///
/// 🔴 **这是「屏幕上那一屏是什么序」的唯一住址。** `Name` 那一支逐字节等于
/// 本刀之前那个写死的版本（那时契约逐字是「目录在前，再按名称小写排，
/// 与 `sftp_pool::sort_entries` 同」）——多出来的只是一个在那一档返回 `Equal`
/// 的 `then_with`，它对结果零影响。
pub fn sort_rows(v: &mut [Listed], by: SortBy) {
    v.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| match by {
                // 🔴 这一支**必须**是 `Equal`：它就是「默认与从前逐字节相同」那句话的落点。
                SortBy::Name => std::cmp::Ordering::Equal,
                SortBy::Size => b.size.cmp(&a.size),
                SortBy::Type => ext_of(&a.name).cmp(&ext_of(&b.name)),
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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

/// epoch 天数 → 公历 `(年, 月, 日)`。
///
/// 🔴 **它是 `crate::utils::days_from_civil` 的逆**（同一篇算法：Howard Hinnant
/// 的 `civil_from_days`）。⚠ **两个方向没住在一起，如实登记**：正向那一份住
/// `utils.rs`，而那份文件不在本刀写区 ⇒ 逆向这一份落在这儿。
/// 接住这处分家的**不是**一句注释：`source_tests::the_two_date_algorithms_are_each_others_inverse`
/// 拿正向那一份当对照，在一段稠密的日子上断**往返恒等**
/// ⇒ 两份漂开（任一侧被改错）当场红，而且两侧**不同源**。
///
/// ⚠ 本函数不带时区：它答的是 **UTC**。理由住 [`format_mtime`]。
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 一格 `mtime_secs` → 列表里那一列写什么。
///
/// # 🔴 为什么尾巴上有一个 `Z`（而不是「看起来干净」的裸时间）
///
/// 本进程手上**没有时区**：仓里一个 `chrono` / `time` 都没有
/// （`src/bridge/Cargo.toml` 的依赖表现打），`SystemTime` 只给 UTC。
/// 而**画一个不说自己是哪个时区的时间，是一次静默的错**：用户会按本地时读它，
/// 在东八区就是差 8 小时 —— 而「文件是什么时候改的」正是他要拿来做判断的东西。
///
/// ⚠ 还有一条比时区更硬的：这是**远端那台机器上**的文件时间，
/// 而那台机器的时区与你面前这台**本来就可能不同** ⇒ 换成「本地时间」也不解决问题，
/// 只是把错的方向换一个。⇒ 一律 UTC，并且**在每一行上说出来**（一个 `Z`，一个字符）。
///
/// ⚠ **买不到什么**：它不会去问那台远端的时区，也没有「几分钟前」这种相对说法。
/// 如实登记为没做。
pub fn format_mtime(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let (hh, mm) = (rem / 3600, (rem % 3600) / 60);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}Z")
}

/// 列一个**本机**目录。
///
/// # 🔴 它在生产路径上**零消费者**，而它还在盘上 —— 逐条理由，不是「以后可能用得上」
///
/// 本机那一侧退役（见本模块头注那块墓碑）之后，它唯一的消费者是
/// `tests/bridge/filewin/find_testing.rs::walk` —— 搜索那一族判据用的**合成后端**
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
    let rd = std::fs::read_dir(dir).map_err(|e| format!("读目录失败: {e}"))?;
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

/// `SftpEntry` → [`Listed`] 的映射。
///
/// 🔴 **抽成具名函数是为了让它可判。** [`list_remote`] 整条路要真远端才跑得起来，
/// 而本仓红线不许起真连接（`tests/bridge/sftp_tests.rs` 逐字：
/// 「跑不了真路（要先连上远端 / 红线不许起真连接）」）
/// ⇒ 那条路上**唯一有逻辑的一段**就是这里，把它抽出来，它就有判据了。
///
/// 🔴〔补齐五项 2026-09-23〕**`is_symlink` 这一格此前在这里被丢掉了。**
/// `SftpEntry` 一直在送它（`sftp_pool.rs` 那个结构体里就有），而旧面板拿它画 `↳`
/// —— 也就是说「看不出哪个是符号链接」这条缺口在**退路**这一侧同样是
/// 「数据在送、窗口不画」，不只主路那一侧。
///
/// ⚠ `mtime_secs` 照旧是 `None`，而那**不是丢掉**：`SftpEntry` 里压根没有这一格
/// （SFTP 那条路交不出它）⇒ 这是 [`Listed::mtime_secs`] 头注里那个
/// 「没送」与「没有」的区别的实例。
pub fn row_from_sftp_entry(e: crate::sftp_pool::SftpEntry) -> Listed {
    Listed {
        row: Row {
            name: e.name,
            path: e.path,
            is_dir: e.is_dir,
            size: e.size,
            lossy_name: e.lossy_name,
        },
        link: e.is_symlink,
        mtime_secs: None,
    }
}

/// 远端 `.` 解出来的那条路径能不能当**起点**用。
///
/// # 🔴 抽成纯函数的理由与 [`row_from_sftp_entry`] 同一条
///
/// [`resolve_remote_home`] 整条路要真远端才跑得起来（本仓红线不许起真连接）
/// ⇒ 那条路上**唯一有逻辑的一段**就是这里，把它抽出来，它就有判据了。
///
/// # 为什么要判一道，而不是把回值直接拿去开窗
///
/// 入口那条命令吃的是一条**绝对路径**。`canonicalize(".")` 在规矩的 SFTP 服务端上
/// 回的就是绝对路径，但那是**对面的承诺**，不是我们的不变量 ——
/// 空串 / 相对路径 / 一串空白，任一种直接拿去开窗都是「窗口出来了、里面是空的」，
/// 而那正是 [`super::entry::open_file_window`] 头注花一整节要避免的那一形。
/// ⇒ 不合格就**带着原文回错**，让 webview 那侧照旧弹它的失败提示。
///
/// ⚠ 末尾的 `/` 会被剥掉（根 `/` 除外）—— 那不是洁癖：
/// [`parent_dir`] 靠「回来的和给出去的相等」判「已经在顶上了」，
/// 而 `/srv/` 与 `/srv` 在那个算法里是两个不同的输入。
pub fn start_dir_from_realpath(answer: &str) -> Result<String, String> {
    let t = answer.trim();
    if t.is_empty() {
        return Err("远端把 home 解成了空路径 —— 没有起点可以开窗".into());
    }
    if !t.starts_with('/') {
        return Err(format!(
            "远端把 home 解成了一条相对路径 `{t}` —— 起点必须是绝对路径"
        ));
    }
    let trimmed = t.trim_end_matches('/');
    Ok(if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    })
}

/// 问远端「`.` 是哪儿」—— 也就是那台机器上的 home 绝对路径。
///
/// 🔴 **这是 `sftp_realpath` 在窗口这一侧的唯一落点。** 在这之前，那条路径的
/// 唯一来源是老面板 `src/sftp/panel.ts` 那处 `sftp_realpath(cfg, ".")`
/// ⇒ 窗口连「自己开在远端 home」都做不到，而 `P3`（老面板退役）因此排不动。
///
/// ⚠ 本函数**自己没有逻辑** —— 判的那一道住 [`start_dir_from_realpath`]。
pub async fn resolve_remote_home(cfg: &RemoteConfig) -> Result<String, String> {
    let answer = crate::sftp_pool::sftp_realpath(cfg.clone(), ".".to_string()).await?;
    start_dir_from_realpath(&answer)
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴〔第十二刀 2026-09-22〕**后端做，前端拿结果** —— 用户指令的第一步
// ═══════════════════════════════════════════════════════════════════════
//
// 用户 2026-09-22 逐字：「文件管理器不应该全部依赖前端 / 应该像我们现在一样的架构 /
// **即后端做, 前端拿结果, 这样才能 0 流量**」。
// 设计稿住 `设计/60 §8`；本段是它的**第 1 步**（只读那一侧，无裁决前置）。
//
// # 为什么这一步不需要任何裁决
//
// 后端**已经有** `files.ls`（`src/backend/files/mod.rs::CAPABILITIES`），而它
// **今天零消费者**。接上去是纯增量，`readonly_guard` 那 4259 行一行不动
//（`files.ls` 的 `effect` 是 `ReadsOnly`）。
//
// # 🔴 为什么是「主路 ＋ 申报过的退路」而不是「把旧路删掉」
//
// 后端**不是恒在的**，两条现打的理由：
// - **后端没推上去 / 没起来**，而 SSH 本身是通的。
// - 还有一条形状上的：窗口**可能没有 tokio 运行时**（判据里大量
//   `FileWindow::seeded(…, None, rows)`）⇒ 问不了后端。
//   ⚠ 这一条的**后果**在本机侧退役之后变了：从前没有运行时还能退回本机 `read_dir`，
//     今天它只剩一种结局 —— 出声（`shell::FileWindow::reload` 那一支）。
//
// ⚠ 从前这一节还有第三条理由，逐字「**本机**：`build.rs` 那句警告 ……
//   裸可执行文件起不了本机后端」。那一条**随本机侧一起退役**：
//   本机没有后端这件事仍然是真的，但这个窗口不再往本机看 ⇒ 它不是这里的退路理由了。
//
// ⇒ 形状照本仓现成的那个（`sftp_pool::CopyVerdict`：走了快路回 `None`，
//   **退了路回一句话**）：[`ListVerdict`]。**退路不许静默** —— 那正是
//   「零流量退化成 2× 流量」当初要出声的同一条纪律。
//
// # ⚠ 这一步**没有**做到什么（别读宽）
//
// - **两份实现还在**，只是降级成了退路。「两份变一份」要等到后端恒在那天
//   （那是另一件事，不是这一刀）。
// - **写那一族一条都没搬**（`设计/60 §8.3` 那道政策题还没拍）。
// - **`Row` 仍然持字符串不持字节** —— 后端那一侧已经走原始字节了
//   （`files.ls` 的 `path` 是 `{"b16":…}` 或字符串），而这一侧还在入口处
//   有损转一次。改 `Row` 会动到六个模块与四十来条判据 ⇒ 单独一刀。
//   ⚠ 但 [`Row::lossy_name`] 这一格**当场变准了**：从前是
//   「名字里含 U+FFFD」（一个**猜**，真叫这个名字的文件会被误判），
//   现在是「那串字节不是合法 UTF-8」（**事实**）。

/// 那条线上命令的名字。⚠ 能力名是 `files.ls`，线上名是 `files-ls`
/// （两者刻意不同形，同 `find.rs` 头注那条）。
pub const CMD_LS: &str = "files-ls";

/// 一趟列目录**走没走成主路**。`None` = 后端答的；`Some(说明)` = 退了路。
///
/// 🔴 与 `sftp_pool::CopyVerdict` 同形、同理由：一次静默降级与一次成功
/// 在屏幕上长得一样，而代价（这里是「分层退回前端」，那里是「2× 流量」）是真的。
pub type ListVerdict = Option<String>;

/// 一趟 `files-ls` 回来的 `entries` 里的**一条** → [`Listed`]。
///
/// 🔴 **抽成具名函数是为了让它可判**（同 [`row_from_sftp_entry`] 的理由）：
/// [`list_via_backend`] 整条路要一条真后端通道才跑得起来，
/// 而那条路上**唯一有逻辑的一段**就是这里。
///
/// # 🔴 逐格说明 —— **这张表现在是一条判据，不是散文**
///
/// | 后端送的 | 窗口这一行的哪一格 | 怎么来的 |
/// |---|---|---|
/// | `path`（字符串 或 `{"b16":…}`） | `path` · `name` · `lossy_name` | 先解成**字节**，再取尾段作名字；有损与否看那串字节 |
/// | `kind`（`dir`/`file`/`symlink`/`other`） | `is_dir` ＋ `link` | `dir` ⇒ 目录；`symlink` ⇒ 链接。**两格，不是一格** |
/// | `size`（可能缺） | `size` | 缺就是 0（同本机那条：读不到 stat 不整趟失败） |
/// | `mtime_secs`（可能缺） | `mtime_secs` | 原样带上来；缺就是 `None`（＝**没送**，不是 1970） |
/// | `entries` | 那一屏有几行 | 由 [`rows_from_ls_data`] 摊开 |
/// | `truncated` | 界面上那句「只拿到了前 N 条」 | 同上 |
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
        .ok_or_else(|| "`files-ls` 的一条 entry 里没有 `path`".to_string())?;
    let bytes = super::find::decode_path(raw)
        .ok_or_else(|| "`path` 的形状不对 —— 只认字符串或 `{\"b16\": …}`".to_string())?;
    // 🔴 有损与否看**字节**，不看转出来的那个串里有没有 U+FFFD。
    //    后者是一个猜：真叫 `\u{FFFD}` 的文件会被误判成有损。
    let lossy_name = std::str::from_utf8(&bytes).is_err();
    let path = String::from_utf8_lossy(&bytes).to_string();
    let name = remote_basename(&path).to_string();
    // 🔴 `kind` 落**两格**，不是一格：压成一个布尔就是「看不出哪个是符号链接」。
    let kind = v.get("kind").and_then(|k| k.as_str());
    let is_dir = kind == Some("dir");
    let link = kind == Some("symlink");
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
        // 🔴 原样带上来。缺了就是 `None`（＝**后端没送**）—— 不许兜底成 0，
        //    那是 1970-01-01，一个看起来很像真读数的假时间。
        mtime_secs: v.get("mtime_secs").and_then(serde_json::Value::as_u64),
    })
}

/// 问**那台机器上的后端**要一个目录。
///
/// 回 `(行, 被截断了吗)`。⚠ 截断要**画出来** —— 「目录里就这么多」与
/// 「后端截断了」在屏幕上长得一样，而那正是本仓的头号病形。
///
/// ⚠ 本函数**自己没有逻辑**，判的那一段住 [`row_from_ls_entry`]。
pub async fn list_via_backend(
    origin: &crate::origin::Origin,
    dir: &str,
    limit: usize,
    by: SortBy,
) -> Result<(Vec<Listed>, bool), String> {
    let args = serde_json::json!({ "path": dir, "limit": limit });
    let d = super::find::call_one(origin, CMD_LS, args, std::time::Duration::from_secs(20))
        .await
        .map_err(super::find::routed_text)?;
    rows_from_ls_data(&d, by)
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
pub fn rows_from_ls_data(d: &serde_json::Value, by: SortBy) -> Result<(Vec<Listed>, bool), String> {
    let arr = d
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "`files-ls` 的 `entries` 不是一个数组".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, one) in arr.iter().enumerate() {
        out.push(row_from_ls_entry(one).map_err(|e| format!("第 {i} 条 entry：{e}"))?);
    }
    // 🔴 **排序在这一侧，而且它只有一份** —— 后端不排（它答的是目录项，不是一屏）。
    //    「按什么排」由调用方给（用户在工具栏上选的那一档），算法只有 `sort_rows` 这一个家。
    sort_rows(&mut out, by);
    let truncated = d
        .get("truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok((out, truncated))
}

/// 一屏最多要多少行。
///
/// ⚠ 后端那侧有自己的默认值（`files/mod.rs::DEFAULT_LIMIT`）；这里**显式给**，
/// 因为「一屏多少」是调用方的事（同 `设计/60 §3.5.2a` 那条「机制在后端 ·
/// 节拍归调用方」的形）。取这个数是因为窗口是**虚拟滚动**的
/// （`ScrollArea::show_rows`），一次拿多少不影响帧时，只影响一次往返的大小。
pub const LS_LIMIT: usize = 50_000;

/// 列一个目录 —— **先问后端，问不到就退回旧路并出声**。
///
/// 回 `(行, 截断了吗, 走没走成主路)`。逐条理由住本段上方那一节。
///
/// 🔴〔补齐五项 2026-09-23〕**`by` 这一格让两支走同一个显示序。**
/// 退路那一支回来的东西在池子里按「目录在前、名称小写」排过一次
/// ⇒ 用户选了「大小」的时候，不在这儿再排一遍就会**静默忽略他的选择**
/// （而屏幕上那个下拉写着「大小」）。
pub async fn list_dir(
    source: &Source,
    dir: &str,
    by: SortBy,
) -> Result<(Vec<Listed>, bool, ListVerdict), String> {
    match list_via_backend(&source.origin(), dir, LS_LIMIT, by).await {
        Ok((rows, truncated)) => Ok((rows, truncated, None)),
        Err(why) => {
            // ── 退路：旧那条路，原样 ────────────────────────────────
            // ⚠ 本机那一侧退役之后这里**只剩一支**（从前是两支）。
            //   `#[allow]` 一个字都不要：少一支不是少一个判据，
            //   是那一支要判的东西整条不在了（头注那块墓碑）。
            let mut rows = list_remote(source.cfg(), dir).await?;
            sort_rows(&mut rows, by);
            Ok((rows, false, Some(why)))
        }
    }
}

/// 列一个**远端**目录 —— 直接调 `sftp_pool`，同进程、无 IPC。
pub async fn list_remote(cfg: &RemoteConfig, dir: &str) -> Result<Vec<Listed>, String> {
    let entries = crate::sftp_pool::sftp_list_dir(cfg.clone(), dir.to_string()).await?;
    // `sftp_list_dir` 内部已按生产契约排过序 ⇒ **本函数不再排一次**。
    // ⚠ 显示序那一步由 [`list_dir`] 统一做（用户选的那一档）—— 它对
    //   `Name` 那一档是恒等（稳定排序作用在已经有序的那一摞上），
    //   对另两档才真的重排。两件事刻意分层：本函数答「那个目录里有什么」，
    //   `list_dir` 答「屏幕上按什么序摆」。
    Ok(entries.into_iter().map(row_from_sftp_entry).collect())
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/source_tests.rs"]
mod tests;
