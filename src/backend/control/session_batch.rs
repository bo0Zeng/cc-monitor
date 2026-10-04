//! **一批会话一次问**：「这个 sid 由哪个 tmux 会话在跑」· 停 · 起。tab 栏的单个菜单与批量菜单都走这里（单个 = 一个 sid 的一批）。
//!
//! 「这个 sid 此刻在这台 tmux 里是什么样」只在 [`standing`] 判一次，停 / 起 / 菜单就绪三处共用：
//! 停 = 同 `kill`（三道门、杀句柄、顺手注销 cc-bus）；起在 tmux 里 = 在跑不另起 · 空 tmux 就地键入直路那一行（同 `launch send-into`）·
//! 都不是就铸名、交一行 ccm（同界面那一行、同一个渲染器，只多 `--detach`：建完不接进去）；开终端 = 只渲那一行，窗口由 monitor 开。
//! 起之前先问记录还在不在（同 `history-record`，查这次要用的那棵账号树）。一个不成不挡下一个。
//!
//! 要动 tmux / 读记录 / 起 ccm 的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_render::{local, wire};
use crate::common::child_env::WithoutOwnEnv;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// 一批的上界：兜坏输入（一屏 tab 栏放不下这么多），不是兜格式。
pub(crate) const MAX_BATCH: usize = 64;

/// 命令级错误：`(code, message)`。
pub(crate) type CmdErr = (&'static str, String);

/// 这台 tmux 里的一个会话（只要四格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TmuxEntry {
    pub(crate) name: String,
    /// 终端句柄（同 `terminals-list` 那一行）。
    pub(crate) terminal: String,
    /// `@ccm_sid`（没设 ⇒ `None`）。
    pub(crate) sid: Option<String>,
    /// 前台是不是 agent 的进程。
    pub(crate) agent: bool,
}

/// 做事要用到的几样。生产那一份由入口拼（`stream/inbound/mod.rs`），判据给替身。
pub(crate) struct Deps<'a> {
    /// 这台的 tmux 名单；`Ok(None)` = 这台没装 tmux；`Err` = 看不见（不是零会话）。
    pub(crate) list: &'a dyn Fn() -> Result<Option<Vec<TmuxEntry>>, String>,
    /// `(sid, 账号根)` ⇒ 记录在不在 ＋ 查的是哪棵树。
    pub(crate) record: &'a dyn Fn(&str, Option<&str>) -> Result<(bool, String), String>,
    /// `(会话名, sid)` ⇒ 杀；成品是 `kill` 那一格 `bus`。
    pub(crate) kill: &'a dyn Fn(&str, &str) -> Result<Value, CmdErr>,
    /// 就地键入：`(会话名, sid, 那一行)` ⇒ 同 `launch send-into`（带 sid：落在挂着它的那个窗格，身份按它判）。
    pub(crate) send_into: &'a dyn Fn(&str, &str, &str) -> Result<(), CmdErr>,
    /// 交一行 ccm（argv，`argv[0]` 是 `ccm`）⇒ `(退出码, stdout, stderr)`。生产那一份起这台后端自己（它就是 ccm）。
    pub(crate) run_ccm: &'a dyn Fn(&[String]) -> Result<(i32, String, String), String>,
    /// 工作目录 ⇒ 这台铸的新会话名（同 `terminal-name-mint`）。
    pub(crate) mint: &'a dyn Fn(&str) -> Result<String, CmdErr>,
    /// 这台 ccm 会哪些（渲那一行用）。
    pub(crate) caps: &'a BTreeSet<String>,
    /// 本机那一形的事实（平台 · 目录在不在）。
    pub(crate) local_facts: local::Facts,
}

/// 一个会话的结局。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Answer {
    pub(crate) sid: String,
    /// `done` 做成了 · `skipped` 这一个不用做 / 做不了 · `failed` 做了没成。
    pub(crate) outcome: &'static str,
    /// 跳过 / 失败的原因码（做成了 ⇒ `None`）。
    pub(crate) why: Option<String>,
    pub(crate) detail: String,
    /// 落在哪个 tmux 会话上（停 · 起在 tmux 里 · 本机开终端那一形）。
    pub(crate) session: Option<String>,
    pub(crate) bus: Option<Value>,
    /// 开终端那一形要跑的那一行。
    pub(crate) cmd: Option<String>,
}

