//! 资源事实（按帧报）· 能力探测（hello 那一下）· 停线（连着两次撞线才算）。
//! 事实只读 `/proc` 与 `statvfs`，不起进程；能力探测要起进程，只在 hello 那一下做。

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::limits;
use crate::root::Root;

/// 这一刻的资源事实。量不到的那一格是 None（调度端当最紧看）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Facts {
    /// 可用内存（字节，`MemAvailable`）。
    pub mem_available: Option<u64>,
    /// 内存压力（`/proc/pressure/memory` 的 some avg10，百分比）。
    pub mem_pressure: Option<f64>,
    /// 根目录所在盘剩余（字节，非特权用户可用的）。
    pub disk_free: Option<u64>,
    /// `/tmp` 剩余（字节）。
    pub tmp_free: Option<u64>,
    /// 账号此刻的线程数。
    pub account_threads: Option<u64>,
    /// RLIMIT_NPROC（无限 ⇒ u64::MAX）。
    pub nproc_limit: Option<u64>,
}

pub fn parse_meminfo_available(s: &str) -> Option<u64> {
    let v = s.lines().find_map(|l| l.strip_prefix("MemAvailable:"))?;
    let kb: u64 = v.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb * 1024)
}

pub fn parse_psi_some_avg10(s: &str) -> Option<f64> {
    let l = s.lines().find(|l| l.starts_with("some "))?;
    l.split_whitespace()
        .find_map(|f| f.strip_prefix("avg10="))?
        .parse()
        .ok()
}

/// 那条路径所在盘上非特权用户还能用的字节数。
pub fn free_bytes(p: &Path) -> Option<u64> {
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some(s.f_bavail as u64 * s.f_frsize as u64)
}

pub fn measure(root: &Root) -> Facts {
    Facts {
        mem_available: fs::read_to_string("/proc/meminfo")
            .ok()
            .as_deref()
            .and_then(parse_meminfo_available),
        mem_pressure: fs::read_to_string("/proc/pressure/memory")
            .ok()
            .as_deref()
            .and_then(parse_psi_some_avg10),
        disk_free: free_bytes(root.base()),
        tmp_free: free_bytes(Path::new("/tmp")),
        account_threads: limits::account_threads(),
        nproc_limit: limits::rlimit_nproc(),
    }
}

// ════════════════ 能力探测 ════════════════

/// 一样能力：名字固定（cargo · tmux · tmuxnew · node · gui · jdk · android · rsync），带版本（探不出版本的空串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub name: &'static str,
    pub version: String,
}

/// hello 报的「这台有哪些程序」查的名单（供调度端核指向程序的变量：RUSTC_WRAPPER · CC 一类）。
pub const PROGRAMS: &[&str] = &[
    "sccache",
    "cc",
    "c++",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "ld",
    "ar",
    "make",
    "cmake",
    "pkg-config",
    "git",
    "tmux",
    "node",
    "npm",
    "java",
    "python3",
];

/// 探测用的 PATH：与作业的同一份（`Root::job_path`）。
pub fn probe_path(root: &Root) -> OsString {
    root.job_path(None)
}

