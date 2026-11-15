//! 帧面宿主：在 app 里登录一个号 · 一个号里的一个 MCP 服务器（`login-start` · `login-finish` · `login-cancel`，本体 `accounts::sign_in`）
//! ＋ 会话外的需手动清单（需登录的号 · 需登录的 MCP，每一项带写好的那一句和怎么解；进 `sessions-needs` 那一份）。
//! `login-start` 要跟着发起它的那条连接走（连接断了作业就收）⇒ 在分派的硬臂里（内建档），入参 · 回包 · 码表照样从这里的类型来。

use crate::accounts::sign_in::{self, FinishFail, SignInEnd};
use crate::wire::typed::{self, Command};
use copy_core::copy_text;
use std::path::Path;

pub(crate) use cmd_wire::needs::{LoginStartReq, ManualFix, ManualItem};

/// `login-start` 的整份入参：登录什么（[`LoginStartReq`]，与它同一层）＋ 发起这一趟的浏览器在不在后端这台（`browserHere`）。
/// 那一格不进目标：同一个目标从哪台浏览器发起都是同一份作业（新开就替掉旧的），`login-finish` 交回的目标也不带它。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct LoginStartArgs {
    #[serde(flatten)]
    pub(crate) target: LoginStartReq,
    /// 打开登录链接的浏览器在后端这台上（桌面连本机）：登录页回跳到这台的本机端口，那一家自己登完。
    /// 缺 ⇒ 按「不在」答（说「会自己登完」却登不完是假话，反过来只是多留一个粘的框）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) browser_here: bool,
}

/// 粘回来的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub(crate) enum PasteKind {
    /// 登录页上给的那一段码（Claude：`码#state`）。
    Code,
    /// 浏览器回跳到本机的那一整条地址（浏览器回不来时地址栏里那一条；Claude 的 MCP 登录）。
    RedirectUrl,
    /// 不粘回：那一家印了链接与一段码（回包的 `code`），在哪台的浏览器里打开链接、填那段码都行；它自己登完推 `changed {login}`。
    ShowCode,
    /// 不粘回：那一家等浏览器回跳到它那一台的本机口；只在那一台的浏览器里登得完（`auto` 说这一趟登不登得完）。
    NoPaste,
}

impl PasteKind {
    fn of(p: crate::agents::PasteShape) -> PasteKind {
        match p {
            crate::agents::PasteShape::Code(_) => PasteKind::Code,
            crate::agents::PasteShape::RedirectUrl(_) => PasteKind::RedirectUrl,
            crate::agents::PasteShape::ShowCode(_) => PasteKind::ShowCode,
            crate::agents::PasteShape::NoPaste => PasteKind::NoPaste,
        }
    }
}

/// `login-start` 的成品：作业号 · 登录链接（在浏览器里打开；不用内嵌网页，密码不经界面）· 粘回来的是什么 ·
/// 会不会自己登完（发起它的浏览器在这台 ⇒ 回调到那一家自己开的本机端口，登完推一帧 `changed {login}`，粘码框只是后备；
/// 不在这台 ⇒ `false`，只能粘）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LoginStarted {
    pub(crate) job: String,
    pub(crate) url: String,
    pub(crate) paste: PasteKind,
    /// 显示码那一形印的那段码（人在登录页里填它）；别的形 ⇒ `null`。
    pub(crate) code: Option<String>,
    pub(crate) auto: bool,
    /// 那一家自己在这台开了登录页（它那一形自己开浏览器、发起它的浏览器又在这台）⇒ 界面不再开第二页。
    pub(crate) opened: bool,
    /// 这一趟只能在那一台的浏览器里登完（不粘回、发起它的浏览器又不在那一台）⇒ 写好的那一句（两端照摆）；别的 ⇒ `null`。
    pub(crate) elsewhere: Option<String>,
}

/// 登录失败之后还能怎么办（线上形住 cmd-wire：登录推送的体也带它）。
pub(crate) use cmd_wire::quota::LoginAgainHow;

/// 登录失败的 `data`：还能怎么办（每个码一个定值，写在码表那一行上；`bad_args` 不带：入参形状不对，不是一条能走的路）·
/// 在终端窗口里登这条路在不在（只有号那一种目标、抽屉那一趟起不来时在：开终端窗口跑那台答的登录那一行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LoginAgain {
    pub(crate) again: LoginAgainHow,
    pub(crate) terminal: bool,
}

