//! 功能侧只读查询的帧面宿主 —— 任务列表（＋ cc-bus 钩子诊断）。本机远端同一个二进制，monitor 按 origin 问那一台（本机也走这里）。
//!
//! 不并进 `read_face`：monitor 侧有一条两向相等判据（`frame_query_tests::the_moved_table_is_what_the_backend_registers`）
//! 数的正是「把活交给 `read_face::answer` 的帧命令」，本族不在那张表里。
//!
//! - 住顶层、不住 `observe/`：`stream/inbound/` 不许出现 `observe::`；本文件只做换壳，读的本体在 `observe/`。
//! - 应答都是成品：`tasks-list` → `{tasks: [...]}`（任务文件读法在 `agents/claudecode/tasks.rs`，成品句在 `observe/tasks_query.rs`），界面经通道直接问、按形状收。
//!   整份超过 [`crate::faces::read_face::LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
//! - 不拨号、不起进程、不写盘。

use crate::wire::typed::{codes, of_fail_with, Command};
use copy_core::copy_text;
use serde_json::Value;

// ───────────────────────── tasks-list（typed）─────────────────────────

/// `tasks-list` 的入参。
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct TasksListReq {
    /// 会话 id（围栏在本体里：走出任务根 ⇒ `bad_args`）。
    pub sid: String,
}

/// `tasks-list` 的回包（成品；线上形住 cmd-wire，`changed {tasks}` 带的也是这一份）。
pub(crate) use cmd_wire::tasks::TasksList;

codes! {
    /// `tasks-list` 的码。
    pub enum TasksListErr {
        /// 任务目录读不动。
        Failed = "failed",
        /// 整份过 [`crate::faces::read_face::LINES_CAP_BYTES`]（不截断）。
        TooLarge = "too_large",
    }
}

/// `tasks-list`：一个会话的任务列表。
pub struct TasksListCmd;

impl Command for TasksListCmd {
    const NAME: &'static str = "tasks-list";
    type Req = TasksListReq;
    type Reply = TasksList;
    type Err = TasksListErr;
}

impl crate::wire::typed::Handle for TasksListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: TasksListReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<TasksList, TasksListErr> {
        tasks_at(&crate::agents::local_homes(), req)
    }
}

/// [`TasksListCmd`] 的本体，家目录是参数。
pub(crate) fn tasks_at(
    home: impl crate::agents::IntoHomes,
    req: TasksListReq,
) -> Result<TasksList, TasksListErr> {
    let fail = of_fail_with(TasksListErr::Failed);
    let tasks = crate::observe::tasks_query::session_tasks(home, &req.sid).map_err(&fail)?;
    capped(crate::observe::tasks_query::tasks_list(tasks)).map_err(TasksListErr::TooLarge)
}

// ───────────────────────── hooks-diag · mcp-read（typed）─────────────────────────
// 与 `session-terminals` 同住这一份（引用图按文件算，`target_parity` 那张名单按这一形写了理由）；本身不碰 tmux。

codes! {
    /// `hooks-diag` 的码（只有整份过上限那一种；读不到 / 坏 JSON 是回包里的 `note`）。
    pub enum HooksDiagErr {
        /// 整份超过上限（不截断）。
        TooLarge = "too_large",
    }
}

/// `hooks-diag`：cc-bus 钩子诊断（这台自己的 `settings.json` ＋ stat ⇒ 诊断 ＋ 两种待贴片段）。
pub struct HooksDiagCmd;

impl Command for HooksDiagCmd {
    const NAME: &'static str = "hooks-diag";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = crate::assets::cc_bus_hooks::HooksReport;
    type Err = HooksDiagErr;
}

impl crate::wire::typed::Handle for HooksDiagCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<crate::assets::cc_bus_hooks::HooksReport, HooksDiagErr> {
        capped(crate::assets::cc_bus_hooks::answer()).map_err(HooksDiagErr::TooLarge)
    }
}

/// `mcp-read` 的入参。
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct McpReadReq {
    /// 项目目录（绝对路径；缺 · `null` ⇒ 不看项目那一段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) project_dir: Option<String>,
}

codes! {
    /// `mcp-read` 的码。
    pub enum McpReadErr {
        /// 整份超过上限（不截断）。
        TooLarge = "too_large",
    }
}

