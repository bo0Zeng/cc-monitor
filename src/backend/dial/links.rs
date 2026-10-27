//! 每一跳可观测：本机后端连接池里那几条 SSH 连接（拨号核心记的读数，`ssh_dial::pool::Pool::links`）归到机器表里的那一台，
//! 写成机器状态成品里的那几项（`MachineState.links`：地址 · 压没压 · 进出字节 · 写好的那一句）。
//!
//! 归到哪台：池按拨号身份分族（`ssh_dial::pool::identity`），身份照机器表现算 —— 与开连接那一处同一个组法（[`super::dial_named`]），
//! 对上的进那台；对不上（机器表改过、旧身份的连接还没收掉 · 测试连接那种表外的）哪台都不进。
//! 住拨号层不住观测层：用它的是控制面（`control/machine_state.rs`），控制面不许引观测面（`layering_guard`）。

use std::collections::BTreeMap;

use cmd_wire::machine::MachineLink;
use ssh_dial::pool::LinkFacts;

use super::Host;

/// 机器名 → 到那台的每一条连接（成品那一格）。
pub(crate) type Links = BTreeMap<String, Vec<MachineLink>>;

/// 此刻那张：机器表 ＋ 本进程那个池。
pub(crate) fn now(hosts: &[Host]) -> Links {
    by_machine(hosts, &ssh_dial::pool::ssh().links())
}

/// 等本进程那个池的「成员动了」（放进一条 · 断了 · 没人用了）。
pub(crate) fn moved() -> tokio::sync::watch::Receiver<u64> {
    ssh_dial::pool::ssh().moved()
}

/// 池快照归到机器表里的每一台（**纯函数**）。没有连接的那台不出现；组不成请求的那台（停用 · 表里不成立）没有身份，也不出现。
pub(crate) fn by_machine(hosts: &[Host], snap: &[(String, Vec<LinkFacts>)]) -> Links {
    let mut out = Links::new();
    for h in hosts {
        let Ok(req) = super::dial_named(hosts, h.name(), "capture", serde_json::json!({})) else {
            continue;
        };
        let key = ssh_dial::pool::identity(&req);
        let Some((_, links)) = snap.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        let items: Vec<MachineLink> = links.iter().flat_map(rows).collect();
        if !items.is_empty() {
            out.insert(h.name().to_string(), items);
        }
    }
    out
}

/// 一条连接 ⇒ 成品里的几项：它自己在前，经过的跳板跟在后面（外在先）。
pub(crate) fn rows(f: &LinkFacts) -> Vec<MachineLink> {
    std::iter::once(item(&f.link, false))
        .chain(f.jumps.iter().map(|w| item(w, true)))
        .collect()
}

fn item(w: &ssh_dial::pool::Wire, jump: bool) -> MachineLink {
    let packing = copy_core::copy_text(
        if w.compressed {
            "beLinks.compressed"
        } else {
            "beLinks.plain"
        },
        &[],
    );
    let line = copy_core::copy_text(
        "beLinks.line",
        &[
            ("endpoint", &w.endpoint),
            ("packing", &packing),
            ("in", &copy_core::size_text(w.bytes_in)),
            ("out", &copy_core::size_text(w.bytes_out)),
        ],
    );
    MachineLink {
        endpoint: w.endpoint.clone(),
        jump,
        compressed: w.compressed,
        bytes_in: w.bytes_in,
        bytes_out: w.bytes_out,
        said: if jump {
            copy_core::copy_text("beLinks.jumpLine", &[("line", &line)])
        } else {
            line
        },
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial/links_tests.rs"]
mod tests;
