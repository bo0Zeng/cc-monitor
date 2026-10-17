//! **起会话 · 恢复 · 接回 · 开 shell —— 全产品一个请求**（帧命令 `session-open`）：意图进去，这台判完、做完，回「落在哪 ＋ 要开终端就跑这一行」。
//!
//! 这台自己判：用哪个号（选不了 ⇒ 不起、带替代号）· 放在哪（没说 ⇒ [`default_run`]；说了 tmux 而这台没有 ⇒ 拒，不改道）·
//! 记录在不在 · 已有进程在写就不起（直路一样）· 在跑不另起 · 空 tmux 就地起 · 都不是就铸名新开。界面与手机只开窗口、等会话出现。
//!
//! 起会话的本体只在这一个模块里（[`start_one`] 一族模块外不可见）：换号重启经 [`relaunch_in`]、`session-say` 恢复那一支经 [`resume_saying`]
//! 进来，判定与渲那一行都是同一份。
//! 要动 tmux / 读记录 / 起 ccm 的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::{self as la, AccountAsk, LaunchedAccount, Settled};
use super::launch_render::{self as lr, Act, Here, Launch};
use super::session_batch::{
    carriers, listed, pids_said, pretrust, CmdErr, Deps, NameBase, Standing, TmuxEntry,
};
pub(crate) mod new;
use crate::platform::child::{Child, Deadline};
pub use cmd_wire::session::{ArriveAsk, ArriveBy, FirstSay, Landed, Opened, Run};
pub(crate) use new::{
    dir_answer, DirChecked, ForkFrom, ForkWrite, OpenDirErr, OpenDirReq, PreRotation,
};
use new::{
    expand_home, first_prompt_refused, OpenField as Field, OpenRefusal as Refusal, RotationPick,
};
use serde::Deserialize;
use serde_json::Value;

/// 失败：码 ＋ 那一句 ＋ `data`（[`Refusal`]：哪一格 · 选不了号那一形）＋ 下层原话（不进那一句，进复制详情；没有 ⇒ `None`）。
pub(crate) type Failed = (&'static str, String, Option<Value>, Option<String>);

/// `session-open` 的入参（`deny_unknown_fields`：多送一格 ⇒ 拒）。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionOpenReq {
    /// 起新的 · 恢复某条 · 接回 · 开一个 shell。
    pub(crate) what: Open,
    /// 缺 ＝ 跟随：恢复 ⇒ 这条会话上次的号；新起 ⇒ 这台的默认号；分叉 ⇒ 源会话的号。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) account: Option<AccountAsk>,
    /// 放在哪；缺 ＝ 这台定（新起：有 tmux 就 tmux、分叉照源会话那一种；恢复：照这台的偏好）。接回不收。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) run: Option<Run>,
    /// 开窗的那台就是这台（本机 Windows 渲 PowerShell；本机直路不带工作目录）。
    pub(crate) local: bool,
    /// 这一趟的票（每次点一下一张；期限到了带同一张再问 ⇒ 认出同一趟，不起第二个）。缺 ⇒ 不认。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) ticket: Option<String>,
    /// 自报的前端（就地键入过身份门，同 `kill`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// 意图。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        rename = "OpenWhat",
        export_to = "../../frontend/ui/generated/"
    )
)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Open {
    /// 起一个新会话（可分叉、可带第一句）。
    New(NewOpen),
    /// 恢复某条：哪一家 · 目录 · 模型 · 权限档这台从记录里读。
    Resume { sid: String },
    /// 接回 / 看一眼，不起 agent：按会话（找挂着它的 tmux 会话）或按 tmux 会话名，两格给一格。
    Look {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        sid: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        session: Option<String>,
    },
    /// 开一个 shell，不起 agent：给了 `session` ⇒ 这台 tmux 里那个会话（不在就在 `cwd` 里建一个空的）；
    /// 只给 `cwd` ⇒ 这台 shell 里进那个目录那一行。`cwd` 只在建的那一刻生效（已在的会话不往里敲 `cd`）。
    Shell {
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        session: Option<String>,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        cwd: Option<String>,
    },
}

/// 新起那一形的几格。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NewOpen {
    /// 哪一家（线上的 kind）。必填。
    pub(crate) agent: crate::agents::AgentKind,
    /// 工作目录（开头的 `~` 按这台的家目录读）。
    pub(crate) cwd: String,
    /// 用户改过的终端名（只跟 tmux）；缺 ⇒ 这台铸（分叉不收：名字从源会话的终端名铸）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) tmux_name: Option<String>,
    /// 这一趟手填的启动命令（空 / 缺 ⇒ 这台偏好里那一家的启动命令）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) command: Option<String>,
    /// 分叉自哪一条（只在这一步写分支记录）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork: Option<ForkFrom>,
    /// 第一句：起的时候交给那一家；只跟 tmux。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) prompt: Option<String>,
    /// 这一趟用的模型（压过偏好表）；缺 ＝ 跟随。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) model: Option<String>,
    /// 起在哪一个权限档（那一家声明的档之一）；缺 ＝ 这台偏好里那一家的那一档，没设 ⇒ 那一家自己的默认。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) permission_mode: Option<String>,
    /// 轮换：缺 ＝ 用「新会话用」的那份策略 · `{strategy: id}` ＝ 起之前按定好的 sid 写成用那一份。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) rotation: Option<RotationPick>,
}

