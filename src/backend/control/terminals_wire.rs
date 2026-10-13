//! 终端管理 L1 三条（`terminals-list` · `terminal-preview` · `terminal-input`）的线上形：入参 · 回包 · 码各一个类型（第 8 批 typed）。
//! 本体在 [`super::terminals`]；这里只有形状与「怎么跑」那一层。两个前端共用（手机也在读）⇒ 回包的键与「有才出」那几格照换形前原样。

use super::terminals::{self as t, CmdErr, On};
use crate::stream::inbound::typed::{self, Blocking, Command, Cx};
use serde::{Deserialize, Serialize};

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

/// `terminals-list` 的回包。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TerminalsList {
    /// 终端名单（每行一个终端）。
    pub(crate) terminals: Vec<TerminalItem>,
    /// `false` ＝ 名单里有读不懂的行（画「部分」）。
    pub(crate) complete: bool,
}

/// 名单里的一个终端。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TerminalItem {
    /// 名单里那一行的不透明句柄（前端不拼、不解析；送字 / 抓屏时交回）。
    pub(crate) terminal: String,
    /// 终端宿主（这一版是 `tmux`）。
    #[cfg_attr(test, ts(type = "\"tmux\""))]
    pub(crate) host: &'static str,
    /// tmux 会话名。
    pub(crate) tmux_name: String,
    /// 里面跑着会话、前台不是 shell 时是窗格标题；否则是前台程序名。
    pub(crate) title: String,
    /// 前台程序名。
    pub(crate) program: String,
    /// 当前目录。
    pub(crate) cwd: String,
    /// 里面跑着会话（`@ccm_sid`）时才有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) session: Option<TermSession>,
    /// 谁起的。
    pub(crate) started_by: StartedBy,
    /// 此刻连着它的终端客户端；空 ＝ 后台。
    pub(crate) clients: Vec<Attached>,
    /// 输入方式：`shared`（tmux：各端都能打字）。
    #[cfg_attr(test, ts(type = "\"shared\""))]
    pub(crate) input: &'static str,
    /// 此刻的样子。
    pub(crate) state: TermState,
    /// 最近动静（秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) last_activity: u64,
    /// 这个调用方能做什么（抓屏不过身份门、恒可用，不在这里）。
    pub(crate) can: TerminalCan,
}

/// 名单一行里跑着的会话。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TermSession {
    /// 会话 id。
    pub(crate) sid: String,
    /// 哪一家（没标就缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent: Option<String>,
}

/// 谁起的这个终端。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct StartedBy {
    /// 会话上的 `@ccm_client`（没声明 ⇒ `null`）。
    pub(crate) client: Option<String>,
    /// 这个调用方过不过身份门（与 `can.input` 是不是 `true` 同一个判定；结束还要看 `can.end`）。
    pub(crate) mine: bool,
}

/// 连着一个终端的客户端。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Attached {
    /// 客户端种类（`terminal-window`）。
    #[cfg_attr(test, ts(type = "\"terminal-window\""))]
    pub(crate) kind: &'static str,
    /// 连上的时刻（秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) since: u64,
    /// 最近动静（秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) last_activity: u64,
}

/// 名单一行此刻的样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum TermState {
    /// 前台不是 shell。
    Running,
    /// 没会话、前台是 shell。
    Idle,
    /// 有会话、前台是 shell（程序已退出）。
    ProgramExited,
}

/// 这个调用方能对这一行做什么。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TerminalCan {
    /// 能不能送字 / 送键。
    #[cfg_attr(
        test,
        ts(type = "true | { no: \"not_yours\" | \"not_managed\", said: string }")
    )]
    pub(crate) input: Ability,
    /// 能不能结束。
    #[cfg_attr(
        test,
        ts(
            type = "true | { no: \"not_yours\" | \"not_managed\" | \"other_windows\", said: string }"
        )
    )]
    pub(crate) end: Ability,
}

/// 能 ⇒ 线上 `true`；不能 ⇒ `{no: 原因码, said: 给人看的那一句}`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ability {
    Yes,
    No(t::Refused),
}

impl Serialize for Ability {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Ability::Yes => s.serialize_bool(true),
            Ability::No(why) => {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("no", why)?;
                m.serialize_entry("said", &t::no_said(*why))?;
                m.end()
            }
        }
    }
}

