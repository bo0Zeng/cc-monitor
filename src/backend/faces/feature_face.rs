//! 功能侧只读查询的帧面宿主 —— 任务列表（＋ cc-bus 钩子诊断）。本机远端同一个二进制，monitor 按 origin 问那一台（本机也走这里）。
//!
//! 不并进 `read_face`：monitor 侧有一条两向相等判据（`frame_query_tests::the_moved_table_matches_the_design_list_and_the_backend_registry`）
//! 数的正是「把活交给 `read_face::answer` 的帧命令」，本族不在那张表里。
//!
//! - 住顶层、不住 `observe/`：`stream/inbound/` 不许出现 `observe::`；本文件只做换壳，读的本体在 `observe/`。
//! - 应答都是成品：`tasks-list` → `{tasks: [...]}`（字段语义住 `observe/tasks_query.rs::task_entry`），界面经通道直接问、按形状收。
//!   整份超过 [`crate::faces::read_face::LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
//! - 不拨号、不起进程、不写盘。

use copy_core::copy_text;
use serde_json::{json, Value};

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

/// 帧面入口：命令名从 `r.cmd` 来（与 `read_face::answer` 同一形）。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
fn answer_at(home: &std::path::Path, cmd: &str, args: &Value) -> Answer {
    match cmd {
        "tasks-list" => {
            let sid = args.get("sid").and_then(Value::as_str).ok_or((
                "bad_args",
                crate::common::contract::malformed("missing `sid` (a string)"),
            ))?;
            // 应答是成品 `{tasks: [...]}`。
            capped(json!({ "tasks": crate::observe::tasks_query::session_tasks(home, sid)? }))
        }
        // 动一个会话之前会打断什么（按族的成品）。
        "session-interrupts" => {
            crate::observe::interrupts_query::answer_at(home, args).map_err(|(c, m)| {
                (
                    if c == "bad_args" {
                        "bad_args"
                    } else {
                        "failed"
                    },
                    m,
                )
            })
        }
        // 此刻是哪个终端在显示这个会话（↗ 点那一刻问一次）：那台读那个进程 / 连着它的 tmux 客户端的环境。
        "session-terminals" => crate::observe::session_terminals::answer_at(home, args),
        // cc-bus 钩子诊断出成品：这台自己的 `settings.json` ＋ stat（本机远端同一条，monitor 那两条 Tauri 命令删了）。
        "hooks-diag" => {
            let v = serde_json::to_value(crate::observe::cc_bus_hooks::answer()).map_err(|e| {
                (
                    "failed",
                    crate::common::contract::malformed(&format!(
                        "serializing the report failed: {e}"
                    )),
                )
            })?;
            capped(v)
        }
        // MCP 列表出成品：读法住适配层那一格（`agents::mcp_read`，唯一声明了 MCP 读面的那一家），这里只换壳。
        "mcp-read" => {
            let dir = match args.get("projectDir") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let d = v.as_str().unwrap_or("");
                    // `INVARIANTS §47` ②：项目目录是外来的路径 —— 绝对 · 不空 · 拒 NUL / CR / LF。
                    if d.is_empty()
                        || !std::path::Path::new(d).is_absolute()
                        || !shell_quote_core::free_text_ok(d)
                    {
                        return Err((
                            "bad_args",
                            crate::common::contract::malformed(
                                "`projectDir` must be an absolute path",
                            ),
                        ));
                    }
                    Some(std::path::PathBuf::from(d))
                }
            };
            // 读 MCP 的那一家：唯一声明了 MCP 读面的那一家（今天只有 Claude）；Codex 的 MCP 来了由请求说是哪一家。
            let read = crate::agents::sole_kind(|a| a.mcp.is_some())
                .and_then(|k| crate::agents::mcp_read(k, dir.as_deref(), &mcp_look(k)))
                .unwrap_or_default();
            capped(mcp_reply(&read))
        }
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 判 MCP 状态要看的那几个家：那一家没设账号时的家 ＋ 账号库里各号的家（同一个目录只算一次，带名字的那一条留下）。
/// 账号库读不出 ⇒ 只看没设账号的那一份（状态少说，不说错）。
fn mcp_look(kind: &str) -> crate::agents::McpLook {
    let mut homes: Vec<(Option<String>, std::path::PathBuf)> = crate::platform::paths::home_dir()
        .and_then(|h| {
            crate::accounts::manage::mcp_share_exec::accounts_in(&h.display().to_string())
                .ok()
                .flatten()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|(name, dir)| (Some(name), std::path::PathBuf::from(dir)))
        .collect();
    if let Some(bare) = crate::agents::home_of_kind(kind) {
        if !homes.iter().any(|(_, d)| *d == bare) {
            homes.push((None, bare));
        }
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    crate::agents::McpLook { homes, now_ms }
}

/// 整份过同一个上限（不截断）。
fn capped(v: Value) -> Answer {
    let size = v.to_string().len();
    if size > crate::faces::read_face::LINES_CAP_BYTES {
        return Err((
            "too_large",
            copy_text(
                "beFeatureFace.answerAt.tooLarge",
                &[
                    ("size", &size.to_string()),
                    (
                        "cap",
                        &(crate::faces::read_face::LINES_CAP_BYTES).to_string(),
                    ),
                ],
            ),
        ));
    }
    Ok(v)
}

/// `mcp-read` 的成品 —— 纯构造器（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 拿它对拍）。
pub(crate) fn mcp_reply(r: &crate::agents::McpRead) -> Value {
    let entries: Vec<Value> = r
        .entries
        .iter()
        .map(|e| {
            json!({
                "scope": e.scope, "name": e.name, "server": e.server, "sourcePath": e.source,
                "status": e.status.wire(), "loginIn": e.login_in, "seenAt": e.seen_ms,
            })
        })
        .collect();
    json!({ "entries": entries, "dirs": r.dirs, "problems": r.problems })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/feature_face_tests.rs"]
mod tests;
