//! 连接本身那几条帧命令的类型（`ping` · `cancel` · `stream-watch`）：入参 · 回包 · 码各是一个类型（`typed.rs`）。
//! `ping` 是普通的异步档；后两条要碰本连接的在跑表 / 在看名单 ⇒ 帧面硬臂，本体在 `stream::inbound::dispatch` 那条硬臂里。

use crate::wire::typed::{self, Command, Cx, Handle, NoArgs};

typed::codes! {
    /// `ping` · `cancel` 没有别的码。
    pub(crate) enum NoErr {}
}

/// `ping`：问活，零载荷、零出参。
pub(crate) struct Ping;

impl Command for Ping {
    const NAME: &'static str = "ping";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = ();
    type Err = NoErr;
}

impl Handle for Ping {
    type Needs = typed::needs::Nothing;
    fn run(
        _req: NoArgs,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<(), NoErr>> + Send + 'static {
        std::future::ready(Ok(()))
    }
}

/// `cancel` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct CancelReq {
    /// 要撤的那条命令的 `id`；撤不存在的也回 `ok`。
    pub(crate) target: String,
}

/// `cancel`：撤掉一条在跑的命令（结果是 `cancelled` 帧，应答只回 ok；撤不动的回 `not_cancellable`）。
pub(crate) struct Cancel;

impl Command for Cancel {
    const NAME: &'static str = "cancel";
    type Req = CancelReq;
    type Reply = ();
    type Err = NoErr;
}

typed::codes! {
    /// `stream-watch` 没有别的码。
    pub(crate) enum StreamWatchErr {}
}

/// `stream-watch`：这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）。入参 [`cmd_wire::watch::WatchAsk`] · 回包 [`cmd_wire::watch::WatchReply`]。
pub(crate) struct StreamWatch;

impl Command for StreamWatch {
    const NAME: &'static str = "stream-watch";
    type Req = cmd_wire::watch::WatchAsk;
    type Reply = cmd_wire::watch::WatchReply;
    type Err = StreamWatchErr;
}
