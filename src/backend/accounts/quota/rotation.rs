//! 轮换的存取：配置表 ＋ 每个会话的那一份，落 `~/.cc-monitor/rotation.json`（第二版形状，`version: 2`）。
//!
//! - **配置**有名字、不绑会话：一份策略的参数值 ＝ 用哪份策略 · 参与的号（每行写全这份策略的每号参数）· 整份参数 · 新加的号缺省。
//!   本机默认 ＝ 指向其中一份（`defaultConfig`），永远至少一份；没有文件时是内置的「默认配置 1 · 2 · 3」，默认指向 1（＝ 不轮换）。
//! - **会话**打开一份配置的副本（`config: {id, rev, copy?}`）：改了写 `copy`、跟原样相等就丢；手动调整（`manual`）· 策略的备忘
//!   （`memo`）· 在用的号 · 换号记录都跟会话走。按会话 id 存，后端重启后还在；会话换了起它的号 ⇒ 钉号随之清掉。
//! - 读到第一版（没有 `version`）⇒ 在锁里换成第二版写回，原文件留一份 `rotation.json.v1`（[`migrate`]）。
//! - 判「换不换、换谁」只在 [`super::decide`]；副本的改 · 存 · 还原只在 [`doc`]；这里只有形状与存取。

pub(crate) mod doc;
pub(crate) mod migrate;

use crate::common::said::Said;
use crate::platform::fs::{Own, Ro, Stamp};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

/// 盘上这一版的形状号。
pub(crate) const BOOK_VERSION: u64 = 2;

/// 每个会话至多留几条换号记录（最早的先丢）。
pub(crate) const HISTORY_KEPT: usize = 32;

/// 配置名至多几个字（按字符数）。
pub(crate) const NAME_MAX: usize = 24;

/// 内置三份配置：（id, 用哪份策略）。没有文件时就是这三份，默认指向第一份。
pub(crate) const SEEDS: [(&str, &str); 3] = [
    ("c_00000001", "1"),
    ("c_00000002", "2"),
    ("c_00000003", "3"),
];

/// 写配置那一下若不是会话 ⇒ 记这个（设置窗）。
pub(crate) const SAVED_BY_SETTINGS: &str = "settings";

/// 参数值：参数名 → 值（形状照那份策略的声明，[`super::strategy::form`] 校验）。
pub type Params = serde_json::Map<String, Value>;

/// 配置里一行是谁：起会话的号 · 账号库里其余的订阅号 · 点名的一个号。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Who {
    Start,
    Rest,
    Id(String),
}

impl Who {
    /// 命令里指一行的写法：`@start` · `@rest` · 号 id。
    pub(crate) fn key(&self) -> &str {
        match self {
            Who::Start => "@start",
            Who::Rest => "@rest",
            Who::Id(a) => a,
        }
    }

    pub(crate) fn of_key(k: &str) -> Self {
        match k {
            "@start" => Who::Start,
            "@rest" => Who::Rest,
            a => Who::Id(a.to_string()),
        }
    }
}

/// 配置里的一行：谁 ＋ 这一行的每号参数（写全这份策略声明的每一格）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "AcctRowWire", into = "AcctRowWire")]
pub struct AcctRow {
    pub who: Who,
    pub params: Params,
}

/// 一行在线上的样子：`{"start": true, params}` · `{"rest": true, params}` · `{"id": "team", params}`（三者恰有其一）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = "../../frontend/ui/generated/", rename = "AcctRow")
)]
pub struct AcctRowWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub rest: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub id: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub params: Params,
}

impl TryFrom<AcctRowWire> for AcctRow {
    type Error = String;
    fn try_from(w: AcctRowWire) -> Result<Self, String> {
        let who = match (w.start, w.rest, w.id) {
            (Some(true), None, None) => Who::Start,
            (None, Some(true), None) => Who::Rest,
            (None, None, Some(id)) if !id.is_empty() => Who::Id(id),
            _ => {
                return Err(
                    "an account row is exactly one of {start: true} · {rest: true} · {id}".into(),
                )
            }
        };
        Ok(Self {
            who,
            params: w.params,
        })
    }
}

impl From<AcctRow> for AcctRowWire {
    fn from(r: AcctRow) -> Self {
        let (start, rest, id) = match r.who {
            Who::Start => (Some(true), None, None),
            Who::Rest => (None, Some(true), None),
            Who::Id(a) => (None, None, Some(a)),
        };
        Self {
            start,
            rest,
            id,
            params: r.params,
        }
    }
}

