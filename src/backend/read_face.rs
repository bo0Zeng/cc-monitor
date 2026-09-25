//! 〔`C1` · 2026-09-24〕**只读查询的帧面宿主** —— `设计/15 §3.2` 层 1 那一搬。
//!
//! # 它补的是哪一格
//!
//! `设计/99 §4.19.2 ⑥` 逐字：「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那 8 条今天全是**一次性子命令**：monitor 每问一次就
//! 新开一条 TCP+SSH+鉴权，exec 一次本二进制，读完 stdout 就断。
//! 而同一台机器上**已经有一条**长连接（流模式那条），入方向早就能一问一答
//! （`inbound.rs`）—— 缺的只是这 8 条没登记上去。
//!
//! # 为什么住顶层，而不是 `observe/`
//!
//! 与 `files/` 同一个理由：`inbound.rs` 不许出现 `observe::`
//! （`inbound_structure_guards::inbound_never_reaches_into_the_observe_layer`），
//! 而查询的本体就住 `observe/`。⇒ 本文件是那道线**之外**的一层薄宿主：
//! 解帧面的 `args`、把输出装进应答、给错误一个 code。**查询本身一行不在这里** ——
//! 每条都调 CLI 那一臂**同一个函数**（只是把 `out` 从 stdout 换成内存），
//! 「一份代码两种宿主」（`设计/60 §8.1`）在这一族上的样子。
//!
//! # 应答的两种形状
//!
//! - **按行**（六条）：`{"lines": [...]}` —— 与 CLI 那条 stdout 逐行、trim 过、剔空行后一致；
//!   整份输出超过 [`LINES_CAP_BYTES`] ⇒ `too_large`（不截断：截断的清单会被当成完整的用）。
//! - **按字节分页**（`history-read`）：一份会话可以是几百 MB，一帧装不下
//!   ⇒ 调用方给 `[offset, until)`，回一页（切在行尾）＋ 续点，调用方循环到 `eof`。
//!   `history-tail` 只回那张「尾段在哪」的图，正文仍走 `history-read`。
//!
//! # 它**不**做什么
//!
//! 不拨号、不起进程、不写盘 —— 全是既有读函数的换壳。`readonly_guard` 那条写盘禁令照旧管它。

use serde_json::{json, Value};

/// `history-read` 一页的目标字节数。
///
/// 取 1 MiB：一页就是一帧应答，它在后端写出方向上与实时 `line` 帧**排同一条队** ——
/// 页越大，实时行被挡住的时间越长；页越小，往返越多。1 MiB 在 1 Mbps 的弱网上约 8 秒一页。
pub(crate) const READ_PAGE_BYTES: usize = 1 << 20;

/// 单行比一页还长时，最多续读到多长（超过 ⇒ `oversized_line`）。
///
/// monitor 那头单帧上限 64 MiB（`ssh_source::BACKEND_FRAME_LINE_CAP`），而字节进 JSON 串
/// 要转义（引号、反斜杠）—— 留一半余量。
pub(crate) const LINE_CAP_BYTES: usize = 32 << 20;

/// 按行那六条整份输出的上限（同上，留一半余量）。
pub(crate) const LINES_CAP_BYTES: usize = 32 << 20;

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

