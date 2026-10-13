//! 帧面宿主：在 app 里登录一个号（`login-start` · `login-finish` · `login-cancel`，本体 `accounts::sign_in`）
//! ＋ 会话外的需手动清单（`manual-list`：需登录的号 · 需登录的 MCP，每一项带写好的那一句和怎么解）。
//! `login-start` 要跟着发起它的那条连接走（连接断了作业就收）⇒ 在分派的硬臂里（内建档），入参 · 回包 · 码表照样从这里的类型来。

use crate::accounts::sign_in::{self, FinishFail, SignInEnd};
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::typed::{self, Blocking, Codes, Command, NoArgs};
use copy_core::copy_text;
use serde_json::Value;

/// `login-start` 的入参：哪一家（`accounts-list` 的 `agent`）· 账号库里的哪个号（账号 0 是 `0`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginStartReq {
    pub(crate) agent: String,
    pub(crate) account: String,
}

/// 粘回来的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum PasteKind {
    /// 登录页上给的那一段码（Claude：`码#state`）。
    Code,
}

/// `login-start` 的成品：作业号 · 登录链接（在浏览器里打开；不用内嵌网页，密码不经界面）· 粘回来的是什么 ·
/// 会不会自己登完（本机浏览器回调到那一家自己开的本机端口；登完推一帧 `changed {login}`，粘码框只是后备）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginStarted {
    pub(crate) job: String,
    pub(crate) url: String,
    pub(crate) paste: PasteKind,
    pub(crate) auto: bool,
}

typed::codes! {
    /// `login-start` 可能回的码。
    pub(crate) enum LoginStartErr {
        /// 账号库里没有这个号。
        NotFound = "not_found",
        /// 这一家不在 app 里登 · 这个号是 API 号（不用登录）。
        Unsupported = "unsupported",
        /// 起不来 · 没给出登录链接就退了 · 等不到链接（原话里是它最后说的几行）。
        Failed = "failed",
    }
}

/// `login-start` 的码表（登记表那一行用它，码不另写）。
pub(crate) const LOGIN_START_CODES: &[&str] = <LoginStartErr as Codes>::ALL;

/// 号在这台账号库里的样子（与 `accounts-list` 同一份成品）：配置目录（账号 0 ⇒ `None`）· 是不是 API 号。
fn account_of(agent: &str, account: &str) -> Option<(Option<String>, bool)> {
    let v = accounts_product(agent);
    v["accounts"]
        .as_array()?
        .iter()
        .find(|a| a["name"] == account)
        .map(|a| {
            (
                a["configDir"]
                    .as_str()
                    .filter(|d| !d.is_empty())
                    .map(str::to_string),
                a["authKind"] == acct_core::AUTH_KIND_API_KEY,
            )
        })
}

fn accounts_product(agent: &str) -> Value {
    let rows = crate::accounts::upstream_select::file_face::machine_rows();
    crate::observe::accounts_query::list_product(
        &rows,
        agent,
        crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
        crate::relay::our_relay_listening(relay_route_core::PORT),
    )
}

/// `login-start` 的本体（分派的硬臂调它：`conn` ＝ 本连接的号，`on_end` ＝ 作业自己收了时推给本连接的那一帧）。
pub(crate) fn start(
    req: &LoginStartReq,
    conn: u64,
    path: Option<std::ffi::OsString>,
    on_end: sign_in::OnEnd,
) -> Result<LoginStarted, LoginStartErr> {
    let face = crate::agents::accounts_face(&req.agent)
        .and_then(|f| f.sign_in)
        .ok_or_else(|| {
            LoginStartErr::Unsupported(copy_text(
                "beLogin.agent.unsupported",
                &[("agent", &req.agent)],
            ))
        })?;
    let (dir, api) = account_of(&req.agent, &req.account).ok_or_else(|| {
        LoginStartErr::NotFound(copy_text(
            "beLogin.account.notFound",
            &[("account", &req.account)],
        ))
    })?;
    if api {
        return Err(LoginStartErr::Unsupported(copy_text(
            "beLogin.account.apiKey",
            &[("account", &req.account)],
        )));
    }
    let ask = sign_in::SignInAsk {
        face,
        account: &req.account,
        dir: dir.as_deref(),
        conn,
        cwd: crate::platform::paths::home_dir(),
        path,
    };
    let s = sign_in::start(&ask, on_end)
        .map_err(|s| typed::of_fail(Fail::from(("failed", s)), LoginStartErr::Failed))?;
    Ok(LoginStarted {
        job: s.job,
        url: s.url,
        paste: PasteKind::Code,
        auto: true,
    })
}

