//! 帧面 `sessions-where` / `sessions-stop` / `sessions-start` 的宿主壳：把本体（`control/session_batch.rs`）要的那几样拼成生产那一份交进去。
//!
//! 本体住 `control/`，而 tmux 名单与记录在不在住观测层（`control → observe` 是反向边）⇒ 由这层顶层壳补上，与 `fork_face` 同形。
//! tmux 名单 = 终端名单那一趟（挂着 sid 的窗格各一行）· 记录在不在 = `history-record` 那一问 · 杀 = `kill`（按 sid 认窗格）·
//! 就地键入 = `launch send-into`（带 sid ⇒ 落在挂着它的那个窗格）· 铸名 = `terminal-name-mint` · 起会话 = 这台后端自己当 ccm 跑那一行 ·
//! 谁在写 = 这台 pidfile 里持着那个 sid 的活进程（`observe::accounts_query::session_writers`）。

use crate::control::session_batch::{self as batch, Deps, TmuxEntry};
use crate::faces::launch_face::{pretrust_with, Files};

use crate::wire::typed::{self, Blocking, Command, WithFiles};

/// `client` ＝ 请求自报的前端（「哪个前端的会话」那一维）：杀与就地键入都带它过门。`agent` ＝ 批量起那一家（整批一格；停 / 问样子用不着挑号）。
fn with_deps<T>(
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> Result<T, batch::CmdErr>,
) -> Result<T, BatchErr> {
    let client = crate::control::gate::requester(client).map_err(BatchErr::from)?;
    with_deps_as(client.as_deref(), agent, files, f).map_err(BatchErr::from)
}

/// 同 [`with_deps`]，前端与那一家已经取好（换号重启每一步在自己的阻塞线程上各拼一份）。
/// `files` ＝ 起之前预标信任经它写；`None` ＝ 这一趟不起会话（问样子 · 停 · 核目录）。
pub(crate) fn with_deps_as<T>(
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    let list = tmux_rows;
    let record = |sid: &str, dir: Option<&str>| -> Result<(bool, String), String> {
        crate::observe::history_query::record_for(&crate::history::listing::agent_home(), dir, sid)
            .map(|p| (p.present, p.root))
    };
    let kill =
        |name: &str, sid: &str| crate::control::kill::run_as(name, sid, client).map(|b| b.note());
    let send_into = |name: &str, sid: &str, line: &str| {
        let req = crate::control::launch::parse_request(crate::control::launch::LaunchReq {
            mode: crate::control::launch::Mode::SendInto,
            name: name.to_string(),
            payload: line.to_string(),
            cwd: None,
            ccm_sid: Some(sid.to_string()),
            agent: None,
            width: None,
            height: None,
            client: client.map(str::to_string),
        })?;
        crate::control::launch::run(&req)
            .map(|_| ())
            .map_err(|e| e.fail)
    };
    let run_ccm = |argv: &[String]| crate::control::session_batch::run_self_as_ccm(argv);
    let mint = |base: batch::NameBase| {
        crate::control::ccm::terminal_name_mint_with(
            base,
            crate::common::session_snapshot::global(),
        )
        .map(|m| m.name)
    };
    let writers = |sid: &str| {
        crate::observe::accounts_query::session_writers(&crate::history::listing::agent_home(), sid)
    };
    let pretrust = |dir: &str, cwd: &str| pretrust_with(files, agent, dir, cwd);
    let caps = crate::ccm_launcher_with(crate::platform::tmux::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    // 这台的偏好（启动命令 · 模型表）：这一趟现读一次。
    let prefs = crate::control::machine_prefs::read_now().prefs();
    crate::faces::launch_face::with_facts(agent, |accounts| {
        f(&Deps {
            list: &list,
            record: &record,
            kill: &kill,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            local_facts: crate::control::launch_render::local::Facts::PRODUCTION,
            accounts,
            writers: &writers,
            pretrust: &pretrust,
            prefs: &prefs,
        })
    })
}

/// 这台的 tmux 名单（挂着 sid 的窗格各一行）；`Ok(None)` ＝ 这台没装 tmux；`Err` ＝ 看不见。分叉那一格也读它。
pub(crate) fn tmux_rows() -> Result<Option<Vec<TmuxEntry>>, (&'static str, String)> {
    // 码只留「过了期限」这一种（`listed` 按它整条回超时）；别的列不成一律当看不见。
    let rows = crate::control::terminals::rows_here().map_err(|f| {
        let code = if f.code == crate::platform::child::TIMED_OUT {
            crate::platform::child::TIMED_OUT
        } else {
            "failed"
        };
        (code, f.into_note())
    })?;
    Ok(rows.map(|rows| {
        rows.into_iter()
            .map(|r| TmuxEntry {
                agent: crate::agents::is_agent_process(&r.program),
                terminal: r.handle(),
                sid: (!r.sid.is_empty()).then_some(r.sid),
                name: r.name,
            })
            .collect()
    }))
}

typed::codes! {
    /// `sessions-where` · `sessions-stop` · `sessions-start` 的码（三条同一份：都先列一次这台的 tmux 名单）。
    pub(crate) enum BatchErr {
        /// 列不出这台的 tmux 名单。
        Unobservable = "unobservable",
        /// 列名单那一次 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

impl From<batch::CmdErr> for BatchErr {
    /// 下层只回这三个码：形状 / 组合不对 · 列名单过了期限 · 列不出（`batch::listed` 把别的都收成 `unobservable`）。
    fn from((code, said): batch::CmdErr) -> Self {
        match code {
            "bad_args" => BatchErr::BadArgs(said),
            crate::platform::child::TIMED_OUT => BatchErr::ChildTimedOut(said),
            _ => {
                debug_assert_eq!(code, "unobservable");
                BatchErr::Unobservable(said)
            }
        }
    }
}

/// `sessions-where`：一批会话各在这台哪个终端里（菜单就绪时问）。
pub(crate) struct SessionsWhere;

impl Command for SessionsWhere {
    const NAME: &'static str = "sessions-where";
    type Req = batch::SidsReq;
    type Reply = batch::Whereabouts;
    type Err = BatchErr;
}

impl Blocking for SessionsWhere {
    fn run(req: batch::SidsReq, _cx: &typed::Cx) -> Result<batch::Whereabouts, BatchErr> {
        with_deps(req.client.as_deref(), "", None, |d| batch::where_(&req, d))
    }
}

/// `sessions-stop`：停一批会话（逐个答，一个不成不挡下一个）。
pub(crate) struct SessionsStop;

impl Command for SessionsStop {
    const NAME: &'static str = "sessions-stop";
    type Req = batch::SidsReq;
    type Reply = batch::Batched;
    type Err = BatchErr;
}

impl Blocking for SessionsStop {
    fn run(req: batch::SidsReq, _cx: &typed::Cx) -> Result<batch::Batched, BatchErr> {
        with_deps(req.client.as_deref(), "", None, |d| batch::stop(&req, d))
    }
}

/// `sessions-start`：起 / 接回一批会话（逐个答）。
pub(crate) struct SessionsStart;

impl Command for SessionsStart {
    const NAME: &'static str = "sessions-start";
    type Req = batch::SessionsStartReq;
    type Reply = batch::Batched;
    type Err = BatchErr;
}

impl WithFiles for SessionsStart {
    fn run(
        req: batch::SessionsStartReq,
        _cx: &typed::Cx,
        files: Files,
    ) -> Result<batch::Batched, BatchErr> {
        with_deps(req.client.as_deref(), &req.agent, Some(files), |d| {
            batch::start(&req, d)
        })
    }
}
