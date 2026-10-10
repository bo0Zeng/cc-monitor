//! 轮换：额度满了（或到阈值）就把会话钉到轮换里下一个号上，不重启进程。落 `~/.cc-monitor/rotation.json`。
//!
//! - 每台一张规则表：一条规则 ＝ 一份有名字的轮换（顺序 · 勾了哪几个 · 换号时机 · 封顶 · 换法 · 最多等几分钟）；
//!   本机默认 ＝ 指向其中一条（`defaultRule`），永远至少一条；缺省那一条只有「起始账号」那一格 ⇒ 缺省不轮换。
//! - 每个会话的来源四选一：跟随默认 · 跟随父会话（链接：父此刻按哪份它就按哪份；谁起的谁只在 `lineage.rs`）·
//!   用某条规则（链接：规则改了它下一发就按新的走）· 本会话自己一份（换成别的来源时自己那一份留着）；按会话 id 存，后端重启后还在。
//! - 每个会话此刻钉在哪个号、从什么时候起、换号记录；会话换了起它的号（重启换号 / 换号恢复）⇒ 钉号随之清掉。
//! - 判「换不换、换谁」只在 [`super::decide`]；这里只有存取。

use crate::common::said::Said;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

/// 每个会话至多留几条换号记录（最早的先丢）。
pub(crate) const HISTORY_KEPT: usize = 32;

/// 触发那一行（`cap["*"]`）的线与单段预算收哪些值。
pub(crate) const THRESHOLD_RANGE: std::ops::RangeInclusive<u8> = 1..=99;
/// 每号覆盖的上限收哪些值：多一个 `0` ＝ 这个号（这一时段）不用，不管用量多少。
pub(crate) const CAP_RANGE: std::ops::RangeInclusive<u8> = 0..=99;
/// 「切兜底前最多等几分钟」收哪些值：`0` ＝ 不等（要切兜底就立刻切）。
pub(crate) const WAIT_RANGE: std::ops::RangeInclusive<u8> = 0..=120;
/// 「切兜底前最多等几分钟」缺省几分钟。
pub(crate) const WAIT_DEFAULT: u8 = 40;
/// 规则名至多几个字（按字符数）。
pub(crate) const RULE_NAME_MAX: usize = 24;

fn wait_default() -> u8 {
    WAIT_DEFAULT
}

/// 每号那一格里「这个号的所有窗口」的键；也是 `cap` · `stint` 里「所有号」那一行的键。
pub(crate) const ALL_WINDOWS: &str = "*";

/// 规则一级的线（`cap["*"]` 那一行）收哪几个语义位：5 小时一格、7 天一格（7 天那一格管所有 7 天窗口，含分档的）。
pub(crate) const LINE_SLOTS: [&str; 2] = ["5h", "7d"];

/// 此刻的号到了线、池里没有线下的号可换时怎么办。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum AtLimit {
    /// 软阈值（缺省）：留在此刻的号上照发；它真被拒时退一步取首个没被拒的号。
    #[default]
    Continue,
    /// 硬上限：不再发上游，回一份那一家自己认得的「用满」回包（重置时刻 ＝ 池里最早回到 N% 以下的那一刻）。
    Stop,
}

/// 轮换列表里的一格：起始账号占位，或一个号（路由第 2 段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum RotationSlot {
    /// `{"start": true}`：起这个会话的那个号。
    Start(StartSlot),
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StartSlot {
    pub start: bool,
}

/// 上限的一个值：一个数，或按时段的几段（此刻不落在任何一段 ⇒ 这一层没有值，落回下一层）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum CapValue {
    N(u8),
    Slots(Vec<CapSlot>),
}

/// 一段：`at` ＝ `"HH:MM-HH:MM"`（那台后端的本地钟，含起不含止，跨午夜可写，止可写 `24:00`）· 这一段的上限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CapSlot {
    pub at: String,
    pub n: u8,
}

impl CapSlot {
    /// 此刻（本地钟的一天里第几分钟）落不落在这一段；`at` 读不懂 ⇒ 不落。
    pub(crate) fn holds(&self, minute: u16) -> bool {
        match span_of(&self.at) {
            Some((from, to)) if from < to => (from..to).contains(&minute),
            Some((from, to)) => minute >= from || minute < to,
            None => false,
        }
    }
}

/// `"HH:MM-HH:MM"` → （起, 止）分钟；起止相同 · 读不懂 ⇒ `None`。
pub(crate) fn span_of(at: &str) -> Option<(u16, u16)> {
    let (a, b) = at.split_once('-')?;
    let (from, to) = (clock_of(a)?, clock_of(b)?);
    (from < 24 * 60 && from != to).then_some((from, to))
}

/// 每号的上限（线）：号 → 窗口键（或 `*` ＝ 这个号的所有窗口）→ 上限；号那一格是 `*` 的那一行 ＝ 所有号（规则一级的「触发」），
/// 只收语义位 `5h` · `7d`、值 `1..=99` 或按时段。没有那一窗的线 ＝ 那一窗满了才换。
pub type Caps = BTreeMap<String, BTreeMap<String, CapValue>>;

/// 每号的单段预算：号（或 `*` ＝ 所有号）→ 窗口键（或 `*`）→ 换进来之后再用几个点就想走。
pub type Stints = BTreeMap<String, BTreeMap<String, u8>>;

/// 一份轮换：顺序 · 勾了哪几个 · 线（触发那一行 ＋ 每号上限，都在 `cap`）· 每号单段预算 · 前面的号回来就切回 · 到上限没号可换时怎么办 · 最多等几分钟。
/// 起始账号占位恒算勾上。后三格缺省（空 · 空 · 关）时不写出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Rotation {
    pub order: Vec<RotationSlot>,
    pub enabled: Vec<String>,
    /// 盘上缺 ⇒ `continue`。
    #[serde(default)]
    pub at_limit: AtLimit,
    /// 线：`{号|"*": {窗口键|"*": n | [{at, n}]}}`；层次 这号这窗口 → 这号这语义位 → 这号全部窗口 → 所有号这语义位（`"*"` 那一行），时段外落回下一层。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Caps>"))]
    pub cap: Caps,
    /// 每号的单段预算（软的）：`{号: {窗口键|"*": n}}` ＝ 换进来之后再用 n 个点就想走。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Stints>"))]
    pub stint: Stints,
    /// 排在此刻的号前面的号又能用了 ⇒ 下一发切回去。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub preempt: bool,
    /// 兜底的号（`order` 里具名的那几个）：只在别的号都用不了、又等不到它们回来时才切过去。没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<String>>"))]
    pub fallback: Vec<String>,
    /// 切兜底前最多等几分钟：要切到兜底号时，非兜底的号（含此刻的）有一个在这么多分钟内回来 ⇒ 先停着等它，不切兜底。
    /// 往非兜底号切照旧立刻切；没标兜底 ⇒ 这一格不起作用。`0` ＝ 不等。盘上缺 ⇒ 缺省 40。
    #[serde(default = "wait_default")]
    pub wait: u8,
}