/// 硬臂那一发：入参按类型收（同 typed 那一处）→ 本体 → 线上那一份。
pub(crate) fn answer_start(
    args: &Value,
    conn: u64,
    on_end: sign_in::OnEnd,
) -> crate::stream::inbound::spec::Outcome {
    let req: LoginStartReq = typed::take(args)?;
    let got = start(&req, conn, None, on_end).map_err(Codes::into_fail)?;
    crate::stream::inbound::spec::wire::<_, Fail>(&got).map(Some)
}

/// 推给发起它的那条连接的那一帧的小成品（`changed {login, key: 作业号, body}`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum SignInState {
    /// 登成了。
    Done,
    /// 登不成：那一句（原话进 `detail`）。
    Failed {
        said: String,
        detail: Option<String>,
    },
}

impl SignInState {
    pub(crate) fn of(e: &SignInEnd) -> SignInState {
        match e {
            SignInEnd::Done => SignInState::Done,
            SignInEnd::Failed(s) => SignInState::Failed {
                said: s.said.clone(),
                detail: s.raw.clone(),
            },
        }
    }
}

/// `login-finish`：把登录页上给的码交给那份作业，等那一家换完。
pub(crate) struct LoginFinish;

/// `login-finish` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginFinishReq {
    pub(crate) job: String,
    /// 登录页上给的那一段（原样；不进日志、不回显）。
    pub(crate) pasted: String,
}

/// `login-finish` 的成品：登成了的那个号。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginDone {
    pub(crate) account: String,
}

typed::codes! {
    /// `login-finish` 可能回的码。
    pub(crate) enum LoginFinishErr {
        /// 粘的格式不对，或那一家说码不对：作业还在，可以再粘。
        BadPaste = "bad_paste",
        /// 那一家换码没成：作业收了，要重来（原话里是它最后说的几行）。
        Refused = "refused",
        /// 没有这份作业（收了 · 被替掉）。
        NoJob = "no_job",
        /// 等了一阵还没答：作业还在。
        ChildTimedOut = "child_timed_out",
    }
}

impl Command for LoginFinish {
    const NAME: &'static str = "login-finish";
    type Req = LoginFinishReq;
    type Reply = LoginDone;
    type Err = LoginFinishErr;
}

impl Blocking for LoginFinish {
    fn run(req: LoginFinishReq, _cx: &typed::Cx) -> Result<LoginDone, LoginFinishErr> {
        sign_in::finish(&req.job, &req.pasted)
            .map(|account| LoginDone { account })
            .map_err(|f| match f {
                FinishFail::NoJob => LoginFinishErr::NoJob(copy_text("beLogin.finish.noJob", &[])),
                FinishFail::BadPaste => {
                    LoginFinishErr::BadPaste(copy_text("beLogin.finish.badPaste", &[]))
                }
                FinishFail::Slow => {
                    LoginFinishErr::ChildTimedOut(copy_text("beLogin.finish.slow", &[]))
                }
                FinishFail::Refused(s) => {
                    typed::of_fail(Fail::from(("refused", s)), LoginFinishErr::Refused)
                }
            })
    }
}

/// `login-cancel`：收一份作业（杀那一家的登录子命令）。
pub(crate) struct LoginCancel;

/// `login-cancel` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginCancelReq {
    pub(crate) job: String,
}

/// `login-cancel` 的成品：有没有这一份（没有 ⇒ 早收了，照样算撤了）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginCancelled {
    pub(crate) was: bool,
}

typed::codes! {
    /// `login-cancel` 自己没有别的码。
    pub(crate) enum LoginCancelErr {}
}

impl Command for LoginCancel {
    const NAME: &'static str = "login-cancel";
    type Req = LoginCancelReq;
    type Reply = LoginCancelled;
    type Err = LoginCancelErr;
}

impl Blocking for LoginCancel {
    fn run(req: LoginCancelReq, _cx: &typed::Cx) -> Result<LoginCancelled, LoginCancelErr> {
        Ok(LoginCancelled {
            was: sign_in::cancel(&req.job),
        })
    }
}