impl LoginAgain {
    const PASTE: LoginAgain = LoginAgain {
        again: LoginAgainHow::Paste,
        terminal: false,
    };
    const RESTART: LoginAgain = LoginAgain {
        again: LoginAgainHow::Restart,
        terminal: false,
    };
    const NONE: LoginAgain = LoginAgain {
        again: LoginAgainHow::None,
        terminal: false,
    };
}

/// 抽屉起不来时还有没有终端窗口那条路：号那一种有（那台答得出登录那一行），MCP 那一种没有。
pub(crate) fn terminal_way(target: &LoginStartReq) -> bool {
    matches!(target, LoginStartReq::Account { .. })
}

typed::codes! {
    /// `login-start` 可能回的码（每个码带 `data: {again, terminal}`，值写在各码那一行）。
    pub enum LoginStartErr {
        /// 账号库里没有这个号 · 那一份配置里没有这个 MCP 服务器。`again: none`。
        NotFound = "not_found" => LoginAgain,
        /// 这一家不在 app 里登（号 · MCP）· 这个号是 API 号（不用登录）。`again: none`。
        Unsupported = "unsupported" => LoginAgain,
        /// 起不来 · 没给出登录链接就退了 · 等不到链接（原话里是它最后说的几行）。`again: restart`；号那一种 `terminal: true`。
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
    fn failed(said: String, terminal: bool) -> Self {
        LoginStartErr::Failed(
            said,
            LoginAgain {
                terminal,
                ..LoginAgain::RESTART
            },
        )
    }
}

/// `login-start`（帧面硬臂：作业跟着发起它的那条连接走；起子命令 ＋ 等它打印链接 ⇒ 阻塞档，`dispatch` 带本连接的号调 [`start`]）。
pub struct LoginStart;

impl Command for LoginStart {
    const NAME: &'static str = "login-start";
    type Req = LoginStartArgs;
    type Reply = LoginStarted;
    type Err = LoginStartErr;
}

/// 这台的账号库（与 `accounts-list` 同一份成品，不按起的那一家过滤）：几家混列，每个号照它自己那一家判
/// （key 表里有没有它那一行 · 新建会话那一格），不照默认那一家。
fn library() -> crate::accounts::manage::library::AccountsList {
    crate::faces::launch_account_face::list_mixed()
}

/// 一个号的配置目录（账号 0 ⇒ `None`）。
fn dir_of(row: &crate::accounts::manage::library::AccountRow) -> Option<String> {
    row.scanned.config_dir.clone().filter(|d| !d.is_empty())
}

/// MCP 登录带了号 ⇒ 那个号的配置目录：号只认请求那一家的（同账号登录那一判，[`row_at`]）；API 号也照取（MCP 登录与号怎么登无关）。
pub(crate) fn mcp_dir_of(
    registry: &[crate::agents::Adapter],
    list: &crate::accounts::manage::library::AccountsList,
    agent: &str,
    account: &str,
) -> Result<Option<String>, AccountRefused> {
    row_at(registry, list, Some(agent), account).map(|(row, _)| dir_of(row))
}

/// 登录 · 退出 · 问状态一个号要的：那个号是哪一家的（号表里那一格）· 那一家的登录形 · 号的配置目录（账号 0 ⇒ `None`）。
pub(crate) struct AccountAt {
    pub(crate) agent: &'static str,
    pub(crate) face: crate::agents::SignInFace,
    pub(crate) dir: Option<String>,
}

/// 照号拿不出 [`AccountAt`] 的那几样（登录 · 退出 · 状态同一套；各自的码表经 [`AccountRefused::for_start`] · [`AccountRefused::for_once`]）。
#[derive(Debug, Clone)]
pub(crate) enum AccountRefused {
    /// 账号库里没有这个号。
    NotFound { account: String },
    /// 请求报了家，与号表里那个号的家对不上（`own` ＝ 号表里那一家，认不出 ⇒ 空）。
    OtherAgent {
        account: String,
        own: String,
        asked: String,
    },
    /// 号表里认不出这个号是哪一家的。
    NoAgent { account: String },
    /// 那一家不在 app 里登。
    Unsupported { agent: String },
    /// API 号（不用登录）。
    ApiKey { account: String },
}

impl AccountRefused {
    /// 那一句（家都照传进来的注册表说成对人的叫法，不露 kind）。
    fn said(&self, registry: &[crate::agents::Adapter]) -> String {
        match self {
            AccountRefused::NotFound { account } => {
                copy_text("beLogin.account.notFound", &[("account", account)])
            }
            AccountRefused::OtherAgent {
                account,
                own,
                asked,
            } => copy_text(
                "beLogin.account.otherAgent",
                &[
                    ("account", account),
                    ("own", &crate::agents::family_said_among(registry, own)),
                    ("agent", &crate::agents::family_said_among(registry, asked)),
                ],
            ),
            AccountRefused::NoAgent { account } => {
                copy_text("beLogin.account.noAgent", &[("account", account)])
            }
            AccountRefused::Unsupported { agent } => copy_text(
                "beLogin.agent.unsupported",
                &[("agent", &crate::agents::family_said_among(registry, agent))],
            ),
            AccountRefused::ApiKey { account } => {
                copy_text("beLogin.account.apiKey", &[("account", account)])
            }
        }
    }

    pub(crate) fn for_start(self, registry: &[crate::agents::Adapter]) -> LoginStartErr {
        let said = self.said(registry);
        match self {
            AccountRefused::NotFound { .. } => LoginStartErr::not_found(said),
            AccountRefused::OtherAgent { .. } => LoginStartErr::BadArgs(said),
            AccountRefused::NoAgent { .. }
            | AccountRefused::Unsupported { .. }
            | AccountRefused::ApiKey { .. } => LoginStartErr::unsupported(said),
        }
    }

    pub(crate) fn for_once(self, registry: &[crate::agents::Adapter]) -> LoginOnceErr {
        let said = self.said(registry);
        match self {
            AccountRefused::NotFound { .. } => LoginOnceErr::NotFound(said),
            AccountRefused::OtherAgent { .. } => LoginOnceErr::BadArgs(said),
            AccountRefused::NoAgent { .. }
            | AccountRefused::Unsupported { .. }
            | AccountRefused::ApiKey { .. } => LoginOnceErr::Unsupported(said),
        }
    }
}

/// 号名 ⇒ 号表里那一行与它是哪一家的（认不出 ⇒ `None`）。请求报了家（`asked`）⇒ 只取那一家的那一行
/// （看家照选号那一判 [`crate::accounts::manage::library::row_of_family`]；两家同名各取各的）；这个名字只有别家的号 ⇒ 拒。
/// 本面按号名找号的几处（账号登录 · 退出 · 状态 · MCP 登录）都经这一处。
fn row_at<'a>(
    registry: &[crate::agents::Adapter],
    list: &'a crate::accounts::manage::library::AccountsList,
    asked: Option<&str>,
    account: &str,
) -> Result<
    (
        &'a crate::accounts::manage::library::AccountRow,
        Option<&'static str>,
    ),
    AccountRefused,