/// `mcp-read`：这台机器的 MCP 列表成品。
pub struct McpReadCmd;

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

/// [`McpReadCmd`] 的本体：读法住适配层那一格（`agents::mcp_read_among`，逐家：`agents::mcp_kinds_among`），这里只换壳。
/// `tz` ＝ 看的那一台的时区：成品里的时刻字按它写。`ext-list` 带本机那一格 `mcp` 也读它（`ext_list_face::with_mcp`）。
pub(crate) fn mcp_read_here(
    req: &McpReadReq,
    tz: &crate::common::time::Tz,
) -> Result<McpReadReply, McpReadErr> {
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
    mcp_read_with(
        crate::agents::REGISTRY,
        dir.as_deref(),
        req,
        tz,
        crate::accounts::manage::mcp_share_exec::mcp_look,
        crate::observe::accounts_query::live_mcp_failed,
    )
}

/// [`mcp_read_here`] 过了入参那一截之后：注册表 · 各号的家怎么收 · 活会话里说断开的怎么收都是参数（判据拿替身家喂它）。
pub(crate) fn mcp_read_with(
    registry: &[crate::agents::Adapter],
    dir: Option<&std::path::Path>,
    req: &McpReadReq,
    tz: &crate::common::time::Tz,
    look_of: impl Fn(&str) -> crate::agents::McpLook,
    failed_of: impl Fn(
        &[crate::agents::AgentHome],
    ) -> Vec<crate::observe::accounts_query::LiveMcpFailed>,
) -> Result<McpReadReply, McpReadErr> {
    // 整台机器的面：声明了 MCP 面的每一家各读一份、合成一份（[`mcp_merge`]）；
    // 每条带上家，默认那一家不写（判法只住 `agents::unless_default`（经 `agents::family_tag`），扩展页那一行 `ExtRow.agent` 同一处判）；界面照名字与行上的家一起挑同一家那一条。
    let bare = crate::agents::asset_default_among(registry);
    let one = |kind: &'static str| {
        let look = look_of(kind);
        let read = crate::agents::mcp_read_among(registry, kind, dir, &look).unwrap_or_default();
        // 断开：这台此刻活着的会话里那一家说的（那一家各号的家都看）；每个名字取说它最晚的那一条。
        let homes: Vec<crate::agents::AgentHome> = look
            .homes
            .iter()
            .map(|(_, home)| crate::agents::AgentHome {
                kind,
                home: home.clone(),
            })
            .collect();
        let failed = failed_of(&homes);
        let now_s = i64::try_from(look.now_ms / 1000).unwrap_or(i64::MAX);
        let say = McpSay {
            registry,
            failed: &failed,
            now_s,
            tz: tz.clone(),
            kind,
            tagged: crate::agents::family_tag(kind, bare).is_some(),
            project: req.project_dir.as_deref(),
            homes: &look.homes,
        };
        mcp_reply(&read, &say)
    };
    mcp_merge(
        crate::agents::mcp_kinds_among(registry)
            .into_iter()
            .map(one),
        crate::faces::read_face::LINES_CAP_BYTES.as_len(),
    )
}

/// 逐家那几份合成一份：条目照注册序接上；上限（`cap` 字节，生产是 [`crate::faces::read_face::LINES_CAP_BYTES`]）量合成后的整份。
fn mcp_merge(
    parts: impl IntoIterator<Item = McpReadReply>,
    cap: usize,
) -> Result<McpReadReply, McpReadErr> {
    let out = McpReadReply {
        entries: parts.into_iter().flat_map(|one| one.entries).collect(),
    };
    capped_at(out, cap).map_err(McpReadErr::TooLarge)
}

/// 整份过 [`crate::faces::read_face::LINES_CAP_BYTES`]（不截断）：过了 ⇒ 那一句。
fn capped<T: serde::Serialize>(v: T) -> Result<T, String> {
    capped_at(v, crate::faces::read_face::LINES_CAP_BYTES.as_len())
}

/// 同 [`capped`]，上限（`cap` 字节）是参数：过了 ⇒ 说出整份多大 · 上限多少的那一句。
fn capped_at<T: serde::Serialize>(v: T, cap: usize) -> Result<T, String> {
    let size = serde_json::to_vec(&v).map_or(0, |b| b.len());
    if size > cap {
        return Err(copy_text(
            "beFeatureFace.answerAt.tooLarge",
            &[("size", &size.to_string()), ("cap", &cap.to_string())],
        ));
    }
    Ok(v)
}

