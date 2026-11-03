//! 每台机器的状态（桌面这一端）：按名字一张表。写点四处都是本机后端自己的：部署每一步 · 部署收了 · 机器表改了 ·
//! 那台的常驻流这一轮怎么了（`control/feeds.rs`，循环交的那一态原样）。判定在 `deploy::machine`；每变一次推 `changed {machine, key: 名字, body: 成品}`。
//! 本机（键 `<local>`）也是其中一台：这个后端在答就是连着、与手上同一份字节，那一行由这里出（[`local_row`]）。
//! 后端重启表就空：各台的常驻流随后端一起重起，各自重报。
//!
//! 成品里到那台怎么连过去的（`route`）与每条 SSH 连接那一句（复制详情末尾）不是这一轮的事实，出成品时照机器表与本进程的连接池现填
//! （[`crate::dial::links`]）；池里成员动了（放进一条 · 断了 · 没人用了）⇒ 每台重推一份（[`follow_pool`]，零定时器）。

use crate::dial::links::Links;
use cmd_wire::machine::{FleetLines, MachineRead, MachineReadReq, MachineState, Round};
use copy_core::{Discard, Why};
use deploy::machine::{fold, product, Entry, Fact};
use std::collections::BTreeMap;
use std::sync::Mutex;

static TABLE: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

/// 那张表。
fn table() -> std::sync::MutexGuard<'static, BTreeMap<String, Entry>> {
    TABLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// 进程里那条「某台的成品变了」的通道（流连接订它推 `changed {machine}`）：那台的名字 ＋ 成品。
pub fn changes() -> &'static tokio::sync::broadcast::Sender<(String, MachineState)> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<(String, MachineState)>> =
        std::sync::OnceLock::new();
    follow_pool();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<(String, MachineState)>(64).0)
}

/// 池里成员动了 ⇒ 每台照此刻的连接重推一份。一个进程起一次：头一回在异步运行时里要这条通道的那一下起
/// （流连接订它、各台的常驻流推它，都在运行时里）；不在运行时里 ⇒ 这一下不起，下一回再看。
fn follow_pool() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Ok(rt) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    rt.spawn(follow(crate::dial::links::moved(), table_moved));
}

/// 每记一笔「成员动了」就推一遍（`push`）；池没了（发端收了）⇒ 收工。
async fn follow(mut moved: tokio::sync::watch::Receiver<u64>, push: fn()) {
    while moved.changed().await.is_ok() {
        push();
    }
}

/// 机器表此刻那一张：名字 → 连不连；连同到每台的连接（照同一张表归）。
fn hosts() -> (Vec<(String, bool)>, Links) {
    let rows =
        crate::control::machines::answer_read_in(crate::platform::paths::home_dir().as_deref())
            .hosts;
    let table: Vec<crate::dial::Host> = rows.iter().map(|r| (**r).clone()).collect();
    let names = table
        .iter()
        .map(|h| (h.name().to_string(), h.connect))
        .collect();
    (names, crate::dial::links::now(&table))
}

fn enabled_in(hosts: &[(String, bool)], machine: &str) -> bool {
    hosts
        .iter()
        .find(|(n, _)| n == machine)
        .is_none_or(|(_, c)| *c)
}

/// 一台的成品：判定那一份 ＋ 此刻到它的连接。
fn product_in(e: &Entry, machine: &str, enabled: bool, links: &Links) -> MachineState {
    let mut p = product(e, machine, enabled, true);
    if let Some(way) = links.get(machine) {
        way.put(&mut p);
    }
    p
}

/// 收一件事实、推一帧。
pub(crate) fn note(machine: &str, fact: Fact) -> MachineState {
    let (hosts, links) = hosts();
    note_in(&hosts, &links, machine, fact)
}

/// [`note`] 的本体（机器表与连接由调用方给）。
pub(crate) fn note_in(
    hosts: &[(String, bool)],
    links: &Links,
    machine: &str,
    fact: Fact,
) -> MachineState {
    let enabled = enabled_in(hosts, machine);
    let p = {
        let mut t = table();
        let e = t.entry(machine.to_string()).or_default();
        fold(e, fact);
        product_in(e, machine, enabled, links)
    };
    // 此刻没有流连接订它 ⇒ 发不出是正常的：表在进程里，客户端重问 `machine-read` 就有
    changes()
        .send((machine.to_string(), p.clone()))
        .discard(Why::NotAnError);
    p
}

