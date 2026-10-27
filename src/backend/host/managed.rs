//! 托管那一种宿主：cc-monitor 自己持伪终端的终端（托管进程 `ccm -- --host-run`，家在数据目录底下 `hosts/<id>/`）。
//!
//! 后端对托管终端不留进程内状态：名单（[`rows`]）与打标（[`tag_pid`]）都是问的那一刻凭盘现做 ⇒ 后端重启 · 升级之后照样认得。

pub(crate) mod attach;
pub(crate) mod client;
pub(crate) mod home;
pub(crate) mod keys;
pub(crate) mod run;
pub(crate) mod screen;
pub(crate) mod serve;
pub(crate) mod wire;

use super::TermRow;
use crate::platform::fs::Own;

/// 托管名单的家：启动入口装一次（看守起来那一下 · 一次性那一形），之后名单与打标照它的数据目录。没装（判据）⇒ 名单空、不打标。
static HOME: std::sync::OnceLock<Own> = std::sync::OnceLock::new();

/// 装一次（拿启动入口造的写句柄；第二次装的不换）。
pub(crate) fn install(own: &Own) {
    if HOME.set(own.clone()).is_err() {
        tracing::debug!("托管名单的家已经装过了，留着第一份");
    }
}

/// 这台此刻的托管终端（每行 `host` 是 [`crate::wire::TerminalHost::Managed`]）。
pub(crate) fn rows() -> Vec<TermRow> {
    HOME.get().map(rows_at).unwrap_or_default()
}

/// [`rows`] 的本体：数据目录由调用方给。
pub(crate) fn rows_at(own: &Own) -> Vec<TermRow> {
    home::alive(own).into_iter().map(row_of).collect()
}

/// 一份事实 ⇒ 名单里的一行（句柄就是托管 id，随机、不复用）。
fn row_of(f: home::Facts) -> TermRow {
    TermRow {
        host: crate::wire::TerminalHost::Managed,
        id: f.id,
        pane: None,
        shared: false,
        name: f.name,
        windows: 1,
        activity: u64::try_from(f.started_ms / 1000).unwrap_or(0),
        sid: f.sid,
        agent: f.agent,
        client: f.client,
        program: String::new(),
        cwd: f.cwd,
        title: String::new(),
    }
}

/// 打标的结局：（这条会话的容器，这次真写了没有）—— 同 `control::identity_tag::tag_seen` 交回的那一对。
pub(crate) type Tagged = (Option<crate::wire::SessionContainer>, bool);

/// 宿主口「打标」托管那一臂：`pid` 住在托管终端里 ⇒ 把 `sid` 写进那个托管终端的事实；不在 ⇒ `None`（交 tmux 那一臂）。
pub(crate) fn tag_pid(pid: u32, sid: &str) -> Option<Tagged> {
    tag_pid_at(HOME.get()?, pid, sid)
}

/// [`tag_pid`] 的本体：数据目录由调用方给。
///
/// 认法：这个进程环境里的 `CCM_HOST`（托管进程给子进程设的，[`run::HOST_ENV`]；里面的 ccm · claude 原样继承）。
/// 没设 · 读不到 ⇒ `None`。设了 ⇒ 容器就是那个托管终端；事实里已是这个 sid ⇒ 不写；否则经托管口发 `tag`，等托管进程写完才回。
pub(crate) fn tag_pid_at(own: &Own, pid: u32, sid: &str) -> Option<Tagged> {
    use crate::platform::proc::{proc_env_var, EnvRead};
    let id = match proc_env_var(pid, run::HOST_ENV) {
        EnvRead::Value(v) => v.trim().to_string(),
        EnvRead::Unset | EnvRead::Unreadable => return None,
    };
    // 形状不对 · sid 不是会话 id：说得出它在托管终端里，但不往里写没核过的字符串。
    if !home::id_ok(&id) {
        tracing::warn!("托管终端打标：pid {pid} 的 {} 形状不对，不打", run::HOST_ENV);
        return Some((None, false));
    }
    let container = Some(crate::wire::SessionContainer::Managed {
        terminal: Some(id.clone()),
    });
    if sid.parse::<cmd_wire::Sid>().is_err() {
        return Some((container, false));
    }
    let dir = home::dir_of(own, &id).ok()?;
    if home::read_facts(dir.as_path()).is_some_and(|f| f.sid == sid) {
        return Some((container, false));
    }
    let done = match home::sock_of(own, &id) {
        Ok(sock) => client::tag(&sock, sid).map_err(|e| e.to_string()),
        Err(s) => Err(s.logged()),
    };
    let wrote = match done {
        Ok(()) => true,
        Err(why) => {
            tracing::warn!("托管终端 {id} 打标没打上（pid {pid} · sid {sid}）：{why}");
            false
        }
    };
    Some((container, wrote))
}

#[cfg(test)]
#[path = "../../../tests/backend/host/managed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/host/life_tests.rs"]
mod life_tests;