> {
    let named = |a: &&crate::accounts::manage::library::AccountRow| a.scanned.name == account;
    let first = list
        .accounts
        .iter()
        .find(named)
        .ok_or_else(|| AccountRefused::NotFound {
            account: account.into(),
        })?;
    let row = match asked {
        None => first,
        Some(asked) => {
            let family = cmd_wire::AgentKind::new(asked);
            list.accounts
                .iter()
                .filter(named)
                .find(|a| crate::accounts::manage::library::row_of_family(&a.scanned, &family))
                .ok_or_else(|| AccountRefused::OtherAgent {
                    account: account.into(),
                    own: first
                        .scanned
                        .agent
                        .as_ref()
                        .map_or("", cmd_wire::AgentKind::as_str)
                        .into(),
                    asked: asked.into(),
                })?
        }
    };
    let own = row
        .scanned
        .agent
        .as_ref()
        .and_then(|k| crate::agents::account_kind_among(registry, Some(k.as_str())));
    Ok((row, own))
}

/// 号 ⇒ 登录 · 退出 · 状态起哪一家的子命令：家只照号表里那个号带的那一格（几家混列，不照请求那一家）。
/// 请求报了家（`asked`）却与号表不符 ⇒ 拒（不照它起子命令）；对得上照收。判据拿夹具注册表 · 夹具清单喂它。
pub(crate) fn account_at(
    registry: &[crate::agents::Adapter],
    list: &crate::accounts::manage::library::AccountsList,
    asked: Option<&str>,
    account: &str,
) -> Result<AccountAt, AccountRefused> {
    let (row, own) = row_at(registry, list, asked, account)?;
    let agent = own.ok_or_else(|| AccountRefused::NoAgent {
        account: account.into(),
    })?;
    let face = crate::agents::accounts_face_among(registry, agent)
        .and_then(|f| f.sign_in)
        .ok_or_else(|| AccountRefused::Unsupported {
            agent: agent.into(),
        })?;
    if row.scanned.auth_kind == acct_core::AUTH_KIND_API_KEY {
        return Err(AccountRefused::ApiKey {
            account: account.into(),
        });
    }
    Ok(AccountAt {
        agent,
        face,
        dir: dir_of(row),
    })
}

