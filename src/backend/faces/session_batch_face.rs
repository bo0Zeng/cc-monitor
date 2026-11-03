//! 帧面 `sessions-where` / `sessions-stop` 的宿主壳（`session-open` · 换号重启起会话要的那几样也由它拼）：把本体（`control/session_batch.rs`）要的那几样拼成生产那一份交进去。
//!
//! 本体住 `control/`，而 tmux 名单与记录在不在住观测层（`control → observe` 是反向边）⇒ 由这层顶层壳补上，与 `fork_face` 同形。
//! 名单 = 终端名单那一趟（`batch::listed_on`，挂着 sid 的窗格各一行；问样子 · 停两种宿主都认，起会话 · 分叉 · 就地键入只认 tmux）· 记录在不在 = `history-record` 那一问 · 结束 = `terminal-end` 那一条（按名单句柄）·
//! 就地键入 = `launch send-into`（带 sid ⇒ 落在挂着它的那个窗格）· 铸名 = `control::ccm::terminal_name_mint_with` · 起会话 = 这台后端自己当 ccm 跑那一行 ·
//! 谁在写 = 这台 pidfile 里持着那个 sid 的活进程（活会话表 `history::live_table::LiveTable::writers`）·
//! 停之前问的会话脸与称呼 = [`with_stop_faces`]（活会话表 ＋ 名单那一格 ⇒ `SessionFaceExt::of`；称呼照 `search_rules::name_face`）。

use crate::control::session_batch::{self as batch, Deps, Listed};
use crate::faces::launch_account_face::{pretrust_with, Files};

use crate::wire::typed::{self, Command};

/// `client` ＝ 请求自报的前端（「哪个前端的会话」那一维）：结束与就地键入都带它过门。`agent` ＝ 批量起那一家（整批一格；停 / 问样子用不着挑号）。
/// 名单两种宿主都认（问样子 · 停：托管那一行按句柄交 `terminal-end`）。
fn with_deps<T>(
    client: Option<&str>,
    f: impl FnOnce(&Deps) -> Result<T, crate::common::said::Said>,
) -> Result<T, BatchErr> {
    let client = crate::control::gate::requester(client).map_err(BatchErr::from)?;
    with_deps_on(both_hosts_rows, client.as_deref(), "", None, f).map_err(BatchErr::from)
}

/// 认会话的名单那一发。
type Listing = fn() -> Result<Listed, crate::common::said::Said>;

/// 同 [`with_deps`]，前端与那一家已经取好（换号重启每一步在自己的阻塞线程上各拼一份）；名单只认 tmux（起会话 · 就地键入只对 tmux 下手）。
/// `files` ＝ 起之前预标信任经它写；`None` ＝ 这一趟不起会话（问样子 · 停 · 核目录）。
pub fn with_deps_as<T>(
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    with_deps_on(tmux_rows, client, agent, files, f)
}

/// 同 [`with_deps_as`]，名单两种宿主都认：接回（`session-open look`）只交出接进去那一行，托管那一行照句柄接，不往里键入。
pub(crate) fn with_deps_both_as<T>(
    client: Option<&str>,
    agent: &str,
    files: Option<Files>,
    f: impl FnOnce(&Deps) -> T,
) -> T {
    with_deps_on(both_hosts_rows, client, agent, files, f)
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
                crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed(&e.to_string()),
                )
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
    let caps = crate::control::ccm::ccm_launcher_with(crate::platform::tmux::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    // 这台的偏好（启动命令 · 模型表 · 恢复时放在哪）：这一趟现读一次。
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
            started: &crate::faces::launch_account_face::host_of,
        })
    })
}

/// 这台的 tmux 名单（挂着 sid 的窗格各一行）；`Err` ＝ 看不见。分叉那一格也读它。
pub(crate) fn tmux_rows() -> Result<Listed, crate::common::said::Said> {
    batch::listed_on(crate::host::tmux::On::default(), None)
}

/// 问样子 · 停认的名单：两种宿主都认（托管那一行按句柄交 `terminal-end`）。
fn both_hosts_rows() -> Result<Listed, crate::common::said::Said> {
    batch::listed_on(
        crate::host::tmux::On::default(),
        crate::host::managed::installed(),
    )
}

typed::codes! {
    /// `sessions-where` · `sessions-stop` 的码（两条同一份：都先列一次这台的 tmux 名单）。
    pub enum BatchErr {
        /// 列不出这台的 tmux 名单。
        Unobservable = "unobservable",
        /// 列名单那一次 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

impl From<crate::common::said::Said> for BatchErr {
    /// 下层只回这三个码：形状 / 组合不对 · 列名单过了期限 · 列不出（`batch::listed` 把别的都收成 `unobservable`）。
    fn from(f: crate::common::said::Said) -> Self {
        match f.code.as_deref() {
            Some("bad_args") => BatchErr::BadArgs(f.said),
            Some(crate::platform::child::TIMED_OUT) => BatchErr::ChildTimedOut(f.said),
            code => {
                debug_assert_eq!(code, Some("unobservable"));
                BatchErr::Unobservable(f.said)
            }
        }
    }
}

/// `sessions-where`：一批会话各在这台哪个终端里（菜单就绪时问）。
pub struct SessionsWhere;

impl Command for SessionsWhere {
    const NAME: &'static str = "sessions-where";
    type Req = batch::SidsReq;
    type Reply = batch::Whereabouts;
    type Err = BatchErr;
}

impl crate::wire::typed::Handle for SessionsWhere {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: batch::SidsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<batch::Whereabouts, BatchErr> {
        with_deps(req.client.as_deref(), |d| batch::where_(&req, d))
    }
}

/// `sessions-stop`：停一批会话（逐个答，一个不成不挡下一个）。
pub struct SessionsStop;

impl Command for SessionsStop {
    const NAME: &'static str = "sessions-stop";
    type Req = batch::SidsReq;
    type Reply = batch::Batched;
    type Err = BatchErr;
}

impl crate::wire::typed::Handle for SessionsStop {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: batch::SidsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<batch::Batched, BatchErr> {
        with_deps(req.client.as_deref(), |d| {
            with_stop_faces(|faces| batch::stop(&req, d, faces))
        })
    }
}

/// 停之前问的会话脸与称呼（生产那一份）：活不活照这台的活会话表（`live_table::life_of`），容器照这台名单 ⇒ 会话脸（`SessionFaceExt::of`）上 `can.end` 那一格；
/// 称呼照 `sessions_batch_face::title_here`（一批会话各样共用那一份）。活会话表与注解一趟各读一次。
fn with_stop_faces<T>(f: impl FnOnce(&batch::StopFaces) -> T) -> T {
    use crate::wire::{Life, SessionFace, SessionFaceExt as _, WireLife};
    let home = crate::history::listing::agent_home();
    let live = crate::history::live_table::now(&home);
    let notes = crate::faces::sessions_batch_face::notes();
    let can_end = |sid: &str, c: &crate::wire::SessionContainer| {
        let life = match live.life_of(sid) {
            WireLife::Live { activity } => Life::live(activity, Some(c)),
            other => Life::of_wire(&other),
        };
        SessionFace::of(life).can.end
    };
    let title = |sid: &str| crate::faces::sessions_batch_face::title_here(&home, &notes, sid);
    f(&batch::StopFaces {
        can_end: &can_end,
        title: &title,
    })
}