/// `session-open` 会回的码（`bad_args` 打头）。
pub(crate) const OPEN_CODES: &[&str] = &[
    "bad_args",
    "unknown_agent",
    "bad_command",
    "no_dir",
    "account_unavailable",
    "place_unavailable",
    "bad_tmux_name",
    "tmux_taken",
    "record_gone",
    "already_live",
    "not_in_tmux",
    "wrong_owner",
    "unobservable",
    "fork_failed",
    "refused",
    "start_failed",
    "child_timed_out",
    "launch_pending",
    "no_strategy",
];

/// `session-open` 的失败（码表封闭：下层交上来的码不在表里 ⇒ 测试档当场红）。
#[derive(Debug)]
pub(crate) struct SessionOpenErr(pub(crate) Failed);

impl crate::wire::typed::Codes for SessionOpenErr {
    const ALL: &'static [&'static str] = OPEN_CODES;
    fn into_fail(self) -> crate::wire::spec::Fail {
        let (code, message, data, raw) = self.0;
        debug_assert!(
            OPEN_CODES.contains(&code),
            "session-open 回了码表里没有的码 `{code}`"
        );
        crate::wire::spec::Fail {
            code: code.to_string(),
            message,
            data,
            raw,
        }
    }
    /// 每一种都带 [`Refusal`]（哪一格 · 选不了号那一形；整体的 `field: null`）。
    const DATA: &'static [&'static str] = OPEN_CODES;
    type Data = crate::wire::typed::Each<Refusal>;
    fn of_code(code: &str, said: String) -> Result<Self, String> {
        match OPEN_CODES.iter().find(|c| **c == code) {
            Some(c) => Ok(SessionOpenErr((c, said, None, None))),
            None => Err(said),
        }
    }
}

pub(crate) fn fail(code: &'static str, said: String, field: Option<Field>) -> Failed {
    let data = Refusal {
        field,
        unavailable: None,
    };
    (code, said, serde_json::to_value(data).ok(), None)
}

fn malformed(m: &str) -> Failed {
    fail("bad_args", crate::common::contract::malformed(m), None)
}

/// 下层（列名单 · 铸名 · 起 ccm · 就地键入）交上来的 `(码, 原话)` ⇒ 码表里的那一个：认得的照码；别的收成 `start_failed`，原话进复制详情。
fn lower((code, said): (&str, String)) -> Failed {
    match OPEN_CODES.iter().find(|c| **c == code) {
        Some(c) => fail(c, said, None),
        None => {
            let (c, s, d, _) = fail(
                "start_failed",
                copy_core::copy_text("beSessionNew.start.failed", &[]),
                None,
            );
            (c, s, d, Some(format!("{code}: {said}")))
        }
    }
}

/// 起会话的意图（只看「放在哪」那一维）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    /// 新起；`fork_tmux` ＝ 分叉的源会话此刻在不在 tmux 里（不是分叉 / 说不出 ⇒ `None`）。
    New {
        fork_tmux: Option<bool>,
    },
    Resume,
}

/// **默认放在哪**（`run` 缺时照它；`sessions-where` · 起新会话框要画勾的那两处读它）。两条规则住这一处：
/// 新起 ⇒ 有 tmux 就 tmux（分叉照源会话那一种）；恢复 ⇒ 照这台的偏好「恢复时进 tmux」。这台没 tmux ⇒ 一律直路。
pub(crate) fn default_run(intent: Intent, resume_in_tmux: bool, tmux: bool) -> Run {
    let want = match intent {
        Intent::New { fork_tmux } => fork_tmux.unwrap_or(true),
        Intent::Resume => resume_in_tmux,
    };
    if want && tmux {
        Run::Tmux
    } else {
        Run::Direct
    }
}

/// 一个要起的会话（恢复那一形，已过形状关）。
pub(crate) struct Item {
    pub(crate) sid: String,
    pub(crate) cwd: String,
    pub(crate) account: AccountAsk,
    /// 起的时候就交的第一句（`session-say` 终端没了、恢复再送那一项）。
    pub(crate) say: Option<String>,
    /// 这一趟用的模型（会话记录里最后那一个）；缺 ＝ 跟随偏好表。
    pub(crate) model: Option<String>,
    /// 起在哪一个权限档（会话记录里最后那一档）；缺 ＝ 跟随那一家自己的默认。
    pub(crate) permission_mode: Option<String>,
}

/// 恢复要从记录里读的那几样（宿主读，[`answer`] 只收结果）。
pub(crate) struct ResumeOf {
    pub(crate) agent: String,
    pub(crate) cwd: String,
    pub(crate) model: Option<String>,
    pub(crate) permission_mode: Option<String>,
}

/// 宿主交进来的事实：恢复那一条读出来的记录（`None` ＝ 读不到 ⇒ `record_gone`）· 分叉源会话此刻在不在 tmux 里 ·
/// 起之前这台报到的序号（回包的 `arrival.since`）。
pub(crate) struct Facts {
    pub(crate) resume: Option<ResumeOf>,
    pub(crate) fork_tmux: Option<bool>,
    pub(crate) since: u64,
}

