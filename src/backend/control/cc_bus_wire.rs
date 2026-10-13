//! cc-bus 那七条帧命令的形状：入参 · 回包 · 码表 · 怎么跑。本体（转调 cc-bus 的脚本、读它的输出）在 [`super::cc_bus`]。

use super::cc_bus;
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::typed::{self, Blocking, Codes, Command, Cx, NoArgs};

/// 本体回的那一对（码 ＋ 那一句）⇒ 这一条的码表。
fn of<E: Codes>(fallback: fn(String) -> E) -> impl Fn((String, String)) -> E {
    move |pair| typed::of_fail(Fail::from(pair), fallback)
}

/// 总线名单的一行（`bus-list` · `bus-broadcast` 挑人都读它）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusAgent {
    /// 总线身份。
    pub(crate) id: String,
    /// tmux 地址。
    pub(crate) target: String,
    /// 待读条数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) unread: i64,
    /// 这个地址今天还在不在：`true` / `false` / `null`（问不到身份空间，**不是**「不在」）。
    pub(crate) live: Option<bool>,
    /// 那个会话绑的会话 id，没绑 ⇒ `null`。
    pub(crate) ccm_sid: Option<String>,
}

/// `bus-list` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusList {
    pub(crate) agents: Vec<BusAgent>,
}

/// `bus-state` 名册的一行（比 [`BusAgent`] 多登记时间）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusRosterRow {
    pub(crate) id: String,
    /// 登记的 pane 地址。
    pub(crate) target: String,
    /// 登记时间，cc-bus 原样。
    pub(crate) registered_at: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) unread: i64,
    /// 三态，同 [`BusAgent::live`]。
    pub(crate) live: Option<bool>,
    pub(crate) ccm_sid: Option<String>,
}

/// `cc-spawn` 派生过的一个会话。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusSpawned {
    pub(crate) id: String,
    /// 派生时的目录。
    pub(crate) dir: String,
    /// 派生时间。
    pub(crate) spawned_at: String,
    /// 余下全部，含 TAB。
    pub(crate) task: String,
    /// 三态：`true` / `false` / `null` = 核不了。
    pub(crate) live: Option<bool>,
}

/// `bus-state` 的应答：总线名单 ＋ spawn 台账一次回全。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusState {
    pub(crate) agents: Vec<BusRosterRow>,
    pub(crate) spawned: Vec<BusSpawned>,
    /// 两张表里读不懂的行数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) skipped: usize,
}

/// 收件箱的一条。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusMessage {
    /// 发件人。
    pub(crate) from: String,
    /// 时间。
    pub(crate) ts: String,
    /// 正文。
    pub(crate) text: String,
    /// 消息类别（cc-bus 原样）。
    pub(crate) class: String,
}

/// `bus-inbox` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusInbox {
    pub(crate) messages: Vec<BusMessage>,
    /// 读不懂的行数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) skipped: usize,
    /// 回显超过 4 MiB ⇒ 保尾，`true`。
    pub(crate) truncated: bool,
}

/// `bus-inbox` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusInboxReq {
    /// 交给 `cc-log` 之前先过形状判定，不过 ⇒ `bad_id`、一个进程都不起。
    pub(crate) id: String,
    /// 1..=2000，缺 ⇒ 200。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) lines: Option<u64>,
}

/// `bus-kill` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusKillReq {
    /// 总线身份。
    pub(crate) id: String,
}

/// `bus-kill` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusKilled {
    pub(crate) id: String,
    /// 会话真的被杀了。
    pub(crate) killed: bool,
    /// 身份对不上：只摘掉那条陈旧登记，会话与收件箱都没动。
    pub(crate) stale_only: bool,
}

/// `bus-send` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusSendReq {
    /// 收件人身份。
    pub(crate) to: String,
    pub(crate) text: String,
    /// 以谁的身份发（可缺；空白当没给）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) from: Option<String>,
}

/// `bus-send` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusSent {
    pub(crate) to: String,
    /// 投出去了（恒 `true`）。
    pub(crate) sent: bool,
    /// 在总线名单里吗。
    pub(crate) registered: bool,
    /// 那个会话今天活着吗，与 `bus-list` 同一套三态。
    pub(crate) live: Option<bool>,
    /// 实际以谁的身份发的（没给 ⇒ `null`）。
    pub(crate) from: Option<String>,
}

/// `bus-broadcast` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusBroadcastReq {
    /// 正文（非空）。
    pub(crate) text: String,
    /// 以谁的身份发，也用来「不发给自己」（可缺）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) from: Option<String>,
}

/// 广播里没投成的一个（键刻意不叫 `code` / `message`：那一对是整条失败的错误信封）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusUndelivered {
    pub(crate) id: String,
    /// `bus-send` 那一套码。
    pub(crate) error: String,
    /// 原话。
    pub(crate) detail: String,
}

/// `bus-broadcast` 的应答：发到几个 · 因不在线跳过几个 · 失败的逐个列。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusBroadcast {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) sent: usize,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) skipped_offline: usize,
    /// 问不到身份空间（全是 `null`），退回发给所有登记的。
    pub(crate) liveness_unknown: bool,
    pub(crate) failed: Vec<BusUndelivered>,
}

/// `bus-spawn` 的入参。`account` 与 `base: true` 恰好给一个（都不给 ⇒ 拒：那是替人选了默认号）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusSpawnReq {
    /// 哪一家（缺 / 空 ⇒ 默认那一家）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) tool: Option<String>,
    /// 工作目录（非空）。
    pub(crate) dir: String,
    /// 交给新会话的任务（可缺）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) task: Option<String>,
    /// 用哪个号起。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) account: Option<String>,
    /// 显式不注入账号。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) base: Option<bool>,
}