/// 一份配置的内容（会话的副本也是这一形）：名字 · 策略 · 参与的号 · 整份参数 · 新加的号缺省 · 换策略时收起的旧参数 · 时区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ConfigBody {
    pub name: String,
    pub strategy: String,
    #[cfg_attr(test, ts(as = "Vec<AcctRowWire>"))]
    pub accounts: Vec<AcctRow>,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub params: Params,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub acct_init: Params,
    /// 换策略时收起的旧参数：策略 id → `{params, acctInit, accounts}`。
    #[serde(default)]
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub parked: BTreeMap<String, Value>,
    /// 时段那几个钟点按哪个时区（IANA 名）：写的那一下由请求信封的 `tz` 盖进来；缺 ⇒ UTC。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub tz: Option<String>,
    /// 时区不明：第一版缺 `tz`、换形那台又说不出自己的时区（[`crate::platform::local_tz::iana_name`]）⇒ 不猜，标这一格，
    /// 上屏需手动（在设置里确认时区）；在设置里存一次（信封带 `tz`）就清掉。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(as = "Option<bool>", optional))]
    pub tz_unknown: bool,
}

impl ConfigBody {
    /// 时刻 `t` 那一刻这份配置的钟比 UTC 快几秒（缺 ⇒ UTC）。
    pub(crate) fn offset_at(&self, t: u64) -> i64 {
        let t = i64::try_from(t).unwrap_or(i64::MAX);
        self.tz
            .as_deref()
            .and_then(crate::common::time::Tz::iana)
            .unwrap_or_default()
            .offset_secs(t)
    }

    /// 时刻 `t` 是这份配置的本地钟一天里的第几分钟。
    pub(crate) fn clock_at(&self, t: u64) -> u32 {
        let local = i128::from(t) + i128::from(self.offset_at(t));
        u32::try_from(local.rem_euclid(86_400) / 60).unwrap_or(0)
    }

    /// 点名的那一行（`rest` 底下的号不在这里）。
    pub(crate) fn row(&self, who: &Who) -> Option<&AcctRow> {
        self.accounts.iter().find(|r| &r.who == who)
    }
}

/// 一份存下的配置：内容 ＋ 版本（每写一次加一，写时带 `ifRev`）· 最后存的时刻 · 谁存的（`settings` 或会话 id）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Config {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub body: ConfigBody,
    #[cfg_attr(test, ts(type = "number"))]
    pub rev: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: u64,
    pub saved_by: String,
}

/// 会话打开的那一份：哪份配置 · 打开时（或跟上时）的版本 · 改过的副本（没改 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenConfig {
    pub(crate) id: String,
    pub(crate) rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) copy: Option<ConfigBody>,
}

/// 手动热换：换到哪个号 · 什么时候。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Hot {
    pub(crate) account: String,
    pub(crate) since: u64,
}

/// 拖过的一个号：放回第几位（0 起）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Place {
    pub(crate) account: String,
    pub(crate) at: usize,
}

/// 手动调整（引擎的积木，跟会话走、不进配置、不算已改）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manual {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hot: Option<Hot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) place: Vec<Place>,
}

impl Manual {
    pub(crate) fn is_empty(&self) -> bool {
        self.hot.is_none() && self.place.is_empty()
    }
}

/// 为什么离开原来那个号（引擎这一侧）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "SwitchLeave"
    )
)]
pub enum Leave {
    /// 目标排到原来那个号上面了。
    #[serde(rename = "top")]
    Outranked,
    /// 原来那个号事实上不能用了 / 策略给它 `()`：`why` 是那一句（`{k: "@line", …}` 这一形）。
    Unusable {
        #[cfg_attr(test, ts(type = "unknown"))]
        why: Value,
    },
    /// 手动热换（不重启）。
    Manual,
    /// 手动换、重启。
    ManualRestart,
    /// 刷新策略以后照策略换走。
    Refresh,
    /// 没号可挑：停发（`from == to`）。
    Held,
    /// 订阅号都满了，留在原号的付费超额上（`from == to`）。
    Overage,
}

/// 换到的那个号策略怎么说：哪份策略 · 那一句 `why`（写字时照那份策略的 `texts` 或 `beRotation.lib.*`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "SwitchPick"
    )
)]
pub struct Pick {
    pub strategy: String,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub why: Value,
}

