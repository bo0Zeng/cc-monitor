//! 帧面 `session-open` · `session-open-facts` · `session-open-dir` 的宿主壳（一族命令一个壳：开 · 事实 · 目录）。
//! `session-open`：恢复那一条的记录（哪一家 · 目录 · 模型 · 权限档）· 分叉源会话此刻在不在 tmux 里 · 叉（照那一家的分叉面） ·
//! 起之前定 sid 写轮换那几下都住观测层与这台的家，由这层读好交给 `control/session_open`；起会话要的那几样同批量那一份（`session_batch_face`）。

use crate::control::session_open::{self as open, Facts, ResumeOf};
use crate::control::session_open::{ForkWrite, PreRotation};
use cmd_wire::listing::ResumeWith;
use cmd_wire::session::{ForkSlot, ForkTerminal};
use std::collections::BTreeMap;

/// `session-open`：起新的 · 恢复某条 · 接回，一个请求。阻塞档；写自家状态（起之前写好打开哪一份轮换配置）⇒ 写句柄由分派那一层递。
pub struct SessionOpen;

impl crate::wire::typed::Command for SessionOpen {
    crate::wire::typed::by_call!(open::SessionOpenReq);
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
            #[expect(
                clippy::disallowed_methods,
                reason = "打开会话那一条带分叉（从某一轮接着开）：在它自己那份实现里领一只只会新建的手（别的命令拿不到它）"
            )]
            let nf = crate::platform::fs::NewFile::issue();
            answer(req, files, own, &nf).map_err(open::SessionOpenErr)
        }
    }
}

fn answer(
    req: open::SessionOpenReq,
    files: super::launch_account_face::Files,
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
) -> Result<open::Opened, crate::common::said::Said> {
    let client = crate::control::gate::requester(req.client.as_deref()).map_err(open::whole)?;
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
    // 接回 · 起新的 · 恢复两种宿主都认（托管那一行照句柄接；名单那一格 `managed` 说这台能不能托管，缺省放在哪照它）；
    // 开 shell 只开 tmux 会话 ⇒ 只认 tmux。
    let both = !matches!(req.what, open::Open::Shell { .. });
    with_launch_ctx(own, nf, |fork, pre, home| {
        let go =
            |d: &crate::control::session_batch::Deps| open::answer(req, facts, d, fork, pre, home);
        let client = client.as_deref();
        if both {
            super::session_batch_face::with_deps_both_as(client, &agent, Some(files), go)
        } else {
            super::session_batch_face::with_deps_as(client, &agent, Some(files), go)
        }
    })
}

/// 同 [`with_launch_ctx`]，叉那一手由这里造（起一组那一族不分叉，手里没有它）。
#[expect(
    clippy::disallowed_methods,
    reason = "分叉那条命令的另一个入口：手里没有那只只会新建的手的调用方由这里领一只"
)]
pub(crate) fn with_launch_ctx_here<T>(
    own: &crate::platform::fs::Own,
    f: impl FnOnce(ForkWrite, &PreRotation, Option<&std::path::Path>) -> T,
) -> T {
    with_launch_ctx(own, &crate::platform::fs::NewFile::issue(), f)
}

/// 起新会话要的那几下（叉 · 先定 sid · 写 / 撤轮换）＋ 这台的家目录，交给 `f`。
pub fn with_launch_ctx<T>(
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
    f: impl FnOnce(ForkWrite, &PreRotation, Option<&std::path::Path>) -> T,
) -> T {
    let home = crate::agents::local_homes();
    let fork =
        |sid: &str, uuid: &str| crate::control::fork_write::fork_for_launch(nf, &home, sid, uuid);
    let user_home = crate::platform::paths::home_dir();
    let ctx = super::rotation_face::Ctx::here();
    let sid = crate::accounts::quota::rotation::new_session_id;
    let write = |sid: &str, kind: &str, rule: &str| {
        super::rotation_edit_face::preset_with(
            &ctx,
            own,
            sid,
            kind,
            rule,
            crate::common::time::now_secs(),
        )
    };
    let forget = |sid: &str| super::rotation_edit_face::forget_preset_with(&ctx, own, sid);
    let pre = PreRotation {
        sid: &sid,
        write: &write,
        forget: &forget,
    };
    f(&fork, &pre, user_home.as_deref())
}

