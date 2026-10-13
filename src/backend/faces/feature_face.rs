//! 功能侧只读查询的帧面宿主 —— 任务列表（＋ cc-bus 钩子诊断）。本机远端同一个二进制，monitor 按 origin 问那一台（本机也走这里）。
//!
//! 不并进 `read_face`：monitor 侧有一条两向相等判据（`frame_query_tests::the_moved_table_matches_the_design_list_and_the_backend_registry`）
//! 数的正是「把活交给 `read_face::answer` 的帧命令」，本族不在那张表里。
//!
//! - 住顶层、不住 `observe/`：`stream/inbound/` 不许出现 `observe::`；本文件只做换壳，读的本体在 `observe/`。
//! - 应答都是成品：`tasks-list` → `{tasks: [...]}`（字段语义住 `observe/tasks_query.rs::task_entry`），界面经通道直接问、按形状收。
//!   整份超过 [`crate::faces::read_face::LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
//! - 不拨号、不起进程、不写盘。

use crate::stream::inbound::typed::{codes, of_pair, Blocking, Command, Cx};
use copy_core::copy_text;
use serde_json::{json, Value};

// ───────────────────────── tasks-list（typed）─────────────────────────

/// `tasks-list` 的入参。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TasksListReq {
    /// 会话 id（围栏在本体里：走出任务根 ⇒ `bad_args`）。
    pub(crate) sid: String,
}

/// `tasks-list` 的回包（成品）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TasksList {
    /// `<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序。
    pub(crate) tasks: Vec<crate::observe::tasks_query::Task>,
}

codes! {
    /// `tasks-list` 的码。
    pub(crate) enum TasksListErr {
        /// 任务目录读不动。
        Failed = "failed",
        /// 整份过 [`crate::faces::read_face::LINES_CAP_BYTES`]（不截断）。
        TooLarge = "too_large",
    }
}

/// `tasks-list`：一个会话的任务列表。
pub(crate) struct TasksListCmd;

impl Command for TasksListCmd {
    const NAME: &'static str = "tasks-list";
    type Req = TasksListReq;
    type Reply = TasksList;
    type Err = TasksListErr;
}

impl Blocking for TasksListCmd {
    fn run(req: TasksListReq, _cx: &Cx) -> Result<TasksList, TasksListErr> {
        tasks_at(&crate::observe::history_query::agent_home(), req)
    }
}

/// [`TasksListCmd`] 的本体，家目录是参数。
pub(crate) fn tasks_at(
    home: &std::path::Path,
    req: TasksListReq,
) -> Result<TasksList, TasksListErr> {
    let fail = of_pair(TasksListErr::Failed);
    let tasks = crate::observe::tasks_query::session_tasks(home, &req.sid).map_err(&fail)?;
    let reply = TasksList { tasks };
    let size = serde_json::to_vec(&reply).map_or(0, |b| b.len());
    if size > crate::faces::read_face::LINES_CAP_BYTES {
        return Err(TasksListErr::TooLarge(too_large(size)));
    }
    Ok(reply)
}

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

/// 帧面入口：命令名从 `r.cmd` 来（与 `read_face::answer` 同一形）。
pub(crate) fn answer(cmd: &str, args: &Value, tz: &crate::Tz) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args, tz)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：成品里的时刻字按它写。
fn answer_at(home: &std::path::Path, cmd: &str, args: &Value, tz: &crate::Tz) -> Answer {
    match cmd {
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
            let Some(kind) = crate::agents::sole_kind(|a| a.mcp.is_some()) else {
                return capped(mcp_reply(&Default::default(), &McpSay::default()));
            };
            let look = mcp_look(kind);
            let read = crate::agents::mcp_read(kind, dir.as_deref(), &look).unwrap_or_default();
            // 断开：这台此刻活着的会话里那一家说的（各号的家都看）；每个名字取说它最晚的那一条。
            let homes: Vec<std::path::PathBuf> =
                look.homes.iter().map(|(_, h)| h.clone()).collect();
            let failed = crate::observe::accounts_query::live_mcp_failed(&homes);
            let now_s = i64::try_from(look.now_ms / 1000).unwrap_or(i64::MAX);
            let say = McpSay {
                failed: &failed,
                now_s,
                tz: tz.clone(),
                login_command: crate::agents::mcp_login_command(kind).unwrap_or(""),
            };
            capped(mcp_reply(&read, &say))
        }
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 判 MCP 状态要看的那几个家：那一家没设账号时的家 ＋ 账号库里各号的家（同一个目录只算一次，带名字的那一条留下）。
/// 账号库读不出 ⇒ 只看没设账号的那一份（状态少说，不说错）。
pub(crate) fn mcp_look(kind: &str) -> crate::agents::McpLook {
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
    let now_ms = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    crate::agents::McpLook { homes, now_ms }
}

/// 整份过同一个上限（不截断）。
fn capped(v: Value) -> Answer {
    let size = v.to_string().len();
    if size > crate::faces::read_face::LINES_CAP_BYTES {
        return Err(("too_large", too_large(size)));
    }
    Ok(v)
}

/// 整份过上限的那一句（`size` 字节）。
fn too_large(size: usize) -> String {
    copy_text(
        "beFeatureFace.answerAt.tooLarge",
        &[
            ("size", &size.to_string()),
            (
                "cap",
                &(crate::faces::read_face::LINES_CAP_BYTES).to_string(),
            ),
        ],
    )
}

/// [`mcp_reply`] 写字要的：这台活会话里说断开的那几个 · 此刻（unix 秒）与看的那一台的时区 · 登录那一步复制的那条命令（那一家的）。
#[derive(Default)]
pub(crate) struct McpSay<'a> {
    pub failed: &'a [crate::observe::accounts_query::LiveMcpFailed],
    pub now_s: i64,
    pub tz: crate::Tz,
    pub login_command: &'static str,
}