/// 一条换号记录。`from == to` 的是没换成的那几种（停发 · 留在超额）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SwitchRecord {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    /// `at` 写给人看的样子（回包出口添；内部与记账一律不填）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    pub from: String,
    pub to: String,
    pub leave: Leave,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub pick: Option<Pick>,
    /// 那一刻原号几点重置（知道才有）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub from_resets_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub from_resets_at_text: Option<String>,
    /// 这一条写成的字（`leave` ＋ `pick`；回包出口添，不落盘）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub text: Option<String>,
}

/// 为什么一个号接不上（换号时问 `ready`）。
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub at_rel_text: Option<String>,
}

/// 这个会话发不出去了（停发）：最早回来的那个号（说不出 ⇒ 缺）· 写好的那一句（卡住条照抄）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Blocked {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub earliest: Option<AccountAt>,
    pub text: String,
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
    /// 中转最后一次看见它的时刻（至多一天刷新一次）；`0` ＝ 没记过（按 `since` 算）。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) seen: u64,
    /// 打开的那一份配置（副本在 `copy`）。
    pub(crate) config: OpenConfig,
    #[serde(default, skip_serializing_if = "Manual::is_empty")]
    pub(crate) manual: Manual,
    /// 策略上一判回的 `@memo`（下一判原样交回）；没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<Value>,
    /// 换进 `current` 那一刻它各窗口的用量；换号 / 换了起它的号就清掉重记。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) baseline: Baseline,
    #[serde(default)]
    pub(crate) history: Vec<SwitchRecord>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// 「看见」的时刻隔多久才刷新一次（秒）。
pub(crate) const SEEN_REFRESH: u64 = 86_400;
/// 没改过副本 · 没手动调整 · 没换过号的会话，多久没被看见就从账本里清掉（秒）。
pub(crate) const DROP_AFTER: u64 = 7 * 86_400;

impl SessionEntry {
    pub(crate) fn fresh(agent: &str, start: &str, now: u64, open: OpenConfig) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            seen: now,
            config: open,
            manual: Manual::default(),
            memo: None,
            baseline: Baseline::new(),
            history: Vec::new(),
        }
    }

    /// 最后一次看见它的时刻（没记过 ⇒ 按 `since`）。
    pub(crate) fn last_seen(&self) -> u64 {
        self.seen.max(self.since)
    }
}

/// 整份 `rotation.json`（第二版）：配置表 · 本机默认 · 每个会话。读进来那一刻补齐「至少一份配置、默认指向一份在的」。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Book {
    pub(crate) default_config: String,
    pub(crate) configs: BTreeMap<String, Config>,
    pub(crate) sessions: BTreeMap<String, SessionEntry>,
    /// 读进来的是第一版 ⇒ 原文（写回第二版时另存一份 `rotation.json.v1`）；不落盘。
    pub(crate) from_v1: Option<Value>,
}

/// 盘上第二版的那一形（读写都经它）。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BookWire {
    version: u64,
    default_config: String,
    configs: BTreeMap<String, Config>,
    #[serde(default)]
    sessions: BTreeMap<String, SessionEntry>,
}

impl Serialize for Book {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            version: u64,
            default_config: &'a str,
            configs: &'a BTreeMap<String, Config>,
            sessions: &'a BTreeMap<String, SessionEntry>,
        }
        Wire {
            version: BOOK_VERSION,
            default_config: &self.default_config,
            configs: &self.configs,
            sessions: &self.sessions,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Book {
    /// 有 `version: 2` ⇒ 照第二版读；没有 `version` ⇒ 第一版，换形（[`migrate::from_v1`]）；别的版本 ⇒ 读不懂（不覆盖）。
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let v = Value::deserialize(d)?;
        let mut b = match v.get("version") {
            None => {
                let local = crate::platform::local_tz::iana_name();
                let mut b = migrate::from_v1(&v, local.as_deref()).map_err(D::Error::custom)?;
                b.from_v1 = Some(v);
                b
            }
            Some(n) if n.as_u64() == Some(BOOK_VERSION) => {
                let w = BookWire::deserialize(v).map_err(D::Error::custom)?;
                Book {
                    default_config: w.default_config,
                    configs: w.configs,
                    sessions: w.sessions,
                    from_v1: None,
                }
            }
            Some(n) => {
                return Err(D::Error::custom(format!(
                    "unknown rotation.json version {n}"
                )))
            }
        };
        b.settle(0);
        Ok(b)
    }
}

impl Default for Book {
    /// 没有文件时那一份：内置三份配置，默认指向第一份。
    fn default() -> Self {
        let mut b = Book {
            default_config: String::new(),
            configs: BTreeMap::new(),
            sessions: BTreeMap::new(),
            from_v1: None,
        };
        b.settle(0);
        b
    }
}

