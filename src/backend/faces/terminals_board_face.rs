//! 「终端」视图那两条（typed）：`terminals-board-here`（这台那几行：终端名单 ＋ 这台的会话事实）·
//! `terminals-board`（本机后端问到各台、合成一份）。本体在 `control/terminals_board.rs`。

use crate::control::terminals::{self as t, On};
use crate::control::terminals_board::{self as b, Got, SessionBits};
use crate::control::terminals_wire::TerminalsListReq;
use crate::wire::typed::{self, codes, Command};
use cmd_wire::board::{BoardHere, TerminalsBoard};

/// `terminals-board-here` 的入参。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BoardHereReq {
    /// 自报的前端（决定每行的 `can_end`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

codes! {
    /// `terminals-board-here` 的码：列名单那两发 tmux。
    pub(crate) enum BoardHereErr {
        /// 列不出终端名单。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `terminals-board-here`：这台的终端名单写成「终端」视图的那几行。阻塞档（起 tmux · 读活会话表）。
pub(crate) struct TerminalsBoardHereCmd;

impl Command for TerminalsBoardHereCmd {
    const NAME: &'static str = "terminals-board-here";
    type Req = BoardHereReq;
    type Reply = BoardHere;
    type Err = BoardHereErr;
}

impl crate::wire::typed::Handle for TerminalsBoardHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BoardHereReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BoardHere, BoardHereErr>> + Send + 'static {
        async move { here(&req) }
    }
}

/// [`TerminalsBoardHereCmd`] 的本体（同步：本机那一台在 `terminals-board` 里也经它答）。
pub(crate) fn here(req: &BoardHereReq) -> Result<BoardHere, BoardHereErr> {
    let list = t::list_on(
        On::default(),
        &TerminalsListReq {
            client: req.client.clone(),
        },
    )
    .map_err(|f| match f.code.as_str() {
        "no_tmux" => BoardHereErr::Unobservable(crate::common::contract::joined(f.said, f.raw)),
        _ => typed::of_fail(f, BoardHereErr::Unobservable),
    })?;
    let agent_home = crate::history::listing::agent_home();
    let doing = crate::history::live_table::now(&agent_home).doing();
    let accounts = crate::observe::accounts_query::session_accounts_here(&agent_home);
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    let session = |sid: &str| -> Option<SessionBits> {
        use crate::wire::SessionFaceExt as _;
        let activity = doing.get(sid)?;
        Some(SessionBits {
            title: crate::history::session_hidden::title_here(sid, &annotations),
            account: accounts
                .iter()
                .find(|a| a.sid.as_deref() == Some(sid))
                .and_then(|a| a.account.clone()),
            face: crate::wire::SessionFace::of(crate::wire::Life::Live {
                activity: *activity,
                host: Some(crate::wire::Housed::In(crate::wire::TerminalHost::Tmux)),
            }),
        })
    };
    let now = u64::try_from(crate::common::time::now_secs()).unwrap_or(0);
    Ok(b::rows_here(
        &list,
        &session,
        now,
        crate::platform::paths::home_dir().as_deref(),
    ))
}

/// `terminals-board` 的入参：问哪几台（本机逐字 `"<local>"`）· 自报的前端。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BoardReq {
    pub(crate) machines: Vec<cmd_wire::Machine>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

codes! {
    /// `terminals-board` 自己没有别的码（哪台没答只进那一组的 `down`）。
    pub(crate) enum BoardErr {}
}

/// `terminals-board`：本机后端问到各台的那几行（本机同进程答，别的台经常驻问 `terminals-board-here`），合成一份再答。
pub(crate) struct TerminalsBoardCmd;

impl Command for TerminalsBoardCmd {
    const NAME: &'static str = "terminals-board";
    type Req = BoardReq;
    type Reply = TerminalsBoard;
    type Err = BoardErr;
}

impl crate::wire::typed::Handle for TerminalsBoardCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BoardReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<TerminalsBoard, BoardErr>> + Send + 'static {
        async move {
            let ask = BoardHereReq {
                client: req.client.clone(),
            };
            let mine = ask.clone();
            let local = async move {
                tokio::task::spawn_blocking(move || here(&mine))
                    .await
                    .map_err(|e| crate::dial::remote_ask::Said {
                        code: None,
                        message: e.to_string(),
                    })?
                    .map_err(|e| {
                        use crate::wire::typed::Codes as _;
                        let f = e.into_said();
                        crate::dial::remote_ask::Said {
                            code: Some(f.code.clone()),
                            message: f.message,
                        }
                    })
            };
            let got = crate::dial::remote_ask::ask_each::<TerminalsBoardHereCmd, _>(
                &req.machines,
                &ask,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map_err(BoardErr::BadArgs)?;
            let now = crate::common::time::now_secs();
            let at = crate::common::time::fmt_at(now, now, &cx.tz);
            let got = got
                .into_iter()
                .map(|(m, r)| {
                    let g = match r {
                        Ok(here) => Got::Answered(here, at.clone()),
                        Err(s) => Got::Down(s.message),
                    };
                    (m, g)
                })
                .collect();
            Ok(b::merge(got, &b::LAST_SEEN))
        }
    }
}
