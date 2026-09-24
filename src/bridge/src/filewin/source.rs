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
//! # ⚠ 排序：一个契约，盘上有两份实现，而本刀够不着另一份
//!
//! 「目录在前，再按名称小写排」这条契约，生产侧的落点是
//! `sftp_pool.rs::sort_entries`（`:673`）。它是**私有**的 ⇒ 本模块调不到，
//! 只能再写一份 [`sort_rows`]。
//!
//! 🔴 **如实登记这个缝**：退路那条（`sftp_list_dir`）的序来自它内部（生产实现），
//! 主路那条（`files-ls`）的序来自 [`sort_rows`]（第二份实现）。
//! [`tests::the_two_orderings_agree_on_a_synthetic_set`] 把**两条路的真实输出**
//! 对拍成相等，所以「两份漂开」这件事**是有判据的**；
//! 但它对拍的是**行为**，不是「只有一份实现」—— 后者要等 `sort_entries` 提级成
//! 共用件才谈得上，**本刀不动 `sftp*.rs`**（另一路在改那棵树）。

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

/// 目录在前，再按名称小写排序。契约与 `sftp_pool::sort_entries` 同。
pub fn sort_rows(v: &mut [Row]) {
    v.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
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
pub fn list_local(dir: &Path) -> Result<Vec<Row>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("读目录失败: {e}"))?;
    let mut out: Vec<Row> = Vec::new();
    for entry in rd.flatten() {
        let raw = entry.file_name();
        let name = raw.to_string_lossy().to_string();
        let lossy_name = name.contains('\u{FFFD}');
        // `metadata()` 跟随符号链接；跟不动（悬空链接）就退回 `symlink_metadata`。
        let meta = entry
            .metadata()
            .or_else(|_| std::fs::symlink_metadata(entry.path()))
            .ok();
        out.push(Row {
            path: entry.path().to_string_lossy().to_string(),
            is_dir: meta.as_ref().is_some_and(|m| m.is_dir()),
            size: meta.as_ref().map_or(0, |m| m.len()),
            lossy_name,
            name,
        });
    }
    sort_rows(&mut out);
    Ok(out)
}

/// `SftpEntry` → [`Row`] 的映射。
///
/// 🔴 **抽成具名函数是为了让它可判。** [`list_remote`] 整条路要真远端才跑得起来，
/// 而本仓红线不许起真连接（`tests/bridge/sftp_tests.rs` 逐字：
/// 「跑不了真路（要先连上远端 / 红线不许起真连接）」）
/// ⇒ 那条路上**唯一有逻辑的一段**就是这里，把它抽出来，它就有判据了。
pub fn row_from_sftp_entry(e: crate::sftp_pool::SftpEntry) -> Row {
    Row {
        name: e.name,
        path: e.path,
        is_dir: e.is_dir,
        size: e.size,
        lossy_name: e.lossy_name,
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

/// 一趟 `files-ls` 回来的 `entries` 里的**一条** → [`Row`]。
///
/// 🔴 **抽成具名函数是为了让它可判**（同 [`row_from_sftp_entry`] 的理由）：
/// [`list_via_backend`] 整条路要一条真后端通道才跑得起来，
/// 而那条路上**唯一有逻辑的一段**就是这里。
///
/// # 逐格说明（后端给的与窗口要的不是同一张表）
///
/// | 后端 | 窗口 | 怎么来的 |
/// |---|---|---|
/// | `path`（字符串 或 `{"b16":…}`） | `path` · `name` · `lossy_name` | 先解成**字节**，再取尾段作名字；有损与否看那串字节 |
/// | `kind`（`dir`/`file`/`symlink`/`other`） | `is_dir` | 只有 `dir` 算目录 |
/// | `size`（可能缺） | `size` | 缺就是 0（同本机那条：读不到 stat 不整趟失败） |
/// | `mtime_secs` | —— | 窗口今天不画它 |
///
/// ⚠ **`kind` 比 `is_dir` 细** —— 后端分得出符号链接，而窗口今天分不出。
/// 那一格**本刀没接**（要在行上多画一种标记），如实登记。
pub fn row_from_ls_entry(v: &serde_json::Value) -> Result<Row, String> {
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
    let is_dir = v.get("kind").and_then(|k| k.as_str()) == Some("dir");
    let size = v
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Ok(Row {
        name,
        path,
        is_dir,
        size,
        lossy_name,
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
) -> Result<(Vec<Row>, bool), String> {
    let args = serde_json::json!({ "path": dir, "limit": limit });
    let d = super::find::call_one(origin, CMD_LS, args, std::time::Duration::from_secs(20))
        .await
        .map_err(super::find::routed_text)?;
    rows_from_ls_data(&d)
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
pub fn rows_from_ls_data(d: &serde_json::Value) -> Result<(Vec<Row>, bool), String> {
    let arr = d
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "`files-ls` 的 `entries` 不是一个数组".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, one) in arr.iter().enumerate() {
        out.push(row_from_ls_entry(one).map_err(|e| format!("第 {i} 条 entry：{e}"))?);
    }
    // 🔴 **排序在这一侧，而且主路上只有这一份** —— 后端不排（它答的是目录项，不是一屏）。
    //    「目录在前、名称小写排」是**显示序**，那是前端的活；
    //    这一步让那个契约从「两份实现对拍」变成「主路上只有一份」。
    sort_rows(&mut out);
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
pub async fn list_dir(source: &Source, dir: &str) -> Result<(Vec<Row>, bool, ListVerdict), String> {
    match list_via_backend(&source.origin(), dir, LS_LIMIT).await {
        Ok((rows, truncated)) => Ok((rows, truncated, None)),
        Err(why) => {
            // ── 退路：旧那条路，原样 ────────────────────────────────
            // ⚠ 本机那一侧退役之后这里**只剩一支**（从前是两支）。
            //   `#[allow]` 一个字都不要：少一支不是少一个判据，
            //   是那一支要判的东西整条不在了（头注那块墓碑）。
            let rows = list_remote(source.cfg(), dir).await?;
            Ok((rows, false, Some(why)))
        }
    }
}

/// 列一个**远端**目录 —— 直接调 `sftp_pool`，同进程、无 IPC。
pub async fn list_remote(cfg: &RemoteConfig, dir: &str) -> Result<Vec<Row>, String> {
    let entries = crate::sftp_pool::sftp_list_dir(cfg.clone(), dir.to_string()).await?;
    // `sftp_list_dir` 内部已按生产契约排过序 ⇒ **这里不再排一次**
    // （再排一次就等于把序的真相源搬到本模块来，那正是上面头注说的那个缝会变宽的方式）。
    Ok(entries.into_iter().map(row_from_sftp_entry).collect())
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/source_tests.rs"]
mod tests;