/// 恢复那一条要从记录里读的：记录按 sid 在这台各家自己的记录根里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 任一找不到 ⇒ `None`（不恢复）。`session-say` 恢复那一支同一份。
pub fn resume_of(sid: &str) -> Option<ResumeOf> {
    resume_in(&crate::agents::local_homes(), sid)
}

/// [`resume_of`] 在记录树 `home` 里找。
pub fn resume_in(home: impl crate::agents::IntoHomes, sid: &str) -> Option<ResumeOf> {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    let home: &[crate::agents::AgentHome] = &home_v;
    use crate::observe::query::locate as hq;
    let home = home.to_vec();
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
    let home = crate::agents::local_homes();
    let source = crate::agents::find_session_in(&home, sid).ok()?;
    hosted(&super::fork_face::launch_of(&home, &source, sid))
}

/// 分叉的源会话此刻在不在 tmux 里（那台说不出 ⇒ `None`）：`session-open` 缺 `run` 与 `session-open-facts` 的 `run` 读的同一格。
fn hosted(launch: &cmd_wire::session::ForkLaunch) -> Option<bool> {
    match &launch.terminal {
        ForkSlot::Known { value, .. } => Some(matches!(value, ForkTerminal::Tmux { .. })),
        ForkSlot::Unknown { .. } => None,
    }
}

/// 新建框默认「运行于」：同 `session-open {what: new}` 不给 `run` 时那一条（[`open::default_run`]）。
pub fn facts_run(fork_tmux: Option<bool>, tmux: bool, managed: bool) -> open::Launch {
    open::default_run(open::Intent::New { fork_tmux }, false, tmux, managed)
}

/// 这台能放在哪几种（`OpenFacts.hosts`，照 `Launch` 的序）：能托管 ⇒ 托管；有 tmux ⇒ tmux；终端窗口（直路）恒在。
pub(crate) fn facts_hosts(tmux: bool, managed: bool) -> Vec<open::Launch> {
    [
        (managed, open::Launch::Managed),
        (tmux, open::Launch::Tmux),
        (true, open::Launch::Direct),
    ]
    .into_iter()
    .filter_map(|(here, r)| here.then_some(r))
    .collect()
}

/// 「最近用过的目录」最多列几条。
pub(crate) const RECENT_MAX: usize = 8;

/// `session-open-dir`：起新会话前核一个目录（在不在 · 这台此刻会给它铸的终端名）。阻塞档：起一次 `tmux` 列会话、铸名。
pub struct SessionOpenDir;

impl crate::wire::typed::Command for SessionOpenDir {
    const NAME: &'static str = "session-open-dir";
    type Req = crate::control::session_open::new::OpenDirReq;
    type Reply = crate::control::session_open::new::DirChecked;
    type Err = crate::control::session_open::new::OpenDirErr;
}