/// 一份内置配置（`默认配置 N`）：
/// 1 ＝ 起会话的号 · 用满才换 · 都到上限后接着用（不轮换）；2 · 3 ＝ 起会话的号 ＋ 其余订阅号 · 到 90% · 停下。
pub(crate) fn seed_config(n: usize, now: u64) -> Option<(String, Config)> {
    let (id, strategy) = SEEDS.get(n)?;
    let s = super::strategy::find(super::strategy::builtins(), strategy)?;
    let mut init = s.acct_init();
    let mut params = s.param_init();
    let mut rows = vec![Who::Start];
    if n == 0 {
        init.insert(super::strategy::form::LINE.into(), Value::Null);
        params.insert(super::strategy::form::ALL_OUT.into(), Value::from("go"));
    } else {
        rows.push(Who::Rest);
    }
    let name = copy_text("beRotation.config.seedName", &[("n", &(n + 1).to_string())]);
    Some((
        id.to_string(),
        Config {
            body: ConfigBody {
                name,
                strategy: strategy.to_string(),
                accounts: rows
                    .into_iter()
                    .map(|who| AcctRow {
                        who,
                        params: init.clone(),
                    })
                    .collect(),
                params,
                acct_init: init,
                parked: BTreeMap::new(),
                tz: None,
                tz_unknown: false,
            },
            rev: 1,
            updated_at: now,
            saved_by: SAVED_BY_SETTINGS.to_string(),
        },
    ))
}

/// 进程内序号：同一秒里连建两份也换一个 id。
static ID_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 一个没用过的配置 id：`c_` ＋ 8 位十六进制（不随改名变）。
pub(crate) fn new_config_id(taken: &BTreeMap<String, Config>) -> String {
    use std::hash::{Hash, Hasher};
    loop {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        crate::common::time::now().hash(&mut h);
        std::process::id().hash(&mut h);
        ID_SEQ
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .hash(&mut h);
        let id = format!("c_{:08x}", h.finish() as u32);
        if !taken.contains_key(&id) && !SEEDS.iter().any(|(s, _)| *s == id) {
            return id;
        }
    }
}

/// 起新会话先定好的 sid（UUID v4 的样子：那一家 `--session-id` 只收这一形）。
pub(crate) fn new_session_id() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let seed = std::collections::hash_map::RandomState::new();
    let half = |salt: u8| {
        let mut h = seed.build_hasher();
        salt.hash(&mut h);
        crate::common::time::now().hash(&mut h);
        std::process::id().hash(&mut h);
        ID_SEQ
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

/// 配置名的比较形：去首尾空白、不分大小写（本机内不许重名按它判）。
pub(crate) fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// 一个会话此刻照的那一份：哪份配置 · 它的内容（有副本用副本）· 版本 · 改没改过。
pub(crate) struct Opened<'a> {
    pub(crate) id: &'a str,
    pub(crate) body: &'a ConfigBody,
    pub(crate) rev: u64,
    pub(crate) dirty: bool,
}

impl Book {
    /// 补齐：没有任何配置 ⇒ 建内置三份、默认指向第一份；默认指向不在的 ⇒ 指向名字排最前的那份。
    pub(crate) fn settle(&mut self, now: u64) {
        if self.configs.is_empty() {
            for n in 0..SEEDS.len() {
                if let Some((id, c)) = seed_config(n, now) {
                    self.configs.insert(id, c);
                }
            }
            self.default_config = SEEDS[0].0.to_string();
        }
        if !self.configs.contains_key(&self.default_config) {
            if let Some((id, _)) = self
                .configs
                .iter()
                .min_by(|a, b| a.1.body.name.cmp(&b.1.body.name))
            {
                self.default_config = id.clone();
            }
        }
    }

    /// 本机默认那份的打开形（新会话照它开）。
    pub(crate) fn open_default(&self) -> OpenConfig {
        OpenConfig {
            id: self.default_config.clone(),
            rev: self.configs.get(&self.default_config).map_or(0, |c| c.rev),
            copy: None,
        }
    }

