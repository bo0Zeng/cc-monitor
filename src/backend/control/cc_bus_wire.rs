//! cc-bus 那几条帧命令的形状：入参 · 回包 · 码表 · 怎么跑。
//! 发信一条（`bus-send`）：核信封（[`super::cc_bus_envelope`]，纯函数）→ 范围（[`super::cc_bus_scope`]）→ 规矩与落盘（[`super::cc_bus::store`]）。
//! 读的几条（`bus-list` · `bus-inbox` · `bus-peek` · `bus-unread`）与推已读位置那一条（`bus-commit`）本体在 [`super::cc_bus`]。

use super::{cc_bus, cc_bus_envelope};
use crate::common::said::Said;
use crate::wire::typed::{self, Codes, Command, NoArgs};
use copy_core::copy_text;

/// 本体回的失败 ⇒ 这一条的码表，那一句照这一条的「码 → 句」表换（下层那句进原话）。
fn of<E: Codes>(fallback: fn(String) -> E, said: impl Fn(&str) -> String) -> impl Fn(Said) -> E {
    move |f| typed::of_fail_said(f, fallback, |c| Some(said(c)))
}

/// 查在线（`bus-list`）被拒成 `code` 时给人看的那一句（只剩读不成那一种）。
pub(crate) fn list_said(_code: &str) -> String {
    copy_text("ccBus.online.otherCode", &[])
}

/// 读收件箱（`bus-inbox` · `bus-peek` · `bus-unread`）被拒成 `code` 时给人看的那一句。
pub(crate) fn read_said(code: &str) -> String {
    match code {
        "bad_id" => copy_text("ccBus.read.badId", &[]),
        _ => copy_text("ccBus.read.otherCode", &[]),
    }
}

/// 推已读位置（`bus-commit`）被拒成 `code` 时的那一句（`cc-bus commit` · Stop 钩子照码分退出码，那一句印到 stderr）。
pub(crate) fn commit_said(code: &str) -> String {
    match code {
        "stale" => copy_text("beCcBus.commit.stale", &[]),
        "changed" => copy_text("beCcBus.commit.changed", &[]),
        "bad_token" => copy_text("beCcBus.commit.badToken", &[]),
        "io_failed" => copy_text("beCcBus.store.failed", &[]),
        other => read_said(other),
    }
}

/// 发信（`bus-send`）被拒成 `code` 时给发信的 agent 看的那一句（`cc-bus send` 原样印到 stderr）。
/// 哪一格、什么值进原话（复制详情），不进这一句。
pub(crate) fn send_said(code: &str) -> String {
    match code {
        "bad_from" => copy_text("beCcBus.envelope.badFrom", &[]),
        "bad_to" => copy_text("beCcBus.envelope.badTo", &[]),
        "bad_tier" => copy_text("beCcBus.envelope.badTier", &[]),
        "bad_title" => copy_text("beCcBus.envelope.badTitle", &[]),
        "need_re" => copy_text("beCcBus.envelope.needRe", &[]),
        "bad_re" => copy_text("beCcBus.envelope.badRe", &[]),
        "bad_ref" => copy_text("beCcBus.envelope.badRef", &[]),
        "too_long" => copy_text("beCcBus.envelope.tooLong", &[]),
        "not_same_group" => copy_text("beCcBus.envelope.notSameGroup", &[]),
        "no_roster" => copy_text("beCcBus.envelope.noRoster", &[]),
        "duplicate" => copy_text("beCcBus.envelope.duplicate", &[]),
        "throttled" => copy_text("beCcBus.envelope.throttled", &[]),
        "io_failed" => copy_text("beCcBus.store.failed", &[]),
        "offline" => copy_text("beCcBus.envelope.offline", &[]),
        _ => copy_text("beCcBus.envelope.badArgs", &[]),
    }
}

/// 总线名单的一行（`bus-list`）：一个收件箱。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusAgent {
    /// 会话 id（收件人就写它）。
    pub(crate) id: String,
    /// 叫它什么（出口照摆）：这台有它的记录 ⇒ 会话标题；没有 ⇒ 照实就是会话 id。
    pub(crate) title: String,
    /// 待读条数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) unread: i64,
    /// 这条会话此刻活着没有：`true` / `false` / `null`（判不了，**不是**「不在」）。
    pub(crate) live: Option<bool>,
}

/// `bus-list` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusList {
    pub(crate) agents: Vec<BusAgent>,
}

