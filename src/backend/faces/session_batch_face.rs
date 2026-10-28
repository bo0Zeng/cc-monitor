//! 帧面 `sessions-where` / `sessions-stop` 的宿主壳（`session-open` · 换号重启起会话要的那几样也由它拼）：把本体（`control/session_batch.rs`）要的那几样拼成生产那一份交进去。
//!
//! 本体住 `control/`，而 tmux 名单与记录在不在住观测层（`control → observe` 是反向边）⇒ 由这层顶层壳补上，与 `fork_face` 同形。
//! 名单 = 终端名单那一趟（`batch::listed_on`，挂着 sid 的窗格各一行；停两种宿主都认，别的只认 tmux）· 记录在不在 = `history-record` 那一问 · 结束 = `terminal-end` 那一条（按名单句柄）·
//! 就地键入 = `launch send-into`（带 sid ⇒ 落在挂着它的那个窗格）· 铸名 = `control::ccm::terminal_name_mint_with` · 起会话 = 这台后端自己当 ccm 跑那一行 ·
//! 谁在写 = 这台 pidfile 里持着那个 sid 的活进程（活会话表 `history::live_table::LiveTable::writers`）。

use crate::control::session_batch::{self as batch, Deps, TmuxEntry};
use crate::faces::launch_account_face::{pretrust_with, Files};

use crate::wire::typed::{self, Command};

/// `client` ＝ 请求自报的前端（「哪个前端的会话」那一维）：结束与就地键入都带它过门。`agent` ＝ 批量起那一家（整批一格；停 / 问样子用不着挑号）。
/// `list` ＝ 认会话的名单（问样子只认 tmux · 停两种宿主都认）。
fn with_deps<T>(
    client: Option<&str>,
    list: Listing,
    f: impl FnOnce(&Deps) -> Result<T, batch::CmdErr>,
) -> Result<T, BatchErr> {
    let client = crate::control::gate::requester(client).map_err(BatchErr::from)?;
    with_deps_on(list, client.as_deref(), "", None, f).map_err(BatchErr::from)
}

/// 认会话的名单那一发。
type Listing = fn() -> Result<Option<Vec<TmuxEntry>>, batch::CmdErr>;

/// 同 [`with_deps`]，前端与那一家已经取好（换号重启每一步在自己的阻塞线程上各拼一份）；名单只认 tmux（起会话 · 就地键入只对 tmux 下手）。
/// `files` ＝ 起之前预标信任经它写；`None` ＝ 这一趟不起会话（问样子 · 停 · 核目录）。
pub(crate) fn with_deps_as<T>(
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    with_deps_on(tmux_rows, client, agent, files, f)
}

fn with_deps_on<T>(
    list: Listing,
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    let record = |sid: &str, dir: Option<&str>| -> Result<(bool, String), String> {
        crate::observe::query::locate::record_for(&crate::history::listing::agent_home(), dir, sid)
            .map(|p| (p.present, p.root))
    };
    let end = |terminal: &str| {
        let req = crate::control::kill::TerminalEndReq {
            terminal: Some(terminal.to_string()),
            sid: None,
            client: client.map(str::to_string),
        };
        crate::control::kill::end_terminal(
            crate::host::tmux::On::default(),
            crate::host::managed::installed(),
            &req,
        )
        .map(|_| ())
    };
    let send_into = |name: &str, sid: &str, line: &str| {
        let req = crate::control::launch::parse_request(crate::control::launch::LaunchReq {
            mode: crate::control::launch::Mode::SendInto,
            name: name.to_string(),
            payload: line.to_string(),
            cwd: None,
            ccm_sid: Some(sid.parse::<cmd_wire::Sid>().map_err(|e| {
                crate::control::launch::CmdErr::from((
                    "bad_args",
                    crate::common::contract::malformed(&e.to_string()),
                ))
            })?),
            agent: None,
            width: None,
            height: None,
            client: client.map(str::to_string),
        })?;
        crate::control::launch::run(&req)
            .map(|_| ())
            .map_err(|e| e.fail)
    };
    let run_ccm = |argv: &[String]| crate::control::session_open::run_self_as_ccm(argv);
    let mint = |base: batch::NameBase| {
        crate::control::ccm::terminal_name_mint_with(base, crate::host::snapshot())
    };
    let writers = |sid: &str| {
        crate::history::live_table::now(&crate::history::listing::agent_home()).writers(sid)
    };
    let pretrust = |dir: &str, cwd: &str| pretrust_with(files, agent, dir, cwd);
    let open_shell =
        |name: &str, cwd: Option<&str>| crate::control::launch::open_shell(name, cwd, client);
    let caps = crate::ccm_launcher_with(crate::platform::tmux::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    // 这台的偏好（启动命令 · 模型表）：这一趟现读一次。
    let prefs = crate::common::machine_prefs::read_now().prefs();
    crate::faces::launch_account_face::with_facts(agent, |accounts| {
        f(&Deps {
            list: &list,
            record: &record,
            end: &end,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            host: crate::control::launch_render::Host::PRODUCTION,
            accounts,
            writers: &writers,
            pretrust: &pretrust,
            open_shell: &open_shell,
            prefs: &prefs,
        })
    })
}

/// 这台的 tmux 名单（挂着 sid 的窗格各一行）；`Ok(None)` ＝ 这台没装 tmux；`Err` ＝ 看不见。分叉那一格也读它。
pub(crate) fn tmux_rows() -> Result<Option<Vec<TmuxEntry>>, (&'static str, String)> {
    batch::listed_on(crate::host::tmux::On::default(), None)
}

/// 停认的名单：两种宿主都认（托管那一行按句柄交 `terminal-end`）。
fn both_hosts_rows() -> Result<Option<Vec<TmuxEntry>>, (&'static str, String)> {
    batch::listed_on(
        crate::host::tmux::On::default(),
        crate::host::managed::installed(),
    )
}

typed::codes! {
    /// `sessions-where` · `sessions-stop` 的码（两条同一份：都先列一次这台的 tmux 名单）。
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

impl crate::wire::typed::Handle for SessionsWhere {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: batch::SidsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<batch::Whereabouts, BatchErr>> + Send + 'static
    {
        async move { with_deps(req.client.as_deref(), tmux_rows, |d| batch::where_(&req, d)) }
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

impl crate::wire::typed::Handle for SessionsStop {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: batch::SidsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<batch::Batched, BatchErr>> + Send + 'static {
        async move {
            with_deps(req.client.as_deref(), both_hosts_rows, |d| {
                batch::stop(&req, d)
            })
        }
    }
}