    /// 这个会话此刻照的那一份：有副本 ⇒ 副本；打开的那份不在了 ⇒ 本机默认那份。走不到 `None`（补齐过）。
    pub(crate) fn opened<'a>(&'a self, s: &'a SessionEntry) -> Option<Opened<'a>> {
        let (id, c) = match self.configs.get_key_value(&s.config.id) {
            Some((id, c)) => (id.as_str(), c),
            None => {
                let (id, c) = self.configs.get_key_value(&self.default_config)?;
                return Some(Opened {
                    id: id.as_str(),
                    body: &c.body,
                    rev: c.rev,
                    dirty: false,
                });
            }
        };
        Some(match &s.config.copy {
            Some(copy) => Opened {
                id,
                body: copy,
                rev: s.config.rev,
                dirty: true,
            },
            None => Opened {
                id,
                body: &c.body,
                rev: c.rev,
                dirty: false,
            },
        })
    }

    /// 开着这份配置的会话（按 sid）。
    pub(crate) fn in_use(&self, id: &str) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.config.id == id)
            .map(|(sid, _)| sid.clone())
            .collect()
    }

    /// 中转第一次看见这个会话 / 会话换了起它的号 ⇒ 记下（新会话打开本机默认那份；换了起它的号 ⇒ 钉号 · 手动调整 · 备忘清掉）。
    /// 每天头一次看见已知的会话 ⇒ 刷新「看见」的时刻；新会话被看见时顺手清旧会话（[`Book::drop_stale`]）。改了 ⇒ `true`。
    pub(crate) fn saw(&mut self, sid: &str, agent: &str, start: &str, now: u64) -> bool {
        let open = self.open_default();
        match self.sessions.get_mut(sid) {
            None => {
                self.drop_stale(now);
                self.sessions.insert(
                    sid.to_string(),
                    SessionEntry::fresh(agent, start, now, open),
                );
                true
            }
            Some(s) if s.start != start || s.agent != agent => {
                s.agent = agent.to_string();
                s.start = start.to_string();
                s.current = start.to_string();
                s.since = now;
                s.baseline.clear();
                s.manual = Manual::default();
                s.memo = None;
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

    /// 清旧会话：没改过副本 · 没手动调整 · 没换过号 · 超过 [`DROP_AFTER`] 没被看见 ⇒ 从账本里删（再来一发照新会话记回来）。
    pub(crate) fn drop_stale(&mut self, now: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            !(s.config.copy.is_none()
                && s.manual.is_empty()
                && s.history.is_empty()
                && now.saturating_sub(s.last_seen()) > DROP_AFTER)
        });
        before - self.sessions.len()
    }

    /// 补这一段的基线：`base` 里此刻还没记的窗口记下。改了 ⇒ `true`。
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

    /// ★ 换号落账：只在**真换成**的那一下记（会话此刻还在 `rec.from` 上、`rec.to` 是别的号）—— 别的请求刚把它换走了 ⇒ 不再记第二条。
    /// 换成了 ⇒ `true`（钉到 `to`、这一段的基线清掉重记）。
    pub(crate) fn pin(&mut self, sid: &str, rec: SwitchRecord) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        if s.current != rec.from || rec.to == rec.from {
            return false;
        }
        s.current = rec.to.clone();
        s.since = rec.at;
        s.baseline.clear();
        push(s, rec);
        true
    }

    /// 没换成的那几种（停发 · 留在超额）：自上一次真换号以来同一句已经记过 ⇒ 不再记。改了 ⇒ `true`。
    pub(crate) fn note_stuck(&mut self, sid: &str, rec: SwitchRecord) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        if rec.from != rec.to || !matches!(rec.leave, Leave::Held | Leave::Overage) {
            return false;
        }
        let dup = s
            .history
            .iter()
            .rev()
            .take_while(|h| h.from == h.to)
            .any(|h| h.from == rec.from && h.leave == rec.leave);
        if !dup {
            push(s, rec);
        }
        !dup
    }

    /// 存下这一判策略回的备忘（整个替换；没写 ⇒ 清掉）。改了 ⇒ `true`。
    pub(crate) fn set_memo(&mut self, sid: &str, memo: Option<Value>) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        if s.memo == memo {
            return false;
        }
        s.memo = memo;
        true
    }

    /// 清掉手动热换（热换的号事实上不能用了 / 策略给它 `()`）。改了 ⇒ `true`。
    pub(crate) fn clear_hot(&mut self, sid: &str) -> bool {
        self.sessions
            .get_mut(sid)
            .is_some_and(|s| s.manual.hot.take().is_some())
    }
}

fn push(s: &mut SessionEntry, rec: SwitchRecord) {
    s.history.push(rec);
    let over = s.history.len().saturating_sub(HISTORY_KEPT);
    s.history.drain(..over);
}

