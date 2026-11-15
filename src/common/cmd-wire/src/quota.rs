//! `quota-read` 的回包线上形（这台的额度账：每号的显示态 · 写好的几行 · 开窗那一判 · 位名表）—— 两侧同一份：
//! 后端照这里的类型回（判定 · 拼字 · 按钟写时刻字都在后端），手机按这里生成的 Kotlin 类解，
//! 界面用从这里导出的类型（`src/frontend/ui/generated/`）。

use crate::cells::{Tone, Words};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 语义位「5 小时」（显示态 `slot` · 窗口键 · 策略设置与叫醒里的 `w` 都照它）。
pub const SLOT_5H: &str = "5h";
/// 语义位「7 天」（分档的窗口键是 `7d:<档>`）。
pub const SLOT_7D: &str = "7d";

/// 限流器对一发的结论（各家的值翻成这三档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum QuotaStatus {
    Allowed,
    /// 还能用，但越过了某条预警线。
    Warning,
    Rejected,
}

/// 号的种类：线上 `sub` · `api`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaKind",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum Kind {
    Sub,
    Api,
}

/// 一个窗口的数从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaSource",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum Source {
    /// 中转经手的回包头。
    Headers,
    /// 官方客户端报的用量（`quota-probe`）。
    Usage,
}

/// 额度账那份文件读到的三态（`quota-read` 的 `state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaReadState",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "lowercase")]
pub enum ReadState {
    /// 读得懂。
    Present,
    /// 还没看到过任何回包。
    Absent,
    /// 文件读不出来 / 家推不出来。
    Unreadable,
}

/// 显示态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum QuotaState {
    Ok,
    Near,
    Refused,
    OverageInUse,
    Unseen,
    ResetSinceSeen,
}

/// 这个号此刻拿不拿得到能用的登录：订阅号看凭据文件 ＋ 账号身份（`ok` / `needsLogin` / `baseUntouched`）；按量号看这台 key 表里有没有它（`ok` / `needsKey`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum LoginState {
    Ok,
    /// 订阅号：要重新登录。
    NeedsLogin,
    /// 按量号：这台 key 表里没有它的 key（只对按量号出）。
    NeedsKey,
    /// 订阅号：那一家声明账号 0 不由 cc-monitor 续，这个账号 0 的令牌又快过期 / 过期了（不是要重新登录，等那一家自己续）。
    BaseUntouched,
}

/// 一个语义位（`5h` / `7d`）的那一格：取整的百分比 · 几点重置（说不出 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SlotShow {
    pub slot: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub pct: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    /// 用满：这个窗口用到 100%、还没重置（画 `✕`；被拒而没用满画「{pct}% · 被拒」）。没用满 ⇒ 缺。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub full: bool,
    /// 这一格的字（唯一写法 `slot_words`：超额在兜 `超额` · 那一窗过了重置 `读取中` / `已重置` · 没数 `—` · 用满 `✕` · 被拒没用满 `58% · 被拒` · 否则 `58%`）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: Words,
    /// 那一格的语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 卡着的窗口快满 ⇒ `warn`；否则 `plain`。
    #[serde(default = "plain")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"plain\" | \"fail\" | \"warn\"")
    )]
    pub tone: Tone,
    /// 这一格那个窗口「1% ≈ 多少用量」写好的字（`1% ≈ 2.1M`，同 [`WindowShow::per_pct_text`]）；量不到 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub per_pct_text: Option<Words>,
}

fn plain() -> Tone {
    Tone::Plain
}

/// 一个号的显示态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct QuotaShow {
    pub kind: Kind,
    pub state: QuotaState,
    /// 最后一次看到距今超过 30 分钟（与 `state` 叠着画）。
    pub stale: bool,
    /// `5h` · `7d` 各一格（有数的才出）。
    pub slots: Vec<SlotShow>,
    pub login: LoginState,
    /// 各窗口照原名一格（`slots` 那两格照留：同一语义位几个窗口并成一格；这里一个窗口一格）；没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<Vec<WindowShow>>")
    )]
    pub windows: Vec<WindowShow>,
    /// 这个号用量那一格（状态栏按钮 · 切号下拉 · 「下一个」那一格同一份）：`usage_of` 写。
    pub usage: Usage,
}