impl crate::wire::typed::Handle for SessionOpenDir {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: crate::control::session_open::new::OpenDirReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<
        crate::control::session_open::new::DirChecked,
        crate::control::session_open::new::OpenDirErr,
    > {
        let user_home = crate::platform::paths::home_dir();
        super::session_batch_face::with_deps_as(None, "", None, |d| {
            crate::control::session_open::new::dir_answer(&req, d, user_home.as_deref())
        })
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

/// 列模型清单那一发的期限（不联网的那一形，当场就出）。
const LIST_WITHIN: crate::platform::child::Deadline = crate::platform::child::Deadline::secs(5);

/// 照那一家的模型清单那一格（[`crate::agents::LaunchFace::model_list`]）跑 `program` 带那几个词，读回列出的模型。
/// 没声明 · 起不来 · 过了期限 · 退出非零 · 读不成 ⇒ `None`（照静态的那一张，见 [`offered_models`]）。
pub(crate) fn listed_models_via(
    face: &crate::agents::LaunchFace,
    program: &str,
) -> Option<Vec<crate::agents::ListedModel>> {
    let list = face.model_list?;
    #[expect(
        clippy::disallowed_methods,
        reason = "列模型清单：那一家声明的不联网 · 不用登录那条子命令（`codex debug models --bundled` 这一形），只读它打到标准输出的清单，argv 不过 shell"
    )]
    let child = crate::platform::child::Child::new(program).args(list.args);
    let out = child.run(LIST_WITHIN).ok()?;
    if !out.status.success() {
        return None;
    }
    (list.read)(&String::from_utf8_lossy(&out.stdout))
}

/// 生产那一份：照那一家的默认启动器跑（[`listed_models_via`]）。
pub(crate) fn listed_models(
    face: &crate::agents::LaunchFace,
) -> Option<Vec<crate::agents::ListedModel>> {
    listed_models_via(face, face.default_launcher)
}

/// 一家此刻可选的模型：清单读得成 ⇒ 照清单；读不成 · 没声明 ⇒ 静态的那一张；都没有 ⇒ 空。
pub(crate) fn offered_models(
    face: &crate::agents::LaunchFace,
    listed: &dyn Fn(&crate::agents::LaunchFace) -> Option<Vec<crate::agents::ListedModel>>,
) -> Vec<crate::agents::ListedModel> {
    listed(face).unwrap_or_else(|| {
        face.models
            .into_iter()
            .flatten()
            .map(crate::agents::ListedModel::of_choice)
            .collect()
    })
}

/// 一家的起会话事实（名字 · 可选的模型与权限档）；名字问 [`crate::agents::family_said_among`]（说不出 ⇒ 照实的那一句）；`kind` 认不出 ⇒ 两张表空。模型照 [`offered_models`]（`listed` 列清单）。
pub(crate) fn agent_facts(
    kind: &'static str,
    listed: &dyn Fn(&crate::agents::LaunchFace) -> Option<Vec<crate::agents::ListedModel>>,
) -> AgentFacts {
    facts_of(crate::agents::REGISTRY, kind, listed)
}

/// [`agent_facts`] 的本体（问哪张注册表是参数：判据喂替身家）。
pub(crate) fn facts_of(
    registry: &[crate::agents::Adapter],
    kind: &str,
    listed: &dyn Fn(&crate::agents::LaunchFace) -> Option<Vec<crate::agents::ListedModel>>,
) -> AgentFacts {
    let face = crate::agents::pick_kind_among(registry, Some(kind))
        .ok()
        .map(|(_, f)| f);
    let pick = |c: crate::agents::Choice| AgentPick {
        value: c.value.to_string(),
        text: (c.text)(),
        efforts: Vec::new(),
        default_effort: None,
    };
    AgentFacts {
        kind: kind.to_string(),
        text: crate::agents::family_said_among(registry, kind),
        models: face
            .map(|f| offered_models(&f, listed))
            .unwrap_or_default()
            .into_iter()
            .map(|m| AgentPick {
                value: m.value,
                text: m.text,
                // 缺省档只出列着的那几档之一（那一家说的不在里面 ⇒ 不出）。
                default_effort: m.default_effort.filter(|d| m.efforts.contains(d)),
                efforts: m
                    .efforts
                    .into_iter()
                    .map(|e| AgentPick {
                        text: copy_core::copy_text("beSessionModel.effort.item", &[("effort", &e)]),
                        value: e,
                        efforts: Vec::new(),
                        default_effort: None,
                    })
                    .collect(),
            })
            .collect(),
        model_default: face.and_then(|f| f.model_default).map(pick),
        model_note: face
            .and_then(|f| f.model_switch)
            .and_then(|m| m.note)
            .map(|n| n()),
        permission_modes: face
            .and_then(|f| f.permission)
            .map(|p| p.modes.iter().map(|m| pick(m.choice())).collect())
            .unwrap_or_default(),
    }
}

