//! 帧面 `session-open` · `session-open-facts` · `session-open-dir` 的宿主壳（一族命令一个壳：开 · 事实 · 目录）。
//! `session-open`：恢复那一条的记录（哪一家 · 目录 · 模型 · 权限档）· 分叉源会话此刻在不在 tmux 里 · 写分支记录 ·
//! 起之前定 sid 写轮换那几下都住观测层与这台的家，由这层读好交给 `control/session_open`；起会话要的那几样同批量那一份（`session_batch_face`）。

use crate::control::session_open::{self as open, Facts, ResumeOf};
use crate::control::session_open::{ForkWrite, PreRotation};
use cmd_wire::listing::ResumeWith;
use cmd_wire::session::{ForkSlot, ForkTerminal};
use serde_json::Value;
use std::collections::BTreeMap;

/// `session-open`：起新的 · 恢复某条 · 接回，一个请求。阻塞档；写自家状态（起之前写好打开哪一份轮换配置）⇒ 写句柄由分派那一层递。
pub(crate) struct SessionOpen;

impl crate::wire::typed::Command for SessionOpen {
    const NAME: &'static str = "session-open";
    type Req = open::SessionOpenReq;
    type Reply = open::Opened;
    type Err = open::SessionOpenErr;
}

impl crate::wire::typed::Handle for SessionOpen {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: open::SessionOpenReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<open::Opened, open::SessionOpenErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);
            answer(req, files, own, &crate::platform::fs::NewFile::issue())
                .map_err(open::SessionOpenErr)
        }
    }
}

fn answer(
    req: open::SessionOpenReq,
    files: super::launch_account_face::Files,
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
) -> Result<open::Opened, open::Failed> {
    let client = crate::control::gate::requester(req.client.as_deref())
        .map_err(|(c, m)| open::fail(c, m, None))?;
    // 起之前记下这台的报到序号：之后报到的那一条才是它（回包的 `arrival.since`）。
    let since = crate::common::arrival_seq::mark();
    let (agent, facts) = match &req.what {
        open::Open::New(n) => {
            let fork_tmux = n
                .fork
                .as_ref()
                .filter(|_| req.run.is_none())
                .and_then(|f| fork_in_tmux(&f.sid));
            (
                n.agent.as_str().to_string(),
                Facts {
                    resume: None,
                    fork_tmux,
                    since,
                },
            )
        }
        open::Open::Resume { sid } => {
            let resume = resume_of(sid);
            let agent = resume
                .as_ref()
                .map_or_else(String::new, |r| r.agent.clone());
            (
                agent,
                Facts {
                    resume,
                    fork_tmux: None,
                    since,
                },
            )
        }
        open::Open::Look { .. } | open::Open::Shell { .. } => (
            String::new(),
            Facts {
                resume: None,
                fork_tmux: None,
                since,
            },
        ),
    };
    with_launch_ctx(own, nf, |fork, pre, home| {
        super::session_batch_face::with_deps_as(client.as_deref(), &agent, Some(files), |d| {
            open::answer(req, facts, d, fork, pre, home)
        })
    })
}

/// 起新会话要的那几下（写分支记录 · 先定 sid · 写 / 撤轮换）＋ 这台的家目录，交给 `f`。
pub(crate) fn with_launch_ctx<T>(
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
    f: impl FnOnce(ForkWrite, &PreRotation, Option<&std::path::Path>) -> T,
) -> T {
    let home = crate::history::listing::agent_home();
    let fork =
        |sid: &str, uuid: &str| crate::control::fork_write::fork_for_launch(nf, &home, sid, uuid);
    let user_home = crate::platform::paths::home_dir();
    let ctx = super::rotation_face::Ctx::here();
    let sid = crate::accounts::quota::rotation::new_session_id;
    let write = |sid: &str, kind: &str, rule: &str| {
        super::rotation_face::preset_with(
            &ctx,
            own,
            sid,
            kind,
            rule,
            crate::accounts::quota::now_unix(),
        )
    };
    let forget = |sid: &str| super::rotation_face::forget_preset_with(&ctx, own, sid);
    let pre = PreRotation {
        sid: &sid,
        write: &write,
        forget: &forget,
    };
    f(&fork, &pre, user_home.as_deref())
}

