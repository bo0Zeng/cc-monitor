//! **每台机器的状态成品**：收事实（接常驻那一轮 · 部署走到哪一步 · 部署收了）成一态，说名字旁那个词与问题行那一句、给哪几颗修法。
//! 桌面由本机后端跑（`control/machine_state.rs`），手机由 mobcore 跑同一份；两端照画。

use cmd_wire::attach::Relation;
use cmd_wire::machine::{
    Blocker, FixButton, MachineFix, MachineState, MachineStateKind as K, Round, RoundProblem,
    Stage, Why,
};
use copy_core::copy_text;
use copy_core::detail::{Detail, Label, Target};

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
        Fact::Step(s) => match s {
            Step::Plan => busy(e, K::Connecting, Some(Stage::Deploy)),
            Step::Place { updating: false } => busy(e, K::Installing, None),
            Step::Place { updating: true } => busy(e, K::Updating, None),
            Step::Restart => busy(e, K::Connecting, Some(Stage::Attach)),
        },
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

/// 名字旁那个词（连着时空；版本不可比的那台说「开发版」）。
fn word(state: K, relation: Option<Relation>) -> String {
    match state {
        K::Up if relation == Some(Relation::Incomparable) => {
            copy_text("machineState.word.incomparable", &[])
        }
        K::Up | K::Unknown => String::new(),
        K::Connecting => copy_text("machineState.word.connecting", &[]),
        K::Installing => copy_text("machineState.word.installing", &[]),
        K::Updating => copy_text("machineState.word.updating", &[]),
        K::Down => copy_text("machinePage.state.down", &[]),
        K::HostKeyChanged => copy_text("machineState.word.hostKey", &[]),
        K::NeedsUpdate => copy_text("machineState.word.needsUpdate", &[]),
        K::Newer => copy_text("machineState.word.newer", &[]),
        K::Disabled => copy_text("machinePage.state.disabled", &[]),
        K::Unsupported => copy_text("machineState.word.unsupported", &[]),
    }
}

/// 问题行那一句（空 ⇒ 不出问题行）。正在连 / 装 / 更新那一句与部署每一步那一句是同一句（[`Step::said`]）。
fn said(state: K, stage: Option<Stage>, why: Option<Why>, machine: &str) -> String {
    match state {
        K::Up | K::Unknown => String::new(),
        K::Connecting => match stage {
            Some(Stage::Deploy) => Step::Plan.said(machine),
            Some(Stage::Attach) | None => Step::Restart.said(machine),
        },
        K::Installing => Step::Place { updating: false }.said(machine),
        K::Updating => Step::Place { updating: true }.said(machine),
        K::Down => match why.unwrap_or(Why::Other) {
            Why::Resolve => copy_text("machineState.down.resolve", &[]),
            Why::Unreachable => copy_text("machineState.down.unreachable", &[]),
            Why::Timeout => copy_text("machineState.down.timeout", &[]),
            Why::Auth => copy_text("machineState.down.auth", &[]),
            Why::Password => copy_text("machineState.down.password", &[]),
            Why::KeyUnreadable => copy_text("machineState.down.keyUnreadable", &[]),
            Why::Jump => copy_text("machineState.down.jump", &[]),
            Why::HostKey | Why::Other => {
                copy_text("machinePage.problem.offline", &[("machine", machine)])
            }
        },
        K::HostKeyChanged => copy_text("machineState.problem.hostKey", &[]),
        K::NeedsUpdate => copy_text("machineState.problem.needsUpdate", &[("machine", machine)]),
        K::Newer => copy_text("machineState.problem.newer", &[("machine", machine)]),
        K::Disabled => copy_text("machinePage.problem.disabled", &[]),
        K::Unsupported => copy_text("machineState.problem.notUnix", &[("machine", machine)]),
    }
}

/// 每一态给哪几颗修法（只在这里定；原因码穷尽，加一个码编不过）。
fn fixes(state: K, why: Option<Why>, blocker: Option<Blocker>, installs: bool) -> Vec<MachineFix> {
    use MachineFix as F;
    match state {
        K::Down if blocker == Some(Blocker::NoBackend) && !installs => vec![F::Install],
        K::Down => match why.unwrap_or(Why::Other) {
            Why::Auth | Why::KeyUnreadable => vec![F::PushKey, F::ConnSettings],
            Why::Password => vec![F::PushKey],
            Why::Resolve | Why::Jump => vec![F::ConnSettings],
            Why::Unreachable | Why::Timeout | Why::HostKey | Why::Other => {
                vec![F::Retry, F::ConnSettings]
            }
        },
        K::HostKeyChanged => vec![F::CompareFingerprint],
        K::NeedsUpdate => vec![F::Update],
        K::Disabled => vec![F::Connect],
        K::Up
        | K::Connecting
        | K::Installing
        | K::Updating
        | K::Newer
        | K::Unsupported
        | K::Unknown => Vec::new(),
    }
}

/// 修法按钮上的字。
pub fn label(fix: MachineFix) -> String {
    match fix {
        MachineFix::Retry => copy_text("machineState.fix.retry", &[]),
        MachineFix::ConnSettings => copy_text("machinePage.conn.toggle", &[]),
        MachineFix::PushKey => copy_text("machineCard.build.pushKey", &[]),
        MachineFix::CompareFingerprint => copy_text("machineState.fix.compareFingerprint", &[]),
        MachineFix::Update => copy_text("machineState.fix.update", &[]),
        MachineFix::Connect => copy_text("machinePage.problem.connect", &[]),
        MachineFix::Install => copy_text("machineState.fix.install", &[]),
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