/// ★ 一个号用量那一格：窗口 · 值 · 几点重置 · 连成的一句 · 语气（唯一一处；出口照抄、不按显示态挑色）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Usage {
    /// 画的是哪个语义位（卡着的那个；回包没说 ⇒ `5h`）；按量号 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub slot: Option<String>,
    /// 那个语义位的字（`5h`）；按量号 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub window: Option<Words>,
    /// 用量那一格（`63%` · `✕` · `超额` · `—` · `被拒` · `按量`）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub value: Words,
    /// 被拒时几点重置（`↻19:00`，后端写好显示态那一下按钟写）；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub reset: Option<Words>,
    /// 连成一句（`5h 63%` · `5h ✕ ↻19:00` · `按量`）：下拉项右侧 · 切换那颗按钮。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: Words,
    /// 语气：用满 · 被拒 ⇒ `fail`；超额在兜 · 快满 ⇒ `warn`；否则 `plain`。
    #[serde(default = "plain")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"plain\" | \"fail\" | \"warn\"")
    )]
    pub tone: Tone,
}

impl Default for Usage {
    fn default() -> Self {
        Usage {
            slot: None,
            window: None,
            value: Words::default(),
            reset: None,
            text: Words::default(),
            tone: Tone::Plain,
        }
    }
}

/// 一个窗口（照原名）：窗口键 · 取整的百分比 · 几点重置 · 几点、从哪看到的 · 1% ≈ 多少用量 · 已重置未计时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct WindowShow {
    /// 那一家的窗口名（`five_hour` · `seven_day` · `seven_day_<模型>` · …）。
    pub name: String,
    /// 窗口键（`5h` · `7d` · `7d:<模型>`；轮换的上限 · 单段预算按它配）；不算用量窗口的（超额）⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub key: Option<String>,
    /// 取整的百分比（已重置未计时 ⇒ 0）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub pct: Option<u32>,
    /// 几点重置；有 ＝ 这个窗口在计时。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    pub from: Source,
    /// 这个号这个窗口「1% ≈ 多少用量」（折合输入 token：各类 token 按那一家的价比折）：中转数到的每发用量 ÷ 读数涨的点数；
    /// 量不到（读数没动 · 样本太少 · 只有一次读数 · 量出来超过 7 天）⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub tokens_per_pct: Option<u64>,
    /// `tokens_per_pct` 写好的字（`1% ≈ 2.1M`）；量不到 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub per_pct_text: Option<Words>,
    /// 这一窗口的语气（`rows` 里更细窗口那一行的值照它上色）：用满 ⇒ `fail`；快满 ⇒ `warn`；否则 `plain`。
    #[serde(default = "plain")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"plain\" | \"fail\" | \"warn\"")
    )]
    pub tone: Tone,
    /// 这一窗在这个号的 `rows` 里是第几行（手机计量条照那一行画）；没成行（没有窗口键 · 没有数）⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub row: Option<u32>,
}

/// 成品上的 `names` 那一格：每个语义位的名字（号名在每号那一行的 `label`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaNames",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Names {
    pub slots: BTreeMap<String, Words>,
}

/// 行里的一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaCell",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Cell {
    pub text: Words,
    pub tone: Tone,
}

pub type Rows = Vec<Vec<Cell>>;

/// 开窗那一判的一个号：`send`（发一句让 5h 窗口开始计时）· `wait`（睡到 `at`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaWarmAct",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub enum WarmAct {
    Send,
    Wait,
}

/// `warm`：quota-warm 照它办（只执行、不判）。`at` 只在 `wait` 时有（旁边由出口写好 `atText`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaWarm",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct Warm {
    pub act: WarmAct,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub at: Option<i64>,
    /// `at` 写给人看的样子（`Warm::stamp`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at_text: Option<Words>,
    /// 为什么这样办（「5h 83% ↻14:20」「被拒（7d）↻10-12 09:00」「无采样 · 发一句才知道」…）。
    pub text: Words,
}