impl Default for Rotation {
    /// 缺省：只有起始账号 ⇒ 不轮换。
    fn default() -> Self {
        Self {
            order: vec![RotationSlot::Start(StartSlot { start: true })],
            enabled: Vec::new(),
            at_limit: AtLimit::Continue,
            cap: Caps::new(),
            stint: Stints::new(),
            preempt: false,
            fallback: Vec::new(),
            wait: WAIT_DEFAULT,
        }
    }
}

impl Rotation {
    /// 这个会话实际的轮换池（按序、去重）：占位换成起始账号，具名的只取勾上的。
    pub(crate) fn pool(&self, start: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for slot in &self.order {
            let a = match slot {
                RotationSlot::Start(_) => start,
                RotationSlot::Named(a) if self.enabled.iter().any(|e| e == a) => a.as_str(),
                RotationSlot::Named(_) => continue,
            };
            if !out.iter().any(|o| o == a) {
                out.push(a.to_string());
            }
        }
        out
    }
}

/// 为什么跳过一个号（换号记录与「切换」结果里的原因码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum Unready {
    /// 拿不到这个号能用的登录（没登录 · 续期失败 · 读不出账号身份）。
    NeedsLogin,
    /// 按量号在这台的 key 表里没有 key。
    NeedsKey,
    /// 这一发的请求体里账号身份那一格认不准，换不了。
    UnsureBody,
}

/// 为什么换（或为什么没换成）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SwitchWhy {
    /// 原号被拒（满了）；`w` 卡住的那个窗口的语义位（`5h` / `7d`），说不出 ⇒ 缺。
    Full {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[cfg_attr(test, ts(optional))]
        w: Option<String>,
    },
    /// 原号用量到了线：`n` 那条线 · `w` 那一窗的语义位（卡得最久的那一窗，与 `fromResetsAt` 同一窗；说不出 ⇒ 缺）。
    Threshold {
        n: u8,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[cfg_attr(test, ts(optional))]
        w: Option<String>,
    },
    /// 原号此刻那一格上限取到 0（时段停用）；`w` 那一格的语义位，整号停用或说不出 ⇒ 缺。
    Off {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[cfg_attr(test, ts(optional))]
        w: Option<String>,
    },
    /// 用户「现在就换」，不重启。
    ManualHot,
    /// 用户「现在就换」，重启。
    ManualRestart,
    /// 轮到这个号时跳过了它。
    Skipped { account: String, reason: Unready },
    /// 订阅号都满了，留在原号的付费超额上。
    ToOverage,
    /// 硬上限：池里没有线下的号，这一发没发上游（`fromResetsAt` ＝ 池里最早回到线下的那一刻）；`n` · `w` ＝ 卡着的那条线与那一窗（说不出 ⇒ `w` 缺）。
    Held {
        n: u8,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[cfg_attr(test, ts(optional))]
        w: Option<String>,
    },
    /// 这一段在原号上用完了单段预算：`w` 那个窗口（窗口键）换进来之后又用了 `n` 个点。
    Stint { w: String, n: u8 },
    /// 前面的号又能用了（`preempt`）⇒ 切回去。
    Preempt,
    /// 要往后换到 `instead` 时，排在它前面的 `account` 在「最多等几分钟」内就回来 ⇒ 先停着等它（这一发没发上游；
    /// `fromResetsAt` ＝ 它回来的那一刻）。
    Wait { account: String, instead: String },
    /// 在兜底号上、池里有非兜底的号又能用了 ⇒ 切到首个能用的那个（`to`），不管换法。
    LeaveFallback,
}

/// 一条换号记录。`from == to` 的是没换成的那几种（跳过 · 留在超额）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SwitchRecord {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    /// `at` 写给人看的样子（回包出口 `common::time::with_texts` 添；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    pub from: String,
    pub to: String,
    pub why: SwitchWhy,
    /// 那一刻原号几点重置（知道才有；记下就不随后来的数变）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub from_resets_at: Option<u64>,
    /// `from_resets_at` 写给人看的样子（回包出口 `common::time::with_texts` 添；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub from_resets_at_text: Option<String>,
}

/// 能不能不重启换号（会话一级的原因码；目标号接不上另在「切换」结果里说）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum InPlace {
    Ok,
    /// 这台的中转没见过这个会话的请求（没走中转）。
    NoRelay,
    /// 会话的进程已经不在了（恢复时再选号）。
    Ended,
    /// 这台没建账号库。
    MachineNotMulti,
    /// 这一家没有可换的账号。
    AgentHasNoAccounts,
}

/// 一个号 ＋ 一个时刻。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AccountAt {
    pub account: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    /// `at` 写给人看的样子（回包出口 `common::time::with_texts` 添；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    /// `at` 距今（`+1h30m`；回包出口 `common::time::with_texts` 添，只在还没到时有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_rel_text: Option<String>,
    /// 这个号此刻是过线卡着的 ⇒ 过线的那一窗（语义位；停发横幅「最早 team 5h ↻…」）；不是过线 · 说不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub w: Option<String>,
}

/// 这个会话发不出去了：轮换里没有能接的号；`earliest` ＝ 最早回来的那个（说不出 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Blocked {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub earliest: Option<AccountAt>,
}

/// 会话的「账号」格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AccountCell {
    /// 起这个会话进程的那个号（`_` ＝ 起会话时没说是哪个号，原样）。
    pub start: String,
    /// 此刻走的号。
    pub current: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub since: u64,
    /// `since` 写给人看的样子（回包出口 `common::time::with_texts` 添；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub since_text: Option<String>,
    /// 换号记录，先的在前。
    pub history: Vec<SwitchRecord>,
    pub in_place: InPlace,
    /// 这一段（换进 `current` 以来）各窗口用了几个点；说不出基线的窗口不出，一个都没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<SegmentShow>>"))]
    pub segment: Vec<SegmentShow>,
    /// 换进 `current` 那一刻池里排在它前面、当时不能用的号（`preempt` 开着时其中一个又能用了 ⇒ 切回去）；没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<String>>"))]
    pub blocked_above: Vec<String>,
}

/// 这一段在一个窗口上：换进来那一刻的用量 · 之后又用了几个点 · 这个窗口的单段预算（没设 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SegmentShow {
    /// 窗口键（`5h` · `7d` · `7d:<模型>`）。
    pub w: String,
    /// 换进来那一刻的用量（%，取整）。
    pub base: u32,
    /// 之后又用了几个点（取整；窗口这期间重置过 ⇒ 只算重置之后的，是下界）。
    pub spent: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub stint: Option<u8>,
}

/// 换进一个号那一刻它一个窗口的用量（基线）：用了多少（比例）· 那时几点重置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Base {
    pub(crate) used: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
}

/// 窗口键 → 基线。
pub(crate) type Baseline = BTreeMap<String, Base>;

