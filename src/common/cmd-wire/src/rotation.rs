//! 轮换那几条命令（`rotation-*`）的回包与失败带的 `data` 的线上形 —— 两侧同一份：后端照这里的类型回（判 · 推演 · 拼字都在后端），
//! 手机嵌的 Rust 核心按这里的类型解（`replies::decode_reply`），界面用从这里导出的类型（`src/frontend/ui/generated/`）。
//! 策略文件头里那份设置声明（`FormCell` 那几样）与换号记录（`RotLog`）后端也拿来读写盘上那几份，同一个形。

use crate::cells::{Tone, Words};
use crate::quota::{Kind, LoginState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一个窗口（`5h` / `7d`）的事实：在计时吗 · 用了多少（0–100）· 几点重置 · 用速（%/分，没根据 ⇒ 缺）。
/// 一个语义位有几个窗口（按模型分的 7 天额度）⇒ 取用得最多的那个。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct WinFact {
    pub open: bool,
    pub used: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets: Option<u64>,
    /// `resets` 写给人看的样子（按看的那一台的时区）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub resets_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rate: Option<f64>,
    /// 这一窗「1% ≈ 多少用量」（折合输入 token；同额度账那一格）：量不到 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub tokens_per_pct: Option<u64>,
}

/// 事实上用不了的原因（跟设置无关的那几种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum FactWhy {
    /// 被拒（429 等），到 `until` 恢复。
    Refused,
    /// 订阅号满了、在付费超额上兜着。
    Overage,
    /// 某个窗口用到 100%。
    Used,
    /// 订阅号要重新登录。
    Login,
    /// 按量号这台没有 key。
    Key,
}

/// 用不了：为什么 · 几点回来（说不出 ⇒ 缺）· 写好的那一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct FactBlocked {
    pub why: FactWhy,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub until: Option<u64>,
    pub text: String,
}

/// 一个号此刻的事实。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Fact {
    /// 路由第 2 段。
    pub account: String,
    pub kind: Kind,
    pub w5h: WinFact,
    pub w7d: WinFact,
    /// 有周上限：读数里有周用量那一格（没有 · 没出过数 ⇒ 缺）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub weekly: bool,
    pub login: LoginState,
    /// 额度账上最后一次看到它（没出过数 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub seen_at: Option<u64>,
    /// 事实上用不了 ⇒ 有；能用 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub blocked: Option<FactBlocked>,
}

/// 格的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "RotFormKind",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum FormKind {
    /// 0–100 的数。
    Percent,
    /// 分钟数（整数）。
    Minutes,
    /// 开 / 关。
    Toggle,
    /// 几个里选一个（线上 `choice`）。
    #[serde(rename = "choice")]
    OneOf,
    /// 24 小时条上的分段：每个窗口一组 `[{from, to, v}]`（一天里的分钟）。
    DaySegments,
}

/// 作用范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormScope {
    /// 总体设置那一块画一份（值在 `settings[key]`）。
    All,
    /// 每个号展开以后画一份（值在 `settings.accounts[i][key]`）。
    Each,
}

/// `choice` 的一项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormChoice {
    pub v: String,
    pub label: StrategyText,
    /// 号取这一项时号名旁的记号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<FormMark>,
}

/// 号名旁的记号：一个字形，或号牌画虚线框。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormMark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dashed: bool,
}

/// 一格只在另一格取某个值时出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormShowIf {
    pub key: String,
    pub v: Value,
}

/// 一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormCell {
    pub key: String,
    pub kind: FormKind,
    pub scope: FormScope,
    /// 只对 `scope: each`：号没另设就跟总体（总体那一份在 `settings[key]`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inherit: bool,
    pub label: StrategyText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<StrategyText>,
    /// 只对 `daySegments`：哪几个窗口（`5h` · `7d`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    pub default: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<FormChoice>,
    /// 能是空（`null`）：空的那一项怎么叫（例如「不回切」）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nullable: Option<StrategyText>,
    /// 数格前面那半句。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<StrategyText>,
    /// 只在另一格取某个值时出。
    #[serde(default, rename = "showIf", skip_serializing_if = "Option::is_none")]
    pub show_if: Option<FormShowIf>,
    /// 只对 `daySegments`：排在后面的窗没另设 ⇒ 同第一窗。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow: bool,
}

