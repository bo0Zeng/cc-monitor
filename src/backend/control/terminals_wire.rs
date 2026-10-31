//! 终端管理 L1 三条（`terminals-list` · `terminal-preview` · `terminal-input`）的线上形：入参 · 回包 · 码各一个类型（第 8 批 typed）。
//! 本体在 [`super::terminals`]；这里只有形状与「怎么跑」那一层。两个前端共用（手机也在读）⇒ 回包的键与「有才出」那几格照换形前原样。

use super::terminals::{self as t, On};
use crate::common::said::Said;
use crate::wire::typed::{self, Command};
use copy_core::copy_text;
use serde::Deserialize;

// ── 入参 ──────────────────────────────────────────────────────────────

// L1 三条的入参住 `cmd-wire::terminal`（名字与回包挂在 `cmd_wire::Call` 上）。
pub(crate) use cmd_wire::terminal::{TerminalInputReq, TerminalPreviewReq, TerminalsListReq};

/// `terminal-take-input` · `terminal-release-input` 的入参：名单里的句柄 ＋ 自报的前端（替它接管 / 交还）。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalHoldReq {
    /// 名单里的不透明句柄（托管那一形）。
    pub(crate) terminal: String,
    /// 自报的前端（`desktop` · `mobile`；缺省按桌面）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// `session-interrupt` 的入参：哪条会话 ＋ 自报的前端（过「哪个前端的会话」那一维）。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionInterruptReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 自报的前端。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

// ── 回包 ──────────────────────────────────────────────────────────────

// 回包的线上形住 `cmd-wire::session`（手机嵌的 Rust 核心按同一份类型解）。
pub(crate) use cmd_wire::session::{
    Ability, Attached, Cursor, Preview, ScreenLine, Sent, SentResult, Span, StartedBy, TermSession,
    TermState, TerminalCan, TerminalItem, TerminalsList,
};

/// 不能 ⇒ 带上给人看的那一句（原因 ⇒ 那一句只在 [`t::no_said`] 一处）。
pub(crate) fn refused(why: t::Refused) -> Ability {
    Ability::No {
        no: why,
        said: t::no_said(why),
    }
}

// ── 码 ────────────────────────────────────────────────────────────────