/// 恢复那一条要从记录里读的：记录按 sid 在这台各家自己的记录根里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 任一找不到 ⇒ `None`（不恢复）。`session-say` 恢复那一支同一份。
pub(crate) fn resume_of(sid: &str) -> Option<ResumeOf> {
    resume_in(&crate::history::listing::agent_home(), sid)
}

/// [`resume_of`] 在记录树 `home` 里找。
pub(crate) fn resume_in(home: &std::path::Path, sid: &str) -> Option<ResumeOf> {
    use crate::observe::query::locate as hq;
    let home = home.to_path_buf();
    // 记录在不在交这台的会话账本（同 `history-record`：说过已结束的那条随之在「已结束 ⇄ 记录已不在」之间翻）。
    let path = hq::session_record(&home, sid);
    crate::observe::record_hook::seen(sid, path.is_ok());
    let path = path.ok()?;
    let path = path.to_str()?;
    let agent = hq::facts_agent(&home, path)?;
    let cwd = hq::facts_project_dir(&home, path)?;
    let facts = hq::facts(&home, path, &Vec::new(), None).ok()?;
    use crate::history::history_annotations as ha;
    let kept = ha::kept_for_resume(ha::resume_with_of(sid), std::path::Path::new(path));
    Some(with_kept(
        ResumeOf {
            agent,
            cwd,
            model: facts.usage.and_then(|u| u.model),
            permission_mode: facts.permission_mode,
        },
        kept,
    ))
}

/// 记下了的那两格盖过记录里的；模型记的是「默认」那一项 ⇒ 不带模型（不写就是那一家自己的默认）。
pub(crate) fn with_kept(r: ResumeOf, kept: Option<ResumeWith>) -> ResumeOf {
    let Some(k) = kept else { return r };
    let default = crate::agents::launch_face_among(crate::agents::REGISTRY, &r.agent)
        .and_then(|f| f.model_default)
        .map(|c| c.value);
    ResumeOf {
        model: match k.model {
            Some(m) => (Some(m.as_str()) != default).then_some(m),
            None => r.model,
        },
        permission_mode: k.permission_mode.or(r.permission_mode),
        ..r
    }
}

/// 分叉的源会话此刻在不在 tmux 里（同 `session-open-facts` 那一格 `fork.launch.terminal`）；说不出 ⇒ `None`。
fn fork_in_tmux(sid: &str) -> Option<bool> {
    let home = crate::history::listing::agent_home();
    let source = crate::agents::find_session(&home, sid).ok()?.file;
    hosted(&super::fork_face::launch_of(&home, &source, sid))
}

/// 分叉的源会话此刻在不在 tmux 里（那台说不出 ⇒ `None`）：`session-open` 缺 `run` 与 `session-open-facts` 的 `run` 读的同一格。
fn hosted(launch: &cmd_wire::session::ForkLaunch) -> Option<bool> {
    match &launch.terminal {
        ForkSlot::Known { value, .. } => Some(matches!(value, ForkTerminal::Hosted { .. })),
        ForkSlot::Unknown { .. } => None,
    }
}

/// 新建框默认「运行于」：同 `session-open {what: new}` 不给 `run` 时那一条（[`open::default_run`]）。
pub(crate) fn facts_run(fork_tmux: Option<bool>, tmux: bool) -> open::Run {
    open::default_run(open::Intent::New { fork_tmux }, false, tmux)
}

/// 「最近用过的目录」最多列几条。
pub(crate) const RECENT_MAX: usize = 8;

/// `session-open-dir`：起新会话前核一个目录（在不在 · 这台此刻会给它铸的终端名）。阻塞档：起一次 `tmux` 列会话、铸名。
pub(crate) struct SessionOpenDir;

impl crate::wire::typed::Command for SessionOpenDir {
    const NAME: &'static str = "session-open-dir";
    type Req = crate::control::session_open::new::OpenDirReq;
    type Reply = crate::control::session_open::new::DirChecked;
    type Err = crate::control::session_open::new::OpenDirErr;
}

impl crate::wire::typed::Handle for SessionOpenDir {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: crate::control::session_open::new::OpenDirReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<
            crate::control::session_open::new::DirChecked,
            crate::control::session_open::new::OpenDirErr,
        >,
    > + Send
           + 'static {
        async move {
            let user_home = crate::platform::paths::home_dir();
            super::session_batch_face::with_deps_as(None, "", None, |d| {
                crate::control::session_open::new::dir_answer(&req, d, user_home.as_deref())
            })
        }
    }
}

