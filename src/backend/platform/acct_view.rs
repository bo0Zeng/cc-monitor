//! 账号库读盘要的平台原语（**只读**）：一项是什么（不跟链接地看）· unix 权限位 · 这台做不做得了多账号。
//!
//! 账号库的每个号是「一个配置目录 ＋ 一排链回共享库的符号链接」—— 符号链接与 `0700` / `0600` 权限位都是平台差异，
//! 只在这里认。写不在这里：建目录 · 建链接 · 改权限 · 复制 · 改名 · 删都经这台的文件管理面（`files-*`）。

use std::path::Path;

/// 一项（**不跟链接**地看）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    Absent,
    File {
        mode: Option<u32>,
        size: u64,
        mtime: Option<i64>,
    },
    Dir {
        mode: Option<u32>,
    },
    /// 符号链接：目标文本原样；`dangling` = 跟过去什么都没有。
    Link {
        target: String,
        dangling: bool,
    },
    Other,
}

impl Item {
    pub(crate) fn exists(&self) -> bool {
        !matches!(self, Item::Absent)
    }
    pub(crate) fn is_link(&self) -> bool {
        matches!(self, Item::Link { .. })
    }
}

/// 这台做不做得了多账号（每个号一个配置目录、共享项靠符号链接、凭据靠 `0600`）。
/// 做不了 ⇒ `Err(平台名)`：账号库那几条命令当场拒，不在业务代码里分平台。
pub(crate) fn multi_account_supported() -> Result<(), &'static str> {
    if cfg!(unix) {
        Ok(())
    } else {
        Err(std::env::consts::OS)
    }
}

/// 这台系统做不了多账号时那一句（做得了 ⇒ `None`）。账号清单的 `meta.unsupported` 与账号库改动那一道闸都按它说。
pub(crate) fn unsupported_said() -> Option<String> {
    multi_account_supported()
        .err()
        .map(|os| copy_core::copy_text("beAcctWire.platform.unsupported", &[("os", os)]))
}

/// `p` 这一项是什么（不跟链接）。读链接成功 ⇒ 它就是一条链接；否则按跟过去的元数据认。
pub(crate) fn item(p: &Path) -> Item {
    if let Ok(t) = std::fs::read_link(p) {
        return Item::Link {
            target: t.to_string_lossy().into_owned(),
            dangling: std::fs::metadata(p).is_err(),
        };
    }
    match std::fs::metadata(p) {
        Err(_) => Item::Absent,
        Ok(m) if m.is_dir() => Item::Dir { mode: mode_of(p) },
        Ok(m) if m.is_file() => Item::File {
            mode: mode_of(p),
            size: m.len(),
            mtime: m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_nanos() / 1_000_000).ok()),
        },
        Ok(_) => Item::Other,
    }
}

/// 一个目录的直接子项名（排好序）。名字不是 UTF-8 或带控制符 ⇒ 不列（这种名字拼不进链接目标，也显不出来）。
/// 目录不在 / 读不了 ⇒ `None`。
pub(crate) fn names(dir: &Path) -> Option<Vec<String>> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut out: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.chars().any(char::is_control))
        .collect();
    out.sort();
    Some(out)
}

/// unix 权限位的低 12 位（跟链接）；非 unix ⇒ `None`（不是 `0`：`0` 是一个真能设的值）。
#[cfg(unix)]
fn mode_of(p: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .ok()
        .map(|m| m.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn mode_of(_p: &Path) -> Option<u32> {
    None
}