/// `terminal-preview` 的回包（终端实时预览每一帧也是它）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Preview {
    /// 这一屏的指纹（16 位十六进制）：内容一变就变，送字时带回来。
    pub(crate) screen: String,
    /// 列数。
    pub(crate) cols: u32,
    /// 行数。
    pub(crate) rows: u32,
    /// 光标。
    pub(crate) cursor: Cursor,
    /// 自上而下的行（往回要的在最前）。
    pub(crate) lines: Vec<ScreenLine>,
    /// 实际往回给了几行。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) scrollback_lines: usize,
    /// 要的比上限多、截到了上限。
    pub(crate) capped: bool,
    /// 抓屏时刻（秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) captured_at: u64,
    /// 抓屏时刻在看的那一台钟上的（请求的 `tz`）`HH:MM:SS`（界面照抄、不换算）。
    pub(crate) captured_at_text: String,
}

/// 光标。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Cursor {
    /// 列。
    pub(crate) x: u32,
    /// 行。
    pub(crate) y: u32,
    /// 看得见。
    pub(crate) visible: bool,
}

/// 一屏里的一行。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ScreenLine {
    /// 那一行的文字。
    pub(crate) text: String,
    /// 着色段（默认样子不出段）；`color: false` ⇒ 不给。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) spans: Option<Vec<Span>>,
}

/// 一段着色：按字符列计（`from` 含、`to` 不含）；属性只在开着时出。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Span {
    /// 起（含）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: usize,
    /// 止（不含）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) to: usize,
    /// 前景：16 色名（`red` · `bright-red` …）或 `#rrggbb`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fg: Option<String>,
    /// 背景：同 `fg`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) bg: Option<String>,
    /// 粗。
    #[serde(skip_serializing_if = "is_off")]
    #[cfg_attr(test, ts(optional, type = "true"))]
    pub(crate) bold: bool,
    /// 暗。
    #[serde(skip_serializing_if = "is_off")]
    #[cfg_attr(test, ts(optional, type = "true"))]
    pub(crate) dim: bool,
    /// 斜。
    #[serde(skip_serializing_if = "is_off")]
    #[cfg_attr(test, ts(optional, type = "true"))]
    pub(crate) italic: bool,
    /// 下划线。
    #[serde(skip_serializing_if = "is_off")]
    #[cfg_attr(test, ts(optional, type = "true"))]
    pub(crate) underline: bool,
    /// 反显。
    #[serde(skip_serializing_if = "is_off")]
    #[cfg_attr(test, ts(optional, type = "true"))]
    pub(crate) inverse: bool,
}

fn is_off(b: &bool) -> bool {
    !*b
}

/// `terminal-input` 的回包：送到了 · 不知道送没送到（别重发）· 被拒（原因码 ＋ 那一句；画面变了带新指纹）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Sent {
    /// 结局。
    pub(crate) result: SentResult,
    /// `refused` 的原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) why: Option<t::Refused>,
    /// `refused` 时给人看的那一句（后端写好）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) said: Option<String>,
    /// `screen_changed` 时带的新指纹。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) screen: Option<String>,
    /// `session-say` 那一下终端已经没了、按记下的号恢复了这条会话、那一句当第一句交了出去 ⇒ `true`（只在这一支出现）。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub(crate) resumed: bool,
}

/// 送字 / 送键的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS, Deserialize))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum SentResult {
    /// 送到了。
    Delivered,
    /// 不知道送没送到（别重发）。
    Unsure,
    /// 没送（`why` 说为什么）。
    Refused,
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

impl Blocking for TerminalsListCmd {
    fn run(req: TerminalsListReq, _cx: &Cx) -> Result<TerminalsList, TerminalsListErr> {
        // 名单这一条登记成「tmux 可选」（没装 ⇒ 空名单，不是错）：装了却起不来那一种（下层回 `no_tmux`）说成「列不出」，
        //   不在这一条的码表里另立一个 `no_tmux`（`target_parity_guard` 钉着两句不许自相矛盾）。
        t::list_on(On::default(), &req).map_err(|f| match f.code.as_str() {
            "no_tmux" => TerminalsListErr::Unobservable(f.into_said()),
            _ => typed::of_fail(f, TerminalsListErr::Unobservable),
        })
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

impl Blocking for TerminalPreviewCmd {
    fn run(req: TerminalPreviewReq, cx: &Cx) -> Result<Preview, TerminalPreviewErr> {
        t::preview_on(On::default(), &req, &cx.tz)
            .map_err(|f| typed::of_fail(f, TerminalPreviewErr::CaptureFailed))
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

impl Blocking for TerminalInputCmd {
    fn run(req: TerminalInputReq, _cx: &Cx) -> Result<Sent, TerminalInputErr> {
        input(&req).map_err(|f| typed::of_fail(f, TerminalInputErr::Unobservable))
    }
}

/// 送字 / 送键（生产：默认 socket）。计划退回那一条也经它送（`faces/plan_return_face.rs`）。
pub(crate) fn input(req: &TerminalInputReq) -> Result<Sent, CmdErr> {
    t::input_on(On::default(), req)
}
