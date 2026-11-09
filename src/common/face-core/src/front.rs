//! **↗ 切到终端**：浮层上那一张（写好的成品）—— ↗ 的句子只在这一份里取，只本机后端 `session-window` 调。
//!
//! 两样：会话为什么没有链（[`Unplaced::face`]，会话在哪的事，用那台的名字写）· 壳窗口那一跳没成的每一种结局码
//! （[`missed_faces`]，随回包整串交出，壳照自己的结局码挑一张、不写字）。出口照摆 [`FrontFace`]，按钮上的字照 [`FrontAct`] 取。
//! 线上那几型（[`FrontFace`] · [`FrontAct`] · [`FrontCode`] · [`MissedFace`]）住 `cmd_wire::front`。
//! 规则：认得准才切，认不准照实说、给办法，永远不挑一个窗口切。

use cmd_wire::cells::{Tone, Words};
use cmd_wire::front::{FrontAct, FrontCode, FrontFace, MissedFace};
use copy_core::copy_text;

/// 本机后端说不出进程链的原因（`session-window` 判），连同问那台没问成的那几种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unplaced {
    /// 那台没有在跑的这个会话。
    NotLive,
    /// 在 tmux 或托管终端里、没终端连着（能接回去 ⇒ 给［在终端里打开］）。
    Detached,
    /// 不在 tmux 里、进程没有终端。
    NoTerminal,
    /// 读不了那台的终端信息。
    Unreadable,
    /// 那台本地屏幕上的终端（不经 ssh）。
    NotSsh,
    /// 连接不是这台开的（`addr` 是那台看到的对面地址）。
    Elsewhere { addr: String },
    /// 经跳板 / 端口转换，对不上这台的连接。
    Mismatch,
    /// 这台的连接表 / 进程表没查成。
    QueryFailed,
    /// 那台后端不认这一问（比这一版旧）。
    PeerOld,
    /// 那台没在期限内答。
    PeerTimedOut,
    /// 那台没问到：`said` 是连接层写的那一句（照摆）。
    Unreached { said: String },
}

/// 排一张：标题 · 正文（空 ⇒ 不出）· 灰字 · 语气 · 按钮（字都已从文案表取好；键在各调用处写成字面量，文案表的静态读认得出）。
fn lay(
    title: String,
    body: String,
    hint: Option<String>,
    tone: Tone,
    acts: Vec<FrontAct>,
) -> FrontFace {
    FrontFace {
        title: Words(title),
        body: Words(body),
        hint: hint.map(Words),
        tone,
        acts,
    }
}

/// 壳窗口那一跳没成的每一种结局码 ⇒ 那一张（一个码恰好一张，次序同 [`FrontCode::ALL`]）。
/// `tmux`：会话在 tmux 里（窗口关了时才给［在终端里打开］）。
pub fn missed_faces(tmux: bool) -> Vec<MissedFace> {
    FrontCode::ALL
        .into_iter()
        .map(|code| MissedFace {
            code,
            face: missed(code, tmux),
        })
        .collect()
}

/// 一个结局码 ⇒ 那一张。
fn missed(code: FrontCode, tmux: bool) -> FrontFace {
    use FrontCode as C;
    let none = String::new;
    match code {
        C::Gone => lay(
            copy_text("front.title.gone", &[]),
            none(),
            None,
            Tone::Plain,
            if tmux {
                vec![FrontAct::OpenInTerminal]
            } else {
                vec![]
            },
        ),
        C::Unclear => lay(
            copy_text("front.title.unsure", &[]),
            none(),
            None,
            Tone::Plain,
            vec![],
        ),
        C::Refused => lay(
            copy_text("front.title.flashing", &[]),
            copy_text("front.body.refused", &[]),
            None,
            Tone::Plain,
            vec![],
        ),
        C::NoWindow => lay(
            copy_text("front.title.noTerminal", &[]),
            copy_text("front.body.background", &[]),
            None,
            Tone::Plain,
            vec![],
        ),
        C::DesktopWontSwitch => lay(
            copy_text("front.title.desktopWontSwitch", &[]),
            copy_text("front.body.desktopWontSwitchBare", &[]),
            None,
            Tone::Plain,
            vec![FrontAct::OpenHere],
        ),
        C::Unsupported => lay(
            copy_text("front.title.failed", &[]),
            copy_text("terminalFront.unavailable.short", &[]),
            None,
            Tone::Plain,
            vec![],
        ),
        // 分不清：原因 · 灰字办法（不挑一个切，按钮只在 Linux 没登记时给［接上终端］）。
        C::SeveralUnregistered => several(
            copy_text("front.body.severalUnregistered", &[]),
            Some(copy_text("front.hint.unregistered", &[])),
            vec![FrontAct::Connect],
        ),
        C::SeveralConsole => several(
            copy_text("front.body.severalConsole", &[]),
            Some(copy_text("front.hint.severalConsole", &[])),
            vec![],
        ),
        C::SeveralElevated => several(
            copy_text("front.body.severalElevated", &[]),
            Some(copy_text("front.hint.severalElevated", &[])),
            vec![],
        ),
        C::SeveralForeign => several(copy_text("front.body.severalForeign", &[]), None, vec![]),
        // 窗口切过去了、标签页没切成：为什么。
        C::TabTitleIgnored => tab_missed(
            copy_text("front.body.tabTitleIgnored", &[]),
            Some(copy_text("front.hint.tabTitleIgnored", &[])),
        ),
        C::TabNoUia => tab_missed(copy_text("front.body.tabNoUia", &[]), None),
        C::TabElevated => tab_missed(copy_text("front.body.tabElevated", &[]), None),
        C::TabNoConsole => tab_missed(copy_text("front.body.tabNoConsole", &[]), None),
        // 窗口那一跳卡住（系统调用没在期限内回）：这台没答，可重试。
        C::TimedOut => lay(
            copy_text("front.title.failed", &[]),
            copy_text(
                "front.body.timeout",
                &[("machine", &copy_core::local_machine())],
            ),
            None,
            Tone::Fail,
            vec![FrontAct::Retry],
        ),
    }
}

