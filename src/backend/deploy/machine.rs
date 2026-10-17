//! **每台机器的状态成品**：收事实（接常驻那一轮 · 部署走到哪一步 · 部署收了）成一态；名字旁那个词 · 问题行那一句 · 修法照
//! `deploy_contract::machine` 那几张表（本机那一行在壳里照同一份出）。
//! 桌面由本机后端跑（`control/machine_state.rs`），手机由 mobcore 跑同一份；两端照画。

use cmd_wire::attach::Relation;
use cmd_wire::machine::{
    Blocker, FixButton, MachineState, MachineStateKind as K, Round, RoundProblem, Stage, Why,
};
use copy_core::detail::{Detail, Label, Target};
use deploy_contract::machine::{fixes, label, said, word};

use crate::Step;

/// 一台手上的事实（[`fold`] 收、[`product`] 读）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    state: Option<K>,
    stage: Option<Stage>,
    relation: Option<Relation>,
    /// 与这一版同一份字节时那台的产品版本号。
    version: Option<String>,
    os: Option<String>,
    /// 这一轮没连上 / 做不了时那一句（原因码 · 那台出示的指纹 · 详情都在里面）。
    problem: Option<RoundProblem>,
    /// 这一轮断在那台本身的哪一处。
    blocker: Option<Blocker>,
}

/// 一件事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// 接常驻那一段的一轮结论；`version` 是本侧的产品版本号（那台与这一版同一份字节时就是它的号；报不出 ⇒ 空）。
    Round {
        round: Round,
        version: Option<String>,
    },
    /// 部署走到哪一步。
    Step(Step),
    /// 部署那一趟收了（成没成都算）；问到了那台的系统就带上。
    Deployed { os: Option<String> },
}

/// 收一件事实。
pub fn fold(e: &mut Entry, fact: Fact) {
    match fact {
        Fact::Round { round, version } => match round {
            Round::Connecting => busy(e, K::Connecting, Some(Stage::Attach)),
            Round::Up { relation, differs } => {
                e.state = Some(match relation {
                    Relation::Older => K::NeedsUpdate,
                    Relation::Same if !differs.is_empty() => K::NeedsUpdate,
                    Relation::Newer => K::Newer,
                    Relation::Same | Relation::Incomparable => K::Up,
                });
                e.stage = None;
                e.relation = Some(relation);
                e.version = version.filter(|_| relation == Relation::Same);
                e.problem = None;
                e.blocker = None;
            }
            Round::Down { problem, blocker } => {
                e.state = Some(match blocker {
                    Some(Blocker::NotUnix) => K::Unsupported,
                    // 不认这一版的 attach 行 ⇒ 那台旧（这一轮没比出新旧，上一轮的作废）。
                    Some(Blocker::OldBackend) => {
                        e.relation = None;
                        e.version = None;
                        K::NeedsUpdate
                    }
                    Some(Blocker::NoBackend) | None if problem.why == Some(Why::HostKey) => {
                        K::HostKeyChanged
                    }
                    Some(Blocker::NoBackend) | None => K::Down,
                });
                e.stage = None;
                e.problem = Some(problem);
                e.blocker = blocker;
            }
        },
        Fact::Step(s) => {
            let (state, stage) = s.kind();
            busy(e, state, stage);
        }
        Fact::Deployed { os } => {
            if os.is_some() {
                e.os = os;
            }
            // 部署收了、这一轮还没结论 ⇒ 接下来是接常驻那一步。
            if matches!(e.state, Some(K::Connecting | K::Installing | K::Updating)) {
                busy(e, K::Connecting, Some(Stage::Attach));
            }
        }
    }
}

fn busy(e: &mut Entry, state: K, stage: Option<Stage>) {
    e.state = Some(state);
    e.stage = stage;
    e.problem = None;
    e.blocker = None;
}

/// 读一台的成品。`machine` 是给人看的名字；`enabled = false` ⇒ 「连接这台」关着（停用）；
/// `installs` ⇒ 这一端自己会把后端装上去（桌面：下一轮自己装，不给［安装］；手机不装）。
pub fn product(e: &Entry, machine: &str, enabled: bool, installs: bool) -> MachineState {
    let state = match (enabled, e.state) {
        (false, _) => K::Disabled,
        (true, s) => s.unwrap_or(K::Unknown),
    };
    let why = match state {
        K::Down | K::HostKeyChanged => e.problem.as_ref().map(|p| p.why.unwrap_or(Why::Other)),
        _ => None,
    };
    let problem = e.problem.as_ref().filter(|_| {
        matches!(
            state,
            K::Down | K::HostKeyChanged | K::Unsupported | K::NeedsUpdate
        )
    });
    let blocker = problem.and(e.blocker);
    MachineState {
        state,
        reason: why,
        stage: if state == K::Connecting {
            e.stage
        } else {
            None
        },
        version: e.version.clone(),
        version_relation: e.relation,
        os: e.os.clone(),
        word: word(state, e.relation),
        said: match (state, blocker, problem) {
            // 没有后端：是哪一种（没装 · 起不了 · 不是我们的）只有通信层那一句说得清。
            (K::Down, Some(Blocker::NoBackend), Some(p)) => p.text.clone(),
            _ => said(state, e.stage, why, machine),
        },
        fixes: fixes(state, why, blocker, installs)
            .into_iter()
            .map(|fix| FixButton {
                fix,
                label: label(fix),
            })
            .collect(),
        seen_host_key: problem
            .filter(|_| state == K::HostKeyChanged)
            .and_then(|p| p.seen_key.clone()),
        expected_host_key: problem
            .filter(|_| state == K::HostKeyChanged)
            .and_then(|p| p.expected_key.clone()),
        detail: problem.map(|p| detail(machine, p, &code(why, blocker))),
        next: None,
    }
}

/// 复制详情里「码」那一项（线上那个名字）：断在那台本身 ⇒ 是哪一处；别的 ⇒ 原因码。
fn code(why: Option<Why>, blocker: Option<Blocker>) -> String {
    match blocker {
        Some(b) => serde_json::to_value(b),
        None => serde_json::to_value(why.unwrap_or(Why::Other)),
    }
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default()
}

/// 那一轮的复制详情：那一句进「断在」、连的是哪台进「对象」、码是这一轮的；其余照那一句带的那份。
fn detail(machine: &str, p: &RoundProblem, code: &str) -> String {
    let d = Detail::parse(&p.detail);
    if d.has_block() {
        return d.render();
    }
    let d = if d.has(Label::Target) {
        d
    } else {
        d.target(Target::Machine(machine))
    };
    let d = if d.has(Label::Hop) {
        d
    } else {
        d.insert(Label::Hop, &p.text)
    };
    let d = if d.has(Label::Code) {
        d
    } else {
        d.insert(Label::Code, code)
    };
    d.render()
}

#[cfg(test)]
#[path = "../../../tests/backend/deploy/machine_tests.rs"]
mod tests;