impl Answer {
    fn new(sid: &str, outcome: &'static str, code: Option<&str>, detail: String) -> Self {
        Self {
            sid: sid.to_string(),
            outcome,
            why: code.map(str::to_string),
            detail,
            session: None,
            bus: None,
            cmd: None,
        }
    }
    fn skipped(sid: &str, code: &str, detail: String) -> Self {
        Self::new(sid, "skipped", Some(code), detail)
    }
    fn failed(sid: &str, (code, detail): (&str, String)) -> Self {
        Self::new(sid, "failed", Some(code), detail)
    }
    fn done(sid: &str) -> Self {
        Self::new(sid, "done", None, String::new())
    }
    fn to_json(&self) -> Value {
        json!({
            "sid": self.sid,
            "outcome": self.outcome,
            "why": self.why,
            "detail": self.detail,
            "session": self.session,
            "bus": self.bus,
            "cmd": self.cmd,
        })
    }
}

fn bad(why: &str) -> CmdErr {
    ("invalid_args", crate::common::contract::malformed(why))
}

/// 一串 sid：非空、不超上界、不重复、每个过 sid 那一关。
fn sids_of(v: Option<&Value>) -> Result<Vec<String>, CmdErr> {
    let arr = v
        .and_then(Value::as_array)
        .ok_or_else(|| bad("`sids` must be an array of session ids"))?;
    let mut out: Vec<String> = Vec::new();
    for s in arr {
        let s = s.as_str().ok_or_else(|| bad("`sids` must be strings"))?;
        if !shell_quote_core::session_id_ok(s) {
            return Err(bad(&format!("not a session id: {s:?}")));
        }
        if out.iter().any(|x| x == s) {
            return Err(bad(&format!("duplicate session id: {s:?}")));
        }
        out.push(s.to_string());
    }
    if out.is_empty() || out.len() > MAX_BATCH {
        return Err(bad(&format!("`sids` must hold 1..={MAX_BATCH} ids")));
    }
    Ok(out)
}

/// 这个 sid 此刻在这台 tmux 里的样子 —— **唯一的判定**（单个菜单亮哪几项、批量停 / 起都读它）。
/// `T` 是带着它的那一个（默认是 tmux 会话名；`sessions-where` 要整行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Standing<T = String> {
    /// 恰好一个带着它（`@ccm_sid`）、前台是 agent 的会话。
    Running(T),
    /// 带着它、前台是 agent 的不止一个（按名单顺序）—— 破坏性动作拒，接回接第一个。
    Ambiguous(Vec<T>),
    /// 没有在跑的，但有带着它的空 tmux（agent 已退、只剩 shell；取第一个）。
    Idle(T),
    /// 没有哪个 tmux 会话带着它（没打上标记的不猜：不按目录认）。
    None,
}

/// 名单 ⇒ 这个 sid 的样子（带着它的那几行）。
fn carriers<'a>(rows: &'a [TmuxEntry], sid: &str) -> Standing<&'a TmuxEntry> {
    let carrying = |agent: bool| -> Vec<&TmuxEntry> {
        rows.iter()
            .filter(|r| r.sid.as_deref() == Some(sid) && r.agent == agent)
            .collect()
    };
    let mut live = carrying(true);
    match live.len() {
        1 => Standing::Running(live.remove(0)),
        0 => carrying(false)
            .into_iter()
            .next()
            .map_or(Standing::None, Standing::Idle),
        _ => Standing::Ambiguous(live),
    }
}

/// 名单 ⇒ 这个 sid 的样子（带着它的 tmux 会话名）。
pub(crate) fn standing(rows: &[TmuxEntry], sid: &str) -> Standing {
    match carriers(rows, sid) {
        Standing::Running(e) => Standing::Running(e.name.clone()),
        Standing::Ambiguous(es) => Standing::Ambiguous(es.iter().map(|e| e.name.clone()).collect()),
        Standing::Idle(e) => Standing::Idle(e.name.clone()),
        Standing::None => Standing::None,
    }
}

