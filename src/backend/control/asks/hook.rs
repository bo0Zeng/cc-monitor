//! `ccm -- --answer-hook <agent>`：会话级答题 hook 的客户端（那一家的 hook 设置里写的就是它，exec 形、不经 shell）。
//!
//! 读 hook 的 stdin，照那一家的答题口（`AskFace`）认是哪一种（[`route`]），都经 `needs-ask` 交这台家里的常驻后端（连法同 `--resident-attach`）：
//! - 批准 · 提问 · 批计划（要人答的那一种），不挡终端的（主会话那种：框与 hook 同时出）⇒ 挂上、阻塞等 → 拿到人的答案，
//!   把常驻交回的那一份原样打到 stdout、退 0；那一份是那一家 `decide` 写的（允许 · 拒绝 · 答了的提问各是那一家要的那一形），这里不拼。
//! - 挡着终端的那一种（`AskIn::blocks_terminal`：那一家先跑 hook、hook 没给决定才弹框）⇒ 交给常驻就走、不等回包、一个字节都不打：
//!   框当场在终端里出（同没装 cc-monitor）；常驻记下这一问，认得出屏上那个框就给按钮（经读屏按键答到终端里），答没答由常驻那一侧判（`control::asks`）。
//! - 那一家不给经钩子答的那一种（`AskFace::cannot`，比如某一家的提问）⇒ 交给常驻就走、不等：常驻不挂、记这条会话需手动（码 `NoAnswer`），
//!   那一家照常在终端里问。
//! - 提交了一句 · 一轮结束 · 会话起来（同一组 hook 里一轮的那几种，`AskFace::heard`）⇒ 交给常驻就走（记进活会话表）。
//!
//! **铁律：没收到人的答案，一个字节都不打**（＝不给决定，框照常在终端里问）：
//! 常驻不在 · 断开 · 握手不对 · 不是要人答的那一种 · 连接断了 —— 一律空手退 0。
//!
//! 终端那边先答了，那一家不会给 hook 任何信号（实测：进程不被杀、写 stdout 也照样成功）⇒ 这里自己判：
//! 盯会话进程的状态文件，见过「在等」之后离开「在等」，或状态更新晚于自己起来那一刻而不在「在等」⇒ 判关了、空手退
//! （连接一断，常驻那一侧把挂着的那条撤掉）。只管挂着等的那一种；挡着终端的那一种这里早已走了。
//! 零定时器：只等套接字与文件事件。

use crate::agents::SessionActivity;
use cap_core::{Cap, Over};
use comms_inward::line::{read_capped_all, CappedAll};
use copy_core::{Discard, Why};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

/// stdin 上限（同 CLI 入参那一道）：超了整份拒。
const STDIN_CAP: Cap = Cap {
    bytes: 1_048_576,
    over: Over::Reject,
};
/// 这一问的请求 id（一条连接只问这一次）。
const REQ_ID: &str = "answer-hook";

/// 这条会话各个活进程此刻的（那一态, 这一态从何时起）：读 `home` 那一家的 pidfile 目录（`main` 交进来：活会话表那一处，`history::live_table::states_in`）。
pub(crate) type StatesOf =
    fn(&crate::agents::AgentHome, &str) -> Vec<(Option<SessionActivity>, Option<u64>)>;

/// 入口（`args[0]` ＝ 子命令本身，`args[1]` ＝ 那一家的 kind）。恒退 0：hook 的退出码那一家不当决定看，空手退就是「不答」。
/// `states` 判「终端那边答了」用（[`StatesOf`]）。
pub async fn run(args: &[String], states: StatesOf) -> i32 {
    let Some(agent) = args.get(1) else { return 0 };
    let Some(face) = crate::agents::ask_face(agent) else {
        return 0;
    };
    // 超了整份拒（不给决定，框照常在终端里问），不截断着用。
    let Ok(CappedAll::All(raw)) = read_capped_all(tokio::io::stdin(), STDIN_CAP).await else {
        return 0;
    };
    let Ok(hook) = serde_json::from_slice::<Value>(&raw) else {
        return 0;
    };
    // 要人答的那一种 ⇒ 挂上等；那一家答不了的 · 一轮的那几种 ⇒ 交给常驻就走。
    let route = route(&face, &hook);
    if route == Route::Skip {
        return 0;
    }
    // 经环境那一路来的 hook 子进程也继承得到：那一发要认是不是 ccm 起的那一个会话（会话里 Bash 起的 `claude -p` 不在这里挂着等）。
    // 认法在常驻那一侧（[`vouched`]：托管名单的家只在常驻里装着，包一层时便条在那一跳上），这里只交发问那个进程的 pid；
    // 说不出 pid ⇒ 认不出、空手退。
    let ours = crate::platform::paths::home_dir()
        .and_then(|h| crate::agents::asks::plugin_dir_in_env(&h))
        .unwrap_or_default();
    let asker = match face.plugin {
        Some(p) if from_env(std::env::var(p.env).ok().as_deref(), &ours) => match asking_pid() {
            Some(pid) => Some(pid),
            None => return 0,
        },
        _ => None,
    };
    let Some(dh) = crate::platform::paths::data_home() else {
        return 0;
    };
    if let Some(out) = deliver(&dh, agent, route, &hook, asker, states).await {
        let mut stdout = tokio::io::stdout();
        // hook 的 stdout 是那一家读决定的管子：写不进去 ＝ 那一家已经不读了（框已关 / 进程走了），晚到的答案它本来也丢
        stdout
            .write_all(out.to_string().as_bytes())
            .await
            .discard(Why::DeadLink);
        stdout.flush().await.discard(Why::DeadLink);
    }
    0
}

