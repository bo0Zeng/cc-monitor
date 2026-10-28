//! 托管那一种宿主：cc-monitor 自己持伪终端的终端（托管进程 `ccm -- --host-run`，家在数据目录底下 `hosts/<id>/`）。
//!
//! 后端对托管终端不留进程内状态：名单（[`rows`]）与打标（[`tag_pid`]）都是问的那一刻凭盘现做 ⇒ 后端重启 · 升级之后照样认得。

pub(crate) mod attach;
pub(crate) mod bin;
pub(crate) mod client;
pub(crate) mod hold;
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

/// 这台此刻的托管终端（每行 `host` 是 [`crate::wire::TerminalHost::Managed`]）＋ 连着它们的真终端（`(托管 id, 接上, 最近动静)`，秒）。
pub(crate) fn listed() -> (Vec<TermRow>, Vec<(String, u64, u64)>) {
    HOME.get().map(listed_at).unwrap_or_default()
}

/// [`listed`] 的本体：数据目录由调用方给。每个托管终端问一次 `clients`（谁连着 · 谁持有输入）；问不到 ⇒ 那一行照列、持有者当没有。
pub(crate) fn listed_at(own: &Own) -> (Vec<TermRow>, Vec<(String, u64, u64)>) {
    let secs = |ms: i64| u64::try_from(ms / 1000).unwrap_or(0);
    let mut rows = Vec::new();
    let mut clients = Vec::new();
    for f in home::alive(own) {
        let asked = home::sock_of(own, &f.id)
            .map_err(|s| std::io::Error::other(s.logged()))
            .and_then(|sock| client::clients(&sock));
        let ends = match asked {
            Ok(ends) => ends,
            Err(e) => {
                tracing::warn!("托管终端 {}：问连着的各端没问到：{e}", f.id);
                Vec::new()
            }
        };
        for e in ends.iter().filter(|e| e.kind == wire::WhoKind::Terminal) {
            clients.push((f.id.clone(), secs(e.since_ms), secs(e.last_ms)));
        }
        let held = ends.iter().find(|e| e.holder).map(|e| match e.kind {
            wire::WhoKind::Terminal => super::HeldBy::Terminal,
            wire::WhoKind::Backend => super::HeldBy::Front(e.front.clone().unwrap_or_default()),
        });
        rows.push(row_of(f, super::Input::Held(held)));
    }
    (rows, clients)
}

/// 只要行（判据 · 不看连着谁的那几处）。
pub(crate) fn rows_at(own: &Own) -> Vec<TermRow> {
    listed_at(own).0
}

/// 一份事实 ⇒ 名单里的一行（句柄就是托管 id，随机、不复用）。
fn row_of(f: home::Facts, input: super::Input) -> TermRow {
    TermRow {
        host: crate::wire::TerminalHost::Managed,
        id: f.id,
        pane: None,
        shared: false,
        name: f.name,
        windows: 1,
        activity: u64::try_from(f.started_ms / 1000).unwrap_or(0),
        sid: f.sid,
        // 托管终端里前台恒是它起的那一家（那一家一退终端就收）⇒ 前台程序就是那一家。
        program: f.agent.clone(),
        agent: f.agent,
        client: f.client,
        cwd: f.cwd,
        title: String::new(),
        input,
    }
}

/// 装好的那一个家（没装 ⇒ `None`）：要把数据目录一路交下去的那几处（`terminal-end`）从这里取，判据交自己的沙箱。
pub(crate) fn installed() -> Option<&'static Own> {
    HOME.get()
}

/// `own` 底下 `id` 的托管口。
fn sock_at(own: &Own, id: &str) -> std::io::Result<std::path::PathBuf> {
    home::sock_of(own, id).map_err(|s| std::io::Error::other(s.logged()))
}

/// 装好的数据目录底下 `id` 的托管口。
fn sock(id: &str) -> std::io::Result<std::path::PathBuf> {
    let own = installed()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no data dir"))?;
    sock_at(own, id)
}

/// 结束 `own` 底下的 `id`（子进程组挂断，等到它收尾）；后端替谁持有着它的输入 ⇒ 先交还。
pub(crate) fn end_at(own: &Own, id: &str) -> std::io::Result<()> {
    hold::release(id);
    client::end(&sock_at(own, id)?)
}

/// 替 `front` 接管 `id` 的输入（[`hold::take`]）。
pub(crate) fn take(id: &str, front: &str) -> std::io::Result<()> {
    hold::take(&sock(id)?, id, front)
}

/// 抓 `id` 一屏（[`client::capture`]）。
pub(crate) fn capture(id: &str, back: usize, color: bool) -> std::io::Result<client::Captured> {
    client::capture(&sock(id)?, back, color)
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
/// 没设 ⇒ `None`；环境读不到（Windows）⇒ 按进程祖先认（[`host_by_ancestry`]），认不出 ⇒ `None`。认出了 ⇒ 容器就是那个托管终端；事实里已是这个 sid ⇒ 不写；否则经托管口发 `tag`，等托管进程写完才回。
pub(crate) fn tag_pid_at(own: &Own, pid: u32, sid: &str) -> Option<Tagged> {
    use crate::platform::proc::{parent_of, proc_env_var, EnvRead};
    let id = match proc_env_var(pid, run::HOST_ENV) {
        EnvRead::Value(v) => v.trim().to_string(),
        EnvRead::Unset => return None,
        // 环境读不到（Windows 上读不到别的进程的环境）⇒ 按祖先认。
        EnvRead::Unreadable => {
            let hosts: Vec<(String, u32)> = home::alive(own)
                .into_iter()
                .filter_map(|f| f.child_pid.map(|c| (f.id, c)))
                .collect();
            host_by_ancestry(pid, &hosts, parent_of)?
        }
    };
    // 形状不对 · sid 不是会话 id：说得出它在托管终端里，但不往里写没核过的字符串。
    if !home::id_ok(&id) {
        tracing::warn!(
            "托管终端打标：pid {pid} 的 {} 形状不对，不打",
            run::HOST_ENV
        );
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

/// 往上走几层父进程就不走了（会话里的 claude 离托管终端的子进程至多隔着几层外壳；父进程表成环时也靠它停）。
const ANCESTRY_MAX: usize = 32;

/// `pid` 住在哪个托管终端里，按进程祖先认：从它自己起往上找父进程，碰到某个托管终端记下的子进程号（`hosts`：`(托管 id, 子进程号)`）
/// 就是那一个。只认此刻活着的托管终端自己写下的子进程号，进程号复用的风险由此兜住。**纯函数**：父进程从 `parent` 问。
pub(crate) fn host_by_ancestry(
    pid: u32,
    hosts: &[(String, u32)],
    parent: impl Fn(u32) -> Option<u32>,
) -> Option<String> {
    let mut at = pid;
    for _ in 0..ANCESTRY_MAX {
        if let Some((id, _)) = hosts.iter().find(|(_, child)| *child == at) {
            return Some(id.clone());
        }
        at = parent(at).filter(|p| *p != 0 && *p != at)?;
    }
    None
}

#[cfg(test)]
#[path = "../../../tests/backend/host/managed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/host/life_tests.rs"]
mod life_tests;