/// `bus-spawn` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusSpawnedNow {
    /// 恒 `true`。
    pub(crate) spawned: bool,
    /// 新会话的总线身份；从回显里认不出 ⇒ `null`（起了，只是名字没认出来，**不是**没起来）。
    pub(crate) id: Option<String>,
    /// `cc-spawn` 的原始回显，给人看。
    pub(crate) said: String,
}

typed::codes! {
    /// 只读那两条（`bus-list` · `bus-state`）的码。
    pub(crate) enum BusReadErr {
        NotInstalled = "not_installed",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
    }
}

typed::codes! {
    /// `bus-broadcast` 的码（`bad_id`：给的 `from` 形状不对，一个人都没发）。
    pub(crate) enum BusBroadcastErr {
        NotInstalled = "not_installed",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
        BadId = "bad_id",
    }
}

typed::codes! {
    /// `bus-kill` · `bus-spawn` · `bus-inbox` 的码。
    pub(crate) enum BusIdErr {
        BadId = "bad_id",
        NotInstalled = "not_installed",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
    }
}

typed::codes! {
    /// `bus-send` 的码。
    pub(crate) enum BusSendErr {
        BadId = "bad_id",
        NotInstalled = "not_installed",
        Rejected = "rejected",
        ChildTimedOut = "child_timed_out",
        TooLong = "too_long",
        Failed = "failed",
    }
}

/// `bus-list`：谁在线 ＋ 各自待读多少（起子进程并等它退出 ⇒ 阻塞档，下同）。
pub(crate) struct BusListCmd;
impl Command for BusListCmd {
    const NAME: &'static str = "bus-list";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = BusList;
    type Err = BusReadErr;
}
impl Blocking for BusListCmd {
    fn run(_req: NoArgs, _cx: &Cx) -> Result<BusList, BusReadErr> {
        cc_bus::list_for_inbound().map_err(of(BusReadErr::Failed))
    }
}

/// `bus-state`：总线名单 ＋ spawn 台账一次回全。
pub(crate) struct BusStateCmd;
impl Command for BusStateCmd {
    const NAME: &'static str = "bus-state";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = BusState;
    type Err = BusReadErr;
}
impl Blocking for BusStateCmd {
    fn run(_req: NoArgs, _cx: &Cx) -> Result<BusState, BusReadErr> {
        cc_bus::state_for_inbound().map_err(of(BusReadErr::Failed))
    }
}

/// `bus-inbox`：只读看一个 agent 收件箱的尾巴。
pub(crate) struct BusInboxCmd;
impl Command for BusInboxCmd {
    const NAME: &'static str = "bus-inbox";
    type Req = BusInboxReq;
    type Reply = BusInbox;
    type Err = BusIdErr;
}
impl Blocking for BusInboxCmd {
    fn run(req: BusInboxReq, _cx: &Cx) -> Result<BusInbox, BusIdErr> {
        cc_bus::inbox_for_inbound(&req).map_err(of(BusIdErr::Failed))
    }
}

/// `bus-kill`：收掉一个总线成员。
pub(crate) struct BusKillCmd;
impl Command for BusKillCmd {
    const NAME: &'static str = "bus-kill";
    type Req = BusKillReq;
    type Reply = BusKilled;
    type Err = BusIdErr;
}
impl Blocking for BusKillCmd {
    fn run(req: BusKillReq, _cx: &Cx) -> Result<BusKilled, BusIdErr> {
        cc_bus::kill_for_inbound(&req).map_err(of(BusIdErr::Failed))
    }
}

/// `bus-send`：发一条消息。
pub(crate) struct BusSendCmd;
impl Command for BusSendCmd {
    const NAME: &'static str = "bus-send";
    type Req = BusSendReq;
    type Reply = BusSent;
    type Err = BusSendErr;
}
impl Blocking for BusSendCmd {
    fn run(req: BusSendReq, _cx: &Cx) -> Result<BusSent, BusSendErr> {
        cc_bus::send_for_inbound(&req).map_err(of(BusSendErr::Failed))
    }
}

/// `bus-broadcast`：给总线上在线的成员群发一条。
pub(crate) struct BusBroadcastCmd;
impl Command for BusBroadcastCmd {
    const NAME: &'static str = "bus-broadcast";
    type Req = BusBroadcastReq;
    type Reply = BusBroadcast;
    type Err = BusBroadcastErr;
}
impl Blocking for BusBroadcastCmd {
    fn run(req: BusBroadcastReq, _cx: &Cx) -> Result<BusBroadcast, BusBroadcastErr> {
        cc_bus::broadcast_for_inbound(&req).map_err(of(BusBroadcastErr::Failed))
    }
}

/// `bus-spawn`：派生一个协作 agent（**会起一个真 agent 进程，烧额度**）。
pub(crate) struct BusSpawnCmd;
impl Command for BusSpawnCmd {
    const NAME: &'static str = "bus-spawn";
    type Req = BusSpawnReq;
    type Reply = BusSpawnedNow;
    type Err = BusIdErr;
}
impl Blocking for BusSpawnCmd {
    fn run(req: BusSpawnReq, _cx: &Cx) -> Result<BusSpawnedNow, BusIdErr> {
        cc_bus::spawn_for_inbound(&req).map_err(of(BusIdErr::Failed))
    }
}
