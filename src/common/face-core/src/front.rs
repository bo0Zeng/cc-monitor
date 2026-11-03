//! **↗ 切到终端**：结局（事实）与浮层上那一张（写好的成品）—— ↗ 的句子只在这一份里取。
//!
//! 两个写家都调这里：壳拉前之后（[`FrontOutcome::told`]，窗口的事）· 本机后端 `session-window` 说不出链时（[`Unplaced::face`]，
//! 会话在哪的事，用那台的名字写）。出口照摆 [`FrontFace`]，按钮上的字照 [`FrontAct`] 取，不按结局自己挑。
//! 这里是结局与原因（不上线的事实）和照它们写字的那一段；线上那三型（[`FrontFace`] · [`FrontAct`] · [`FrontTold`]）住 `cmd_wire::front`。
//! 规则：认得准才切，认不准照实说、给办法，永远不挑一个窗口切。

use cmd_wire::cells::{Tone, Words};
use cmd_wire::front::{FrontAct, FrontFace, FrontMiss, FrontTold};
use copy_core::copy_text;
use serde::Serialize;

/// 壳拉前一次的结局（闭集）。分不清是哪个窗口时**不挑一个切**：照实说拉不了、带候选个数和原因。
/// 不上线（线上走 [`FrontTold`]）；诊断子命令 `--ccm-pick` 照它印一行 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrontOutcome {
    /// 窗口到前面了（核过前台），要切的标签页也选中了（或窗口里只有一个标签 / 没有标签这回事）。
    Switched,
    /// 链上哪一级都认不出显示在哪个窗口，终端程序又开着几个窗口 ⇒ 分不清；`count` 是候选窗口个数，`why` 是为什么认不出。
    Several {
        program: String,
        count: usize,
        why: SeveralWhy,
    },
    /// 窗口到前面了，标签页没切成（原因见 `why`）：前面显示的可能是同一窗口里别的标签。
    SwitchedWindow { why: TabMiss },
    /// 认得的那个窗口已经不在了。
    WindowGone,
    /// 句柄或进程号已被别的进程复用 ⇒ 认不准（细节进日志）。
    Unclear,
    /// 找到了，系统不许抢前台（它在任务栏闪），或答应了前台却没换。
    Refused,
    /// 整条链上哪一级都没有窗口：真在后台。
    NoWindow { program: String },
    /// Wayland 会话：别的程序的窗口 cc-monitor 看不见、也切不过去（`desktop` 是桌面名，可能空）。
    DesktopWontSwitch { desktop: String },
    /// 这台系统没有「按句柄找 / 验 / 拉前窗口」这一族。
    Unsupported,
}

/// 分不清时，链上为什么没有一级认出它显示在哪个窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeveralWhy {
    /// Linux：本会话的 shell 没登记（开在接上终端之前）。
    Unregistered,
    /// Windows：借本会话的控制台被系统拒了。
    ConsoleUnreachable,
    /// Windows：终端是以管理员身份运行的，cc-monitor 借不到它名下的控制台。
    Elevated,
    /// Windows：终端程序不提供控制台（不把伪控制台挂到自己窗口下，如 VS Code 的终端）。
    Foreign,
}

/// 窗口切过去了、标签页没切成的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TabMiss {
    /// 记号一直没出现在标签名上：标签名被固定（手动改过名）· 终端设置不显示程序标题（`suppressApplicationTitle`）。
    TitleIgnored,
    /// 读不到 / 点不了那个窗口的标签页（UI Automation 不给）。
    NoUia,
    /// 终端是以管理员身份运行的：cc-monitor 无权读它的标签页。
    Elevated,
    /// 读不到本会话的控制台（挂不上记号）。
    NoConsole,
}

/// 本机后端说不出进程链的原因（`session-window` 判），连同问那台没问成的那几种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unplaced {
    /// 那台没有在跑的这个会话。
    NotLive,
    /// 在 tmux 里、没终端连着。
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
    /// 出口那一跳别的出错：`detail` 是原话（可复制）。
    Broken { detail: String },
    /// 出口收到的认不出（链空又没带那一张）：`detail` 是认不出的那一处（可复制）。
    BadShape { detail: String },
}