/// 那一家的几样（启动命令 · 模型表 · 起会话的说法）。
struct Batch {
    agent: String,
    launcher: String,
    default_launcher: String,
    models: std::collections::BTreeMap<String, String>,
    face: Option<crate::agents::LaunchFace>,
}

impl Batch {
    fn of(agent: &str, prefs: &crate::control::machine_prefs::Prefs) -> Batch {
        let (launcher, default_launcher) = Launch::launchers(agent, prefs);
        Batch {
            agent: agent.to_string(),
            launcher,
            default_launcher,
            models: prefs.models.clone(),
            face: crate::agents::launch_face_among(crate::agents::REGISTRY, agent),
        }
    }
    fn takes_first_say(&self) -> bool {
        self.face.is_some_and(|f| f.first_prompt_arg)
    }
}

fn here_of(local: bool, deps: &Deps) -> Here {
    if local {
        Here::Local {
            windows: deps.host.windows,
        }
    } else {
        Here::Remote
    }
}

/// `session-open` 的本体（入参已按类型解好，恢复那一条的记录宿主已读）。`home` ＝ 这台的家目录（展开 `~`）。
pub(crate) fn answer(
    req: SessionOpenReq,
    facts: Facts,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<Opened, Failed> {
    let ticket = req.ticket.clone();
    if ticket
        .as_deref()
        .is_some_and(|t| !super::session_new_ticket::ticket_ok(t))
    {
        return Err(malformed("`ticket` must be 1..=64 chars of [A-Za-z0-9-]"));
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
        || {
            let since = facts.since;
            dispatch(req, facts, deps, fork, pre, home).map(|o| with_arrival(o, since))
        },
    )
}

/// 起了一家 ⇒ 带上等它报到那一问：有 sid（恢复 · 先定好的新起）按 sid，否则按目录。在跑 · 只剩空 shell · 没起 agent 的 ⇒ 不等。
pub(crate) fn with_arrival(o: Opened, since: u64) -> Opened {
    let starts =
        matches!(o.landed, Landed::Started | Landed::InPlace | Landed::Window) && o.agent.is_some();
    let by = match (&o.sid, &o.cwd) {
        (Some(sid), _) => Some(ArriveBy::Sid { sid: sid.clone() }),
        (None, Some(cwd)) => Some(ArriveBy::Cwd { cwd: cwd.clone() }),
        (None, None) => None,
    };
    let arrival = by.filter(|_| starts).map(|by| ArriveAsk {
        by,
        since,
        session: o.session.clone(),
    });
    Opened { arrival, ..o }
}

fn dispatch(
    req: SessionOpenReq,
    facts: Facts,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<Opened, Failed> {
    let here = here_of(req.local, deps);
    let account = req.account.clone().unwrap_or(AccountAsk::Follow);
    match req.what {
        Open::New(n) => start_new(
            n,
            &account,
            req.run,
            here,
            facts.fork_tmux,
            deps,
            fork,
            pre,
            home,
        ),
        Open::Resume { sid } => {
            if !shell_quote_core::session_id_ok(&sid) {
                return Err(malformed("`sid` must be a session id"));
            }
            let Some(r) = facts.resume else {
                return Err(fail(
                    "record_gone",
                    copy_core::copy_text("beSessionOpen.record.gone", &[]),
                    None,
                ));
            };
            let it = Item {
                sid,
                cwd: r.cwd,
                account,
                say: None,
                model: r.model,
                permission_mode: r.permission_mode,
            };
            let run = req.run.map_or_else(|| resume_default(deps), Ok)?;
            start_one(&it, &Batch::of(&r.agent, deps.prefs), run, here, deps)
        }
        Open::Look { sid, session } => {
            if req.run.is_some() || req.account.is_some() {
                return Err(malformed("`look` takes neither `run` nor `account`"));
            }
            look(sid.as_deref(), session.as_deref(), here, deps)
        }
        Open::Shell { session, cwd } => {
            if req.run.is_some() || req.account.is_some() {
                return Err(malformed("`shell` takes neither `run` nor `account`"));
            }
            shell(session.as_deref(), cwd.as_deref(), here, deps, home)
        }
    }
}

/// 恢复没说放在哪 ⇒ 照这台的偏好 × 有没有 tmux。
fn resume_default(deps: &Deps) -> Result<Run, Failed> {
    let rows = listed(deps).map_err(lower)?;
    Ok(default_run(
        Intent::Resume,
        deps.prefs.resume_in_tmux,
        rows.is_some(),
    ))
}

/// 这台的名单；没 tmux ⇒ `place_unavailable`（给了 tmux 不悄悄改直路）。
fn tmux_rows(deps: &Deps) -> Result<Vec<TmuxEntry>, Failed> {
    match listed(deps).map_err(lower)? {
        Some(rows) => Ok(rows),
        None => Err(fail(
            "place_unavailable",
            copy_core::copy_text("beSessionNew.place.noTmux", &[]),
            Some(Field::Place),
        )),
    }
}

/// 接进 `name` 那个 tmux 会话那一行。
fn attach_line(name: &str, here: Here, deps: &Deps) -> Result<String, Failed> {
    let l = Launch {
        agent: String::new(),
        act: Act::Attach(name.to_string()),
        tmux: Some(name.to_string()),
        cwd: None,
        ccm_sid: None,
        model: None,
        launcher: String::new(),
        default_launcher: String::new(),
        preset_args: Vec::new(),
    };
    let entry = own_entry(deps).map_err(|m| fail("refused", m, None))?;
    lr::line_of(&l, &Settled::Unsaid, here, deps.caps, &entry).map_err(|m| fail("refused", m, None))
}

fn opened(landed: Landed, session: Option<String>, line: Option<String>) -> Opened {
    Opened {
        landed,
        session,
        line,
        sid: None,
        agent: None,
        cwd: None,
        account: None,
        note: None,
        prompt: None,
        arrival: None,
    }
}

/// 接回：按会话（挂着它的那个；命中多个接第一个、`note` 说一句）或按 tmux 会话名。
fn look(
    sid: Option<&str>,
    session: Option<&str>,
    here: Here,
    deps: &Deps,
) -> Result<Opened, Failed> {
    let rows = tmux_rows(deps)?;
    let (landed, name, note) = match (sid, session) {
        (Some(sid), None) if shell_quote_core::session_id_ok(sid) => match carriers(&rows, sid) {
            Standing::Running(e) => (Landed::Running, e.name.clone(), None),
            Standing::Ambiguous(es) => {
                let first = es[0].name.clone();
                let n = es.len().to_string();
                let note =
                    copy_core::copy_text("beSessionOpen.look.many", &[("n", &n), ("name", &first)]);
                (Landed::Running, first, Some(note))
            }
            Standing::Idle(e) => (Landed::Idle, e.name.clone(), None),
            Standing::None => {
                return Err(fail(
                    "not_in_tmux",
                    copy_core::copy_text("beSessionOpen.look.notInTmux", &[]),
                    None,
                ))
            }
        },
        (None, Some(name)) => match rows.iter().find(|r| r.name == name) {
            Some(r) => (
                if r.agent {
                    Landed::Running
                } else {
                    Landed::Idle
                },
                r.name.clone(),
                None,
            ),
            None => {
                return Err(fail(
                    "not_in_tmux",
                    copy_core::copy_text("beSessionOpen.look.noSession", &[("name", name)]),
                    None,
                ))
            }
        },
        _ => {
            return Err(malformed(
                "`look` takes exactly one of `sid` (a session id) and `session`",
            ))
        }
    };
    let line = attach_line(&name, here, deps)?;
    Ok(Opened {
        sid: sid.map(str::to_string),
        note,
        ..opened(landed, Some(name), Some(line))
    })
}

/// 开一个 shell：给了会话名 ⇒ 不在就在 `cwd` 里建一个空的（`idle`）、已在 ⇒ 照它此刻的样子，回接进去那一行；
/// 只给目录 ⇒ 这台 shell 里进那个目录那一行（`window`）。目录先展开 `~`、核在不在。
fn shell(
    session: Option<&str>,
    cwd: Option<&str>,
    here: Here,
    deps: &Deps,
    home: Option<&std::path::Path>,
) -> Result<Opened, Failed> {
    let dir = match cwd.map(str::trim) {
        None => None,
        Some(raw) if !shell_quote_core::free_text_ok(raw) => {
            return Err(malformed("`cwd` must not hold NUL, CR or LF"))
        }
        Some(raw) => {
            let d = expand_home(raw, home);
            if d.is_empty() || !(deps.host.is_dir)(&d) {
                return Err(fail(
                    "no_dir",
                    copy_core::copy_text("beSessionNew.cwd.missing", &[("cwd", raw)]),
                    Some(Field::Cwd),
                ));
            }
            Some(d)
        }
    };
    let Some(name) = session.map(str::trim) else {
        let Some(d) = dir else {
            return Err(malformed("`shell` needs `session` or `cwd`"));
        };
        let line = crate::platform::shell::enter_dir_line(&d);
        return Ok(Opened {
            cwd: Some(d),
            ..opened(Landed::Window, None, Some(line))
        });
    };
    let rows = tmux_rows(deps)?;
    let (landed, cwd) = match rows.iter().find(|r| r.name == name) {
        Some(r) if r.agent => (Landed::Running, None),
        Some(_) => (Landed::Idle, None),
        None => {
            if crate::control::gate_rules::new_tmux_name_issue(name).is_some() {
                return Err(fail(
                    "bad_tmux_name",
                    copy_core::copy_text("beSessionNew.tmux.badName", &[("name", name)]),
                    Some(Field::TmuxName),
                ));
            }
            (deps.open_shell)(name, dir.as_deref()).map_err(|f| {
                let (c, s, d, raw) = lower((&f.code, f.message));
                (c, s, d, raw.or(f.raw))
            })?;
            (Landed::Idle, dir)
        }
    };
    let line = attach_line(name, here, deps)?;
    Ok(Opened {
        cwd,
        ..opened(landed, Some(name.to_string()), Some(line))
    })
}

/// 这一项交给那一家的那一半末尾：权限档旗标 ＋ `--` ＋ 那一句。权限档只认那一家声明可选的那几档 ⇒ 别的不给、跟随。
fn tail_args(it: &Item, b: &Batch) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(m), Some(p)) = (
        it.permission_mode.as_deref(),
        b.face.and_then(|f| f.permission),
    ) {
        if p.modes.iter().any(|c| c.value == m) {
            out.extend([p.flag.to_string(), m.to_string()]);
        }
    }
    if let Some(text) = it.say.as_ref().filter(|_| b.takes_first_say()) {
        out.extend([crate::ccm_line::argv::flag::END.to_string(), text.clone()]);
    }
    out
}