/// 头里能写成一句、也能分语言写的字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StrategyText {
    One(String),
    Langs {
        zh: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        en: Option<String>,
    },
}

/// 一条换号记录：只记后端看得见的事；策略那一句 `why` 跟着存一份（显示用，后端不解）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(rename = "RotLog", export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct RotLog {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    pub from: String,
    pub to: String,
    pub cause: RotLogCause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<String>,
}

/// 后端看得见的换号原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "RotLogCause",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum RotLogCause {
    /// 策略交的先后换了第一个。
    Order,
    /// 在用的号事实上发不出去了。
    Unusable,
    /// 照用户拖出来的顺序。
    Temp,
    /// 用户点了「现在用」。
    UseNow,
    /// 停发（`from == to`），到 `until`。
    Hold {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
        until: Option<u64>,
    },
    /// 策略出错，走了兜底。
    Error,
}

/// 策略说的一个号：一句为什么。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryRow {
    pub account: String,
    pub why: String,
}

/// 这一判。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryDecision {
    /// 用哪个号（停发 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub pick: Option<String>,
    /// 停发到几点 · 到时恢复的号（停发才有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub hold_until: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hold_account: Option<String>,
    /// 策略交的先后（此刻愿意用的号）。
    pub order: Vec<String>,
    /// 每个号一句（策略写的）。
    pub why: Vec<RotTryRow>,
    /// 这一号来自用户拖出来的顺序。
    pub from_temp: bool,
    pub skipped: Vec<String>,
    pub temp: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub next_node: Option<u64>,
    /// 策略这一判出错（走了兜底）：原话。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub strategy_error: Option<String>,
}

/// 「用哪个号」那一条的一段。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryUse {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub account: Option<String>,
    pub from_temp: bool,
    /// 在用的号用到上限的时刻还不知道（额度账里量不到它的用速）：这一段只排确定的事件。
    pub cap_unknown: bool,
    pub skipped: Vec<String>,
}

/// 每个号那一行的一段。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryLaneSeg {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    /// 事实上发得出去吗（被拒 · 登录 · key · 超额 · 用到 100 都算发不出去）。
    pub sendable: bool,
    /// 这一段开头两窗的用量（5 小时 · 7 天）。
    pub used: [f64; 2],
    pub reset: bool,
    /// 这一段写的字：发不出去 ⇒ 空；重置那段 `↻18:30`；此刻那段 `剩 40%`。
    pub text: String,
    /// 这一段开头剩多少（事实：100 − 两窗里用得多的那个，不低于 0）。
    pub left: f64,
}

/// 一个号那一行。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryLane {
    pub account: String,
    pub segs: Vec<RotTryLaneSeg>,
}

/// 一次换号。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTrySwitch {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    /// `at` 的钟面（`HH:MM`）。
    pub at_text: String,
    pub account: String,
}

/// 时间轴上的一格刻度：整点 · 钟面。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryTick {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    pub text: String,
}

/// 推演。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryTimeline {
    /// 将来每次换到别的号（时间轴上方那几个「16:07 work」）。
    pub switches: Vec<RotTrySwitch>,
    pub uses: Vec<RotTryUse>,
    pub lanes: Vec<RotTryLane>,
    pub truncated: bool,
    /// 此刻之后的整点刻度（推 12 小时以内每 2 小时、更长每 4 小时，看的那一台的钟）。
    pub ticks: Vec<RotTryTick>,
}