/// `login-start` 的本体（分派的硬臂调它：`conn` ＝ 本连接的号，`replies` ＝ 本连接的应答通道）。
/// 作业自己收了 ⇒ 推一帧 `changed {login}` 给本连接、敲需手动的铃（作业跟着本连接走，那一帧也只给它）。
pub fn start(
    args: &LoginStartArgs,
    conn: u64,
    replies: tokio::sync::mpsc::Sender<crate::wire::Frame>,
) -> Result<LoginStarted, LoginStartErr> {
    let on_end: sign_in::OnEnd = Box::new(move |job, ended| {
        use crate::wire::FrameMake as _;
        use copy_core::discard::{Discard as _, Why};
        replies
            .blocking_send(crate::wire::Frame::changed(crate::wire::Changed::Login {
                key: job.to_string(),
                body: Some(SignInState::of(ended)),
            }))
            .discard(Why::PeerGone); // 发起登录的那条连接已断 ⇒ 「登录作业收了」那一帧没人收（作业本来就跟着它收）
        crate::history::needs_changed::ring();
    });
    // 目标键：入参原样（同一个目标新开就替掉旧的；`finish` 成了解回来交给界面）。
    let req = &args.target;
    let target = serde_json::to_string(req).unwrap_or_default();
    let (face, dir, then, cwd) = match req {
        LoginStartReq::Account { agent, account } => {
            let at = account_at(
                crate::agents::REGISTRY,
                &library(),
                agent.as_ref().map(cmd_wire::AgentKind::as_str),
                account,
            )
            .map_err(|r| r.for_start(crate::agents::REGISTRY))?;
            (
                at.face,
                at.dir,
                Vec::new(),
                crate::platform::paths::home_dir(),
            )
        }
        LoginStartReq::Mcp {
            agent,
            account,
            server,
            project_dir,
        } => {
            let face = crate::agents::mcp_face(agent)
                .and_then(|f| f.sign_in)
                .ok_or_else(|| mcp_unsupported_among(crate::agents::REGISTRY, agent))?;
            // 号：给了就要是账号库里这一家的号（API 号也照登：MCP 登录与号怎么登无关）；没给 ⇒ 那一家的默认配置目录。
            let dir = match account {
                Some(a) => mcp_dir_of(crate::agents::REGISTRY, &library(), agent, a)
                    .map_err(|r| r.for_start(crate::agents::REGISTRY))?,
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
        path: None,
    };
    let s = sign_in::start(&ask, on_end).map_err(|s| {
        LoginStartErr::failed(
            {
                let f = crate::wire::spec::fail("failed", s);
                crate::common::contract::joined(f.said, f.raw)
            },
            terminal_way(req),
        )
    })?;
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
        code: s.code,
        // 显示码那一形在哪台的浏览器里填码都登得完；别的形只有回跳到得了那一台才自己登完。
        auto: matches!(face.paste, crate::agents::PasteShape::ShowCode(_)) || args.browser_here,
        opened: face.opens_browser && args.browser_here,
        elsewhere: (matches!(face.paste, crate::agents::PasteShape::NoPaste) && !args.browser_here)
            .then(|| copy_text("beLogin.start.elsewhere", &[])),
    }
}

/// 这个名字是不是那一份配置里的一个 MCP 服务器（`mcp-read` 同一个读法；项目目录给了 ⇒ 连项目级 · 本地级一起看）。
/// 名字要进子命令的 argv ⇒ 以 `-` 开头的一律不认（不会被当成旗标）。
fn mcp_listed(agent: &str, project: Option<&std::path::Path>, server: &str) -> bool {
    if server.is_empty() || server.starts_with('-') {
        return false;
    }
    let look = crate::accounts::manage::mcp_share_exec::mcp_look(agent);
    crate::agents::mcp_read(agent, project, &look).is_some_and(|r| {
        crate::agents::mcp_held(&r.entries, &look.homes)
            .iter()
            .any(|h| h.entry.name == server)
    })
}

/// 推给发起它的那条连接的那一帧的小成品（`changed {login, key: 作业号, body}`；线上形住 cmd-wire）。
pub use cmd_wire::quota::SignInState;

/// 作业收了 ⇒ 那一份小成品（扩展特征：孤儿规则不许在 cmd-wire 的类型上加固有方法）。
pub trait SignInStateExt {
    fn of(e: &SignInEnd) -> SignInState;
}

impl SignInStateExt for SignInState {
    fn of(e: &SignInEnd) -> SignInState {
        match e {
            SignInEnd::Done => SignInState::Done,
            SignInEnd::Failed(s) => SignInState::Failed {
                said: s.said.clone(),
                detail: s.raw.clone(),
                again: LoginAgainHow::Restart,
            },
        }
    }
}

/// `login-finish`：把登录页上给的码交给那份作业，等那一家换完。
pub struct LoginFinish;

/// `login-finish` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct LoginFinishReq {
    pub(crate) job: String,
    /// 登录页上给的那一段（原样；不进日志、不回显）。
    pub(crate) pasted: String,
}