/// 各家记录里的工作目录 ⇒ 最近用过的那几个（新的在前，同一个目录只出一次；不出会话的那个目录不算）。
/// 显示形按 `home`（这台的家目录）写（[`crate::platform::paths::home_short`]）。
pub(crate) fn recent_dirs(
    items: &[(String, i64)],
    hidden: &dyn Fn(&str) -> bool,
    clock: &crate::common::time::TextClock,
    home: Option<&std::path::Path>,
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
            shown: crate::platform::paths::home_short(cwd, home),
            last_text: clock
                .text(u64::try_from(ms.div_euclid(1000)).unwrap_or(0))
                .0,
        })
        .collect()
}

/// 记录树那一家的项目 ＋ 合成历史那几家的会话 ⇒ `(工作目录, 最近一次)`。
fn record_dirs(home: impl crate::agents::IntoHomes) -> Vec<(String, i64)> {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    let home: &[crate::agents::AgentHome] = &home_v;
    // 读不出记录树 ⇒ 记一行、少这一家的目录（框里照常起，只是「最近的」少几项）。
    let projects = crate::history::listing::list_projects(home).unwrap_or_else(|e| {
        tracing::warn!("session-open-facts：读不出记录树的项目清单：{e}");
        None
    });
    let mut out: Vec<(String, i64)> = projects
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.project_path, p.last_activity_ms))
        .collect();
    for (kind, face) in crate::agents::history_faces() {
        let homes = crate::agents::homes_of(kind);
        out.extend(
            (face.sessions)(&homes)
                .into_iter()
                .map(|s| (s.cwd, s.mtime_ms)),
        );
    }
    out
}

/// `session-open-facts` 的入参：线上形住 `cmd-wire::open`（名字与回包挂在 `cmd_wire::Call` 上）。
pub(crate) use cmd_wire::open::OpenFactsReq;

crate::wire::typed::codes! {
    /// `session-open-facts` 的码。
    pub enum OpenFactsErr {
        /// 分叉源会话的记录找不着 / 记录树读不出。
        ForkFailed = "fork_failed",
    }
}

/// `session-open-facts`：起新会话那个框要的事实（只读，不写分支记录）。阻塞档：扫各家记录目录 · 问一次 tmux。
pub struct SessionOpenFacts;

impl crate::wire::typed::Command for SessionOpenFacts {
    crate::wire::typed::by_call!(OpenFactsReq);
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

            facts(&req, &cx.tz).map_err(|s| match s.code.as_deref() {
                Some("bad_args") => OpenFactsErr::BadArgs(s.said),
                _ => {
                    debug_assert_eq!(s.code.as_deref(), Some("fork_failed"));
                    OpenFactsErr::ForkFailed(s.said)
                }
            })
        }
    }
}

/// 分叉框顶那一句（[`ForkFacts::from_text`] 的唯一一处）：说得出第几轮 ⇒ 带轮次与钟面；说不出 ⇒ 只点名源会话。
pub(crate) fn fork_from_text(title: &str, turn: Option<u64>, start_text: Option<&str>) -> String {
    match turn {
        Some(n) => copy_core::copy_text(
            "newSession.fork.from",
            &[
                ("title", title),
                ("n", &n.to_string()),
                ("time", start_text.unwrap_or("")),
            ],
        ),
        None => crate::observe::recognize::facts::forked_line(title).0,
    }
}

