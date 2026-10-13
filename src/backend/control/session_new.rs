//! **起一个新会话 —— 全产品一个请求**（帧命令 `session-new`）：`{agent, cwd, account?, place, tmuxName?, command?, forkFrom?}`。
//!
//! 这台自己判：那一家认不认得 · 启动命令能不能拼 · 目录在不在 · 用哪个号（选不了 ⇒ 不起、带替代号）· 有没有 tmux ·
//! 终端名（给了就核不占用，没给就铸）· 分叉（只在这一步写分支记录，前面几格全过了才写）· 起。
//!
//! 回：`started`（在这台 tmux 里后台起好了，带会话名）· `open`（要开一个终端跑那一行：带 `cmd`，窗口由 monitor 开）。
//! 某一格不行 ⇒ 失败信封，`data.field` 说是哪一格（`agent` · `command` · `cwd` · `account` · `place` · `tmuxName`；整体的 ⇒ `null`），
//! 选不了号 ⇒ `data.unavailable` 是那一形（带替代号）。会话报没报到由界面等那台的会话流（不在这里等）。
//!
//! 要动 tmux / 起 ccm / 写分支记录的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::{self as la, AccountAsk, Settled};
use super::launch_render::{local, wire};
use super::session_batch::{carriers, Deps, NameBase, Standing};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 回包的线上形住 `cmd-wire::session`（手机嵌的 Rust 核心按同一份类型解）。
pub use cmd_wire::session::{FirstSay, SessionNew, SessionNewOutcome};
use std::collections::BTreeMap;

/// 失败：码 ＋ 那一句 ＋ `data`（[`Refusal`]）＋ 下层原话（不进那一句，进应答的复制详情；没有 ⇒ `None`）。
pub(crate) type Failed = (&'static str, String, Option<Value>, Option<String>);

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
    /// 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）。
    pub(crate) strategy: String,
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
    /// 启动命令（空 / 缺席 ⇒ 那一家的默认启动器）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) command: Option<String>,
    /// 分叉自哪一条（只在这一步写分支记录）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_from: Option<ForkFrom>,
    /// 这台的模型偏好表（号 → 模型，用户设置的原值）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, String>>"))]
    pub(crate) models: BTreeMap<String, String>,
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

impl crate::stream::inbound::typed::Codes for SessionNewErr {
    const ALL: &'static [&'static str] = SESSION_NEW_CODES;
    fn into_fail(self) -> crate::stream::inbound::spec::Fail {
        let (code, message, data, raw) = self.0;
        debug_assert!(
            SESSION_NEW_CODES.contains(&code),
            "session-new 回了码表里没有的码 `{code}`"
        );
        // 那一句与原话本来就分开（`Failed` 第四格），照原样投（不经 `Fail::new` 再拆一次）。
        crate::stream::inbound::spec::Fail {
            code: code.to_string(),
            message,
            data,
            raw,
        }
    }
    fn of_code(code: &str, said: String) -> Result<Self, String> {
        match SESSION_NEW_CODES.iter().find(|c| **c == code) {
            Some(c) => Ok(SessionNewErr((c, said, None, None))),
            None => Err(said),
        }
    }
}

/// 哪一格不行（`data.field`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionNewRefusal {
    /// 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
    pub field: Option<SessionNewField>,
    /// 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
    pub unavailable: Option<la::AccountUnavailable>,
}

/// 起之前定 sid 那几下（宿主给）：铸一个 sid（那一家认先定 sid 的新起都铸）· 选了策略时起之前把那个会话写成用那一份（策略不在 ⇒ `no_strategy`）· 起不成撤掉。
pub(crate) struct PreRotation<'a> {
    pub(crate) sid: &'a dyn Fn() -> String,
    pub(crate) write: &'a dyn Fn(&str, &str, &str) -> Result<(), (&'static str, String)>,
    pub(crate) forget: &'a dyn Fn(&str),
}

