//! 每台机器的状态（桌面这一端）：按名字一张表。写点四处都是本机后端自己的：部署每一步 · 部署收了 · 机器表改了 ·
//! 那台的常驻流这一轮怎么了（`control/feeds.rs`，循环交的那一态原样）。判定在 `deploy::machine`；每变一次推 `changed {machine, key: 名字, body: 成品}`。
//! 本机（键 `<local>`）也是其中一台：这个后端在答就是连着、与手上同一份字节，那一行由这里出（[`local_row`]）。
//! 后端重启表就空：各台的常驻流随后端一起重起，各自重报。

use cmd_wire::machine::{MachineState, Round};
use deploy::machine::{fold, product, Entry, Fact};
use std::collections::BTreeMap;
use std::sync::Mutex;

static TABLE: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

/// 那张表（取的头一回，带特性 `shots` 编的后端先收下台架交的部署事实：[`shots_deploy`]）。
fn table() -> std::sync::MutexGuard<'static, BTreeMap<String, Entry>> {
    #[allow(unused_mut)]
    let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(feature = "shots")]
    shots_deploy::seed(&mut t);
    #[cfg(feature = "shots")]
    shots_rounds::seed(&mut t);
    t
}

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
        let mut t = table();
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
        let mut t = table();
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

/// [`read`] 的本体（机器表由调用方给）：表里每台，外加本机那一行。
pub(crate) fn read_in(hosts: &[(String, bool)]) -> BTreeMap<String, MachineState> {
    let t = table();
    let mut out: BTreeMap<String, MachineState> = hosts
        .iter()
        .map(|(n, c)| {
            (
                n.clone(),
                product(t.get(n).unwrap_or(&Entry::default()), n, *c, true),
            )
        })
        .collect();
    out.insert(cmd_wire::machine::LOCAL.to_string(), local_row());
    out
}

/// 本机那一行：问得到这个后端 ⇒ 连着、与手上同一份字节；版本号是这份二进制出自的那一版（`build.rs` 交的）。
fn local_row() -> MachineState {
    let mut e = Entry::default();
    fold(
        &mut e,
        Fact::Round {
            round: Round::Up {
                relation: cmd_wire::attach::Relation::Same,
                differs: Vec::new(),
            },
            version: Some(env!("CC_MONITOR_VERSION").to_string()),
        },
    );
    product(&e, &copy_core::local_machine(), true, true)
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

/// 截图台架的部署事实：台架没有 ssh 那条路、部署不真跑，每台走到哪一步 · 那台是什么系统由台架起进程时交
/// （环境变量 `CCM_SHOTS_DEPLOY`：`{"<名字>": {"os"?: "Linux", "step"?: "plan" | "install" | "update" | "restart"}}`），
/// 表头一回被取时照部署那一侧的两件事实收下（[`Fact::Deployed`] · [`Fact::Step`]）。默认构建里这一段不编进去。
#[cfg(feature = "shots")]
mod shots_deploy {
    use super::{fold, Entry, Fact};
    use deploy::Step;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Given {
        os: Option<String>,
        step: Option<StepGiven>,
    }

    #[derive(Clone, Copy, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum StepGiven {
        Plan,
        Install,
        Update,
        Restart,
    }

    static DONE: AtomicBool = AtomicBool::new(false);

    pub(super) fn seed(t: &mut BTreeMap<String, Entry>) {
        if DONE.swap(true, Ordering::SeqCst) {
            return;
        }
        let Ok(raw) = std::env::var("CCM_SHOTS_DEPLOY") else {
            return;
        };
        let given: BTreeMap<String, Given> = match serde_json::from_str(&raw) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("CCM_SHOTS_DEPLOY 认不出：{e}");
                return;
            }
        };
        for (machine, g) in given {
            let e = t.entry(machine).or_default();
            fold(e, Fact::Deployed { os: g.os });
            if let Some(s) = g.step {
                fold(
                    e,
                    Fact::Step(match s {
                        StepGiven::Plan => Step::Plan,
                        StepGiven::Install => Step::Place { updating: false },
                        StepGiven::Update => Step::Place { updating: true },
                        StepGiven::Restart => Step::Restart,
                    }),
                );
            }
        }
    }
}

/// 截图台架那几台这一轮怎么了：台架的远端各台不由本后端自己那几条常驻流接（带 `shots` 特性编的后端不起它们，`control/feeds.rs::start`），
/// 每台那一轮的结论由台架起进程时交（环境变量 `CCM_SHOTS_ROUNDS`：`{"<名字>": <Round>}`，同常驻流交的那一形），
/// 表头一回被取时收下（版本号照常驻流那一处：这份二进制的）。默认构建里这一段不编进去。
#[cfg(feature = "shots")]
mod shots_rounds {
    use super::{fold, Entry, Fact, Round};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    static DONE: AtomicBool = AtomicBool::new(false);

    pub(super) fn seed(t: &mut BTreeMap<String, Entry>) {
        if DONE.swap(true, Ordering::SeqCst) {
            return;
        }
        let Ok(raw) = std::env::var("CCM_SHOTS_ROUNDS") else {
            return;
        };
        let given: BTreeMap<String, Round> = match serde_json::from_str(&raw) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("CCM_SHOTS_ROUNDS 认不出：{e}");
                return;
            }
        };
        for (machine, round) in given {
            fold(
                t.entry(machine).or_default(),
                Fact::Round {
                    round,
                    version: Some(env!("CC_MONITOR_VERSION").to_string()),
                },
            );
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_state_tests.rs"]
mod tests;
