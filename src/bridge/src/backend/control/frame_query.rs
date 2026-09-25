//! 〔`C1` · 2026-09-24〕**只读查询走已有的长连接** —— `设计/15 §3.2` 层 1 在 monitor 侧的那一半。
//!
//! # 它换掉的是什么
//!
//! `设计/99 §4.19.2 ⑥` 逐字：「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那八条（`--list-projects` / `--list-sessions` /
//! `--read-session` / `--read-session-tail` / `--session-accounts` / `--list-accounts` /
//! `--search` / `--list-subagents`）此前每问一次就新拨一条 TCP+SSH+鉴权，exec 一次后端、
//! 读完 stdout 就断 —— 而同一台机器上**早就有一条**长连接（流模式那条），
//! 入方向一问一答也早就通了（`inbound_client`）。后端那边这一拍把八条登记上了帧面
//! （`history-*` / `accounts-*`，`src/backend/read_face.rs`），本模块是 monitor 这一侧的发送端。
//!
//! # 走哪条分流
//!
//! 发送端一律走 `backend_route` 的出口（`route_call_error` / `no_channel`），
//! 登记在 `backend_route_tests::SENDERS`。**没有第二条路可回落**：逐次拨号那条就是本件要删的东西 ——
//! 长连接不在时这里**明说**「没有控制通道」，不悄悄再拨一次 SSH。
//! 代价如实写：历史浏览从此依赖那台的流连接活着（此前它是独立拨号，流断了也能翻历史）。
//!
//! # 逐次拨号那条路没有了〔C4d · 第四波 4B〕
//!
//! 不在帧面上的一次性查询从前落到 `remote_history.rs` 的逐次拨号那条路（`run_list_query`〔散文墓碑〕），
//! 而那条路只放行一张「仍拨号」的表 —— C4c 起那张表就是空的（最后两条随账号层上了帧面）。
//! 主会话 09-25 裁删：那条路、那张表与那道闸门一起没了。认不出帧命令的查询**当场说**（`subagent·rs::Backend::query`），
//! 不拨号、不回落；「新长一条逐次拨号的查询」从此在代码里无处可落（判据在 `frame_query_tests.rs`）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 题面那八条 ＋〔SR1a〕两条：CLI 子命令 → 帧命令。**判据的一侧**（另一侧从后端源码数，见测试）。
/// 生产段不读它 —— 它是判据的一侧（与 `inbound::CommandSpec` 那几栏同理），故精确 allow。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const MOVED: &[(&str, &str)] = &[
    ("--list-projects", "history-projects"),
    ("--list-sessions", "history-sessions"),
    ("--read-session", "history-read"),
    ("--read-session-tail", "history-tail"),
    ("--search", "history-search"),
    ("--list-subagents", "history-subagents"),
    ("--list-accounts", "accounts-list"),
    ("--session-accounts", "accounts-sessions"),
    // 〔SR1a · 09-24〕骨架索引与大纲清单：每开一个大会话就要一次（不是「点一次才发一次」）。
    //   `--read-session-from-offset` 的另一形（`--until`，按区间取正文）早就走 `history-read` ——
    //   两形都上了帧面，这个子命令从此一条拨号都不剩。
    ("--read-session-from-offset", "history-index"),
    ("--list-user-inputs", "history-user-inputs"),
    // 〔SR1a × SE2〕会话内查找（Ctrl+F）：同一个处境（新子命令、此前在远端逐次拨号），一起上帧面。
    ("--find-in-session", "history-find"),
    // 〔C4c · 第四波 4B〕换号前的信任预检（主会话裁：随账号层一起上帧面）。两形合进**一条**帧命令
    //   （`configDir` 缺席 / null = 账号 0）⇒ 右列 `accounts-trust` 出现两次，判据按集合比。
    ("--account-trust", "accounts-trust"),
    ("--account-trust-zero", "accounts-trust"),
];

/// 〔U4b · 第四波〕**生在帧面上**的只读查询：交给后端只读宿主（`read_face::answer`），但**没有**
/// 一个被它替掉的逐次拨号子命令（与 [`MOVED`] 那几条的来历不同）。判据的一侧：
/// `MOVED` 右列 ∪ 本表 == 后端真登记给只读宿主的那几条。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BORN_ON_FRAME: &[&str] = &[
    // resume 之前问「这条会话的记录还在不在」（〔C4c〕monitor 这一侧不再发它：界面经通道直接问）。
    "history-record",
    // 〔CF2 · 第四波 4B〕按行号取回一段（没接骨架的会话丢掉的正文从这里要回来）。
    "history-lines",
];

