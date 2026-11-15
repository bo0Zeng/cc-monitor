//! 节点的 sccache server：跨作业常驻，一台一份（只主节点起），不进任何作业组、不带任何一趟的记号；节点退出时停掉。
//! 作业经节点设的 SCCACHE_DIR · SCCACHE_SERVER_PORT 连它；用不用 sccache 由作业自己的 RUSTC_WRAPPER 定。

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;

use crate::procs;
use crate::root::Root;

/// `state/jobs/` 下它那一份记录的名字（作业的记录是 4 位短号，撞不上）。
pub const RECORD: &str = "sccache";

#[derive(Debug)]
pub struct Server {
    pub pid: i32,
    pub port: u16,
}

/// 根下的 sccache（跟 cargo · rustc 一样只认根下的）。
pub fn exe(root: &Root) -> PathBuf {
    root.toolchains().join("cargo/bin/sccache")
}

/// 本机回环上挑一个此刻空着的端口。
fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

/// 根下有 sccache 就起一份 server：`Ok(None)` ＝ 根下没有；`Err` ＝ 起不来（调用方记一行、照常接活）。
/// `nproc_soft` 是线（同作业的软限）：server 与它起的编译器也过不了线；它那一组由调用方算进「我们的」。
pub fn start(root: &Root, nproc_soft: u64) -> std::io::Result<Option<Server>> {
    let exe = exe(root);
    if !exe.is_file() {
        return Ok(None);
    }
    let port = free_port()?;
    let mut env: Vec<(OsString, OsString)> = Vec::new();
    if let Some(h) = std::env::var_os("HOME") {
        env.push(("HOME".into(), h));
    }
    env.extend(
        root.tool_env()
            .into_iter()
            .map(|(k, v)| (OsString::from(k), v)),
    );
    for (k, v) in [
        ("SCCACHE_SERVER_PORT", port.to_string()),
        ("SCCACHE_IDLE_TIMEOUT", "0".into()),
        ("SCCACHE_START_SERVER", "1".into()),
        ("SCCACHE_NO_DAEMON", "1".into()),
    ] {
        env.push((k.into(), v.into()));
    }
    let _ = fs::create_dir_all(root.cache().join("sccache"));
    let out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.log().join("sccache.log"));
    let (o, e) = match out {
        Ok(f) => (Stdio::from(f.try_clone()?), Stdio::from(f)),
        Err(_) => (Stdio::null(), Stdio::null()),
    };
    let pid = procs::spawn_detached(&exe, &env, Some(nproc_soft), o, e)?;
    let _ = procs::write_record(&root.jobs_dir(), RECORD, pid, std::ffi::OsStr::new(""));
    Ok(Some(Server { pid, port }))
}

impl Server {
    /// 停掉它（整组：它起的编译器也在组里）、删记录。只发信号；回收归调用方的 reap。
    pub fn stop(&self, root: &Root) {
        procs::kill_group(self.pid, None, 0);
        procs::remove_record(&root.jobs_dir(), RECORD);
    }
}
