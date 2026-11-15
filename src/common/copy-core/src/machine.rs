//! **机器状态那张字表**：线上那一态（[`MachineStateKind`]）⇒ 点画哪一档 · 问题行的语气 · 名字旁那个词 · 问题行那一句 · 修法按钮上的字，
//! 只此一份。后端收事实、合成品（`deploy::machine`）照它写；壳的本机后端那一格（还没连上本机后端那几样）照 [`local`] 写 ——
//! 两处出的字与点同一张表。

use cmd_wire::attach::Relation;
use cmd_wire::cells::Tone;
use cmd_wire::machine::{
    Blocker, MachineDot, MachineFix, MachineStateKind as K, MachineStripLine, Stage, Why,
};

use crate::copy_text;

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
            Some(Stage::Deploy) => connecting(copy_text("mobHostEdit.backend.check", &[])),
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
            Why::SessionDenied => {
                copy_text("machineState.down.sessionDenied", &[("machine", machine)])
            }
            Why::HostKey | Why::Other => crate::offline(machine),
        },
        K::HostKeyChanged => copy_text("machineState.problem.hostKey", &[]),
        K::NeedsUpdate => copy_text("front.title.update", &[("machine", machine)]),
        K::Newer => copy_text("machineState.problem.newer", &[("machine", machine)]),
        K::Disabled => copy_text("machinePage.problem.disabled", &[]),
        K::Unsupported => copy_text("machineState.problem.notUnix", &[("machine", machine)]),
    }
}

/// 机器条上那一行：进不进条 · 语气 · 带不带进度。连着的 · 比这一版新的（也连着）· 用户停用的 · 还不知道的不进条；
/// 缺后端：这一端不装（手机）⇒ 故障（修法是［安装］）；这一端自己会装（桌面，下一轮装上）⇒ 同没连上。
pub fn strip(state: K, blocker: Option<Blocker>, installs: bool) -> Option<MachineStripLine> {
    let (tone, progress) = match state {
        K::Up | K::Newer | K::Disabled | K::Unknown => return None,
        K::Connecting => (Tone::Busy, false),
        K::Installing | K::Updating => (Tone::Busy, true),
        K::NeedsUpdate => (Tone::Warn, false),
        K::Down if blocker == Some(Blocker::NoBackend) && !installs => (Tone::Fail, false),
        K::Down => (Tone::Offline, false),
        K::HostKeyChanged | K::Unsupported => (Tone::Fail, false),
    };
    Some(MachineStripLine { tone, progress })
}

/// 那台的点画哪一档（两端都照它，不各自照态挑）：连着 · 比这一版新 ⇒ 连着；正在连 · 装 · 更新 · 还不知道 ⇒ 待定；
/// 那台旧了 ⇒ 要留意；没连上 · 指纹不对 · 做不了 ⇒ 坏了；停用 ⇒ 关着。
pub fn dot(state: K) -> MachineDot {
    match state {
        K::Up | K::Newer => MachineDot::Up,
        K::Connecting | K::Installing | K::Updating | K::Unknown => MachineDot::Pending,
        K::NeedsUpdate => MachineDot::Warn,
        K::Down | K::HostKeyChanged | K::Unsupported => MachineDot::Failed,
        K::Disabled => MachineDot::Off,
    }
}

/// 修法按钮上的字。
pub fn label(fix: MachineFix) -> String {
    match fix {
        MachineFix::Retry => copy_text("front.act.retry", &[]),
        MachineFix::ConnSettings => copy_text("machinePage.conn.toggle", &[]),
        // 钥匙被拒 · 正用密码登着：同一条路（请那台把这一端的公钥并进 `authorized_keys`）⇒ 同一个字。
        MachineFix::PushKey | MachineFix::UseKey => copy_text("beMachine.fix.useKey", &[]),
        MachineFix::CompareFingerprint => copy_text("machineState.fix.compareFingerprint", &[]),
        MachineFix::Update => copy_text("front.act.update", &[]),
        MachineFix::Connect => copy_text("machinePage.problem.connect", &[]),
        MachineFix::Install => copy_text("machineState.fix.install", &[]),
    }
}

/// 本机后端还没连上的那几样（壳自己的事实：这台电脑上的进程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Local<'a> {
    /// 起本机后端那一趟正在走。
    Starting,
    /// 口上占着的那一份比这一版旧。
    Older,
    /// 口上占着的那一份比这一版新（没连着）。
    Newer,
    /// 没起来、接不上：`failed` ＝ 起不来的那一句（没有 ⇒ 只说本机连接失败）。
    Down { failed: Option<&'a str> },
}

/// 本机那一格怎么画：点 · 语气 · 词 · 句。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLook {
    pub dot: MachineDot,
    pub tone: Tone,
    pub word: String,
    pub said: String,
}

/// 本机后端没连着那几样：词与句照远端同一态那一格（正在起 ⇒ 正在连到接常驻那一步；口上旧 ⇒ 那台旧了；
/// 口上新 ⇒ 比这一版新；没起来 ⇒ 没连上，句子是起不来那一句，没有 ⇒「本机连接失败」）。
/// 点与语气：正在起 · 口上旧同远端那一格（正在连 ⇒ 待定 · 忙；旧了 ⇒ 要留意）；口上新 · 没起来是本机自己的一条：
/// 本机后端没连着 ⇒ 坏了 · 失败（远端「比这一版新」是连着的点；远端没连上是离线的灰，本机没有离线这回事）。
pub fn local(l: Local<'_>, machine: &str) -> LocalLook {
    let remote = |k: K, tone: Tone, stage: Option<Stage>| LocalLook {
        dot: dot(k),
        tone,
        word: word(k, None),
        said: said(k, stage, None, machine),
    };
    let unlinked = |k: K, said: String| LocalLook {
        dot: MachineDot::Failed,
        tone: Tone::Fail,
        word: word(k, None),
        said,
    };
    match l {
        Local::Starting => remote(K::Connecting, Tone::Busy, Some(Stage::Attach)),
        Local::Older => remote(K::NeedsUpdate, Tone::Warn, None),
        Local::Newer => unlinked(K::Newer, said(K::Newer, None, None, machine)),
        Local::Down { failed } => unlinked(
            K::Down,
            failed.map_or_else(|| copy_text("machineState.down.local", &[]), str::to_string),
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/machine_tests.rs"]
mod tests;