/// `login-finish` 的成品：登成了的那个目标（`login-start` 时交的那一份）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LoginDone {
    pub(crate) target: LoginStartReq,
}

typed::codes! {
    /// `login-finish` 可能回的码（每个码带 `data: {again}`，值写在各码那一行）。
    pub enum LoginFinishErr {
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
            FinishFail::Refused(s) => LoginFinishErr::Refused(
                {
                    let f = crate::wire::spec::fail("refused", s);
                    crate::common::contract::joined(f.said, f.raw)
                },
                LoginAgain::RESTART,
            ),
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
    async fn run(
        req: LoginFinishReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<LoginDone, LoginFinishErr> {
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

/// `login-out` · `login-status` 的入参：哪个号（家由后端照号表认，界面不交）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct LoginAccountReq {
    /// 报了家 ⇒ 要与号表里那个号的家对得上，对不上 ⇒ `bad_args`（不照它起子命令）；缺 ⇒ 照号表。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) agent: Option<String>,
    pub(crate) account: String,
}

typed::codes! {
    /// `login-out` · `login-status` 可能回的码。
    pub enum LoginOnceErr {
        /// 账号库里没有这个号。
        NotFound = "not_found",
        /// 这一家没声明这一条（不在 app 里退出 · 不在 app 里问状态）· 这个号是 API 号。
        Unsupported = "unsupported",
        /// 起不来 · 退非 0 · 读不出（原话里是它最后说的几行）。
        Failed = "failed",
    }
}

/// 那个号 ⇒ 它那一家（号表里那一格，[`account_at`]）＋ 起一次要交的（那一家的登录形 · 号的配置目录）。
fn once_of(req: &LoginAccountReq) -> Result<(&'static str, sign_in::OnceAsk), LoginOnceErr> {
    let at = account_at(
        crate::agents::REGISTRY,
        &library(),
        req.agent.as_deref(),
        &req.account,
    )
    .map_err(|r| r.for_once(crate::agents::REGISTRY))?;
    Ok((
        at.agent,
        sign_in::OnceAsk {
            face: at.face,
            dir: at.dir,
            cwd: crate::platform::paths::home_dir(),
            path: None,
        },
    ))
}

fn once_failed(s: crate::common::said::Said) -> LoginOnceErr {
    let f = crate::wire::spec::fail("failed", s);
    LoginOnceErr::Failed(crate::common::contract::joined(f.said, f.raw))
}

/// 退出 · 问状态：那一家没声明那一条 ⇒ 「不在 app 里登」那一句（照 `registry` 说那一家的叫法，同 [`AccountRefused::Unsupported`]）。
fn not_declared_among(registry: &[crate::agents::Adapter], agent: &str) -> LoginOnceErr {
    AccountRefused::Unsupported {
        agent: agent.to_string(),
    }
    .for_once(registry)
}

/// 起 MCP 登录：那一家没有登录形 ⇒ 同一句，照起登录那一套码表。
fn mcp_unsupported_among(registry: &[crate::agents::Adapter], agent: &str) -> LoginStartErr {
    AccountRefused::Unsupported {
        agent: agent.to_string(),
    }
    .for_start(registry)
}

/// `login-out`：退出一个号的登录（跑那一家声明的退出那一条；令牌由那一家自己删）。
pub struct LoginOut;

/// `login-out` 的成品：写好的那一句（「x 已退出登录」，两端照摆）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LoginSignedOut {
    pub(crate) said: String,
}

impl Command for LoginOut {
    const NAME: &'static str = "login-out";
    type Req = LoginAccountReq;
    type Reply = LoginSignedOut;
    type Err = LoginOnceErr;
}

impl crate::wire::typed::Handle for LoginOut {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: LoginAccountReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<LoginSignedOut, LoginOnceErr> {
        let (agent, ask) = once_of(&req)?;
        match sign_in::sign_out(&ask) {
            Ok(true) => Ok(LoginSignedOut {
                said: copy_text("beLogin.out.done", &[("account", &req.account)]),
            }),
            Ok(false) => Err(not_declared_among(crate::agents::REGISTRY, agent)),
            Err(s) => Err(once_failed(s)),
        }
    }
}

/// `login-status`：问一个号登没登（跑那一家声明的状态那一条、照它的读法读）。
pub struct LoginStatus;

/// `login-status` 的成品：写好的那一句（「x 已登录」·「x 未登录」，两端照摆；登没登已写进句子，两端不另判）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct LoginStatusRead {
    pub(crate) said: String,
}

impl Command for LoginStatus {
    const NAME: &'static str = "login-status";
    type Req = LoginAccountReq;
    type Reply = LoginStatusRead;
    type Err = LoginOnceErr;
}

impl crate::wire::typed::Handle for LoginStatus {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: LoginAccountReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<LoginStatusRead, LoginOnceErr> {
        let (agent, ask) = once_of(&req)?;
        match sign_in::status(&ask) {
            Ok(Some(signed_in)) => Ok(LoginStatusRead {
                said: if signed_in {
                    copy_text("beLogin.status.in", &[("account", &req.account)])
                } else {
                    copy_text("beLogin.status.out", &[("account", &req.account)])
                },
            }),
            Ok(None) => Err(not_declared_among(crate::agents::REGISTRY, agent)),
            Err(s) => Err(once_failed(s)),
        }
    }
}

/// `login-cancel`：收一份作业（杀那一家的登录子命令）。
pub struct LoginCancel;

/// `login-cancel` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct LoginCancelReq {
    pub(crate) job: String,
}