/// `mcp-read` 的成品 —— 纯构造器（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 拿它对拍）。
/// 每条带写好的字（界面照抄）：`login`（需登录那一句 · 悬停那一截 · 复制的命令）· `failed`（断开那一句 · 悬停那一截 · 原话）·
/// `mark`（小标画哪一种：断开压过需登录；停用的不连 ⇒ 不标断开）。
pub(crate) fn mcp_reply(r: &crate::agents::McpRead, say: &McpSay) -> Value {
    use crate::agents::McpStatus;
    let time = |ms: i64| crate::common::time::fmt_at(ms.div_euclid(1000), say.now_s, &say.tz);
    let entries: Vec<Value> = r
        .entries
        .iter()
        .map(|e| {
            let login = (e.status == McpStatus::NeedsLogin).then(|| {
                let who = e.login_in.join(&copy_text("beMcp.mark.sep", &[]));
                let at = e.seen_ms.map_or(String::new(), |t| time(i64::try_from(t).unwrap_or(i64::MAX)));
                let (said, tip) = if who.is_empty() {
                    (copy_text("beMcp.mark.loginBare", &[("time", &at)]), copy_text("beMcp.mark.loginTipBare", &[]))
                } else {
                    (
                        copy_text("beMcp.mark.login", &[("accounts", &who), ("time", &at)]),
                        copy_text("beMcp.mark.loginTip", &[("accounts", &who)]),
                    )
                };
                json!({ "said": said, "tip": tip, "copy": say.login_command })
            });
            let failed = (e.status != McpStatus::Disabled)
                .then(|| say.failed.iter().find(|f| f.name == e.name))
                .flatten()
                .map(|f| {
                    let said = match f.at_ms {
                        Some(t) => copy_text("beMcp.mark.failed", &[("title", &f.title), ("time", &time(t))]),
                        None => copy_text("beMcp.mark.failedUntimed", &[("title", &f.title)]),
                    };
                    json!({ "said": said, "tip": copy_text("beMcp.mark.failedTip", &[]), "detail": f.detail })
                });
            let mark = if failed.is_some() {
                json!(McpStatus::Failed)
            } else if login.is_some() {
                json!(McpStatus::NeedsLogin)
            } else {
                Value::Null
            };
            json!({
                "scope": e.scope, "name": e.name, "server": e.server, "sourcePath": e.source,
                "status": e.status, "loginIn": e.login_in, "seenAt": e.seen_ms,
                "mark": mark, "login": login, "failed": failed,
            })
        })
        .collect();
    json!({ "entries": entries, "dirs": r.dirs, "problems": r.problems })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/feature_face_tests.rs"]
mod tests;

/// `session-interrupts`：动一个会话之前，会打断什么（按族答，界面只画）。阻塞档：读 pidfile 目录 ＋ 任务目录。
pub(crate) struct SessionInterrupts;

crate::stream::inbound::typed::codes! {
    /// `session-interrupts` 自己没有别的码（读不到按「没有」答；sid 空 ⇒ `bad_args`）。
    pub(crate) enum SessionInterruptsErr {}
}

/// `session-interrupts` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionInterruptsReq {
    /// 会话 id（空 ⇒ `bad_args`）。
    pub(crate) sid: String,
}

impl crate::stream::inbound::typed::Command for SessionInterrupts {
    const NAME: &'static str = "session-interrupts";
    type Req = SessionInterruptsReq;
    type Reply = crate::observe::interrupts_query::SessionInterrupts;
    type Err = SessionInterruptsErr;
}

impl crate::stream::inbound::typed::Blocking for SessionInterrupts {
    fn run(
        req: SessionInterruptsReq,
        _cx: &crate::stream::inbound::typed::Cx,
    ) -> Result<crate::observe::interrupts_query::SessionInterrupts, SessionInterruptsErr> {
        crate::observe::interrupts_query::answer_at(
            &crate::observe::history_query::agent_home(),
            &req.sid,
        )
        .map_err(SessionInterruptsErr::BadArgs)
    }
}

/// `session-terminals`：此刻是哪个终端在显示这个会话（↗ 点那一刻问一次）。阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。
pub(crate) struct SessionTerminals;

crate::stream::inbound::typed::codes! {
    /// `session-terminals` 自己的码。
    pub(crate) enum SessionTerminalsErr {
        /// 这台没有在跑的这个会话。
        NoSuchSession = "no_such_session",
        /// 问 tmux 那一发没成（起不来 / 报错）。
        Failed = "failed",
        /// 问 tmux 那一发过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

impl crate::stream::inbound::typed::Command for SessionTerminals {
    const NAME: &'static str = "session-terminals";
    type Req = crate::observe::session_terminals::SidReq;
    type Reply = crate::observe::session_terminals::Showing;
    type Err = SessionTerminalsErr;
}

impl crate::stream::inbound::typed::Blocking for SessionTerminals {
    fn run(
        req: crate::observe::session_terminals::SidReq,
        _cx: &crate::stream::inbound::typed::Cx,
    ) -> Result<crate::observe::session_terminals::Showing, SessionTerminalsErr> {
        crate::observe::session_terminals::answer_at(
            &crate::observe::history_query::agent_home(),
            &req,
        )
        .map_err(|(code, said)| match code {
            "bad_args" => SessionTerminalsErr::BadArgs(said),
            "no_such_session" => SessionTerminalsErr::NoSuchSession(said),
            crate::platform::child::TIMED_OUT => SessionTerminalsErr::ChildTimedOut(said),
            _ => SessionTerminalsErr::Failed(said),
        })
    }
}
