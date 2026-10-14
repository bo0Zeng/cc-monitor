//! 会话族那几条帧命令的**回包**线上形（`terminal-input` · `session-new` · `launch-render-cli` · `session-new-facts` · `session-model` ·
//! `terminals-list` · `terminal-preview` · `kill` · `history-tail`）—— 两侧同一份：后端按这里的类型回，
//! 手机嵌的 Rust 核心按这里的类型解（[`decode_reply`]），界面用从这里导出的类型（`src/frontend/ui/generated/`）。
//! 入参与码还住后端（`control/terminals_wire.rs` 等）：手机交的入参不经这里。

use serde::{Deserialize, Serialize};

// 生成物目录按本包根的绝对路径写（`concat!(env!("CARGO_MANIFEST_DIR"), …)`）：后端那一侧的生成类型嵌着这里的几样
// （`Forked` 嵌 `ForkLaunch` · `ModeSwitched` 嵌 `ModeSwitchResult` …），导出时两个包算的落点与 import 路径要是同一处；
// 写成相对路径的话是相对各自那一包的导出根，深度不同就落到仓外去。

/// 送不了 / 结束不了的原因（`terminal-input` 被拒的 `why` · 名单 `can.*` 里 `{no, said}` 的 `no`）。
/// 线上照码那一形（小写下划线，与同族命令级码同一写法；手机端也吃这两格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Refused {
    /// 别的前端的会话。
    NotYours,
    /// 不归我们管（不是我们起的）。
    NotManaged,
    /// 那个 tmux 会话里还有别的窗口（结束会连带关掉）。
    OtherWindows,
    /// 名单里此刻没有它。
    NotKnown,
    /// 不止一个终端挂着这个 sid。
    Ambiguous,
    /// 送的那一刻它已经没了。
    Ended,
    /// 画面变了（带新指纹）。
    ScreenChanged,
    /// 那台说这条会话在等人（批准 · 回答）：此刻打的字会落进那个框里（`session-say` 才回它）。
    Waiting,
    /// 这台 tmux 里没有它的终端、可另有活进程在写它（开在 tmux 之外的终端里）：不另起一个（`session-say` 才回它）。
    Elsewhere,
    /// 终端已经没了，按记下的号恢复这条会话没成（`session-say` 才回它；原话记日志）。
    NotResumed,
}

/// `terminal-input` 的回包：送到了 · 不知道送没送到（别重发）· 被拒（原因码 ＋ 那一句；画面变了带新指纹）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Sent {
    /// 结局。
    pub result: SentResult,
    /// `refused` 的原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<Refused>,
    /// `refused` 时给人看的那一句（后端写好）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub said: Option<String>,
    /// `screen_changed` 时带的新指纹。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub screen: Option<String>,
    /// `session-say` 那一下终端已经没了、按记下的号恢复了这条会话、那一句当第一句交了出去 ⇒ `true`（只在这一支出现）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub resumed: bool,
}

/// 送字 / 送键的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum SentResult {
    /// 送到了。
    Delivered,
    /// 不知道送没送到（别重发）。
    Unsure,
    /// 没送（`why` 说为什么）。
    Refused,
}

/// `terminals-list` 的回包。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TerminalsList {
    /// 终端名单（每行一个终端）。
    pub terminals: Vec<TerminalItem>,
    /// `false` ＝ 名单里有读不懂的行（画「部分」）。
    pub complete: bool,
}

/// 名单里的一个终端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TerminalItem {
    /// 名单里那一行的不透明句柄（前端不拼、不解析；送字 / 抓屏时交回）。
    pub terminal: String,
    /// 终端宿主（这一版是 `tmux`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"tmux\""))]
    pub host: String,
    /// tmux 会话名。
    pub tmux_name: String,
    /// 里面跑着会话、前台不是 shell 时是窗格标题；否则是前台程序名。
    pub title: String,
    /// 前台程序名。
    pub program: String,
    /// 当前目录。
    pub cwd: String,
    /// 里面跑着会话（`@ccm_sid`）时才有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub session: Option<TermSession>,
    /// 谁起的。
    pub started_by: StartedBy,
    /// 此刻连着它的终端客户端；空 ＝ 后台。
    pub clients: Vec<Attached>,
    /// 输入方式：`shared`（tmux：各端都能打字）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"shared\""))]
    pub input: String,
    /// 此刻的样子。
    pub state: TermState,
    /// 最近动静（秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub last_activity: u64,
    /// 这个调用方能做什么（抓屏不过身份门、恒可用，不在这里）。
    pub can: TerminalCan,
}

