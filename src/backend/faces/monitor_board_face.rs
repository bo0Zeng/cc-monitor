//! 监控板那两条（typed）：`monitor-board-here`（这台那几条会话的序 · 活不活 · 在跑几个）·
//! `monitor-board`（本机后端问到各台、合成一份：分组 · 组内先后 · 组头 · 汇总那一句）。本体在 `control/monitor_board.rs`。

use crate::control::monitor_board::{self as b, Called};
use crate::wire::typed::{codes, Command};
use cmd_wire::board::{MonitorBoard, MonitorHere};
use std::collections::BTreeMap;

/// `monitor-board-here` 的入参：问的那几条会话。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitorHereReq {
    pub(crate) sids: Vec<String>,
}

codes! {
    /// `monitor-board-here` 没有别的码（这台不认得的那几条不进回包）。
    pub(crate) enum MonitorHereErr {}
}

/// `monitor-board-here`：这台照自己发出去的那张脸与运行簿答那几条。只读进程里那两本，不读盘。
pub(crate) struct MonitorBoardHereCmd;

impl Command for MonitorBoardHereCmd {
    const NAME: &'static str = "monitor-board-here";
    type Req = MonitorHereReq;
    type Reply = MonitorHere;
    type Err = MonitorHereErr;
}

impl crate::wire::typed::Handle for MonitorBoardHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MonitorHereReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MonitorHere, MonitorHereErr>> + Send + 'static
    {
        async move { Ok(here(&req)) }
    }
}

/// [`MonitorBoardHereCmd`] 的本体（本机那一台在 `monitor-board` 里也经它答）。没起 watcher（一次性进程）⇒ 一条都不认得。
fn here(req: &MonitorHereReq) -> MonitorHere {
    let Some(book) = crate::observe::watcher::faces_here() else {
        return MonitorHere { rows: Vec::new() };
    };
    let runs = crate::observe::watcher::run_book();
    let running = |sid: &str| runs.as_ref().map_or(0, |r| r.running(sid));
    let book = book.lock().unwrap_or_else(|e| e.into_inner());
    b::rows_here(&req.sids, &book, &running)
}

/// 请求里的一条：在哪台 · 哪条会话。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitorAsk {
    pub(crate) machine: cmd_wire::Machine,
    pub(crate) sid: String,
}

/// `monitor-board` 的入参：这一窗此刻列着的那几条（照列的先后；本机逐字 `"<local>"`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitorBoardReq {
    pub(crate) sessions: Vec<MonitorAsk>,
}

codes! {
    /// `monitor-board` 自己没有别的码（哪台没答只进那一组的 `down`）。
    pub(crate) enum MonitorBoardErr {}
}

/// `monitor-board`：本机后端问到各台（本机同进程答，别的台经常驻问 `monitor-board-here`），合成一份再答。
pub(crate) struct MonitorBoardCmd;

impl Command for MonitorBoardCmd {
    const NAME: &'static str = "monitor-board";
    type Req = MonitorBoardReq;
    type Reply = MonitorBoard;
    type Err = MonitorBoardErr;
}

impl crate::wire::typed::Handle for MonitorBoardCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MonitorBoardReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MonitorBoard, MonitorBoardErr>> + Send + 'static
    {
        async move {
            let asked: Vec<(cmd_wire::Machine, String)> = req
                .sessions
                .into_iter()
                .map(|a| (a.machine, a.sid))
                .collect();
            let machines: Vec<cmd_wire::Machine> = asked.iter().map(|(m, _)| m.clone()).collect();
            // 每台收同一份（全部 sid）：哪条归哪台由合成那一步照请求认，那台不认得的不进它那一份。
            let ask = MonitorHereReq {
                sids: asked.iter().map(|(_, s)| s.clone()).collect(),
            };
            let local = std::future::ready(Ok(here(&ask)));
            let got = crate::dial::remote_ask::ask_each::<MonitorBoardHereCmd, _>(
                &machines,
                &ask,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map_err(MonitorBoardErr::BadArgs)?;
            let got = got.into_iter().map(|(m, r)| (m, r.ok())).collect();
            let called: BTreeMap<String, Called> = crate::control::machine_state::read()
                .into_iter()
                .map(|(k, s)| {
                    (
                        k,
                        Called {
                            name: s.name,
                            offline: s.offline_said,
                        },
                    )
                })
                .collect();
            Ok(b::merge(&asked, got, &table_order(), &called))
        }
    }
}

/// 机器表里各台的名字，照表的序（与 `control/machine_state.rs` 列台同一张表）。
fn table_order() -> Vec<String> {
    crate::control::machines::answer_read_in(crate::platform::paths::home_dir().as_deref())
        .hosts
        .iter()
        .map(|r| r.name().to_string())
        .collect()
}