/// PATH 里找一个程序（不起进程）。
pub fn which(name: &str, path: &OsString) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path).map(|d| d.join(name)).find(|c| {
        fs::metadata(c)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// 探一个程序的版本最多等这么久（rustup 的代理在空的 RUSTUP_HOME 下会去下载工具链，实测一等半小时）。
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn run_version(root: &Root, prog: &str, args: &[&str]) -> Option<String> {
    use std::io::Read;
    let path = probe_path(root);
    // Rust 工具链只认根下的：不用用户自己的 ~/.cargo · ~/.rustup（远端的一切都收在一个根里）
    let exe = if prog == "cargo" || prog == "rustc" {
        which(prog, &OsString::from(root.toolchains().join("cargo/bin")))?
    } else {
        which(prog, &path)?
    };
    let mut ch = Command::new(exe)
        .args(args)
        .env("PATH", &path)
        .env("CARGO_HOME", root.toolchains().join("cargo"))
        .env("RUSTUP_HOME", root.toolchains().join("rustup"))
        .env("RUSTUP_AUTO_INSTALL", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let t0 = std::time::Instant::now();
    let status = loop {
        match ch.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if t0.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => {
                let _ = ch.kill();
                let _ = ch.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut s = String::new();
    let _ = ch.stdout.take()?.read_to_string(&mut s);
    let _ = ch.stderr.take()?.read_to_string(&mut s);
    Some(
        s.lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string(),
    )
}

/// `tmux 3.2a` · `tmux next-3.4` ⇒ (3, 2)。
pub fn parse_tmux_version(s: &str) -> Option<(u32, u32)> {
    let v = s.split_whitespace().nth(1)?;
    let v = v.trim_start_matches(|c: char| !c.is_ascii_digit());
    let mut it = v.split('.');
    let major = it.next()?.parse().ok()?;
    let minor: String = it
        .next()
        .unwrap_or("0")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    Some((major, minor.parse().unwrap_or(0)))
}

/// 送源码要的最低 rsync 版本（`--delete-missing-args` 从 3.1.0 起有；目的目录由远端 shell 先建，不用 `--mkpath`）。
pub const RSYNC_MIN: (u32, u32, u32) = (3, 1, 0);

/// `rsync  version 3.4.1  protocol version 32` ⇒ (3, 4, 1)。
pub fn parse_rsync_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.split_whitespace();
    it.find(|w| *w == "version")?;
    let v = it.next()?;
    let mut p = v.split('.').map(|x| {
        x.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
    });
    let major = p.next()?.ok()?;
    let minor = p.next().and_then(|x| x.ok()).unwrap_or(0);
    let patch = p.next().and_then(|x| x.ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// 探测这台的能力（起进程，只在 hello 那一下）。
pub fn probe_capabilities(root: &Root) -> Vec<Capability> {
    let mut v = Vec::new();
    if let Some(c) = run_version(root, "cargo", &["--version"]) {
        let rustc = run_version(root, "rustc", &["--version"]).unwrap_or_default();
        v.push(Capability {
            name: "cargo",
            version: if rustc.is_empty() {
                c
            } else {
                format!("{c}; {rustc}")
            },
        });
    }
    if let Some(t) = run_version(root, "tmux", &["-V"]) {
        if parse_tmux_version(&t).is_some_and(|ver| ver >= (3, 2)) {
            v.push(Capability {
                name: "tmuxnew",
                version: t.clone(),
            });
        }
        v.push(Capability {
            name: "tmux",
            version: t,
        });
    }
    if let Some(n) = run_version(root, "node", &["--version"]) {
        v.push(Capability {
            name: "node",
            version: n,
        });
    }
    if let Some(g) = run_version(root, "pkg-config", &["--modversion", "webkit2gtk-4.1"]) {
        v.push(Capability {
            name: "gui",
            version: g,
        });
    }
    if let Some(j) = run_version(root, "java", &["-version"]) {
        v.push(Capability {
            name: "jdk",
            version: j,
        });
    }
    if root.android_sdk().is_dir() {
        v.push(Capability {
            name: "android",
            version: String::new(),
        });
    }
    if let Some(r) = run_version(root, "rsync", &["--version"]) {
        if parse_rsync_version(&r).is_some_and(|ver| ver >= RSYNC_MIN) {
            v.push(Capability {
                name: "rsync",
                version: r,
            });
        }
    }
    v
}

/// 作业里的 cargo 会读到的配置文件：从 `<根>/projects` 往上各层的 `.cargo/config.toml` · `.cargo/config`，
/// 再加 CARGO_HOME（`<根>/toolchains/cargo`）下的。根在家目录下时，那台家目录里的配置也在里面（接受、不隔断，照实报）。
pub fn cargo_configs(root: &Root) -> Vec<String> {
    let mut v = Vec::new();
    let mut push = |d: &Path| {
        for name in ["config.toml", "config"] {
            let p = d.join(name);
            if p.is_file() {
                v.push(p.to_string_lossy().into_owned());
            }
        }
    };
    for d in root.base().join("projects").ancestors() {
        push(&d.join(".cargo"));
    }
    push(&root.toolchains().join("cargo"));
    v
}

/// 这台 PATH 里有的程序（名单见 [`PROGRAMS`]）。
pub fn present_programs(root: &Root) -> Vec<String> {
    let path = probe_path(root);
    PROGRAMS
        .iter()
        .filter(|p| which(p, &path).is_some())
        .map(|p| p.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        assert_eq!(
            parse_rsync_version("rsync  version 3.4.1  protocol version 32"),
            Some((3, 4, 1))
        );
        assert!(
            parse_rsync_version("rsync  version 3.1.3  protocol version 31")
                .is_some_and(|v| v >= RSYNC_MIN),
            "3.1.3 够（不用 --mkpath，--delete-missing-args 从 3.1.0 起）"
        );
        assert!(
            parse_rsync_version("rsync  version 3.0.9  protocol version 30")
                .is_some_and(|v| v < RSYNC_MIN)
        );
        assert_eq!(parse_rsync_version("rsync  version v3.2.3pre1"), None);
        assert_eq!(
            parse_meminfo_available("MemTotal: 10 kB\nMemAvailable:    2048 kB\n"),
            Some(2048 * 1024)
        );
        assert_eq!(parse_meminfo_available("MemTotal: 10 kB\n"), None);
        assert_eq!(
            parse_psi_some_avg10("some avg10=1.50 avg60=0.00 avg300=0.00 total=1\nfull avg10=0.00"),
            Some(1.5)
        );
        assert_eq!(parse_tmux_version("tmux 3.0a"), Some((3, 0)));
        assert_eq!(parse_tmux_version("tmux 3.2a"), Some((3, 2)));
        assert_eq!(parse_tmux_version("tmux next-3.4"), Some((3, 4)));
        assert_eq!(parse_tmux_version("tmux"), None);
    }

    #[test]
    fn measures_here() {
        let r = Root::new(std::env::temp_dir());
        let f = measure(&r);
        assert!(f.mem_available.is_some_and(|m| m > 0));
        assert!(f.disk_free.is_some());
        assert!(f.account_threads.is_some_and(|n| n >= 1));
        // 能力探测不崩、名字都在固定表里
        for c in probe_capabilities(&r) {
            assert!(
                ["cargo", "tmux", "tmuxnew", "node", "gui", "jdk", "android", "rsync"]
                    .contains(&c.name)
            );
        }
        assert!(present_programs(&r)
            .iter()
            .all(|p| PROGRAMS.contains(&p.as_str())));
    }
}