/// 机器表改了（`machines::ring`）· 池里的连接动了（[`follow_pool`]）：表里没了的那几台作废；
/// 每台照「连接这台」与此刻的连接现推一帧。
pub(crate) fn table_moved() {
    let (hosts, links) = hosts();
    table_moved_in(&hosts, &links);
}

/// [`table_moved`] 的本体（机器表与连接由调用方给）。
pub(crate) fn table_moved_in(hosts: &[(String, bool)], links: &Links) {
    let products: Vec<(String, MachineState)> = {
        let mut t = table();
        t.retain(|m, _| hosts.iter().any(|(n, _)| n == m));
        hosts
            .iter()
            .map(|(n, c)| {
                (
                    n.clone(),
                    product_in(t.get(n).unwrap_or(&Entry::default()), n, *c, links),
                )
            })
            .collect()
    };
    for p in products {
        changes().send(p).discard(Why::NotAnError); // 没人订就发不出，同上
    }
}

/// 机器表里每台此刻的成品（`machine-read`）。
pub fn read() -> BTreeMap<String, MachineState> {
    let (hosts, links) = hosts();
    read_in(&hosts, &links)
}

/// [`read`] 的本体（机器表与连接由调用方给）：表里每台，外加本机那一行。
pub(crate) fn read_in(hosts: &[(String, bool)], links: &Links) -> BTreeMap<String, MachineState> {
    let t = table();
    let mut out: BTreeMap<String, MachineState> = hosts
        .iter()
        .map(|(n, c)| {
            (
                n.clone(),
                product_in(t.get(n).unwrap_or(&Entry::default()), n, *c, links),
            )
        })
        .collect();
    out.insert(cmd_wire::machine::LOCAL.to_string(), local_row());
    out
}

/// 此刻各台合一次写好的那几句（[`deploy::machine::fleet`]）：本机那一行（这个后端在答 ⇒ 报完了）＋ 机器表里每台（停用的也在）
/// 与它这一条连接上报完了没有。
pub fn fleet() -> FleetLines {
    let (hosts, links) = hosts();
    let t = table().clone();
    fleet_over(&t, &hosts, &links)
}

/// [`fleet`] 照给的那一张表算（不碰进程里那一张）。
fn fleet_over(t: &BTreeMap<String, Entry>, hosts: &[(String, bool)], links: &Links) -> FleetLines {
    let local = local_row();
    let rows: Vec<(MachineState, bool)> = hosts
        .iter()
        .map(|(n, c)| {
            let e = t.get(n).cloned().unwrap_or_default();
            (product_in(&e, n, *c, links), e.listed())
        })
        .collect();
    let mut all: Vec<(&MachineState, bool)> = vec![(&local, true)];
    all.extend(rows.iter().map(|(m, l)| (m, *l)));
    deploy::machine::fleet(&all)
}

/// 本机那一行：问得到这个后端 ⇒ 连着、与手上同一份字节；版本号是这份二进制出自的那一版（`common::id::VERSION`）。
/// 不经 SSH ⇒ 没有连接那一格。
fn local_row() -> MachineState {
    let mut e = Entry::default();
    fold(
        &mut e,
        Fact::Round {
            round: Round::Up {
                relation: cmd_wire::attach::Relation::Same,
                differs: Vec::new(),
            },
            version: Some(crate::common::id::VERSION.to_string()),
        },
    );
    product(&e, &copy_core::local_machine(), true, true)
}

crate::wire::typed::codes! {
    /// `machine-read` 没有别的码。
    pub enum MachineReadErr {}
}

/// `machine-read`：机器表里每台此刻的成品（主题 `machine` 的重问）。
pub struct MachineReadCmd;

impl crate::wire::typed::Command for MachineReadCmd {
    crate::wire::typed::by_call!(MachineReadReq);
    const TAKES_INPUT: bool = false;
    type Err = MachineReadErr;
}

impl crate::wire::typed::Handle for MachineReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        _req: MachineReadReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<MachineRead, MachineReadErr> {
        Ok(MachineRead {
            machines: read(),
            empty: fleet(),
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_state_tests.rs"]
mod tests;
