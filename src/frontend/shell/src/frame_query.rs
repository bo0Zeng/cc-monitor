//! **只读查询走已有的长连接** —— 层 1 在 monitor 侧的那一半。
//!
//! # 它换掉的是什么
//!
//! 「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那八条（`--list-projects` / `--list-sessions` /
//! `--read-session` / `--read-session-tail` / `--session-accounts` / `--list-accounts` /
//! `--search` 等）此前每问一次就新拨一条 TCP+SSH+鉴权，exec 一次后端、
//! 读完 stdout 就断 —— 而同一台机器上**早就有一条**长连接（流模式那条），
//! 入方向一问一答也早就通了（`inbound_client`）。后端那边这一拍把八条登记上了帧面
//! （`history-*` / `accounts-*`，`src/backend/faces/read_face.rs`），本模块是 monitor 这一侧的发送端。
//!
//! # 走哪条分流
//!
//! 发送端一律走 `backend_route` 的出口（`route_call_error` / `no_channel`），
//! 登记在 `backend_route_tests::SENDERS`。**没有第二条路可回落**：逐次拨号那条就是本件要删的东西 ——
//! 长连接不在时这里**明说**「没有控制通道」，不悄悄再拨一次 SSH。
//! 代价如实写：历史浏览从此依赖那台的流连接活着（此前它是独立拨号，流断了也能翻历史）。
//!
//! # 逐次拨号那条路没有了
//!
//! 不在帧面上的一次性查询从前落到 `remote_history.rs` 的逐次拨号那条路（`run_list_query`〔散文墓碑〕），
//! 而那条路只放行一张「仍拨号」的表 —— C4c 起那张表就是空的（最后两条随账号域上了帧面）。
//! 那条路、那张表与那道闸门一起删了。认不出帧命令的查询**当场说**（`subagent·rs::Backend::query`），
//! 不拨号、不回落；「新长一条逐次拨号的查询」从此在代码里无处可落（判据在 `frame_query_tests.rs`）。
//!
//! # 期限：一件事一个绝对时刻
//!
//! 本模块的每一个出口都收一个 [`Deadline`]，**只用、不造**：发起这件事的那一手（`tasks` · `subagent` · 快照 ·
//! 历史浏览器读整份 · 按行号取一段）在事情开始时造一次（[`Deadline::within`]），之后这件事里的每一问、每一页
//! 都拿**同一个**时刻去等（`InboundClient::call_until`）—— 越往后剩得越少，没有一页会重新拿一整份。
//! 〔墓碑 —— DL1 之前每一问各自 `now + LINES_BUDGET / PAGE_BUDGET`：分页读（`read_lines` · 快照 · 读整份）每页重新计时、
//!  没有总时限，病 2「每 59 s 吐一个字节的对端能拖到无限」。〕
//! 期限的**值**暂住下面那几个常量：「期限的值归谁」待定，定了再搬。

use crate::backend_route::{no_channel, route_call_error, Routed};
use crate::copy_table::copy_text;
use crate::inbound_client;
use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// CLI 子命令 → 帧命令。**判据的一侧**（另一侧从后端源码数，见测试）。
/// 生产段不读它 —— 它是判据的一侧（与 `inbound::CommandSpec` 那几栏同理），故精确 allow。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const MOVED: &[(&str, &str)] = &[
    ("--read-session", "history-read"),
    ("--read-session-tail", "history-tail"),
    ("--search", "history-search"),
    ("--list-accounts", "accounts-list"),
    ("--session-accounts", "accounts-sessions"),
    // 骨架索引与大纲清单：每开一个大会话就要一次（不是「点一次才发一次」）。
    //   `--read-session-from-offset` 的另一形（`--until`，按区间取正文）早就走 `history-read` ——
    //   两形都上了帧面，这个子命令从此一条拨号都不剩。
    ("--read-session-from-offset", "history-index"),
    ("--list-user-inputs", "history-user-inputs"),
    // 会话内查找（Ctrl+F）：同一个处境（新子命令、此前在远端逐次拨号），一起上帧面。
    ("--find-in-session", "history-find"),
    // 换号前的信任预检（随账号域一起上帧面）。两形合进**一条**帧命令
    //   （`configDir` 缺席 / null = 账号 0）⇒ 右列 `accounts-trust` 出现两次，判据按集合比。
    ("--account-trust", "accounts-trust"),
    ("--account-trust-zero", "accounts-trust"),
];

