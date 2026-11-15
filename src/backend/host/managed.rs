//! 托管那一种宿主：cc-monitor 自己持伪终端的终端（托管进程 `ccm -- --host-run`，家在数据目录底下 `hosts/<id>/`）。
//!
//! 后端对托管终端不留进程内状态：名单（[`rows`]）与打标（[`tag_pid`]）都是问的那一刻凭盘现做 ⇒ 后端重启 · 升级之后照样认得。

pub mod attach;
pub mod bin;
pub mod client;
pub mod hold;
pub mod home;
pub(crate) mod keys;
pub mod run;
pub mod screen;
pub(crate) mod serve;
pub mod status;
pub mod wire;

use super::tmux::seat::{seat_with, Seat};
use super::TermRow;
use crate::platform::fs::Own;

/// 托管名单的家：上层起来那一下装一次（看守 `observe::watcher` · 一次性那一形 `stream::cli` · ccm 那一趟 `control::ccm`），
/// 之后名单与打标照它的数据目录。没装（判据）⇒ 名单空、不打标。
static HOME: std::sync::OnceLock<Own> = std::sync::OnceLock::new();

/// 装一次（拿上层起来那一下造的写句柄；第二次装的不换）。
pub fn install(own: &Own) {
    if HOME.set(own.clone()).is_err() {
        tracing::debug!("托管名单的家已经装过了，留着第一份");
    }
}

/// 托管那一臂的名单：行 ＋ 连着它们的真终端（`(托管 id, 接上, 最近动静)`，秒）。
pub type Hosted = (Vec<TermRow>, Vec<(String, u64, u64)>);

/// 这台此刻的托管终端（每行 `host` 是 [`crate::wire::TerminalHost::Managed`]）＋ 连着它们的真终端（`(托管 id, 接上, 最近动静)`，秒）。
pub fn listed() -> Hosted {
    HOME.get().map(listed_at).unwrap_or_default()
}

/// [`listed`] 的本体：数据目录由调用方给。每个托管终端问一次 `clients`（谁连着 · 谁持有输入）；问不到 ⇒ 那一行照列、持有者当没有。
pub fn listed_at(own: &Own) -> Hosted {
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
pub fn rows_at(own: &Own) -> Vec<TermRow> {
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
        // 托管终端的子进程就是它起的那一家（那一家一退终端就收）⇒ 程序那一格报那一家（它在里面另起的后台进程不算）。
        program: f.agent.clone(),
        agent: f.agent,
        client: f.client,
        cwd: f.cwd,
        title: String::new(),
        input,
    }
}

/// 装好的那一个家（没装 ⇒ `None`）：要把数据目录一路交下去的那几处（`terminal-end`）从这里取，判据交自己的沙箱。
pub fn installed() -> Option<&'static Own> {
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
pub fn end_at(own: &Own, id: &str) -> std::io::Result<()> {
    hold::release(id);
    client::end(&sock_at(own, id)?)
}

/// 替 `front` 接管 `id` 的输入（[`hold::take`]）。
pub fn take(id: &str, front: &str) -> std::io::Result<()> {
    hold::take(&sock(id)?, id, front)
}

/// 打标的结局（托管这一臂与 `control::identity_tag::tag_seen` 交回同一形）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tagged {
    /// 这条会话住在什么容器里（判不了 ⇒ 没有）。
    pub container: Option<crate::wire::SessionContainer>,
    /// 这次真写了身份标没有。
    pub wrote: bool,
    /// 这个进程没有控制终端（不是谁的终端会话，`claude -p` 那一形）⇒ 观测侧当后台宣告。两臂都照 `host::tmux::seat` 那一处判。
    pub detached: bool,
}

impl Tagged {
    /// 有控制终端的那一形。
    pub fn seated(container: Option<crate::wire::SessionContainer>, wrote: bool) -> Tagged {
        Tagged {
            container,
            wrote,
            detached: false,
        }
    }
}

/// 宿主口「打标」托管那一臂：`pid` 住在托管终端里 ⇒ 把 `sid` 写进那个托管终端的事实；不在 ⇒ `None`（交 tmux 那一臂）。
pub fn tag_pid(pid: u32, sid: &str) -> Option<Tagged> {
    tag_pid_at(HOME.get()?, pid, sid)
}

