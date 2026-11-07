//! 轮换那几条命令（`rotation-*`）的回包与失败带的 `data` 的线上形 —— 两侧同一份：后端照这里的类型回（判 · 推演 · 拼字都在后端），
//! 手机按这里生成的 Kotlin 类解，界面用从这里导出的类型（`src/frontend/ui/generated/`）。
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
    /// `resets` 写给人看的样子（按看的那一台的时区；后端写行用，不上线）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
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
    /// 额度账上最后一次看到它（没出过数 ⇒ 缺；后端自己用，不上线）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
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
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum RotFormKind {
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
    pub kind: RotFormKind,
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    /// 那一判看的事实摘要（事后核每一次为什么换）；没记 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub saw: Option<RotSaw>,
}

/// 一判看的事实摘要：各号的 5 小时窗 ＋ 本会话量出来的；量不到的格缺。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct RotSaw {
    pub accounts: Vec<RotSawAcct>,
    /// 本会话用速：折合输入 token / 分。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub pace: Option<f64>,
    /// 本会话用速的起伏（σ/μ）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub spread: Option<f64>,
    /// 本会话上下文（token）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub context: Option<u64>,
}

/// 摘要里的一个号：5 小时用量 · 重置时刻 · 用速（点 / 分）· 别的会话压在它上面的用速（点 / 分）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct RotSawAcct {
    pub id: String,
    pub used5h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets5h: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rate5h: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub others: Option<f64>,
}

/// 后端看得见的换号原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
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
    /// 停发到几点（停发才有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub hold_until: Option<u64>,
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
    pub skipped: Vec<String>,
    /// 这一段写的字：号名 · 停发那一句。
    pub text: String,
}

/// 一个号那一行。
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTryLane {
    pub account: String,
    pub segs: Vec<RotTrySeg>,
}

/// 推演里每号一行的一段：事实上发得出去吗（被拒 · 登录 · key · 超额 · 用到 100 都算发不出去）·
/// 开头是不是刚开了新窗 · 写的字（发不出去 ⇒ 空；重置那段 `↻HH:MM`；此刻那段 `剩 40%`；这个会话先不用 ⇒ 「不用」）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotTrySeg {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    pub sendable: bool,
    pub reset: bool,
    pub text: String,
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
    /// 这份的设置说明写法不合文案规矩（设置照样能用）：只用来出 `RotHint::WordsBad` 那一条提示，不上线。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
    pub words_bad: bool,
    /// 文件头里没写时区（分时段按 UTC 算）。
    pub tz_unknown: bool,
    /// 不是照编辑器那一份存出来的（代码或声明跟它此刻的不一样）。
    pub hand: bool,
}

/// `rotation-strategies` 的应答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        /// `until` 的钟面（`HH:MM`，看的那一台的时区；不在今天带日期）：后端写提示那一句用，不上线。
        #[serde(skip)]
        #[cfg_attr(any(test, feature = "ts"), ts(skip))]
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

/// 会话顺序里一行落在哪一组：在用（第一格）· 接下来可用（可挪）· 此刻用不了 · 这个会话先不用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotGroup {
    Use,
    Next,
    Out,
    Off,
}

/// 会话顺序里的一行：号 · 哪一组 · 右格写的字（这个号在顺序里此刻怎样；「下一个」带的钟点由同一份推演出，推不准就不写）·
/// 策略写的那一句为什么（名下那行灰字；没写 ⇒ 缺）· 能不能拖 · 不能拖时那一句为什么（悬停 · 读屏念它）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotLine {
    pub account: String,
    pub group: RotGroup,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<String>,
    pub movable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub stay: Option<String>,
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
    /// 用的那份策略；会话不轮换（没选策略）⇒ `null`，这时 `decision` 也是 `null`、只用开会话的号。
    pub strategy: Option<RotStrategyItem>,
    /// 设置那几格照声明解好（此刻的值 · 从哪来 · 字）：会话 ⇒ 它那份副本里的；设置页 ⇒ 文件此刻的。
    pub board: RotBoard,
    /// 开会话用的号 · 此刻在用的号。
    pub start: String,
    pub current: String,
    /// 会话那一形的顺序（设置页 ⇒ 空）：第一格 ＝ 在用的号，往下是接下来先用谁（拖出来的 · 策略交的先后），此刻用不了的与先不用的沉底。
    pub lineup: Vec<RotLine>,
    /// 会话那一形：这个会话有拖出来的顺序（「照策略排」清掉它）。
    pub manual: bool,
    /// 这个会话先不用的号。
    pub off: Vec<String>,
    /// 每个号的事实 ＋ 号牌，照号列表的先后（列表里没有的接在后面）。
    pub accounts: Vec<RotAccount>,
    /// 号列表里有几个（`accounts` 前这么多个是列表里的）。
    pub listed: usize,
    pub decision: Option<RotTryDecision>,
    /// 推演（设置页那一形才有：改一格就看得到接下来怎么换；会话那一形 · 不轮换 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub timeline: Option<RotTryTimeline>,
    /// 换号记录（会话才有）。
    pub log: Vec<RotLogRow>,
    pub hints: Vec<RotNote>,
    /// 会话那一形此刻能做的几样（设置页 ⇒ 缺）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub can: Option<RotCan>,
}

