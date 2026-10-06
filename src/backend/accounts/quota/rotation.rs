//! 轮换：额度满了（或到阈值）就把会话钉到轮换里下一个号上，不重启进程。落 `~/.cc-monitor/rotation.json`。
//!
//! - 每台一份默认轮换（顺序 · 勾了哪几个 · 换号时机）；缺省只有「起始账号」那一格 ⇒ 缺省不轮换。
//! - 每个会话跟随默认，或用自己那一份（切回跟随时自己那一份留着）；按会话 id 存，后端重启后还在。
//! - 每个会话此刻钉在哪个号、从什么时候起、换号记录；会话换了起它的号（重启换号 / 换号恢复）⇒ 钉号随之清掉。
//! - 判「换不换、换谁」只在 [`super::decide`]；这里只有存取。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

/// 每个会话至多留几条换号记录（最早的先丢）。
pub(crate) const HISTORY_KEPT: usize = 32;

/// 上限（「到 N% 换」的 N · 每号覆盖的上限）与单段预算收哪些值。
pub(crate) const THRESHOLD_RANGE: std::ops::RangeInclusive<u8> = 1..=99;

/// 每号那一格里「这个号的所有窗口」的键。
pub(crate) const ALL_WINDOWS: &str = "*";

/// 换号时机。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum RotationWhen {
    /// 上游拒了才换（缺省）。
    #[default]
    Full,
    /// 用量到 `n`% 就换（下一发起）。
    Threshold { n: u8 },
}

/// 阈值模式下此刻的号到了 N%、池里没有 N% 以下的号可换时怎么办。
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
    let minute = |s: &str| -> Option<u16> {
        let (h, m) = s.split_once(':')?;
        if h.len() != 2 || m.len() != 2 || !(h.bytes().chain(m.bytes())).all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
        (m < 60 && (h < 24 || (h == 24 && m == 0))).then_some(h * 60 + m)
    };
    let (a, b) = at.split_once('-')?;
    let (from, to) = (minute(a)?, minute(b)?);
    (from < 24 * 60 && from != to).then_some((from, to))
}

/// 每号覆盖的上限：号 → 窗口键（或 `*` ＝ 这个号的所有窗口）→ 上限。
pub type Caps = BTreeMap<String, BTreeMap<String, CapValue>>;

/// 每号的单段预算：号 → 窗口键（或 `*`）→ 换进来之后再用几个点就想走。
pub type Stints = BTreeMap<String, BTreeMap<String, u8>>;

/// 一份轮换：顺序 · 勾了哪几个 · 缺省上限（`when`）· 每号上限 · 每号单段预算 · 前面的号回来就切回 · 到上限没号可换时怎么办。
/// 起始账号占位恒算勾上。后三格缺省（空 · 空 · 关）时不写出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Rotation {
    pub order: Vec<RotationSlot>,
    pub enabled: Vec<String>,
    pub when: RotationWhen,
    /// 盘上缺 ⇒ `continue`。
    #[serde(default)]
    pub at_limit: AtLimit,
    /// 每号覆盖的上限：`{号: {窗口键|"*": n | [{at, n}]}}`（盖过 `when`；时段外落回下一层）。
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
}