/// 按行那几条的期限：30s（与已删的逐次拨号那条路的整体限时同值）。它从此只盖「远端跑查询 ＋ 回程」，不再盖握手。
const LINES_BUDGET: Duration = Duration::from_secs(30);
/// 一页 `history-read` / 一次 `history-tail` 的期限：与旧逐行读的单次超时同值（60s）。
const PAGE_BUDGET: Duration = Duration::from_secs(60);

/// 发一条帧命令，拿 `data`。
///
/// 〔RM1c · 第四波〕开成 `pub(crate)`：代码全景（`panorama_call.rs`）要一个**期限由调用方给**的出口
/// （建索引是分钟级，`lines` 那一档的 30 s 不够）。**不新增发送端** —— 仍是这一处、仍走同一个分流器。
pub(crate) async fn call(
    origin: &Origin,
    cmd: &str,
    args: Value,
    budget: Duration,
) -> Result<Value, String> {
    let origin = origin.as_wire_str();
    let Some(client) = inbound_client::client_for(origin) else {
        return Err(said(no_channel(origin)));
    };
    // 能力协商放在发之前（同 `tmux::capture_via_backend`）：「这台的后端太旧」是问得出答案的，
    // 不许与超时同形。
    if !client.accepts(cmd) {
        return Err(format!(
            "远端 [{origin}] 的后端还不认 `{cmd}` —— 这条查询是后来才上长连接的，重装那台机器的后端就有了"
        ));
    }
    let data = client.call(cmd, args, budget).await.map_err(|e| {
        said(route_call_error(&e, |code, message| {
            format!("远端 [{origin}] 查询失败（{code}）：{message}")
        }))
    })?;
    data.ok_or_else(|| format!("远端 [{origin}] `{cmd}` 的应答没有 data —— 两端契约对不上"))
}

/// 三态里给人看的那句话。`Done` 在本族走不到（查询不产「已完成」这一档）。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => "查询出了内部错误，没有拿到结果".to_string(),
    }
}

/// 按行那六条：`data.lines` 原样拿回（逐行、trim 过、剔空行 —— 与已删的逐次拨号那条路的出参同形）。
pub(crate) async fn lines(origin: &Origin, cmd: &str, args: Value) -> Result<Vec<String>, String> {
    let data = call(origin, cmd, args, LINES_BUDGET).await?;
    let origin = origin.as_wire_str();
    let rows = data
        .get("lines")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("远端 [{origin}] `{cmd}` 的应答没有 `lines` —— 两端契约对不上"))?;
    Ok(rows
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// `history-tail` 那张图（字段同后端 `history_query::TailPlan`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TailPlan {
    pub total: u64,
    pub tail_from: u64,
    pub split_at: u64,
    pub end: u64,
}

/// 问尾段在哪。
pub(crate) async fn tail(origin: &Origin, path: &str, n: u64) -> Result<TailPlan, String> {
    let data = call(
        origin,
        "history-tail",
        json!({"path": path, "n": n}),
        PAGE_BUDGET,
    )
    .await?;
    let origin = origin.as_wire_str();
    let num = |k: &str| {
        data.get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("远端 [{origin}] `history-tail` 的应答缺 `{k}`"))
    };
    let plan = TailPlan {
        total: num("total")?,
        tail_from: num("tail_from")?,
        split_at: num("split_at")?,
        end: num("end")?,
    };
    if plan.tail_from > plan.total || plan.split_at > plan.end {
        return Err(format!(
            "远端 [{origin}] `history-tail` 的应答自相矛盾：{plan:?}"
        ));
    }
    Ok(plan)
}

// 〔C4c · 第四波 4B〕`history-record` 那一问的发送端（`record` / `parse_record` / `RecordProbe`〔散文墓碑〕）删了：
//   唯一调用方（Tauri 命令 `probe_session_record`）退役，界面经通道直接问后端（`src/session-reads.ts::probeSessionRecord`，
//   「缺一格不许读成『不在』」那条口径随之搬到 TS 的 `decodeRecord`）。