// ── 落盘 ─────────────────────────────────────────────────────────────────

/// 这台的落点：`<家>/rotation.json`（家同额度账）。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    super::ledger::path_from(get).map(|p| p.with_file_name(FILE_NAME))
}

pub(crate) fn path_now() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

/// 第一版换形前的原文留在哪：`rotation.json.v1`（与 [`FILE_NAME`] 同一层）。
fn v1_name() -> String {
    format!("{FILE_NAME}.v1")
}

/// 读一次盘。三态：没有（没动过）/ 读得懂 / 读不懂（不覆盖）。
pub(crate) type Read = crate::common::own_state::Read<Book>;

/// 读盘的上限。
const MAX_BYTES: u64 = 16 << 20;

pub(crate) fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct RotationStore {
    path: Option<PathBuf>,
    /// （盘上的戳, 上一份读懂的, 此刻读不懂的话为什么）。读不懂 ⇒ 照上一份读懂的接着用、不覆盖盘上那份。
    cache: Mutex<(Option<Stamp>, Book, Option<Said>)>,
}

impl RotationStore {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cache: Mutex::new((None, Book::default(), None)),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 本机策略目录（家推不出 ⇒ `None`）。
    pub(crate) fn strategy_dir(&self) -> Option<PathBuf> {
        self.path.as_deref().and_then(super::strategy::dir_from)
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 当作缺省；读到第一版 ⇒ 当场换形写回）。
    pub(crate) fn now(&self) -> Book {
        let read = {
            let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let Some(p) = self.path.as_deref() else {
                return g.1.clone();
            };
            let s = Ro::stamp(p);
            if s.is_some() && s == g.0 {
                return g.1.clone();
            }
            let b = match read_at(p) {
                Read::Present(b) => b,
                Read::Absent => Book::default(),
                // 读不懂 ⇒ 照上一份读懂的接着挑号（没有过 ⇒ 内置那一份），把为什么留着交给会话那一份；盘上那份不动。
                Read::Unreadable(e) => {
                    if g.0 != s {
                        tracing::warn!(
                            "[rotation] {} 读不懂，照上一份读懂的接着用：{}",
                            p.display(),
                            e.logged()
                        );
                    }
                    g.0 = s;
                    g.2 = Some(e);
                    return g.1.clone();
                }
            };
            g.2 = None;
            // 第一版 ⇒ 只在内存里换形（`from_v1` 记着），不写盘：写回只在接了中转的那个进程起来时做一次（[`migrate_here`]）。
            g.1 = b.clone();
            g.0 = s;
            b
        };
        read
    }

    /// 盘上那份此刻读不懂 ⇒ 为什么（一句 ＋ 原话）；读得懂 · 没有 ⇒ `None`。
    pub(crate) fn unreadable(&self) -> Option<Said> {
        self.now();
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .2
            .clone()
    }

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的会话各响一下（[`changes`]）。读不懂的那一份不覆盖。
    /// 外面只经两扇门进来：中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）。
    fn change<R>(&self, own: &Own, f: impl FnOnce(&mut Book) -> R) -> Result<R, Said> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let (r, before, after) = match self.path.as_deref() {
            None => {
                let before = g.1.clone();
                let r = f(&mut g.1);
                (r, before, g.1.clone())
            }
            Some(path) => {
                let (r, before, after) = write_locked(own, false, f)?;
                g.0 = Ro::stamp(path);
                g.1 = after.clone();
                g.2 = None;
                let base = seen_swap(path, g.0, &after).unwrap_or(before.clone());
                (r, base, after)
            }
        };
        drop(g);
        ring_changed(&before, &after);
        Ok(r)
    }
}

/// 本进程最后见过的盘上那一份（按路径）。别的进程写了 ⇒ 戳对不上 ⇒ 重扫时比对它推。
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
    let st = Ro::stamp(path);
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

/// 本机策略目录里的文件变了：每个会话都响一下（它此刻照的那份策略可能变了），配置那条通道也响一下。
fn strategies_moved(path: &Path) {
    let b = match read_at(path) {
        Read::Present(b) => b,
        _ => Book::default(),
    };
    for sid in b.sessions.keys() {
        let _ = changes().send(sid.clone());
    }
    let _ = configs_changes().send(());
}