impl Default for Rotation {
    /// 缺省：只有起始账号 ⇒ 不轮换。
    fn default() -> Self {
        Self {
            order: vec![RotationSlot::Start(StartSlot { start: true })],
            enabled: Vec::new(),
            when: RotationWhen::Full,
            at_limit: AtLimit::Continue,
            cap: Caps::new(),
            stint: Stints::new(),
            preempt: false,
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
    /// 原号用量到了阈值。
    Threshold { n: u8 },
    /// 用户「现在就换」，不重启。
    ManualHot,
    /// 用户「现在就换」，重启。
    ManualRestart,
    /// 轮到这个号时跳过了它。
    Skipped { account: String, reason: Unready },
    /// 订阅号都满了，留在原号的付费超额上。
    ToOverage,
    /// 硬上限：池里没有 `n`% 以下的号，这一发没发上游（`fromResetsAt` ＝ 池里最早回到 `n`% 以下的那一刻）。
    Held { n: u8 },
    /// 这一段在原号上用完了单段预算：`w` 那个窗口（窗口键）换进来之后又用了 `n` 个点。
    Stint { w: String, n: u8 },
    /// 前面的号又能用了（`preempt`）⇒ 切回去。
    Preempt,
}

/// 一条换号记录。`from == to` 的是没换成的那几种（跳过 · 留在超额）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SwitchRecord {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    pub from: String,
    pub to: String,
    pub why: SwitchWhy,
    /// 那一刻原号几点重置（知道才有；记下就不随后来的数变）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub from_resets_at: Option<u64>,
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
    /// 跟随这台的默认轮换。
    pub follow: bool,
    /// 这个会话自己那一份（跟随时也留着）。
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
    /// 跟随这台的默认轮换。
    pub(crate) follow: bool,
    /// 这个会话自己那一份（切回跟随时留着）。
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
}

impl SessionEntry {
    pub(crate) fn fresh(agent: &str, start: &str, now: u64) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            follow: true,
            custom: None,
            history: Vec::new(),
            baseline: Baseline::new(),
            blocked_above: Vec::new(),
        }
    }
}

/// 整份 `rotation.json`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Book {
    /// 这台的默认轮换；`None` ＝ 没动过（[`Rotation::default`]）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) default: Option<Rotation>,
    #[serde(default)]
    pub(crate) sessions: BTreeMap<String, SessionEntry>,
}

impl Book {
    pub(crate) fn default_rotation(&self) -> Rotation {
        self.default.clone().unwrap_or_default()
    }

    /// 这个会话此刻按哪一份轮换。
    pub(crate) fn rotation_of(&self, s: &SessionEntry) -> Rotation {
        match (&s.custom, s.follow) {
            (Some(c), false) => c.clone(),
            _ => self.default_rotation(),
        }
    }

    /// 中转第一次看见这个会话 / 会话换了起它的号 ⇒ 记下（换了起它的号 ⇒ 钉号清掉，从新号起算）。改了 ⇒ `true`。
    pub(crate) fn saw(&mut self, sid: &str, agent: &str, start: &str, now: u64) -> bool {
        match self.sessions.get_mut(sid) {
            None => {
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
                true
            }
            Some(_) => false,
        }
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
        if matches!(rec.why, SwitchWhy::ToOverage | SwitchWhy::Held { .. }) {
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
#[derive(Debug)]
pub(crate) enum Read {
    Absent,
    Present(Book),
    Unreadable(String),
}

pub(crate) fn read_at(path: &Path) -> Read {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Absent,
        Err(e) => Read::Unreadable(e.to_string()),
        Ok(s) => match serde_json::from_str::<Book>(&s) {
            Ok(b) => Read::Present(b),
            Err(e) => Read::Unreadable(e.to_string()),
        },
    }
}

type Stamp = (std::time::SystemTime, u64);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
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
    fn change<R>(&self, f: impl FnOnce(&mut Book) -> R) -> Result<R, String> {
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
                (r, before, after)
            }
        };
        drop(g);
        ring_changed(&before, &after);
        Ok(r)
    }
}

/// 中转那一路的写口（上游选择换号：第一次看见会话 · 钉号 · 记一条）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, String> {
    store.change(f)
}

/// 帧面那一路的写口（改默认轮换 · 改会话轮换 · 现在就换）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, String> {
    store.change(f)
}

fn ring_changed(before: &Book, after: &Book) {
    let default_moved = before.default != after.default;
    for (sid, s) in &after.sessions {
        let moved = before.sessions.get(sid) != Some(s) || (default_moved && s.follow);
        if moved {
            let _ = changes().send(sid.clone());
        }
    }
}

/// 进程里那条「某个会话的轮换 / 账号格变了」的通道（流连接订它推 `rotation_changed`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(256).0)
}