/// 分叉框顶那一句的接线：源会话叫什么与历史清单 · 标签页同一条规则（`custom` ＝ 这台注解里改过的名 ＞ 记录里的标题 · 第一句）；
/// 记录里的那几样照按 sid 找到它的那一家（`at.kind`）读，不照路径另认；`registry` 里没有那一家 ⇒ 说不清；
/// `turn` ＝ 那一条在第几轮 ＋ 那一轮开头的时刻，钟面按 `tz` 写。
pub(crate) fn fork_line_for(
    registry: &[crate::agents::Adapter],
    at: &crate::agents::SessionAt,
    custom: Option<String>,
    turn: Option<(u64, String)>,
    tz: &crate::common::time::Tz,
) -> String {
    let title = crate::history::listing::session_title_as(registry, at.kind, &at.file, custom);
    let start_text = turn
        .as_ref()
        .and_then(|t| crate::common::time::iso_hm(&t.1, tz));
    fork_from_text(&title, turn.map(|t| t.0), start_text.as_deref())
}

/// `session-open-facts` 的本体：`{forkOf?, at?}` ⇒ `{recent, hosts, run, agents, fork}`。
/// `tz` ＝ 看的那一台的时区：分叉那一轮的起点钟面按它写。
pub(crate) fn facts(
    req: &OpenFactsReq,
    tz: &crate::common::time::Tz,
) -> Result<OpenFacts, crate::common::said::Said> {
    let fork_of = match req.fork_of.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) => Some(s),
        Some(_) => {
            return Err(crate::common::said::Said::new(
                "bad_args",
                crate::common::contract::malformed("`forkOf` must be a session id"),
            ))
        }
    };
    let at = match req.at.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) && fork_of.is_some() => Some(s),
        Some(_) => {
            return Err(crate::common::said::Said::new(
                "bad_args",
                crate::common::contract::malformed(
                    "`at` must be a message id, given with `forkOf`",
                ),
            ))
        }
    };
    let home = crate::agents::local_homes();
    let fork = match fork_of {
        None => None,
        Some(sid) => {
            // 分叉那一家照按 sid 找到它的那一家认（不照路径另认：落在另一家记录根下也认得出）。
            let found = crate::agents::find_session_in(&home, sid)
                .map_err(|m| crate::common::said::Said::new("fork_failed", m))?;
            let source = found.file.clone();
            // 第几轮照那一份的扫描图数（与 `history-turns` 同一张：回退掉的那几句不算一轮）；那一家照找到它的那一家。
            let family = crate::observe::record_scan::Family::of(found.kind);
            let turn = at.and_then(|uuid| {
                crate::observe::record_scan::global()
                    .get(&source, family)
                    .ok()?
                    .turn_at(uuid)
            });
            let start_text = turn
                .as_ref()
                .and_then(|t| crate::common::time::iso_hm(&t.1, tz));
            let custom = match crate::history::history_annotations::load() {
                crate::history::history_annotations::Loaded::Read(t) => {
                    t.get(sid).and_then(|e| e.custom_title.clone())
                }
                _ => None,
            };
            Some(ForkFacts {
                agent: found.kind.into(),
                launch: super::fork_face::launch_of(&home, &found, sid),
                from_text: fork_line_for(crate::agents::REGISTRY, &found, custom, turn.clone(), tz),
                turn: turn.as_ref().map(|t| t.0),
                start_text,
            })
        }
    };
    let tmux = !matches!(
        super::session_batch_face::tmux_rows(),
        Ok(crate::control::session_batch::Listed { tmux: false, .. })
    );
    let managed = crate::control::session_batch::hosts_with(crate::host::managed::installed());
    let run = facts_run(fork.as_ref().and_then(|f| hosted(&f.launch)), tmux, managed);
    Ok(OpenFacts {
        recent: recent_dirs(
            &record_dirs(&home),
            &crate::history::listing::hidden_cwd,
            &crate::common::time::TextClock::new(crate::common::time::now_secs(), tz),
            crate::platform::paths::home_dir().as_deref(),
        ),
        hosts: facts_hosts(tmux, managed),
        run,
        agents: launchable_here(&launcher_found)
            .into_iter()
            .map(|k| agent_facts(k, &listed_models))
            .collect(),
        fork,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_open_face_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/faces/launch_readers_tests.rs"]
mod launch_readers_tests;
