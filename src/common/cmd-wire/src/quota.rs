//! `quota-read` 的回包线上形（这台的额度账：每号的显示态 · 写好的几行 · 开窗那一判 · 号名 / 位名表）—— 两侧同一份：
//! 后端照这里的类型回（判定 · 拼字 · 按钟写时刻字都在后端），手机嵌的 Rust 核心按这里的类型解（`replies::decode_reply`），
//! 界面用从这里导出的类型（`src/frontend/ui/generated/`）。

use crate::cells::{Tone, Words};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// 这个号此刻拿不拿得到能用的登录：订阅号看凭据文件 ＋ 账号身份（`ok` / `needsLogin`）；按量号看这台 key 表里有没有它（`ok` / `needsKey`）。
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
    /// `resets_at` 写给人看的样子（出口那一下按钟写，`SlotShow::stamp`；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub resets_at_text: Option<Words>,
    /// `resets_at` 距今（`+1h50m`；同上，只在还没到时有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub resets_at_rel_text: Option<Words>,
    /// 用满：这个窗口用到 100%、还没重置（画 `✕`；被拒而没用满画「{pct}% · 被拒」）。没用满 ⇒ 缺。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub full: bool,
    /// 这一格的字（唯一写法 `slot_words`：超额在兜 `超额` · 上一窗已过 / 没数 `—` · 用满 `✕` · 被拒没用满 `58% · 被拒` · 否则 `58%`）。
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
    /// 按钮上那个窗口的语义位（卡着的那个；回包没说 ⇒ 用得最多的那个）；没有分窗口的数 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub limiting: Option<String>,
    /// `5h` · `7d` 各一格（有数的才出）。
    pub slots: Vec<SlotShow>,
    pub login: LoginState,
    /// 同一订阅的稳定标识（账号身份的散列，不含原值）：两台看到的同一订阅它相同。订阅号读得出身份才有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sub_id: Option<String>,
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
    /// 被拒时几点重置（`↻19:00`，出口那一下按钟写，`QuotaShow::stamp`）；没有 ⇒ 缺。
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
    /// 重置那一刻（只给出口那一下写 `reset`，不上线）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip, optional, type = "number"))]
    pub resets_at: Option<u64>,
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
            resets_at: None,
        }
    }
}

/// 一个窗口（照原名）：窗口键 · 取整的百分比 · 几点重置 · 几点、从哪看到的 · 已重置未计时。
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
    /// `resets_at` 写给人看的样子（出口那一下按钟写，`WindowShow::stamp`；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub resets_at_text: Option<Words>,
    /// `resets_at` 距今（同上，只在还没到时有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub resets_at_rel_text: Option<Words>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seen_at: u64,
    /// `seen_at` 写给人看的样子（同上）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub seen_at_text: Option<Words>,
    /// `seen_at` 距今（别台的钟走在前面时才有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "string"))]
    pub seen_at_rel_text: Option<Words>,
    pub from: Source,
    /// 重置时刻已过、之后没再看到：上次的数不再作数（用量当 0、窗口没开）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub reset_since_seen: bool,
}

/// 成品上的 `names` 那一格：`accounts` 只列与原名不同的号（今天只有 `_`），`slots` 列每个语义位。
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
    pub accounts: BTreeMap<String, Words>,
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
    /// `at` 距今。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at_rel_text: Option<Words>,
    /// 为什么这样办（「5h 83% ↻14:20」「被拒（7d）↻10-12 09:00」「无采样 · 发一句才知道」…）。
    pub text: Words,
}

/// `quota-read` 的应答：这台的额度账。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 每个出过数的号一条，按 `(agent, account)` 排。
    pub accounts: Vec<QuotaAccount>,
    /// 账号库里有、额度账上从没出过数的号。
    pub unseen: Vec<QuotaUnseen>,
    /// 此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜。
    pub usable_now: Vec<String>,
    /// 换号 · 加号那几处的候选：出过数的在前、没出过的在后，`_` 不在里面（`is_candidate`）。界面只照它画，不另拼、不另滤。
    pub candidates: Vec<Candidate>,
    /// 被拒 / 超额在兜的号里最早回来的那个；没有、或都说不出时刻 ⇒ `null`。
    pub earliest_return: Option<EarliestReturn>,
    /// 读不出 / 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`（`quota_rows::head_text`）。
    pub text: Option<Words>,
    /// 读不出 ⇒「5h 那一格」写好的字（`5h 读不到`，`quota_rows::head_five_hour`）；否则 `null`（每号自己那一格）。
    pub five_hour: Option<Words>,
    /// 号名 / 位名表（出口画号名 · 位名只照它）。
    pub names: Names,
}