typed::codes! {
    /// `terminals-list` 的码：列名单那两发 tmux。
    pub(crate) enum TerminalsListErr {
        /// 列不出：`list-panes` 失败（不是「没起 server」那一种），或 tmux 装了、但起不来。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

typed::codes! {
    /// `terminal-preview` 的码：认目标 · 列名单 · 抓屏 · 问尺寸。
    pub(crate) enum TerminalPreviewErr {
        /// 目标没恰给一个（或不是非空短串）。
        BadTarget = "bad_target",
        /// 名单里此刻没有它。
        NotKnown = "not_known",
        /// 不止一个终端挂着这个 sid。
        Ambiguous = "ambiguous",
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// 一个 tmux server 都没有（抓屏那一发）。
        NoServer = "no_server",
        /// 那个终端在读的时候没了。
        NoSuchSession = "no_such_session",
        /// 抓屏失败（tmux 原话进复制详情）。
        CaptureFailed = "capture_failed",
        /// 列名单失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

typed::codes! {
    /// `terminal-input` 的码：认目标 · 列名单 · 过门 · 比画面 · 送。认不出 / 不归我们 / 画面变了不是错（`result: "refused"`）。
    pub(crate) enum TerminalInputErr {
        /// 目标没恰给一个（或不是非空短串）。
        BadTarget = "bad_target",
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// 一个 tmux server 都没有（比画面那一发）。
        NoServer = "no_server",
        /// 那个终端在读的时候没了。
        NoSuchSession = "no_such_session",
        /// 比画面那一次抓屏失败。
        CaptureFailed = "capture_failed",
        /// 列名单失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

typed::codes! {
    /// `terminal-take-input` · `terminal-release-input` 的码：认目标 · 过门 · 托管口那一下。
    pub(crate) enum TerminalHoldErr {
        /// 句柄不是非空短串。
        BadTarget = "bad_target",
        /// 名单里此刻没有它。
        NotKnown = "not_known",
        /// 不止一个终端挂着它。
        Ambiguous = "ambiguous",
        /// 是 tmux 那一形：各端都能打字，没有持有。
        NotManaged = "not_managed",
        /// 别的前端的终端。
        NotYours = "not_yours",
        /// 托管口没答（终端在问的时候没了）。
        NoSuchSession = "no_such_session",
        /// 列名单失败（tmux 装了却起不来也落这一格）。
        Unobservable = "unobservable",
        /// 列名单那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

// ── 怎么跑 ────────────────────────────────────────────────────────────

/// `terminal-take-input`：替自报的前端接管托管终端的输入（之后 `terminal-input` 经它送）。阻塞档（连托管口）。
pub(crate) struct TerminalTakeInputCmd;

impl Command for TerminalTakeInputCmd {
    const NAME: &'static str = "terminal-take-input";
    type Req = TerminalHoldReq;
    type Reply = cmd_wire::session::InputNow;
    type Err = TerminalHoldErr;
}

impl crate::wire::typed::Handle for TerminalTakeInputCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalHoldReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<cmd_wire::session::InputNow, TerminalHoldErr>>
           + Send
           + 'static {
        async move { hold(&req, true) }
    }
}

/// `terminal-release-input`：交还（托管进程交给最近连上的真终端，没有就空着）。阻塞档。
pub(crate) struct TerminalReleaseInputCmd;

impl Command for TerminalReleaseInputCmd {
    const NAME: &'static str = "terminal-release-input";
    type Req = TerminalHoldReq;
    type Reply = cmd_wire::session::InputNow;
    type Err = TerminalHoldErr;
}

impl crate::wire::typed::Handle for TerminalReleaseInputCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalHoldReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<cmd_wire::session::InputNow, TerminalHoldErr>>
           + Send
           + 'static {
        async move { hold(&req, false) }
    }
}

fn hold(req: &TerminalHoldReq, take: bool) -> Result<cmd_wire::session::InputNow, TerminalHoldErr> {
    t::hold_on(&req.terminal, req.client.as_deref(), take)
        .map(|input| cmd_wire::session::InputNow { input })
        .map_err(hold_refused)
}

/// 接管 · 交还没做成 ⇒ 这一条的码与那一句：认得出为什么的照送字那一套说（终端没了就说没了），别的照下层那句；下层那句进原话。
pub(crate) fn hold_refused(f: Said) -> TerminalHoldErr {
    match f.code.as_str() {
        // 同 `terminals-list`：tmux 装了却起不来说成「列不出」。
        "no_tmux" => TerminalHoldErr::Unobservable(crate::common::contract::joined(f.said, f.raw)),
        _ => typed::of_fail_said(f, TerminalHoldErr::Unobservable, |c| match c {
            "not_known" | "no_such_session" => Some(t::no_said(t::Refused::Ended)),
            "not_yours" => Some(t::no_said(t::Refused::NotYours)),
            "ambiguous" => Some(t::no_said(t::Refused::Ambiguous)),
            _ => None,
        }),
    }
}

/// `terminals-list`：这台的终端名单。阻塞档（起 tmux）。
pub(crate) struct TerminalsListCmd;

impl Command for TerminalsListCmd {
    typed::by_call!(TerminalsListReq);
    type Err = TerminalsListErr;
}

impl crate::wire::typed::Handle for TerminalsListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalsListReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<TerminalsList, TerminalsListErr>> + Send + 'static
    {
        async move {
            // 名单这一条登记成「tmux 可选」（没装 ⇒ 空名单，不是错）：装了却起不来那一种（下层回 `no_tmux`）说成「列不出」，
            //   不在这一条的码表里另立一个 `no_tmux`（`target_parity_guard` 钉着两句不许自相矛盾）。
            t::list_on(On::default(), &req).map_err(|f| match f.code.as_str() {
                "no_tmux" => TerminalsListErr::Unobservable(crate::common::contract::joined(f.said, f.raw)),
                _ => typed::of_fail(f, TerminalsListErr::Unobservable),
            })
        }
    }
}

/// `terminal-preview`：抓一个终端的一屏（只抓一次，轮询归调用方）。只读、不过身份门。
pub(crate) struct TerminalPreviewCmd;

impl Command for TerminalPreviewCmd {
    typed::by_call!(TerminalPreviewReq);
    type Err = TerminalPreviewErr;
}

impl crate::wire::typed::Handle for TerminalPreviewCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalPreviewReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Preview, TerminalPreviewErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            t::preview_on(On::default(), &req, &cx.tz).map_err(|f| {
                typed::of_fail_said(f, TerminalPreviewErr::CaptureFailed, |c| {
                    Some(preview_said(c))
                })
            })
        }
    }
}