/// `login-cancel` 的成品：有没有这一份（没有 ⇒ 早收了，照样算撤了）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LoginCancelled {
    pub(crate) was: bool,
}

typed::codes! {
    /// `login-cancel` 自己没有别的码。
    pub enum LoginCancelErr {}
}

impl Command for LoginCancel {
    const NAME: &'static str = "login-cancel";
    type Req = LoginCancelReq;
    type Reply = LoginCancelled;
    type Err = LoginCancelErr;
}

impl crate::wire::typed::Handle for LoginCancel {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: LoginCancelReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<LoginCancelled, LoginCancelErr> {
        Ok(LoginCancelled {
            was: sign_in::cancel(&req.job),
        })
    }
}

/// 这台此刻会话外的需手动清单（[`manual_of`] 喂这台的账号库 · 换号那一侧记的 · MCP 读面；之后是 [`secret_files`] 这台那两份）；`sessions-needs` 那一截。
pub(crate) fn manual_here() -> Result<Vec<ManualItem>, String> {
    let mut items = logins_here()?;
    items.extend(secret_files(
        crate::accounts::upstream_select::file_face::machine_key_path().as_deref(),
        crate::platform::paths::home_dir()
            .map(|h| crate::relay::key_path(&h))
            .as_deref(),
    ));
    items.extend(own_files(
        crate::common::machine_prefs::path_now().as_deref(),
    ));
    Ok(items)
}

/// 这台自家状态文件（偏好）比此后端读得懂的还老 ⇒ 列那一项（读口当没有、照缺省用，这里照实说一句、落需手动）。不在 · 读得懂 ⇒ 不列；每问一次现读。
pub(crate) fn own_files(prefs: Option<&Path>) -> Vec<ManualItem> {
    let Some(path) = prefs else {
        return Vec::new();
    };
    let Some(o) = crate::common::machine_prefs::older_at(path) else {
        return Vec::new();
    };
    // 下次写时原文照字节留在旁边那一份（`own_state` 写口：`<名>.v<盘上那一版>`）。
    let mut keep = path.as_os_str().to_owned();
    keep.push(format!(".v{}", o.found));
    vec![ManualItem::OwnFile {
        said: copy_text(
            "beManual.list.prefsOlder",
            &[
                ("path", &path.display().to_string()),
                ("theirs", &o.found.to_string()),
                ("mine", &o.readable()),
            ],
        ),
        fix: None,
        why: Some(copy_text(
            "beManual.why.prefsOlder",
            &[("keep", &Path::new(&keep).display().to_string())],
        )),
    }]
}

/// 这台自家两份秘密文件（API key 凭据文件 · 中转钥匙文件）各该列的那一项：不在 · 只给本人 ⇒ 不列。判法只在 `creds_core::perm`；每问一次现量。
pub(crate) fn secret_files(creds: Option<&Path>, key: Option<&Path>) -> Vec<ManualItem> {
    use creds_core::perm::Verdict;
    [
        (creds, copy_text("credsPerm.notice.apikey", &[])),
        (key, copy_text("credsPerm.notice.relayKey", &[])),
    ]
    .into_iter()
    .filter_map(|(path, what)| {
        let path = path?;
        let (how, why) = match creds_core::perm::verdict_at(path)? {
            Verdict::OwnerOnly => return None,
            Verdict::TooWide { how, fix } => (how, Some(fix)),
            Verdict::Undetermined { why, .. } => (why, None),
        };
        Some(ManualItem::SecretFile {
            said: copy_text(
                "beManual.list.secretFile",
                &[
                    ("what", &what),
                    ("path", &path.display().to_string()),
                    ("how", &how),
                ],
            ),
            fix: None,
            why,
        })
    })
    .collect()
}

