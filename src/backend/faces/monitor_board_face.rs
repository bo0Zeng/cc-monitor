//! 监控板那两条（typed）与它的成品：`monitor-board-here`（这台把问到的那几条会话写成行：序 · 活不活 · 几个 agent 在跑，[`rows_here`]）·
//! `monitor-board`（本机后端问到各台、合成一份，[`merge`]）。
//!
//! - 组序：本机在前，其余照机器表的序；机器表里已没有的那几台排最后、按键序。
//! - 组头：那台成品的名字 ＋ 几条。组内：按那台答的序，同序照请求先后（稳定）；那台不认得的排组尾，不丢。
//! - 一台没答 ⇒ 那一组照请求先后、带那台离线那一句，不数进活会话与在跑。
//! - 汇总那一句：几台 · 几条活会话 · 几个 agent 在跑；一组都没有 ⇒ 空。

use crate::common::cells::Words;
use crate::wire::typed::{codes, Command};
use cmd_wire::board::{MonitorBoard, MonitorGroup, MonitorHere, MonitorHereRow};
use cmd_wire::frame::Life;
use cmd_wire::Machine;
use copy_core::copy_text;
use face_core::book::SessionBook;
use std::collections::BTreeMap;

/// `monitor-board-here` 的入参：问的那几条会话。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct MonitorHereReq {
    pub(crate) sids: Vec<String>,
}

codes! {
    /// `monitor-board-here` 没有别的码（这台不认得的那几条不进回包）。
    pub enum MonitorHereErr {}
}

/// `monitor-board-here`：这台照自己发出去的那张脸与运行簿答那几条。只读进程里那两本，不读盘。
pub struct MonitorBoardHereCmd;

impl Command for MonitorBoardHereCmd {
    const NAME: &'static str = "monitor-board-here";
    type Req = MonitorHereReq;
    type Reply = MonitorHere;
    type Err = MonitorHereErr;
}

impl crate::wire::typed::Handle for MonitorBoardHereCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: MonitorHereReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<MonitorHere, MonitorHereErr> {
        Ok(here(&req))
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
    rows_here(&req.sids, book.book(), &running)
}

/// 请求里的一条：在哪台 · 哪条会话。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitorAsk {
    pub(crate) machine: cmd_wire::Machine,
    pub(crate) sid: String,
}

/// `monitor-board` 的入参：这一窗此刻列着的那几条（照列的先后；本机逐字 `"<local>"`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct MonitorBoardReq {
    pub(crate) sessions: Vec<MonitorAsk>,
}

codes! {
    /// `monitor-board` 自己没有别的码（哪台没答只进那一组的 `down`）。
    pub enum MonitorBoardErr {}
}

/// `monitor-board`：本机后端问到各台（本机同进程答，别的台经常驻问 `monitor-board-here`），合成一份再答。
pub struct MonitorBoardCmd;

impl Command for MonitorBoardCmd {
    const NAME: &'static str = "monitor-board";
    type Req = MonitorBoardReq;
    type Reply = MonitorBoard;
    type Err = MonitorBoardErr;
}

impl crate::wire::typed::Handle for MonitorBoardCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: MonitorBoardReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<MonitorBoard, MonitorBoardErr> {
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
        Ok(merge(&asked, got, &table_order(), &called))
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

/// 问到的那几条里这台认得的写成行（照请求先后）：序与活不活照这台发出去的那张脸，`running` 照这台运行簿。
pub fn rows_here(
    sids: &[String],
    book: &SessionBook,
    running: &dyn Fn(&str) -> usize,
) -> MonitorHere {
    let rows = sids
        .iter()
        .filter_map(|sid| {
            let face = book.face(sid)?;
            Some(MonitorHereRow {
                sid: sid.clone(),
                order: face.order,
                live: matches!(face.life, Life::Live { .. }),
                running: u32::try_from(running(sid)).unwrap_or(u32::MAX),
            })
        })
        .collect();
    MonitorHere { rows }
}

/// 一台叫什么 · 离线时那一句（取自那台的机器成品：`name` · `offline_said`）。
pub struct Called {
    pub name: String,
    pub offline: String,
}

/// 各台的结局合成一份。`asked` ＝ 请求里那几条（台 · sid，照界面列的先后）；`got` ＝ 每台答的那份（没答 ⇒ `None`）；
/// `table` ＝ 机器表里各台的名字（照表的序）；`called` ＝ 各台成品里的名字与离线那一句（键 ＝ 台）。
pub fn merge(
    asked: &[(Machine, String)],
    got: Vec<(Machine, Option<MonitorHere>)>,
    table: &[String],
    called: &BTreeMap<String, Called>,
) -> MonitorBoard {
    let mut got: BTreeMap<String, Option<MonitorHere>> =
        got.into_iter().map(|(m, h)| (m.to_string(), h)).collect();
    let mut machines: Vec<&Machine> = Vec::new();
    for (m, _) in asked {
        if !machines.contains(&m) {
            machines.push(m);
        }
    }
    // 本机 0 · 机器表里第 i 台 1+i · 表里没有的排最后（同档按键序）。
    let rank = |m: &Machine| -> (usize, String) {
        let key = m.to_string();
        if m.is_local() {
            return (0, key);
        }
        match table.iter().position(|n| *n == key) {
            Some(i) => (1 + i, key),
            None => (usize::MAX, key),
        }
    };
    machines.sort_by_key(|m| rank(m));
    let (mut live, mut running) = (0usize, 0usize);
    let groups: Vec<MonitorGroup> = machines
        .into_iter()
        .map(|machine| {
            let key = machine.to_string();
            let wanted: Vec<&str> = asked
                .iter()
                .filter(|(m, _)| m == machine)
                .map(|(_, s)| s.as_str())
                .collect();
            let (name, offline) = match called.get(&key) {
                Some(c) => (c.name.clone(), c.offline.clone()),
                None => (key.clone(), copy_core::offline(&key)),
            };
            let (sids, down): (Vec<String>, Option<Words>) = match got.remove(&key).flatten() {
                Some(here) => {
                    let mut seen: Vec<(u8, usize, &str)> = Vec::new();
                    for (i, sid) in wanted.iter().enumerate() {
                        let r = here.rows.iter().find(|r| r.sid == *sid);
                        if let Some(r) = r {
                            live += usize::from(r.live);
                            running += r.running as usize;
                        }
                        seen.push((r.map_or(u8::MAX, |r| r.order), i, sid));
                    }
                    seen.sort();
                    (
                        seen.into_iter().map(|(_, _, s)| s.to_string()).collect(),
                        None,
                    )
                }
                None => (
                    wanted.iter().map(|s| s.to_string()).collect(),
                    Some(Words(offline)),
                ),
            };
            MonitorGroup {
                title: Words(copy_text(
                    "monitorBoard.group.title",
                    &[("machine", &name), ("n", &sids.len().to_string())],
                )),
                machine: machine.clone(),
                sids,
                down,
            }
        })
        .collect();
    let summary = if groups.is_empty() {
        Words::default()
    } else {
        Words(copy_text(
            "monitorBoard.head.summary",
            &[
                ("machines", &groups.len().to_string()),
                ("liveSessions", &live.to_string()),
                ("runningAgents", &running.to_string()),
            ],
        ))
    };
    MonitorBoard { summary, groups }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/monitor_board_face_tests.rs"]
mod tests;