/// [`mcp_reply`] 写字要的：这台活会话里说断开的那几个 · 此刻（unix 秒）与看的那一台的时区 · 读的是哪一家 · 每条写不写那一家 ·
/// 哪个项目目录（项目级 · 本地级那几条的登录要它）。
#[derive(Default)]
pub struct McpSay<'a> {
    /// 读的那一份注册表（需登录那一格的解法 · 那一家的叫法照它认）。
    pub registry: &'a [crate::agents::Adapter],
    pub failed: &'a [crate::observe::accounts_query::LiveMcpFailed],
    pub now_s: i64,
    pub tz: crate::common::time::Tz,
    pub kind: &'a str,
    /// 每条的 `agent` 写上 `kind`；默认那一家 ⇒ `false`（不写）。
    pub tagged: bool,
    pub project: Option<&'a str>,
    /// 那一家各号的家（[`crate::agents::McpLook::homes`]）：每条出自哪个号照它认（[`crate::agents::mcp_held`]）。
    pub homes: &'a [(Option<String>, std::path::PathBuf)],
}

/// `mcp-read` 的回包。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct McpReadReply {
    pub entries: Vec<McpReadEntry>,
}

/// `mcp-read` 的一条（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 钉）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct McpReadEntry {
    /// 哪一家的（线上的 kind）；默认那一家 ⇒ 缺（同扩展页那一行 `ExtRow.agent`）；界面照名字与家一起挑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    /// 闭集 `user` · `local` · `project`（适配层那一格写的）。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"user\" | \"local\" | \"project\"")
    )]
    pub scope: &'static str,
    pub name: String,
    /// 那一条配置原样（未知字段不丢）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub server: Value,
    /// 读自哪一份文件（几个号合成的一条 ⇒ 头一个号那一份）。
    pub source_path: String,
    /// 出自哪几个号（账号库里的名字，排好序）：用户级 · 本地级同名同配置的几号合成这一条；没设账号的那一份 · 不在哪个号家里的 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub accounts: Vec<String>,
    /// 同一 scope 同名、各号配置却不一样 ⇒ `true`（这几条照实分开列、各带各的号）；一致 ⇒ 缺。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub mismatch: bool,
    /// 「在哪几个号里 · 各号不一致」写好的字（界面照抄）；没号名又一致 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub held: Option<McpHeldMark>,
    /// 配置层只说三种：`needsLogin` · `disabled` · `unknown`（不说连上了）。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"needsLogin\" | \"disabled\" | \"unknown\"")
    )]
    pub status: crate::agents::McpStatus,
    /// 小标画哪一种：断开压过需登录；都没有 ⇒ `null`（只说装了，不说连上了）。
    pub mark: Option<McpMark>,
    pub login: Option<McpLoginMark>,
    pub failed: Option<McpFailedMark>,
}

/// 小标那一种（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub enum McpMark {
    NeedsLogin,
    Failed,
}

/// 需登录时写好的字：抽屉那一句 · 悬停那一截 · 每个要登的号怎么解（没设账号的那一份 ⇒ 一项、`account: null`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct McpLoginMark {
    pub said: String,
    pub tip: String,
    pub fix: Vec<crate::faces::login_face::McpLoginFix>,
}

/// 出自哪几个号 · 各号不一致时写好的字：抽屉那一句（点出号名）· 悬停那一截（不一致才有）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct McpHeldMark {
    pub said: String,
    pub tip: Option<String>,
}

/// 那一格的字：有号名 ⇒ 点出号名；不一致 ⇒ 另说不一致、带悬停；没号名又一致 ⇒ 不写。
fn held_mark(accounts: &[String], mismatch: bool) -> Option<McpHeldMark> {
    let who = accounts.join(&copy_text("plan.page.listSep", &[]));
    match (who.is_empty(), mismatch) {
        (true, false) => None,
        (false, false) => Some(McpHeldMark {
            said: copy_text("beMcp.mark.accounts", &[("accounts", &who)]),
            tip: None,
        }),
        (bare, true) => Some(McpHeldMark {
            said: if bare {
                copy_text("beMcp.mark.mismatchBare", &[])
            } else {
                copy_text("beMcp.mark.mismatch", &[("accounts", &who)])
            },
            tip: Some(copy_text("beMcp.mark.mismatchTip", &[])),
        }),
    }
}

