//! 这台电脑上的 ssh 客户端在哪 —— 开远端终端那一行（`dial/terminal.rs`）用它的全路径，不靠窗口里再按 PATH 找一遍。
//!
//! 不起子进程，只查文件：
//! - Windows：先 `%SystemRoot%\System32\OpenSSH\ssh.exe`（系统可选功能「OpenSSH 客户端」装的位置，不在 PATH 上也算），再按 PATH 逐项找 `ssh.exe`；
//! - 别处：按 PATH 逐项找可执行的 `ssh`。
//!
//! 三种结局：找到（全路径）· 每一处都查清了、都没有（没装）· 没找到而查的时候出过错（读不到 PATH / SystemRoot、某一项读不了）——
//! 最后这一种是「判不了」，不说没装。

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// Windows 上系统可选功能装 ssh 的位置（相对 `%SystemRoot%`）。
pub(crate) const WINDOWS_OPENSSH_REL: &str = "System32\\OpenSSH\\ssh.exe";

/// 找的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SshClient {
    /// 在这儿（全路径）。
    At(String),
    /// 每一处都查清了，都没有。
    Missing,
    /// 没找到，而查的时候出过错 ⇒ 判不了（带那一处的原话）。
    Unknown(String),
}

/// 这台电脑上找（读本进程的 `SystemRoot` / `PATH`，查真文件）。
pub(crate) fn locate() -> SshClient {
    locate_with(
        cfg!(windows),
        std::env::var_os("SystemRoot").as_deref(),
        std::env::var_os("PATH").as_deref(),
        &runnable,
    )
}

/// [`locate`] 的本体：平台 · 两格环境 · 查文件那一下都由调用方给。
pub(crate) fn locate_with(
    windows: bool,
    system_root: Option<&OsStr>,
    path: Option<&OsStr>,
    probe: &dyn Fn(&Path) -> io::Result<bool>,
) -> SshClient {
    let mut trouble: Option<String> = None;
    let mut places: Vec<PathBuf> = Vec::new();
    if windows {
        match system_root.filter(|r| !r.is_empty()) {
            Some(r) => places.push(Path::new(r).join(WINDOWS_OPENSSH_REL)),
            None => trouble = Some("SystemRoot not set".into()),
        }
    }
    let name = if windows { "ssh.exe" } else { "ssh" };
    let dirs: Vec<PathBuf> = path
        .map(|p| {
            std::env::split_paths(p)
                .filter(|d| !d.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default();
    if dirs.is_empty() {
        trouble.get_or_insert_with(|| "PATH not set or empty".into());
    }
    places.extend(dirs.into_iter().map(|d| d.join(name)));
    for p in places {
        match probe(&p) {
            Ok(true) => match p.to_str() {
                Some(s) => return SshClient::At(s.to_string()),
                None => {
                    trouble.get_or_insert_with(|| format!("{}: path is not UTF-8", p.display()));
                }
            },
            Ok(false) => {}
            Err(e) => {
                trouble.get_or_insert_with(|| format!("{}: {e}", p.display()));
            }
        }
    }
    trouble.map_or(SshClient::Missing, SshClient::Unknown)
}

/// 这一处有没有一个能跑的文件：不在（连同父路径不是目录 · 名字在这台不合法）⇒ `Ok(false)`；别的读不了 ⇒ `Err`。
pub(crate) fn runnable(p: &Path) -> io::Result<bool> {
    match std::fs::metadata(p) {
        Ok(m) => Ok(m.is_file() && launchable(p)),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::NotADirectory
                    | io::ErrorKind::InvalidFilename
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// unix：有执行位。
#[cfg(unix)]
fn launchable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

/// 别处没有执行位：按名字（找的就是 `ssh.exe`）。
#[cfg(not(unix))]
fn launchable(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/ssh_client_tests.rs"]
mod tests;
