//! **只读查询走已有的长连接** —— 层 1 在 monitor 侧的那一半。
//!
//! # 它换掉的是什么
//!
//! 「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那八条（`--list-projects` / `--list-sessions` /
//! `--read-session` / `--read-session-tail` / `--session-accounts` / `--list-accounts` /
//! `--search` 等）此前每问一次就新拨一条 TCP+SSH+鉴权，exec 一次后端、
//! 读完 stdout 就断 —— 而同一台机器上**早就有一条**长连接（流模式那条），
//! 入方向一问一答也早就通了（`inbound_client`）。后端那边把八条登记上了帧面
//! （`history-*` / `accounts-*`，`src/backend/faces/read_face.rs`），本模块是 monitor 这一侧的发送端。
//!
//! # 走哪条分流
//!
//! 发送端一律走 `backend_route` 的出口（`route_call_error` / `no_channel`），
//! 登记在 `backend_route_tests::SENDERS`。**没有第二条路可回落**：逐次拨号那条就是这里要删的东西 ——
//! 长连接不在时这里**明说**「没有控制通道」，不悄悄再拨一次 SSH。
//! 代价如实写：历史浏览从此依赖那台的流连接活着（此前它是独立拨号，流断了也能翻历史）。
//!
//! # 没有逐次拨号
//!
//! 认不出帧命令的查询**当场说**，不拨号、不回落；「新长一条逐次拨号的查询」在代码里无处可落（判据在 `frame_query_tests.rs`）。
//!
//! # 期限：一件事一个绝对时刻
//!
//! 本模块的每一个出口都收一个 [`Deadline`]，**只用、不造**：发起这件事的那一手（`tasks` · `subagent` · 快照 ·
//! 历史浏览器读整份 · 按行号取一段）在事情开始时造一次（[`Deadline::within`]），之后这件事里的每一问、每一页
//! 都拿**同一个**时刻去等（`InboundClient::call_until`）—— 越往后剩得越少，没有一页会重新拿一整份。
//! 期限的**值**暂住下面那几个常量：「期限的值归谁」待定，定了再搬。

use crate::backend_route::{no_channel, route_call_error, Routed};
use crate::chan::said::machine as who;
use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::inbound_client;
use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 一问一页的期限（一次 `history-tail` · 一段 `history-lines`）：与旧逐行读的单次超时同值（60s）。
pub(crate) const PAGE_BUDGET: Duration = Duration::from_secs(60);
/// 分页读一大份时假定的**最低**速率（字节 / 秒）：「最大那一份在不低于它时读得完」。
/// ⚠ 暂定、没有读数（「每条路该给多少秒没有证据」照旧开着）。
pub(crate) const READ_FLOOR_BPS: u64 = 512 * 1024;

/// 分页读 `bytes` 字节那一件的总时限：一页的期限 ＋ 按 [`READ_FLOOR_BPS`] 读完要的秒数（向上取整）。
/// 历史浏览器读整份（事先不知道多大）按它的字节上限给；快照读正文按问图之后已知的字节数给。
pub(crate) fn read_budget(bytes: u64) -> Duration {
    PAGE_BUDGET + Duration::from_secs(bytes.div_ceil(READ_FLOOR_BPS))
}

/// **一件事的总期限**：一个绝对时刻 ＋ 当初给了多少（后者只为说人话）。
///
/// 造它的只有 [`Deadline::within`]，调它的是**发起这件事的那一手**（判据：`frame_query_tests` 的造期限点登记表，两向）；
/// 本模块与 `InboundClient` 只拿它去等，零处重新计时。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadline {
    until: tokio::time::Instant,
    total: Duration,
}

impl Deadline {
    /// 从此刻起 `total`。**唯一**的构造。
    pub(crate) fn within(total: Duration) -> Self {
        Self {
            until: tokio::time::Instant::now() + total,
            total,
        }
    }

    /// 已经到点了吗。
    fn passed(&self) -> bool {
        tokio::time::Instant::now() >= self.until
    }

