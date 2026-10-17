//! 每台远端机器的状态（桌面这一端）：按名字一张表。写点三处是本机后端自己的（部署每一步 · 部署收了 · 机器表改了），
//! 第四处是壳原样交来的接常驻那一轮（`machine-round`）。判定在 `deploy::machine`；每变一次推 `changed {machine, key: 名字, body: 成品}`。
//! 后端重启表就空：壳在本机通道接上那一刻把每台最新那一轮重交一遍。

use cmd_wire::machine::{MachineState, Round};
use deploy::machine::{fold, product, Entry, Fact};
use std::collections::BTreeMap;
use std::sync::Mutex;

static TABLE: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

/// 进程里那条「某台的成品变了」的通道（流连接订它推 `changed {machine}`）：那台的名字 ＋ 成品。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<(String, MachineState)> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<(String, MachineState)>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<(String, MachineState)>(64).0)
}

/// 机器表此刻那一张：名字 → 连不连。
fn hosts() -> Vec<(String, bool)> {
    crate::control::machines::answer_read_in(crate::platform::paths::home_dir().as_deref())
        .hosts
        .iter()
        .map(|h| (h.name().to_string(), h.connect))
        .collect()
}

fn enabled_in(hosts: &[(String, bool)], machine: &str) -> bool {
    hosts
        .iter()
        .find(|(n, _)| n == machine)
        .is_none_or(|(_, c)| *c)
}

/// 收一件事实、推一帧。
pub(crate) fn note(machine: &str, fact: Fact) -> MachineState {
    note_in(&hosts(), machine, fact)
}

/// [`note`] 的本体（机器表由调用方给）。
pub(crate) fn note_in(hosts: &[(String, bool)], machine: &str, fact: Fact) -> MachineState {
    let enabled = enabled_in(hosts, machine);
    let p = {
        let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
        let e = t.entry(machine.to_string()).or_default();
        fold(e, fact);
        product(e, machine, enabled, true)
    };
    let _ = changes().send((machine.to_string(), p.clone()));
    p
}

/// 机器表改了（`machines::ring`）：表里没了的那几台作废；每台照「连接这台」现推一帧（停用 ⇄ 恢复）。
pub(crate) fn table_moved() {
    table_moved_in(&hosts());
}

/// [`table_moved`] 的本体（机器表由调用方给）。
pub(crate) fn table_moved_in(hosts: &[(String, bool)]) {
    let products: Vec<(String, MachineState)> = {
        let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
        t.retain(|m, _| hosts.iter().any(|(n, _)| n == m));
        hosts
            .iter()
            .map(|(n, c)| {
                (
                    n.clone(),
                    product(t.get(n).unwrap_or(&Entry::default()), n, *c, true),
                )
            })
            .collect()
    };
    for p in products {
        let _ = changes().send(p);
    }
}

/// 机器表里每台此刻的成品（`machine-read`）。
pub(crate) fn read() -> BTreeMap<String, MachineState> {
    read_in(&hosts())
}

/// [`read`] 的本体（机器表由调用方给）。
pub(crate) fn read_in(hosts: &[(String, bool)]) -> BTreeMap<String, MachineState> {
    let t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    hosts
        .iter()
        .map(|(n, c)| {
            (
                n.clone(),
                product(t.get(n).unwrap_or(&Entry::default()), n, *c, true),
            )
        })
        .collect()
}

/// `machine-round` 的入参：那台 · 接常驻那一段的一轮结论（通信层交出的那一态原样）· 本侧的产品版本号。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineRoundReq {
    pub(crate) machine: String,
    pub(crate) round: Round,
    pub(crate) version: String,
}

crate::wire::typed::codes! {
    /// `machine-round` 的码：`machine` 是空串。
    pub(crate) enum MachineRoundErr {}
}

/// `machine-round`：壳交来接常驻那一轮的结论，回那台此刻的成品。
pub(crate) struct MachineRoundCmd;

impl crate::wire::typed::Command for MachineRoundCmd {
    const NAME: &'static str = "machine-round";
    type Req = MachineRoundReq;
    type Reply = MachineState;
    type Err = MachineRoundErr;
}

impl crate::wire::typed::Handle for MachineRoundCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MachineRoundReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineState, MachineRoundErr>> + Send + 'static
    {
        async move {
            if req.machine.trim().is_empty() {
                return Err(MachineRoundErr::BadArgs(
                    crate::common::contract::malformed("missing `machine` (non-empty string)"),
                ));
            }
            Ok(note(
                &req.machine,
                Fact::Round {
                    round: req.round,
                    version: Some(req.version),
                },
            ))
        }
    }
}

/// `machine-read` 的回包：机器表里每台 → 成品。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct MachineRead {
    pub(crate) machines: BTreeMap<String, MachineState>,
}

crate::wire::typed::codes! {
    /// `machine-read` 没有别的码。
    pub(crate) enum MachineReadErr {}
}

/// `machine-read`：机器表里每台此刻的成品（主题 `machine` 的重问）。
pub(crate) struct MachineReadCmd;

impl crate::wire::typed::Command for MachineReadCmd {
    const NAME: &'static str = "machine-read";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = MachineRead;
    type Err = MachineReadErr;
}

impl crate::wire::typed::Handle for MachineReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<MachineRead, MachineReadErr>> + Send + 'static
    {
        async move { Ok(MachineRead { machines: read() }) }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_state_tests.rs"]
mod tests;