/// [`tag_pid`] 的本体：数据目录由调用方给（观测侧的判据交它起的那个托管终端的数据目录，走真判法）。
///
/// 认法：这个进程环境里的 `CCM_HOST`（托管进程给子进程设的，[`run::HOST_ENV`]；里面的 ccm · claude 原样继承）。
/// 没设 ⇒ `None`；环境读不到（Windows）⇒ 按进程祖先认住在哪个托管终端里（[`host_by_ancestry`]），认不出 ⇒ `None`。认出了、却不是那个托管终端前台的那一个
/// （没控制终端 · 后台进程组 · 别的终端上 · 说不清，[`seat_in`]）⇒ 不打、容器说不清；是 ⇒ 容器就是那个托管终端；事实里已是这个 sid ⇒ 不写；否则经托管口发 `tag`，等托管进程写完才回。
pub fn tag_pid_at(own: &Own, pid: u32, sid: &str) -> Option<Tagged> {
    let id = host_of_pid(own, pid)?;
    // 形状不对 · sid 不是会话 id：说得出它在托管终端里，但不往里写没核过的字符串。
    if !home::id_ok(&id) {
        tracing::warn!(
            "托管终端打标：pid {pid} 的 {} 形状不对，不打",
            run::HOST_ENV
        );
        return Some(Tagged::seated(None, false));
    }
    // 容器只在事实里挂着这个 sid 时报（接回认的就是那一行）；不往里写 · 写不上 ⇒ 说不清（`None`），不给一颗接不上的按钮。
    let container = Some(crate::wire::SessionContainer::Managed {
        terminal: Some(id.clone()),
    });
    if sid.parse::<cmd_wire::Sid>().is_err() {
        return Some(Tagged::seated(None, false));
    }
    let dir = home::dir_of(own, &id).ok()?;
    let facts = home::read_facts(dir.as_path());
    // 只给那个托管终端前台的那一个打标（判法同 tmux 窗格那一处，[`seat_with`]）：托管终端里再起的 `claude -p`（没控制终端）·
    // 后台进程组里的进程环境里也带着 `CCM_HOST`，打上了就把事实里主会话的 sid 换成自己的 ⇒ 主会话那张脸亮着、接回落空。
    // 托管这一形认首领：开那个伪终端的会话首领就是托管进程起的那个子进程（事实里的 `child_pid`）；读不到终端的平台照 ccm 起的那一跳认（[`seat_in`]）。
    match seat_in(
        facts.as_ref(),
        pid,
        READS_TTY,
        crate::platform::proc::parent_of,
    ) {
        Seat::OnPane => {}
        Seat::Detached => {
            return Some(Tagged {
                container: None,
                wrote: false,
                detached: true,
            })
        }
        // 说不清（进程没了 · 事实里还没写下子进程号 / 那一跳 · 读不到终端的平台上是那一跳的后代）⇒ 不打。
        Seat::Elsewhere | Seat::Unknown => return Some(Tagged::seated(None, false)),
    }
    if facts.is_some_and(|f| f.sid == sid) {
        return Some(Tagged::seated(container, false));
    }
    let done = match home::sock_of(own, &id) {
        Ok(sock) => client::tag(&sock, sid).map_err(|e| e.to_string()),
        Err(s) => Err(s.logged()),
    };
    match done {
        Ok(()) => Some(Tagged::seated(container, true)),
        // 写不上 · 等回话过了期限（标也许随后落盘）：此刻说不清。标签页那张脸的容器宣告后不再改 ⇒ 标随后落了盘也灰着（清单那几张问的那一刻现读、会亮）。
        Err(why) => {
            tracing::warn!("托管终端 {id} 打标没打上（pid {pid} · sid {sid}）：{why}");
            Some(Tagged::seated(None, false))
        }
    }
}

/// 这个平台读得到终端那一样（`tty_of` 只在 Linux 上有）。
const READS_TTY: bool = cfg!(target_os = "linux");

/// `pid` 跟它住的那个托管终端（事实 `facts`）的关系 —— 打标与「谁在显示它」都经它，判法只此一处：
/// - 读得到终端（`reads_tty`，Linux）：[`seat_with`]，托管这一形认首领的法子是事实里的子进程号
///   （开那个伪终端的会话首领就是托管进程起的那个子进程）；事实读不到 · 还没写下子进程号 ⇒ 说不清。
/// - 读不到（Windows · macOS）：照里面那个 ccm 起的那一跳（事实里的 `hop_pid` · `hop_wrapped`）认 —— 会话进程就是它，
///   或那一跳是 ccm 明知的「包一层」（`cmd.exe /c claude.cmd` · `powershell -File claude.ps1`，那一层只跑这一条）且会话进程的父进程
///   （`parent`）就是它 ⇒ 前台；再往下的后代 · 还没记下 · 事实读不到 ⇒ 说不清。祖先里有它不顶替：后代里也有没控制终端 · 后台的那几形，分不出。
fn seat_in(
    facts: Option<&home::Facts>,
    pid: u32,
    reads_tty: bool,
    parent: impl Fn(u32) -> Option<u32>,
) -> Seat {
    if !reads_tty {
        let Some(hop) = facts.and_then(hop_in) else {
            return Seat::Unknown;
        };
        return if is_the_hop(pid, hop, parent) {
            Seat::OnPane
        } else {
            Seat::Unknown
        };
    }
    let child = facts.and_then(|f| f.child_pid);
    seat_with(pid, |leader| child.map(|c| c == leader))
}

