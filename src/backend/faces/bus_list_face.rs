//! 帧面 `bus-list` 的宿主：名单的行问 cc-bus（`control::cc_bus_wire::bus_list`，转调 `cc-bus list --tsv`），
//! 每条会话叫它什么 · 此刻有没有谁持着它问观测层那一处（control 不许引用 observe，接线就住这一层）。

use crate::control::cc_bus::Seen;
use crate::control::cc_bus_wire::{bus_list, BusList, BusListCmd, BusReadErr};
use std::path::Path;

/// 一条会话叫它什么 · 此刻有没有谁持着它（判活只这一处：`observe::accounts_query::session_holders`）。
/// 这台找不到它的记录 ⇒ 两格都判不了。
pub(crate) fn seen_at(home: &Path, sid: &str) -> Seen {
    match crate::agents::find_session(home, sid) {
        Ok(at) => Seen {
            title: Some(crate::history::listing::session_title_of(&at.file)),
            live: crate::observe::accounts_query::session_holders(home, &at.file)
                .map(|holders| !holders.is_empty()),
        },
        Err(_) => Seen {
            title: None,
            live: None,
        },
    }
}

impl crate::wire::typed::Handle for BusListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusList, BusReadErr>> + Send + 'static {
        async move {
            let home = crate::history::listing::agent_home();
            bus_list(&|sid| seen_at(&home, sid))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/bus_list_face_tests.rs"]
mod tests;