/// `rotation-try` 的应答。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationTryReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub now: u64,
    /// 这份策略的说明（原文开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段）。
    pub description: String,
    /// 给了 `settings` ⇒ 换上这份设置以后的整份原文（头重写、代码原样），存成文件就是它；没给 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub source: Option<String>,
    /// 试跑「现在用」碰上用不了：几点恢复（说不出 ⇒ `null`）；这时 `decision` 是不换的那一判。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number | null"))]
    pub use_now_refused: Option<Option<u64>>,
    pub decision: RotTryDecision,
    pub timeline: RotTryTimeline,
}

/// 读不懂的那一份为什么：第几行（说得出才有）· 原话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotStrategyError {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub line: Option<usize>,
    pub msg: String,
}

/// 策略目录里的一份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotStrategyItem {
    pub id: String,
    /// 说明（文件开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段；读不懂 ⇒ 空）。标题就是 `id`（文件名，§2.1）。
    pub description: String,
    /// 说明的第一行（下拉里那行灰字；没有 ⇒ 空）。
    pub summary: String,
    pub rev: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub error: Option<RotStrategyError>,
    /// 此刻还活着、用着它的会话。
    pub in_use: Vec<String>,
    pub for_new: bool,
    /// 这份的设置说明写法不合文案规矩（设置照样能用）。
    pub words_bad: bool,
    /// 文件头里没写时区（分时段按 UTC 算）。
    pub tz_unknown: bool,
    /// 不是照编辑器那一份存出来的（代码或声明跟它此刻的不一样）。
    pub hand: bool,
}

/// `rotation-strategies` 的应答。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategiesReply {
    /// 照 id 排。
    pub strategies: Vec<RotStrategyItem>,
    pub for_new: String,
    /// `strategies/v3/` 里没有加载的第三版脚本几份。
    pub old_scripts: usize,
}

/// 提示（第一行下面那几条）：停发 · 策略出错 · 时区不明 · 旧格式没加载 · 说明写法不合规 · 策略改过了。字照文案表写。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotHint {
    /// 停发：到几点 · 到时恢复的号。
    Hold {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
        until: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        account: Option<String>,
        /// `until` 的钟面（`HH:MM`，看的那一台的时区；不在今天带日期）。
        #[serde(rename = "untilText", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        until_text: Option<String>,
    },
    /// 策略出错（这一判走了兜底：此刻的号发得出去就照用，否则照号列表的先后）。`path` ＝ 那份文件在那台上的路径（文件不在 ⇒ 缺）：「看代码」照它开。
    StrategyError {
        error: RotStrategyError,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        path: Option<String>,
    },
    /// 文件头里没写时区：分时段按 UTC 算。`zone` ＝「设时区」设成的那一个：看的那一台的时区名（请求信封的 `tz`；
    /// `local` ⇒ 这台系统的时区名）；那一台说不出 ⇒ `UTC`（此刻就是按它算的）。
    TzUnknown { zone: String },
    /// `strategies/v3/` 里有没加载的第三版脚本：几份 · 哪几份（「看是哪几份」列它们）。
    OldScripts { n: usize, files: Vec<RotOldFile> },
    /// 会话那一形：策略文件在它载入之后改过了，这个会话还在用它载入的那一版（「载入新版」）。
    Newer,
    /// 设置页那一形：还在用这份旧版的活会话有几个（「都载入新版」）。
    Older { n: usize },
    /// 这份策略的设置说明写法不合文案规矩。
    WordsBad,
    /// `rotation.json` 读不懂（照上一份读懂的接着用，盘上那份不动）：那一句。
    BookUnreadable { text: String },
}

/// `strategies/v3/` 里没加载的一份：文件名 · 在那台上的路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotOldFile {
    pub name: String,
    pub path: String,
}

/// 过去那一截最上那条的一段（照这个会话的换号记录）：号 ⇒ 用它；缺 ⇒ 停发。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotPastUse {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub account: Option<String>,
}

