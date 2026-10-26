//! cc-bus 那三条帧命令的形状：入参 · 回包 · 码表 · 怎么跑。本体（转调 cc-bus 的脚本、读它的输出）在 [`super::cc_bus`]。

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

/// 读收件箱（`bus-inbox`）被拒成 `code` 时给人看的那一句。
pub(crate) fn read_said(code: &str) -> String {
    match code {
        "not_installed" => copy_text("ccBus.read.notInstalled", &[]),
        "child_timed_out" => copy_text("ccBus.read.timedOut", &[]),
        "bad_id" => copy_text("ccBus.read.badId", &[]),
        _ => copy_text("ccBus.read.otherCode", &[]),
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

/// 总线名单的一行（`bus-list`）。
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

typed::codes! {
    /// `bus-list` 的码。
    pub(crate) enum BusReadErr {
        NotInstalled = "not_installed",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
    }
}

typed::codes! {
    /// `bus-inbox` 的码。
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
