//! 帧命令 `session-window`：**这条会话在这台电脑上的哪儿**（点 ↗ 时本机后端答一次）——
//! 显示它的那个终端窗口的标签 · 那个终端往上的进程链，或一条说得出的原因。本机会话与远端会话同一个形，壳照它找窗口、拉前。
//!
//! - 本机会话：显示它的终端进程（在 tmux 里 ⇒ 连着它的 tmux 客户端；不在 ⇒ 它自己，`observe/session_terminals.rs::here_by`）
//!   往上数进程链（`dial/terminal_processes.rs::chain`，一趟读这台的进程表）。
//! - 远端会话：经 `--resident-call` 问那台的常驻 `session-terminals`（谁在显示它，那台的原因照搬），
//!   报来的终端连接拿这台的连接表对（`dial/terminal_processes.rs`，对不上的原因照搬）。
//! - `labels`：各终端带的窗口标签（`LC_CCM_WINDOW`，最近动静在前），壳先按它找；`chain` 空 ⇔ `why` 在。
//! - 那台没问到 ⇒ 码 `unreached`，那一句是连接层写的（照摆）；那台不认 ⇒ `backend_old`；那台到期 ⇒ `timed_out`。

use crate::dial::remote_ask::{Remote, Said, Table};
use crate::dial::terminal_processes::{Link, TerminalProcessesReq};
use crate::observe::session_terminals::{Here, Showing, SidReq};
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
    /// 这台上显示它的那个终端进程往上的进程链（自下而上）；空 ⇔ 有 `why`。
    pub(crate) chain: Vec<Link>,
    /// 链为什么是空的：`detached`（在 tmux 里、没终端连着）· `no-terminal`（进程没有终端）· `unreadable`（读不了那台的终端）·
    /// `not-ssh`（那台本地屏幕上的终端）· `elsewhere`（连接不是这台开的）· `mismatch`（经跳板 / 端口转换）· `query-failed`（这台没查成）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        test,
        ts(
            optional,
            type = "\"detached\" | \"no-terminal\" | \"unreadable\" | \"not-ssh\" | \"elsewhere\" | \"mismatch\" | \"query-failed\""
        )
    )]
    pub(crate) why: Option<&'static str>,
    /// `elsewhere` 时那台看到的对面地址。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) addr: Option<String>,
}

crate::wire::typed::codes! {
    /// `session-window` 的码。
    pub(crate) enum SessionWindowErr {
        /// 那台没有在跑的这个会话。
        NoSuchSession = "no_such_session",
        /// 问 tmux 那一发没成（起不来 / 报错）· 那台答的形状自相矛盾。
        Failed = "failed",
        /// 问 tmux 那一发过了期限。
        ChildTimedOut = "child_timed_out",
        /// 那台后端不认这一问（比这一版旧）。
        BackendOld = "backend_old",
        /// 那台没在期限内答。
        TimedOut = "timed_out",
        /// 那台没问到（断开 · 常驻不在）：那一句是连接层写的。
        Unreached = "unreached",
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
    type Needs = crate::wire::typed::needs::Nothing;
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
            )
            .await
        }
    }
}

/// 生产那一份「本机这个会话此刻由谁显示」：会话账里找它的进程号，再问 [`crate::observe::session_terminals::here_by`]。
fn local_here(sid: String) -> Result<Here, SessionWindowErr> {
    let home = crate::history::listing::agent_home();
    let pid = crate::observe::watcher::running_sessions(&home)
        .into_iter()
        .find(|(s, _)| *s == sid)
        .map(|(_, pid)| pid)
        .ok_or_else(|| {
            SessionWindowErr::NoSuchSession(crate::common::contract::malformed(
                "no running session with that `sid` on this machine",
            ))
        })?;
    crate::observe::session_terminals::here_by(pid).map_err(|(code, said)| match code {
        crate::platform::child::TIMED_OUT => SessionWindowErr::ChildTimedOut(said),
        _ => SessionWindowErr::Failed(said),
    })
}

