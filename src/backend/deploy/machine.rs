//! **每台机器的状态成品**：收事实（接常驻那一轮 · 部署走到哪一步 · 部署收了）成一态，说名字旁那个词与问题行那一句、给哪几颗修法。
//! 桌面由本机后端跑（`control/machine_state.rs`），手机由 mobcore 跑同一份；两端照画。

use cmd_wire::attach::Relation;
use cmd_wire::machine::{
    FixButton, MachineFix, MachineState, MachineStateKind as K, Round, RoundProblem, Stage, Why,
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
}

/// 一件事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// 接常驻那一段的一轮结论；`version` 是本侧的产品版本号（那台与这一版同一份字节时就是它的号）。
    Round { round: Round, version: String },
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
                e.version = (relation == Relation::Same).then_some(version);
                e.problem = None;
            }
            Round::Down {
                problem,
                unsupported,
            } => {
                e.state = Some(if unsupported {
                    K::Unsupported
                } else if problem.why == Some(Why::HostKey) {
                    K::HostKeyChanged
                } else {
                    K::Down
                });
                e.stage = None;
                e.problem = Some(problem);
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
}

/// 读一台的成品。`machine` 是给人看的名字；`enabled = false` ⇒ 「连接这台」关着（停用）。
pub fn product(e: &Entry, machine: &str, enabled: bool) -> MachineState {
    let state = match (enabled, e.state) {
        (false, _) => K::Disabled,
        (true, s) => s.unwrap_or(K::Unknown),
    };
    let why = match state {
        K::Down | K::HostKeyChanged => e.problem.as_ref().map(|p| p.why.unwrap_or(Why::Other)),
        _ => None,
    };
    let problem = e
        .problem
        .as_ref()
        .filter(|_| matches!(state, K::Down | K::HostKeyChanged | K::Unsupported));
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
        said: said(state, e.stage, why, machine),
        fixes: fixes(state, why)
            .into_iter()
            .map(|fix| FixButton {
                fix,
                label: label(fix),
            })
            .collect(),
        seen_host_key: problem
            .filter(|_| state == K::HostKeyChanged)
            .and_then(|p| p.seen_key.clone()),
        detail: problem.map(|p| detail(machine, p, &code(state, why))),
    }
}

/// 名字旁那个词（连着时空；版本不可比的那台说「开发版」）。
fn word(state: K, relation: Option<Relation>) -> String {
    let key = match state {
        K::Up if relation == Some(Relation::Incomparable) => "machineState.word.incomparable",
        K::Up | K::Unknown => return String::new(),
        K::Connecting => "machineState.word.connecting",
        K::Installing => "machineState.word.installing",
        K::Updating => "machineState.word.updating",
        K::Down => "machinePage.state.down",
        K::HostKeyChanged => "machineState.word.hostKey",
        K::NeedsUpdate => "machineState.word.needsUpdate",
        K::Newer => "machineState.word.newer",
        K::Disabled => "machinePage.state.disabled",
        K::Unsupported => "machineState.word.unsupported",
    };
    copy_text(key, &[])
}

/// 问题行那一句（空 ⇒ 不出问题行）。正在连 / 装 / 更新那一句与部署每一步那一句是同一句（[`Step::said`]）。
fn said(state: K, stage: Option<Stage>, why: Option<Why>, machine: &str) -> String {
    let m = [("machine", machine)];
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
            Why::HostKey | Why::Other => copy_text("machinePage.problem.offline", &m),
        },
        K::HostKeyChanged => copy_text("machineState.problem.hostKey", &[]),
        K::NeedsUpdate => copy_text("machineState.problem.needsUpdate", &m),
        K::Newer => copy_text("machineState.problem.newer", &m),
        K::Disabled => copy_text("machinePage.problem.disabled", &[]),
        K::Unsupported => copy_text("machineState.problem.notUnix", &m),
    }
}

/// 每一态给哪几颗修法（只在这里定；原因码穷尽，加一个码编不过）。
fn fixes(state: K, why: Option<Why>) -> Vec<MachineFix> {
    use MachineFix as F;
    match state {
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
    let key = match fix {
        MachineFix::Retry => "machineState.fix.retry",
        MachineFix::ConnSettings => "machinePage.conn.toggle",
        MachineFix::PushKey => "machineCard.build.pushKey",
        MachineFix::CompareFingerprint => "machineState.fix.compareFingerprint",
        MachineFix::Update => "machineState.fix.update",
        MachineFix::Connect => "machinePage.problem.connect",
        MachineFix::Install => "machineState.fix.install",
    };
    copy_text(key, &[])
}

/// 复制详情里「码」那一项：没连上 ⇒ 原因码（线上那个名字）；做不了 ⇒ `not_unix`。
fn code(state: K, why: Option<Why>) -> String {
    if state == K::Unsupported {
        return "not_unix".to_string();
    }
    serde_json::to_value(why.unwrap_or(Why::Other))
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
