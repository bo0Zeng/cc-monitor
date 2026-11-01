//! `set-plan`（补足一组会话的预览）· `set-launch`（照补足起）：判定住 `control::session_set`；这里交事实与手 ——
//! 配置文件（经这台的文件管理面读）· 这台此刻活着的会话（活会话表：活着的交互进程，后台任务不算）· 每条经哪条别名起的
//! （启动台账 `aliases`）· 目录在不在 · 刚起、还没报到的那几条（本进程记着，[`PENDING_MS`] 内算「正在启动」）；
//! 起一条走 `session-open` 那一份本体（先定 sid 那一下换成「定 sid ＋ 写好组注解」）。

use crate::control::session_open as open;
use crate::control::session_set::{
    self as set, answer, GroupRef, Hands, Live, SetLaunchReq, SetLaunched, SetPlan, SetPlanErr,
    SetPlanReq,
};
use crate::wire::spec::Settle as _;
use crate::wire::typed::{of_fail_with, Command};
use cmd_wire::failed::Failed;
use std::sync::Mutex;

/// 刚起、还没报到的那几条算「正在启动」多久（同单个起等报到的期限）。
const PENDING_MS: i64 = 20_000;

/// 刚起、还没报到的那几条：`(会话 id, 目录, 别名, 起的时刻毫秒)`。只在本进程（常驻后端）记，补足连点两下不多起。
static PENDING: Mutex<Vec<(String, String, String, i64)>> = Mutex::new(Vec::new());

fn pending() -> std::sync::MutexGuard<'static, Vec<(String, String, String, i64)>> {
    PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 这台此刻活着的会话（交互进程、说得出会话 id 与目录的），各带台账里记的别名；再加上刚起、还没报到的那几条。
fn live_here() -> Vec<Live> {
    let table = crate::history::live_table::now(&crate::history::listing::agent_home());
    let aliases = crate::platform::paths::data_home()
        .and_then(|d| crate::history::launch_book::read_book(&d).ok())
        .map(|b| b.aliases)
        .unwrap_or_default();
    let mut out: Vec<Live> = Vec::new();
    for r in table
        .rows()
        .iter()
        .filter(|r| r.alive && !r.fact.background)
    {
        let (Some(sid), Some(cwd)) = (r.sid(), r.fact.cwd.as_deref()) else {
            continue;
        };
        // 同一条会话几个进程持着只算一个。
        if out.iter().any(|l| l.sid == sid) {
            continue;
        }
        out.push(Live {
            sid: sid.to_string(),
            cwd: cwd.to_string(),
            alias: aliases.get(sid).cloned(),
        });
    }
    let now = crate::common::time::now_ms();
    let mut p = pending();
    p.retain(|(sid, ..)| !out.iter().any(|l| &l.sid == sid));
    p.retain(|(.., at)| now - *at < PENDING_MS);
    for (sid, cwd, alias, _) in p.iter() {
        out.push(Live {
            sid: sid.clone(),
            cwd: cwd.clone(),
            alias: Some(alias.clone()),
        });
    }
    out
}

/// 这台没有 tmux ⇒ `no_tmux`（一组只起在 tmux 里：这台自己当 ccm 起、`--detach`）。探不出 ⇒ 照做，起的那一步再说。
fn no_tmux() -> Result<(), crate::common::said::Said> {
    match crate::control::terminals::tmux_here() {
        Some(false) => Err(crate::common::said::Said::new(
            crate::NO_TMUX,
            copy_core::copy_text("beSessionNew.place.noTmux", &[]),
        )),
        _ => Ok(()),
    }
}

fn is_dir(d: &str) -> bool {
    crate::platform::fs::Ro::metadata(d)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// `set-plan`：只读不写（读配置文件 · 活会话表 · 启动台账 ⇒ 阻塞档，带文件面）。
pub(crate) struct SetPlanCmd;

impl Command for SetPlanCmd {
    const NAME: &'static str = "set-plan";
    type Req = SetPlanReq;
    type Reply = SetPlan;
    type Err = SetPlanErr;
}

impl crate::wire::typed::Handle for SetPlanCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: SetPlanReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SetPlan, SetPlanErr>> + Send + 'static {
        let (_cx, files) = cx.split();
        async move {
            no_tmux().map_err(of_fail_with(SetPlanErr::Refused))?;
            let store = crate::assets::aliases::load(files).map_err(SetPlanErr::Refused)?;
            let home = std::path::PathBuf::from(&store.home);
            answer(&req, &store.book, &live_here(), Some(&home), &is_dir)
                .map_err(of_fail_with(SetPlanErr::Refused))
        }
    }
}

/// 起一组那一下就地拒的一条（`refused`）：那一句进交出去那一形，详情里的命令是 `set-launch`。
fn refused_launch(said: String) -> Failed {
    crate::wire::spec::fail("refused", said).settle(SetLaunchCmd::NAME)
}