/// 盯 `rotation.json` 所在的目录（只认这个文件名）与本机策略目录：别的进程写了 ⇒ [`rescan`]；策略文件变了 ⇒ 重推。
/// 返回的那一份活着就一直盯。起的时候先记下此刻那一份（之后的改动才有得比）。
/// 那两层目录（数据目录 · 策略目录）不在就经写句柄建。
pub(crate) fn watch(own: &Own) -> Result<crate::platform::watch_file::Watching, String> {
    let no_home = || copy_text("beRotation.read.noHome", &[]);
    let root = own.root().ok_or_else(no_home)?;
    let sub = own.path(super::strategy::DIR_NAME).map_err(|e| e.said)?;
    let file = own.path(FILE_NAME).map_err(|e| e.said)?;
    own.ensure_dir(&root).map_err(|e| e.to_string())?;
    own.ensure_dir(&sub).map_err(|e| e.to_string())?;
    let dir = root.as_path().to_path_buf();
    let sdir = sub.as_path().to_path_buf();
    let path = file.as_path();
    rescan(path);
    let target = path.to_path_buf();
    let sdir_for = sdir.clone();
    crate::platform::watch_file::watch(
        &[(dir, false), (sdir.clone(), false)],
        move |p| {
            p.file_name().and_then(|n| n.to_str()) == Some(FILE_NAME)
                || (p.parent() == Some(sdir_for.as_path())
                    && p.extension().and_then(|x| x.to_str()) == Some("rhai"))
        },
        "rotation-watch",
        move |hit| {
            if hit
                .iter()
                .any(|p| p.extension().and_then(|x| x.to_str()) == Some("rhai"))
            {
                strategies_moved(&target);
            }
            rescan(&target)
        },
    )
}

/// 常驻 / 流那一路的后端起来时调一次：盯这台的 `rotation.json` 与策略目录，进程活着就一直盯。盯不上只出声。
pub(crate) fn watch_here(own: &Own) {
    static HELD: Mutex<Option<crate::platform::watch_file::Watching>> = Mutex::new(None);
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    match watch(own) {
        Ok(w) => *g = Some(w),
        Err(e) => {
            tracing::warn!("[rotation] 盯不上 {FILE_NAME}：{e}（别处写的轮换要等面板重开才看得见）")
        }
    }
}

/// 第一版换形写回（一次）：盘上那份是第一版 ⇒ 在锁里写成第二版、原文另存 `rotation.json.v1`；已是第二版 · 没有 · 读不懂 ⇒ 不动。
/// 只由接上了中转的那个进程起来时调（[`migrate_here`]）：那一刻旧版的中转已经让出了口，不会再有读不懂第二版的读者。
pub(crate) fn migrate_at(own: &Own) -> Result<bool, Said> {
    let file = own.path(FILE_NAME)?;
    if !matches!(read_at(file.as_path()), Read::Present(ref b) if b.from_v1.is_some()) {
        return Ok(false);
    }
    write_locked(own, true, |_| ())?;
    Ok(true)
}

/// 这台的 `rotation.json` 换形写回（[`migrate_at`]）；写不成只出声。
pub(crate) fn migrate_here(own: &Own) {
    match migrate_at(own) {
        Ok(true) => tracing::info!("[rotation] {FILE_NAME} 换成第二版，原文留在 .v1"),
        Ok(false) => {}
        Err(e) => tracing::warn!("[rotation] {FILE_NAME} 换形写回不成：{}", e.logged()),
    }
}

/// 中转那一路的写口（第一次看见会话 · 钉号 · 记一条 · 存备忘）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