/// 断开时写好的字：抽屉那一句 · 悬停那一截 · 那一家的原话（没写 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct McpFailedMark {
    pub said: String,
    pub tip: String,
    pub detail: Option<String>,
}

/// `mcp-read` 的成品 —— 纯构造器（跨语言金样 `tests/__fixtures__/mcp-read.golden.json` 拿它对拍）。
/// 每条带写好的字（界面照抄）：`login`（需登录那一句 · 悬停那一截 · 复制的命令）· `failed`（断开那一句 · 悬停那一截 · 原话）·
/// `mark`（小标画哪一种：断开压过需登录；停用的不连 ⇒ 不标断开）。
pub fn mcp_reply(r: &crate::agents::McpRead, say: &McpSay) -> McpReadReply {
    use crate::agents::McpStatus;
    let time = |ms: i64| crate::common::time::fmt_at(ms.div_euclid(1000), say.now_s, &say.tz);
    // 每条出自哪几个号、几号同名同配置合成一条、不一致的照实分开：判法只住核心那一处（`agents::mcp_held`）。
    let entries = crate::agents::mcp_held(&r.entries, say.homes)
        .into_iter()
        .map(|held| {
            let e = &held.entry;
            let login = (e.status == McpStatus::NeedsLogin).then(|| {
                // 那一句只点号名；没设账号那一位只出在解法里（`account: null`），只有它时那一句不带号。
                let names: Vec<&str> = e.login_in.iter().flatten().map(String::as_str).collect();
                let who = names.join(&copy_text("plan.page.listSep", &[]));
                let at = e.seen_ms.map_or(String::new(), |t| {
                    time(i64::try_from(t).unwrap_or(i64::MAX))
                });
                let (said, tip) = if who.is_empty() {
                    (
                        copy_text("beMcp.mark.loginBare", &[("time", &at)]),
                        copy_text("acct.tag.login", &[]),
                    )
                } else {
                    (
                        copy_text("beMcp.mark.login", &[("accounts", &who), ("time", &at)]),
                        copy_text("beMcp.mark.loginTip", &[("accounts", &who)]),
                    )
                };
                // 用户级那一条不属于哪个项目：不带项目目录。
                let project = (e.scope != "user").then_some(say.project).flatten();
                McpLoginMark {
                    said,
                    tip,
                    fix: e
                        .login_in
                        .iter()
                        .cloned()
                        .map(|a| {
                            crate::faces::login_face::mcp_fix(
                                say.registry,
                                say.kind,
                                a,
                                &e.name,
                                project,
                            )
                        })
                        .collect(),
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
                        tip: copy_text("beServer.words.cantConnect", &[]),
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
                agent: say.tagged.then(|| cmd_wire::AgentKind::from(say.kind)),
                scope: e.scope,
                name: e.name.clone(),
                server: e.server.clone(),
                source_path: e.source.clone(),
                status: e.status,
                mark,
                login,
                failed,
                held: held_mark(&held.accounts, held.mismatch),
                accounts: held.accounts,
                mismatch: held.mismatch,
            }
        })
        .collect();
    McpReadReply { entries }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/feature_face_tests.rs"]
mod tests;

/// `session-terminals`：此刻是哪个终端在显示这个会话（↗ 点那一刻问一次）。阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。
pub struct SessionTerminals;

crate::wire::typed::codes! {
    /// `session-terminals` 自己的码。
    pub enum SessionTerminalsErr {
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
    async fn run(
        req: crate::observe::session_terminals::SidReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<crate::observe::session_terminals::Showing, SessionTerminalsErr> {
        crate::observe::session_terminals::answer_at(&crate::agents::local_homes(), &req).map_err(
            |s| match s.code.as_deref() {
                Some("bad_args") => SessionTerminalsErr::BadArgs(s.said),
                Some("no_such_session") => SessionTerminalsErr::NoSuchSession(s.said),
                Some(crate::platform::child::TIMED_OUT) => {
                    SessionTerminalsErr::ChildTimedOut(s.said)
                }
                _ => SessionTerminalsErr::Failed(s.said),
            },
        )
    }
}
