//! cc-bus 那七条帧命令的形状：入参 · 回包 · 码表 · 怎么跑。本体（转调 cc-bus 的脚本、读它的输出）在 [`super::cc_bus`]。

use super::cc_bus;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Codes, Command, NoArgs};
use copy_core::copy_text;

/// 本体回的那一对（码 ＋ 那一句）⇒ 这一条的码表，那一句照这一条的「码 → 句」表换（下层那句进原话）。
fn of<E: Codes>(
    fallback: fn(String) -> E,
    said: impl Fn(&str) -> String,
) -> impl Fn((String, String)) -> E {
    move |pair| typed::of_fail_said(Fail::from(pair), fallback, |c| Some(said(c)))
}

/// 查在线（`bus-list`）被拒成 `code` 时给人看的那一句。
pub(crate) fn list_said(code: &str) -> String {
    match code {
        "not_installed" => copy_text("ccBus.online.notInstalled", &[]),
        "child_timed_out" => copy_text("ccBus.online.timedOut", &[]),
        _ => copy_text("ccBus.online.otherCode", &[]),
    }
}

/// 读名册 / 收件箱（`bus-state` · `bus-inbox`）被拒成 `code` 时给人看的那一句。
pub(crate) fn read_said(code: &str) -> String {
    match code {
        "not_installed" => copy_text("ccBus.read.notInstalled", &[]),
        "child_timed_out" => copy_text("ccBus.read.timedOut", &[]),
        "bad_id" => copy_text("ccBus.read.badId", &[]),
        _ => copy_text("ccBus.read.otherCode", &[]),
    }
}

/// 收掉一个成员（`bus-kill`）被拒成 `code` 时给人看的那一句。
pub(crate) fn kill_said(id: &str, code: &str) -> String {
    match code {
        "bad_args" => copy_text("ccBus.kill.invalidArgs", &[("id", id)]),
        "bad_id" => copy_text("ccBus.kill.badId", &[]),
        "not_installed" => copy_text("ccBus.kill.notInstalled", &[("id", id)]),
        "child_timed_out" => copy_text("ccBus.kill.timedOut", &[("id", id)]),
        _ => copy_text("ccBus.kill.otherCode", &[("id", id)]),
    }
}

/// 发一条（`bus-send`）被拒成 `code` 时给人看的那一句。`id` 是收件人。
pub(crate) fn send_said(id: &str, code: &str) -> String {
    match code {
        "bad_args" => copy_text("ccBus.send.invalidArgs", &[("id", id)]),
        "bad_id" => copy_text("ccBus.send.badId", &[]),
        "not_installed" => copy_text("ccBus.send.notInstalled", &[("id", id)]),
        "rejected" => copy_text("ccBus.send.rejected", &[("id", id)]),
        "child_timed_out" => copy_text("ccBus.send.timedOut", &[("id", id)]),
        "too_long" => copy_text("ccBus.send.tooLong", &[("id", id)]),
        _ => copy_text("ccBus.send.otherCode", &[("id", id)]),
    }
}

/// 新开一个 agent（`bus-spawn`）被拒成 `code` 时给人看的那一句。
pub(crate) fn spawn_said(code: &str) -> String {
    match code {
        "bad_args" => copy_text("ccBus.spawn.invalidArgs", &[]),
        "bad_id" => copy_text("ccBus.spawn.badId", &[]),
        "not_installed" => copy_text("ccBus.spawn.notInstalled", &[]),
        "child_timed_out" => copy_text("ccBus.spawn.timedOut", &[]),
        _ => copy_text("ccBus.spawn.otherCode", &[]),
    }
}

/// 广播（`bus-broadcast`）被拒成 `code` 时给人看的那一句。
pub(crate) fn broadcast_said(code: &str) -> String {
    match code {
        "bad_args" => copy_text("ccBus.broadcast.invalidArgs", &[]),
        "bad_id" => copy_text("ccBus.broadcast.badId", &[]),
        "not_installed" => copy_text("ccBus.broadcast.notInstalled", &[]),
        "child_timed_out" => copy_text("ccBus.broadcast.timedOut", &[]),
        _ => copy_text("ccBus.broadcast.otherCode", &[]),
    }
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
    /// 等它报到那一问（`session-arrive` 的入参，原样交回）。
    pub(crate) arrival: cmd_wire::session::ArriveAsk,
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
impl crate::wire::typed::Handle for BusListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusList, BusReadErr>> + Send + 'static {
        async move { cc_bus::list_for_inbound().map_err(of(BusReadErr::Failed, list_said)) }
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
impl crate::wire::typed::Handle for BusStateCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusState, BusReadErr>> + Send + 'static {
        async move { cc_bus::state_for_inbound().map_err(of(BusReadErr::Failed, read_said)) }
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
impl crate::wire::typed::Handle for BusInboxCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusInboxReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusInbox, BusIdErr>> + Send + 'static {
        async move { cc_bus::inbox_for_inbound(&req).map_err(of(BusIdErr::Failed, read_said)) }
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
impl crate::wire::typed::Handle for BusKillCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusKillReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusKilled, BusIdErr>> + Send + 'static {
        async move {
            cc_bus::kill_for_inbound(&req).map_err(of(BusIdErr::Failed, |c| kill_said(&req.id, c)))
        }
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
impl crate::wire::typed::Handle for BusSendCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusSendReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusSent, BusSendErr>> + Send + 'static {
        async move {
            cc_bus::send_for_inbound(&req)
                .map_err(of(BusSendErr::Failed, |c| send_said(&req.to, c)))
        }
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
impl crate::wire::typed::Handle for BusBroadcastCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusBroadcastReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusBroadcast, BusBroadcastErr>> + Send + 'static
    {
        async move {
            cc_bus::broadcast_for_inbound(&req).map_err(of(BusBroadcastErr::Failed, broadcast_said))
        }
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
impl crate::wire::typed::Handle for BusSpawnCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusSpawnReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusSpawnedNow, BusIdErr>> + Send + 'static {
        async move { cc_bus::spawn_for_inbound(&req).map_err(of(BusIdErr::Failed, spawn_said)) }
    }
}