/// 名单一行里跑着的会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TermSession {
    /// 会话 id。
    pub sid: String,
    /// 哪一家（没标就缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<String>,
}

/// 谁起的这个终端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct StartedBy {
    /// 会话上的 `@ccm_client`（没声明 ⇒ `null`）。
    pub client: Option<String>,
    /// 这个调用方过不过身份门（与 `can.input` 是不是 `true` 同一个判定；结束还要看 `can.end`）。
    pub mine: bool,
}

/// 连着一个终端的客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Attached {
    /// 客户端种类（`terminal-window`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"terminal-window\""))]
    pub kind: String,
    /// 连上的时刻（秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub since: u64,
    /// 最近动静（秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub last_activity: u64,
}

/// 名单一行此刻的样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum TermState {
    /// 前台不是 shell。
    Running,
    /// 没会话、前台是 shell。
    Idle,
    /// 有会话、前台是 shell（程序已退出）。
    ProgramExited,
}

/// 这个调用方能对这一行做什么。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TerminalCan {
    /// 能不能送字 / 送键。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "true | { no: \"not_yours\" | \"not_managed\", said: string }")
    )]
    pub input: Ability,
    /// 能不能结束。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(
            type = "true | { no: \"not_yours\" | \"not_managed\" | \"other_windows\", said: string }"
        )
    )]
    pub end: Ability,
}

/// 能 ⇒ 线上 `true`；不能 ⇒ `{no: 原因码, said: 给人看的那一句}`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ability {
    Yes,
    No { no: Refused, said: String },
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AbilityWire {
    Yes(bool),
    No { no: Refused, said: String },
}

impl Serialize for Ability {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Ability::Yes => AbilityWire::Yes(true),
            Ability::No { no, said } => AbilityWire::No {
                no: *no,
                said: said.clone(),
            },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Ability {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match AbilityWire::deserialize(d)? {
            AbilityWire::Yes(true) => Ok(Ability::Yes),
            AbilityWire::Yes(false) => Err(serde::de::Error::custom(
                "an ability is `true` or `{no, said}`, never `false`",
            )),
            AbilityWire::No { no, said } => Ok(Ability::No { no, said }),
        }
    }
}
/// `terminal-preview` 的回包（终端实时预览每一帧也是它）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Preview {
    /// 这一屏的指纹（16 位十六进制）：内容一变就变，送字时带回来。
    pub screen: String,
    /// 列数。
    pub cols: u32,
    /// 行数。
    pub rows: u32,
    /// 光标。
    pub cursor: Cursor,
    /// 自上而下的行（往回要的在最前）。
    pub lines: Vec<ScreenLine>,
    /// 实际往回给了几行。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub scrollback_lines: usize,
    /// 要的比上限多、截到了上限。
    pub capped: bool,
    /// 抓屏时刻（秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub captured_at: u64,
    /// 抓屏时刻在看的那一台钟上的（请求的 `tz`）`HH:MM:SS`（界面照抄、不换算）。
    pub captured_at_text: String,
}

/// 光标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Cursor {
    /// 列。
    pub x: u32,
    /// 行。
    pub y: u32,
    /// 看得见。
    pub visible: bool,
}

/// 一屏里的一行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ScreenLine {
    /// 那一行的文字。
    pub text: String,
    /// 着色段（默认样子不出段）；`color: false` ⇒ 不给。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub spans: Option<Vec<Span>>,
}

/// 一段着色：按字符列计（`from` 含、`to` 不含）；属性只在开着时出。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Span {
    /// 起（含）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: usize,
    /// 止（不含）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: usize,
    /// 前景：16 色名（`red` · `bright-red` …）或 `#rrggbb`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub fg: Option<String>,
    /// 背景：同 `fg`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub bg: Option<String>,
    /// 粗。
    #[serde(default, skip_serializing_if = "is_off")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub bold: bool,
    /// 暗。
    #[serde(default, skip_serializing_if = "is_off")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub dim: bool,
    /// 斜。
    #[serde(default, skip_serializing_if = "is_off")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub italic: bool,
    /// 下划线。
    #[serde(default, skip_serializing_if = "is_off")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub underline: bool,
    /// 反显。
    #[serde(default, skip_serializing_if = "is_off")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub inverse: bool,
}

fn is_off(b: &bool) -> bool {
    !*b
}