/// **生在帧面上**的只读查询：交给后端只读宿主（`read_face::answer`），但**没有**
/// 一个被它替掉的逐次拨号子命令（与 [`MOVED`] 那几条的来历不同）。判据的一侧：
/// `MOVED` 右列 ∪ 本表 == 后端真登记给只读宿主的那几条。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BORN_ON_FRAME: &[&str] = &[
    // resume 之前问「这条会话的记录还在不在」（monitor 这一侧不再发它：界面经通道直接问）。
    "history-record",
    // 按行号取回一段（没接骨架的会话丢掉的正文从这里要回来）。
    "history-lines",
    // 会话事实出成品（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）。
    //   此前是前端 `onLine` 旁路自己攒的，没有被替掉的拨号子命令；monitor 这一侧从不发它（界面经通道直接问）。
    "history-facts",
    // 主线外清单的冷读（界面经通道直接问；monitor 这一侧从不发它）。
    "history-branch",
    // 一轮的摘要（过程行 · 刻度 · 大纲同源；界面经通道直接问，monitor 这一侧从不发它）。
    "history-turns",
    // 那台后端自己的 stderr 诊断文件尾部（设置页「日志」经通道直接问；monitor 这一侧从不发它）。
    "backend-log",
    // 记录解释进后端之后生在帧面上的两条（界面经通道直接问；monitor 这一侧从不发它们）。
    "history-page",
    "drift-report",
    // 各台搜索结果合一份（合并排序进本机后端；界面经通道直接问，monitor 这一侧从不发它）。
    "history-search-merge",
    // 按运行读一个子运行的记录（替掉按目录 ＋ 描述 ＋ 时间戳挑的那一条与它的列候选子命令；界面经通道直接问）。
    "history-run",
    // 停 / 重启 / 更新 / 卸载之前会打断什么（这台的活会话 ＋ 通往那台的转发；界面经通道问那台与本机，monitor 这一侧从不发它）。
    "machine-interrupts",
];

// 按行一问的期限 `LINES_BUDGET`〔散文墓碑〕删：「按行那几条」最后的发送端（子 agent 列候选）随命令退役。
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
    /// `who` 是 [`who`] 说的那个「谁」（本机 / 远端 [x]）。
    fn overdue(&self, who: &str) -> String {
        copy_text(
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
/// 今天的调用方都在本文件里。
///
/// 期限是调用方给的**那一件事**的 [`Deadline`]：已经到点 ⇒ **一个字节都不发**（同 `src/comms/inward/chan.ts`
/// 「已经过了 ⇒ 一个字节都不发」）；发出去之后到点 ⇒ `InboundClient` 补发 `cancel`，这里说「没在 N 秒内答完」。
pub(crate) async fn call(
    origin: &Origin,
    cmd: &str,
    args: Value,
    deadline: Deadline,
) -> Result<Value, String> {
    let who = who(origin);
    let origin = origin.as_wire_str();
    let Some(client) = inbound_client::client_for(origin) else {
        // 说「谁」用同一个 [`who`]：本机那几问改走 `<local>` 之后，不许把 `<local>` 这个键原样说给人看。
        return Err(said(no_channel(&who)));
    };
    // 能力协商放在发之前：「这台的后端太旧」是问得出答案的，
    // 不许与超时同形。
    if !client.accepts(cmd) {
        return Err(copy_core::backend_old(&who));
    }
    if deadline.passed() {
        return Err(deadline.overdue(&who));
    }
    let data = client
        .call_until(cmd, args, deadline.until)
        .await
        .map_err(|e| {
            if deadline.passed() {
                return deadline.overdue(&who);
            }
            said(route_call_error(&e, &who, |code, message| {
                // 码只进日志；给人看的是哪一问没成 ＋ 那台的原话。
                tracing::warn!("frame query {cmd} refused ({code}): {message}");
                if message.trim().is_empty() {
                    copy_text("rsFrameQuery.call.failedNoReason", &[("who", &who)])
                } else {
                    copy_text(
                        "rsFrameQuery.call.failed",
                        &[("who", &who), ("message", &message.to_string())],
                    )
                }
            }))
        })?;
    data.ok_or_else(|| copy_core::reply_unreadable(&who))
}

/// 三态里给人看的那句话。`Done` 在本族走不到（查询不产「已完成」这一档）。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
    }
}

/// 报错里的「谁」：本机说「本机」，远端说「远端 [x]」。
///
/// 本机那几问从 exec 一次性后端改走 `<local>` 长连接之后，同一句话本机远端共用 ——
/// 不许把本机说成「远端 [<local>]」。
pub(crate) fn who(origin: &Origin) -> String {
    if origin.is_local() {
        copy_text("rsFrameQuery.who.local", &[])
    } else {
        copy_text(
            "rsFrameQuery.who.remote",
            &[("machine", origin.as_wire_str())],
        )
    }
}