/// 一封信（规范信封）：`bus-send` 盖出来、原样写进收件箱的那一行，`bus-inbox` 读回的也是它。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusEnvelope {
    /// 消息 id（`re` · `msg:` 引的就是它）。
    pub(crate) id: String,
    /// 时间（UTC，`YYYY-MM-DDTHH:MM:SSZ`）。
    pub(crate) ts: String,
    /// 发件人（会话 id）。
    pub(crate) from: String,
    /// 收件人（会话 id）。
    pub(crate) to: String,
    /// 1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。
    pub(crate) tier: u8,
    /// 这一封是做什么的（线上码）。
    pub(crate) act: cmd_wire::bus::BusAct,
    /// 标题（一行）。
    pub(crate) title: String,
    /// 接在哪一条后面（消息 id），没有 ⇒ `null`。
    pub(crate) re: Option<String>,
    /// 引用（路径 · `pb:` · `msg:` · `commit:` · URL）。
    pub(crate) refs: Vec<String>,
    /// 正文。
    pub(crate) text: String,
}

/// 收件箱里的一封：信封原样 ＋ 给人看的动作词（后端照码取文案表写好，界面照摆）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusInboxItem {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) envelope: BusEnvelope,
    /// 动作码显示成的那个词（问 · 答 · 请求 · 告知 · 交回 · 更正）。
    pub(crate) act_said: String,
}

/// 动作码 ⇒ 给人看的那个词（只这一处取，界面不按码分支）。
pub(crate) fn act_said(a: cmd_wire::bus::BusAct) -> String {
    use cmd_wire::bus::BusAct as A;
    match a {
        A::Ask => copy_text("ccBus.act.ask", &[]),
        A::Answer => copy_text("ccBus.act.answer", &[]),
        A::Request => copy_text("ccBus.act.request", &[]),
        A::Inform => copy_text("ccBus.act.inform", &[]),
        A::Handback => copy_text("speaker.tag.handback", &[]),
        A::Correct => copy_text("ccBus.act.correct", &[]),
    }
}

/// `bus-inbox` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusInbox {
    pub(crate) messages: Vec<BusInboxItem>,
    /// 读不懂的行数（不是规范信封的也算）。
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
    /// 先过形状判定，不过 ⇒ `bad_id`。
    pub(crate) id: String,
    /// 1..=2000，缺 ⇒ 200。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) lines: Option<u64>,
}

/// `bus-send` 的入参：发信人要发的那一封（`cc-bus send` 组好交来）。各格的规矩在 [`cc_bus_envelope`]。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusEnvelopeReq {
    /// 发件人：会话 id。
    pub(crate) from: String,
    /// 收件人：会话 id。
    pub(crate) to: String,
    /// 收件会话在哪台（机器表里的名字）。缺 ⇒ 本机；别台 ⇒ 经跨台那一跳交给那台收（`dial::remote_ask::hop`）。
    #[serde(default = "cmd_wire::Machine::local")]
    #[cfg_attr(test, ts(as = "Option<cmd_wire::Machine>", optional))]
    pub(crate) machine: cmd_wire::Machine,
    /// 1 ⇒ 敲门（对方闲着才送）；2 ⇒ 只进收件箱。缺 ⇒ 2。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) tier: Option<u8>,
    /// 这一封是做什么的（线上码；写错的码在解入参那一下就拒）。
    pub(crate) act: cmd_wire::bus::BusAct,
    /// 一行，不超过 80 字。
    pub(crate) title: String,
    /// 接在哪一条后面（消息 id）；答 · 交回 · 更正必带。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) re: Option<String>,
    /// 引用，只核前缀形状。缺 ⇒ 没有引用。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) refs: Option<Vec<String>>,
    /// 正文，不超过 1200 字节（长的写成文件，给路径）。缺 ⇒ 只有标题。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) text: Option<String>,
}

/// `bus-send` 的应答：收进收件箱的那一封 ＋ 敲了门没有（tier 1、这条会话去抖窗里没敲过、投进了它的输入队；敲门是收件那台的后端投的）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusSent {
    pub(crate) envelope: BusEnvelope,
    pub(crate) knock: bool,
}

/// `bus-peek` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusPeekReq {
    /// 会话 id（先过形状判定）。
    pub(crate) id: String,
    /// 这一段最多几封（分段排空积压）；缺 ⇒ 不分段。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) max_lines: Option<u64>,
}