/// `rotation` 那一格：缺 ⇒ `None`（新会话用的那份）；`{strategy: id}` ⇒ 那一份；id 空 ⇒ `bad_args`（别的形状在解入参那一步就回了）。
fn strategy_of(v: Option<&RotationPick>) -> Result<Option<String>, Failed> {
    match v {
        None => Ok(None),
        Some(p) if !p.strategy.is_empty() => Ok(Some(p.strategy.clone())),
        Some(_) => Err((
            "bad_args",
            crate::common::contract::malformed("`rotation` must be {\"strategy\": id}"),
            None,
            None,
        )),
    }
}

/// 写分支记录：`(源 sid, 消息 uuid)` ⇒ 新 sid；不成 ⇒ `(码, 那一句)`。
pub(crate) type ForkWrite<'a> = &'a dyn Fn(&str, &str) -> Result<String, (&'static str, String)>;

fn fail(code: &'static str, said: String, field: Option<SessionNewField>) -> Failed {
    let data = SessionNewRefusal {
        field,
        unavailable: None,
    };
    (code, said, serde_json::to_value(data).ok(), None)
}

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

/// `session-new` 的本体（入参已按类型解好）。`home` ＝ 这台的家目录（展开 `~`）。
pub(crate) fn answer(
    req: NewRequest,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<SessionNew, Failed> {
    let ticket = req.ticket.clone();
    if ticket
        .as_deref()
        .is_some_and(|t| !super::session_new_ticket::ticket_ok(t))
    {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`ticket` must be 1..=64 chars of [A-Za-z0-9-]"),
            None,
            None,
        ));
    }
    super::session_new_ticket::TICKETS.with(
        ticket.as_deref(),
        || {
            fail(
                "launch_pending",
                copy_core::copy_text("beSessionNew.ticket.pending", &[]),
                None,
            )
        },
        || start(req, deps, fork, pre, home),
    )
}

