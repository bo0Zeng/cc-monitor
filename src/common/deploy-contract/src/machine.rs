//! 每台机器状态成品里「态 ⇒ 字」那几张表：名字旁那个词 · 问题行那一句 · 给哪几颗修法 · 按钮上的字，
//! 外加本机那一行（本机后端没起时壳出，连上之前就要判）。远端各台由后端 `deploy::machine` 收事实、照这里出字；两端只有这一份。

use cmd_wire::attach::Relation;
use cmd_wire::machine::{
    Blocker, FixButton, MachineFix, MachineState, MachineStateKind as K, Stage, Why,
};
use copy_core::copy_text;

/// 名字旁那个词（连着时空；版本不可比的那台说「开发版」）。
pub fn word(state: K, relation: Option<Relation>) -> String {
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

/// 问题行那一句（空 ⇒ 不出问题行）。正在连 / 装 / 更新那一句也是部署每一步那一句。
pub fn said(state: K, stage: Option<Stage>, why: Option<Why>, machine: &str) -> String {
    let connecting = |stage: String| {
        copy_text(
            "machineState.busy.connecting",
            &[("machine", machine), ("stage", &stage)],
        )
    };
    match state {
        K::Up | K::Unknown => String::new(),
        K::Connecting => match stage {
            Some(Stage::Deploy) => connecting(copy_text("machineState.stage.deploy", &[])),
            Some(Stage::Attach) | None => connecting(copy_text("machineState.stage.attach", &[])),
        },
        K::Installing => copy_text("machineState.busy.installing", &[("machine", machine)]),
        K::Updating => copy_text("machineState.busy.updating", &[("machine", machine)]),
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

/// 每一态给哪几颗修法（原因码穷尽，加一个码编不过）。`installs` ⇒ 这一端自己会把后端装上去（不给［安装］）。
pub fn fixes(
    state: K,
    why: Option<Why>,
    blocker: Option<Blocker>,
    installs: bool,
) -> Vec<MachineFix> {
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

/// 本机那一台手上的事实（宿主交）。
pub struct Local<'a> {
    /// 本机后端的通道在不在。
    pub channel: bool,
    /// 起本机后端那一趟正在走。
    pub starting: bool,
    /// 手上这一版的构建标识；没带后端字节（开发构建）⇒ `None`。
    pub mine: Option<&'a str>,
    /// 口上占着的、构建与这一版不同的那一份的构建标识。
    pub foreign: Option<&'a str>,
    /// 手上这一版的产品版本号。
    pub version: &'a str,
    /// 起的时候被拒 ⇒ 那一句下一步。
    pub refused: Option<String>,
}

/// 本机那一台的成品：通道在 ⇒ 连着；正在起 ⇒ 连接中（到「启动」那一步、不给修法）；
/// 不在而口上占着别的构建 ⇒ 按两边构建的序说「要更新」/「较新」（解不出序 ⇒ 照没连上说）；别的 ⇒ 没连上。
/// 没连上的修法只有［重试］（起本机后端）；被拒那一句下一步只在没连上时带。
pub fn local(f: Local<'_>) -> MachineState {
    let relation = match (f.channel, f.mine, f.foreign) {
        (true, Some(_), _) => Relation::Same,
        (false, Some(m), Some(theirs)) => crate::build_relation(theirs, m),
        (false, Some(_), None) => Relation::Same,
        (_, None, _) => Relation::Incomparable,
    };
    let state = match (f.channel, relation) {
        (true, _) => K::Up,
        (false, _) if f.starting => K::Connecting,
        (false, Relation::Older) => K::NeedsUpdate,
        (false, Relation::Newer) => K::Newer,
        (false, _) => K::Down,
    };
    let stage = (state == K::Connecting).then_some(Stage::Attach);
    let said = match state {
        K::Down => copy_text("machineState.down.local", &[]),
        s => said(s, stage, None, &copy_text("remote.cards.local", &[])),
    };
    let fixes = match state {
        K::Up | K::Connecting => Vec::new(),
        _ => vec![FixButton {
            fix: MachineFix::Retry,
            label: label(MachineFix::Retry),
        }],
    };
    MachineState {
        state,
        reason: None,
        stage,
        version: (relation == Relation::Same).then(|| f.version.to_string()),
        version_relation: Some(relation),
        os: None,
        word: word(state, Some(relation)),
        said,
        fixes,
        seen_host_key: None,
        expected_host_key: None,
        detail: None,
        next: if f.channel { None } else { f.refused },
    }
}
