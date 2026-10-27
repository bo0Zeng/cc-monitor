//! 每一跳可观测：本机后端连接池里那几条 SSH 连接（拨号核心记的读数，`ssh_dial::pool::Pool::links`）归到机器表里的那一台，
//! 写成机器状态成品里的那几项（`MachineState.links`：地址 · 经没经代理 · 压没压 · 进出字节 · 写好的那一句）。
//! 一条连接写成哪几项、那一句怎么写在拨号核心（`ssh_dial::pool::rows_of`，手机那份成品同一处）；这里只管「归到哪台」。
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
        let items = ssh_dial::pool::rows_of(snap, &req);
        if !items.is_empty() {
            out.insert(h.name().to_string(), items);
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/dial/links_tests.rs"]
mod tests;