/// 恢复那一项的那一行：`tmux` ＝ 要新建的会话名（`None` ＝ 直路）。
fn resume_launch(it: &Item, b: &Batch, tmux: Option<&str>) -> Launch {
    Launch {
        agent: b.agent.clone(),
        act: Act::Resume(it.sid.clone()),
        tmux: tmux.map(str::to_string),
        cwd: Some(it.cwd.clone()),
        ccm_sid: tmux.map(|_| it.sid.clone()),
        model: it
            .model
            .clone()
            .filter(|m| shell_quote_core::model_name_ok(m)),
        launcher: b.launcher.clone(),
        default_launcher: b.default_launcher.clone(),
        preset_args: tail_args(it, b),
    }
}

/// 这台 `ccm` 的入口（那几行直接叫它，不靠 `PATH`）。
fn own_entry(deps: &Deps) -> Result<String, String> {
    (deps.host.entry)().ok_or_else(|| copy_core::copy_text("beLaunchRender.entry.noHome", &[]))
}

/// 应答里的 `account`：实际用的号（账号库里的那一形；别的 ⇒ `None`）。
fn launched(account: &Settled) -> Option<LaunchedAccount> {
    match account {
        Settled::Account(a) => Some(a.clone()),
        _ => None,
    }
}

/// 选不了号 ⇒ `account_unavailable`，`data` 带要的号与替代。
fn unavailable(u: la::AccountUnavailable) -> Failed {
    let said = la::unavailable_said(&u);
    let data = Refusal {
        field: Some(Field::Account),
        unavailable: Some(u),
    };
    (
        "account_unavailable",
        said,
        serde_json::to_value(data).ok(),
        None,
    )
}