/// `rotation-session-read` 里一个会话的那一份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionRotation {
    pub agent: String,
    /// 轮换从哪来：跟随默认 · 某条规则 · 本会话。
    pub source: Source,
    /// 此刻生效的那条规则叫什么（跟随默认 ⇒ 默认那条；本会话 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub rule_name: Option<String>,
    /// 按此刻生效的那一份写好的一句规则说明（界面照抄）。
    pub explain: String,
    /// 会话血缘里它的父（不管来源是不是跟随它；界面据此决定来源下拉里列不列「跟随父会话」）；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub parent: Option<String>,
    /// 来源是跟随父会话、却追不到父那一条（父没经过中转 · 已清掉 · 绕回来）⇒ `true`，此刻按跟随默认；否则缺。
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub parent_missing: bool,
    /// 这个会话自己那一份（换成别的来源时也留着）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub custom: Option<Rotation>,
    pub account: AccountCell,
    /// 此刻的号触发了会换到谁；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub next: Option<String>,
    /// 发不出去了（被拒、轮换里没有能接的）；能发 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub blocked: Option<Blocked>,
    /// 这台可用、不在这个会话轮换里的按量号；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub fallback_api: Option<String>,
    /// 到上限此刻实际照哪一档办：轮换说 `stop`、这一家给不出「用满」回包 ⇒ `continue`。
    pub at_limit: AtLimit,
    /// 此刻那个号的显示态（「快满」按这个会话的 N）。
    pub quota: super::show::QuotaShow,
}

/// 一个会话在这台查得到吗：查得到 ⇒ 那一份；中转没见过 ⇒ 只说能不能不重启换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SessionRotationState {
    Present(Box<SessionRotation>),
    #[serde(rename_all = "camelCase")]
    Absent {
        in_place: InPlace,
    },
}

/// 「切换」里一个会话的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SwitchOutcome {
    #[serde(rename = "done")]
    Switched,
    /// 没动它（不重启换不成立：`noRelay` · `ended` · `machineNotMulti` · `agentHasNoAccounts`）。
    Skipped { code: String },
    /// 动了没成（`targetNeedsLogin` · `targetNeedsKey` · 重启换那一路的失败码原样）。
    #[serde(rename = "failed")]
    NotSwitched { code: String },
}

/// 重启换里一个会话的结果：成了 ⇒ 新进程起在哪个终端；没成 ⇒ 码 ＋ 旧会话还在不在（`kept` 还在跑 · `ended` 已停）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum RestartOutcome {
    #[serde(rename = "done")]
    Restarted { terminal: String },
    /// `code`：`session-restart` 的失败码原样 · 起了但到期限没报出 ⇒ `notArrived` · 记账写不进 ⇒ `ioFailed`。
    #[serde(rename = "failed")]
    NotRestarted { code: String, old: OldSession },
}

/// 重启换没成时，旧会话还在不在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum OldSession {
    Kept,
    Ended,
}

/// 一个会话在这台的轮换状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionEntry {
    /// 路由第 1 段（哪一家）。
    pub(crate) agent: String,
    /// 起这个会话进程的那个号（路由第 2 段）。
    pub(crate) start: String,
    /// 此刻走的号。
    pub(crate) current: String,
    /// 从什么时候起走 `current`。
    pub(crate) since: u64,
    /// 轮换从哪来。
    pub(crate) source: Source,
    /// 这个会话自己那一份（换成别的来源时留着）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) custom: Option<Rotation>,
    #[serde(default)]
    pub(crate) history: Vec<SwitchRecord>,
    /// 换进 `current` 那一刻它各窗口的用量（`since` 起算的这一段的基线）；换号 / 换了起它的号就清掉重记。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) baseline: Baseline,
    /// 换进 `current` 那一刻池里排在它前面、当时不能用的号（`preempt` 只等它们回来）；换号 / 换了起它的号就清掉重记。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) blocked_above: Vec<String>,
    /// 中转最后一次看见它的时刻（至多一天刷新一次，免得每发都写盘）；`0` ＝ 没记过（按 `since` 算）。清旧会话按它。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) seen: u64,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// 跟随父会话最多追几层（再深 ⇒ 按跟随默认）。
pub(crate) const PARENT_DEPTH: usize = 8;

/// 「看见」的时刻隔多久才刷新一次（秒）。
pub(crate) const SEEN_REFRESH: u64 = 86_400;
/// 跟随默认 · 没换过号 · 没有自己那一份的会话，多久没被看见就从账本里清掉（秒）。
pub(crate) const DROP_AFTER: u64 = 7 * 86_400;

impl SessionEntry {
    pub(crate) fn fresh(agent: &str, start: &str, now: u64) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            source: Source::Follow,
            custom: None,
            history: Vec::new(),
            baseline: Baseline::new(),
            blocked_above: Vec::new(),
            seen: now,
        }
    }

    /// 最后一次看见它的时刻（没记过 ⇒ 按 `since`）。
    pub(crate) fn last_seen(&self) -> u64 {
        self.seen.max(self.since)
    }
}

/// 一个会话的轮换从哪来。线上 `"follow"` · `"custom"` · `{"rule": "<id>"}` · `{"parent": "<sid>"}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotationSource"
    )
)]
pub enum Source {
    /// 跟随这台的默认规则（默认换了一条也跟着走）。
    Follow,
    /// 跟随父会话（链接：父那一条此刻按哪份它就按哪份；值是父的会话 id，取自会话血缘）。只共享规则，钉号与换号记录各自一份。
    Parent(String),
    /// 本会话自己那一份。
    Custom,
    /// 用某条规则（链接）。
    Rule(String),
}

/// 一条规则：名字 · 那一份轮换 · 版本（每写一次加一，写时带 `ifRev` 防两处互相覆盖）· 最后改的时刻。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Rule {
    pub(crate) name: String,
    pub(crate) rotation: Rotation,
    pub(crate) rev: u64,
    pub(crate) updated_at: u64,
}

/// 整份 `rotation.json`：规则表 · 默认指向哪条 · 每个会话。读进来那一刻补齐「至少一条规则、默认指向一条在的」。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Book {
    pub(crate) rules: BTreeMap<String, Rule>,
    /// 默认规则的 id（空 ＝ 还没有任何规则：只在内存里那一刻，落盘前补齐）。
    pub(crate) default_rule: String,
    pub(crate) sessions: BTreeMap<String, SessionEntry>,
}

/// 盘上读进来的那一形：今天的格 ＋ 升级前的两格（`default` 一份轮换 · 会话的 `follow`）。只在读的那一下用，写出去只有今天的格。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BookOnDisk {
    #[serde(default)]
    rules: BTreeMap<String, Rule>,
    #[serde(default)]
    default_rule: String,
    /// 升级前：这台的默认轮换。
    #[serde(default)]
    default: Option<Rotation>,
    #[serde(default)]
    sessions: BTreeMap<String, SessionOnDisk>,
}