/// 托管终端里那个 ccm 起 agent 的那一跳（事实里的 `hop_pid` · `hop_wrapped`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hop {
    /// 那一跳的 pid（ccm 的起会话便条也留在它上面）。
    pub pid: u32,
    /// 那一跳是 ccm 明知的「包一层」：它的直接子进程才是 agent。
    pub wrapped: bool,
}

/// 事实里记下的那一跳；还没记下 ⇒ `None`。
fn hop_in(f: &home::Facts) -> Option<Hop> {
    f.hop_pid.map(|pid| Hop {
        pid,
        wrapped: f.hop_wrapped,
    })
}

/// **会话进程 `pid` 是不是 ccm 起的那一跳** —— 只此一处判（读不到终端的平台认前台 [`seat_in`] · 认起会话便条 `control::launch_account`）：
/// 就是它，或那一跳是包一层、`pid` 的父进程（`parent`）就是它 ⇒ 是；再往下的后代 ⇒ 不是（祖先里有它不顶替）。**纯函数**。
pub fn is_the_hop(pid: u32, hop: Hop, parent: impl Fn(u32) -> Option<u32>) -> bool {
    pid == hop.pid || (hop.wrapped && parent(pid) == Some(hop.pid))
}

/// `pid` 住在托管终端里 ⇒ 那个托管终端事实里记的那一跳（数据目录是装上的那一份）；不住在托管终端里 · 没装 · 事实读不到 · 还没记下 ⇒ `None`。
pub fn hop_of_pid(pid: u32) -> Option<Hop> {
    let own = HOME.get()?;
    let id = host_of_pid(own, pid)?;
    let dir = home::dir_of(own, &id).ok()?;
    home::read_facts(dir.as_path()).as_ref().and_then(hop_in)
}

/// `pid` 住在哪个托管终端里（打标与「谁在显示它」同一处认法）：它环境里的 `CCM_HOST`（[`run::HOST_ENV`]）；
/// 环境读不到（Windows 上读不到别的进程的环境）⇒ 按进程祖先认（[`host_by_ancestry`]）。不在托管终端里 ⇒ `None`。
fn host_of_pid(own: &Own, pid: u32) -> Option<String> {
    use crate::platform::proc::{parent_of, proc_env_var, EnvRead};
    match proc_env_var(pid, run::HOST_ENV) {
        EnvRead::Value(v) => Some(v.trim().to_string()),
        EnvRead::Unset => None,
        EnvRead::Unreadable => {
            let hosts: Vec<(String, u32)> = home::alive(own)
                .into_iter()
                .filter_map(|f| f.child_pid.map(|c| (f.id, c)))
                .collect();
            host_by_ancestry(pid, &hosts, parent_of)
        }
    }
}

/// 附着端里显示会话的那几个真终端（后端那一端不算）：`(进程号, 最近动静秒)`，报不出进程号 ⇒ `None`；最近动静在前。**纯函数**。
pub(crate) fn terminal_ends(rows: Vec<wire::ClientRow>) -> Vec<(Option<u32>, u64)> {
    let mut ends: Vec<(Option<u32>, u64)> = rows
        .into_iter()
        .filter(|r| r.kind == wire::WhoKind::Terminal)
        .map(|r| (r.pid, u64::try_from(r.last_ms / 1000).unwrap_or(0)))
        .collect();
    ends.sort_by_key(|e| std::cmp::Reverse(e.1));
    ends
}

/// 住在托管终端里的进程，那个托管终端的附着端在不在显示它（[`terminal_ends_at`]）。
#[derive(Debug)]
pub enum Ends {
    /// 它是那个托管终端前台的那一个：此刻附着着的真终端（[`terminal_ends`]；`Ok(空)` ＝ 一个都没有，问托管口没问成 ⇒ `Err`）。
    Front(std::io::Result<Vec<(Option<u32>, u64)>>),
    /// 不是（没控制终端 · 后台进程组 · 别的终端上 · 说不清）：附着端不在显示它，托管口不问。
    Not(Seat),
}

/// [`terminal_ends_at`]，数据目录是装上的那一份；名单的家没装 ⇒ `None`。
pub fn terminal_ends_of_pid(pid: u32) -> Option<Ends> {
    terminal_ends_at(HOME.get()?, pid)
}

/// `pid` 住在托管终端里 ⇒ 那个托管终端的附着端在不在显示它（[`Ends`]）；不住在托管终端里 ⇒ `None`。
pub fn terminal_ends_at(own: &Own, pid: u32) -> Option<Ends> {
    let id = host_of_pid(own, pid)?;
    let facts = home::dir_of(own, &id)
        .ok()
        .and_then(|dir| home::read_facts(dir.as_path()));
    match seat_in(
        facts.as_ref(),
        pid,
        READS_TTY,
        crate::platform::proc::parent_of,
    ) {
        Seat::OnPane => {}
        seat => return Some(Ends::Not(seat)),
    }
    let asked = home::sock_of(own, &id)
        .map_err(|s| std::io::Error::other(s.logged()))
        .and_then(|sock| client::clients(&sock));
    Some(Ends::Front(asked.map(terminal_ends)))
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