/// 交给 `data_home` 那一家里的常驻 ⇒ 要打到 stdout 的那一份（`None` ＝ 一个字节都不打）。
/// `asker` ＝ 经环境来的那一发里发问那个进程（常驻照它认是不是 ccm 起的那一个，[`vouched`]）；不是经环境来的 ⇒ `None`。
pub async fn deliver(
    data_home: &std::path::Path,
    agent: &str,
    route: Route,
    hook: &Value,
    asker: Option<u32>,
    states: StatesOf,
) -> Option<Value> {
    let line = request(agent, hook, asker);
    match route {
        Route::Hang(asked) if !asked.blocks_terminal => {
            let closed = watch_terminal(agent, &asked.sid, states);
            ask_resident(data_home, &line, closed).await
        }
        Route::Skip => None,
        // 挡着终端的 · 答不了的 · 一轮的那几种：交给常驻就走，不等回包。
        _ => {
            hand_over(data_home, &line).await;
            None
        }
    }
}

/// 只交、不等：连上、握手、把请求那一行写出去就丢连接（常驻那一侧照收照办）。
async fn hand_over(dh: &std::path::Path, line: &Value) {
    let Ok(mut here) = crate::control::resident::Here::open(dh).await else {
        return;
    };
    // 写不出去 ＝ 常驻那一头已经断了：这一份它本来也收不到，框照常在终端里问
    here.send(line).await.discard(Why::DeadLink);
}

/// `needs-ask` 那一行（一条连接只问这一次；`asker` 只在经环境来的那一发里带）。
fn request(agent: &str, hook: &Value, asker: Option<u32>) -> Value {
    let mut args = json!({ "agent": agent, "hook": hook });
    if let Some(pid) = asker {
        args["asker"] = json!(pid);
    }
    json!({ "id": REQ_ID, "cmd": "needs-ask", "args": args })
}

/// hook 交来的那一份是哪一种（纯函数；常驻那一侧 `needs-ask` 照同一个分）。
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// 要人答的那一种：挂上等。
    Hang(Box<crate::agents::AskIn>),
    /// 要人答、这一家却不给经钩子答的那一种（[`crate::agents::AskFace::cannot`]）：不挂，这条会话记需手动。
    Refused(String),
    /// 一轮的那几种（[`crate::agents::AskFace::heard`]）。
    Heard(String, crate::agents::AgentEvent),
    /// 认不出：空手退。
    Skip,
}

/// 照那一家的答题口分 hook 交来的那一份。
pub fn route(face: &crate::agents::AskFace, hook: &Value) -> Route {
    match ((face.read)(hook), (face.heard)(hook)) {
        (Some(a), _) if (face.cannot)(&a) => Route::Refused(a.sid),
        (Some(a), _) => Route::Hang(Box::new(a)),
        (None, Some((sid, event))) => Route::Heard(sid, event),
        (None, None) => Route::Skip,
    }
}

/// 常驻交回的那一行 ⇒ 要打到 stdout 的那一份（那一家 `decide` 写的，原样）；不成 · 不给决定 ⇒ `None`。
pub fn decision_of(reply: &Value) -> Option<Value> {
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    reply
        .pointer("/data/decision")
        .filter(|d| d.is_object())
        .cloned()
}

/// 经常驻后端问；拿到决定 ⇒ `Some`。`closed` 先到 ⇒ `None`（丢掉连接 ＝ 撤）。
async fn ask_resident(
    dh: &std::path::Path,
    line: &Value,
    closed: Option<(
        crate::platform::watch_file::Watching,
        tokio::sync::oneshot::Receiver<()>,
    )>,
) -> Option<Value> {
    // 连常驻 ＋ 握手（hello → attach 行）与 `--resident-call` 同一处（`control/resident.rs::Here`）。
    let mut here = crate::control::resident::Here::open(dh).await.ok()?;
    here.send(line).await.ok()?;
    let (_watching, mut closed) = match closed {
        Some((w, rx)) => (Some(w), Some(rx)),
        None => (None, None),
    };
    tokio::select! {
        v = here.reply(REQ_ID) => decision_of(&v.ok()?),
        r = async {
            match closed.as_mut() {
                Some(rx) => rx.await,
                None => std::future::pending().await,
            }
        } => {
            r.discard(Why::Signal); // 关了或发端被丢都算「框已关」
            None
        }
    }
}