#[derive(Deserialize)]
struct SessionOnDisk {
    #[serde(flatten)]
    entry: SessionEntryLoose,
    /// 升级前：跟随默认轮换。
    #[serde(default)]
    follow: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionEntryLoose {
    agent: String,
    start: String,
    current: String,
    since: u64,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default)]
    custom: Option<Rotation>,
    #[serde(default)]
    history: Vec<SwitchRecord>,
    #[serde(default)]
    baseline: Baseline,
    #[serde(default)]
    blocked_above: Vec<String>,
    #[serde(default)]
    seen: u64,
}

impl<'de> Deserialize<'de> for Book {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let disk = BookOnDisk::deserialize(d)?;
        let sessions = disk
            .sessions
            .into_iter()
            .map(|(sid, s)| {
                let e = s.entry;
                let source = e.source.unwrap_or(match s.follow {
                    Some(false) => Source::Custom,
                    _ => Source::Follow,
                });
                let entry = SessionEntry {
                    agent: e.agent,
                    start: e.start,
                    current: e.current,
                    since: e.since,
                    source,
                    custom: e.custom,
                    history: e.history,
                    baseline: e.baseline,
                    blocked_above: e.blocked_above,
                    seen: e.seen,
                };
                (sid, entry)
            })
            .collect();
        let mut b = Book {
            rules: disk.rules,
            default_rule: disk.default_rule,
            sessions,
        };
        b.settle(disk.default, 0);
        Ok(b)
    }
}

impl Default for Book {
    /// 没有文件时那一份：只有一条缺省的「默认」规则。
    fn default() -> Self {
        let mut b = Book {
            rules: BTreeMap::new(),
            default_rule: String::new(),
            sessions: BTreeMap::new(),
        };
        b.settle(None, 0);
        b
    }
}

/// 补齐时建的那条「默认」规则的 id（固定：文件还没有时读几次都是同一条）。
pub(crate) const SEED_RULE_ID: &str = "r_00000000";

/// 进程内序号：同一秒里连建两条规则也换一个 id。
static RULE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 一个没用过的规则 id：`r_` ＋ 8 位十六进制（不随改名变）。
pub(crate) fn new_rule_id(taken: &BTreeMap<String, Rule>) -> String {
    use std::hash::{Hash, Hasher};
    loop {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut h);
        std::process::id().hash(&mut h);
        RULE_SEQ
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .hash(&mut h);
        let id = format!("r_{:08x}", h.finish() as u32);
        if !taken.contains_key(&id) && id != SEED_RULE_ID {
            return id;
        }
    }
}

/// 起新会话先定好的 sid（UUID v4 的样子：那一家 `--session-id` 只收这一形）。随机来源同规则 id：时刻 · 进程 · 进程内序号。
pub(crate) fn new_session_id() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let seed = std::collections::hash_map::RandomState::new();
    let half = |salt: u8| {
        let mut h = seed.build_hasher();
        salt.hash(&mut h);
        std::time::SystemTime::now().hash(&mut h);
        std::process::id().hash(&mut h);
        RULE_SEQ
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .hash(&mut h);
        h.finish()
    };
    let (a, b) = (half(1), half(2));
    let a = (a & !0xF000) | 0x4000; // 版本 4
    let b = (b & !(0xC000 << 48)) | (0x8000 << 48); // 变体 10xx
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0xFFFF,
        b >> 48,
        b & 0xFFFF_FFFF_FFFF
    )
}

/// 规则名的比较形：去首尾空白、不分大小写（本机内不许重名按它判）。
pub(crate) fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

impl Book {
    /// 补齐：没有任何规则 ⇒ 建一条「默认」（升级前那一份默认轮换，没有就缺省那份）并设为默认；默认指向不在的 ⇒ 指向名字排最前的那条。
    pub(crate) fn settle(&mut self, legacy_default: Option<Rotation>, now: u64) {
        if self.rules.is_empty() {
            let id = SEED_RULE_ID.to_string();
            self.rules.insert(
                id.clone(),
                Rule {
                    name: copy_text("beRotation.rule.defaultName", &[]),
                    rotation: legacy_default.unwrap_or_default(),
                    rev: 1,
                    updated_at: now,
                },
            );
            self.default_rule = id;
        }
        if !self.rules.contains_key(&self.default_rule) {
            if let Some((id, _)) = self.rules.iter().min_by(|a, b| a.1.name.cmp(&b.1.name)) {
                self.default_rule = id.clone();
            }
        }
    }

    pub(crate) fn default_rotation(&self) -> Rotation {
        self.rules
            .get(&self.default_rule)
            .map(|r| r.rotation.clone())
            .unwrap_or_default()
    }

    /// 顺着「跟随父会话」追到说了算的那一条（自己不是跟随父会话 ⇒ 就是自己）；最多追 [`PARENT_DEPTH`] 层。
    /// 追不到（父不在账本里 · 绕回来 · 太深）⇒ `None`：按跟随默认。
    pub(crate) fn decider<'a>(&'a self, s: &'a SessionEntry) -> Option<&'a SessionEntry> {
        let mut cur = s;
        for _ in 0..=PARENT_DEPTH {
            match &cur.source {
                Source::Parent(p) => cur = self.sessions.get(p)?,
                _ => return Some(cur),
            }
        }
        None
    }

    /// 这个会话此刻实际按哪条规则（本会话 ⇒ `None`；指向的规则不在了 / 跟随父会话追不到 ⇒ 默认那条）。
    pub(crate) fn rule_of<'a>(&'a self, s: &'a SessionEntry) -> Option<&'a str> {
        let d = self.decider(s);
        match d.map(|d| (&d.source, d)) {
            Some((Source::Custom, d)) if d.custom.is_some() => None,
            Some((Source::Rule(id), _)) if self.rules.contains_key(id) => Some(id.as_str()),
            _ => self
                .rules
                .contains_key(&self.default_rule)
                .then_some(self.default_rule.as_str()),
        }
    }

    /// 这个会话此刻按哪一份轮换：跟随 ⇒ 默认那条 · 跟随父会话 ⇒ 父此刻那份 · 规则 ⇒ 那条（不在了 ⇒ 默认那条）· 本会话 ⇒ 自己那份。
    pub(crate) fn rotation_of(&self, s: &SessionEntry) -> Rotation {
        match self.decider(s) {
            Some(SessionEntry {
                source: Source::Custom,
                custom: Some(c),
                ..
            }) => c.clone(),
            _ => self
                .rule_of(s)
                .and_then(|id| self.rules.get(id))
                .map(|r| r.rotation.clone())
                .unwrap_or_default(),
        }
    }