/// 帧面那一路装一份本机策略：`<家>/strategies/<id>.rhai`（目录只建这一层，整份原子写）。读不读得懂由调用方先判。
pub(crate) fn strategy_file_write(own: &Own, id: &str, source: &str) -> Result<PathBuf, Said> {
    let dir = own
        .path(super::strategy::DIR_NAME)
        .map_err(|_| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let shown = dir.as_path().display().to_string();
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beRotation.write.failed",
                &[("path", &shown), ("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let file = dir.join(&format!("{id}.rhai"))?;
    let held = own.lock(&dir)?;
    own.write(&file, &held, source.as_bytes())?;
    Ok(file.as_path().to_path_buf())
}

/// 帧面那一路删一份本机策略（只删本机策略目录里的那一份）。
pub(crate) fn strategy_file_remove(own: &Own, path: &Path) -> Result<(), Said> {
    let refuse = || Said::from(copy_text("beRotation.read.noHome", &[]));
    let dir = own.path(super::strategy::DIR_NAME).map_err(|_| refuse())?;
    if path.parent() != Some(dir.as_path()) {
        return Err(refuse());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(refuse)?;
    let file = dir.join(name)?;
    own.delete_file(&file).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beRotation.write.failed",
                &[
                    ("path", &path.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 帧面那一路的写口（存配置 · 改副本 · 手动调整 · 现在就换）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    own: &Own,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, Said> {
    store.change(own, f)
}

/// 改到的会话各响一下：它那一格变了，或它此刻照的那份配置变了；配置表 / 默认指向变了 ⇒ 配置那条通道也响一下。
fn ring_changed(before: &Book, after: &Book) {
    let configs_moved =
        before.configs != after.configs || before.default_config != after.default_config;
    for (sid, s) in &after.sessions {
        let moved = match before.sessions.get(sid) {
            Some(b) => {
                b != s
                    || (configs_moved
                        && before.opened(b).map(|o| o.body.clone())
                            != after.opened(s).map(|o| o.body.clone()))
            }
            None => true,
        };
        if moved {
            let _ = changes().send(sid.clone());
        }
    }
    if configs_moved {
        let _ = configs_changes().send(());
    }
}

/// 进程里那条「这台的配置表 / 本机默认 / 策略变了」的通道（流连接订它推 `changed {rotation_configs}`）。
pub(crate) fn configs_changes() -> &'static tokio::sync::broadcast::Sender<()> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> = std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<()>(16).0)
}

/// 进程里那条「某个会话的轮换 / 账号格变了」的通道（流连接订它推 `changed {rotation}`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(256).0)
}

fn write_locked<R>(
    own: &Own,
    migrate: bool,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<(R, Book, Book), Said> {
    let file = own.path(FILE_NAME)?;
    let dir = own
        .root()
        .ok_or_else(|| Said::from(copy_text("beRotation.read.noHome", &[])))?;
    let path = file.as_path();
    let shown = path.display().to_string();
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beRotation.write.failed",
                &[("path", &shown), ("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        )
    })?;
    let held = own.lock(&dir)?;
    let mut before = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        Read::Unreadable(e) => {
            return Err(e.wrap(|said| copy_text("beRotation.write.unreadable", &[("said", said)])))
        }
    };
    let v1 = before.from_v1.take();
    // 盘上还是第一版：只有起来时接了中转的那个进程换形（那时旧版的中转已经让出了口）；别的写一律不写，免得旧版的中转读不懂。
    if v1.is_some() && !migrate {
        return Err(Said::from(copy_text(
            "beRotation.write.v1Pending",
            &[("path", &shown)],
        )));
    }
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before && v1.is_none() {
        return Ok((r, before, after));
    }
    if let Some(orig) = v1 {
        // 第一版换形：原文留一份（已经有了 ⇒ 不覆盖）。
        let backup = own.path(&v1_name())?;
        if !backup.as_path().exists() {
            let body = crate::common::own_state::json_body(backup.as_path(), &orig)?;
            own.write(&backup, &held, &body)?;
        }
    }
    let body = crate::common::own_state::json_body(path, &after)?;
    own.write(&file, &held, &body)?;
    Ok((r, before, after))
}

/// 编辑器里一格填错了：哪一格（`name` · `params.<k>` · `accounts[i].<k>` · `acctInit.<k>`，时段再细到 `[j].at` / `[j].line`）·
/// 短码（`empty` · `dup` · `tooLong` · `type` · `range` · `of` · `time` · `overlap` · `strategy` · `account`）· 那一格下面的那一句（核心写好）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct CellError {
    pub cell: String,
    pub code: String,
    #[cfg_attr(test, ts(type = "string"))]
    pub said: crate::common::cells::Words,
}

/// 一格错（那一句照短码由核心写）。
pub(crate) fn cell_err(cell: String, code: &str) -> CellError {
    let said = match code {
        "empty" => copy_text("beRotation.cell.empty", &[]),
        "dup" => copy_text("beRotation.cell.dup", &[]),
        "tooLong" => copy_text("beRotation.cell.tooLong", &[("n", &NAME_MAX.to_string())]),
        "type" => copy_text("beRotation.cell.type", &[]),
        "range" => copy_text("beRotation.cell.range", &[]),
        "of" => copy_text("beRotation.cell.of", &[]),
        "overlap" => copy_text("beRotation.cell.overlap", &[]),
        "strategy" => copy_text("beRotation.cell.strategy", &[]),
        "account" => copy_text("beRotation.cell.account", &[]),
        _ => copy_text("beRotation.cell.time", &[]),
    };
    CellError {
        cell,
        code: code.to_string(),
        said: crate::common::cells::Words(said),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
