//! **cc-monitor 那份主机钥匙库**（`<家>/.cc-monitor/known_hosts`，OpenSSH 的格式）：拨号核心按它认每台的钥匙
//! （读 · 记 · 忘的规矩在 `ssh_dial::known_hosts`，这里只有文件、锁与落盘）；开终端那一行经 [`super::ssh_config`] 交给 `ssh`，
//! 目标与每台跳板认同一份、不再问一遍，也不往用户自己的 `~/.ssh/known_hosts` 写。

use std::path::PathBuf;

/// 这份文件在哪（家目录说不出 ⇒ `None`）。
pub(crate) fn path() -> Option<PathBuf> {
    let mut p = crate::platform::paths::home_dir()?;
    for seg in relay_route_core::KNOWN_HOSTS_REL.split('/') {
        p.push(seg);
    }
    Some(p)
}

/// 改这份库的那只手：写句柄（家目录那一根）只装在这里面，对外只有 [`Writer::rewrite`]。
///
/// 记钥匙那一刻（拨号核心鉴权过了，经 [`KnownHostsFile`] 进来）在哪条拨号路上都可能发生（开链路 · 远端问话 · 探针 · 传输），
/// 那些路大多只读 ⇒ 不顺着它们递写句柄（递了就把写能力交给了只读的那几条），而是启动入口装一次（[`install`]），
/// 拨号那一层只取得到这只只会改这份库的手（[`installed`]）。
#[derive(Clone)]
pub(crate) struct Writer(crate::platform::fs::Own);

static INSTALLED: std::sync::OnceLock<Writer> = std::sync::OnceLock::new();

/// 启动入口（`main.rs`）装一次：此后记 · 忘都经这份句柄。再装 ⇒ 不动（第一份为准）。
pub fn install(own: &crate::platform::fs::Own) {
    INSTALLED.get_or_init(|| Writer(own.clone()));
}

/// 指着某个写句柄的家的那一只（帧命令递来的写句柄 · 判据；不装）。
pub(crate) fn writer_at(own: &crate::platform::fs::Own) -> Writer {
    Writer(own.clone())
}

/// 装了的那一只（没装 ⇒ `None`：一次性子命令 · 判据，照读、不记）。
pub(crate) fn installed() -> Option<Writer> {
    INSTALLED.get().cloned()
}

impl Writer {
    /// 在那个目录的跨进程锁里现读 → `edit` → 有变化就写（两个后端同时记钥匙时，后写的不盖掉先写的那一台）。**本模块唯一的写口**。
    pub(crate) fn rewrite(
        &self,
        edit: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Result<(), crate::common::said::Said> {
        let own = &self.0;
        let file = own.at_home(KNOWN_HOSTS.rel)?;
        let dir = own.home_dir()?;
        own.ensure_dir(&dir).map_err(|e| e.to_string())?;
        let held = own.lock(&dir).map_err(crate::common::said::Said::from)?;
        let existing = read_at(file.as_path())?;
        let Some(body) = edit(&existing) else {
            return Ok(());
        };
        own.write(&file, &held, body.as_bytes())
    }
}

/// 读那份文件（没有 ⇒ 空串）。读不出来的那份不当空的（当空的写回会把别的几台记下的钥匙一起抹掉）。
pub(crate) fn read_at(file: &std::path::Path) -> Result<String, crate::common::said::Said> {
    use crate::common::own_state::Read;
    match KNOWN_HOSTS.read_with(file, utf8) {
        Read::Absent => Ok(String::new()),
        Read::Present(s) => Ok(s),
        Read::Unreadable(why) => Err(why),
    }
}

fn utf8(b: Vec<u8>) -> Result<String, crate::common::said::Said> {
    String::from_utf8(b).map_err(|e| e.to_string().into())
}

/// 拨号核心的那份库（[`ssh_dial::env::HostKeys`]）：哪个进程都读那份文件；写只经启动时装的那只手（[`installed`]），
/// 没装（一次性子命令 · 判据）⇒ 照读、不记。
pub(crate) struct KnownHostsFile;

impl ssh_dial::env::HostKeys for KnownHostsFile {
    fn read(&self) -> Result<String, ssh_dial::Said> {
        match path() {
            Some(p) => read_at(&p),
            None => Ok(String::new()),
        }
    }