/// `bus-peek` 的应答：给 agent 读的那一段 ＋ 令牌（交给 `bus-commit`）。没有新的 ⇒ 正文空、令牌 `null`。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusPeeked {
    pub(crate) text: String,
    pub(crate) token: Option<String>,
    /// 这一段几封。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) count: u64,
    /// 读不懂、跳过的几行（它们不会再出现）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) skipped: u64,
}

/// `bus-commit` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusCommitReq {
    pub(crate) id: String,
    /// `bus-peek` 给的令牌。
    pub(crate) token: String,
}

/// `bus-commit` 的应答：已读位置推到了第几行。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusCommitted {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) read: u64,
}

/// `bus-unread` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusUnreadReq {
    pub(crate) id: String,
}

/// `bus-unread` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct BusUnread {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) unread: u64,
}

typed::codes! {
    /// `bus-list` 的码。
    pub(crate) enum BusReadErr {
        Failed = "failed",
    }
}

typed::codes! {
    /// `bus-inbox` · `bus-peek` · `bus-unread` 的码。
    pub(crate) enum BusIdErr {
        BadId = "bad_id",
        Failed = "failed",
    }
}

typed::codes! {
    /// `bus-commit` 的码。
    pub(crate) enum BusCommitErr {
        BadId = "bad_id",
        /// 令牌坏了（不是 `bus-peek` 给的那一形 · 自相矛盾）。
        BadToken = "bad_token",
        /// 盘上的已读位置不是令牌的 from：别人推过了（手上那段照常处理，不重试）。
        Stale = "stale",
        /// 令牌末行那一行在两跳之间变了：不推。
        Changed = "changed",
        IoFailed = "io_failed",
    }
}

typed::codes! {
    /// `bus-send` 的码：每条校验一个码（次序即校验次序）。`act` 是类型，写错在解入参时就是 `bad_args`。
    pub(crate) enum BusSendErr {
        BadFrom = "bad_from",
        BadTo = "bad_to",
        BadTier = "bad_tier",
        BadTitle = "bad_title",
        NeedRe = "need_re",
        BadRe = "bad_re",
        BadRef = "bad_ref",
        TooLong = "too_long",
        /// 收件人不在发件人那几组里（范围，`cc_bus_scope`）。
        NotSameGroup = "not_same_group",
        /// 问不到组员名单（不退回成全放行）。
        NoRoster = "no_roster",
        /// 去重窗里同一封已经投过（`cc_bus::rules`）。
        Duplicate = "duplicate",
        /// 这一对或这台超了限流（`cc_bus::rules`）。
        Throttled = "throttled",
        /// 收件箱或状态文件读写不成（读不出来的不覆盖）。
        IoFailed = "io_failed",
        /// 收件会话那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）：两边一个字节没写。
        Offline = "offline",
    }
}

/// `bus-list`：各收件箱 ＋ 叫它什么 · 在不在 ＋ 待读多少。
/// 它的 Handle 住帧面宿主（`faces/mod.rs`）：在不在问观测层那一处，control 不引用 observe。
pub(crate) struct BusListCmd;
impl Command for BusListCmd {
    const NAME: &'static str = "bus-list";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = BusList;
    type Err = BusReadErr;
}

/// `bus-list` 的本体：名单读收件箱那一处，每条会话的那两样问 `seen`；码表照这一条的换。
pub(crate) fn bus_list(seen: &dyn Fn(&str) -> cc_bus::Seen) -> Result<BusList, BusReadErr> {
    cc_bus::list_for_inbound(seen).map_err(of(BusReadErr::Failed, list_said))
}

/// `bus-inbox`：只读看一个收件箱的尾巴。
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

/// `bus-peek`：两阶段读的第一跳（纯读）。
pub(crate) struct BusPeekCmd;
impl Command for BusPeekCmd {
    const NAME: &'static str = "bus-peek";
    type Req = BusPeekReq;
    type Reply = BusPeeked;
    type Err = BusIdErr;
}
impl crate::wire::typed::Handle for BusPeekCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusPeekReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusPeeked, BusIdErr>> + Send + 'static {
        async move { cc_bus::peek_for_inbound(&req).map_err(of(BusIdErr::Failed, read_said)) }
    }
}