/// 这一行合下来的那几格 ⇒ 起一条的 `session-open` 入参：哪一家 · 号 · 启动命令照别名（这里给的会盖过别名，所以照它给）；
/// 一律起在 tmux 里（`--detach`，这台自己当 ccm 跑）；那一行 ccm 以 `@别名` 打头，台账记下别名。
fn open_req(
    r: &crate::assets::aliases::profile::RowEdit,
    book: &crate::assets::aliases::profile::Book,
) -> Result<open::SessionOpenReq, Failed> {
    let resolved =
        crate::assets::aliases::profile::resolve(book, &r.alias, &[]).map_err(refused_launch)?;
    let crate::ccm_line::argv::Parsed::Opts(o) = resolved.parsed else {
        return Err(refused_launch(copy_core::copy_text(
            "beSessionSet.alias.missing",
            &[("alias", &r.alias)],
        )));
    };
    let account = if o.use_base {
        Some(crate::control::launch_account::AccountAsk::Base)
    } else if !o.account.is_empty() {
        Some(crate::control::launch_account::AccountAsk::Named {
            name: o.account.clone(),
        })
    } else {
        None
    };
    Ok(open::SessionOpenReq {
        what: open::Open::New(open::NewOpen {
            agent: crate::agents::AgentKind::new(o.agent.as_str()),
            cwd: r.cwd.clone(),
            tmux_name: None,
            command: (!o.launcher.is_empty()).then(|| o.launcher.clone()),
            fork: None,
            prompt: None,
            model: None,
            permission_mode: None,
            rotation: None,
            alias: Some(r.alias.clone()),
        }),
        account,
        run: Some(open::Launch::Tmux),
        local: false,
        ticket: None,
        client: None,
    })
}

/// 这台那一片里叫 `id` 的那一组还在不在（在 ⇒ 它此刻的样子）。
fn group_here(id: &str) -> Option<crate::history::groups::HereGroup> {
    let (_, book) = crate::history::history_annotations::load_all()?.ok()?;
    let g = book.get(id)?;
    Some(crate::history::groups::HereGroup {
        id: id.to_string(),
        name: g.name.name.clone(),
        made: g.made,
        named: g.name.named,
    })
}

