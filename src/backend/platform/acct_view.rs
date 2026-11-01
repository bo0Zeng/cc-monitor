//! 账号库读盘要的平台原语（**只读**）：一项是什么（不跟链接地看）· unix 权限位 · 这台做不做得了多账号。
//!
//! 账号库的每个号是「一个配置目录 ＋ 一排链回共享库的符号链接」—— 符号链接与 `0700` / `0600` 权限位都是平台差异，
//! 只在这里认。写不在这里：建目录 · 建链接 · 改权限 · 复制 · 改名 · 删都经这台的文件管理面（`files-*`）。

use crate::platform::fs::Ro;
use std::path::Path;

/// 一项（**不跟链接**地看）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
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
    pub fn exists(&self) -> bool {
        !matches!(self, Item::Absent)
    }
    pub fn is_link(&self) -> bool {
        matches!(self, Item::Link { .. })
    }
}

/// 这台做不做得了多账号（每个号一个配置目录、共享项靠符号链接；凭据只给本人：unix 靠 `0600`，Windows 靠家目录本来的权限）。
/// 做不了 ⇒ `Err(那一句)`：账号库那几条命令当场拒，不在业务代码里分平台。
/// unix 恒做得了；Windows 看此刻建不建得了符号链接（开发者模式或建链接的特权，`platform::fs::links_creatable`）；别的平台做不了。
pub fn multi_account_supported() -> Result<(), String> {
    if cfg!(unix) {
        Ok(())
    } else if cfg!(windows) {
        if crate::platform::fs::links_creatable() {
            Ok(())
        } else {
            Err(copy_core::copy_text("beAcctWire.platform.noSymlink", &[]))
        }
    } else {
        Err(copy_core::copy_text(
            "beAcctWire.platform.unsupported",
            &[("os", std::env::consts::OS)],
        ))
    }
}

/// 这台系统做不了多账号时那一句（做得了 ⇒ `None`）。账号清单的 `meta.unsupported` 与账号库改动那一道闸都按它说。
pub fn unsupported_said() -> Option<String> {
    multi_account_supported().err()
}

/// 这台有没有 unix 权限位那一格（没有 ⇒ 账号库执行那一步不改权限、核对那一步不核：Windows 的家目录本来只给本人）。
pub fn has_mode_bits() -> bool {
    cfg!(unix)
}

/// 账号库这一族认的路径写法：一律 `/` 分隔（Windows 上把 `\` 换成 `/`，盘符照留：`C:/Users/…`；系统两种都认）。
/// 家目录进账号库之前、链接目标读出来之后各过一次，这一族的拼接与比较（`scan::join` · `is_under`）就只有一种写法。
pub fn slash_form(p: &str) -> String {
    if cfg!(windows) {
        p.replace('\\', "/")
    } else {
        p.to_string()
    }
}

/// 是不是这一族认的绝对路径（[`slash_form`] 那一形）：`/…`；Windows 上还有盘符那一形 `C:/…`。
pub fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || (cfg!(windows) && drive_of(p).is_some())
}

/// `C:/…` 的盘符（`C`）；不是这一形 ⇒ `None`。
fn drive_of(p: &str) -> Option<char> {
    let mut c = p.chars();
    match (c.next(), c.next(), c.next()) {
        (Some(d), Some(':'), Some('/')) if d.is_ascii_alphabetic() => Some(d),
        _ => None,
    }
}

/// 一个绝对路径在备份目录的 `root` 底下照原样摆时的那一截（以 `/` 开头）：`/x` 原样；Windows 的 `C:/x` ⇒ `/C/x`（名字里不许有 `:`）。
pub fn under_root(p: &str) -> String {
    match drive_of(p) {
        Some(d) if cfg!(windows) => format!("/{d}{}", &p[2..]),
        _ => p.to_string(),
    }
}

/// 这台账号库里一个号的配置目录写得对不对（绝对 · 无 `..` 段 · 无危险字符）：unix 是 POSIX 那一形；
/// Windows 是盘符 / UNC 那一形、且照 [`slash_form`] 只用 `/`。
pub fn config_dir_ok(dir: &str) -> bool {
    if cfg!(windows) {
        acct_core::config_dir_ok(dir) && !dir.contains('\\')
    } else {
        acct_core::config_dir_posix_ok(dir)
    }
}

/// `p` 这一项是什么（不跟链接）。读链接成功 ⇒ 它就是一条链接；否则按跟过去的元数据认。
/// 链接目标照 [`slash_form`] 交出（Windows 上系统存的是 `\` 那一形）。
pub fn item(p: &Path) -> Item {
    if let Ok(t) = Ro::read_link(p) {
        return Item::Link {
            target: slash_form(&t.to_string_lossy()),
            dangling: Ro::metadata(p).is_err(),
        };
    }
    match Ro::metadata(p) {
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
pub fn names(dir: &Path) -> Option<Vec<String>> {
    let rd = Ro::read_dir(dir).ok()?;
    let mut out: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.chars().any(char::is_control))
        .collect();
    out.sort();
    Some(out)
}

/// unix 权限位的低 12 位（跟链接）；非 unix ⇒ `None`（不是 `0`：`0` 是一个真能设的值）。
fn mode_of(p: &Path) -> Option<u32> {
    Ro::mode(&Ro::metadata(p).ok()?).map(|m| m & 0o7777)
}
