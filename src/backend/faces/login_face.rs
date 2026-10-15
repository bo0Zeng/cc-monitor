//! 帧面宿主：在 app 里登录一个号 · 一个号里的一个 MCP 服务器（`login-start` · `login-finish` · `login-cancel`，本体 `accounts::sign_in`）
//! ＋ 会话外的需手动清单（`manual-list`：需登录的号 · 需登录的 MCP，每一项带写好的那一句和怎么解）。
//! `login-start` 要跟着发起它的那条连接走（连接断了作业就收）⇒ 在分派的硬臂里（内建档），入参 · 回包 · 码表照样从这里的类型来。

use crate::accounts::sign_in::{self, FinishFail, SignInEnd};
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, NoArgs};
use copy_core::copy_text;
use serde_json::Value;

/// `login-start` 的入参：登录什么（同一个目标新开一份就替掉旧的；`login-finish` 成了原样交回）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum LoginStartReq {
    /// 一个号：哪一家（`accounts-list` 的 `agent`）· 账号库里的哪个号（账号 0 是 `0`）。
    Account { agent: String, account: String },
    /// 一个号里的一个 MCP 服务器：哪一家 · 哪个号（没设账号 ⇒ `null`：那一家的默认配置目录）· 服务器名（`mcp-read` 里那一条）·
    /// 那一条配置所属的项目目录（项目级 · 本地级要它；用户级 ⇒ `null`）。
    Mcp {
        agent: String,
        account: Option<String>,
        server: String,
        project_dir: Option<String>,
    },
}

/// `login-start` 的整份入参：登录什么（[`LoginStartReq`]，与它同一层）＋ 发起这一趟的浏览器在不在后端这台（`browserHere`）。
/// 那一格不进目标：同一个目标从哪台浏览器发起都是同一份作业（新开就替掉旧的），`login-finish` 交回的目标也不带它。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginStartArgs {
    #[serde(flatten)]
    pub(crate) target: LoginStartReq,
    /// 打开登录链接的浏览器在后端这台上（桌面连本机）：登录页回跳到这台的本机端口，那一家自己登完。
    /// 缺 ⇒ 按「不在」答（说「会自己登完」却登不完是假话，反过来只是多留一个粘的框）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) browser_here: bool,
}

/// 粘回来的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum PasteKind {
    /// 登录页上给的那一段码（Claude：`码#state`）。
    Code,
    /// 浏览器回跳到本机的那一整条地址（浏览器回不来时地址栏里那一条；Claude 的 MCP 登录）。
    RedirectUrl,
}

impl PasteKind {
    fn of(p: crate::agents::PasteShape) -> PasteKind {
        match p {
            crate::agents::PasteShape::Code => PasteKind::Code,
            crate::agents::PasteShape::RedirectUrl => PasteKind::RedirectUrl,
        }
    }
}

/// `login-start` 的成品：作业号 · 登录链接（在浏览器里打开；不用内嵌网页，密码不经界面）· 粘回来的是什么 ·
/// 会不会自己登完（发起它的浏览器在这台 ⇒ 回调到那一家自己开的本机端口，登完推一帧 `changed {login}`，粘码框只是后备；
/// 不在这台 ⇒ `false`，只能粘）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginStarted {
    pub(crate) job: String,
    pub(crate) url: String,
    pub(crate) paste: PasteKind,
    pub(crate) auto: bool,
}

/// 登录失败之后还能怎么办（`login-start` · `login-finish` 失败的 `data.again`）：由后端照码定，界面照它画，不按码去猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum LoginAgainHow {
    /// 作业还在，接着粘（粘的框留着）。
    Paste,
    /// 作业没了，从头再起一趟（`login-start`）。
    Restart,
    /// 没路：再来一趟也是同样的结果（没有这个号 · 这一家不在 app 里登 …）。
    None,
}

/// 登录失败的 `data`：还能怎么办。每个码一个定值，写在码表那一行上（`bad_args` 不带：入参形状不对，不是一条能走的路）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginAgain {
    pub(crate) again: LoginAgainHow,
}

impl LoginAgain {
    const PASTE: LoginAgain = LoginAgain {
        again: LoginAgainHow::Paste,
    };
    const RESTART: LoginAgain = LoginAgain {
        again: LoginAgainHow::Restart,
    };
    const NONE: LoginAgain = LoginAgain {
        again: LoginAgainHow::None,
    };
}