/// **恢复一个**：先判号（选不了 ⇒ 不起），再问记录在不在（查这个号那棵树），已有进程在写 ⇒ 不起（直路也一样），再按放在哪做。
fn start_one(it: &Item, b: &Batch, run: Run, here: Here, deps: &Deps) -> Result<Opened, Failed> {
    let account =
        la::settle(&it.account, Some(&it.sid), &b.models, deps.accounts).map_err(unavailable)?;
    let root = match &account {
        Settled::Account(a) => Some(a.config_dir.as_str()),
        Settled::Base | Settled::Unsaid => None,
    };
    match (deps.record)(&it.sid, root) {
        Err(e) => return Err(fail("bad_args", e, None)),
        Ok((false, root)) => {
            let (c, s, d, _) = fail(
                "record_gone",
                copy_core::copy_text("beSessionOpen.record.gone", &[]),
                None,
            );
            return Err((c, s, d, Some(root)));
        }
        Ok((true, _)) => {}
    }
    let live = (deps.writers)(&it.sid);
    let out = match run {
        Run::Tmux => start_in_tmux(it, b, &account, &tmux_rows(deps)?, here, &live, deps)?,
        Run::Direct => {
            if !live.is_empty() {
                return Err(already_live(&live));
            }
            pretrust(&account, &it.cwd, deps);
            let entry = own_entry(deps).map_err(|m| fail("refused", m, None))?;
            let line = lr::line_of(
                &resume_launch(it, b, None),
                &account,
                here,
                deps.caps,
                &entry,
            )
            .map_err(|m| fail("refused", m, None))?;
            opened(Landed::Window, None, Some(line))
        }
    };
    Ok(Opened {
        sid: Some(it.sid.clone()),
        agent: Some(crate::agents::AgentKind::new(b.agent.as_str())),
        cwd: Some(it.cwd.clone()),
        account: launched(&account).filter(|_| out.landed != Landed::Running),
        ..out
    })
}

/// 已有活进程在写 ⇒ 不起（再起一个就是两个进程同写一份记录）；`raw` 是那几个 pid。
fn already_live(live: &[u32]) -> Failed {
    let pids = pids_said(live);
    let (c, s, d, _) = fail(
        "already_live",
        copy_core::copy_text(
            "beSessionOpen.live.writing",
            &[("n", &live.len().to_string()), ("pids", &pids)],
        ),
        None,
    );
    (c, s, d, Some(pids))
}

