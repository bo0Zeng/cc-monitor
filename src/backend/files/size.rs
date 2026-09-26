//! 〔W5-FILES · 第五波 · 2026-09-25〕**`files.size`：算一个目录有多大** —— `设计/60 §6.2`「算目录大小」。
//!
//! 用户 V45「我能连 ssh 对机器文件进行什么操作，后端就应该能进行什么操作」＋「我觉得要零流量」：
//! 在那台机器上走一遍、只回几个数，字节不过网。设计住 `调研/第四波记录/W5-FILES.md` §2.3。
//!
//! # 走法（逐条）
//!
//! - **显式栈，不递归**（理由同 `index::build`：目录深度是用户数据说了算的）。
//! - **不跟链接**：链接记数（`links`），**不算字节**（它指向的东西不一定在这棵树里，算进来会重复或越界）。
//! - `bytes` ＝ 普通文件的**表观大小**之和（`len()`，与列表那一栏同口径；不是占盘块数）。
//! - 挂在底下的**别的文件系统不进去**（`skipped_mounts` 记数，设备号走 `platform::paths::device_of`，`设计/60 §3.7`）；
//!   非 unix 上设备号问不出 ⇒ 那一判不开口（全当同一个文件系统），如实登记。
//! - 读不进去的目录记数（`unreadable_dirs`），不中断。
//! - `path` 是一个文件 ⇒ 回它自己（`files: 1`）；顶上那一格**跟链接**（与 `files.stat` 同一个读法）。
//!
//! # ⚠ 不设条目上限（如实）
//!
//! 与 `index::build` 同形（本族走整棵树的那一个先例也不设）：阻塞档、开跑之后取消不掉，
//! 在一棵极大的树上就是一趟很长的往返 —— 调用方的期限是它唯一的上界。纯读，一个字节不写（`设计/96 §2.9` 边界①）。

use std::path::{Path, PathBuf};

/// 一趟算下来的几个数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Measured {
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    /// 不是目录、文件、链接的（设备 / 管道 / 套接字）。
    pub other: u64,
    pub skipped_mounts: u64,
    pub unreadable_dirs: u64,
}

/// 生产入口：设备号走 `platform/`。顶上那一格读不到 ⇒ `Err`（调用方回 `unreadable`）。
pub fn measure(top: &Path) -> std::io::Result<Measured> {
    measure_with(top, crate::platform::paths::device_of)
}

/// [`measure`] 的本体，设备号由调用方给（判据注入「这个子目录在另一个设备上」—— 真挂载点在测试里造不出来）。
pub fn measure_with(
    top: &Path,
    device_of: impl Fn(&Path) -> Option<u64>,
) -> std::io::Result<Measured> {
    let md = std::fs::metadata(top)?;
    let mut m = Measured::default();
    if !md.is_dir() {
        if md.is_file() {
            m.files = 1;
            m.bytes = md.len();
        } else {
            m.other = 1;
        }
        return Ok(m);
    }
    let top_dev = device_of(top);
    m.dirs = 1;
    let mut stack: Vec<PathBuf> = vec![top.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            m.unreadable_dirs += 1;
            continue;
        };
        for entry in listing {
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(ft) = entry.file_type() else {
                m.other += 1;
                continue;
            };
            if ft.is_symlink() {
                m.links += 1;
            } else if ft.is_dir() {
                let path = entry.path();
                if top_dev.is_some() && device_of(&path) != top_dev {
                    m.skipped_mounts += 1;
                    continue;
                }
                m.dirs += 1;
                stack.push(path);
            } else if ft.is_file() {
                m.files += 1;
                m.bytes += entry.metadata().map(|x| x.len()).unwrap_or(0);
            } else {
                m.other += 1;
            }
        }
    }
    Ok(m)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/size_tests.rs"]
mod tests;