/// 这一趟实际用的号（应答里的 `account`；账号 0 / 不指定 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct LaunchedAccount {
    pub name: String,
    pub config_dir: String,
    /// 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
    pub model: Option<String>,
}

/// `launch-render-cli` · `launch-local` 的应答：那一行 `ccm …` ＋ 实际用的号（账号 0 / 不指定 ⇒ `null`）。
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Rendered {
    /// 那一行 `ccm …`。
    pub cmd: String,
    /// 实际用的号（账号库里的那一形；别的 ⇒ `null`）。
    pub account: Option<LaunchedAccount>,
}

/// 第一句交进会话的结局（同 `terminal-input` 的应答那几格）。第一句作为启动参数交给那一家 ⇒ 起好了就是 `delivered`（起不成整条失败、走不到这里）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct FirstSay {
    /// `delivered` · `unsure`（不知道送没送到，别重发）· `refused`。
    pub result: String,
    /// `refused` 的原因（同 `terminal-input`）；别的 ⇒ `null`。
    pub why: Option<String>,
    /// `refused` 时给人看的那一句（写好）；别的 ⇒ `null`。
    pub said: Option<String>,
}

/// 结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum SessionNewOutcome {
    /// 在这台 tmux 里后台起好了（`session` 是会话名）。
    Started,
    /// 要开一个终端窗口跑 `cmd`（窗口由 monitor 开）。
    Open,
}

/// `session-new` 的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SessionNew {
    pub outcome: SessionNewOutcome,
    /// tmux 会话名（`started` 那一形；开窗那一形 ⇒ `null`）。
    pub session: Option<String>,
    /// 分叉出来的新会话 sid · 新起时这台先定好的 sid（那一家认先定 sid）；那一家不认 ⇒ `null`（报到之前说不出）。
    pub sid: Option<String>,
    /// 开窗那一形要跑的那一行（`started` ⇒ `null`）。
    pub cmd: Option<String>,
    /// 实际用的号（账号 0 / 不指定 ⇒ `null`）。
    pub account: Option<LaunchedAccount>,
    /// 起的是哪一家。
    pub agent: String,
    /// 起在哪个目录（`~` 已按这台的家目录展开：认报到的会话按它）。
    pub cwd: String,
    /// 第一句交进去的结局；没给第一句 ⇒ `null`。
    pub prompt: Option<FirstSay>,
}

/// `session-new-facts` 的应答。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct NewFacts {
    /// 这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个）。
    pub recent: Vec<RecentDir>,
    /// 这台有没有 tmux（`false` ⇒ 只能开终端窗口）。
    pub tmux: bool,
    /// 这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序），各带名字与可选的模型 · 权限档。
    pub agents: Vec<AgentFacts>,
    /// 没给 `forkOf` ⇒ `null`。
    pub fork: Option<ForkFacts>,
}

/// 最近用过的一个工作目录。
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct RecentDir {
    /// 工作目录。
    pub cwd: String,
    /// 那个目录最近一次会话的修改时刻（毫秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub last_ms: i64,
    /// 同一刻按请求的 `tz` 写给人看的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub last_text: String,
}

/// 这台能起的一家。
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct AgentFacts {
    /// 线上的 kind（交回 `session-new.agent` 的那一格）。
    pub kind: String,
    /// 这一家写给人看的名字。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: String,
    /// 起会话可选的模型（不选 ＝ 跟着那一家自己的默认）；这一家没声明 ⇒ 空。
    pub models: Vec<Pick>,
    /// 换模型抽屉的第一项「默认」：会话开着时交 `session-model` 这个值 ⇒ 回到那一家自己的缺省模型（起会话不交它，不选就是默认）；
    /// 这一家没声明 ⇒ `null`。
    pub model_default: Option<Pick>,
    /// 抽屉底那一行：换模型的副作用（那一家自己的行为，例如也改这个号以后新会话的默认）；没有 ⇒ `null`。
    pub model_note: Option<String>,
    /// 起会话可选的权限档（不给越过批准那一档）；这一家没有 ⇒ 空。
    pub permission_modes: Vec<Pick>,
}

/// 一个可选值：交回去的原值 ＋ 写给人看的字。
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "NewSessionPick", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Pick {
    /// 原值（`session-new` 的 `model` / `permissionMode` 收的就是它）。
    pub value: String,
    /// 写给人看的字。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: String,
}