/// 盯这条会话的状态文件，判「终端那边答了」：返回监听（活着才盯）与「关了」那一声。挂不上 ⇒ `None`（只能等人答或连接断）。
fn watch_terminal(
    agent: &str,
    sid: &str,
    states: StatesOf,
) -> Option<(
    crate::platform::watch_file::Watching,
    tokio::sync::oneshot::Receiver<()>,
)> {
    // 家取这个 hook 进程自己环境里那一家的：hook 是发问那个进程的子进程，环境（号 · 配置根）就是那条会话自己的。
    let a = crate::agents::adapter_among(crate::agents::REGISTRY, agent)?;
    let local = a.local?;
    let home = (local.home_at)(None, true);
    let at = SessionHome {
        dir: (local.pidfile_dir)(&home),
        home: crate::agents::AgentHome { kind: a.kind, home },
    };
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let w = watch_closed(
        &at,
        sid,
        crate::common::time::now_ms(),
        states,
        "answer-hook-status",
        move || {
            tx.send(()).discard(Why::PeerGone); // 等它的那一头已经拿到答案走了
        },
    )?;
    Some((w, rx))
}

/// 这条会话自己那一家的家（常驻那一侧用：常驻自己环境里那一家不一定是这条会话的号）：`registry` 里 `agent` 那一家各号的家
/// （这台环境里那一个 · 默认那一个 · 账号库里各号，[`crate::agents::homes_among`]）里，pidfile 目录里有活进程持着 `sid` 的头一个。
/// 字面上同一个 pidfile 目录只问一次（链过去的照字面各问一次，答的一样）。那一家没有本机布局 · 哪一个都没有 ⇒ `None`。
/// 与 `accounts::manage::library::session_home`（读进程环境里的配置根，要活会话表先看得见那个进程）不是一件事：
/// 这里要的是那个进程写状态文件的目录，活会话表看不见的号目录也找得到。
pub fn status_home(
    registry: &[crate::agents::Adapter],
    agent: &str,
    sid: &str,
    states: StatesOf,
) -> Option<SessionHome> {
    let a = crate::agents::adapter_among(registry, agent)?;
    let local = a.local?;
    let mut homes = vec![(local.home_at)(None, true), (local.home_at)(None, false)];
    homes.extend(
        crate::agents::homes_among(a)
            .into_iter()
            .map(|h| (local.home_at)(Some(&h.home), false)),
    );
    let mut asked: Vec<SessionHome> = Vec::new();
    for home in homes {
        let dir = (local.pidfile_dir)(&home);
        if asked.iter().any(|s| s.dir == dir) {
            continue;
        }
        let at = SessionHome {
            home: crate::agents::AgentHome { kind: a.kind, home },
            dir,
        };
        if !states(&at.home, sid).is_empty() {
            return Some(at);
        }
        asked.push(at);
    }
    None
}

/// 这条会话那一家的家与它的 pidfile 目录（目录照找它的那一张注册表里那一家的布局算好，盯的就是它）。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionHome {
    pub home: crate::agents::AgentHome,
    pub dir: std::path::PathBuf,
}

/// **盯状态文件判「终端那边答了」的唯一一处**（hook 挂着等的那一种 · 常驻记下的挡着终端的那一种都经它）：
/// 盯 `at` 那个 pidfile 目录，每有动静照 [`closed_now`] 判一次；判出关了 ⇒ 调一次 `on_closed`，之后不再判。
/// `started` ＝ 挂上那一刻。挂不上监听 ⇒ `None`。
pub(crate) fn watch_closed(
    at: &SessionHome,
    sid: &str,
    started: u64,
    states: StatesOf,
    thread: &str,
    on_closed: impl FnOnce() + Send + 'static,
) -> Option<crate::platform::watch_file::Watching> {
    let mut on_closed = Some(on_closed);
    let mut seen = Seen::default();
    let (sid, home) = (sid.to_string(), at.home.clone());
    let w = crate::platform::watch_file::watch(
        &[(at.dir.clone(), false)],
        |p| p.extension().and_then(|e| e.to_str()) == Some("json"),
        thread,
        move |_paths| {
            if on_closed.is_none() {
                return;
            }
            if closed_now(&states(&home, &sid), started, &mut seen) {
                if let Some(f) = on_closed.take() {
                    f();
                }
            }
        },
    )
    .ok()?;
    // 挂上之后先判一次此刻的样子（之前的那几次变动没赶上事件）。
    w.kick();
    Some(w)
}