/// `sessions-where`：`{sids}` ⇒ `{results: [{sid, standing, names, terminals}]}`（菜单就绪时问：这一项亮不亮、写哪个名字）。
/// `terminals` 与 `names` 同序同数，每一项 `{host, terminal}`（词同容器那一格与 `terminals-list`）。
pub(crate) fn where_(args: &Value, deps: &Deps) -> Result<Value, CmdErr> {
    let sids = sids_of(args.get("sids"))?;
    let rows = (deps.list)().map_err(|m| ("unobservable", m))?;
    let host = crate::stream::wire::TerminalHost::Tmux.as_wire();
    let results: Vec<Value> = sids
        .iter()
        .map(|sid| {
            let (kind, found) = match rows.as_deref().map(|r| carriers(r, sid)) {
                None => ("no_tmux", vec![]),
                Some(Standing::Running(e)) => ("running", vec![e]),
                Some(Standing::Ambiguous(es)) => ("ambiguous", es),
                Some(Standing::Idle(e)) => ("idle", vec![e]),
                Some(Standing::None) => ("none", vec![]),
            };
            let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
            let terminals: Vec<Value> = found
                .iter()
                .map(|e| json!({ "host": host, "terminal": e.terminal }))
                .collect();
            json!({ "sid": sid, "standing": kind, "names": names, "terminals": terminals })
        })
        .collect();
    Ok(json!({ "results": results }))
}

/// `sessions-stop`：`{sids}` ⇒ `{results}`。
pub(crate) fn stop(args: &Value, deps: &Deps) -> Result<Value, CmdErr> {
    let sids = sids_of(args.get("sids"))?;
    let rows = (deps.list)().map_err(|m| ("unobservable", m))?;
    let results: Vec<Value> = sids
        .iter()
        .map(|sid| stop_one(sid, rows.as_deref(), deps).to_json())
        .collect();
    Ok(json!({ "results": results }))
}

fn stop_one(sid: &str, rows: Option<&[TmuxEntry]>, deps: &Deps) -> Answer {
    let Some(rows) = rows else {
        return Answer::skipped(sid, "no_tmux", String::new());
    };
    let name = match standing(rows, sid) {
        Standing::Running(n) | Standing::Idle(n) => n,
        Standing::None => return Answer::skipped(sid, "not_in_tmux", String::new()),
        // 命中多个不杀（选错了不可逆）。
        Standing::Ambiguous(ns) => {
            return Answer {
                session: ns.first().cloned(),
                ..Answer::skipped(sid, "ambiguous", ns.join(", "))
            }
        }
    };
    match (deps.kill)(&name, sid) {
        Ok(bus) => Answer {
            session: Some(name),
            bus: Some(bus),
            ..Answer::done(sid)
        },
        Err(e) => Answer {
            session: Some(name),
            ..Answer::failed(sid, e)
        },
    }
}

/// 一个要起的会话（已过形状关）。
struct Item {
    /// 这个会话是哪一家（线上的 kind）。
    agent: String,
    sid: String,
    cwd: String,
    account: Acct,
    model: Option<String>,
    launcher: String,
    default_launcher: String,
}

#[derive(Debug, Clone)]
enum Acct {
    Inherit,
    Base,
    Named {
        name: Option<String>,
        config_dir: String,
    },
}

impl Acct {
    fn config_dir(&self) -> Option<&str> {
        match self {
            Acct::Named { config_dir, .. } => Some(config_dir),
            _ => None,
        }
    }
    /// 界面那一行（`launch-render-cli`）的账号形：没有「继承」那一态 —— 同单个就地 resume，落账号 0。
    fn wire(&self) -> wire::WireAccount {
        match self {
            Acct::Inherit | Acct::Base => wire::WireAccount::Base,
            Acct::Named { name, config_dir } => wire::WireAccount::Account {
                name: name.clone(),
                config_dir: Some(config_dir.clone()),
            },
        }
    }
    fn local(&self) -> Option<local::LaunchAccount> {
        match self {
            Acct::Inherit => None,
            Acct::Base => Some(local::LaunchAccount::Base),
            Acct::Named { name, config_dir } => Some(local::LaunchAccount::Named {
                config_dir: config_dir.clone(),
                name: name.clone(),
            }),
        }
    }
}

fn str_of<'v>(o: &'v Map<String, Value>, k: &str) -> Result<&'v str, CmdErr> {
    o.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing string `{k}`")))
}