/// 在 tmux 里起：在跑 ⇒ 不另起（接进去那一行）；已有活进程在写（不在这台的 tmux 里）⇒ 不起；
/// 空 tmux ⇒ 就地键入直路那一行；都不是 ⇒ 铸名、交那一行 ccm（`--detach`）。
fn start_in_tmux(
    it: &Item,
    b: &Batch,
    account: &Settled,
    rows: &[TmuxEntry],
    here: Here,
    live: &[u32],
    deps: &Deps,
) -> Result<Opened, Failed> {
    let (landed, name, note) = match carriers(rows, &it.sid) {
        Standing::Running(e) => (Landed::Running, e.name.clone(), None),
        Standing::Ambiguous(es) => {
            let first = es[0].name.clone();
            let note = copy_core::copy_text(
                "beSessionOpen.look.many",
                &[("n", &es.len().to_string()), ("name", &first)],
            );
            (Landed::Running, first, Some(note))
        }
        _ if !live.is_empty() => return Err(already_live(live)),
        Standing::Idle(e) => {
            let entry = own_entry(deps).map_err(|m| fail("refused", m, None))?;
            // 键进那个窗格的直路那一行：窗格里已经在那个目录，不带 `--cwd`。
            let l = Launch {
                cwd: None,
                ..resume_launch(it, b, None)
            };
            let line = lr::line_of(&l, account, Here::Remote, deps.caps, &entry)
                .map_err(|m| fail("refused", m, None))?;
            pretrust(account, &it.cwd, deps);
            (deps.send_into)(&e.name, &it.sid, &line).map_err(|f| {
                let (c, s, d, raw) = lower((&f.code, f.message));
                (c, s, d, raw.or(f.raw))
            })?;
            (Landed::InPlace, e.name.clone(), None)
        }
        Standing::None => {
            let name = (deps.mint)(NameBase::Cwd(&it.cwd)).map_err(lower)?;
            start_named(it, b, account, &name, here, deps)?;
            (Landed::Started, name, None)
        }
    };
    let line = attach_line(&name, here, deps)?;
    Ok(Opened {
        note,
        ..opened(landed, Some(name), Some(line))
    })
}

/// 必铸新终端名：问这台铸，再核一遍不落在名单里任何一个上（父会话那个名字绝不复用）。落上了 ⇒ `tmux_taken`，不起。
fn fresh_name(base: NameBase, rows: &[TmuxEntry], deps: &Deps) -> Result<String, Failed> {
    let name = (deps.mint)(base).map_err(lower)?;
    if rows.iter().any(|r| r.name == name) {
        return Err(taken(&name));
    }
    Ok(name)
}

/// 会话名被占 ⇒ `tmux_taken`，原话格是那个名字。
fn taken(name: &str) -> Failed {
    let (c, s, d, _) = fail(
        "tmux_taken",
        copy_core::copy_text("beSessionNew.tmux.taken", &[("name", name)]),
        Some(Field::TmuxName),
    );
    (c, s, d, Some(name.to_string()))
}

fn source_terminal<'a>(rows: &'a [TmuxEntry], sid: Option<&str>) -> Option<&'a str> {
    match carriers(rows, sid?) {
        Standing::Running(e) => Some(e.name.as_str()),
        Standing::Ambiguous(es) => es.first().map(|e| e.name.as_str()),
        Standing::Idle(_) | Standing::None => None,
    }
}

/// 以 `name` 新建一个 tmux 会话、在里面起这一个（交一行 ccm，`--detach`）。
fn start_named(
    it: &Item,
    b: &Batch,
    account: &Settled,
    name: &str,
    here: Here,
    deps: &Deps,
) -> Result<(), Failed> {
    let argv = own_entry(deps)
        .and_then(|entry| {
            lr::argv_of(
                &resume_launch(it, b, Some(name)),
                account,
                here,
                true,
                deps.caps,
                &entry,
            )
        })
        .map_err(|m| fail("refused", m, None))?;
    pretrust(account, &it.cwd, deps);
    run_ccm(&argv, name, deps)
}

/// 交那一行 ccm：退出码 3 ＝ 会话名被占（ccm 响亮失败，不接回别人的会话）；别的非零 ⇒ `start_failed`，退出码与 stderr 进复制详情。
fn run_ccm(argv: &[String], name: &str, deps: &Deps) -> Result<(), Failed> {
    match (deps.run_ccm)(argv) {
        Ok((0, _, _)) => Ok(()),
        Ok((3, _, _)) => Err(taken(name)),
        Ok((rc, _, err)) => {
            let (c, s, d, _) = fail(
                "start_failed",
                copy_core::copy_text("beSessionNew.start.failed", &[]),
                None,
            );
            Err((c, s, d, Some(format!("exit {rc}\n{}", err.trim()))))
        }
        Err(e) => Err(lower(e)),
    }
}

/// 当 ccm 起自己那一趟的期限。
const SELF_AS_CCM_SECS: u64 = 15;
const SELF_AS_CCM_WITHIN: Deadline = Deadline::secs(SELF_AS_CCM_SECS);

/// `session-open` 一条的总期限：列一次名单（同 `sessions-where` 的 8 s）＋ 当 ccm 起一趟。
pub(crate) const OPEN_CAP: Deadline = Deadline::secs(8 + SELF_AS_CCM_SECS);