/// 默认启动器在起会话那个 shell 的 `PATH` 上找得到 —— 与足迹里「装没装」、`ccm` 起它那一下问同一个函数（`platform::shell::find_session_command`）。
fn launcher_found(name: &str) -> bool {
    launcher_found_in(
        name,
        crate::platform::shell::session_shell_path().as_deref(),
        cfg!(windows),
    )
}

/// [`launcher_found`] 的本体（那份 `PATH` 与平台是参数）。问不出那个 shell 的 `PATH` ⇒ 不藏（判不了不当成没装）。
pub(crate) fn launcher_found_in(name: &str, session_path: Option<&str>, windows: bool) -> bool {
    !matches!(
        crate::platform::shell::find_session_command(name, session_path, windows, &|p| p.is_file()),
        crate::platform::shell::SessionCommand::Absent
    )
}

/// 这台能起的几家（注册表里由我们起的、且默认启动器在这台找得到的；注册表序）。
pub(crate) fn launchable_here(found: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    crate::agents::launchable_kinds()
        .into_iter()
        .filter(|k| crate::agents::pick_kind(Some(k)).is_ok_and(|(_, f)| found(f.default_launcher)))
        .collect()
}

/// `session-open-facts` 的应答与它引到的几样：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::{AgentFacts, AgentPick, ForkFacts, OpenFacts, RecentDir};

/// 一家的起会话事实（名字 · 可选的模型与权限档）；`kind` 认不出 ⇒ 名字照 kind、两张表空。
pub(crate) fn agent_facts(
    kind: &'static str,
    prefs: &crate::control::machine_prefs::Prefs,
) -> AgentFacts {
    let face = crate::agents::pick_kind(Some(kind)).ok().map(|(_, f)| f);
    let picks = |cs: &[crate::agents::Choice]| {
        cs.iter()
            .map(|c| AgentPick {
                value: c.value.to_string(),
                text: (c.text)(),
            })
            .collect()
    };
    AgentFacts {
        kind: kind.to_string(),
        text: face.map_or(kind, |f| f.display_name).to_string(),
        models: face.and_then(|f| f.models).map(picks).unwrap_or_default(),
        model_default: face.and_then(|f| f.model_default).map(|c| AgentPick {
            value: c.value.to_string(),
            text: (c.text)(),
        }),
        model_note: face
            .and_then(|f| f.model_switch)
            .and_then(|m| m.note)
            .map(|n| n()),
        permission_modes: face
            .and_then(|f| f.permission)
            .map(|p| picks(p.modes))
            .unwrap_or_default(),
        permission_default: prefs
            .permission_modes
            .get(kind)
            .and_then(|m| crate::control::machine_prefs::offered(kind, m))
            .map(|c| AgentPick {
                value: c.value.to_string(),
                text: (c.text)(),
            }),
    }
}

/// 各家记录里的工作目录 ⇒ 最近用过的那几个（新的在前，同一个目录只出一次；不出会话的那个目录不算）。
pub(crate) fn recent_dirs(
    items: &[(String, i64)],
    hidden: &dyn Fn(&str) -> bool,
    clock: &crate::common::time::TextClock,
) -> Vec<RecentDir> {
    let mut best: BTreeMap<&str, i64> = BTreeMap::new();
    for (cwd, ms) in items {
        if cwd.is_empty() || hidden(cwd) {
            continue;
        }
        let e = best.entry(cwd.as_str()).or_insert(*ms);
        *e = (*e).max(*ms);
    }
    let mut v: Vec<(&str, i64)> = best.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.into_iter()
        .take(RECENT_MAX)
        .map(|(cwd, ms)| RecentDir {
            cwd: cwd.to_string(),
            last_ms: ms,
            last_text: clock
                .text(u64::try_from(ms.div_euclid(1000)).unwrap_or(0))
                .0,
        })
        .collect()
}

