//! **agent 自问**：`ccm -- --session-self` 答「我是哪条会话 · 在哪几组 · 组员有谁」。
//!
//! - 认会话只照调用进程往上的进程链：链上第一个在这台活会话表里的进程（pidfile 的 pid、此刻还是写它的那一个，挡 pid 复用）就是那条会话。
//!   不收 agent 自报的 id（参数 · 环境变量都不认）。链上一个都没有 ⇒ `not_in_session`。
//! - 组照 `groups-read` 那一份（并几台 · 判定 · 计数都在那里）：先转给这台的常驻后端问到各台（一次性进程里可达表是空的）；
//!   常驻不在 ⇒ 只问这台、照实标 [`Looked::ThisMachine`]；两边都没答成 ⇒ [`Looked::Nothing`]，组一个不编。
//!   转给常驻 · 读机器表是控制面的事：那一段接线住命令表（`stream/inbound/registry/history.rs`），历史这一层不往控制面里引。
//! - 只留有「这台这一条」的那几组，组员整组带上（别的台的组员也在）。

use serde::Serialize;

use super::groups::{GroupRead, GroupsRead};
use super::live_table::LiveRow;
use crate::wire::typed::codes;

/// 往上最多找几层（进程树再深也到不了这么多；挡住读坏的父进程号绕圈）。
pub(crate) const MAX_HOPS: usize = 64;

/// 从 `start` 往上的进程链（含它自己）：到顶（没有父进程 · 父进程号是 0）· 绕回 · 满 [`MAX_HOPS`] 都停。
pub(crate) fn chain_from(start: u32, parent: impl Fn(u32) -> Option<u32>) -> Vec<u32> {
    let mut out = vec![start];
    while out.len() < MAX_HOPS {
        let Some(p) = parent(*out.last().unwrap_or(&start)).filter(|p| *p != 0) else {
            break;
        };
        if out.contains(&p) {
            break;
        }
        out.push(p);
    }
    out
}

/// 链上第一个在活会话表里、此刻还是写那份 pidfile 的那一个进程 ⇒ 它说的会话 id。
pub(crate) fn first_session(chain: &[u32], rows: &[LiveRow]) -> Option<String> {
    chain.iter().find_map(|pid| {
        rows.iter()
            .find(|r| r.pid == *pid && r.alive)
            .and_then(|r| r.sid())
            .map(str::to_string)
    })
}

/// 从 `start` 往上（真读父进程号）找 `agent_home` 那张活会话表里的会话。
pub fn who_at(agent_home: &std::path::Path, start: u32) -> Option<String> {
    let chain = chain_from(start, crate::platform::proc::parent_of);
    first_session(
        &chain,
        super::live_table::LiveTable::scan(agent_home).rows(),
    )
}

/// 只留有这台这一条（`sid`）的那几组。
pub(crate) fn mine(read: GroupsRead, sid: &str) -> Vec<GroupRead> {
    read.groups
        .into_iter()
        .filter(|g| {
            g.members
                .iter()
                .any(|m| m.origin.is_local() && m.sid == sid)
        })
        .collect()
}

/// 这一问看了哪几台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Looked {
    /// 常驻答的：问到了各台（哪台没答在各组的 `known` 里）。
    Every,
    /// 常驻不在：只看了这台。
    ThisMachine,
    /// 两边都没答成：组一个都没看成。
    Nothing,
}

/// `session-self` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionSelf {
    /// 我是哪条会话（这台的）。
    pub(crate) sid: String,
    /// 我在的那几组（组员整组）。
    pub(crate) groups: Vec<GroupRead>,
    pub(crate) looked: Looked,
}

/// `session-self` 的后一半（命令表那一处接上常驻）：先问常驻（各台），没答成再问这台。
pub async fn answer(
    sid: String,
    via_resident: impl std::future::Future<Output = Result<GroupsRead, String>>,
    here: impl std::future::Future<Output = Result<GroupsRead, String>>,
) -> SessionSelf {
    let (groups, looked) = match via_resident.await {
        Ok(r) => (mine(r, &sid), Looked::Every),
        Err(why) => {
            tracing::info!("session-self：常驻没答（{why}），只看这台");
            match here.await {
                Ok(r) => (mine(r, &sid), Looked::ThisMachine),
                Err(why) => {
                    tracing::warn!("session-self：这台的组也没读成：{why}");
                    (Vec::new(), Looked::Nothing)
                }
            }
        }
    };
    SessionSelf {
        sid,
        groups,
        looked,
    }
}

codes! {
    /// `session-self` 的码。
    pub enum SessionSelfErr {
        /// 调用进程往上找不到这台的哪一条会话。
        NotInSession = "not_in_session",
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/session_self_tests.rs"]
mod tests;