/// 出口那一跳没成（界面挑的种类）⇒ 同一张表里的那一种：本机后端不认 · 期限到 · 没问到与问那台没问成同一张，另两种带原话。
impl From<FrontMiss> for Unplaced {
    fn from(m: FrontMiss) -> Self {
        match m {
            FrontMiss::Old => Unplaced::PeerOld,
            FrontMiss::TimedOut => Unplaced::PeerTimedOut,
            FrontMiss::Unreached { said } => Unplaced::Unreached { said },
            FrontMiss::Broken { detail } => Unplaced::Broken { detail },
            FrontMiss::BadShape { detail } => Unplaced::BadShape { detail },
        }
    }
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

impl FrontOutcome {
    /// 结局 ⇒ 交给出口的那一份。`tmux`：会话在 tmux 里（窗口关了时才给［在终端里打开］，`session-window` 带来）。
    pub fn told(self, tmux: bool) -> FrontTold {
        use FrontOutcome as O;
        let none = String::new;
        FrontTold::Said(match self {
            O::Switched => return FrontTold::Switched,
            O::Several {
                program,
                count,
                why,
            } => several(&program, count, why),
            O::SwitchedWindow { why } => tab_missed(why),
            O::WindowGone => lay(
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
            O::Unclear => lay(
                copy_text("front.title.unsure", &[]),
                none(),
                None,
                Tone::Plain,
                vec![],
            ),
            O::Refused => lay(
                copy_text("front.title.flashing", &[]),
                copy_text("front.body.refused", &[]),
                None,
                Tone::Plain,
                vec![],
            ),
            O::NoWindow { .. } => lay(
                copy_text("front.title.noTerminal", &[]),
                copy_text("front.body.background", &[]),
                None,
                Tone::Plain,
                vec![],
            ),
            O::DesktopWontSwitch { desktop } => lay(
                copy_text("front.title.desktopWontSwitch", &[]),
                if desktop.is_empty() {
                    copy_text("front.body.desktopWontSwitchBare", &[])
                } else {
                    copy_text("front.body.desktopWontSwitch", &[("desktop", &desktop)])
                },
                None,
                Tone::Plain,
                vec![FrontAct::OpenHere],
            ),
            O::Unsupported => lay(
                copy_text("front.title.failed", &[]),
                copy_text("terminalFront.unavailable.short", &[]),
                None,
                Tone::Plain,
                vec![],
            ),
        })
    }
}

/// 分不清：候选个数与原因 · 灰字办法（不挑一个切，按钮只在 Linux 没登记时给［接上终端］）。
fn several(program: &str, count: usize, why: SeveralWhy) -> FrontFace {
    let n = count.to_string();
    let (body, hint, acts) = match why {
        SeveralWhy::Unregistered => (
            copy_text(
                "front.body.severalUnregistered",
                &[("program", program), ("n", &n)],
            ),
            Some(copy_text("front.hint.unregistered", &[])),
            vec![FrontAct::Connect],
        ),
        SeveralWhy::ConsoleUnreachable => (
            copy_text(
                "front.body.severalConsole",
                &[("program", program), ("n", &n)],
            ),
            Some(copy_text("front.hint.severalConsole", &[])),
            vec![],
        ),
        SeveralWhy::Elevated => (
            copy_text(
                "front.body.severalElevated",
                &[("program", program), ("n", &n)],
            ),
            Some(copy_text("front.hint.severalElevated", &[])),
            vec![],
        ),
        SeveralWhy::Foreign => (
            copy_text(
                "front.body.severalForeign",
                &[("program", program), ("n", &n)],
            ),
            None,
            vec![],
        ),
    };
    lay(
        copy_text("front.title.unsure", &[]),
        body,
        hint,
        Tone::Warn,
        acts,
    )
}

/// 窗口切过去了、标签页没切成：为什么。
fn tab_missed(why: TabMiss) -> FrontFace {
    let (body, hint) = match why {
        TabMiss::TitleIgnored => (
            copy_text("front.body.tabTitleIgnored", &[]),
            Some(copy_text("front.hint.tabTitleIgnored", &[])),
        ),
        TabMiss::NoUia => (copy_text("front.body.tabNoUia", &[]), None),
        TabMiss::Elevated => (copy_text("front.body.tabElevated", &[]), None),
        TabMiss::NoConsole => (copy_text("front.body.tabNoConsole", &[]), None),
    };
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
            U::Broken { detail } => lay(
                copy_text("front.title.failed", &[]),
                copy_text("reason.io.unknown", &[]),
                None,
                Tone::Fail,
                vec![FrontAct::Copy {
                    detail: detail.clone(),
                }],
            ),
            U::BadShape { detail } => lay(
                copy_text("front.title.badShape", &[]),
                String::new(),
                None,
                Tone::Fail,
                vec![FrontAct::Copy {
                    detail: detail.clone(),
                }],
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/front_tests.rs"]
mod tests;
