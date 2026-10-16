//! **cc-monitor 自己那份 `known_hosts`**（`<家>/.cc-monitor/known_hosts`，OpenSSH 的格式）：本后端连上的每台主机钥匙
//! （握手时认下、鉴权也过了的那一把）记在这里；开终端那一行把它交给 `ssh`
//! （`-o UserKnownHostsFile=<这份>`，[`super::terminal`]）⇒ 终端里那一跳认得同一把钥匙、不再问一遍，
//! 也不往用户自己的 `~/.ssh/known_hosts` 写。主机钥匙核对照旧开着：钥匙变了 OpenSSH 照样拒。
//!
//! 一个「主机 ＋ 口」一行（换了钥匙 ⇒ 那一行换掉）；内容没变不写盘。

use std::path::PathBuf;

pub(crate) use ssh_dial::known_hosts::{host_pattern, merged};

/// 这份文件在哪（家目录说不出 ⇒ `None`）。
pub(crate) fn path() -> Option<PathBuf> {
    let mut p = crate::platform::paths::home_dir()?;
    for seg in relay_route_core::KNOWN_HOSTS_REL.split('/') {
        p.push(seg);
    }
    Some(p)
}

/// 记下认下的钥匙的那只手：写句柄（家目录那一根）只装在这里面，对外只有 [`Writer::remember`]。
///
/// 鉴权过了的那一刻（拨号核心 `ssh-dial/connect.rs`，经 [`KnownHostsFile`] 进来）在哪条拨号路上都可能发生（开链路 · 远端问话 · 探针 · 传输），
/// 那些路大多只读 ⇒ 不顺着它们递写句柄（递了就把写能力交给了只读的那几条），而是启动入口装一次（[`install`]），
/// 拨号那一层只取得到这只只会记钥匙的手（[`installed`]）。
#[derive(Clone)]
pub(crate) struct Writer(crate::platform::fs::Own);

static INSTALLED: std::sync::OnceLock<Writer> = std::sync::OnceLock::new();

/// 启动入口（`main.rs`）装一次：此后握手认下的钥匙经这份句柄记。再装 ⇒ 不动（第一份为准）。
pub fn install(own: &crate::platform::fs::Own) {
    INSTALLED.get_or_init(|| Writer(own.clone()));
}

/// 装了的那一只（没装 ⇒ `None`：一次性子命令 · 判据，握手照常、不记）。
pub(crate) fn installed() -> Option<Writer> {
    INSTALLED.get().cloned()
}

impl Writer {
    /// 记下这台的钥匙（`key` = OpenSSH 公钥那一行的前两段：算法 ＋ base64）。**本模块唯一的写口**。
    /// 写不了只记日志：开终端照样开，只是 ssh 会再问一遍。
    pub(crate) fn remember(&self, host: &str, port: u16, key: &str) {
        if let Err(e) = self.remember_locked(host, port, key) {
            tracing::warn!(
                "known_hosts：{} 没记进 {}：{}",
                host_pattern(host, port),
                relay_route_core::KNOWN_HOSTS_REL,
                e.logged()
            );
        }
    }

    /// 读 → 改那一行 → 写，整段在那个目录的跨进程锁里（两个后端同时认下钥匙时，后写的不盖掉先写的那一台）。
    fn remember_locked(
        &self,
        host: &str,
        port: u16,
        key: &str,
    ) -> Result<(), crate::common::said::Said> {
        let own = &self.0;
        let file = own.at_home(relay_route_core::KNOWN_HOSTS_REL)?;
        let dir = own.home_dir()?;
        own.ensure_dir(&dir).map_err(|e| e.to_string())?;
        let held = own.lock(&dir).map_err(crate::common::said::Said::from)?;
        use crate::common::own_state::{read_bytes, Read};
        // 读不出来的那份不覆盖（当成空的写回会把别的几台认下的钥匙一起抹掉）。
        let existing = match read_bytes(file.as_path(), MAX_BYTES) {
            Read::Absent => String::new(),
            Read::Present(b) => String::from_utf8(b).map_err(|e| e.to_string())?,
            Read::Unreadable(why) => return Err(why),
        };
        let Some(body) = merged(&existing, &host_pattern(host, port), key) else {
            return Ok(());
        };
        own.write(&file, &held, body.as_bytes())
    }
}

/// 拨号核心认下一把 host key、鉴权也过了时交到这里（[`ssh_dial::env::HostKeys`]）：经启动时装的那只手记（[`installed`]）。
/// 没装（一次性子命令 · 判据）⇒ 不记。
pub(crate) struct KnownHostsFile;

impl ssh_dial::env::HostKeys for KnownHostsFile {
    fn remember(&self, host: &str, port: u16, key: &str) {
        if let Some(w) = installed() {
            w.remember(host, port, key);
        }
    }
}

/// 读盘的上限（一台一行，远到不了）。
const MAX_BYTES: u64 = 4 << 20;

#[cfg(test)]
#[path = "../../../tests/backend/dial_known_hosts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/dial_known_hosts_disk_tests.rs"]
mod disk_tests;
