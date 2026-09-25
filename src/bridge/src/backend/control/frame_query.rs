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
//! # 还在逐次拨号的那几条（[`STILL_DIALED`]，逐条带理由）
//!
//! 不在题面那八条里的一次性查询照旧走 `remote_history::run_list_query`，
//! 而那条路**只放行本表里的子命令** —— 八条里任何一条从那里漏出去都会被当场拒掉
//! （[`dial_allowed`]，判据在 `tests/bridge/backend/control/frame_query_tests.rs`）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 题面那八条：CLI 子命令 → 帧命令。**判据的一侧**（另一侧从后端源码数，见测试）。
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
];

/// 仍然逐次拨号的一次性查询 —— `(子命令, 为什么今天还拨)`。**只有它们**过得了拨号那条路。
pub(crate) const STILL_DIALED: &[(&str, &str)] = &[
    (
        "--account-trust",
        "换号前的信任预检：不在题面八条里；用户点一次换号才发一次，不在任何轮询上",
    ),
    ("--account-trust-zero", "同上一行（账号 0 那一形）"),
    (
        "--read-session-from-offset",
        "只剩 `--index` 那一形（会话骨架索引）：不在题面八条里、帧面没有对应命令。\
         同一个子命令的 `--until` 那一形（按区间取正文）已经走 `history-read`，不在这里拨",
    ),
    (
        "--list-user-inputs",
        "〔C2 · 补第二波的集成缝〕会话大纲（SE1）：与本表同波落地，既不在题面八条里、帧面也没有对应命令 \
         ⇒ 此前在这里被当场拒，远端大纲**每次**都拿到「这是本程序的 bug」。上帧面要加一条帧命令 \
         （= 后端子命令面 ＋1，要 bump），另拍；今天先让它照旧拨",
    ),
    (
        "--find-in-session",
        "〔SE2〕会话内查找（Ctrl+F）：与上一行同一个处境 —— 新子命令、帧面没有对应命令；\
         用户按一次 Enter 才发一次，不在任何轮询上。上帧面另拍（同上一行）",
    ),
];

/// 拨号那条路放不放行这条子命令（`args` 的第一个 token）。
pub(crate) fn dial_allowed(subcommand: &str) -> bool {
    STILL_DIALED.iter().any(|(f, _)| *f == subcommand)
}

/// 按行那几条的期限：与旧 `LIST_TIMEOUT` 同值（30s）。它从此只盖「远端跑查询 ＋ 回程」，不再盖握手。
const LINES_BUDGET: Duration = Duration::from_secs(30);
/// 一页 `history-read` / 一次 `history-tail` 的期限：与旧逐行读的单次超时同值（60s）。
const PAGE_BUDGET: Duration = Duration::from_secs(60);

/// 发一条帧命令，拿 `data`。
async fn call(origin: &Origin, cmd: &str, args: Value, budget: Duration) -> Result<Value, String> {
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

/// 按行那六条：`data.lines` 原样拿回（逐行、trim 过、剔空行 —— 与旧 `run_list_query` 同形）。
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

/// 读整段区间，收成逐行（trim 过、剔空行）—— 与旧 `run_list_query` 读 `--read-session` 的出参同形。
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
/// 认不出 ⇒ `None`，调用方落到拨号那条路 —— 而那条路只放行 [`STILL_DIALED`]。
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
        ["--list-projects"] => Some(ArgvRoute::Lines("history-projects", json!({}))),
        ["--list-sessions", dir] => Some(ArgvRoute::Lines(
            "history-sessions",
            json!({"project_dir": dir}),
        )),
        ["--list-subagents", parent] => Some(ArgvRoute::Lines(
            "history-subagents",
            json!({"parent": parent}),
        )),
        ["--list-accounts"] => Some(ArgvRoute::Lines("accounts-list", json!({}))),
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
