//! `24e` 数据面：**列一个目录**。本机走文件系统，远端走已有的 SFTP 那一套。
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
//! 只能为本机那一侧再写一份 [`sort_rows`]。
//!
//! 🔴 **如实登记这个缝**：远端那条路的序来自 `sftp_list_dir` 内部（生产实现），
//! 本机那条路的序来自 [`sort_rows`]（第二份实现）。
//! [`tests::the_two_orderings_agree_on_a_synthetic_set`] 把**两条路的真实输出**
//! 对拍成相等，所以「两份漂开」这件事**是有判据的**；
//! 但它对拍的是**行为**，不是「只有一份实现」—— 后者要等 `sort_entries` 提级成
//! 共用件才谈得上，**本刀不动 `sftp*.rs`**（另一路在改那棵树）。

use std::path::Path;

use crate::ssh_source::RemoteConfig;

/// 文件列表里的一行。**故意比 `SftpEntry` 窄** —— 列表只画得下这些。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// 绝对路径。远端恒用 `/`；本机用本机分隔符。
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// 非 UTF-8 文件名有损显示 —— 本机侧同样会有（`to_string_lossy`）。
    pub lossy_name: bool,
}

/// 这个窗口现在在看哪儿。
#[derive(Clone, Debug)]
pub enum Source {
    Local,
    Remote(Box<RemoteConfig>),
}

impl Source {
    /// 给窗口标题/面包屑用的短名。
    pub fn label(&self) -> String {
        match self {
            Source::Local => "本机".to_string(),
            Source::Remote(cfg) => cfg.origin_label(),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Source::Remote(_))
    }

    /// 这一趟问的是**哪台机器** —— 走 [`crate::origin::Origin`]，全仓那一个类型。
    ///
    /// 🔴 它与 [`Source::label`] **刻意分开两个函数**：`label` 是给人看的
    /// （本机那一格逐字是「本机」两个中文字），而这一个是**寻址用的**。
    /// 合成一个就得让「本机」去当入方向登记表的键，而那张表的本机键是
    /// `origin::LOCAL`（`"<local>"`）。
    ///
    /// ⚠ **刻意不回 `String`**：`origin_tests::no_new_raw_string_origin_parameters`
    /// 是一条递减棘轮 —— `设计/00 §2.5 ①` 逐字「origin 归一 —— 这是地基」，
    /// 新代码一律用这个类型，不许再给这个概念造一种表达。
    /// ⚠ 远端那一支用 `origin_label()`，与 `ssh_source` 的 `stream_loop` 登记时
    /// 用的是**同一个函数** —— 两处漂开的症状是「命令发给了一个谁都没登记过的
    /// origin，而且不报错」（`inbound_client::LOCAL_ORIGIN` 的头注记过同一形）。
    pub fn origin(&self) -> crate::origin::Origin {
        match self {
            Source::Local => crate::origin::Origin::local(),
            Source::Remote(cfg) => crate::origin::Origin(cfg.origin_label()),
        }
    }
}

/// 上一级目录。
///
/// 🔴 **两侧不是同一个算法，所以它吃 `Source` 而不是只吃一个字符串**：
/// 远端路径**恒用 `/`**（SFTP 协议就是这么定的，对面是 Windows 也一样），
/// 本机路径用**本机分隔符**。拿 `std::path` 去切远端路径，在 Windows 上会把
/// `\` 也当分隔符 ⇒ 远端一个名字里含反斜杠的目录会被切成两级。
///
/// ⚠ 到顶了就**返回原值**（不是空串、不是 `None`）—— 调用方靠「回来的和给出去的相等」
/// 判断「已经在顶上了」，这样「到顶」这件事不需要第二个返回通道。
pub fn parent_dir(source: &Source, cwd: &str) -> String {
    match source {
        Source::Remote(_) => {
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
        Source::Local => std::path::Path::new(cwd)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| cwd.to_string()),
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
