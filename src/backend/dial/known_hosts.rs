//! **cc-monitor 自己那份 `known_hosts`**（`<家>/.cc-monitor/known_hosts`，OpenSSH 的格式）：本后端握手时认下的每台主机钥匙
//! （按机器表里固化的指纹校验过，或首连认下、随即固化的那一把）记在这里；开终端那一行把它交给 `ssh`
//! （`-o UserKnownHostsFile=<这份>`，[`super::terminal`]）⇒ 终端里那一跳认得同一把钥匙、不再问一遍，
//! 也不往用户自己的 `~/.ssh/known_hosts` 写。主机钥匙核对照旧开着：钥匙变了 OpenSSH 照样拒。
//!
//! 一个「主机 ＋ 口」一行（换了钥匙 ⇒ 那一行换掉）；内容没变不写盘。

use std::path::PathBuf;

/// 这份文件在哪（家目录说不出 ⇒ `None`）。
pub(crate) fn path() -> Option<PathBuf> {
    let mut p = crate::platform::paths::home_dir()?;
    for seg in relay_route_core::KNOWN_HOSTS_REL.split('/') {
        p.push(seg);
    }
    Some(p)
}

/// OpenSSH 认的主机名写法：口 22 只写主机，别的口写 `[主机]:口`（IPv6 字面量的方括号先剥掉）。
pub(crate) fn host_pattern(host: &str, port: u16) -> String {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if port == 22 {
        bare.to_string()
    } else {
        format!("[{bare}]:{port}")
    }
}

/// 现有内容 ＋ 这台的一行（`<主机名> <算法> <base64>`）⇒ 新内容；同一台已是这一把 ⇒ `None`（不写）。
/// 同一主机名的旧行（别的钥匙）去掉；别的行原样留着。
pub(crate) fn merged(existing: &str, pattern: &str, key: &str) -> Option<String> {
    let line = format!("{pattern} {key}");
    let mut placed = false;
    let mut lines: Vec<&str> = Vec::new();
    for l in existing.lines().filter(|l| !l.trim().is_empty()) {
        if l.split_whitespace().next() != Some(pattern) {
            lines.push(l);
        } else if !placed {
            lines.push(&line);
            placed = true;
        }
    }
    if !placed {
        lines.push(&line);
    }
    let body = lines.join("\n") + "\n";
    (body != existing).then_some(body)
}

/// 记下这台的钥匙（`key` = OpenSSH 公钥那一行的前两段：算法 ＋ base64）。**本模块唯一的写口**（`readonly_guard` 第四层登记；
/// 门是 `dial/connect.rs` 的钥匙核对）。写不了只记日志：开终端照样开，只是 ssh 会再问一遍。
pub(crate) fn remember(file: &std::path::Path, host: &str, port: u16, key: &str) {
    if let Err(e) = remember_locked(file, host, port, key) {
        tracing::warn!(
            "known_hosts：{} 没记进 {}：{e}",
            host_pattern(host, port),
            file.display()
        );
    }
}

/// 读 → 改那一行 → 写，整段在那个目录的跨进程锁里（两个后端同时认下钥匙时，后写的不盖掉先写的那一台）。
fn remember_locked(file: &std::path::Path, host: &str, port: u16, key: &str) -> Result<(), String> {
    let dir = file.parent().ok_or_else(|| file.display().to_string())?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| e.to_string())?;
    let _lock = crate::platform::lock::hold(dir)?;
    use crate::common::own_state::{read_bytes, Read};
    // 读不出来的那份不覆盖（当成空的写回会把别的几台认下的钥匙一起抹掉）。
    let existing = match read_bytes(file, MAX_BYTES) {
        Read::Absent => String::new(),
        Read::Present(b) => String::from_utf8(b).map_err(|e| e.to_string())?,
        Read::Unreadable(why) => return Err(why),
    };
    let Some(body) = merged(&existing, &host_pattern(host, port), key) else {
        return Ok(());
    };
    crate::common::own_state::write(file, body.as_bytes())
}

/// 读盘的上限（一台一行，远到不了）。
const MAX_BYTES: u64 = 4 << 20;

#[cfg(test)]
#[path = "../../../tests/backend/dial_known_hosts_tests.rs"]
mod tests;