/// 过去那一截每号一行的一段：发得出去吗 · 开头是不是刚开了新窗 · 写的字（重置那段 `↻HH:MM`，别的空）。
/// 不带「剩多少」：只知道此刻的读数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotPastSeg {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    pub sendable: bool,
    pub reset: bool,
    pub text: String,
}

/// 过去那一截一个号那一行（说不出的那几截不在 `segs` 里：空着）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotPastLane {
    pub account: String,
    pub segs: Vec<RotPastSeg>,
}

/// 时间轴过去那一截（`from` 到此刻）：最上那条 · 每号一行，号的先后同推演那几行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotPast {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    pub uses: Vec<RotPastUse>,
    pub lanes: Vec<RotPastLane>,
}

/// 一条提示：哪一种（按钮照它挂）· 语气（`need` ＝ 需手动）· 写好的那一句（停发那一句里 `{dur}` 是「还有多久」，界面在画的那一刻照 `now` · `until` 填）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotNote {
    pub hint: RotHint,
    pub tone: Tone,
    pub text: String,
    /// 复制详情（出错那一条带原话；别的没有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
}

/// 一条换号记录 ＋ 为什么那一句（策略写的；没写 ⇒ 后端看得见的原因）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotLogRow {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub log: RotLog,
    pub because: String,
}

/// `rotation-view` 的应答。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationViewReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub now: u64,
    /// `now` 的钟面（`HH:MM`，看的那一台的时区）。
    pub now_text: String,
    /// 会话那一形：这条会话叫什么（同历史清单那一格：改过的 ＞ 标题 ＞ 第一句；这台记录树里找不到 ⇒ 会话 id 前 8 位）；设置页 ⇒ `null`。
    pub title: Option<String>,
    /// 用的那份策略；会话不轮换（没选策略）⇒ `null`，这时 `decision` · `timeline` 也是 `null`、只用开会话的号。
    pub strategy: Option<RotStrategyItem>,
    /// 设置那几格照声明解好（此刻的值 · 从哪来 · 字）：会话 ⇒ 它那份副本里的；设置页 ⇒ 文件此刻的。
    pub board: RotBoard,
    /// 开会话用的号 · 此刻在用的号。
    pub start: String,
    pub current: String,
    /// 会话那一形的顺序（设置页 ⇒ 空）：第一格 ＝ 在用的号，往下是接下来先用谁（拖出来的 · 策略交的先后），此刻用不了的与先不用的沉底。
    pub lineup: Vec<String>,
    /// 这个会话先不用的号。
    pub off: Vec<String>,
    /// 每个号的事实 ＋ 号牌，照号列表的先后（列表里没有的接在后面）。
    pub accounts: Vec<RotAccount>,
    /// 号列表里有几个（`accounts` 前这么多个是列表里的）。
    pub listed: usize,
    pub decision: Option<RotTryDecision>,
    pub timeline: Option<RotTryTimeline>,
    /// 时间轴过去那一截（会话才有；设置页没有会话 ⇒ `null`）。
    pub past: Option<RotPast>,
    /// 换号记录（会话才有）。
    pub log: Vec<RotLogRow>,
    pub hints: Vec<RotNote>,
}

/// 收不下的那几格（`rotation-settings`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotRefused {
    pub cells: Vec<RotBadCell>,
}

/// 一格收不下：哪一格 · 短码（`type` · `range` · `of` · `unknown` · `dup` · `account` · `overlap` · `order`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotBadCell {
    pub cell: String,
    pub code: String,
}

/// 别处改过：此刻的版本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotConflict {
    pub rev: String,
}

/// 什么都不回的应答。
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotDone {}

/// `rotation-temp` 的应答：改完的那一排。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationTempReply {
    pub temp: Vec<String>,
    pub off: Vec<String>,
}

/// 「现在用」碰上那个号事实上发不出去：为什么 · 几点恢复（说不出 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotUnusable {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<FactWhy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub until: Option<u64>,
}