/// `terminal-input`：往一个终端送字或送键（过身份门）。
pub(crate) struct TerminalInputCmd;

impl Command for TerminalInputCmd {
    typed::by_call!(TerminalInputReq);
    type Err = TerminalInputErr;
}

impl crate::wire::typed::Handle for TerminalInputCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalInputReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Sent, TerminalInputErr>> + Send + 'static {
        async move {
            input(&req).map_err(|f| {
                typed::of_fail_said(f, TerminalInputErr::Unobservable, |_| {
                    Some(copy_text("terminalReads.input.refused", &[]))
                })
            })
        }
    }
}

/// 读画面被拒成 `code` 时给人看的那一句（终端的显示名后端不知道，界面放在句子旁边）。
pub(crate) fn preview_said(code: &str) -> String {
    match code {
        "no_tmux" => copy_text("terminalReads.preview.noTmux", &[]),
        "no_server" => copy_text("terminalReads.preview.noServer", &[]),
        "no_such_session" => copy_text("terminalReads.preview.noSuchSession", &[]),
        "capture_failed" => copy_text("terminalReads.preview.failed", &[]),
        "bad_target" | "bad_args" => copy_text("terminalReads.preview.badTarget", &[]),
        "not_known" => copy_text("terminalReads.preview.notKnown", &[]),
        "ambiguous" => copy_text("terminalReads.preview.ambiguous", &[]),
        "unobservable" => copy_text("terminalReads.preview.unobservable", &[]),
        "child_timed_out" => copy_text("terminalReads.preview.childTimedOut", &[]),
        _ => copy_text("terminalReads.preview.otherCode", &[]),
    }
}

/// 送字 / 送键（生产：默认 socket）。计划退回那一条也经它送（`faces/plan_return_face.rs`）。
pub(crate) fn input(req: &TerminalInputReq) -> Result<Sent, Said> {
    t::input_on(On::default(), req)
}

/// `session-interrupt`：中断一条会话这一轮（会话 ■）。不进输入队、不靠持有输入：托管那一形经托管口 `interrupt`，tmux 那一形送一个 Escape。
/// 回包同 `terminal-input`（送到了 · 不知道 · 被拒 ＋ 那一句）。阻塞档（起 tmux · 连托管口）。
pub(crate) struct SessionInterruptCmd;

impl Command for SessionInterruptCmd {
    const NAME: &'static str = "session-interrupt";
    type Req = SessionInterruptReq;
    type Reply = Sent;
    type Err = TerminalInputErr;
}

impl crate::wire::typed::Handle for SessionInterruptCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: SessionInterruptReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Sent, TerminalInputErr>> + Send + 'static {
        async move {
            t::interrupt_on(On::default(), &req.sid, req.client.as_deref()).map_err(|f| {
                typed::of_fail_said(f, TerminalInputErr::Unobservable, |_| {
                    Some(copy_text("terminalReads.input.refused", &[]))
                })
            })
        }
    }
}

/// 往一条会话说一句（生产：默认 socket）：同 [`input`]，窗格前台只剩 shell ⇒ 不送、回 `ended`（[`t::say_on`]）。输入队投递经它送（`faces/session_send.rs`）。
pub(crate) fn say(req: &TerminalInputReq) -> Result<Sent, Said> {
    t::say_on(On::default(), req)
}