/// 本体（本机那一问 · 这台的两张表 · 可达表 · 对面由调用方给，判据喂替身）。
pub(crate) async fn answer_with<H, Q>(
    req: &SessionWindowReq,
    here: H,
    tables: Q,
    tz: &crate::Tz,
    until: Option<crate::platform::child::Until>,
    table: &Table,
    remote: &dyn Remote,
) -> Result<SessionWindow, SessionWindowErr>
where
    H: FnOnce(String) -> Result<Here, SessionWindowErr> + Send + 'static,
    Q: FnOnce() -> Result<String, String> + Send + 'static,
{
    let sid = req.sid.as_str().to_string();
    if req.machine.is_local() {
        return blocking(move || local_window(here(sid)?, tables)).await;
    }
    let Some(until) = until else {
        return Err(SessionWindowErr::BadArgs(
            crate::common::contract::malformed(
                "`within_ms` is required when asking another machine: how long to wait belongs to the asker",
            ),
        ));
    };
    let machine = req.machine.as_wire_str();
    let shown = crate::dial::remote_ask::ask_resident::<crate::faces::feature_face::SessionTerminals>(
        machine,
        &SidReq { sid: req.sid.clone() },
        tz,
        crate::platform::child::Deadline::millis(until.left_ms()),
        table,
        remote,
    )
    .await
    .map_err(not_asked)?;
    blocking(move || remote_window(shown, tables)).await
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

/// 链空、只有原因的那一份。
fn reason(labels: Vec<String>, why: &'static str, addr: Option<String>) -> SessionWindow {
    SessionWindow {
        labels,
        chain: Vec::new(),
        why: Some(why),
        addr,
    }
}

/// 本机会话：显示它的终端进程 ⇒ 第一个在进程表里数得出链的那一个。
fn local_window(
    here: Here,
    tables: impl FnOnce() -> Result<String, String>,
) -> Result<SessionWindow, SessionWindowErr> {
    let seen = match here {
        Here::Detached => return Ok(reason(Vec::new(), "detached", None)),
        Here::By(seen) => seen,
    };
    let labels = seen.iter().filter_map(|(_, t)| t.window.clone()).collect();
    let procs = match tables().and_then(|raw| crate::dial::terminal_processes::parse_tables(&raw)) {
        Ok(t) => t.procs,
        Err(e) => {
            tracing::warn!("session-window: 进程表没问成：{e}");
            return Ok(reason(labels, "query-failed", None));
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
                why: None,
                addr: None,
            },
            None => reason(labels, "query-failed", None),
        },
    )
}

/// 远端会话：那台的原因照搬；报来的终端连接拿这台的连接表对。
fn remote_window(
    shown: Showing,
    tables: impl FnOnce() -> Result<String, String>,
) -> Result<SessionWindow, SessionWindowErr> {
    if let Some(why) = shown.why {
        return Ok(reason(Vec::new(), why.word(), None));
    }
    let labels = shown.terminals.iter().filter_map(|t| t.window.clone()).collect();
    let found = crate::dial::terminal_processes::answer_with(
        &TerminalProcessesReq {
            terminals: shown.terminals,
        },
        tables,
    )
    .map_err(|(_, said)| SessionWindowErr::Failed(said))?;
    Ok(match found.why {
        Some(why) => reason(labels, why, found.addr),
        None => SessionWindow {
            labels,
            chain: found.chain,
            why: None,
            addr: None,
        },
    })
}

/// 问那台没问成 ⇒ 码：不认 · 形状对不上 ⇒ 旧；到期 ⇒ 到期；那台自己答的码照转；别的（断开 · 常驻不在）⇒ 没问到，那一句照摆。
fn not_asked(s: Said) -> SessionWindowErr {
    match s.code.as_deref() {
        Some("unknown_command" | "backend_differs") => SessionWindowErr::BackendOld(s.message),
        Some(crate::wire::listen::CALL_TIMED_OUT) => SessionWindowErr::TimedOut(s.message),
        Some("no_such_session") => SessionWindowErr::NoSuchSession(s.message),
        Some("failed") => SessionWindowErr::Failed(s.message),
        Some(crate::platform::child::TIMED_OUT) => SessionWindowErr::ChildTimedOut(s.message),
        _ => SessionWindowErr::Unreached(s.message),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_window_face_tests.rs"]
mod tests;