fn item_of(v: &Value) -> Result<Item, CmdErr> {
    let o = v
        .as_object()
        .ok_or_else(|| bad("each item must be an object"))?;
    for k in o.keys() {
        if ![
            "agent",
            "sid",
            "cwd",
            "account",
            "model",
            "launcher",
            "defaultLauncher",
        ]
        .contains(&k.as_str())
        {
            return Err(bad(&format!("unknown item field `{k}`")));
        }
    }
    let sid = str_of(o, "sid")?;
    if !shell_quote_core::session_id_ok(sid) {
        return Err(bad(&format!("not a session id: {sid:?}")));
    }
    let a = o
        .get("account")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("missing object `account`"))?;
    let account = match str_of(a, "kind")? {
        "inherit" => Acct::Inherit,
        "base" => Acct::Base,
        "named" => Acct::Named {
            name: a.get("name").and_then(Value::as_str).map(str::to_string),
            config_dir: str_of(a, "configDir")?.to_string(),
        },
        k => return Err(bad(&format!("unknown account kind `{k}`"))),
    };
    let model = match o.get("model") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(bad("`model` must be a string or null")),
    };
    Ok(Item {
        agent: str_of(o, "agent")?.to_string(),
        sid: sid.to_string(),
        cwd: str_of(o, "cwd")?.to_string(),
        account,
        model,
        launcher: str_of(o, "launcher")?.to_string(),
        default_launcher: str_of(o, "defaultLauncher")?.to_string(),
    })
}

/// 界面那一行的上线入参（`launch-render-cli`），由这一个起会话项拼：`tmux` = 要新建的会话名（`None` = 直路）。
/// 单个那条在界面拼的就是这一份（直路：cwd 只在开终端那一形带；建进 tmux：带 cwd、打 sid 标记）。
fn wire_req(it: &Item, tmux: Option<&str>, cwd: bool) -> wire::CliRenderRequest {
    wire::CliRenderRequest {
        agent: it.agent.clone(),
        action: wire::WireAction::Resume {
            sid: it.sid.clone(),
        },
        container: match tmux {
            Some(name) => wire::WireContainer::Tmux {
                name: name.to_string(),
                send_into: false,
            },
            None => wire::WireContainer::None,
        },
        cwd: Some(it.cwd.clone()).filter(|c| cwd && !c.is_empty()),
        account: it.account.wire(),
        ccm_sid: tmux.map(|_| it.sid.clone()),
        model: it.model.clone(),
        launcher: it.launcher.clone(),
        default_launcher: it.default_launcher.clone(),
    }
}

/// 本机那一形的入参（`launch-local`）。
fn local_req(it: &Item, tmux: Option<String>) -> local::LocalLaunchRequest {
    local::LocalLaunchRequest {
        agent: it.agent.clone(),
        action: local::LocalAction::Resume {
            sid: it.sid.clone(),
        },
        cwd: Some(it.cwd.clone()).filter(|c| !c.is_empty()),
        launcher: Some(it.launcher.clone()).filter(|l| l != &it.default_launcher),
        account: it.account.local(),
        tmux_name: tmux,
        default_launcher: it.default_launcher.clone(),
    }
}

/// `sessions-start`：`{mode, local, items}` ⇒ `{results}`。
pub(crate) fn start(args: &Value, deps: &Deps) -> Result<Value, CmdErr> {
    let o = args
        .as_object()
        .ok_or_else(|| bad("args must be an object"))?;
    let tmux = match str_of(o, "mode")? {
        "tmux" => true,
        "window" => false,
        m => return Err(bad(&format!("unknown mode `{m}` (tmux / window)"))),
    };
    let here = o
        .get("local")
        .and_then(Value::as_bool)
        .ok_or_else(|| bad("missing bool `local`"))?;
    let raw = o
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("missing array `items`"))?;
    if raw.is_empty() || raw.len() > MAX_BATCH {
        return Err(bad(&format!("`items` must hold 1..={MAX_BATCH} entries")));
    }
    let items = raw.iter().map(item_of).collect::<Result<Vec<_>, _>>()?;
    for (i, it) in items.iter().enumerate() {
        if items[..i].iter().any(|x| x.sid == it.sid) {
            return Err(bad(&format!("duplicate session id: {:?}", it.sid)));
        }
    }
    // tmux 那一形先看一眼这台的名单（一批一次）；开终端那一形用不着。
    let rows = if tmux {
        Some((deps.list)().map_err(|m| ("unobservable", m))?)
    } else {
        None
    };
    let results: Vec<Value> = items
        .iter()
        .map(|it| start_one(it, rows.as_ref(), here, deps).to_json())
        .collect();
    Ok(json!({ "results": results }))
}