/// 一个会话在顺序那一块能做的几样：拖着排先后（「拖动排序」那一句跟着它）· 勾掉「先不用」一个号 · 编辑它载入的那份策略。
/// 后端照这个会话轮不轮换判一次（`rotation_view_face`）；界面只照它摆，不看 `strategy` 空不空。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct RotCan {
    pub reorder: bool,
    pub off: bool,
    pub edit: bool,
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
pub struct RotStale {
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

/// `rotation-view` 里的一个号：事实 ＋ 号牌 ＋ 用满了的那几窗（`5h` · `7d`；号牌上那条小计量照它画满）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotAccount {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub fact: Fact,
    pub plate: RotPlate,
    pub full: Vec<String>,
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

/// 24 小时条上的一段：一天里的分钟 `[from, to)` · 那两刻写成钟点（一段一行那两格里的字，`24:00` 是一整天的末尾）·
/// 值 · 段上写的字 · 悬停那一句。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotDaySeg {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub to: u64,
    pub from_text: String,
    pub to_text: String,
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
    pub kind: RotFormKind,
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
    /// 这一格能［恢复默认］（自己设的；分段后面那几窗另设的不算，它们照［同第一窗］回去）：两端照它出那颗按钮。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub reset: bool,
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

/// 号列表的一行：号 · 它在设置号列表里是第几行 · 展开头一行（两窗用量 · 重置；这台账号库没有这个号 ⇒ 照实说）· 号牌悬停那一句 · `scope: each` 那几格（只回此刻该出的）· 那几格写成的句子（会话里只看的规则摘要，一格一句）· 号名旁的记号。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotRow {
    pub account: String,
    /// 这一行在 `settings.accounts` 里的下标：改设置 · 移出 · 拖动一律交它（不拿显示的先后当行号）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub row: usize,
    pub line: String,
    pub hover: String,
    pub cells: Vec<RotCell>,
    pub rules: Vec<String>,
    pub marks: Vec<RotMark>,
    /// 这台账号库里没有这个号（号列表里留着的）：号列表那一节照它出「清掉库里没有的号」。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub missing: bool,
}

/// 设置那几块：总体设置（`scope: all`）· 号列表每一行（照 `settings.accounts` 的先后，一个号一行、各带它的行号；与 `accounts` 前 `listed` 个一一对应）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotBoard {
    pub all: Vec<RotCell>,
    pub rows: Vec<RotRow>,
}

// ── 入参（手机 · 界面照这些写，后端照这些收：`deny_unknown_fields`，可缺的格不写出）────────────────────

/// `rotation-facts` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct RotationFactsReq {
    /// 哪一家（线上的 kind）；缺 ⇒ 有账号库的那一家。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
}

/// `rotation-strategies` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategiesReq {}

/// 看哪一份：一个会话（会话面板，照它载入的副本）· 一份策略（设置页，照文件此刻的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotTarget {
    Session { sid: String },
    Strategy { strategy: String },
}

/// `rotation-view` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationViewReq {
    pub target: RotTarget,
    /// 设置页那一形往后推几小时（1–48，缺 ⇒ 12）；会话那一形不画推演，不看它。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hours: Option<u8>,
}

/// 分段里的一段（[`RotEdit::Seg`]）：`[from, to)` 是一段一行里打的钟点原样的字（后端照 `day_minute` 认）· 上限。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotSeg {
    pub from: String,
    pub to: String,
    pub v: f64,
}