    /// 到点那句话（说法归发起方：带这件事当初给了多少秒；读查询不说「无法确认远端有没有执行」那句）。
    /// `who` 是那台的称呼（[`who`]）。
    fn overdue(&self, who: &str) -> Said {
        Said::of(
            "rsFrameQuery.call.overdue",
            &[
                ("who", &who.to_string()),
                ("dur", &copy_core::format_elapsed(self.total)),
            ],
        )
    }
}

/// 发一条帧命令，拿 `data`。
///
/// 今天的调用方都在本文件里。`view` ＝ 出口的声明（原样进请求信封；`None` ＝ 全量）。
///
/// 期限是调用方给的**那一件事**的 [`Deadline`]：已经到点 ⇒ **一个字节都不发**（同 `src/comms/inward/chan.ts`
/// 「已经过了 ⇒ 一个字节都不发」）；发出去之后到点 ⇒ `InboundClient` 补发 `cancel`，这里说「没在 N 秒内答完」。
/// 那台拒了 ⇒ 句子就是它那一句、详情是它写的那份（[`Said::of_call`]，远端补「本机」）。
pub(crate) async fn call(
    origin: &Origin,
    cmd: &str,
    args: Value,
    view: Option<&Value>,
    deadline: Deadline,
) -> Result<Value, Said> {
    let who = who(origin);
    let Some(client) = inbound_client::client_for(origin.as_wire_str()) else {
        // 说「谁」用同一个 [`who`]：不许把 `<local>` 这个键原样说给人看。
        return Err(Said::written(said(no_channel(&who))));
    };
    // 能力协商放在发之前：「这台的后端太旧」是问得出答案的，
    // 不许与超时同形。
    if !client.accepts(cmd) {
        return Err(Said::written(copy_core::backend_old(&who)));
    }
    if deadline.passed() {
        return Err(deadline.overdue(&who));
    }
    let answer = match view {
        Some(v) => client.call_until_viewed(cmd, args, v, deadline.until).await,
        None => client.call_until(cmd, args, deadline.until).await,
    };
    let data = answer.map_err(|e| {
        if deadline.passed() {
            return deadline.overdue(&who);
        }
        let sentence = said(route_call_error(&e, &who, |code, message| {
            // 码只进日志（详情里也有）。
            tracing::warn!("frame query {cmd} refused ({code}): {message}");
            crate::chan::said::refused(message, origin)
        }));
        Said::of_call(sentence, cmd, origin, &e)
    })?;
    data.ok_or_else(|| Said::written(copy_core::reply_unreadable(&who)))
}

/// 三态里给人看的那句话。`Done` 在本族走不到（查询不产「已完成」这一档）。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
    }
}

/// `history-tail` 的一问（[`tail`]）：那张图 · 怎么接 · `rows` 第一行的行号 · 第一页 · 读到头之后要存的那一份。
#[derive(Debug)]
pub(crate) struct Tail {
    pub plan: cmd_wire::session::TailPlan,
    pub how: cmd_wire::session::TailHow,
    pub from: u64,
    pub page: Page,
    pub held: cmd_wire::session::Held,
}

/// 问尾段在哪、手上那一份续不续得上，连第一页正文一起拿回来（一问；期限由发起方给，值是 [`PAGE_BUDGET`]）。
pub(crate) async fn tail(
    origin: &Origin,
    path: &str,
    n: u64,
    held: Option<&cmd_wire::session::Held>,
    deadline: Deadline,
) -> Result<Tail, String> {
    let data = call(
        origin,
        "history-tail",
        json!({"path": path, "n": n, "held": held}),
        Some(folded_view()),
        deadline,
    )
    .await
    .map_err(|e| e.said)?;
    let who = who(origin);
    let reply: cmd_wire::session::HistoryTail =
        serde_json::from_value(data).map_err(|_| copy_core::reply_unreadable(&who))?;
    let plan = reply.plan;
    if plan.tail_from > plan.total || plan.split_at > plan.end || reply.from > plan.total {
        return Err(copy_text(
            "rsFrameQuery.reply.inconsistent",
            &[("who", &who), ("plan", &format!("{:?}", plan))],
        ));
    }
    let rows = rows_of(reply.rows).ok_or_else(|| copy_core::reply_unreadable(&who))?;
    Ok(Tail {
        plan,
        how: reply.how,
        from: reply.from,
        page: Page {
            rows,
            next: reply.next,
            eof: reply.eof,
        },
        held: reply.held,
    })
}