    /// 同 [`Book::saw`]；这一下是新建的、且血缘里有父、父在账本里且与它同一家 ⇒ 来源 ＝ 跟随父会话（不同家的不继承）。
    /// 已记下的会话来源一律不动。
    pub(crate) fn saw_child(
        &mut self,
        sid: &str,
        agent: &str,
        start: &str,
        now: u64,
        parent: Option<&str>,
    ) -> bool {
        let fresh = !self.sessions.contains_key(sid);
        let moved = self.saw(sid, agent, start, now);
        if fresh {
            let same = parent
                .filter(|p| *p != sid)
                .filter(|p| self.sessions.get(*p).is_some_and(|ps| ps.agent == agent));
            if let (Some(p), Some(s)) = (same, self.sessions.get_mut(sid)) {
                s.source = Source::Parent(p.to_string());
            }
        }
        moved
    }

    /// 中转第一次看见这个会话 / 会话换了起它的号 ⇒ 记下（换了起它的号 ⇒ 钉号清掉，从新号起算）。改了 ⇒ `true`。
    /// 每天头一次看见已知的会话 ⇒ 刷新「看见」的时刻（`true`，写一次盘）；新会话被看见时顺手清旧会话（[`Book::drop_stale`]）。
    pub(crate) fn saw(&mut self, sid: &str, agent: &str, start: &str, now: u64) -> bool {
        match self.sessions.get_mut(sid) {
            None => {
                self.drop_stale(now);
                self.sessions
                    .insert(sid.to_string(), SessionEntry::fresh(agent, start, now));
                true
            }
            Some(s) if s.start != start || s.agent != agent => {
                s.agent = agent.to_string();
                s.start = start.to_string();
                s.current = start.to_string();
                s.since = now;
                s.baseline.clear();
                s.blocked_above.clear();
                s.seen = now;
                true
            }
            Some(s) if now.saturating_sub(s.last_seen()) >= SEEN_REFRESH => {
                s.seen = now;
                true
            }
            Some(_) => false,
        }
    }

    /// 清旧会话（稿第 12 题）：跟随默认或跟随父会话 · 没换过号 · 没有自己那一份 · 超过 [`DROP_AFTER`] 没被看见 ⇒ 从账本里删。
    /// 用规则的 · 本会话的 · 换过号的都不动；删掉的那种再来一发会照新会话记回来（一样的一条，不丢东西）。回删了几条。
    pub(crate) fn drop_stale(&mut self, now: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            !(matches!(s.source, Source::Follow | Source::Parent(_))
                && s.custom.is_none()
                && s.history.is_empty()
                && now.saturating_sub(s.last_seen()) > DROP_AFTER)
        });
        before - self.sessions.len()
    }

    /// 记下换进此刻的号那一刻挡在它前面的号（换号 · 换了起它的号时已清空）。改了 ⇒ `true`。
    pub(crate) fn block_above(&mut self, sid: &str, above: &[String]) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        let changed = s.blocked_above != above;
        s.blocked_above = above.to_vec();
        changed
    }

    /// 补这一段的基线：`base` 里此刻还没记的窗口记下（换号 · 换了起它的号时已清空 ⇒ 等于整份记；
    /// 换进来时说不出的窗口，之后第一次说得出时补上）。改了 ⇒ `true`。
    pub(crate) fn rebase(&mut self, sid: &str, base: &Baseline) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        let mut changed = false;
        for (k, b) in base {
            if !s.baseline.contains_key(k) {
                s.baseline.insert(k.clone(), b.clone());
                changed = true;
            }
        }
        changed
    }

    /// 钉到 `to`，记一条（以及轮到时跳过的那几个）。
    pub(crate) fn pin(&mut self, sid: &str, rec: SwitchRecord, skipped: &[(String, Unready)]) {
        let Some(s) = self.sessions.get_mut(sid) else {
            return;
        };
        for (account, reason) in skipped {
            note(
                s,
                SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: rec.at,
                    from: rec.from.clone(),
                    to: rec.from.clone(),
                    why: SwitchWhy::Skipped {
                        account: account.clone(),
                        reason: *reason,
                    },
                    from_resets_at: None,
                },
            );
        }
        if rec.to != s.current {
            s.current = rec.to.clone();
            s.since = rec.at;
            s.baseline.clear();
            s.blocked_above.clear();
        }
        push(s, rec);
    }

    /// 没换成的那几种（跳过 · 留在超额 · 硬上限卡住）：自上一次真换号以来同一句已经记过 ⇒ 不再记。改了 ⇒ `true`。
    pub(crate) fn note_stuck(
        &mut self,
        sid: &str,
        rec: SwitchRecord,
        skipped: &[(String, Unready)],
    ) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        let mut changed = false;
        for (account, reason) in skipped {
            changed |= note(
                s,
                SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: rec.at,
                    from: rec.from.clone(),
                    to: rec.from.clone(),
                    why: SwitchWhy::Skipped {
                        account: account.clone(),
                        reason: *reason,
                    },
                    from_resets_at: None,
                },
            );
        }
        if matches!(
            rec.why,
            SwitchWhy::ToOverage | SwitchWhy::Held { .. } | SwitchWhy::Wait { .. }
        ) {
            changed |= note(s, rec);
        }
        changed
    }
}

fn push(s: &mut SessionEntry, rec: SwitchRecord) {
    s.history.push(rec);
    let over = s.history.len().saturating_sub(HISTORY_KEPT);
    s.history.drain(..over);
}

/// 记一条「没换成」的：自上一次真换号以来已有同一句 ⇒ 不记。
fn note(s: &mut SessionEntry, rec: SwitchRecord) -> bool {
    let dup = s
        .history
        .iter()
        .rev()
        .take_while(|h| h.from == h.to)
        .any(|h| h.from == rec.from && h.why == rec.why);
    if !dup {
        push(s, rec);
    }
    !dup
}

// ── 落盘 ─────────────────────────────────────────────────────────────────

/// 这台的落点：`<家>/rotation.json`（家同额度账）。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    super::ledger::path_from(get).map(|p| p.with_file_name(FILE_NAME))
}

pub(crate) fn path_now() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

/// 读一次盘。三态：没有（没动过）/ 读得懂 / 读不懂（不覆盖）。
pub(crate) type Read = crate::common::own_state::Read<Book>;

/// 读盘的上限（会话条数与每条的换号记录都有上限，远到不了）。
const MAX_BYTES: u64 = 16 << 20;

pub(crate) fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// 盘上那一份的戳：改动时刻 · 长度 · inode（每次写都是换名落盘 ⇒ 新 inode；文件系统的时刻是粗粒度的，
/// 同一个时钟格里写两次、长度又一样时只有它分得出）。
type Stamp = (std::time::SystemTime, u64, u64);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let ino = 0;
    Some((m.modified().ok()?, m.len(), ino))
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct RotationStore {
    path: Option<PathBuf>,
    cache: Mutex<(Option<Stamp>, Book)>,
}