/// `history-read` 的一页。
pub(crate) struct Page {
    /// 这一页（后端 UTF-8 有损解码过）。切在行尾，区间末尾那一页例外（余下的全给，含 torn 残尾）。
    pub text: String,
    /// 下一页从这里起（原始字节偏移）。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 读 `[offset, until)` 的**一页**。
///
/// ⚠ 续点必须**前进**：后端回一页零字节却说没到头 ⇒ 当场报错，调用方的循环不会空转。
pub(crate) async fn read_page(
    origin: &Origin,
    path: &str,
    offset: u64,
    upto: Option<u64>,
) -> Result<Page, String> {
    let mut args = json!({"path": path, "offset": offset});
    if let Some(u) = upto {
        args["until"] = json!(u);
    }
    let data = call(origin, "history-read", args, PAGE_BUDGET).await?;
    let origin = origin.as_wire_str();
    let text = data.get("text").and_then(Value::as_str);
    let next = data.get("next").and_then(Value::as_u64);
    let eof = data.get("eof").and_then(Value::as_bool);
    let (Some(text), Some(next), Some(eof)) = (text, next, eof) else {
        return Err(format!(
            "远端 [{origin}] `history-read` 的应答缺 `text`/`next`/`eof` —— 两端契约对不上"
        ));
    };
    if !eof && next <= offset {
        return Err(format!(
            "远端 [{origin}] `history-read` 在偏移 {offset} 处不前进 —— 停下，不空转"
        ));
    }
    Ok(Page {
        text: text.to_string(),
        next,
        eof,
    })
}

/// 〔CF2 · 第四波 4B〕`history-lines` 的一段（字段同后端 `history_query::LinesPage`）。
pub(crate) struct LinesPage {
    /// 第一条的行号。
    pub from: u64,
    /// 可计行的原文（后端只交可计行；第 k 条是第 `from + k` 行）。
    pub lines: Vec<String>,
    /// 下一段从这一行起（恒 `from + lines.len()`）。
    pub next: u64,
    /// 读过了最后一个完整行。
    pub eof: bool,
}

/// 〔CF2 · 第四波 4B〕按**行号**取回第 `[from, upto)` 行的**一段**（`upto` 缺 ＝ 到末尾）。
///
/// 期限同一页 `history-read`（后端要从文件头数到 `from`，与读一页同量级）。
/// ⚠ 应答自己对不上（`from` 不是问的那个 · `next != from + 条数` · 没到头却一条没交）⇒ 当场报错，
/// 调用方的循环不会空转、取回的正文不会落错行号。
pub(crate) async fn session_lines(
    origin: &Origin,
    path: &str,
    from: u64,
    upto: Option<u64>,
) -> Result<LinesPage, String> {
    // 形参叫 `upto`（同 [`read_page`]）：`rust_timer_registry` 的 shell 周期唤醒扫描认「until 空格」。
    let mut args = json!({"path": path, "from": from});
    if let Some(u) = upto {
        args["until"] = json!(u);
    }
    let data = call(origin, "history-lines", args, PAGE_BUDGET).await?;
    parse_session_lines(origin, from, &data)
}

/// [`session_lines`] 的应答解释（纯函数）。
pub(crate) fn parse_session_lines(
    origin: &Origin,
    asked: u64,
    data: &Value,
) -> Result<LinesPage, String> {
    let origin = origin.as_wire_str();
    let from = data.get("from").and_then(Value::as_u64);
    let next = data.get("next").and_then(Value::as_u64);
    let eof = data.get("eof").and_then(Value::as_bool);
    let lines = data.get("lines").and_then(Value::as_array);
    let (Some(from), Some(next), Some(eof), Some(lines)) = (from, next, eof, lines) else {
        return Err(format!(
            "[{origin}] `history-lines` 的应答缺 `from`/`next`/`eof`/`lines` —— 两端契约对不上"
        ));
    };
    let lines: Vec<String> = lines
        .iter()
        .map(|l| l.as_str().map(str::to_string))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("[{origin}] `history-lines` 的 `lines` 里有不是字符串的一格"))?;
    if from != asked || next != from + lines.len() as u64 || (!eof && lines.is_empty()) {
        return Err(format!(
            "[{origin}] `history-lines` 的应答自相矛盾（问第 {asked} 行起，答 from={from} next={next} \
             条数={} eof={eof}）—— 停下，不落错行号",
            lines.len()
        ));
    }
    Ok(LinesPage {
        from,
        lines,
        next,
        eof,
    })
}

