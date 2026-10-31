//! 监控板（成品）：每台把问到的那几条会话写成行（[`rows_here`]：序 · 活不活 · 几个 agent 在跑），本机后端把各台合成一份（[`merge`]）。
//!
//! - 组序：本机在前，其余照机器表的序；机器表里已没有的那几台排最后、按键序。
//! - 组头：那台成品的名字 ＋ 几条。组内：按那台答的序，同序照请求先后（稳定）；那台不认得的排组尾，不丢。
//! - 一台没答 ⇒ 那一组照请求先后、带那台离线那一句，不数进活会话与在跑。
//! - 汇总那一句：几台 · 几条活会话 · 几个 agent 在跑；一组都没有 ⇒ 空。

use crate::common::cells::Words;
use cmd_wire::board::{MonitorBoard, MonitorGroup, MonitorHere, MonitorHereRow};
use cmd_wire::frame::Life;
use cmd_wire::Machine;
use copy_core::copy_text;
use resident_client::book::SessionBook;
use std::collections::BTreeMap;

/// 问到的那几条里这台认得的写成行（照请求先后）：序与活不活照这台发出去的那张脸，`running` 照这台运行簿。
pub(crate) fn rows_here(
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
pub(crate) struct Called {
    pub(crate) name: String,
    pub(crate) offline: String,
}

/// 各台的结局合成一份。`asked` ＝ 请求里那几条（台 · sid，照界面列的先后）；`got` ＝ 每台答的那份（没答 ⇒ `None`）；
/// `table` ＝ 机器表里各台的名字（照表的序）；`called` ＝ 各台成品里的名字与离线那一句（键 ＝ 台）。
pub(crate) fn merge(
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
                None => (key.clone(), comms_inward::chan::said::offline(&key)),
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
#[path = "../../../tests/backend/control/monitor_board_tests.rs"]
mod tests;