/// 需登录的号 · 需登录的 MCP（[`manual_of`] 喂这台的）。
fn logins_here() -> Result<Vec<ManualItem>, String> {
    let hop = crate::faces::rotation_face::Ctx::here().hop;
    let now = crate::common::time::now_secs();
    // 续令牌被拒的号照各家自己的号表问（那一家起会话那一面的路由名）。
    let relogin = |kind: &str| {
        crate::agents::launch_face_among(crate::agents::REGISTRY, kind)
            .map(|f| hop.needs_relogin(f.adapter_id, now))
            .unwrap_or_default()
    };
    logins_among(
        crate::agents::REGISTRY,
        crate::accounts::manage::library::library_unreadable_here(),
        relogin,
        library(),
        |kind| {
            // 口径同待办：用户级 ＋ 这台此刻活着的会话所在项目里的（判活住 history，由这里递进去）。
            let cwds = crate::agents::home_of_kind(kind)
                .map(|home| {
                    crate::history::live_table::now(&crate::agents::AgentHome { kind, home }).cwds()
                })
                .unwrap_or_default();
            crate::accounts::manage::mcp_share_exec::needs_login(kind, &cwds)
        },
    )
}

/// [`logins_here`] 的本体：注册表 · 账号库（几家混列那一份，[`library`]）· 各家要登录的 MCP 都是参数（判据拿替身家喂它）。
/// 问的是声明了账号面或 MCP 面的每一家：账号库里的号各照它那一家算（每一家声明了账号面的各列各的号）；
/// 要登录的 MCP 逐家各问一遍（只声明 MCP 面、没声明账号面的那一家也问）、并进那一家自己那一份。
pub(crate) fn logins_among(
    registry: &[crate::agents::Adapter],
    unreadable: Option<String>,
    relogin: impl Fn(&str) -> Vec<String>,
    library: crate::accounts::manage::library::AccountsList,
    needs: impl Fn(&'static str) -> Vec<crate::accounts::manage::mcp_share_exec::McpNeed>,
) -> Result<Vec<ManualItem>, String> {
    let mcp_kinds = crate::agents::mcp_kinds_among(registry);
    let kinds: Vec<&'static str> = registry
        .iter()
        .filter(|a| a.accounts.is_some() || mcp_kinds.contains(&a.kind))
        .map(|a| a.kind)
        .collect();
    let mut out = Vec::new();
    for agent in kinds {
        let mcp = crate::agents::mcp_kind_among(registry, agent).map(|k| (k, needs(k)));
        let mut list = library.clone();
        // 号是哪一家的照选号那一判（没写家的扫号单已归缺省那一家）；号表里写了认不出的家（`unknown_agent`）不进任何一家。
        let kind = cmd_wire::AgentKind::new(agent);
        list.accounts
            .retain(|r| crate::accounts::manage::library::row_of_family(&r.scanned, &kind));
        out.extend(manual_of(
            registry,
            agent,
            &list,
            unreadable.clone(),
            &relogin(agent),
            mcp.as_ref().map(|(k, n)| (*k, n.as_slice())),
        )?);
    }
    // 写了认不出的家的号不归任何一家：照实出一行（号表原名套登录拒的那一句，一个意思一把键）。
    out.extend(library.accounts.iter().filter_map(|r| {
        let agent = r.scanned.unknown_agent.clone()?;
        Some(ManualItem::AccountNoAgent {
            agent,
            account: r.scanned.name.clone(),
            said: copy_text("beLogin.account.noAgent", &[("account", &r.scanned.name)]),
            go: copy_text("beNeeds.manual.toSettings", &[]),
            fix: None,
        })
    }));
    Ok(out)
}

/// 会话外的需手动清单（需登录的号 · 需登录的 MCP，`sessions-needs` 里会外那一截）的纯构造器：账号清单成品（`accounts-list` 那一份）·
/// 续令牌被拒的号（`relogin`，换号那一侧记的）· 要登录的 MCP（哪一家的 · `mcp_share_exec::needs_login` 判好的那一串，项目级的带着项目）⇒ 清单。判据拿夹具喂它。
/// `unreadable` ＝ 账号库清单在、却读不懂的那一句（`accounts::manage::library::library_unreadable`）：给了 ⇒ 原样回 `Err`，不列
/// （说不出哪些号需登录，不回空清单：空清单看起来像「没事」）。
/// 认家（登不登得了 · 那一家的叫法）照交进来的 `registry`；号名本身的字（`_` · 账号 0 的短形）照 `name_words::account_text_of`，不归这一格。
pub(crate) fn manual_of(
    registry: &[crate::agents::Adapter],
    agent: &str,
    accounts: &crate::accounts::manage::library::AccountsList,
    unreadable: Option<String>,
    relogin: &[String],
    mcp: Option<(&str, &[crate::accounts::manage::mcp_share_exec::McpNeed])>,
) -> Result<Vec<ManualItem>, String> {
    if let Some(why) = unreadable {
        return Err(why);
    }
    let signs_in =
        crate::agents::accounts_face_among(registry, agent).is_some_and(|f| f.sign_in.is_some());
    let mut items: Vec<ManualItem> = accounts
        .accounts
        .iter()
        .map(|a| &a.scanned)
        .filter(|a| {
            signs_in
                && a.auth_kind == acct_core::AUTH_KIND_SUBSCRIPTION
                && a.exists
                && (!a.auth_ready || relogin.contains(&a.name))
        })
        .map(|a| {
            let who = crate::accounts::quota::name_words::account_text_of(agent, &a.name);
            ManualItem::AccountLogin {
                agent: agent.into(),
                account: a.name.clone(),
                // 盘上有登录、续令牌却被拒 ⇒ 要重新登录；盘上没有 ⇒ 要登录。
                said: if a.auth_ready {
                    copy_text("beManual.list.accountRelogin", &[("account", &who)])
                } else {
                    copy_text("beManual.list.accountLogin", &[("account", &who)])
                },
                fix: ManualFix {
                    command: "login-start".into(),
                    // 不带家：家由 `login-start` 照号表认（同一个号从哪里点进来都是同一个目标）。
                    args: LoginStartReq::Account {
                        agent: None,
                        account: a.name.clone(),
                    },
                },
            }
        })
        .collect();
    let (mcp_kind, entries) = match mcp {
        Some((k, m)) => (k, m),
        None => ("", &[][..]),
    };
    for e in entries {
        for account in e.login_in.iter().cloned() {
            let said = match &account {
                Some(a) => copy_text(
                    "beManual.list.mcpLogin",
                    &[
                        ("server", &e.name),
                        (
                            "account",
                            &crate::accounts::quota::name_words::account_text_of(mcp_kind, a),
                        ),
                    ],
                ),
                None => copy_text("beManual.list.mcpLoginBare", &[("server", &e.name)]),
            };
            let McpLoginFix { fix, why, .. } = mcp_fix(
                registry,
                mcp_kind,
                account.clone(),
                &e.name,
                e.project.as_deref(),
            );
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
    Ok(items)
}

/// 一个 MCP 服务器在一个号里要登录时怎么解（`sessions-needs` 会外那一项 · `mcp-read` 需登录那一格同一处写）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct McpLoginFix {
    /// 在哪个号里；没设账号的那一份 ⇒ `null`。
    pub(crate) account: Option<String>,
    /// 在 app 里登：`login-start {kind: mcp}`；那一家不在 app 里登 MCP ⇒ `null`。
    pub(crate) fix: Option<ManualFix>,
    /// `fix` 为 `null` 时必有：为什么没有解法（写好的那一句）；有 `fix` ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) why: Option<String>,
}

/// ★ `kind` 那一家在号 `account` 里登录 MCP 服务器 `server` 怎么解（唯一一处）。`project` ＝ 项目级 · 本地级那一条所在的项目目录。
pub(crate) fn mcp_fix(
    registry: &[crate::agents::Adapter],
    kind: &str,
    account: Option<String>,
    server: &str,
    project: Option<&str>,
) -> McpLoginFix {
    let signs_in =
        crate::agents::mcp_face_among(registry, kind).is_some_and(|f| f.sign_in.is_some());
    McpLoginFix {
        fix: signs_in.then(|| ManualFix {
            command: "login-start".into(),
            args: LoginStartReq::Mcp {
                agent: kind.into(),
                account: account.clone(),
                server: server.to_string(),
                project_dir: project.map(str::to_string),
            },
        }),
        why: (!signs_in).then(|| {
            copy_text(
                "beManual.list.mcpNoFix",
                &[("agent", &crate::agents::family_said_among(registry, kind))],
            )
        }),
        account,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/login_face_tests.rs"]
mod tests;