/// 交一行 ccm：这台后端自己就是 ccm（同一个二进制，argv 不过 shell）。不接进去（`--detach`），等它退出、收它的话。
/// 去掉 `TMUX` / `TMUX_PANE`：单个那一项是在一个新开的终端里跑这一行，那里不在 tmux 里。
/// 失败 ⇒ 单个那一条的 `(码, 原话)`：起不来 ⇒ `start_failed`；过了期限 ⇒ `child_timed_out`。
pub(crate) fn run_self_as_ccm(argv: &[String]) -> Result<(i32, String, String), CmdErr> {
    let me = std::env::current_exe().map_err(|e| ("start_failed", e.to_string()))?;
    let out = Child::new(me)
        .args(argv.iter().skip(1))
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .run(SELF_AS_CCM_WITHIN)
        .map_err(|e| e.into_cmd_err("start_failed", |e| e.to_string()))?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// 原因码：那一家起的时候不收第一句（恢复再送那一项起不了）。
pub(crate) const NO_FIRST_PROMPT: &str = "no_first_prompt";

/// **恢复再送**（`session-say` 送的时候终端已经没了）：在 tmux 里恢复一项，那一句随行交给那一家；那一家不收第一句 ⇒ 不起。
/// 起法用远端那一行（这台后端自己当 ccm 跑，工作目录随行）。
pub(crate) fn resume_saying(it: &Item, agent: &str, deps: &Deps) -> Result<Opened, Failed> {
    let b = Batch::of(agent, deps.prefs);
    if it.say.is_some() && !b.takes_first_say() {
        return Err((NO_FIRST_PROMPT, agent.to_string(), None, None));
    }
    start_one(it, &b, Run::Tmux, Here::Remote, deps)
}

/// **换号重启**起新那一步：旧的已停，在让出来的那个名字里用判好的号起。
pub(crate) fn relaunch_in(
    it: &Item,
    agent: &str,
    account: &Settled,
    name: &str,
    local: bool,
    deps: &Deps,
) -> Result<(), Failed> {
    start_named(
        it,
        &Batch::of(agent, deps.prefs),
        account,
        name,
        here_of(local, deps),
        deps,
    )
}

/// 新起：认那一家 · 启动命令 · 目录 · 号 · 放在哪 · 终端名 · 分叉（前面几格全过了才写分支记录）· 起。
#[allow(clippy::too_many_arguments)]
fn start_new(
    req: NewOpen,
    asked: &AccountAsk,
    run: Option<Run>,
    here: Here,
    fork_tmux: Option<bool>,
    deps: &Deps,
    fork: ForkWrite,
    pre: &PreRotation,
    home: Option<&std::path::Path>,
) -> Result<Opened, Failed> {
    if req.agent.as_str().trim().is_empty() {
        return Err(fail(
            "unknown_agent",
            copy_core::copy_text("beSessionNew.agent.missing", &[]),
            Some(Field::Agent),
        ));
    }
    let (kind, face) = crate::agents::pick_kind(Some(req.agent.as_str()))
        .map_err(|m| fail("unknown_agent", m, Some(Field::Agent)))?;
    let rule = strategy_of(req.rotation.as_ref())?;
    // 新起的要先定 sid 才能起之前写好打开哪一份：那一家不认 ⇒ 不收（分叉的 sid 由分支记录定）。
    if rule.is_some() && req.fork.is_none() && face.preset_sid.is_none() {
        return Err(malformed(
            "this agent cannot take a rotation strategy at launch",
        ));
    }
    // 放在哪：没说 ⇒ 这台定；说了 tmux 而这台没有 ⇒ 拒（下面铸名那一步说）。
    let rows = listed(deps).map_err(lower)?;
    let run = run.unwrap_or_else(|| default_run(Intent::New { fork_tmux }, false, rows.is_some()));
    // 第一句：只跟 tmux（直路那一形的终端在界面那一侧）；空的不收；那一家声明不认起的时候带 ⇒ 不收。
    let prompt = match req.prompt.as_deref().map(str::trim) {
        None => None,
        Some("") => return Err(malformed("`prompt` must not be empty")),
        Some(_) if run != Run::Tmux => {
            return Err(malformed("`prompt` goes only with `run: tmux`"))
        }
        Some(_) if first_prompt_refused(&face).is_some() => {
            return Err(malformed(first_prompt_refused(&face).unwrap_or_default()))
        }
        Some(_) => req.prompt.clone(),
    };
    if req
        .model
        .as_deref()
        .is_some_and(|m| !shell_quote_core::model_name_ok(m))
    {
        return Err(malformed("`model` is not a model name"));
    }
    // 没给 ⇒ 这台偏好里那一家的那一档（那一家已不认 ⇒ 不带）。
    let asked_mode = req
        .permission_mode
        .clone()
        .or_else(|| crate::control::machine_prefs::permission_mode_for(deps.prefs, kind));
    let permission_args: Vec<String> = match asked_mode.as_deref() {
        None => Vec::new(),
        Some(m) => match face.permission {
            Some(p) if p.modes.iter().any(|c| c.value == m) => {
                vec![p.flag.to_string(), m.to_string()]
            }
            Some(_) => return Err(malformed("`permissionMode` is not one this agent offers")),
            None => return Err(malformed("this agent has no permission modes")),
        },
    };
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
            Some(Field::Command),
        ));
    }
    let launcher = command.map_or_else(
        || deps.prefs.launcher_for(kind, face.default_launcher),
        str::to_string,
    );
    let cwd = expand_home(req.cwd.trim(), home);
    if cwd.is_empty() || !(deps.host.is_dir)(&cwd) {
        return Err(fail(
            "no_dir",
            copy_core::copy_text("beSessionNew.cwd.missing", &[("cwd", req.cwd.trim())]),
            Some(Field::Cwd),
        ));
    }
    if let Some(f) = &req.fork {
        if [&f.sid, &f.uuid]
            .iter()
            .any(|id| !shell_quote_core::session_id_ok(id))
        {
            return Err(malformed("`fork` needs a session id and a message id"));
        }
        if req.tmux_name.is_some() {
            return Err(malformed(
                "`tmuxName` does not go with `fork` (the fork mints its own)",
            ));
        }
    }
    let source = req.fork.as_ref().map(|f| f.sid.as_str());
    let account =
        la::settle(asked, source, &deps.prefs.models, deps.accounts).map_err(unavailable)?;
    let name = match run {
        Run::Direct => None,
        Run::Tmux => {
            let Some(rows) = rows else {
                return Err(fail(
                    "place_unavailable",
                    copy_core::copy_text("beSessionNew.place.noTmux", &[]),
                    Some(Field::Place),
                ));
            };
            Some(new_tmux_name(&req, &cwd, &rows, deps)?)
        }
    };
    // 前面几格全过了才写分支记录（某格不行都不留东西）。
    let forked = match &req.fork {
        Some(f) => Some(fork(&f.sid, &f.uuid).map_err(|(c, m)| fail(c, m, None))?),
        None => None,
    };
    // 新起的会话：那一家认先定 sid ⇒ 这台先定（交给那一家的那一串里带它），回包就说得出 sid。
    let preset = match (&forked, face.preset_sid) {
        (None, Some(_)) => Some((pre.sid)()),
        _ => None,
    };
    let mut preset_args: Vec<String> = match (&preset, face.preset_sid) {
        (Some(s), Some(flag)) => vec![flag.to_string(), s.clone()],
        _ => Vec::new(),
    };
    preset_args.extend(permission_args);
    if let Some(text) = &prompt {
        preset_args.extend([crate::ccm_line::argv::flag::END.to_string(), text.clone()]);
    }
    let sid = forked.clone().or_else(|| preset.clone());
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
    let l = Launch {
        agent: kind.to_string(),
        act: match &forked {
            Some(s) => Act::Resume(s.clone()),
            None => Act::New,
        },
        tmux: name.clone(),
        cwd: Some(cwd.clone()),
        // 身份标记打在建出来的 tmux 会话上（resume · 先定好 sid 的新起才说得出 sid）。
        ccm_sid: sid.clone().filter(|_| name.is_some()),
        model: req.model.clone(),
        launcher,
        default_launcher: face.default_launcher.to_string(),
        preset_args,
    };
    pretrust(&account, &cwd, deps);
    let launch = || -> Result<Opened, Failed> {
        let entry = own_entry(deps).map_err(|m| fail("refused", m, None))?;
        Ok(match &name {
            Some(name) => {
                let argv = lr::argv_of(&l, &account, here, true, deps.caps, &entry)
                    .map_err(|m| fail("refused", m, None))?;
                run_ccm(&argv, name, deps)?;
                let line = attach_line(name, here, deps)?;
                opened(Landed::Started, Some(name.clone()), Some(line))
            }
            None => {
                let line = lr::line_of(&l, &account, here, deps.caps, &entry)
                    .map_err(|m| fail("refused", m, None))?;
                opened(Landed::Window, None, Some(line))
            }
        })
    };
    let out = launch().map_err(undo)?;
    Ok(Opened {
        sid,
        agent: Some(crate::agents::AgentKind::new(kind)),
        cwd: Some(cwd),
        account: launched(&account),
        // 第一句起的时候已经交给那一家了 ⇒ 交到了。
        prompt: prompt.map(|_| FirstSay {
            result: "delivered".into(),
            why: None,
            said: None,
        }),
        ..out
    })
}

