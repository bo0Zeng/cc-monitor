//! 每一跳可观测：机器表里每一台「怎么连过去的」（跳板照配置 · 经代理照本机后端连接池里那几条连接的读数）
//! 与到它的每一条 SSH 连接那一句（拨号核心记的读数，`ssh_dial::pool::Pool::links`），进那台的机器状态成品
//! （`MachineState.route` 与复制详情末尾那一段）。那两样怎么写在拨号核心（`ssh_dial::pool::way_of`，手机那份成品同一处）；
//! 这里只管「归到哪台」。
//!
//! 归到哪台：池按拨号身份分族（`ssh_dial::pool::identity`），身份照机器表现算 —— 与开连接那一处同一个组法（[`super::dial_named`]），
//! 对上的进那台；对不上（机器表改过、旧身份的连接还没收掉 · 测试连接那种表外的）哪台都不进。
//! 住拨号层不住观测层：用它的是控制面（`control/machine_state.rs`），控制面不许引观测面（控制面那个包不依赖观测面）。
//!
//! 本进程起的中转去上游那一跳（读数与那一句住中转核心 `comms_outward::hop`）归本机那台（[`local_from`]），同 SSH 那几跳进复制详情末尾那一段。

use std::collections::BTreeMap;

use backend_relay as relay;
use ssh_dial::pool::{LinkFacts, Way};

use super::Host;

/// 机器名 → 到那台怎么连过去的（成品那两格）；本机那台的键是 [`cmd_wire::machine::LOCAL`]。
pub type Links = BTreeMap<String, Way>;

/// 此刻那张：机器表 ＋ 本进程那个池 ＋ 本进程起的中转（归本机，键 [`cmd_wire::machine::LOCAL`]）。
pub fn now(hosts: &[Host]) -> Links {
    let mut links = by_machine(hosts, &ssh_dial::pool::ssh().links());
    if let Some(way) = local_from(relay::hop_now()) {
        links.insert(cmd_wire::machine::LOCAL.to_string(), way);
    }
    links
}

/// 中转那一跳的读数 ⇒ 本机那台的那一段（**纯函数**）：一条连接那一句，不另起「怎么连过去的」。
pub(crate) fn local_from(facts: Option<relay::HopFacts>) -> Option<Way> {
    let line = facts?.line()?;
    Some(Way {
        route: None,
        hops: vec![line],
    })
}

/// 等本进程那个池的「成员动了」（放进一条 · 断了 · 没人用了）。
pub fn moved() -> tokio::sync::watch::Receiver<u64> {
    ssh_dial::pool::ssh().moved()
}

/// 池快照归到机器表里的每一台（**纯函数**）。直连又没有连接的那台不出现；组不成请求的那台（停用 · 表里不成立）没有身份，也不出现。
pub(crate) fn by_machine(hosts: &[Host], snap: &[(String, Vec<LinkFacts>)]) -> Links {
    let mut out = Links::new();
    for h in hosts {
        let Ok(req) = super::dial_named(hosts, h.name(), "capture", serde_json::json!({})) else {
            continue;
        };
        let way = ssh_dial::pool::way_of(snap, &req);
        if way != Way::default() {
            out.insert(h.name().to_string(), way);
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/dial/links_tests.rs"]
mod tests;