/// 读整段区间，收成逐行（trim 过、剔空行）—— 与已删的逐次拨号那条路读 `--read-session` 的出参同形。
pub(crate) async fn read_lines(
    origin: &Origin,
    path: &str,
    from: u64,
    upto: Option<u64>,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut offset = from;
    loop {
        let page = read_page(origin, path, offset, upto).await?;
        out.extend(
            page.text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        );
        offset = page.next;
        if page.eof {
            return Ok(out);
        }
    }
}

/// `subagent::Backend::query` 远端那一支用的：一条 argv 能不能走帧面、走哪条。
///
/// 认得的形状恰好是本仓今天真在发的那几种（`subagent.rs` 与 `session_skeleton.rs` 造的 argv）；
/// 认不出 ⇒ `None`，调用方当场说「这条查询没有帧命令」（〔C4d〕逐次拨号那条路删了，不回落）。
pub(crate) enum ArgvRoute {
    Lines(&'static str, Value),
    Read {
        path: String,
        from: u64,
        upto: Option<u64>,
    },
}

impl ArgvRoute {
    /// 这条路走的是哪条帧命令（报错与「认不认」那一问用）。
    pub(crate) fn frame_cmd(&self) -> &'static str {
        match self {
            ArgvRoute::Lines(cmd, _) => cmd,
            ArgvRoute::Read { .. } => "history-read",
        }
    }
}

/// 〔C2 · SE1 欠账〕长连接**在**、却**不认**这条帧命令 ⇒ `true`（对面的后端比这条查询老）。
///
/// 没有长连接 ⇒ `false`：那是「够不着」（瞬时），不是「对面老」—— 由 [`run_routed`] 照旧报「没有控制通道」。
/// 与 [`call`] 里那一问是**同一个** `accepts`，只是提前问，让调用方拿到种类而不是一句话。
pub(crate) fn refuses(origin: &Origin, route: &ArgvRoute) -> bool {
    inbound_client::client_for(origin.as_wire_str()).is_some_and(|c| !c.accepts(route.frame_cmd()))
}

pub(crate) fn route_argv(argv: &[&str]) -> Option<ArgvRoute> {
    match argv {
        // 〔C4d · 第四波 4B〕`--list-projects` / `--list-sessions` 两形删了：历史清单前端经通道问**本机**常驻后端
        //   （`history-projects` / `history-sessions` 带 `origin`，它沿池里那条 SSH 去问那台），monitor 这一侧不再有路发它们。
        ["--list-subagents", parent] => Some(ArgvRoute::Lines(
            "history-subagents",
            json!({"parent": parent}),
        )),
        // 〔C4c · 第四波 4B〕`--list-accounts` 那一形删了：账号清单前端经通道直接说 `accounts-list`（后端出成品）。
        // 〔C4b · 第四波 4B〕骨架索引 · 会话内查找 · 大纲清单三形删了（原是 SR1a / SE2 加的三臂）：
        //   界面经通道直接说 `history-index` / `history-find` / `history-user-inputs`，后端出成品，
        //   monitor 这一侧再没有任何一条路发它们（`frame_query_tests` 那条「迁过去的只走通道」钉着）。
        // 〔C4a · 第四波〕`--session-accounts` 那一形删了：「会话 ↔ 账号」前端经通道直接说 `accounts-sessions`，
        //   monitor 这一侧再没有任何一条路发它（`frame_query_tests` 那条「迁过去的只走通道」钉着）。
        ["--read-session", path] => Some(ArgvRoute::Read {
            path: path.to_string(),
            from: 0,
            upto: None,
        }),
        // `session_skeleton::range_argv` 那一形（选项在前）。
        ["--read-session-from-offset", "--until", end, path, off] => {
            let (Ok(end), Ok(off)) = (end.parse::<u64>(), off.parse::<u64>()) else {
                return None;
            };
            Some(ArgvRoute::Read {
                path: path.to_string(),
                from: off,
                upto: Some(end),
            })
        }
        _ => None,
    }
}

/// 按 [`route_argv`] 的结论跑那条帧查询，出逐行。
pub(crate) async fn run_routed(origin: &Origin, route: ArgvRoute) -> Result<Vec<String>, String> {
    match route {
        ArgvRoute::Lines(cmd, args) => lines(origin, cmd, args).await,
        ArgvRoute::Read { path, from, upto } => read_lines(origin, &path, from, upto).await,
    }
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/frame_query_tests.rs"]
mod tests;
