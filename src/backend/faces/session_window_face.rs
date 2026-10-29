//! 帧命令 `session-window`：**这条会话在这台电脑上的哪儿**（点 ↗ 时本机后端答一次）——
//! 显示它的那个终端窗口的标签 · 那个终端往上的进程链，或写好的那一张（为什么没有链）。本机会话与远端会话同一个形，壳照它找窗口、拉前。
//!
//! - 本机会话：显示它的终端进程（在 tmux 里 ⇒ 连着它的 tmux 客户端；不在 ⇒ 它自己，`observe/session_terminals.rs::here_by`）
//!   往上数进程链（`dial/terminal_processes.rs::chain`，一趟读这台的进程表）。
//! - 远端会话：终端是 cc-monitor 开的 ⇒ 先照开的那一刻记下的标签在这台找（`dial/opened_windows.rs`：那个 shell 还是它、
//!   它自己或底下开着 ssh ⇒ 从那个 ssh 往上的链），找到就不问那台；
//!   否则经 `--resident-call` 问那台的常驻 `session-terminals`（谁在显示它，那台的原因照搬），
//!   报来的终端连接拿这台的连接表对（`dial/terminal_processes.rs`，对不上的原因照搬）。这台的表一趟只读一次。
//! - `labels`：各终端带的窗口标签（`LC_CCM_WINDOW`，最近动静在前），壳先按它找；`chain` 空 ⇔ `face` 在。
//! - `face`：用那台的名字写好的那一张（`cmd_wire::front::Unplaced::face`）。会话不在 · 那台旧 · 那台到期 · 那台没问到（那一句是连接层写的，照摆）
//!   也是那一张，不是码；码只剩这台自己没查成的 `failed` · `child_timed_out`。
//! - `tmux`：会话在 tmux 里（窗口关了时壳才给［在终端里打开］，出口原样交壳）。

use crate::dial::remote_ask::{Remote, Said, Table};
use crate::dial::opened_windows::Records;
use crate::dial::terminal_processes::{Link, OpenedHere, TerminalProcessesReq};
use crate::observe::session_terminals::{Here, Showing, ShownWhy, SidReq};
use cmd_wire::front::{FrontFace, Unplaced};
use serde::Serialize;

/// `session-window` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionWindowReq {
    /// 会话所在那台（本机 ⇒ `<local>`）。
    pub(crate) machine: cmd_wire::Machine,
    /// 会话 id。
    pub(crate) sid: cmd_wire::Sid,
}

/// `session-window` 的应答。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SessionWindow {
    /// 显示它的终端带的窗口标签（`<shell 进程号>-<起始时刻>`），最近动静在前；没带 ⇒ 空。
    pub(crate) labels: Vec<String>,
    /// 这台上显示它的那个终端进程往上的进程链（自下而上）；空 ⇔ 有 `face`。
    pub(crate) chain: Vec<Link>,
    /// 会话在 tmux 里。
    pub(crate) tmux: bool,
    /// 链为什么是空的：写好的那一张（出口照摆；壳按标签也没找到窗口时摆它）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) face: Option<FrontFace>,
}