impl RotationStore {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cache: Mutex::new((None, Book::default())),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 当作缺省，不轮换）。
    pub(crate) fn now(&self) -> Book {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.1.clone();
        };
        let s = stamp(p);
        if s.is_none() || s != g.0 {
            g.1 = match read_at(p) {
                Read::Present(b) => b,
                Read::Absent | Read::Unreadable(_) => Book::default(),
            };
            g.0 = s;
        }
        g.1.clone()
    }

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的会话各响一下（[`changes`]）。读不懂的那一份不覆盖。
    /// 外面只经两扇门进来：中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）。
    fn change<R>(&self, f: impl FnOnce(&mut Book) -> R) -> Result<R, Said> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let (r, before, after) = match self.path.as_deref() {
            None => {
                let before = g.1.clone();
                let r = f(&mut g.1);
                (r, before, g.1.clone())
            }
            Some(path) => {
                let (r, before, after) = write_locked(path, f)?;
                g.0 = stamp(path);
                g.1 = after.clone();
                // 上一回见过的那一份（别的进程在那之后写过 ⇒ 它改到的会话也在这一下里响）；本进程记下自己写成的这一份，
                // 盯盘那一路再看见同一个戳就不重推。
                let base = seen_swap(path, g.0, &after).unwrap_or(before.clone());
                (r, base, after)
            }
        };
        drop(g);
        ring_changed(&before, &after);
        Ok(r)
    }
}

/// 本进程最后见过的盘上那一份（按路径）：本进程写成的 · 盯盘重扫读到的。别的进程写了 ⇒ 戳对不上 ⇒ 重扫时比对它推。
type DiskSeen = std::collections::HashMap<PathBuf, (Option<Stamp>, Book)>;

fn seen() -> &'static Mutex<DiskSeen> {
    static SEEN: std::sync::OnceLock<Mutex<DiskSeen>> = std::sync::OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(DiskSeen::new()))
}

/// 记下这一份，回上一份（没见过 ⇒ `None`）。
fn seen_swap(path: &Path, st: Option<Stamp>, book: &Book) -> Option<Book> {
    let mut g = seen().lock().unwrap_or_else(|e| e.into_inner());
    g.insert(path.to_path_buf(), (st, book.clone()))
        .map(|(_, b)| b)
}

/// 重扫一次盘：戳与上回见过的不同（别的进程写了）⇒ 重读、比对、改到的会话各响一下。第一次 ⇒ 只记下。读不懂 ⇒ 不动。
pub(crate) fn rescan(path: &Path) {
    let st = stamp(path);
    let known = {
        let g = seen().lock().unwrap_or_else(|e| e.into_inner());
        g.get(path).map(|(s, _)| *s)
    };
    if known == Some(st) {
        return;
    }
    let now = match read_at(path) {
        Read::Present(b) => b,
        Read::Absent => Book::default(),
        Read::Unreadable(e) => {
            tracing::warn!("[rotation] 重扫 {} 读不懂：{e}", path.display());
            return;
        }
    };
    if let Some(prev) = seen_swap(path, st, &now) {
        if known.is_some() {
            ring_changed(&prev, &now);
        }
    }
}

/// 盯 `rotation.json` 所在的目录（只认这个文件名）：别的进程写了 ⇒ [`rescan`]。返回的那一份活着就一直盯。
/// 起的时候先记下此刻那一份（之后的改动才有得比）。
pub(crate) fn watch(path: &Path) -> Result<notify::RecommendedWatcher, String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?
        .to_path_buf();
    crate::common::own_dir::ensure_private_dir(&dir).map_err(|e| e.to_string())?;
    rescan(path);
    let target = path.to_path_buf();
    crate::platform::watch_file::watch(
        &[(dir, false)],
        |p| p.file_name().and_then(|n| n.to_str()) == Some(FILE_NAME),
        "rotation-watch",
        move || rescan(&target),
    )
}

/// 常驻 / 流那一路的后端起来时调一次：盯这台的 `rotation.json`，进程活着就一直盯。盯不上只出声。
pub(crate) fn watch_here() {
    static HELD: Mutex<Option<notify::RecommendedWatcher>> = Mutex::new(None);
    let Some(path) = path_now() else { return };
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    match watch(&path) {
        Ok(w) => *g = Some(w),
        Err(e) => tracing::warn!(
            "[rotation] 盯不上 {}：{e}（别处写的轮换要等面板重开才看得见）",
            path.display()
        ),
    }
}

/// 中转那一路的写口（上游选择换号：第一次看见会话 · 钉号 · 记一条）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(f)
}

/// 帧面那一路的写口（改默认轮换 · 改会话轮换 · 现在就换）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(f)
}

/// 改到的会话各响一下：它那一格变了，或它此刻生效的那一份轮换变了（链接：规则改了 ＝ 用它的会话都变了）；
/// 规则表 / 默认指向变了 ⇒ 规则那条通道也响一下。
fn ring_changed(before: &Book, after: &Book) {
    let rules_moved = before.rules != after.rules || before.default_rule != after.default_rule;
    for (sid, s) in &after.sessions {
        let moved = match before.sessions.get(sid) {
            // 跟随父会话的：父那一条变了，它此刻生效那份就变了（规则表没动也算）。
            Some(b) => {
                b != s
                    || ((rules_moved || matches!(s.source, Source::Parent(_)))
                        && before.rotation_of(b) != after.rotation_of(s))
            }
            None => true,
        };
        if moved {
            let _ = changes().send(sid.clone());
        }
    }
    if rules_moved {
        let _ = rules_changes().send(());
    }
}

/// 进程里那条「这台的规则表 / 默认指向变了」的通道（流连接订它推 `rotation_rules_changed`）。
pub(crate) fn rules_changes() -> &'static tokio::sync::broadcast::Sender<()> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> = std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<()>(16).0)
}

/// 进程里那条「某个会话的轮换 / 账号格变了」的通道（流连接订它推 `rotation_changed`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(256).0)
}

fn write_locked<R>(path: &Path, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book, Book), Said> {
    let shown = path.display().to_string();
    let dir = path
        .parent()
        .ok_or_else(|| Said::from(copy_text("beRotation.write.noParent", &[("path", &shown)])))?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beRotation.write.failed",
                &[("path", &shown), ("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let _lock = crate::platform::lock::hold(dir)?;
    let before = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        // 读不懂的那份不覆盖：读不懂那一句（带路径与原因词）后面接「未覆盖」，原话照它的。
        Read::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beRotation.write.unreadable", &[("said", said)])))
        }
    };
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before {
        return Ok((r, before, after));
    }
    crate::common::own_state::write_json(path, &after)?;
    Ok((r, before, after))
}

// ── 线上那一份轮换的读法（整份收、不合法整份拒并说哪一格） ─────────────────

/// 窗口键（适配层给的不透明串，如 `5h` · `7d` · `7d:<模型>`）或 `*`：写得进轮换的那一形。
fn window_key_ok(k: &str) -> bool {
    k == ALL_WINDOWS
        || (!k.is_empty()
            && k.len() <= 64
            && k.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.')))
}

fn pct_in_range(v: &Value) -> Option<u8> {
    in_range(v, &THRESHOLD_RANGE)
}

fn in_range(v: &Value, range: &std::ops::RangeInclusive<u8>) -> Option<u8> {
    v.as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|n| range.contains(n))
}