/// `quota-read` 的应答：这台的额度账。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaRead {
    /// 那份文件读到的三态。
    pub state: ReadState,
    /// 只在 `unreadable` 时有：为什么读不出来（那一句不带原话）。
    pub reason: Option<String>,
    /// 只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
    pub detail: Option<String>,
    /// 那份文件的绝对路径（家推不出来时 `null`）。
    pub path: Option<String>,
    /// 这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub now: u64,
    /// 每个出过数的号一条，按 `(agent, account)` 排（不按能不能发重排：额度一变就重读，按状态排的话卡片会跳位）。
    pub accounts: Vec<QuotaAccount>,
    /// 账号库里有、额度账上从没出过数的号。
    pub unseen: Vec<QuotaUnseen>,
    /// 换号 · 加号那几处的候选：出过数的在前、没出过的在后，`_` 不在里面（`is_candidate`）。界面只照它画，不另拼、不另滤。
    pub candidates: Vec<Candidate>,
    /// 读不出 / 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`（`quota_rows::head_text`）。
    pub text: Option<Words>,
    /// 位名表（出口画位名只照它）。
    pub names: Names,
    /// 「现在还能不能发」那两行写好的字（用量页的结论 · 全满时那一句）；读不出 / 一个号都没有 ⇒ `null`（那时看 `text`）。
    pub verdict: Option<Verdict>,
}

/// 「现在还能不能发」：大字一句 · 小字一句（哪几个号能发 · 最早几点回来；说不出 ⇒ 缺）· 语气（能发 `plain`、都不能发 `fail`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaVerdict",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Verdict {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub text: Words,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub sub: Option<Words>,
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"plain\" | \"fail\" | \"warn\"")
    )]
    pub tone: Tone,
}

/// 出过数的一个号：账上那一条的原数 ＋ 显示态（`AccountHead`）＋ 照它写好的几格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub head: AccountHead,
    pub rows: Rows,
    /// 几点采的那一句（「采样于 12:03」）：不成行，出口放进这个号的悬停提示。
    pub sampled: Words,
    pub warm: Warm,
}

/// 出过数的一个号除写好的几行之外的那一截（行与开窗那一判照它的线上样子算）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaAccountHead",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct AccountHead {
    pub agent: crate::AgentKind,
    pub account: String,
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub show: QuotaShow,
}

/// 没出过数的一个号。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaUnseen {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub head: UnseenHead,
    pub rows: Rows,
    pub warm: Warm,
}

/// 没出过数的一个号除写好的几行之外的那一截。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaUnseenHead",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct UnseenHead {
    pub agent: crate::AgentKind,
    pub account: String,
    pub kind: Kind,
    pub login: LoginState,
    /// 用量那一格（没出过数 ⇒ `5h —` / `按量`；同出过数的号那一格一个写法，只是没有重置那一格）。
    pub usage: UnseenUsage,
}

/// 没出过数的号用量那一格：`Usage` 去掉重置那一格（没出过数就说不出几点重置）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaUnseenUsage",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct UnseenUsage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub window: Option<Words>,
    pub value: Words,
    pub text: Words,
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"plain\" | \"fail\" | \"warn\"")
    )]
    pub tone: Tone,
}

impl From<Usage> for UnseenUsage {
    fn from(u: Usage) -> Self {
        UnseenUsage {
            slot: u.slot,
            window: u.window,
            value: u.value,
            text: u.text,
            tone: u.tone,
        }
    }
}

/// 一个候选号：哪一家 · 哪个号 · 种类（加号那一处要分按量号）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaCandidate",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub agent: crate::AgentKind,
    pub account: String,
    pub kind: Kind,
}

/// 登录失败之后还能怎么办（`login-start` · `login-finish` 失败的 `data.again`）：由后端照码定，界面照它画，不按码去猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub enum LoginAgainHow {
    /// 作业还在，接着粘（粘的框留着）。
    Paste,
    /// 作业没了，从头再起一趟（`login-start`）。
    Restart,
    /// 没路：再来一趟也是同样的结果（没有这个号 · 这一家不在 app 里登 …）。
    None,
}

/// 推给发起它的那条连接的那一帧的小成品（`changed {login, key: 作业号, body}`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub enum SignInState {
    /// 登成了。
    Done,
    /// 登不成：那一句（原话进 `detail`）· 还能怎么办（作业自己收了 ⇒ 恒 `restart`）。
    Failed {
        said: String,
        detail: Option<String>,
        again: LoginAgainHow,
    },
}

/// `quota-read` 的入参（没有格）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct QuotaReadReq {}
