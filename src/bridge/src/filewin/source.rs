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