// 按行那几条的出口 `lines`〔散文墓碑〕删：最后的调用方（子 agent 列候选 · 按 argv 分流）随命令退役。

/// `history-tail` 那张图（字段同后端 `history_query::TailPlan`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TailPlan {
    pub total: u64,
    pub tail_from: u64,
    pub split_at: u64,
    pub end: u64,
}

/// 问尾段在哪（一问；期限由发起方给，值是 [`PAGE_BUDGET`]）。
pub(crate) async fn tail(
    origin: &Origin,
    path: &str,
    n: u64,
    deadline: Deadline,
) -> Result<TailPlan, String> {
    let data = call(
        origin,
        "history-tail",
        json!({"path": path, "n": n}),
        deadline,
    )
    .await?;
    let who = who(origin);
    let num = |k: &str| {
        data.get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| copy_core::reply_unreadable(&who))
    };
    let plan = TailPlan {
        total: num("total")?,
        tail_from: num("tail_from")?,
        split_at: num("split_at")?,
        end: num("end")?,
    };
    if plan.tail_from > plan.total || plan.split_at > plan.end {
        return Err(copy_text(
            "rsFrameQuery.reply.inconsistent",
            &[("who", &who), ("plan", &format!("{:?}", plan))],
        ));
    }
    Ok(plan)
}

// `history-record` 那一问的发送端（`record` / `parse_record` / `RecordProbe`〔散文墓碑〕）删了：
//   唯一调用方（Tauri 命令 `probe_session_record`）退役，界面经通道直接问后端（`src/frontend/ui/session-reads.ts::probeSessionRecord`，
//   「缺一格不许读成『不在』」那条口径随之搬到 TS 的 `decodeRecord`）。

/// `history-read` 那一页里的一个可计行（后端出的成品：monitor 不解析记录，只搬）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// 这一行（含 `\n`）之后那个字节的偏移（后端按原始字节算，说得准）；残尾 ⇒ `None`。
    pub end: Option<u64>,
    /// 这一行正文的摘要（后端算，跨进程稳定）—— 续传前核「还是不是那一行」。
    pub hash: u64,
    /// 这一行在渲染模型里的样子；`None` ＝ 不进界面（照占号）。
    pub message: Option<crate::ui_contract::RecordBody>,
    /// 这条记录自己的 `cwd`。
    pub cwd: Option<String>,
}

/// `history-read` 的一页。
pub(crate) struct Page {
    /// 这一页里的可计行（次序同文件）。
    pub rows: Vec<Row>,
    /// 下一页从这里起（原始字节偏移）。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 一行成品的形状（后端 `observe/record_page.rs::rows_of`）；不对 ⇒ `None`（整页当形状对不上）。
pub(crate) fn row_of(v: &Value) -> Option<Row> {
    let end = match v.get("end")? {
        Value::Null => None,
        e => Some(e.as_u64()?),
    };
    let message = match v.get("message") {
        None => None,
        Some(m) if m.is_object() => Some(crate::ui_contract::RecordBody::from_json(m.to_string())?),
        Some(_) => return None,
    };
    let cwd = match v.get("cwd") {
        None => None,
        Some(c) => Some(c.as_str()?.to_string()),
    };
    Some(Row {
        end,
        hash: v.get("hash")?.as_u64()?,
        message,
        cwd,
    })
}

/// 读 `[offset, until)` 的**一页**。
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
    let data = call(origin, "history-read", args, deadline).await?;
    let who = who(origin);
    let rows = data
        .get("rows")
        .and_then(Value::as_array)
        .and_then(|rs| rs.iter().map(row_of).collect::<Option<Vec<Row>>>());
    let next = data.get("next").and_then(Value::as_u64);
    let eof = data.get("eof").and_then(Value::as_bool);
    let (Some(rows), Some(next), Some(eof)) = (rows, next, eof) else {
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

// 按行号取一段（`session_lines` / `parse_session_lines`）· 读整段逐行（`read_lines`）·〔散文墓碑〕
//   一次性查询按 argv 分流（`ArgvRoute` / `route_argv` / `refuses` / `run_routed`）〔散文墓碑〕删了：它们的调用方
//   （按行号取回 · 子 agent · 按偏移取一段三条 Tauri 命令）退役，界面经通道直问那台后端的成品（`src/frontend/ui/record-reads.ts`）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/frame_query_tests.rs"]
mod tests;