/// 帧面入口：命令名从 `r.cmd` 来（与 `files::answer_wire` 同一形 —— 「登记的名字」与
/// 「真正跑的那条」在类型上是同一个值）。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
fn answer_at(home: &std::path::Path, cmd: &str, args: &Value) -> Answer {
    use crate::observe::{accounts_query, history_query, search_query};
    match cmd {
        "history-projects" => {
            lines(|out| history_query::list_projects_into(home, out).map_err(|e| ("failed", e)))
        }
        "history-sessions" => {
            let dir = str_arg(args, "project_dir")?;
            lines(|out| {
                history_query::list_sessions_into(home, dir, out).map_err(|e| ("failed", e))
            })
        }
        "history-subagents" => {
            let parent = str_arg(args, "parent")?;
            lines(|out| history_query::list_subagents_into(home, parent, out))
        }
        "history-search" => {
            let query = str_arg(args, "query")?;
            let rest = search_rest(args)?;
            lines(|out| {
                search_query::search_into(home, query, &rest, out).map_err(|e| ("failed", e))
            })
        }
        "accounts-list" | "accounts-sessions" => {
            let which = if cmd == "accounts-list" {
                accounts_query::FrameAccounts::List
            } else {
                accounts_query::FrameAccounts::BySession
            };
            let rows = accounts_query::lines_for_frame(home, which);
            let size: usize = rows.iter().map(|l| l.len() + 1).sum();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(json!({ "lines": rows }))
        }
        "history-read" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let page = history_query::read_page(
                home,
                path,
                offset,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            Ok(json!({
                "text": String::from_utf8_lossy(&page.bytes),
                "next": page.next,
                "eof": page.eof,
            }))
        }
        // 〔SR1a · 2026-09-24〕`frame_query::STILL_DIALED` 缩到只剩真该拨号的那两条：骨架索引与大纲清单
        // 是**每开一个大会话就要一次**的查询，不是「点一次才发一次」。与 CLI 那一臂同一个函数。
        "history-index" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            lines(|out| {
                history_query::session_index_into(home, path, offset, until, out)
                    .map_err(|e| ("failed", e))
            })
        }
        "history-user-inputs" => {
            let path = str_arg(args, "path")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            lines(|out| {
                history_query::list_user_inputs_into(home, path, from, out)
                    .map_err(|e| ("failed", e))
            })
        }
        "history-tail" => {
            let path = str_arg(args, "path")?;
            let n = u64_arg(args, "n")?.ok_or(("bad_args", "缺 `n`".to_string()))?;
            let (plan, _) =
                history_query::tail_plan(home, path, n as usize).map_err(|e| ("failed", e))?;
            Ok(json!({
                "total": plan.total,
                "tail_from": plan.tail_from,
                "split_at": plan.split_at,
                "end": plan.end,
            }))
        }
        other => Err(("bad_args", format!("本族不认识 `{other}`"))),
    }
}

/// 跑一个「往 `out` 里逐行写」的查询，收成 `{"lines": [...]}`。
fn lines(f: impl FnOnce(&mut CappedBuf) -> Result<(), (&'static str, String)>) -> Answer {
    let mut out = CappedBuf::default();
    let res = f(&mut out);
    if out.over {
        return Err(too_large(out.seen));
    }
    res?;
    let text = String::from_utf8_lossy(&out.buf);
    let rows: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    Ok(json!({ "lines": rows }))
}

fn too_large(size: usize) -> (&'static str, String) {
    (
        "too_large",
        format!("结果超过 {LINES_CAP_BYTES} 字节上限，没有返回（至少 {size} 字节）"),
    )
}

/// 有上限的内存出口。超了就报错（让查询函数停下），并记下「超了」—— 调用方据此回 `too_large`，
/// 而不是把查询函数报的那句「write failed」当成真原因。
#[derive(Default)]
struct CappedBuf {
    buf: Vec<u8>,
    seen: usize,
    over: bool,
}

impl std::io::Write for CappedBuf {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.seen += data.len();
        if self.buf.len() + data.len() > LINES_CAP_BYTES {
            self.over = true;
            return Err(std::io::Error::other("reply cap reached"));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or(("bad_args", format!("缺 `{key}`（要一个字符串）")))
}

fn u64_arg(args: &Value, key: &str) -> Result<Option<u64>, (&'static str, String)> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or(("bad_args", format!("`{key}` 要一个非负整数"))),
    }
}

/// `history-search` 的选项 → `--search <query>` 之后那一截 argv。
///
/// 🔴 **不在这里另写一份选项语义**：解析走 CLI 那一臂同一个 `parse_opts`，
/// 本函数只把 JSON 摊回它认的那几个 token（`include_tools` / `scope` / `after_ms` / `limit`）。
fn search_rest(args: &Value) -> Result<Vec<String>, (&'static str, String)> {
    let mut rest = Vec::new();
    if args.get("include_tools").and_then(Value::as_bool) == Some(true) {
        rest.push("--include-tools".to_string());
    }
    if let Some(s) = args.get("scope").and_then(Value::as_str) {
        rest.push("--scope".to_string());
        rest.push(s.to_string());
    }
    if let Some(n) = u64_arg(args, "after_ms")? {
        rest.push("--after-ms".to_string());
        rest.push(n.to_string());
    }
    if let Some(n) = u64_arg(args, "limit")? {
        rest.push("--limit".to_string());
        rest.push(n.to_string());
    }
    Ok(rest)
}

#[cfg(test)]
#[path = "../../tests/backend/read_face_tests.rs"]
mod tests;