fn several(body: String, hint: Option<String>, acts: Vec<FrontAct>) -> FrontFace {
    lay(
        copy_text("front.title.unsure", &[]),
        body,
        hint,
        Tone::Warn,
        acts,
    )
}

fn tab_missed(body: String, hint: Option<String>) -> FrontFace {
    lay(
        copy_text("front.title.tabMissed", &[]),
        body,
        hint,
        Tone::Plain,
        vec![],
    )
}

impl Unplaced {
    /// 原因 ⇒ 那一张。`machine` 是会话那台的名字（本机 ⇒ 「本机」）。
    pub fn face(&self, machine: &str) -> FrontFace {
        use Unplaced as U;
        let plain = |title: String, body: String| lay(title, body, None, Tone::Plain, vec![]);
        match self {
            U::NotLive => plain(
                copy_text("front.title.unbound", &[]),
                copy_text("front.body.notLive", &[]),
            ),
            U::Detached => lay(
                copy_text("front.title.noTerminal", &[]),
                copy_text("front.body.detached", &[]),
                None,
                Tone::Plain,
                vec![FrontAct::OpenInTerminal],
            ),
            U::NoTerminal => plain(
                copy_text("front.title.noTerminal", &[]),
                copy_text("front.body.background", &[]),
            ),
            U::Unreadable => {
                let body = copy_text("front.body.unreadable", &[("machine", machine)]);
                let detail = body.clone();
                lay(
                    copy_text("front.title.unsure", &[]),
                    body,
                    None,
                    Tone::Plain,
                    vec![FrontAct::Copy { detail }],
                )
            }
            U::NotSsh => plain(
                copy_text("front.title.elsewhere", &[]),
                copy_text("front.body.notSsh", &[("machine", machine)]),
            ),
            U::Elsewhere { addr } => plain(
                copy_text("front.title.elsewhere", &[]),
                copy_text("front.body.elsewhere", &[("addr", addr)]),
            ),
            U::Mismatch => plain(
                copy_text("front.title.unsure", &[]),
                copy_text("front.body.mismatch", &[]),
            ),
            U::QueryFailed => lay(
                copy_text("front.title.unknown", &[]),
                String::new(),
                None,
                Tone::Fail,
                vec![
                    FrontAct::Retry,
                    FrontAct::Copy {
                        detail: "session-window: query-failed".into(),
                    },
                ],
            ),
            U::PeerOld => lay(
                copy_text("front.title.update", &[("machine", machine)]),
                copy_text("front.body.update", &[]),
                None,
                Tone::Warn,
                vec![FrontAct::Update],
            ),
            U::PeerTimedOut => lay(
                copy_text("front.title.failed", &[]),
                copy_text("front.body.timeout", &[("machine", machine)]),
                None,
                Tone::Fail,
                vec![FrontAct::Retry],
            ),
            U::Unreached { said } => lay(
                copy_text("front.title.failed", &[]),
                said.clone(),
                None,
                Tone::Fail,
                vec![FrontAct::Reconnect],
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/front_tests.rs"]
mod tests;
