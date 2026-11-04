//! 帧面 `bus-list` 的宿主：名单的行是这台的收件箱（`control::cc_bus_wire::bus_list`），
//! 每条会话叫它什么 · 此刻有没有谁持着它问观测层那一处（control 不许引用 observe，接线就住这一层）。

use crate::control::cc_bus::Seen;
use crate::control::cc_bus_wire::{bus_list, BusList, BusReadErr};
use std::path::Path;

/// `bus-list`：各收件箱 ＋ 叫它什么 · 在不在 ＋ 待读多少。入参 · 回包 · 码住控制面（`control::cc_bus_wire`）；
/// 命令标记与处理器住这里：在不在问观测层那一处，控制面不引用观测面，而命令的 `Handle` 只能跟标记类型同包。
pub struct BusListCmd;
impl crate::wire::typed::Command for BusListCmd {
    const NAME: &'static str = "bus-list";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = BusList;
    type Err = BusReadErr;
}

/// 一条会话叫它什么 · 此刻有没有谁持着它（判活只这一处：`observe::accounts_query::session_holders`）。
/// 叫什么带上这台注解里改过的名（`annotations`）。这台找不到它的记录 ⇒ 两格都判不了。
pub(crate) fn seen_at(
    home: &Path,
    sid: &str,
    annotations: &crate::history::history_annotations::Table,
) -> Seen {
    match crate::agents::find_session(home, sid) {
        Ok(at) => Seen {
            title: Some(crate::history::listing::session_title_of(
                &at.file,
                annotations.get(sid).and_then(|e| e.custom_title.clone()),
            )),
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
    async fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<BusList, BusReadErr> {
        let home = crate::history::listing::agent_home();
        let annotations = match crate::history::history_annotations::load() {
            crate::history::history_annotations::Loaded::Read(t) => t,
            _ => Default::default(),
        };
        bus_list(&|sid| seen_at(&home, sid, &annotations))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/bus_list_face_tests.rs"]
mod tests;