/// 这条会话各进程的态里见过什么（[`closed_now`] 一次次记下）。
#[derive(Debug, Default)]
pub struct Seen {
    /// 见过这条会话的进程。
    alive: bool,
    /// 见过「在等」（框出来了）。
    waiting: bool,
}

/// **「终端那边答了」的唯一判法**（纯函数；hook 挂着等的那一种 · 常驻记下的挡着终端的那一种都照它）：
/// 有一个进程在「在等」⇒ 没关（记下见过）；都不在「在等」且（见过「在等」或状态更新晚于 `started`）⇒ 关了；
/// 见过进程、后来一个都没了（会话退了）⇒ 关了（框随它没了）。
/// 依据（Claude Code 2.1.322 源码）：状态文件的 `status` 照主会话那一个框队列的队头写，后台子 agent 的批准框也进这一个队列 ⇒ 框在 ⇒ `waiting`。
/// 「晚于 `started`」那一条接住答得快的：框出来又答掉落在同一阵动静里（或挂监听之前），见不到「在等」也判得出。
pub fn closed_now(
    states: &[(Option<SessionActivity>, Option<u64>)],
    started: u64,
    seen: &mut Seen,
) -> bool {
    if states.is_empty() {
        return seen.alive || seen.waiting;
    }
    seen.alive = true;
    if states
        .iter()
        .any(|(a, _)| *a == Some(SessionActivity::NeedsYou))
    {
        seen.waiting = true;
        return false;
    }
    seen.waiting || states.iter().any(|(_, at)| at.is_some_and(|t| t > started))
}

/// 发问的那个进程：hook 是它的直接子进程（exec 形，不经 shell）。说不出 ⇒ `None`。
fn asking_pid() -> Option<u32> {
    #[cfg(unix)]
    {
        Some(std::os::unix::process::parent_id())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// 这一发是不是经环境那一路来的（纯函数）：钩子自己环境里那个插件目录变量（`own_dirs`）带着我们的目录（`ours`）。
/// `--settings` 那一路不传给子进程 ⇒ 不带 ⇒ 不用再认。
pub fn from_env(own_dirs: Option<&str>, ours: &str) -> bool {
    own_dirs.is_some_and(|l| !ours.is_empty() && crate::agents::asks::dirs_have(l, ours))
}

/// 常驻那一侧：经环境来的那一发（`asker` ＝ 发问那个进程）认不认是 ccm 起的那一个 —— 托管名单的家只在常驻里装着，
/// 包一层时那一跳照它找得到（`host::managed::hop_of_pid`）。那一家不经环境交答题插件 ⇒ 认；认不出是哪条会话 ⇒ 不认。
pub(crate) fn vouched(face: &crate::agents::AskFace, route: &Route, asker: u32) -> bool {
    let Some(p) = face.plugin else { return true };
    let sid = match route {
        Route::Hang(a) => a.sid.as_str(),
        Route::Refused(sid) | Route::Heard(sid, _) => sid.as_str(),
        Route::Skip => return false,
    };
    let in_table = crate::platform::paths::data_home()
        .is_some_and(|data| in_table_among(&data, asker, sid, &crate::host::managed::hop_of_pid));
    admitted(
        in_table,
        crate::platform::proc::proc_env_var(asker, p.child_marker),
    )
}

/// 常驻那份「ccm 起的会话」表：照发问那个进程找得到 ccm 留的起会话便条（包一层时在那一跳上；`/clear` 换了会话 id 进程不变，
/// 照样认得）· 这个会话 id 记在起会话那份记录里。`data` ＝ 数据目录，`hop_of` ＝ 发问那个进程住的托管终端里记的那一跳
/// （便条照会话找只经 [`crate::control::launch_account::session_note`]）。
pub(crate) fn in_table_among(
    data: &std::path::Path,
    pid: u32,
    sid: &str,
    hop_of: &dyn Fn(u32) -> Option<crate::host::managed::Hop>,
) -> bool {
    crate::control::launch_account::session_note(data, pid, hop_of).is_some()
        || crate::history::launch_book::last_of(Some(data), sid).is_some()
}

/// 经环境来的那一发认不认（纯函数）：表里有 ⇒ 认；发问那个进程起来时的环境里没有那一家的嵌套标记 ⇒ 认（ccm 起会话前清掉了它）；
/// 带着 ⇒ 子进程；读不出 ⇒ 认不出。
pub fn admitted(in_table: bool, marker: crate::platform::proc::EnvRead) -> bool {
    in_table || marker == crate::platform::proc::EnvRead::Unset
}
