//! 功能侧只读查询的帧面宿主 —— 任务列表（＋ cc-bus 钩子诊断）。本机远端同一个二进制，monitor 按 origin 问那一台（本机也走这里）。
//!
//! 不并进 `read_face`：monitor 侧有一条两向相等判据（`frame_query_tests::the_moved_table_matches_the_design_list_and_the_backend_registry`）
//! 数的正是「把活交给 `read_face::answer` 的帧命令」，本族不在那张表里。
//!
//! - 住顶层、不住 `observe/`：`stream/inbound/` 不许出现 `observe::`；本文件只做换壳，读的本体在 `observe/`。
//! - 应答都是成品：`tasks-list` → `{tasks: [...]}`（字段语义住 `observe/tasks_query.rs::task_entry`），界面经通道直接问、按形状收。
//!   整份超过 [`crate::faces::read_face::LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
//! - 不拨号、不起进程、不写盘。

use crate::wire::typed::{codes, of_pair, Command};
use copy_core::copy_text;
use serde_json::Value;

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

impl crate::wire::typed::Handle for TasksListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TasksListReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<TasksList, TasksListErr>> + Send + 'static {
        async move { tasks_at(&crate::history::listing::agent_home(), req) }
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

// ───────────────────────── hooks-diag · mcp-read（typed）─────────────────────────
// 与 `session-terminals` 同住这一份（引用图按文件算，`target_parity` 那张名单按这一形写了理由）；本身不碰 tmux。

codes! {
    /// `hooks-diag` 的码（只有整份过上限那一种；读不到 / 坏 JSON 是回包里的 `note`）。
    pub(crate) enum HooksDiagErr {
        /// 整份超过上限（不截断）。
        TooLarge = "too_large",
    }
}

/// `hooks-diag`：cc-bus 钩子诊断（这台自己的 `settings.json` ＋ stat ⇒ 诊断 ＋ 两种待贴片段）。
pub(crate) struct HooksDiagCmd;

impl Command for HooksDiagCmd {
    const NAME: &'static str = "hooks-diag";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = crate::observe::cc_bus_hooks::HooksReport;
    type Err = HooksDiagErr;
}

impl crate::wire::typed::Handle for HooksDiagCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::observe::cc_bus_hooks::HooksReport, HooksDiagErr>,
    > + Send
           + 'static {
        async move { capped(crate::observe::cc_bus_hooks::answer()).map_err(HooksDiagErr::TooLarge) }
    }
}

/// `mcp-read` 的入参。
#[derive(Debug, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct McpReadReq {
    /// 项目目录（绝对路径；缺 · `null` ⇒ 不看项目那一段）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) project_dir: Option<String>,
}

codes! {
    /// `mcp-read` 的码。
    pub(crate) enum McpReadErr {
        /// 整份超过上限（不截断）。
        TooLarge = "too_large",
    }
}

/// `mcp-read`：这台机器的 MCP 列表成品。
pub(crate) struct McpReadCmd;

impl Command for McpReadCmd {
    const NAME: &'static str = "mcp-read";
    type Req = McpReadReq;
    type Reply = McpReadReply;
    type Err = McpReadErr;
}

impl crate::wire::typed::Handle for McpReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: McpReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<McpReadReply, McpReadErr>> + Send + 'static {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            mcp_read_here(&req, &cx.tz)
        }
    }
}

/// [`McpReadCmd`] 的本体：读法住适配层那一格（`agents::mcp_read`，唯一声明了 MCP 读面的那一家），这里只换壳。
/// `tz` ＝ 看的那一台的时区：成品里的时刻字按它写。
fn mcp_read_here(req: &McpReadReq, tz: &crate::Tz) -> Result<McpReadReply, McpReadErr> {
    let dir = match req.project_dir.as_deref() {
        None => None,
        Some(d) => {
            // `INVARIANTS §47` ②：项目目录是外来的路径 —— 绝对 · 不空 · 拒 NUL / CR / LF。
            if d.is_empty()
                || !std::path::Path::new(d).is_absolute()
                || !shell_quote_core::free_text_ok(d)
            {
                return Err(McpReadErr::BadArgs(crate::common::contract::malformed(
                    "`projectDir` must be an absolute path",
                )));
            }
            Some(std::path::PathBuf::from(d))
        }
    };
    // 读 MCP 的那一家：唯一声明了 MCP 读面的那一家（今天只有 Claude）；Codex 的 MCP 来了由请求说是哪一家。
    let Some(kind) = crate::agents::sole_kind(|a| a.mcp.is_some()) else {
        return capped(mcp_reply(&Default::default(), &McpSay::default()))
            .map_err(McpReadErr::TooLarge);
    };
    let look = crate::accounts::manage::mcp_share_exec::mcp_look(kind);
    let read = crate::agents::mcp_read(kind, dir.as_deref(), &look).unwrap_or_default();
    // 断开：这台此刻活着的会话里那一家说的（各号的家都看）；每个名字取说它最晚的那一条。
    let homes: Vec<std::path::PathBuf> = look.homes.iter().map(|(_, h)| h.clone()).collect();
    let failed = crate::observe::accounts_query::live_mcp_failed(&homes);
    let now_s = i64::try_from(look.now_ms / 1000).unwrap_or(i64::MAX);
    let say = McpSay {
        failed: &failed,
        now_s,
        tz: tz.clone(),
        login_command: crate::agents::mcp_login_command(kind).unwrap_or(""),
    };
    capped(mcp_reply(&read, &say)).map_err(McpReadErr::TooLarge)
}