/// 记录树那一家的项目 ＋ 合成历史那几家的会话 ⇒ `(工作目录, 最近一次)`。
fn record_dirs(home: &std::path::Path) -> Vec<(String, i64)> {
    let mut buf: Vec<u8> = Vec::new();
    // 读不出记录树 ⇒ 记一行、少这一家的目录（框里照常起，只是「最近的」少几项）。
    if let Err(e) = crate::history::listing::list_projects_into(home, &mut buf) {
        tracing::warn!("session-open-facts：读不出记录树的项目清单：{e}");
    }
    let mut out: Vec<(String, i64)> = String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            Some((
                v.get("projectPath")?.as_str()?.to_string(),
                v.get("lastActivityMs")?.as_i64()?,
            ))
        })
        .collect();
    for (_, face) in crate::agents::history_faces() {
        out.extend((face.sessions)().into_iter().map(|s| (s.cwd, s.mtime_ms)));
    }
    out
}

/// `session-open-facts` 的入参。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenFactsReq {
    /// 可缺：分叉源会话 sid。
    #[serde(default, rename = "forkOf")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_of: Option<String>,
    /// 可缺（随 `forkOf`）：从哪条消息处分叉（框顶说第几轮、几点）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) at: Option<String>,
}

crate::wire::typed::codes! {
    /// `session-open-facts` 的码。
    pub(crate) enum OpenFactsErr {
        /// 分叉源会话的记录找不着 / 记录树读不出。
        ForkFailed = "fork_failed",
    }
}

/// `session-open-facts`：起新会话那个框要的事实（只读，不写分支记录）。阻塞档：扫各家记录目录 · 问一次 tmux。
pub(crate) struct SessionOpenFacts;

impl crate::wire::typed::Command for SessionOpenFacts {
    const NAME: &'static str = "session-open-facts";
    type Req = OpenFactsReq;
    type Reply = OpenFacts;
    type Err = OpenFactsErr;
}

impl crate::wire::typed::Handle for SessionOpenFacts {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: OpenFactsReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<OpenFacts, OpenFactsErr>> + Send + 'static {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            facts(&req, &cx.tz).map_err(|(code, said)| match code {
                "bad_args" => OpenFactsErr::BadArgs(said),
                _ => {
                    debug_assert_eq!(code, "fork_failed");
                    OpenFactsErr::ForkFailed(said)
                }
            })
        }
    }
}

/// `session-open-facts` 的本体：`{forkOf?, at?}` ⇒ `{recent, tmux, agents, fork}`。
/// `tz` ＝ 看的那一台的时区：分叉那一轮的起点钟面按它写。
pub(crate) fn facts(
    req: &OpenFactsReq,
    tz: &crate::Tz,
) -> Result<OpenFacts, (&'static str, String)> {
    let fork_of = match req.fork_of.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) => Some(s),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`forkOf` must be a session id"),
            ))
        }
    };
    let at = match req.at.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) && fork_of.is_some() => Some(s),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "`at` must be a message id, given with `forkOf`",
                ),
            ))
        }
    };
    let home = crate::history::listing::agent_home();
    let fork = match fork_of {
        None => None,
        Some(sid) => {
            let source = crate::agents::find_session(&home, sid)
                .map_err(|m| ("fork_failed", m))?
                .file;
            // 第几轮照那一份的扫描图数（与 `history-turns` 同一张：回退掉的那几句不算一轮）。
            let turn = at.and_then(|uuid| {
                crate::observe::record_scan::global()
                    .get(&source)
                    .ok()?
                    .turn_at(uuid)
            });
            Some(ForkFacts {
                agent: crate::agents::record_kind_of(&source)
                    .unwrap_or_default()
                    .to_string(),
                launch: super::fork_face::launch_of(&home, &source, sid),
                turn: turn.as_ref().map(|t| t.0),
                start_text: turn.and_then(|t| crate::common::time::iso_hm(&t.1, tz)),
            })
        }
    };
    let tmux = !matches!(super::session_batch_face::tmux_rows(), Ok(None));
    let run = facts_run(fork.as_ref().and_then(|f| hosted(&f.launch)), tmux);
    Ok(OpenFacts {
        recent: recent_dirs(
            &record_dirs(&home),
            &crate::history::listing::hidden_cwd,
            &crate::common::time::TextClock::new(
                u64::try_from(crate::common::time::now_secs()).unwrap_or(0),
                tz,
            ),
        ),
        tmux,
        run,
        agents: {
            let prefs = crate::control::machine_prefs::read_now().prefs();
            launchable_here(&launcher_found)
                .into_iter()
                .map(|k| agent_facts(k, &prefs))
                .collect()
        },
        fork,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_open_face_tests.rs"]
mod tests;