/// 分叉源会话的那几格。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct ForkFacts {
    /// 源会话是哪一家（说不出 ⇒ 空串）。
    pub agent: String,
    /// 起分叉会话要的三格（同 `session-fork` 的 `launch`）。
    pub launch: ForkLaunch,
    /// `at` 那一条在第几轮（没给 / 说不出 ⇒ `null`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub turn: Option<u64>,
    /// 那一轮那句在看的那一台钟上的（请求的 `tz`） `HH:MM`（说不出 ⇒ `null`）。
    pub start_text: Option<String>,
}

/// 「知道」那一格从哪知道：会话记录 · 源会话的进程 · 终端名单。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ForkWhence", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum ForkWhence {
    Record,
    Process,
    TerminalList,
}

/// 「不知道」那一格为什么：源会话已退出 · 记录里没有工作目录 · 活着但说不出号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ForkWhy", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum ForkWhy {
    Exited,
    NoCwd,
    LiveNoAccount,
}

/// 一格：知道（值 ＋ 从哪知道）或不知道（为什么）。线上 `{kind:"known", value, from}` · `{kind:"unknown", why}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ForkSlot", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForkSlot<T> {
    Known { value: T, from: ForkWhence },
    Unknown { why: ForkWhy },
}

/// 源会话在哪个终端：某个终端（句柄）· 不在任何终端里。线上同会话容器那一格：`{host:"tmux", terminal}` · `{host:"none"}`
/// （两个 `host` 词与 `stream::wire::TerminalHost::as_wire` · `HOST_NONE` 同字，判据钉着）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ForkTerminal", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "host")]
pub enum ForkTerminal {
    #[serde(rename = "tmux")]
    Hosted { terminal: String },
    #[serde(rename = "none")]
    None,
}

/// 三格推断的结果（线上 `session-fork` 回复的 `launch`，`session-new-facts` 的 `fork.launch` 同一形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ForkLaunch", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ForkLaunch {
    pub cwd: ForkSlot<String>,
    /// 值 `null` ＝ 账号 0，串 ＝ 号名。
    pub account: ForkSlot<Option<String>>,
    pub terminal: ForkSlot<ForkTerminal>,
}

/// 切档的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "ModeSwitchResult", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum SwitchResult {
    /// 读回来就是那一档。
    Done,
    /// 不知道切没切到（认不出 · 不在循环里 · 按了没变 · 按键结果未知）。
    Unsure,
    /// 没按（`why` 说为什么）。
    Refused,
}

/// `session-model` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ModelSwitched {
    /// 结局（同切权限档那一组）。
    pub result: SwitchResult,
    /// 读回的显示名（那一家写的原样，例如 `Opus 5.5 (1M context)`）；没读到 ⇒ `null`。
    pub model: Option<String>,
    /// 读回时那一家顺带说的思考档（原样）；没说 ⇒ 不出。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub effort: Option<String>,
    /// `refused` 的原因（同 `terminal-input`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<Refused>,
    /// 给人看的那一句（换成了 · 没变 · 结果未知 · 被拒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub said: Option<String>,
}

/// `kill` 的回包：杀成了才有（没杀成走失败信封）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Killed {
    /// 杀掉的 tmux 会话名。
    pub session: String,
    /// 杀成了（恒 `true`；界面照它确认，不当成功之外的任何东西）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "true"))]
    pub killed: bool,
    /// 顺手从 cc-bus 注销的结局。
    pub bus: BusNote,
}

/// 注销结局的线上形（`kill` 回包的 `bus` · 批量停每一项的那一格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BusNote {
    /// 注销掉了的 id。
    pub removed: Vec<String>,
    /// 没注销成的那几个（`why` 是原话）。
    pub failed: Vec<BusFailed>,
    /// 名册读不到（原话）⇒ 登记在上面的一个都没注销；读得到 ⇒ `null`。
    pub unread: Option<String>,
    /// 给人看的那几行（没有 ⇒ `null`）。
    pub said: Option<String>,
    /// 复制详情（没有 ⇒ 空串）。
    pub detail: String,
}

/// 没注销成的一个。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BusFailed {
    /// 那个 id。
    pub id: String,
    /// 原话。
    pub why: String,
}