/// 一项需手动怎么解：问哪条命令、交什么入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ManualFix {
    pub(crate) command: &'static str,
    pub(crate) args: LoginStartReq,
}

/// 会话外的一项需手动（每一项带写好的那一句）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ManualItem {
    /// 一个订阅号还没登录（`fix` ＝ 在 app 里登：`login-start`）。
    AccountLogin {
        agent: String,
        account: String,
        said: String,
        fix: ManualFix,
    },
    /// 一个 MCP 服务器要登录（那一家最近一次连它时记下的、还在有效期内）。在 app 里登 MCP 还没做 ⇒ 没有 `fix`。
    McpLogin {
        /// 在哪个号里；没设账号的那一份 ⇒ `null`。
        account: Option<String>,
        server: String,
        scope: String,
        said: String,
    },
}

/// `manual-list` 的成品。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ManualList {
    pub(crate) items: Vec<ManualItem>,
}

/// `manual-list`：会话外的需手动清单（需登录的号 · 需登录的 MCP）。只读。
pub(crate) struct ManualListCmd;

typed::codes! {
    /// `manual-list` 读不到的一律按「没有」答，自己没有别的码。
    pub(crate) enum ManualListErr {}
}

impl Command for ManualListCmd {
    const NAME: &'static str = "manual-list";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = ManualList;
    type Err = ManualListErr;
}

impl Blocking for ManualListCmd {
    fn run(_req: NoArgs, _cx: &typed::Cx) -> Result<ManualList, ManualListErr> {
        let Some(agent) = crate::agents::sole_kind(|a| a.accounts.is_some()) else {
            return Ok(ManualList { items: Vec::new() });
        };
        let mcp = crate::agents::sole_kind(|a| a.mcp.is_some()).map(|kind| {
            let look = crate::faces::feature_face::mcp_look(kind);
            crate::agents::mcp_read(kind, None, &look).unwrap_or_default()
        });
        Ok(manual_of(agent, &accounts_product(agent), mcp.as_ref()))
    }
}

/// [`ManualListCmd`] 的纯构造器：账号清单成品（`accounts-list` 那一份）· MCP 读面 ⇒ 清单。判据拿夹具喂它。
pub(crate) fn manual_of(
    agent: &str,
    accounts: &Value,
    mcp: Option<&crate::agents::McpRead>,
) -> ManualList {
    let signs_in = crate::agents::accounts_face(agent).is_some_and(|f| f.sign_in.is_some());
    let mut items: Vec<ManualItem> = accounts["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| {
            signs_in
                && a["authKind"] == acct_core::AUTH_KIND_SUBSCRIPTION
                && a["exists"] == true
                && a["authReady"] == false
        })
        .filter_map(|a| a["name"].as_str())
        .map(|name| ManualItem::AccountLogin {
            agent: agent.to_string(),
            account: name.to_string(),
            said: copy_text(
                "beManual.list.accountLogin",
                &[(
                    "account",
                    &crate::accounts::quota::name_words::account_text(name),
                )],
            ),
            fix: ManualFix {
                command: "login-start",
                args: LoginStartReq {
                    agent: agent.to_string(),
                    account: name.to_string(),
                },
            },
        })
        .collect();
    for e in mcp.map(|m| m.entries.as_slice()).unwrap_or_default() {
        if e.status != crate::agents::McpStatus::NeedsLogin {
            continue;
        }
        let who: Vec<Option<String>> = if e.login_in.is_empty() {
            vec![None]
        } else {
            e.login_in.iter().cloned().map(Some).collect()
        };
        for account in who {
            let said = match &account {
                Some(a) => copy_text(
                    "beManual.list.mcpLogin",
                    &[
                        ("server", &e.name),
                        (
                            "account",
                            &crate::accounts::quota::name_words::account_text(a),
                        ),
                    ],
                ),
                None => copy_text("beManual.list.mcpLoginBare", &[("server", &e.name)]),
            };
            items.push(ManualItem::McpLogin {
                account,
                server: e.name.clone(),
                scope: e.scope.to_string(),
                said,
            });
        }
    }
    ManualList { items }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/login_face_tests.rs"]
mod tests;