/// 每号那一层：`{号: {窗口键|"*": 值}}`，值由 `one(值, 这一格的名字)` 读。
fn per_account<T>(
    v: Option<&Value>,
    name: &str,
    account_ok: &dyn Fn(&str) -> bool,
    one: &dyn Fn(&Value, &str) -> Result<T, String>,
) -> Result<BTreeMap<String, BTreeMap<String, T>>, String> {
    let Some(v) = v else {
        return Ok(BTreeMap::new());
    };
    let o = v.as_object().ok_or_else(|| {
        format!("`{name}` must be an object {{account: {{window|\"*\": value}}}}")
    })?;
    let mut out = BTreeMap::new();
    for (a, per) in o {
        if !account_ok(a) {
            return Err(format!("`{name}.{a}` must name an account"));
        }
        let per = per.as_object().filter(|m| !m.is_empty()).ok_or_else(|| {
            format!("`{name}.{a}` must be a non-empty object {{window|\"*\": value}}")
        })?;
        let mut row = BTreeMap::new();
        for (k, x) in per {
            if !window_key_ok(k) {
                return Err(format!("`{name}.{a}.{k}` is not a window key or \"*\""));
            }
            row.insert(k.clone(), one(x, &format!("{name}.{a}.{k}"))?);
        }
        out.insert(a.clone(), row);
    }
    Ok(out)
}

/// 上限的一个值：`0..=99`，或 `[{at: "HH:MM-HH:MM", n: 0..=99}, …]`（至少一段）。`0` ＝ 不用这个号。
fn cap_value(v: &Value, cell: &str) -> Result<CapValue, String> {
    if let Some(n) = in_range(v, &CAP_RANGE) {
        return Ok(CapValue::N(n));
    }
    let bad = || {
        format!(
            "`{cell}` must be an integer 0..=99 or [{{\"at\":\"HH:MM-HH:MM\",\"n\":0..=99}}, …]"
        )
    };
    let arr = v.as_array().filter(|a| !a.is_empty()).ok_or_else(bad)?;
    let mut slots = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let m = item
            .as_object()
            .filter(|m| m.len() == 2)
            .ok_or_else(|| format!("`{cell}[{i}]` must be {{\"at\", \"n\"}}"))?;
        let at = m
            .get("at")
            .and_then(Value::as_str)
            .filter(|at| span_of(at).is_some())
            .ok_or_else(|| {
                format!("`{cell}[{i}].at` must be \"HH:MM-HH:MM\" (00:00..24:00, start ≠ end)")
            })?;
        let n = m
            .get("n")
            .and_then(|n| in_range(n, &CAP_RANGE))
            .ok_or_else(|| format!("`{cell}[{i}].n` must be an integer 0..=99"))?;
        if let Some(j) = slots.iter().position(|x: &CapSlot| overlaps(&x.at, at)) {
            return Err(format!("`{cell}[{i}].at` overlaps `{cell}[{j}].at`"));
        }
        slots.push(CapSlot {
            at: at.to_string(),
            n,
        });
    }
    Ok(CapValue::Slots(slots))
}

/// 一段占一天里的哪几截（跨午夜的拆成两截）。
fn pieces(at: &str) -> Vec<(u16, u16)> {
    match span_of(at) {
        Some((from, to)) if from < to => vec![(from, to)],
        Some((from, to)) => vec![(from, 24 * 60), (0, to)],
        None => Vec::new(),
    }
}

/// 两段有没有重叠的分钟（含跨午夜的）。
pub(crate) fn overlaps(a: &str, b: &str) -> bool {
    pieces(a)
        .iter()
        .any(|(f1, t1)| pieces(b).iter().any(|(f2, t2)| f1 < t2 && f2 < t1))
}