/// 尾段那一趟扫描的结果：可计行总数 · 尾段起点行号 · 两段的字节边界。
///
/// 帧面那条（`history-tail`）只要**这张图**，正文按字节区间另走 `history-read` 分页拉 —— 一帧应答装不下几十 MB 的会话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct TailPlan {
    /// 可计行总数（`line_counts` 口径）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total: u64,
    /// 尾段第一行的行号。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub tail_from: u64,
    /// 尾段第一行的字节起点（＝ 头段的字节终点）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub split_at: u64,
    /// 最后一个完整行（`\n` 收尾）之后的字节位置；torn 残尾不在任何一段里。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub end: u64,
}

// ── 客户端那一侧：按类型解回包 ─────────────────────────────────────────

fn canon<T: Serialize + serde::de::DeserializeOwned>(
    data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let typed: T = serde_json::from_value(data).map_err(|e| e.to_string())?;
    serde_json::to_value(&typed).map_err(|e| e.to_string())
}

/// 一张表出两样：客户端按类型解回包的 [`decode_reply`] · 判据拿来对后端 `impl Command` 的 [`reply_names`]（同一张表，不会一边多一条）。
macro_rules! replies {
    ($($cmd:literal => $ty:ty,)*) => {
        /// 回包类型住这里的那几条命令（按命令名排）。
        pub const COMMANDS: &[&str] = &[$($cmd),*];

        /// 客户端那一侧（手机嵌的 Rust 核心）：`cmd` 的回包 `data` 按这里的类型解。
        /// 解得开 ⇒ `Some(Ok(按类型重写的那一份))`；形状对不上 ⇒ `Some(Err(serde 那一句))`；回包类型不住这里 ⇒ `None`（原样用）。
        pub fn decode_reply(
            cmd: &str,
            data: serde_json::Value,
        ) -> Option<Result<serde_json::Value, String>> {
            Some(match cmd {
                $($cmd => canon::<$ty>(data),)*
                _ => return None,
            })
        }

        /// `(命令, 解它用的那个类型的 TS 名)`：后端判据对着那条命令 `impl Command` 的回包类型核（同一个名 ⇒ 同一个类型）。
        #[cfg(any(test, feature = "ts"))]
        pub fn reply_names() -> Vec<(&'static str, String)> {
            let cfg = ts_rs::Config::default();
            vec![$(($cmd, <$ty as ts_rs::TS>::name(&cfg)),)*]
        }
    };
}

replies! {
    "history-tail" => TailPlan,
    "kill" => Killed,
    "launch-render-cli" => Rendered,
    "session-model" => ModelSwitched,
    "session-new" => SessionNew,
    "session-new-facts" => NewFacts,
    "terminal-input" => Sent,
    "terminal-preview" => Preview,
    "terminals-list" => TerminalsList,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表里每一条都解得动（名字没写错），表外的不碰。
    #[test]
    fn every_listed_command_is_decoded_and_others_pass_through() {
        for cmd in COMMANDS {
            assert!(
                decode_reply(cmd, serde_json::Value::Null).is_some(),
                "{cmd}"
            );
        }
        assert!(decode_reply("history-list", serde_json::json!({})).is_none());
    }

    /// 缺一格 ⇒ 解不开；多出来的格不拦（回包不 deny_unknown_fields：新版多一格不算对不上，形状指纹另管）。
    #[test]
    fn a_missing_cell_is_refused_and_the_rewrite_is_the_same_bytes() {
        let tail = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30});
        assert_eq!(decode_reply("history-tail", tail.clone()), Some(Ok(tail)));
        let short = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10});
        assert!(matches!(decode_reply("history-tail", short), Some(Err(_))));
        let refused = serde_json::json!({"result": "refused", "why": "not_yours", "said": "x"});
        assert_eq!(
            decode_reply("terminal-input", refused.clone()),
            Some(Ok(refused))
        );
        assert!(matches!(
            decode_reply("terminal-input", serde_json::json!({"result": "maybe"})),
            Some(Err(_))
        ));
    }

    /// 能做那一格：`true` ⇄ 能；`{no, said}` ⇄ 不能；`false` 不是合法的一形。
    #[test]
    fn ability_is_true_or_a_reason() {
        let yes: Ability = serde_json::from_value(serde_json::json!(true)).unwrap();
        assert_eq!(yes, Ability::Yes);
        let no: Ability =
            serde_json::from_value(serde_json::json!({"no": "not_managed", "said": "x"})).unwrap();
        assert_eq!(
            serde_json::to_value(&no).unwrap(),
            serde_json::json!({"no": "not_managed", "said": "x"})
        );
        assert!(serde_json::from_value::<Ability>(serde_json::json!(false)).is_err());
    }
}