/// 整份过同一个上限（不截断）：过了 ⇒ 那一句。
fn capped<T: serde::Serialize>(v: T) -> Result<T, String> {
    let size = serde_json::to_vec(&v).map_or(0, |b| b.len());
    if size > crate::faces::read_face::LINES_CAP_BYTES {
        return Err(too_large(size));
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

/// `mcp-read` 的回包。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpReadReply {
    pub entries: Vec<McpReadEntry>,
    /// agent 用户级配置里那张项目表的键（排序）：用过的项目目录。
    pub dirs: Vec<String>,
    /// 在而读不出 / 不是 JSON 的那几份各一句；不在的静默。
    pub problems: Vec<String>,
}

/// `mcp-read` 的一条（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 钉）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpReadEntry {
    /// 闭集 `user` · `local` · `project`（适配层那一格写的）。
    #[cfg_attr(test, ts(type = "\"user\" | \"local\" | \"project\""))]
    pub scope: &'static str,
    pub name: String,
    /// 那一条配置原样（未知字段不丢）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub server: Value,
    /// 读自哪一份文件。
    pub source_path: String,
    /// 配置层只说三种：`needsLogin` · `disabled` · `unknown`（不说连上了）。
    #[cfg_attr(test, ts(type = "\"needsLogin\" | \"disabled\" | \"unknown\""))]
    pub status: crate::agents::McpStatus,
    /// `needsLogin` 时：在哪几个号里要登录；别的状态恒空。
    pub login_in: Vec<String>,
    /// `needsLogin` 时：最近一次看到是何时（epoch ms）；别的状态 `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub seen_at: Option<u64>,
    /// 小标画哪一种：断开压过需登录；都没有 ⇒ `null`（只说装了，不说连上了）。
    pub mark: Option<McpMark>,
    pub login: Option<McpLoginMark>,
    pub failed: Option<McpFailedMark>,
}

/// 小标那一种（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum McpMark {
    NeedsLogin,
    Failed,
}

/// 需登录时写好的字：抽屉那一句 · 悬停那一截 · 在那一家会话里登录要敲的那条命令。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpLoginMark {
    pub said: String,
    pub tip: String,
    pub copy: String,
}

/// 断开时写好的字：抽屉那一句 · 悬停那一截 · 那一家的原话（没写 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpFailedMark {
    pub said: String,
    pub tip: String,
    pub detail: Option<String>,
}

/// `mcp-read` 的成品 —— 纯构造器（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 拿它对拍）。
/// 每条带写好的字（界面照抄）：`login`（需登录那一句 · 悬停那一截 · 复制的命令）· `failed`（断开那一句 · 悬停那一截 · 原话）·
/// `mark`（小标画哪一种：断开压过需登录；停用的不连 ⇒ 不标断开）。
pub(crate) fn mcp_reply(r: &crate::agents::McpRead, say: &McpSay) -> McpReadReply {
    use crate::agents::McpStatus;
    let time = |ms: i64| crate::common::time::fmt_at(ms.div_euclid(1000), say.now_s, &say.tz);
    let entries = r
        .entries
        .iter()
        .map(|e| {
            let login = (e.status == McpStatus::NeedsLogin).then(|| {
                let who = e.login_in.join(&copy_text("beMcp.mark.sep", &[]));
                let at = e.seen_ms.map_or(String::new(), |t| {
                    time(i64::try_from(t).unwrap_or(i64::MAX))
                });
                let (said, tip) = if who.is_empty() {
                    (
                        copy_text("beMcp.mark.loginBare", &[("time", &at)]),
                        copy_text("beMcp.mark.loginTipBare", &[]),
                    )
                } else {
                    (
                        copy_text("beMcp.mark.login", &[("accounts", &who), ("time", &at)]),
                        copy_text("beMcp.mark.loginTip", &[("accounts", &who)]),
                    )
                };
                McpLoginMark {
                    said,
                    tip,
                    copy: say.login_command.to_string(),
                }
            });
            let failed = (e.status != McpStatus::Disabled)
                .then(|| say.failed.iter().find(|f| f.name == e.name))
                .flatten()
                .map(|f| {
                    let said = match f.at_ms {
                        Some(t) => copy_text(
                            "beMcp.mark.failed",
                            &[("title", &f.title), ("time", &time(t))],
                        ),
                        None => copy_text("beMcp.mark.failedUntimed", &[("title", &f.title)]),
                    };
                    McpFailedMark {
                        said,
                        tip: copy_text("beMcp.mark.failedTip", &[]),
                        detail: f.detail.clone(),
                    }
                });
            let mark = if failed.is_some() {
                Some(McpMark::Failed)
            } else if login.is_some() {
                Some(McpMark::NeedsLogin)
            } else {
                None
            };
            McpReadEntry {
                scope: e.scope,
                name: e.name.clone(),
                server: e.server.clone(),
                source_path: e.source.clone(),
                status: e.status,
                login_in: e.login_in.clone(),
                seen_at: e.seen_ms,
                mark,
                login,
                failed,
            }
        })
        .collect();
    McpReadReply {
        entries,
        dirs: r.dirs.clone(),
        problems: r.problems.clone(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/feature_face_tests.rs"]
mod tests;

/// `session-terminals`：此刻是哪个终端在显示这个会话（↗ 点那一刻问一次）。阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。
pub(crate) struct SessionTerminals;

crate::wire::typed::codes! {
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

impl crate::wire::typed::Command for SessionTerminals {
    const NAME: &'static str = "session-terminals";
    type Req = crate::observe::session_terminals::SidReq;
    type Reply = crate::observe::session_terminals::Showing;
    type Err = SessionTerminalsErr;
}

impl crate::wire::typed::Handle for SessionTerminals {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: crate::observe::session_terminals::SidReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::observe::session_terminals::Showing, SessionTerminalsErr>,
    > + Send
           + 'static {
        async move {
            crate::observe::session_terminals::answer_at(
                &crate::history::listing::agent_home(),
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
}