/// 起（票那一层之后）。
fn start(
    req: NewRequest,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<SessionNew, Failed> {
    // 没说是哪一家就不起（不落默认那一家：默认是界面读画像时的事）。
    if req.agent.trim().is_empty() {
        return Err(fail(
            "unknown_agent",
            copy_core::copy_text("beSessionNew.agent.missing", &[]),
            Some(SessionNewField::Agent),
        ));
    }
    let (kind, face) = crate::agents::pick_kind(Some(&req.agent))
        .map_err(|m| fail("unknown_agent", m, Some(SessionNewField::Agent)))?;
    let rule = strategy_of(req.rotation.as_ref())?;
    // 新起的要先定 sid 才能起之前写好打开哪一份：那一家不认 ⇒ 不收（分叉的 sid 由分支记录定，不用它）。
    if rule.is_some() && req.fork_from.is_none() && face.preset_sid.is_none() {
        return Err((
            "bad_args",
            crate::common::contract::malformed(
                "this agent cannot take a rotation strategy at launch",
            ),
            None,
            None,
        ));
    }
    let malformed = |m: &str| -> Failed {
        (
            "bad_args",
            crate::common::contract::malformed(m),
            None,
            None,
        )
    };
    // 第一句：只跟 tmux 那一形（开窗那一形的终端在界面那一侧）；空的不收；那一家声明不认起的时候带 ⇒ 不收。
    let prompt = match req.prompt.as_deref().map(str::trim) {
        None => None,
        Some("") => return Err(malformed("`prompt` must not be empty")),
        Some(_) if req.place != Place::Tmux => {
            return Err(malformed("`prompt` goes only with `place: tmux`"))
        }
        Some(_) if first_prompt_refused(&face).is_some() => {
            return Err(malformed(first_prompt_refused(&face).unwrap_or_default()))
        }
        Some(_) => req.prompt.clone(),
    };
    // 模型：过全仓那一条模型名判（与偏好表那一路同一条）。
    if req
        .model
        .as_deref()
        .is_some_and(|m| !shell_quote_core::model_name_ok(m))
    {
        return Err(malformed("`model` is not a model name"));
    }
    // 权限档：那一家声明的旗标与可选的档（没有 ⇒ 不收）。
    let permission_args: Vec<String> = match req.permission_mode.as_deref() {
        None => Vec::new(),
        Some(m) => match face.permission {
            Some(p) if p.modes.iter().any(|c| c.value == m) => {
                vec![p.flag.to_string(), m.to_string()]
            }
            Some(_) => return Err(malformed("`permissionMode` is not one this agent offers")),
            None => return Err(malformed("this agent has no permission modes")),
        },
    };
    // 启动命令：拼进那一行之前过全仓那一张命令片段白名单。
    let command = req
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    if let Some(c) = command.and_then(shell_quote_core::launcher_refused_char) {
        return Err(fail(
            "bad_command",
            copy_core::copy_text(
                "rsHistory.launcher.badChars",
                &[
                    ("launcher", &format!("{:?}", command.unwrap_or_default())),
                    ("c", &format!("{c:?}")),
                ],
            ),
            Some(SessionNewField::Command),
        ));
    }
    let launcher = command.unwrap_or(face.default_launcher).to_string();
    let cwd = expand_home(req.cwd.trim(), home);
    if cwd.is_empty() || !(deps.local_facts.is_dir)(&cwd) {
        return Err(fail(
            "no_dir",
            copy_core::copy_text("beSessionNew.cwd.missing", &[("cwd", req.cwd.trim())]),
            Some(SessionNewField::Cwd),
        ));
    }
    if let Some(f) = &req.fork_from {
        for id in [&f.sid, &f.uuid] {
            if !shell_quote_core::session_id_ok(id) {
                return Err(fail(
                    "bad_args",
                    crate::common::contract::malformed(
                        "`forkFrom` needs a session id and a message id",
                    ),
                    None,
                ));
            }
        }
        if req.tmux_name.is_some() {
            return Err(fail(
                "bad_args",
                crate::common::contract::malformed(
                    "`tmuxName` does not go with `forkFrom` (the fork mints its own)",
                ),
                None,
            ));
        }
    }
    // 号：跟随时分叉跟源会话上次那个号。
    let asked = req.account.clone().unwrap_or(AccountAsk::Follow);
    let source = req.fork_from.as_ref().map(|f| f.sid.as_str());
    let account = la::settle(&asked, source, &req.models, deps.accounts).map_err(|u| {
        let said = la::unavailable_said(&u);
        let data = SessionNewRefusal {
            field: Some(SessionNewField::Account),
            unavailable: Some(u),
        };
        (
            "account_unavailable",
            said,
            serde_json::to_value(data).ok(),
            None,
        )
    })?;
    // 终端名（只 tmux 那一形）：给了 ⇒ 核写法与不占用；没给 ⇒ 这台铸（分叉从源会话此刻的终端名铸）。
    let name = match req.place {
        Place::Window => None,
        Place::Tmux => Some(tmux_name(&req, &cwd, deps)?),
    };
    // 前面几格全过了才写分支记录（取消 / 某格不行都不留东西）。
    let forked = match &req.fork_from {
        Some(f) => Some(fork(&f.sid, &f.uuid).map_err(|(c, m)| fail(c, m, None))?),
        None => None,
    };
    // 新起的会话：那一家认先定 sid ⇒ 这台先定（交给那一家的那一串里带 `--session-id <sid>`），回包就说得出 sid。
    let preset = match (&forked, face.preset_sid) {
        (None, Some(_)) => Some((pre.sid)()),
        _ => None,
    };
    let mut preset_args: Vec<String> = match (&preset, face.preset_sid) {
        (Some(s), Some(flag)) => vec![flag.to_string(), s.clone()],
        _ => Vec::new(),
    };
    preset_args.extend(permission_args);
    // 起的时候就交的第一句：交给那一家的那一半末尾、`--` 之后（开头是 `-` 也不会被当成旗标）。
    if let Some(text) = &prompt {
        preset_args.extend([super::ccm::argv::flag::END.to_string(), text.clone()]);
    }
    let resumed = forked.clone();
    let sid = forked.or_else(|| preset.clone());
    // 起之前写好打开哪一份：会话一报到就已经是它（界面不补写）。
    let wrote = match (&rule, &sid) {
        (Some(r), Some(s)) => {
            (pre.write)(s, kind, r).map_err(|(c, m)| fail(c, m, None))?;
            Some(s.clone())
        }
        _ => None,
    };
    let undo = |e: Failed| -> Failed {
        if let Some(s) = &wrote {
            (pre.forget)(s);
        }
        e
    };
    let launched = match &account {
        Settled::Account(a) => Some(a.clone()),
        _ => None,
    };
    let reply = |outcome, session, cmd| SessionNew {
        outcome,
        session,
        sid: sid.clone(),
        cmd,
        account: launched.clone(),
        agent: kind.to_string(),
        cwd: cwd.clone(),
        prompt: None,
    };
    // 起之前：工作目录标成要用的那个号信任过（三种起法同一下）。
    super::session_batch::pretrust(&account, &cwd, deps);
    let launch = || -> Result<SessionNew, Failed> {
        Ok(match name {
            Some(name) => {
                let argv = own_entry(deps).and_then(|entry| {
                    wire::ccm_launch_argv(
                        &wire_req(
                            kind,
                            &launcher,
                            face.default_launcher,
                            resumed.as_deref(),
                            preset.as_deref(),
                            &preset_args,
                            Some(&name),
                            &cwd,
                            &asked,
                            req.model.as_deref(),
                            &req.models,
                        ),
                        &account,
                        deps.caps,
                        &entry,
                        true,
                    )
                });
                let argv = argv.map_err(|m| fail("refused", m, None))?;
                match (deps.run_ccm)(&argv) {
                    Ok((0, _, _)) => reply(SessionNewOutcome::Started, Some(name), None),
                    // ccm 的退出码 3 = 会话名被占（它响亮失败，不接回别人的会话）。
                    Ok((3, _, _)) => {
                        return Err(fail(
                            "tmux_taken",
                            copy_core::copy_text("beSessionNew.tmux.taken", &[("name", &name)]),
                            Some(SessionNewField::TmuxName),
                        ))
                    }
                    // 别的退出码：那一句只说原因；退出码与 ccm 的 stderr 进复制详情（C-W18：句子里不接原话）。
                    Ok((rc, _, err)) => {
                        let (code, said, data, _) = fail(
                            "start_failed",
                            copy_core::copy_text("beSessionNew.start.failed", &[]),
                            None,
                        );
                        return Err((code, said, data, Some(format!("exit {rc}\n{}", err.trim()))));
                    }
                    Err((c, m)) => return Err(fail(c, m, None)),
                }
            }
            None if req.local => {
                let lreq = local::LocalLaunchRequest {
                    agent: kind.to_string(),
                    action: match &resumed {
                        Some(s) => local::LocalAction::Resume { sid: s.clone() },
                        None => local::LocalAction::New,
                    },
                    cwd: Some(cwd.clone()),
                    launcher: Some(launcher.clone()).filter(|l| l != face.default_launcher),
                    account: Some(asked.clone()),
                    tmux_name: None,
                    default_launcher: face.default_launcher.to_string(),
                    preset_args: preset_args.clone(),
                    model: req.model.clone(),
                };
                let cmd = local::plan(&lreq, &account, &deps.local_facts)
                    .map_err(|m| fail("refused", m, None))?;
                reply(SessionNewOutcome::Open, None, Some(cmd))
            }
            None => {
                let cmd = own_entry(deps).and_then(|entry| {
                    wire::render_ccm_launch_with(
                        &wire_req(
                            kind,
                            &launcher,
                            face.default_launcher,
                            resumed.as_deref(),
                            preset.as_deref(),
                            &preset_args,
                            None,
                            &cwd,
                            &asked,
                            req.model.as_deref(),
                            &req.models,
                        ),
                        &account,
                        deps.caps,
                        &entry,
                    )
                });
                reply(
                    SessionNewOutcome::Open,
                    None,
                    Some(cmd.map_err(|m| fail("refused", m, None))?),
                )
            }
        })
    };
    let mut out = launch().map_err(undo)?;
    // 第一句起的时候已经交给那一家了 ⇒ 交到了。
    if prompt.is_some() && out.outcome == SessionNewOutcome::Started {
        out.prompt = Some(FirstSay {
            result: "delivered".into(),
            why: None,
            said: None,
        });
    }
    Ok(out)
}

/// 那一家起的时候收不收第一句（[`crate::agents::LaunchFace::first_prompt_arg`]）：不收 ⇒ 那句不行的话。
pub(crate) fn first_prompt_refused(face: &crate::agents::LaunchFace) -> Option<&'static str> {
    (!face.first_prompt_arg).then_some("this agent cannot take a first prompt at launch")
}