/// 出过数的一个号：账上那一条的原数 ＋ 显示态（`AccountHead`）＋ 照它写好的几格。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub head: AccountHead,
    pub rows: Rows,
    /// 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）；按量号 ⇒ `null`。
    pub five_hour: Option<Words>,
    pub warm: Warm,
}

/// 出过数的一个号除写好的几行之外的那一截（行与开窗那一判照它的线上样子算）。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub agent: String,
    pub account: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seen_at: u64,
    pub seen_at_text: Words,
    /// 别台的钟走在前面时才有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub seen_at_rel_text: Option<Words>,
    /// 那一条账的额度快照（原数照原样；时刻旁边的字已添）。
    pub reading: ReadingShown,
    /// 各窗口几点、从哪看到的（原数照原样；账上没记 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub windows_seen: Option<BTreeMap<String, WindowSeenShown>>,
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub show: QuotaShow,
}

/// 账上那一条的额度快照（`QuotaReading`）＋ 时刻旁边写好的字（`TextClock::text` · `rel`：有时刻才添、`…RelText` 只给还没到的）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaReadingShown",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ReadingShown {
    /// 限流器的结论；回包里没说 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub status: Option<QuotaStatus>,
    /// 那一发被上游拒了（没被服务）。
    pub refused: bool,
    /// 此刻卡着的那个窗口名。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub limiting: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub resets_at: ResetsAt,
    pub windows: Vec<WindowReadShown>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub overage: Option<OverageShown>,
}

/// 「几点重置」那一格与它旁边写好的字（三格都可缺：没有时刻 ⇒ 三格都缺；过了 ⇒ 没有 `resetsAtRelText`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaResetsAt",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ResetsAt {
    /// 几点重置（unix 秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub resets_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub resets_at_text: Option<Words>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub resets_at_rel_text: Option<Words>,
}

/// 快照里的一个窗口（`QuotaWindow`）＋ 重置时刻旁边写好的字。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaWindowRead",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct WindowReadShown {
    pub name: String,
    /// 已用比例（通常 0–1，可以超过 1）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub used: Option<f64>,
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub resets_at: ResetsAt,
    /// 越过了哪条预警线（比例）；没越过就没有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub warned_at: Option<f64>,
}

/// 超额那一档（`QuotaOverage`）＋ 重置时刻旁边写好的字。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaOverageShown",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct OverageShown {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub status: Option<QuotaStatus>,
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub resets_at: ResetsAt,
    /// 为什么不可用（那一家的原值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub disabled: Option<String>,
    /// 这一发正在用超额。
    pub in_use: bool,
}

/// 一个窗口几点 · 从哪看到的（`WindowSeen`）＋ 那一刻旁边写好的字。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaWindowSeen",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct WindowSeenShown {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    pub at_text: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at_rel_text: Option<Words>,
    pub from: Source,
}

/// 没出过数的一个号。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct QuotaUnseen {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub head: UnseenHead,
    pub rows: Rows,
    /// 没出过数 ⇒ 恒 `null`（这一格只给出过数的订阅号）。
    pub five_hour: Option<Words>,
    pub warm: Warm,
}

/// 没出过数的一个号除写好的几行之外的那一截。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub agent: String,
    pub account: String,
    pub kind: Kind,
    pub login: LoginState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sub_id: Option<String>,
    /// 用量那一格（没出过数 ⇒ `5h —` / `按量`；同出过数的号那一格一个写法，只是没有重置那一格）。
    pub usage: UnseenUsage,
}

/// 没出过数的号用量那一格：`Usage` 去掉重置那一格（没出过数就说不出几点重置）。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub agent: String,
    pub account: String,
    pub kind: Kind,
}

/// 被拒 / 超额在兜的号里最早回来的那个。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"),
    ts(
        rename = "QuotaEarliestReturn",
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct EarliestReturn {
    pub account: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    pub at_text: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at_rel_text: Option<Words>,
}