/// 改设置的一个意图（`rotation-settings` 的 `edit`）：设一格 · 恢复默认 · 分段里改 / 加 / 删一段 · 同第一窗与否 · 拖号牌 · 加号 · 移出 · 清掉账号库里没有的号。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotEdit {
    /// 设一格（`row` 缺 ⇒ 总体那一格）。
    Set {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        row: Option<usize>,
        #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
        v: Value,
    },
    /// 恢复默认：那一格的键从那一行删掉（`row` 缺 ⇒ 从总体删；分段那一格几窗一起删），回到跟总体或声明里的默认。
    /// 不借 `set` 的 `v: null`：可空的那一格，`null` 是「设成空的那一项」。
    Reset {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        row: Option<usize>,
    },
    /// 分段那一窗此刻生效的段里：`at` ＋ `seg` ⇒ 第 `at` 段换成 `seg`；只 `at` ⇒ 删第 `at` 段；只 `seg` ⇒ 加一段（压过它盖住的）。写成那一行自己的。
    Seg {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        row: Option<usize>,
        window: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        at: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        seg: Option<RotSeg>,
    },
    /// 分段后面那一窗：`same` ⇒ 删掉自己的、同第一窗；否则把此刻生效的写成自己的（「另设」）。
    Same {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        row: Option<usize>,
        window: String,
        same: bool,
    },
    /// 拖号牌：第 `from` 行松在第 `to` 行上 ⇒ 排到它前头。
    Move { from: usize, to: usize },
    /// 加一个号（排在最后）。
    Add { account: String },
    /// 移出一行。
    Remove { row: usize },
    /// 号列表里这台账号库没有的号一并移出（开会话的号那一行与库里有的照留、照原先后）。
    Prune {},
}

/// `rotation-settings` 的入参：哪一份 · 改什么 · 改之前读到的版本（对不上 ⇒ `stale`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationSettingsReq {
    pub strategy: String,
    pub edit: RotEdit,
    pub if_rev: String,
}

/// `rotation-use` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationUseReq {
    pub sid: String,
    /// `null` ⇒ 这个会话不再轮换（只用开会话的号）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub strategy: Option<String>,
}

/// `rotation-reload` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationReloadReq {
    pub strategy: String,
}

/// `rotation-for-new` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationForNewReq {
    /// `null` ⇒ 新会话不轮换（只用开会话的号）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub strategy: Option<String>,
}

/// `rotation-strategy-add` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategyAddReq {
    /// 一份策略文件的原文（说明 ＋ 头 ＋ 代码）：从文件添加；缺 ⇒ 新建（照编辑器那一份，号列表 ＝ 账号库全部号，时区 ＝ 这台的）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub source: Option<String>,
    /// 存成的文件名（＝ 标题）。
    pub name: String,
}

/// `rotation-strategy-copy` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategyCopyReq {
    pub id: String,
    pub name: String,
}

/// `rotation-strategy-rename` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategyRenameReq {
    pub id: String,
    pub name: String,
    pub if_rev: String,
}

/// `rotation-strategy-tz` 的入参：那一份 · 设成哪个时区（IANA 名）· 改之前读到的版本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategyTzReq {
    pub id: String,
    pub tz: String,
    pub if_rev: String,
}

/// `rotation-strategy-delete` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationStrategyDeleteReq {
    pub id: String,
}

/// 会话侧的临时动作：顺序里把一行拖到第 `to` 格（0 起，第 0 格是在用的号）· 清掉 · 这个会话先不用 / 再用一个号。
/// 第几格由后端照这个会话此刻的顺序认（界面只交松手的那一格）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum RotTempOp {
    Move { account: String, to: usize },
    Clear,
    Off { account: String, off: bool },
}

/// `rotation-temp` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationTempReq {
    pub sid: String,
    pub op: RotTempOp,
}

/// `rotation-use-now` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationUseNowReq {
    pub sid: String,
    pub account: String,
}

/// `rotation-try` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RotationTryReq {
    /// 一份策略文件的原文（头 ＋ 代码）。
    pub source: String,
    /// 换掉头里的设置（形状由这份策略自己定；缺 ⇒ 用头里的）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub settings: Option<Value>,
    /// 开会话用的号（缺 ⇒ 事实里第一个号）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub start: Option<String>,
    /// 在用的号（缺 ⇒ 开会话用的号）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub current: Option<String>,
    /// 临时条目。
    #[serde(default)]
    pub temp: Vec<String>,
    /// 往后推几小时（1–48，缺 ⇒ 12）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hours: Option<u8>,
    /// 试跑「现在用这个号」：事实上发得出去 ⇒ 判的是换过去、放进拖出来的顺序最前之后的那一判；发不出去 ⇒ `useNowRefused` 带几点恢复。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub use_now: Option<String>,
    /// 哪一家（缺 ⇒ 有账号库的那一家）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
}