/// `rotation` 那一格：缺 ⇒ `None`（新会话用的那份）；`{strategy: id}` ⇒ 那一份；`{strategy: null}` ⇒ `Some("")`（不轮换）。
fn strategy_of(v: Option<&RotationPick>) -> Result<Option<String>, Failed> {
    match v.map(|p| p.strategy.as_deref()) {
        None => Ok(None),
        Some(None) => Ok(Some(String::new())),
        Some(Some(id)) if !id.is_empty() => Ok(Some(id.to_string())),
        Some(_) => Err(malformed("`rotation` must be {\"strategy\": id}")),
    }
}

/// 新起 tmux 那一形的会话名：给了 ⇒ 核写法与不占用；没给 ⇒ 这台铸（分叉从源会话此刻的终端名铸）。
fn new_tmux_name(
    req: &NewOpen,
    cwd: &str,
    rows: &[TmuxEntry],
    deps: &Deps,
) -> Result<String, Failed> {
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
                Some(Field::TmuxName),
            ));
        }
        if rows.iter().any(|r| r.name == n) {
            return Err(taken(n));
        }
        return Ok(n.to_string());
    }
    let base = match &req.fork {
        Some(f) => NameBase::ForkOf(source_terminal(rows, Some(&f.sid)).unwrap_or(cwd)),
        None => NameBase::Cwd(cwd),
    };
    fresh_name(base, rows, deps)
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/session_open_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/backend/control/session_open_new_tests.rs"]
mod new_tests;
