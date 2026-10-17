//! 帧命令 `session-new`（起一个新会话）：入参 `{agent, cwd, account?, place, tmuxName?, command?, forkFrom?}` 换成 `session-open` 新起那一支（[`super::session_open`]）。
//!
//! 要动 tmux / 起 ccm / 写分支记录的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::{self as la, AccountAsk};
use super::session_batch::{carriers, Deps, NameBase, Standing};
use serde::{Deserialize, Serialize};

/// 回包的线上形住 `cmd-wire::session`（手机嵌的 Rust 核心按同一份类型解）。
pub use cmd_wire::session::{FirstSay, SessionNew, SessionNewOutcome};

/// 失败：同 `session-open` 那一形。
pub(crate) use super::session_open::Failed;

/// 放在哪：这台 tmux 里后台起（关终端不断）· 开一个新的终端窗口直接跑。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum Place {
    Tmux,
    Window,
}

/// 分叉自：源会话 ＋ 从哪条消息处。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ForkFrom {
    /// 源会话 id。
    pub(crate) sid: String,
    /// 从哪条消息处分叉。
    pub(crate) uuid: String,
}

/// 轮换：用哪一份策略。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct RotationPick {
    /// 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）；`null` ⇒ 这个会话不轮换（新会话用的那份不管它）。
    #[cfg_attr(test, ts(type = "string | null"))]
    pub(crate) strategy: Option<String>,
}

/// `session-new` 的入参（`deny_unknown_fields`：多送一格 ⇒ 拒）。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NewRequest {
    /// 哪一家（线上的 kind）。必填。
    pub(crate) agent: String,
    /// 工作目录（开头的 `~` 按这台的家目录读）。
    pub(crate) cwd: String,
    /// 缺席 ＝ 跟随（分叉跟随源会话上次的号；新起的 ⇒ 这台的默认号）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) account: Option<AccountAsk>,
    /// `tmux`（在这台 tmux 里后台起，关终端不断）· `window`（开一个新终端窗口直接跑）。
    pub(crate) place: Place,
    /// 用户改过的终端名；缺席 ⇒ 这台铸（分叉不收这一格：名字从源会话的终端名铸）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) tmux_name: Option<String>,
    /// 这一趟手填的启动命令（空 / 缺席 ⇒ 这台偏好里那一家的启动命令，没设过 ⇒ 那一家的默认启动器）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) command: Option<String>,
    /// 分叉自哪一条（只在这一步写分支记录）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_from: Option<ForkFrom>,
    /// 发请求的界面就在这台上（开窗那一形本机与远端渲法不同）。
    pub(crate) local: bool,
    /// 这一趟的票（界面每次点［新建］一张；期限到了再问一次带同一张 ⇒ 认出同一趟，不起第二个：[`super::session_new_ticket`]）。缺 ⇒ 不认。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) ticket: Option<String>,
    /// 轮换：缺 ＝ 用「新会话用」的那份策略（新会话本来就这样，不写）· `{strategy: id}` ＝ 起之前按定好的 sid 写成用那一份。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) rotation: Option<RotationPick>,
    /// 第一句（B4）：起好了之后由这台交进会话（宿主等它就绪 · 信任框过了再送）；只跟 `place: tmux`。缺 ⇒ 不交。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) prompt: Option<String>,
    /// 这一趟用的模型（压过 `models` 偏好表）；缺 ＝ 跟随。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) model: Option<String>,
    /// 起在哪一个权限档（那一家声明的档之一，`LaunchFace::permission`）；缺 ＝ 跟随那一家自己的默认。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) permission_mode: Option<String>,
}

/// `session-new` 的失败：码 ＋ 那一句 ＋ 按码定形的 `data`（[`SessionNewRefusal`]：哪一格 · 选不了号那一形）＋ 下层原话。
/// 码表封闭（[`SESSION_NEW_CODES`]）：下层交上来的码不在表里 ⇒ 测试档当场红。
#[derive(Debug)]
pub(crate) struct SessionNewErr(pub(crate) Failed);

/// `session-new` 会回的码（`bad_args` 打头）。
pub(crate) const SESSION_NEW_CODES: &[&str] = &[
    "bad_args",
    "unknown_agent",
    "bad_command",
    "no_dir",
    "account_unavailable",
    "place_unavailable",
    "bad_tmux_name",
    "tmux_taken",
    "unobservable",
    "fork_failed",
    "refused",
    "start_failed",
    "child_timed_out",
    "launch_pending",
    "no_strategy",
];

impl crate::wire::typed::Codes for SessionNewErr {
    const ALL: &'static [&'static str] = SESSION_NEW_CODES;
    fn into_fail(self) -> crate::wire::spec::Fail {
        let (code, message, data, raw) = self.0;
        debug_assert!(
            SESSION_NEW_CODES.contains(&code),
            "session-new 回了码表里没有的码 `{code}`"
        );
        // 那一句与原话本来就分开（`Failed` 第四格），照原样投（不经 `Fail::new` 再拆一次）。
        crate::wire::spec::Fail {
            code: code.to_string(),
            message,
            data,
            raw,
        }
    }
    /// 起不来的每一种都带 [`SessionNewRefusal`]（哪一格 · 选不了号那一形；整体的那几种 `field: null`）。
    const DATA: &'static [&'static str] = SESSION_NEW_CODES;
    type Data = crate::wire::typed::Each<SessionNewRefusal>;
    fn of_code(code: &str, said: String) -> Result<Self, String> {
        match SESSION_NEW_CODES.iter().find(|c| **c == code) {
            Some(c) => Ok(SessionNewErr((c, said, None, None))),
            None => Err(said),
        }
    }
}