fn write_locked<R>(path: &Path, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book, Book), String> {
    use std::io::Write as _;
    let shown = path.display().to_string();
    let failed = |e: &dyn std::fmt::Display| {
        copy_text(
            "beRotation.write.failed",
            &[("path", &shown), ("e", &e.to_string())],
        )
    };
    let dir = path
        .parent()
        .ok_or_else(|| copy_text("beRotation.write.noParent", &[("path", &shown)]))?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| failed(&e))?;
    let _lock = crate::platform::lock::hold(dir)?;
    let before = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        Read::Unreadable(e) => {
            return Err(copy_text(
                "beRotation.write.unreadable",
                &[("path", &shown), ("e", &e)],
            ))
        }
    };
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before {
        return Ok((r, before, after));
    }
    let body = serde_json::to_string(&after).map_err(|e| failed(&e))?;
    let tmp = dir.join(format!(
        "{FILE_NAME}.{}.{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| failed(&e))?;
        file.write_all(body.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|e| failed(&e))?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| failed(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map(|()| (r, before, after))
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
    v.as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|n| THRESHOLD_RANGE.contains(n))
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

/// 上限的一个值：`1..=99`，或 `[{at: "HH:MM-HH:MM", n: 1..=99}, …]`（至少一段）。
fn cap_value(v: &Value, cell: &str) -> Result<CapValue, String> {
    if let Some(n) = pct_in_range(v) {
        return Ok(CapValue::N(n));
    }
    let bad = || {
        format!(
            "`{cell}` must be an integer 1..=99 or [{{\"at\":\"HH:MM-HH:MM\",\"n\":1..=99}}, …]"
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
            .and_then(pct_in_range)
            .ok_or_else(|| format!("`{cell}[{i}].n` must be an integer 1..=99"))?;
        slots.push(CapSlot {
            at: at.to_string(),
            n,
        });
    }
    Ok(CapValue::Slots(slots))
}

/// 读一份轮换：键 `order` · `enabled` · `when`，可选 `atLimit`（`"continue"` · `"stop"`，缺 ⇒ `continue`）·
/// `cap`（`{号: {窗口键|"*": 1..=99 | [{at, n}]}}`）· `stint`（`{号: {窗口键|"*": 1..=99}}`）· `preempt`（布尔，缺 ⇒ 关）。`start_slots` ＝ 起始账号占位该有几个（默认恰好 1；会话自己那份 0 或 1）。
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
        "rotation must be an object {order, enabled, when, atLimit?, cap?, stint?, preempt?}",
    )?;
    if let Some(k) = o.keys().find(|k| {
        !matches!(
            k.as_str(),
            "order" | "enabled" | "when" | "atLimit" | "cap" | "stint" | "preempt"
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
    let when = match o.get("when") {
        Some(Value::String(s)) if s == "full" => RotationWhen::Full,
        Some(Value::Object(m)) if m.len() == 1 && m.contains_key("threshold") => {
            let n = m["threshold"]
                .as_object()
                .filter(|t| t.len() == 1)
                .and_then(|t| t.get("n"))
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| THRESHOLD_RANGE.contains(n))
                .ok_or("`when.threshold.n` must be an integer 1..=99")?;
            RotationWhen::Threshold { n }
        }
        _ => return Err("`when` must be \"full\" or {\"threshold\":{\"n\":1..=99}}".into()),
    };
    let at_limit = match o.get("atLimit") {
        None => AtLimit::Continue,
        Some(Value::String(s)) if s == "continue" => AtLimit::Continue,
        Some(Value::String(s)) if s == "stop" => AtLimit::Stop,
        Some(_) => return Err("`atLimit` must be \"continue\" or \"stop\"".into()),
    };
    let cap = per_account(o.get("cap"), "cap", account_ok, &cap_value)?;
    let stint = per_account(o.get("stint"), "stint", account_ok, &|v, cell| {
        pct_in_range(v).ok_or_else(|| format!("`{cell}` must be an integer 1..=99"))
    })?;
    let preempt = match o.get("preempt") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("`preempt` must be true or false".into()),
    };
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
        when,
        at_limit,
        cap,
        stint,
        preempt,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