typed::codes! {
    /// `login-start` 可能回的码（每个码带 `data: {again}`，值写在各码那一行）。
    pub(crate) enum LoginStartErr {
        /// 账号库里没有这个号 · 那一份配置里没有这个 MCP 服务器。`again: none`。
        NotFound = "not_found" => LoginAgain,
        /// 这一家不在 app 里登（号 · MCP）· 这个号是 API 号（不用登录）。`again: none`。
        Unsupported = "unsupported" => LoginAgain,
        /// 起不来 · 没给出登录链接就退了 · 等不到链接（原话里是它最后说的几行）。`again: restart`。
        Failed = "failed" => LoginAgain,
    }
}

impl LoginStartErr {
    fn not_found(said: String) -> Self {
        LoginStartErr::NotFound(said, LoginAgain::NONE)
    }
    fn unsupported(said: String) -> Self {
        LoginStartErr::Unsupported(said, LoginAgain::NONE)
    }
    fn failed(said: String) -> Self {
        LoginStartErr::Failed(said, LoginAgain::RESTART)
    }
}

/// `login-start`（帧面硬臂：作业跟着发起它的那条连接走；起子命令 ＋ 等它打印链接 ⇒ 阻塞档，`dispatch` 带本连接的号调 [`start`]）。
pub(crate) struct LoginStart;

impl Command for LoginStart {
    const NAME: &'static str = "login-start";
    type Req = LoginStartArgs;
    type Reply = LoginStarted;
    type Err = LoginStartErr;
}

/// 号在这台账号库里的样子（与 `accounts-list` 同一份成品）：配置目录（账号 0 ⇒ `None`）· 是不是 API 号。
fn account_of(agent: &str, account: &str) -> Option<(Option<String>, bool)> {
    accounts_product(agent)
        .accounts
        .into_iter()
        .find(|a| a.scanned.name == account)
        .map(|a| {
            (
                a.scanned.config_dir.filter(|d| !d.is_empty()),
                a.scanned.auth_kind == acct_core::AUTH_KIND_API_KEY,
            )
        })
}

fn accounts_product(agent: &str) -> crate::observe::accounts_query::AccountsList {
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
    args: &LoginStartArgs,
    conn: u64,
    path: Option<std::ffi::OsString>,
    on_end: sign_in::OnEnd,
) -> Result<LoginStarted, LoginStartErr> {
    let unsupported = |agent: &str| {
        LoginStartErr::unsupported(copy_text("beLogin.agent.unsupported", &[("agent", agent)]))
    };
    let not_found = |account: &str| {
        LoginStartErr::not_found(copy_text(
            "beLogin.account.notFound",
            &[("account", account)],
        ))
    };
    // 目标键：入参原样（同一个目标新开就替掉旧的；`finish` 成了解回来交给界面）。
    let req = &args.target;
    let target = serde_json::to_string(req).unwrap_or_default();
    let (face, dir, then, cwd) = match req {
        LoginStartReq::Account { agent, account } => {
            let face = crate::agents::accounts_face(agent)
                .and_then(|f| f.sign_in)
                .ok_or_else(|| unsupported(agent))?;
            let (dir, api) = account_of(agent, account).ok_or_else(|| not_found(account))?;
            if api {
                return Err(LoginStartErr::unsupported(copy_text(
                    "beLogin.account.apiKey",
                    &[("account", account)],
                )));
            }
            (face, dir, Vec::new(), crate::platform::paths::home_dir())
        }
        LoginStartReq::Mcp {
            agent,
            account,
            server,
            project_dir,
        } => {
            let face = crate::agents::mcp_face(agent)
                .and_then(|f| f.sign_in)
                .ok_or_else(|| unsupported(agent))?;
            // 号：给了就要在账号库里（API 号也照登：MCP 登录与号怎么登无关）；没给 ⇒ 那一家的默认配置目录。
            let dir = match account {
                Some(a) => account_of(agent, a).ok_or_else(|| not_found(a))?.0,
                None => None,
            };
            let project = project_dir.as_deref().map(std::path::Path::new);
            if !mcp_listed(agent, project, server) {
                return Err(LoginStartErr::not_found(copy_text(
                    "beLogin.mcp.notFound",
                    &[("server", server)],
                )));
            }
            let cwd = project
                .map(std::path::Path::to_path_buf)
                .or_else(crate::platform::paths::home_dir);
            (face, dir, vec![server.clone()], cwd)
        }
    };
    let ask = sign_in::SignInAsk {
        face,
        target: &target,
        dir: dir.as_deref(),
        then: &then,
        conn,
        cwd,
        path,
    };
    let s = sign_in::start(&ask, on_end)
        .map_err(|s| LoginStartErr::failed(Fail::from(("failed", s)).into_said()))?;
    Ok(started(s, face, args))
}