crate::wire::typed::codes! {
    /// `session-window` 的码（这台自己没查成；那台的事写成那一张）。
    pub(crate) enum SessionWindowErr {
        /// 问 tmux 那一发没成（起不来 / 报错）· 那台答的形状自相矛盾。
        Failed = "failed",
        /// 问 tmux 那一发过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `session-window`：这条会话在这台电脑上的哪儿。
pub(crate) struct SessionWindowCmd;

impl crate::wire::typed::Command for SessionWindowCmd {
    const NAME: &'static str = "session-window";
    type Req = SessionWindowReq;
    type Reply = SessionWindow;
    type Err = SessionWindowErr;
}

impl crate::wire::typed::Handle for SessionWindowCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: SessionWindowReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SessionWindow, SessionWindowErr>> + Send + 'static
    {
        async move {
            answer_with(
                &req,
                local_here,
                crate::platform::connection_and_process_tables,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                crate::dial::opened_windows::OnDisk(cx.needs),
            )
            .await
        }
    }
}

/// 生产那一份「本机这个会话此刻由谁显示」：会话账里找它的进程号（没有 ⇒ `None`），再问 [`crate::observe::session_terminals::here_by`]。
fn local_here(sid: String) -> Result<Option<Here>, SessionWindowErr> {
    let home = crate::history::listing::agent_home();
    let Some(pid) = crate::observe::watcher::running_sessions(&home)
        .into_iter()
        .find(|(s, _)| *s == sid)
        .map(|(_, pid)| pid)
    else {
        return Ok(None);
    };
    crate::observe::session_terminals::here_by(pid)
        .map(Some)
        .map_err(|(code, said)| match code {
            crate::platform::child::TIMED_OUT => SessionWindowErr::ChildTimedOut(said),
            _ => SessionWindowErr::Failed(said),
        })
}

/// 本体（本机那一问 · 这台的两张表 · 可达表 · 对面由调用方给，判据喂替身）。
pub(crate) async fn answer_with<H, Q, R>(
    req: &SessionWindowReq,
    here: H,
    tables: Q,
    tz: &crate::Tz,
    until: Option<crate::platform::child::Until>,
    table: &Table,
    remote: &dyn Remote,
    records: R,
) -> Result<SessionWindow, SessionWindowErr>
where
    H: FnOnce(String) -> Result<Option<Here>, SessionWindowErr> + Send + 'static,
    Q: FnOnce() -> Result<String, String> + Send + 'static,
    R: Records,
{
    let sid = req.sid.as_str().to_string();
    if req.machine.is_local() {
        let name = copy_core::local_machine();
        return blocking(move || match here(sid)? {
            Some(h) => local_window(h, tables, &name),
            None => Ok(reason(Vec::new(), false, Unplaced::NotLive, &name)),
        })
        .await;
    }
    let Some(until) = until else {
        return Err(SessionWindowErr::BadArgs(
            crate::common::contract::malformed(
                "`within_ms` is required when asking another machine: how long to wait belongs to the asker",
            ),
        ));
    };
    let Some(machine) = req.machine.host_name().map(str::to_string) else {
        return Err(SessionWindowErr::BadArgs(
            crate::common::contract::malformed("`machine` must name a machine"),
        ));
    };
    // 本机开过的那个终端：先照开的那一刻记下的标签在这台找，找到就不问那台；这一趟读的这台的表留给下面那一支用。
    let (hit, tables) = {
        let (sid, machine) = (sid.clone(), machine.clone());
        tokio::task::spawn_blocking(move || opened_here(&records, &sid, &machine, tables))
            .await
            .map_err(|e| SessionWindowErr::Failed(e.to_string()))?
    };
    if let Some(w) = hit {
        return Ok(w);
    }
    let shown =
        crate::dial::remote_ask::ask_resident::<crate::faces::feature_face::SessionTerminals>(
            &machine,
            &SidReq {
                sid: req.sid.clone(),
            },
            tz,
            crate::platform::child::Deadline::millis(until.left_ms()),
            table,
            remote,
        )
        .await;
    match shown {
        Ok(shown) => blocking(move || remote_window(shown, tables, &machine)).await,
        Err(s) => Ok(reason(Vec::new(), false, not_asked(s)?, &machine)),
    }
}

/// 这台的连接表与进程表那一趟：没读过 ⇒ 到用的时候读；读过 ⇒ 用读到的那一份。
type TablesOnce = Box<dyn FnOnce() -> Result<String, String> + Send>;

/// 本机开过的那个终端（`dial/opened_windows.rs`）：记下的那台就是这台 · 标签那个 shell 还是它 · 它自己或底下开着 ssh
/// ⇒ 从那个 ssh 往上的链（`labels` 就是那个标签；不问那台，说不出在不在 tmux 里 ⇒ `tmux: false`）。
/// shell 没了 ⇒ 删那一份。没记 · 别台的 · 没开着 ssh ⇒ `None`，问那台。
fn opened_here<Q, R>(records: &R, sid: &str, machine: &str, tables: Q) -> (Option<SessionWindow>, TablesOnce)
where
    Q: FnOnce() -> Result<String, String> + Send + 'static,
    R: Records,
{
    let Some(rec) = records.get(sid).filter(|r| r.machine == machine) else {
        return (None, Box::new(tables));
    };
    let raw = tables();
    let found = raw
        .clone()
        .and_then(|r| crate::dial::terminal_processes::parse_tables(&r))
        .map(|t| crate::dial::terminal_processes::opened_chain(&rec.label, &t.procs));
    let hit = match found {
        Ok(OpenedHere::Chain(chain)) => Some(SessionWindow {
            labels: vec![rec.label],
            chain,
            tmux: false,
            face: None,
        }),
        Ok(OpenedHere::Gone) => {
            records.forget(sid);
            None
        }
        Ok(OpenedHere::NoSsh) => None,
        Err(e) => {
            tracing::warn!("session-window: 进程表没问成：{e}");
            None
        }
    };
    (hit, Box::new(move || raw))
}

/// 阻塞的那一段（读 `/proc` · 起 tmux · 读系统表）放到阻塞线程上。
async fn blocking<F>(f: F) -> Result<SessionWindow, SessionWindowErr>
where
    F: FnOnce() -> Result<SessionWindow, SessionWindowErr> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(SessionWindowErr::Failed(e.to_string())))
}