/// 哪一格不行（`data.field`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum SessionNewField {
    Agent,
    Command,
    Cwd,
    Account,
    Place,
    TmuxName,
}

/// 失败信封的 `data`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionNewRefusal {
    /// 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
    pub field: Option<SessionNewField>,
    /// 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
    pub unavailable: Option<la::AccountUnavailable>,
}

/// 起之前定 sid 那几下（宿主给）：铸一个 sid（那一家认先定 sid 的新起都铸）· 选了策略（或「不轮换」，id 空）时起之前把那个会话写成用那一份（策略不在 ⇒ `no_strategy`）· 起不成撤掉。
pub(crate) struct PreRotation<'a> {
    pub(crate) sid: &'a dyn Fn() -> String,
    pub(crate) write: &'a dyn Fn(&str, &str, &str) -> Result<(), (&'static str, String)>,
    pub(crate) forget: &'a dyn Fn(&str),
}

/// 写分支记录：`(源 sid, 消息 uuid)` ⇒ 新 sid；不成 ⇒ `(码, 那一句)`。
pub(crate) type ForkWrite<'a> = &'a dyn Fn(&str, &str) -> Result<String, (&'static str, String)>;

/// `~` / `~/…` ⇒ 这台的家目录下（界面显示的目录常写成 `~/…`）。家说不出 ⇒ 原样。
pub(crate) fn expand_home(cwd: &str, home: Option<&std::path::Path>) -> String {
    let rest = match cwd.strip_prefix('~') {
        Some(r) if r.is_empty() || r.starts_with('/') => r,
        _ => return cwd.to_string(),
    };
    match home {
        Some(h) => format!("{}{rest}", h.to_string_lossy().trim_end_matches('/')),
        None => cwd.to_string(),
    }
}

/// `session-new` 的本体（入参已按类型解好）：同 `session-open` 新起那一支，回包换成旧那一形。
pub(crate) fn answer(
    req: NewRequest,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<SessionNew, Failed> {
    use super::session_open as open;
    let what = open::Open::New(open::NewOpen {
        agent: crate::agents::AgentKind::new(req.agent),
        cwd: req.cwd,
        tmux_name: req.tmux_name,
        command: req.command,
        fork: req.fork_from,
        prompt: req.prompt,
        model: req.model,
        permission_mode: req.permission_mode,
        rotation: req.rotation,
    });
    let run = match req.place {
        Place::Tmux => open::Run::Tmux,
        Place::Window => open::Run::Direct,
    };
    let o = open::answer(
        open::SessionOpenReq {
            what,
            account: req.account,
            run: Some(run),
            local: req.local,
            ticket: req.ticket,
            client: None,
        },
        open::Facts {
            resume: None,
            fork_tmux: None,
        },
        deps,
        fork,
        pre,
        home,
    )?;
    let started = o.landed == open::Landed::Started;
    Ok(SessionNew {
        outcome: if started {
            SessionNewOutcome::Started
        } else {
            SessionNewOutcome::Open
        },
        session: o.session,
        sid: o.sid,
        cmd: o.line.filter(|_| !started),
        account: o.account,
        agent: o.agent.map(|a| a.as_str().to_string()).unwrap_or_default(),
        cwd: o.cwd.unwrap_or_default(),
        prompt: o.prompt,
    })
}

/// 那一家起的时候收不收第一句（[`crate::agents::LaunchFace::first_prompt_arg`]）：不收 ⇒ 那句不行的话。
pub(crate) fn first_prompt_refused(face: &crate::agents::LaunchFace) -> Option<&'static str> {
    (!face.first_prompt_arg).then_some("this agent cannot take a first prompt at launch")
}

/// `session-new-dir` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionNewDirReq {
    /// 工作目录（开头的 `~` 按这台的家目录读）。
    pub(crate) cwd: String,
    /// 分叉出来的那一条：源会话 sid（铸名按它此刻所在的终端名派生 `<…>-fork-cc`）。
    #[serde(default, rename = "forkOf")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_of: Option<String>,
}

/// `session-new-dir` 的应答。
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirChecked {
    /// 这个目录在不在。
    pub(crate) exists: bool,
    /// 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
    pub(crate) tmux_name: Option<String>,
}

crate::wire::typed::codes! {
    /// `session-new-dir` 自己没有别的码（`forkOf` 不是会话 id ⇒ `bad_args`）。
    pub(crate) enum SessionNewDirErr {}
}

/// `session-new-dir` 的本体：目录在不在 · 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
pub(crate) fn dir_answer(
    req: &SessionNewDirReq,
    deps: &Deps,
    home: Option<&std::path::Path>,
) -> Result<DirChecked, SessionNewDirErr> {
    let fork_of = match req.fork_of.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) => Some(s),
        Some(_) => {
            return Err(SessionNewDirErr::BadArgs(
                crate::common::contract::malformed("`forkOf` must be a session id"),
            ))
        }
    };
    let cwd = expand_home(req.cwd.trim(), home);
    let exists = !cwd.is_empty() && (deps.host.is_dir)(&cwd);
    let tmux_name = match (deps.list)() {
        Ok(Some(rows)) => {
            let base = match fork_of {
                Some(sid) => NameBase::ForkOf(match carriers(&rows, sid) {
                    Standing::Running(e) => e.name.as_str(),
                    Standing::Ambiguous(es) => es.first().map_or(cwd.as_str(), |e| e.name.as_str()),
                    Standing::Idle(_) | Standing::None => cwd.as_str(),
                }),
                None => NameBase::Cwd(&cwd),
            };
            (deps.mint)(base).ok()
        }
        Ok(None) | Err(_) => None,
    };
    Ok(DirChecked { exists, tmux_name })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_new_tests.rs"]
mod tests;