/// 起好了的那一份 ⇒ 回包：会不会自己登完照发起它的浏览器在不在这台（不在 ⇒ 回跳到不了这台的本机端口，只能粘）。
fn started(
    s: sign_in::Started,
    face: crate::agents::SignInFace,
    args: &LoginStartArgs,
) -> LoginStarted {
    LoginStarted {
        job: s.job,
        url: s.url,
        paste: PasteKind::of(face.paste),
        auto: args.browser_here,
    }
}

/// 这个名字是不是那一份配置里的一个 MCP 服务器（`mcp-read` 同一个读法；项目目录给了 ⇒ 连项目级 · 本地级一起看）。
/// 名字要进子命令的 argv ⇒ 以 `-` 开头的一律不认（不会被当成旗标）。
fn mcp_listed(agent: &str, project: Option<&std::path::Path>, server: &str) -> bool {
    if server.is_empty() || server.starts_with('-') {
        return false;
    }
    let look = crate::accounts::manage::mcp_share_exec::mcp_look(agent);
    crate::agents::mcp_read(agent, project, &look)
        .is_some_and(|r| r.entries.iter().any(|e| e.name == server))
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

/// `login-finish` 的成品：登成了的那个目标（`login-start` 时交的那一份）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LoginDone {
    pub(crate) target: LoginStartReq,
}

typed::codes! {
    /// `login-finish` 可能回的码（每个码带 `data: {again}`，值写在各码那一行）。
    pub(crate) enum LoginFinishErr {
        /// 粘的格式不对，或那一家说码不对：作业还在，可以再粘。`again: paste`。
        BadPaste = "bad_paste" => LoginAgain,
        /// 那一家换码没成：作业收了，要重来（原话里是它最后说的几行）。`again: restart`。
        Refused = "refused" => LoginAgain,
        /// 没有这份作业（收了 · 被替掉）。`again: restart`。
        NoJob = "no_job" => LoginAgain,
        /// 等了一阵还没答：作业还在（那一家答了就推 `changed {login}`），粘的框留着。`again: paste`。
        ChildTimedOut = "child_timed_out" => LoginAgain,
    }
}

impl LoginFinishErr {
    fn of(f: FinishFail) -> Self {
        match f {
            FinishFail::NoJob => Self::no_job(),
            FinishFail::BadPaste => LoginFinishErr::BadPaste(
                copy_text("beLogin.finish.badPaste", &[]),
                LoginAgain::PASTE,
            ),
            FinishFail::Slow => LoginFinishErr::ChildTimedOut(
                copy_text("beLogin.finish.slow", &[]),
                LoginAgain::PASTE,
            ),
            FinishFail::Refused(s) => {
                LoginFinishErr::Refused(Fail::from(("refused", s)).into_said(), LoginAgain::RESTART)
            }
        }
    }
    fn no_job() -> Self {
        LoginFinishErr::NoJob(copy_text("beLogin.finish.noJob", &[]), LoginAgain::RESTART)
    }
}

impl Command for LoginFinish {
    const NAME: &'static str = "login-finish";
    type Req = LoginFinishReq;
    type Reply = LoginDone;
    type Err = LoginFinishErr;
}