/// `set-launch` 的本体：先算补足，再逐条起。
fn launch(
    req: &SetLaunchReq,
    files: super::launch_account_face::Files,
    own: &crate::platform::fs::Own,
) -> Result<SetLaunched, crate::common::said::Said> {
    use crate::history::groups::{self as groups, GroupsWriteReq, GroupsWritten, HereGroup};
    no_tmux()?;
    let store = crate::assets::aliases::load(files)
        .map_err(|m| crate::common::said::Said::new("refused", m))?;
    let home = std::path::PathBuf::from(&store.home);
    let live = live_here();
    let plan = answer(
        &SetPlanReq {
            name: Some(req.name.clone()),
            rows: None,
        },
        &store.book,
        &live,
        Some(&home),
        &is_dir,
    )?;
    let rows = store
        .book
        .find_set(&req.name)
        .map(|s| crate::assets::aliases::profile::edit_of_set(s).rows)
        .unwrap_or_default();
    let now = crate::common::time::now_ms;
    // 组那一笔：照这台那一片里那一组此刻的样子抄进组员的注解（[`groups::write_at`] 的 `move`）。
    let here_of = |g: &GroupRef| {
        group_here(&g.id).unwrap_or_else(|| HereGroup {
            id: g.id.clone(),
            name: g.name.clone(),
            made: now(),
            named: now(),
        })
    };
    let join = |sid: &str, g: &GroupRef| {
        let r = groups::write_at(
            own,
            GroupsWriteReq::Move {
                sid: sid.to_string(),
                join: Some(here_of(g)),
                leave: None,
            },
            now(),
        );
        if let Err(e) = r {
            use crate::wire::typed::Codes as _;
            tracing::warn!("起一组：{sid} 没归进组 {}：{}", g.id, e.into_said().said);
        }
    };
    let mut group_for = |name: &str| -> Result<GroupRef, String> {
        if let Some(h) = set::bound(own, name).and_then(|id| group_here(&id)) {
            return Ok(GroupRef {
                id: h.id,
                name: h.name,
            });
        }
        let made = groups::write_at(
            own,
            GroupsWriteReq::Make {
                name: name.to_string(),
            },
            now(),
        )
        .map_err(|e| {
            use crate::wire::typed::Codes as _;
            e.into_said().said
        })?;
        let GroupsWritten::Made(h) = made else {
            return Err(name.to_string());
        };
        set::bind(own, name, &h.id).map_err(|e| e.logged())?;
        Ok(GroupRef {
            id: h.id,
            name: h.name,
        })
    };
    // 本来就在、还散着的那几条也在这一下归进它那一行的组（只这一下，只动不在任何组里的）。
    let loose: Vec<(usize, String)> = {
        let table = crate::history::history_annotations::load_all()
            .and_then(Result::ok)
            .map(|(t, _)| t)
            .unwrap_or_default();
        rows.iter()
            .enumerate()
            .flat_map(|(i, r)| {
                set::members(r, &live, Some(&home))
                    .into_iter()
                    .filter(|l| table.get(&l.sid).is_none_or(|e| e.groups.is_empty()))
                    .map(move |l| (i, l.sid.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let leave = |sid: &str, g: &GroupRef| {
        let r = groups::write_at(
            own,
            GroupsWriteReq::Move {
                sid: sid.to_string(),
                join: None,
                leave: Some(g.id.clone()),
            },
            now(),
        );
        if let Err(e) = r {
            use crate::wire::typed::Codes as _;
            tracing::warn!("起一组：{sid} 没出组 {}：{}", g.id, e.into_said().said);
        }
    };
    // 先定的那几个 sid 各进了哪一组（那一家没用上先定的 ⇒ 照它摘）。
    let marked = std::cell::RefCell::new(Vec::<(String, GroupRef)>::new());
    let mut mark = |g: &GroupRef| -> Option<String> {
        let sid = crate::accounts::quota::rotation::new_session_id();
        join(&sid, g);
        marked.borrow_mut().push((sid.clone(), g.clone()));
        Some(sid)
    };
    let mut start = |r: &crate::assets::aliases::profile::RowEdit, sid: Option<&str>| {
        let req = open_req(r, &store.book)?;
        let facts = open::Facts {
            resume: None,
            fork_tmux: None,
            since: crate::common::arrival_seq::mark(),
        };
        let fixed = sid.map(str::to_string);
        let got = super::session_open_face::with_launch_ctx_here(own, |fork, pre, user_home| {
            let preset = || fixed.clone().unwrap_or_else(|| (pre.sid)());
            let pre = open::PreRotation {
                sid: &preset,
                write: pre.write,
                forget: pre.forget,
            };
            super::session_batch_face::with_deps_as(None, &req_agent(&req), Some(files), |d| {
                open::answer(req, facts, d, fork, &pre, user_home)
            })
        });
        let o = got.map_err(|s| s.settle(SetLaunchCmd::NAME))?;
        // 那一家不认先定 sid（没用上先定的那一个）⇒ 先写的那一笔摘掉，起好的那一条（说得出 sid 才）照归。
        if let Some(f) = fixed.as_deref().filter(|f| o.sid.as_deref() != Some(*f)) {
            if let Some(g) = marked
                .borrow()
                .iter()
                .find(|(s, _)| s == f)
                .map(|(_, g)| g.clone())
            {
                leave(f, &g);
                if let Some(s) = &o.sid {
                    join(s, &g);
                }
            }
        }
        if let Some(s) = &o.sid {
            pending().push((
                s.clone(),
                r.cwd.clone(),
                r.alias.clone(),
                crate::common::time::now_ms(),
            ));
        }
        Ok(o)
    };
    let mut unmark = |sid: &str, g: &GroupRef| leave(sid, g);
    let out = set::launch_rows(
        &rows,
        &plan,
        &mut Hands {
            group_for: &mut |name: &str| group_for(name).map_err(refused_launch),
            mark: &mut mark,
            start: &mut start,
            unmark: &mut unmark,
        },
    );
    for (i, sid) in loose {
        if let Some(g) = out.rows.get(i).and_then(|r| r.group.as_ref()) {
            join(&sid, g);
        }
    }
    Ok(out)
}

/// 入参里那一家（批量那一份 deps 按它备）。
fn req_agent(req: &open::SessionOpenReq) -> String {
    match &req.what {
        open::Open::New(n) => n.agent.as_str().to_string(),
        _ => String::new(),
    }
}

crate::wire::typed::codes! {
    /// `set-launch` 的码：没有那一组 · 那一组写错或配置文件读不了（那一句说清哪一样）。一条起不成不算整条失败（记在那一行里）。
    pub(crate) enum SetLaunchErr {
        /// 这台没有 tmux（一组只起在 tmux 里）。
        NoTmux = "no_tmux",
        NoSuchSet = "no_such_set",
        Refused = "refused",
    }
}

/// `set-launch`：照存好的那一组补足、逐条起（写组注解 · 起会话 ⇒ 阻塞档，带文件面与写句柄）。
pub(crate) struct SetLaunchCmd;

impl Command for SetLaunchCmd {
    const NAME: &'static str = "set-launch";
    type Req = SetLaunchReq;
    type Reply = SetLaunched;
    type Err = SetLaunchErr;
}

impl crate::wire::typed::Handle for SetLaunchCmd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: SetLaunchReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SetLaunched, SetLaunchErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);
            launch(&req, files, own).map_err(of_fail_with(SetLaunchErr::Refused))
        }
    }
}