/// `history-read` 那一页里的一个可计行（后端出的成品：monitor 不解析记录，只搬）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// 这一行（含 `\n`）之后那个字节的偏移（后端按原始字节算，说得准）；残尾 ⇒ `None`。
    pub end: Option<u64>,
    /// 这一行在渲染模型里的样子；`None` ＝ 不进界面（照占号）。
    pub record: Option<crate::ui_contract::RecordBody>,
    /// 这条记录自己的 `cwd`。
    pub cwd: Option<String>,
}

/// `history-read` 的一页。
#[derive(Debug)]
pub(crate) struct Page {
    /// 这一页里的可计行（次序同文件）。
    pub rows: Vec<Row>,
    /// 下一页从这里起（原始字节偏移）。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 一页的逐行成品（后端 `observe/query/body.rs::rows_of`）⇒ 搬运的那一形；有一行的记录不是一团 ⇒ `None`（整页当形状对不上）。
pub(crate) fn rows_of(rows: Vec<cmd_wire::record::ReadRow>) -> Option<Vec<Row>> {
    rows.into_iter()
        .map(|r| {
            let record = match r.record {
                None => None,
                Some(m) if m.is_object() => {
                    Some(crate::ui_contract::RecordBody::from_json(m.to_string())?)
                }
                Some(_) => return None,
            };
            Some(Row {
                end: r.end,
                record,
                cwd: r.cwd,
            })
        })
        .collect()
}

/// 「折起那一行」那份声明（省掉工具入参 · 结果正文 · 逐段改动 · 一步展开那一格）：只住 `src/shared/views/folded-record.json` 一处，
/// 界面 `record-reads.ts` 读同一份。旁路快照交给界面的行就是折起那一形，展开那一下界面按记录 id 取回全文。
fn folded_view() -> &'static Value {
    static V: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("../../../shared/views/folded-record.json"))
            .expect("src/shared/views/folded-record.json 是 JSON（判据 the_snapshot_page_carries_the_folded_view_from_the_shared_file 读同一份）")
    })
}

/// 读 `[offset, until)` 的**一页**（交折起那一行的声明，[`folded_view`]）。
///
/// ⚠ 续点必须**前进**：后端回一页零字节却说没到头 ⇒ 当场报错，调用方的循环不会空转。
/// `deadline` 是**整件事**的（调用方在读第一页之前造一次，之后每一页传同一个）—— 本函数不重新计时。
pub(crate) async fn read_page(
    origin: &Origin,
    path: &str,
    offset: u64,
    upto: Option<u64>,
    deadline: Deadline,
) -> Result<Page, String> {
    let mut args = json!({"path": path, "offset": offset});
    if let Some(u) = upto {
        args["until"] = json!(u);
    }
    let data = call(origin, "history-read", args, Some(folded_view()), deadline)
        .await
        .map_err(|e| e.said)?;
    let who = who(origin);
    let page: Option<(Vec<Row>, u64, bool)> =
        serde_json::from_value::<cmd_wire::record::HistoryRead>(data)
            .ok()
            .and_then(|p| Some((rows_of(p.rows)?, p.next, p.eof)));
    let Some((rows, next, eof)) = page else {
        return Err(copy_core::reply_unreadable(&who));
    };
    if !eof && next <= offset {
        return Err(copy_text(
            "rsFrameQuery.readPage.stuck",
            &[("who", &who), ("offset", &offset.to_string())],
        ));
    }
    Ok(Page { rows, next, eof })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/frame_query_tests.rs"]
mod tests;