impl crate::wire::typed::Handle for LoginFinish {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: LoginFinishReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<LoginDone, LoginFinishErr>> + Send + 'static {
        async move {
            sign_in::finish(&req.job, &req.pasted)
                .map_err(LoginFinishErr::of)
                .and_then(|key| {
                    // 键是起它时那份入参原样 ⇒ 解得回来；解不回来不会发生（只有本模块写键），照「没有这份作业」答。
                    serde_json::from_str(&key)
                        .map(|target| LoginDone { target })
                        .map_err(|_| LoginFinishErr::no_job())
                })
        }
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

impl crate::wire::typed::Handle for LoginCancel {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: LoginCancelReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<LoginCancelled, LoginCancelErr>> + Send + 'static
    {
        async move {
            Ok(LoginCancelled {
                was: sign_in::cancel(&req.job),
            })
        }
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
    /// 一个 MCP 服务器要登录（那一家最近一次连它时记下的、还在有效期内）。`fix` ＝ 在 app 里登：`login-start {kind: mcp}`；
    /// 那一家不在 app 里登 MCP ⇒ `null`（只能在会话里登），这时 `why` 说为什么没有解法。
    McpLogin {
        /// 在哪个号里；没设账号的那一份 ⇒ `null`。
        account: Option<String>,
        server: String,
        scope: String,
        said: String,
        fix: Option<ManualFix>,
        /// `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        why: Option<String>,
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
    /// `manual-list` 可能回的码。MCP 的缓存读不到按「没有」答（那一项本来就是「最近一次连它时记下的」）。
    pub(crate) enum ManualListErr {
        /// 账号库清单在、却读不懂：说不出哪些号需登录，不回空清单（空清单看起来像「没事」）。那一句是为什么。
        Unreadable = "unreadable",
    }
}

impl Command for ManualListCmd {
    const NAME: &'static str = "manual-list";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = ManualList;
    type Err = ManualListErr;
}

impl crate::wire::typed::Handle for ManualListCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ManualList, ManualListErr>> + Send + 'static {
        async move {
            let Some(agent) = crate::agents::sole_kind(|a| a.accounts.is_some()) else {
                return Ok(ManualList { items: Vec::new() });
            };
            let mcp = crate::agents::sole_kind(|a| a.mcp.is_some()).map(|kind| {
                let look = crate::accounts::manage::mcp_share_exec::mcp_look(kind);
                (
                    kind,
                    crate::agents::mcp_read(kind, None, &look).unwrap_or_default(),
                )
            });
            manual_of(
                agent,
                &serde_json::to_value(accounts_product(agent)).unwrap_or_default(),
                crate::observe::accounts_query::library_unreadable_here(),
                mcp.as_ref().map(|(k, r)| (*k, r)),
            )
        }
    }
}

/// [`ManualListCmd`] 的纯构造器：账号清单成品（`accounts-list` 那一份）· MCP 读面（哪一家的 · 用户级那一份）⇒ 清单。判据拿夹具喂它。
/// `unreadable` ＝ 账号库清单在、却读不懂的那一句（`accounts_query::library_unreadable`）：给了 ⇒ 回 `unreadable`，不列。
pub(crate) fn manual_of(
    agent: &str,
    accounts: &Value,
    unreadable: Option<String>,
    mcp: Option<(&str, &crate::agents::McpRead)>,
) -> Result<ManualList, ManualListErr> {
    if let Some(why) = unreadable {
        return Err(ManualListErr::Unreadable(why));
    }
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
                args: LoginStartReq::Account {
                    agent: agent.to_string(),
                    account: name.to_string(),
                },
            },
        })
        .collect();
    let (mcp_kind, entries) = match mcp {
        Some((k, m)) => (k, m.entries.as_slice()),
        None => ("", &[][..]),
    };
    let mcp_signs_in = crate::agents::mcp_face(mcp_kind).is_some_and(|f| f.sign_in.is_some());
    for e in entries {
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
            let why = (!mcp_signs_in).then(|| {
                copy_text(
                    "beManual.list.mcpNoFix",
                    &[("agent", &agent_name(mcp_kind))],
                )
            });
            let fix = mcp_signs_in.then(|| ManualFix {
                command: "login-start",
                args: LoginStartReq::Mcp {
                    agent: mcp_kind.to_string(),
                    account: account.clone(),
                    server: e.name.clone(),
                    project_dir: None,
                },
            });
            items.push(ManualItem::McpLogin {
                account,
                server: e.name.clone(),
                scope: e.scope.to_string(),
                said,
                fix,
                why,
            });
        }
    }
    Ok(ManualList { items })
}

/// 这一家对用户的叫法（没有起会话那一面 ⇒ 它的 id）。
fn agent_name(kind: &str) -> String {
    crate::agents::launch_face_among(crate::agents::REGISTRY, kind)
        .map_or(kind, |l| l.display_name)
        .to_string()
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/login_face_tests.rs"]
mod tests;