/// `bus-commit`：两阶段读的第二跳（锁里推已读位置）。
pub(crate) struct BusCommitCmd;
impl Command for BusCommitCmd {
    const NAME: &'static str = "bus-commit";
    type Req = BusCommitReq;
    type Reply = BusCommitted;
    type Err = BusCommitErr;
}
impl crate::wire::typed::Handle for BusCommitCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: BusCommitReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusCommitted, BusCommitErr>> + Send + 'static
    {
        let (_cx, own) = cx.split();
        async move {
            cc_bus::commit_for_inbound(&own, &req).map_err(of(BusCommitErr::IoFailed, commit_said))
        }
    }
}

/// `bus-unread`：未读几封（只读）。
pub(crate) struct BusUnreadCmd;
impl Command for BusUnreadCmd {
    const NAME: &'static str = "bus-unread";
    type Req = BusUnreadReq;
    type Reply = BusUnread;
    type Err = BusIdErr;
}
impl crate::wire::typed::Handle for BusUnreadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusUnreadReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusUnread, BusIdErr>> + Send + 'static {
        async move { cc_bus::unread_for_inbound(&req).map_err(of(BusIdErr::Failed, read_said)) }
    }
}

/// `bus-send`：核信封 · 判范围（组，`cc_bus_scope`）· 盖上 id 与时间 · 照规矩收进收件人的收件箱，回那一封与敲不敲。
pub(crate) struct BusSendCmd;
impl Command for BusSendCmd {
    const NAME: &'static str = "bus-send";
    type Req = BusEnvelopeReq;
    type Reply = BusSent;
    type Err = BusSendErr;
}
impl crate::wire::typed::Handle for BusSendCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: BusEnvelopeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusSent, BusSendErr>> + Send + 'static {
        let (cx, own) = cx.split();
        async move {
            let tz = cx.tz.clone();
            send(
                &own,
                &req,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
                || super::cc_bus_scope::groups_now(tz),
            )
            .await
        }
    }
}

/// 拒的那一条（码 ＋ 原话）⇒ `bus-send` 的码表，那一句照「码 → 句」表换；原话记诊断日志一行。
fn refused(s: Said) -> BusSendErr {
    tracing::info!("cc-bus send refused: {} {}", s.code_or_fallback(), s.said);
    of(BusSendErr::BadArgs, send_said)(s)
}

/// 收件会话在这台的那一半：盖上 id 与时间 · 照 `roster`（这台读到的组名单）判范围 · 去重限流 · 收进收件箱。
/// 别台交来的（跨台那一跳）也是这一半在收件那台跑。
pub(crate) fn send_here(
    own: &crate::platform::Own,
    req: &BusEnvelopeReq,
    roster: Result<&crate::history::groups::GroupsRead, &str>,
    now_ms: i64,
) -> Result<BusSent, Said> {
    let env = cc_bus_envelope::envelope_now(req)?;
    super::cc_bus_scope::scope(&env.from, &env.to, roster)?;
    let knock = cc_bus::store::deliver(own, &env, now_ms)?;
    Ok(BusSent {
        envelope: env,
        knock,
    })
}

/// `bus-send` 的本体：照 `req.machine` 分。
/// - 本机 ⇒ 读这台的组名单（`roster`）· [`send_here`]。
/// - 别台 ⇒ 先在这台核信封各格（坏信封不出门），再经跨台那一跳（`dial::remote_ask::hop`，会话输入队那三条也走它）交给那台的常驻：
///   范围照那台的名单判、收进那台的收件箱、一档由那台敲门（投进那台那条会话的输入队），那台答的原样交回；
///   够不到 ⇒ `offline`，这台一个字节不写、不攒。
pub(crate) async fn send<R, F>(
    own: &crate::platform::Own,
    req: &BusEnvelopeReq,
    tz: &crate::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn crate::dial::remote_ask::Remote,
    roster: R,
) -> Result<BusSent, BusSendErr>
where
    R: FnOnce() -> F,
    F: std::future::Future<Output = Result<crate::history::groups::GroupsRead, String>>,
{
    if req.machine.is_local() {
        let r = roster().await;
        let now = crate::common::time::now_ms() as i64;
        return send_here(own, req, r.as_ref().map_err(String::as_str), now).map_err(refused);
    }
    cc_bus_envelope::check(req).map_err(refused)?;
    let here = BusEnvelopeReq {
        machine: cmd_wire::Machine::local(),
        ..req.clone()
    };
    crate::dial::remote_ask::hop::<BusSendCmd>(
        &req.machine,
        &here,
        BusSendErr::ALL,
        tz,
        until,
        table,
        remote,
    )
    .await
    .map_err(of(BusSendErr::BadArgs, send_said))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_hop_tests.rs"]
mod hop_tests;