/// tmux 那一形的会话名。这台没 tmux ⇒ `place` 那一格不行；看不见名单 ⇒ 整体不行（不拿空名单铸：那是不避让）。
fn tmux_name(req: &NewRequest, cwd: &str, deps: &Deps) -> Result<String, Failed> {
    let rows = match super::session_batch::listed(deps) {
        Ok(Some(rows)) => rows,
        Ok(None) => {
            return Err(fail(
                "place_unavailable",
                copy_core::copy_text("beSessionNew.place.noTmux", &[]),
                Some(SessionNewField::Place),
            ))
        }
        Err((code, m)) => return Err(fail(code, m, None)),
    };
    let taken = |n: &str| rows.iter().any(|r| r.name == n);
    if let Some(n) = req
        .tmux_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        if crate::control::gate_rules::new_tmux_name_issue(n).is_some() {
            return Err(fail(
                "bad_tmux_name",
                copy_core::copy_text("beSessionNew.tmux.badName", &[("name", n)]),
                Some(SessionNewField::TmuxName),
            ));
        }
        if taken(n) {
            return Err(fail(
                "tmux_taken",
                copy_core::copy_text("beSessionNew.tmux.taken", &[("name", n)]),
                Some(SessionNewField::TmuxName),
            ));
        }
        return Ok(n.to_string());
    }
    let base = match &req.fork_from {
        // 源会话此刻所在终端的名字（在跑的那一个；命中多个取第一个）；不在任何终端里 ⇒ 这一项的工作目录。
        Some(f) => NameBase::ForkOf(match carriers(&rows, &f.sid) {
            Standing::Running(e) => e.name.as_str(),
            Standing::Ambiguous(es) => es.first().map_or(cwd, |e| e.name.as_str()),
            Standing::Idle(_) | Standing::None => cwd,
        }),
        None => NameBase::Cwd(cwd),
    };
    let name = (deps.mint)(base).map_err(|(c, m)| fail(c, m, None))?;
    if taken(&name) {
        return Err(fail(
            "tmux_taken",
            copy_core::copy_text("beSessionNew.tmux.taken", &[("name", &name)]),
            Some(SessionNewField::TmuxName),
        ));
    }
    Ok(name)
}

