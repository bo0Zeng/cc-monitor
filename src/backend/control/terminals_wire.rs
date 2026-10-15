//! 终端管理 L1 三条（`terminals-list` · `terminal-preview` · `terminal-input`）的线上形：入参 · 回包 · 码各一个类型（第 8 批 typed）。
//! 本体在 [`super::terminals`]；这里只有形状与「怎么跑」那一层。两个前端共用（手机也在读）⇒ 回包的键与「有才出」那几格照换形前原样。

use super::terminals::{self as t, CmdErr, On};
use crate::wire::typed::{self, Command};
use copy_core::copy_text;
use serde::Deserialize;

// ── 入参 ──────────────────────────────────────────────────────────────

/// `terminals-list` 的入参。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalsListReq {
    /// 自报的前端（1–32 个 `[a-z0-9-]`）：决定每行的 `can` 与 `started_by.mine`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// `terminal-preview` 的入参。目标恰给一个：`terminal`（名单里的句柄）· `sid`（会话 id）。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalPreviewReq {
    /// 目标：名单里的句柄（与 `sid` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sid: Option<String>,
    /// 要不要颜色；缺省 `true`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) color: Option<bool>,
    /// 往回多要几行；缺省 0、上限 2000（超了截到上限、回包 `capped: true`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) scrollback: Option<u64>,
}

/// `terminal-input` 的入参。目标同 [`TerminalPreviewReq`]；送什么恰给一个：`text`（＋ `enter`）· `key`。
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalInputReq {
    /// 目标：名单里的不透明句柄（前端不拼、不解析）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sid: Option<String>,
    /// 送字：字面字，原样送、不解释成键名；多行按粘贴送（与 `key` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) text: Option<String>,
    /// `text` 之后补一个回车；缺省 `true`（只跟 `text`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) enter: Option<bool>,
    /// 送键：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) key: Option<String>,
    /// 送之前看到的那一屏的指纹；画面已经变了 ⇒ 不送、回 `refused` ＋ `screen_changed`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) seen_screen: Option<String>,
    /// 自报的前端（过「哪个前端的会话」那一维）。
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

// ── 怎么跑 ────────────────────────────────────────────────────────────

/// `terminals-list`：这台的终端名单。阻塞档（起 tmux）。
pub(crate) struct TerminalsListCmd;

impl Command for TerminalsListCmd {
    const NAME: &'static str = "terminals-list";
    type Req = TerminalsListReq;
    type Reply = TerminalsList;
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
                "no_tmux" => TerminalsListErr::Unobservable(f.into_said()),
                _ => typed::of_fail(f, TerminalsListErr::Unobservable),
            })
        }
    }
}

/// `terminal-preview`：抓一个终端的一屏（只抓一次，轮询归调用方）。只读、不过身份门。
pub(crate) struct TerminalPreviewCmd;

impl Command for TerminalPreviewCmd {
    const NAME: &'static str = "terminal-preview";
    type Req = TerminalPreviewReq;
    type Reply = Preview;
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
    const NAME: &'static str = "terminal-input";
    type Req = TerminalInputReq;
    type Reply = Sent;
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
pub(crate) fn input(req: &TerminalInputReq) -> Result<Sent, CmdErr> {
    t::input_on(On::default(), req)
}

/// 往一条会话说一句（生产：默认 socket）：同 [`input`]，窗格前台只剩 shell ⇒ 不送、回 `ended`（[`t::say_on`]）。`session-say` 经它送。
pub(crate) fn say(req: &TerminalInputReq) -> Result<Sent, CmdErr> {
    t::say_on(On::default(), req)
}