/// 读一份轮换：键 `order` · `enabled`，可选 `atLimit`（`"continue"` · `"stop"`，缺 ⇒ `continue`）·
/// `cap`（`{号: {窗口键|"*": 0..=99 | [{at, n}]}}`，`0` ＝ 不用这个号；同一格的时段不许重叠；号那一格写 `"*"` ＝ 所有号（触发那一行），只收 `5h` · `7d`、`1..=99`）· `stint`（`{号: {窗口键|"*": 1..=99}}`）· `preempt`（布尔，缺 ⇒ 关）·
/// `fallback`（兜底的号，须在 `order` 里具名）· `wait`（切兜底前最多等几分钟 `0..=120`，缺 ⇒ 40）。`start_slots` ＝ 起始账号占位该有几个（默认恰好 1；会话自己那份 0 或 1）。
/// `account_ok(号)` 判这一格当得了轮换里的号；`is_api(号)` 判按量号；`prior` 是改之前那一份：**新勾上的按量号挪到 `order` 末尾**（订阅号用完才轮到它）。
/// 不合法 ⇒ `Err(哪一格、为什么)`（英文诊断，不进文案表）。
pub(crate) fn rotation_from(
    v: &Value,
    start_slots: std::ops::RangeInclusive<usize>,
    account_ok: &dyn Fn(&str) -> bool,
    is_api: &dyn Fn(&str) -> bool,
    prior: Option<&Rotation>,
) -> Result<Rotation, String> {
    let o = v.as_object().ok_or(
        "rotation must be an object {order, enabled, atLimit?, cap?, stint?, preempt?, fallback?, wait?}",
    )?;
    if let Some(k) = o.keys().find(|k| {
        !matches!(
            k.as_str(),
            "order" | "enabled" | "atLimit" | "cap" | "stint" | "preempt" | "fallback" | "wait"
        )
    }) {
        return Err(format!("unknown field `{k}`"));
    }
    let order_v = o
        .get("order")
        .and_then(Value::as_array)
        .ok_or("`order` must be an array")?;
    let mut order: Vec<RotationSlot> = Vec::new();
    let mut starts = 0usize;
    for (i, item) in order_v.iter().enumerate() {
        let slot = match item {
            Value::String(a) if account_ok(a) => RotationSlot::Named(a.clone()),
            Value::Object(m) if m.len() == 1 && m.get("start") == Some(&Value::Bool(true)) => {
                starts += 1;
                RotationSlot::Start(StartSlot { start: true })
            }
            _ => {
                return Err(format!(
                    "`order[{i}]` must be an account id or {{\"start\":true}}"
                ))
            }
        };
        if order.contains(&slot) && matches!(slot, RotationSlot::Named(_)) {
            return Err(format!("`order[{i}]` repeats an account"));
        }
        order.push(slot);
    }
    if !start_slots.contains(&starts) {
        return Err(format!(
            "`order` must hold {}..={} {{\"start\":true}} slot(s), got {starts}",
            start_slots.start(),
            start_slots.end()
        ));
    }
    let named = |a: &str| {
        order
            .iter()
            .any(|s| s == &RotationSlot::Named(a.to_string()))
    };
    let enabled_v = o
        .get("enabled")
        .and_then(Value::as_array)
        .ok_or("`enabled` must be an array")?;
    let mut enabled: Vec<String> = Vec::new();
    for (i, item) in enabled_v.iter().enumerate() {
        let a = item
            .as_str()
            .filter(|a| named(a))
            .ok_or_else(|| format!("`enabled[{i}]` must name an account listed in `order`"))?;
        if enabled.iter().any(|e| e == a) {
            return Err(format!("`enabled[{i}]` repeats an account"));
        }
        enabled.push(a.to_string());
    }
    let at_limit = match o.get("atLimit") {
        None => AtLimit::Continue,
        Some(Value::String(s)) if s == "continue" => AtLimit::Continue,
        Some(Value::String(s)) if s == "stop" => AtLimit::Stop,
        Some(_) => return Err("`atLimit` must be \"continue\" or \"stop\"".into()),
    };
    let cap = per_account(
        o.get("cap"),
        "cap",
        &|a| a == ALL_WINDOWS || account_ok(a),
        &cap_value,
    )?;
    if let Some(row) = cap.get(ALL_WINDOWS) {
        for (k, v) in row {
            if !LINE_SLOTS.contains(&k.as_str()) {
                return Err(format!("`cap.*.{k}` must be one of `5h` · `7d`"));
            }
            let zero = match v {
                CapValue::N(n) => *n == 0,
                CapValue::Slots(s) => s.iter().any(|x| x.n == 0),
            };
            if zero {
                return Err(format!("`cap.*.{k}` must be 1..=99"));
            }
        }
    }
    // 单段预算多收一行 `"*"`（所有号）：换法「单段预算 N 点」写的就是 `{"*": {"*": n}}`。
    let stint = per_account(
        o.get("stint"),
        "stint",
        &|a| a == ALL_WINDOWS || account_ok(a),
        &|v, cell| pct_in_range(v).ok_or_else(|| format!("`{cell}` must be an integer 1..=99")),
    )?;
    let preempt = match o.get("preempt") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("`preempt` must be true or false".into()),
    };
    let wait = match o.get("wait") {
        None => WAIT_DEFAULT,
        Some(v) => in_range(v, &WAIT_RANGE).ok_or("`wait` must be an integer 0..=120")?,
    };
    let mut fallback: Vec<String> = Vec::new();
    if let Some(v) = o.get("fallback") {
        let arr = v.as_array().ok_or("`fallback` must be an array")?;
        for (i, item) in arr.iter().enumerate() {
            let a = item
                .as_str()
                .filter(|a| named(a))
                .ok_or_else(|| format!("`fallback[{i}]` must name an account listed in `order`"))?;
            if fallback.iter().any(|f| f == a) {
                return Err(format!("`fallback[{i}]` repeats an account"));
            }
            fallback.push(a.to_string());
        }
    }
    let was = |a: &str| prior.is_some_and(|p| p.enabled.iter().any(|e| e == a));
    let newly: Vec<RotationSlot> = enabled
        .iter()
        .filter(|a| is_api(a) && !was(a))
        .map(|a| RotationSlot::Named(a.clone()))
        .collect();
    order.retain(|s| !newly.contains(s));
    order.extend(newly);
    Ok(Rotation {
        order,
        enabled,
        at_limit,
        cap,
        stint,
        preempt,
        fallback,
        wait,
    })
}

/// 编辑器里一格填错了：哪一格（`name` · `wait` · `cap.*.5h` · `cap.*.7d`（触发那一行）· `cap.<号>.<窗口键>` · `cap.<号>.<窗口键>[i]` · `stint.<号>.<窗口键>`）·
/// 短码（`empty` · `dup` · `tooLong` · `range` · `time` · `same` · `overlap`）· 重叠时与第几段（0 起）。界面只照它标红、按短码取文案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CellError {
    pub cell: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub with: Option<usize>,
}

fn cell_err(cell: String, code: &str, with: Option<usize>) -> CellError {
    CellError {
        cell,
        code: code.to_string(),
        with,
    }
}

/// `"HH:MM"` → 分钟（`24:00` 收）；写错 ⇒ `None`。
fn clock_of(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 || !(h.bytes().chain(m.bytes())).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (m < 60 && (h < 24 || (h == 24 && m == 0))).then_some(h * 60 + m)
}

/// 填的人会填错的那几格逐格判（形状不对的不在这里，交 [`rotation_from`] 整份拒）。
pub(crate) fn cell_errors(v: &Value) -> Vec<CellError> {
    let mut out = Vec::new();
    let Some(o) = v.as_object() else {
        return out;
    };
    if let Some(w) = o.get("wait") {
        if in_range(w, &WAIT_RANGE).is_none() {
            out.push(cell_err("wait".into(), "range", None));
        }
    }
    let rows = |k: &str| -> Vec<(String, String, Value)> {
        o.get(k)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|m| m.iter())
            .flat_map(|(a, per)| {
                per.as_object()
                    .into_iter()
                    .flat_map(|m| m.iter())
                    .map(move |(w, x)| (a.clone(), w.clone(), x.clone()))
            })
            .collect()
    };
    for (a, w, x) in rows("cap") {
        let cell = format!("cap.{a}.{w}");
        // 触发那一行不收 0（空着那一窗就是满了才换）。
        let range = if a == ALL_WINDOWS {
            THRESHOLD_RANGE
        } else {
            CAP_RANGE
        };
        match &x {
            Value::Array(items) => {
                let mut ok: Vec<(usize, String)> = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let at = item.get("at").and_then(Value::as_str).unwrap_or("");
                    let here = format!("{cell}[{i}]");
                    let ends = at.split_once('-').map(|(f, t)| (clock_of(f), clock_of(t)));
                    match ends {
                        Some((Some(f), Some(t))) if f == t => {
                            out.push(cell_err(here.clone(), "same", None));
                        }
                        Some((Some(_), Some(_))) if span_of(at).is_some() => {
                            if let Some((j, _)) = ok.iter().find(|(_, b)| overlaps(b, at)) {
                                out.push(cell_err(here.clone(), "overlap", Some(*j)));
                            } else {
                                ok.push((i, at.to_string()));
                            }
                        }
                        _ => out.push(cell_err(here.clone(), "time", None)),
                    }
                    if item.get("n").and_then(|n| in_range(n, &range)).is_none() {
                        out.push(cell_err(here, "range", None));
                    }
                }
            }
            n if in_range(n, &range).is_none() => out.push(cell_err(cell, "range", None)),
            _ => {}
        }
    }
    for (a, w, x) in rows("stint") {
        if pct_in_range(&x).is_none() {
            out.push(cell_err(format!("stint.{a}.{w}"), "range", None));
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