/// 那一行的上线入参（与界面那一行同一份映射、同一个渲染器）：分叉 ⇒ resume 新 sid；新起 ⇒ new。
#[allow(clippy::too_many_arguments)]
fn wire_req(
    kind: &str,
    launcher: &str,
    default_launcher: &str,
    sid: Option<&str>,
    preset: Option<&str>,
    preset_args: &[String],
    tmux: Option<&str>,
    cwd: &str,
    account: &AccountAsk,
    model: Option<&str>,
    models: &BTreeMap<String, String>,
) -> wire::CliRenderRequest {
    wire::CliRenderRequest {
        agent: kind.to_string(),
        action: match sid {
            Some(s) => wire::WireAction::Resume { sid: s.to_string() },
            None => wire::WireAction::New,
        },
        container: match tmux {
            Some(name) => wire::WireContainer::Tmux {
                name: name.to_string(),
                send_into: false,
            },
            None => wire::WireContainer::None,
        },
        cwd: Some(cwd.to_string()),
        account: account.clone(),
        // 身份标记打在建出来的 tmux 会话上（resume · 先定好 sid 的新起才说得出 sid）。
        ccm_sid: sid
            .or(preset)
            .filter(|_| tmux.is_some())
            .map(str::to_string),
        model: model.map(str::to_string),
        models: models.clone(),
        launcher: launcher.to_string(),
        default_launcher: default_launcher.to_string(),
        preset_args: preset_args.to_vec(),
    }
}

fn own_entry(deps: &Deps) -> Result<String, String> {
    (deps.local_facts.entry)()
        .ok_or_else(|| copy_core::copy_text("beLaunchRender.entry.noHome", &[]))
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

crate::stream::inbound::typed::codes! {
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
    let exists = !cwd.is_empty() && (deps.local_facts.is_dir)(&cwd);
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
