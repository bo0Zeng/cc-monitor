//! cc-bus 那三条帧命令的形状：入参 · 回包 · 码表 · 怎么跑。
//! 读的两条（`bus-list` · `bus-inbox`）本体在 [`super::cc_bus`]（转调 cc-bus 的脚本、读它的输出）；
//! 信封那一条（`bus-envelope`）本体在 [`super::cc_bus_envelope`]（唯一一处校验，纯函数，不起进程）。

use super::{cc_bus, cc_bus_envelope};
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

/// 信封（`bus-envelope`）被拒成 `code` 时给发信的 agent 看的那一句（`cc-send` 原样印到 stderr）。
/// 哪一格、什么值进原话（复制详情），不进这一句。
pub(crate) fn envelope_said(code: &str) -> String {
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
        _ =>copy_text("beCcBus.envelope.badArgs", &[]),
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

/// 一封信（规范信封）：`bus-envelope` 盖出来、`cc-send` 原样写进收件箱的那一行，`bus-inbox` 读回的也是它。
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
        A::Handback => copy_text("ccBus.act.handback", &[]),
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
    /// 交给 `cc-log` 之前先过形状判定，不过 ⇒ `bad_id`、一个进程都不起。
    pub(crate) id: String,
    /// 1..=2000，缺 ⇒ 200。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) lines: Option<u64>,
}

/// `bus-envelope` 的入参：发信人要发的那一封（`cc-send` 组好交来）。各格的规矩在 [`cc_bus_envelope`]。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct BusEnvelopeReq {
    /// 发件人：会话 id。
    pub(crate) from: String,
    /// 收件人：会话 id。
    pub(crate) to: String,
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
    /// `bus-envelope` 的码：每条校验一个码（次序即校验次序）。`act` 是类型，写错在解入参时就是 `bad_args`。
    pub(crate) enum BusEnvelopeErr {
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
    }
}

/// `bus-list`：各收件箱 ＋ 叫它什么 · 在不在 ＋ 待读多少（起子进程并等它退出 ⇒ 阻塞档，下同）。
/// 它的 Handle 住帧面宿主（`faces/mod.rs`）：在不在问观测层那一处，control 不引用 observe。
pub(crate) struct BusListCmd;
impl Command for BusListCmd {
    const NAME: &'static str = "bus-list";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = BusList;
    type Err = BusReadErr;
}

/// `bus-list` 的本体：名单问 cc-bus，每条会话的那两样问 `seen`；码表照这一条的换。
pub(crate) fn bus_list(seen: &dyn Fn(&str) -> cc_bus::Seen) -> Result<BusList, BusReadErr> {
    cc_bus::list_for_inbound(seen).map_err(of(BusReadErr::Failed, list_said))
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

/// `bus-envelope`：核一封信、判范围（组，`cc_bus_scope`）、盖上 id 与时间。各格的校验是纯函数；范围那一步读组名单（问常驻 · 读这台）。
pub(crate) struct BusEnvelopeCmd;
impl Command for BusEnvelopeCmd {
    const NAME: &'static str = "bus-envelope";
    type Req = BusEnvelopeReq;
    type Reply = BusEnvelope;
    type Err = BusEnvelopeErr;
}
impl crate::wire::typed::Handle for BusEnvelopeCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: BusEnvelopeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BusEnvelope, BusEnvelopeErr>> + Send + 'static
    {
        let tz = cx.tz.clone();
        async move {
            let refused = |(c, raw): cc_bus_envelope::Refusal| {
                of(BusEnvelopeErr::BadArgs, envelope_said)((c.to_string(), raw))
            };
            let env = cc_bus_envelope::envelope_now(&req).map_err(refused)?;
            let roster = super::cc_bus_scope::groups_now(tz).await;
            super::cc_bus_scope::scope(&env.from, &env.to, roster.as_ref().map_err(String::as_str))
                .map_err(refused)?;
            Ok(env)
        }
    }
}
