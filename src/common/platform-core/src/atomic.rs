//! 自家状态文件（不是用户文件）的原子写：后端 · monitor · 文件窗口三个写口只经 [`write_atomic`]。
//!
//! 一种语义（三份里最严的拼起来）：
//! 1. 旁名由调用方开（[`create_new_private`]：`O_EXCL`，unix 出生即 0600；后端交 `creds_core::perm::create_private`，
//!    Windows 上建出来就是只给本人的 DACL —— 那条安全性质只住 creds-core，开 `harden` 的只有后端）。
//!    开不出来 ⇒ 那个名字上的东西不是我们的，什么都不删。
//! 2. 正文由调用方往里写（流式写不用先攒整份）→ `sync_all`：换上去之前正文已落盘，崩在换名前后盘上都是完整的某一份。
//! 3. 换上去：目标已在 ⇒ Windows `ReplaceFileW`（保目标原有的 ACL、写穿）· unix `rename`；目标不在 ⇒ `rename`。
//! 4. 2、3 任何一步失败 ⇒ 删掉自己建的旁名，目标原样。
//!
//! 旁名叫什么由调用方定（各家的旁名形状各有认它的一侧），必须与目标同目录（换名不跨盘）。
//! 用户的文件不在这里：经那台机器的后端文件管理面写。

use copy_core::{Discard as _, Why};
use std::fs::File;
use std::io;
use std::path::Path;

/// 卡在哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 开旁名（没建出来：什么都没删）。
    CreateTmp,
    /// 写旁名 · 落盘（旁名已删）。
    WriteTmp,
    /// 换上去（旁名已删）。
    Replace,
}

/// 原子写没成：卡在哪一步 ＋ 系统原话。目标一定是原样。
#[derive(Debug)]
pub struct AtomicFail {
    pub stage: Stage,
    pub err: io::Error,
}

/// 新建一份只给本人的旁名（已在 ⇒ 拒）。unix 0600；别的平台照常建（ACL 按父目录继承，换上去时保目标原有的）。
pub fn create_new_private(at: &Path) -> io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    opts.open(at)
}

/// 整份原子写 `path`：开 `tmp`（`create_tmp`）→ `fill` 写正文 → 落盘 → 换上去；失败删旁名、目标原样。
pub fn write_atomic(
    path: &Path,
    tmp: &Path,
    create_tmp: impl FnOnce(&Path) -> io::Result<File>,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
) -> Result<(), AtomicFail> {
    let mut f = create_tmp(tmp).map_err(|err| AtomicFail {
        stage: Stage::CreateTmp,
        err,
    })?;
    let result = fill(&mut f)
        .and_then(|()| f.sync_all())
        .map_err(|err| AtomicFail {
            stage: Stage::WriteTmp,
            err,
        })
        .and_then(|()| {
            drop(f);
            replace(tmp, path).map_err(|err| AtomicFail {
                stage: Stage::Replace,
                err,
            })
        });
    if result.is_err() {
        std::fs::remove_file(tmp).discard(Why::CleanupAfterFailure); // 主错误已在回；删不掉只剩一份只给本人的旁名
    }
    result
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn ReplaceFileW(
        replaced: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        exclude: *mut core::ffi::c_void,
        reserved: *mut core::ffi::c_void,
    ) -> i32;
}

/// 换上去：目标已在 ⇒ `ReplaceFileW`（保目标的 ACL、写穿）；不在 ⇒ `rename`（标准库自己给长路径加前缀）。
#[cfg(windows)]
fn replace(src: &Path, dst: &Path) -> io::Result<()> {
    /// `REPLACEFILE_WRITE_THROUGH`。
    const WRITE_THROUGH: u32 = 0x0000_0001;
    if !dst.exists() {
        return std::fs::rename(src, dst);
    }
    // 带长路径前缀：过 260 字符的路径照样换得上。
    let (src_w, dst_w) = (
        win_path_core::win32_path(src)?,
        win_path_core::win32_path(dst)?,
    );
    // SAFETY：两条路径以 0 结尾、活到调用结束；备份名给空 = 不留备份；后两格保留，给空。
    let ok = unsafe {
        ReplaceFileW(
            dst_w.as_ptr(),
            src_w.as_ptr(),
            std::ptr::null(),
            WRITE_THROUGH,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// 换上去：同目录 `rename` 本身就是原子的（目标已在 ⇒ 盖掉）。
#[cfg(not(windows))]
fn replace(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::rename(src, dst)
}