    fn rewrite(&self, edit: &mut dyn FnMut(&str) -> Option<String>) -> Result<(), ssh_dial::Said> {
        match installed() {
            Some(w) => w.rewrite(edit),
            None => Ok(()),
        }
    }
}

/// 开终端那一行 `ssh -F` 用的那份配置（[`relay_route_core::SSH_CONFIG_REL`]）的路径，经装了的那只手写好（[`Writer::terminal_config`]）。
/// 没装写句柄 / 写不成 ⇒ `None`（终端那一行照 ssh 自己的默认；写不成记一行日志）。
pub(crate) fn terminal_config() -> Option<PathBuf> {
    match installed()?.terminal_config() {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(
                "ssh_config：{} 没写成：{}",
                relay_route_core::SSH_CONFIG_REL,
                e.logged()
            );
            None
        }
    }
}

impl Writer {
    /// 那份配置内容不对就在锁里写成对的（内容没变不写盘），回它的路径。
    pub(crate) fn terminal_config(&self) -> Result<PathBuf, crate::common::said::Said> {
        let own = &self.0;
        let known = own.at_home(KNOWN_HOSTS.rel)?;
        let file = own.at_home(SSH_CONFIG.rel)?;
        let body = config_text(
            &known.as_path().to_string_lossy(),
            USER_CONFIG,
            &crate::platform::ssh_client::system_config(),
        );
        let dir = own.home_dir()?;
        own.ensure_dir(&dir).map_err(|e| e.to_string())?;
        let held = own.lock(&dir).map_err(crate::common::said::Said::from)?;
        let now = match SSH_CONFIG.read_with(file.as_path(), utf8) {
            crate::common::own_state::Read::Present(s) => Some(s),
            _ => None,
        };
        if now.as_deref() != Some(body.as_str()) {
            own.write(&file, &held, body.as_bytes())?;
        }
        Ok(file.as_path().to_path_buf())
    }
}

/// 用户自己那份 ssh 配置（ssh 自己按它的家展开 `~`）。
const USER_CONFIG: &str = "~/.ssh/config";

/// 那份配置的全文：主机钥匙认这一份（在前 ⇒ ssh 取先读到的值，这一格算数），之后照读用户与系统的 ssh 配置（同 ssh 自己的先后）。
pub(crate) fn config_text(known_hosts: &str, user: &str, system: &str) -> String {
    format!(
        "UserKnownHostsFile {}\nInclude {}\nInclude {}\n",
        config_word(&known_hosts.replace('%', "%%")),
        config_word(user),
        config_word(system),
    )
}

/// ssh 配置里的一个值：含空白就加双引号。
fn config_word(v: &str) -> String {
    if v.chars().any(char::is_whitespace) {
        format!("\"{v}\"")
    } else {
        v.to_string()
    }
}

/// 读盘的上限（一台一行，远到不了）。
const MAX_BYTES: u64 = 4 << 20;

/// 主机钥匙库：状态（读不懂不覆盖）。
pub(crate) const KNOWN_HOSTS: crate::common::own_state::Doc<String> =
    crate::common::own_state::Doc::state(relay_route_core::KNOWN_HOSTS_REL, MAX_BYTES);

/// 开终端那一行用的配置：缓存（照钥匙库的路径现写，读不懂 ⇒ 当没有、重写）。
const SSH_CONFIG: crate::common::own_state::Doc<String> =
    crate::common::own_state::Doc::cache(relay_route_core::SSH_CONFIG_REL, MAX_BYTES);

#[cfg(test)]
#[path = "../../../tests/backend/dial_known_hosts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/dial_known_hosts_disk_tests.rs"]
mod disk_tests;