/// 一个：先问记录在不在，再按那一形做。`rows` = tmux 那一形的名单（`Some(None)` = 这台没 tmux）；开终端那一形 `None`。
fn start_one(it: &Item, rows: Option<&Option<Vec<TmuxEntry>>>, here: bool, deps: &Deps) -> Answer {
    match (deps.record)(&it.sid, it.account.config_dir()) {
        Err(e) => return Answer::failed(&it.sid, ("bad_args", e)),
        Ok((false, root)) => return Answer::skipped(&it.sid, "record_gone", root),
        Ok((true, _)) => {}
    }
    match rows {
        Some(rows) => start_in_tmux(it, rows.as_deref(), here, deps),
        None if here => start_window_here(it, deps),
        None => match wire::render_ccm_launch_with(&wire_req(it, None, true), deps.caps) {
            Ok(cmd) => Answer {
                cmd: Some(cmd),
                ..Answer::done(&it.sid)
            },
            Err(said) => Answer::failed(&it.sid, ("refused", said)),
        },
    }
}

/// 在 tmux 里起（不接进去）：在跑 ⇒ 不另起；空 tmux ⇒ 就地键入直路那一行；都不是 ⇒ 铸名、交那一行 ccm（`--detach`）。
fn start_in_tmux(it: &Item, rows: Option<&[TmuxEntry]>, here: bool, deps: &Deps) -> Answer {
    let Some(rows) = rows else {
        return Answer::skipped(&it.sid, "no_tmux", String::new());
    };
    match standing(rows, &it.sid) {
        Standing::Running(n) => Answer {
            session: Some(n.clone()),
            ..Answer::skipped(&it.sid, "running", n)
        },
        Standing::Ambiguous(ns) => Answer {
            session: ns.first().cloned(),
            ..Answer::skipped(&it.sid, "ambiguous", ns.join(", "))
        },
        Standing::Idle(n) => {
            let done = match wire::render_ccm_launch_with(&wire_req(it, None, false), deps.caps) {
                Err(said) => Answer::failed(&it.sid, ("refused", said)),
                Ok(line) => match (deps.send_into)(&n, &it.sid, &line) {
                    Ok(()) => Answer::done(&it.sid),
                    Err(e) => Answer::failed(&it.sid, e),
                },
            };
            Answer {
                session: Some(n),
                ..done
            }
        }
        Standing::None => {
            let name = match (deps.mint)(&it.cwd) {
                Ok(n) => n,
                Err(e) => return Answer::failed(&it.sid, e),
            };
            let argv = if here {
                local::plan_argv(&local_req(it, Some(name.clone())), &deps.local_facts, true)
            } else {
                wire::ccm_launch_argv(&wire_req(it, Some(&name), true), deps.caps, true)
            };
            let done = match argv {
                Err(said) => Answer::failed(&it.sid, ("refused", said)),
                Ok(argv) => match (deps.run_ccm)(&argv) {
                    Ok((0, _, _)) => Answer::done(&it.sid),
                    // ccm 的退出码 3 = 会话名被占（它响亮失败，不接回别人的会话）。
                    Ok((3, _, _)) => Answer::failed(&it.sid, ("name_taken", name.clone())),
                    Ok((_, _, err)) => {
                        Answer::failed(&it.sid, ("start_failed", err.trim().to_string()))
                    }
                    Err(e) => Answer::failed(&it.sid, ("start_failed", e)),
                },
            };
            Answer {
                session: Some(name),
                ..done
            }
        }
    }
}

/// 本机开终端：同 `launch-local` 那一条（POSIX 上铸名建进 tmux；Windows 上直路）。
fn start_window_here(it: &Item, deps: &Deps) -> Answer {
    let name = if deps.local_facts.windows {
        None
    } else {
        match (deps.mint)(&it.cwd) {
            Ok(n) => Some(n),
            Err(e) => return Answer::failed(&it.sid, e),
        }
    };
    let req = local_req(it, name.clone());
    match local::plan(&req, &deps.local_facts) {
        Ok(p) => Answer {
            cmd: Some(p.cmd),
            session: name,
            ..Answer::done(&it.sid)
        },
        Err(said) => Answer::failed(&it.sid, ("refused", said)),
    }
}

/// 交一行 ccm：这台后端自己就是 ccm（同一个二进制，argv 不过 shell）。不接进去（`--detach`），等它退出、收它的话。
/// 去掉 `TMUX` / `TMUX_PANE`：单个那一项是在一个新开的终端里跑这一行，那里不在 tmux 里。
pub(crate) fn run_self_as_ccm(argv: &[String]) -> Result<(i32, String, String), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(me)
        .without_own_env()
        .args(argv.iter().skip(1))
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_batch_tests.rs"]
mod tests;