/// 链空、只有那一张的那一份。
fn reason(labels: Vec<String>, tmux: bool, why: Unplaced, machine: &str) -> SessionWindow {
    SessionWindow {
        labels,
        chain: Vec::new(),
        tmux,
        face: Some(why.face(machine)),
    }
}

/// 本机会话：显示它的终端进程 ⇒ 第一个在进程表里数得出链的那一个。
fn local_window(
    here: Here,
    tables: impl FnOnce() -> Result<String, String>,
    machine: &str,
) -> Result<SessionWindow, SessionWindowErr> {
    let (seen, tmux) = match here {
        Here::Detached => return Ok(reason(Vec::new(), true, Unplaced::Detached, machine)),
        Here::By { seen, tmux } => (seen, tmux),
    };
    let labels = seen.iter().filter_map(|(_, t)| t.window.clone()).collect();
    let procs = match tables().and_then(|raw| crate::dial::terminal_processes::parse_tables(&raw)) {
        Ok(t) => t.procs,
        Err(e) => {
            tracing::warn!("session-window: 进程表没问成：{e}");
            return Ok(reason(labels, tmux, Unplaced::QueryFailed, machine));
        }
    };
    Ok(
        match seen
            .iter()
            .find_map(|(pid, _)| crate::dial::terminal_processes::chain(*pid, &procs))
        {
            Some(chain) => SessionWindow {
                labels,
                chain,
                tmux,
                face: None,
            },
            None => reason(labels, tmux, Unplaced::QueryFailed, machine),
        },
    )
}

/// 远端会话：那台的原因照搬；报来的终端连接拿这台的连接表对。
fn remote_window(
    shown: Showing,
    tables: impl FnOnce() -> Result<String, String>,
    machine: &str,
) -> Result<SessionWindow, SessionWindowErr> {
    let tmux = shown.tmux;
    if let Some(why) = shown.why {
        let why = match why {
            ShownWhy::Detached => Unplaced::Detached,
            ShownWhy::NoTerminal => Unplaced::NoTerminal,
            ShownWhy::Unreadable => Unplaced::Unreadable,
        };
        return Ok(reason(Vec::new(), tmux, why, machine));
    }
    let labels = shown
        .terminals
        .iter()
        .filter_map(|t| t.window.clone())
        .collect();
    let found = crate::dial::terminal_processes::answer_with(
        &TerminalProcessesReq {
            terminals: shown.terminals,
        },
        tables,
    )
    .map_err(|(_, said)| SessionWindowErr::Failed(said))?;
    // 对不上的那几个词（`dial/terminal_processes.rs::product` 写的）⇒ 原因。
    let why = match (found.why, found.addr) {
        (None, _) => {
            return Ok(SessionWindow {
                labels,
                chain: found.chain,
                tmux,
                face: None,
            })
        }
        (Some("not-ssh"), _) => Unplaced::NotSsh,
        (Some("elsewhere"), addr) => Unplaced::Elsewhere {
            addr: addr.unwrap_or_default(),
        },
        (Some("mismatch"), _) => Unplaced::Mismatch,
        (Some(_), _) => Unplaced::QueryFailed,
    };
    Ok(reason(labels, tmux, why, machine))
}

/// 问那台没问成：不认 · 形状对不上 ⇒ 那台旧；到期 ⇒ 那台到期；那台说会话不在 ⇒ 不在；那台自己没查成 ⇒ 码照转；
/// 别的（断开 · 常驻不在）⇒ 没问到，那一句照摆。
fn not_asked(s: Said) -> Result<Unplaced, SessionWindowErr> {
    Ok(match s.code.as_deref() {
        Some("unknown_command" | "backend_differs") => Unplaced::PeerOld,
        Some(crate::wire::listen::CALL_TIMED_OUT) => Unplaced::PeerTimedOut,
        Some("no_such_session") => Unplaced::NotLive,
        Some("failed") => return Err(SessionWindowErr::Failed(s.message)),
        Some(crate::platform::child::TIMED_OUT) => {
            return Err(SessionWindowErr::ChildTimedOut(s.message))
        }
        _ => Unplaced::Unreached { said: s.message },
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_window_face_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/faces/front_face_tests.rs"]
mod front_face_tests;