/// 写成的那一份。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotStrategyId {
    pub id: String,
}

/// 读不懂：第几行（说得出才有）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotBadStrategy {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub line: Option<usize>,
}

/// `rotation-settings` · `rotation-strategy-tz` 的应答：写成以后的版本。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotStrategyRev {
    pub rev: String,
}

/// 删不了：哪几个活会话在用（空 ⇒ 是新会话用的那份）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotInUse {
    pub sids: Vec<String>,
}

/// `rotation-facts` 的应答。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationFactsReply {
    /// 这台此刻（unix 秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub now: u64,
    /// 照账号库的先后。
    pub accounts: Vec<Fact>,
}

/// `rotation-view` 里的一个号：事实 ＋ 号牌。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotAccount {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub fact: Fact,
    pub plate: RotPlate,
}

/// 一个号的号牌：号色（`--acct-c0..c7` 第几格，同 `accounts-list` 那一格）· 号名（人眼里叫什么，同额度那几处）· 号牌字（同 `accounts-list` 那一格）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotPlate {
    pub color: u8,
    pub label: Words,
    pub text: Words,
}

/// 一格此刻的值从哪来：自己设的 · 跟总体（号那一行没设、声明说跟总体）· 声明里的默认 · 同第一窗（分段后面几窗没另设）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotFrom {
    Own,
    All,
    Default,
    Same,
}

/// 24 小时条上的一段：一天里的分钟 `[from, to)` · 值 · 段上写的字 · 悬停那一句。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotDaySeg {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    pub v: f64,
    pub text: String,
    pub tip: String,
}

/// `choice` 的一项（字已挑好）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotChoice {
    pub v: String,
    pub label: String,
}

/// 设置的一格（24 小时条每窗一格），照声明解好：此刻的值 · 从哪来 · 字。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotCell {
    pub key: String,
    pub kind: FormKind,
    /// 24 小时条那一格是哪一窗（`5h` · `7d`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub window: Option<String>,
    /// 标签（24 小时条那一格带窗名）。
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub help: Option<String>,
    /// 此刻生效的值（`null` ＝ 可空的那一格选了空的那一项；24 小时条 ⇒ `null`，段在 `segs`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub v: Value,
    /// 数格框里写的数（值是空的那一项 ⇒ 声明默认 · 下限 · 0 里头一个是数的）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub num: Option<f64>,
    pub from: RotFrom,
    /// 能是空（`null`）：空的那一项怎么叫。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub nullable: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<RotChoice>>"))]
    pub choices: Vec<RotChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<RotDaySeg>>"))]
    pub segs: Vec<RotDaySeg>,
    /// 数格前面那半句。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub lead: Option<String>,
    /// 这一格只在那一格取某值时出：挂在它下面（那一格的 `key`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub under: Option<String>,
    /// 分段后面那几窗：能「同第一窗」，那颗按钮的字（「同 5h」）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub same: Option<String>,
}

/// 号名旁的一个记号（照声明里那一项）：字形或虚线框 · 悬停那一句（「切入 · 强制切入」）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotMark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub glyph: Option<String>,
    pub dashed: bool,
    pub tip: String,
}

/// 号列表的一行：号 · 展开头一行（两窗用量 · 重置）· 号牌悬停那一句 · `scope: each` 那几格（只回此刻该出的）· 那几格写成的句子（会话里只看的规则摘要，一格一句）· 号名旁的记号。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotRow {
    pub account: String,
    pub line: String,
    pub hover: String,
    pub cells: Vec<RotCell>,
    pub rules: Vec<String>,
    pub marks: Vec<RotMark>,
}

/// 设置那几块：总体设置（`scope: all`）· 号列表每一行（照 `settings.accounts` 的先后，与 `accounts` 前 `listed` 个一一对应）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotBoard {
    pub all: Vec<RotCell>,
    pub rows: Vec<RotRow>,
}
