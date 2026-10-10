//! **一批会话一次问**：「这个 sid 由哪个 tmux 会话在跑」· 停 · 起。tab 栏的单个菜单与批量菜单都走这里（单个 = 一个 sid 的一批）。
//!
//! 「这个 sid 此刻在这台 tmux 里是什么样」只在 [`standing`] 判一次，停 / 起 / 菜单就绪三处共用：
//! 停 = 同 `kill`（三道门、杀句柄、顺手注销 cc-bus）；起在 tmux 里 = 在跑不另起 · 空 tmux 就地键入直路那一行（同 `launch send-into`）·
//! 都不是就铸名、交一行 ccm（同界面那一行、同一个渲染器，只多 `--detach`：建完不接进去）；开终端 = 只渲那一行，窗口由 monitor 开。
//! 起之前先问记录还在不在（同 `history-record`，查这次要用的那棵账号树）。一个不成不挡下一个。
//!
//! 要动 tmux / 读记录 / 起 ccm 的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::{self as la, AccountAsk, AccountUnavailable, LaunchedAccount, Settled};
use super::launch_render::{local, wire};
use crate::platform::child::{Child, Deadline};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// 一批的上界：兜坏输入（一屏 tab 栏放不下这么多），不是兜格式。
pub(crate) const MAX_BATCH: usize = 64;

/// `sessions-where` 整条命令总期限的上限（列名单两发）。
pub(crate) const SESSIONS_WHERE_CAP: Deadline = Deadline::secs(8);
/// 一批停 / 起整条命令总期限的上限：底数 8 s ＋ 每个 6 s，按一批最多的个数算（发起方按个数给的期限比它短就照发起方的）。
/// 整批共用；用完了剩下的各自回超时，不让一个卡住整批。
pub(crate) const BATCH_CAP: Deadline = Deadline::secs(8 + 6 * MAX_BATCH as u64);

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
    /// 这台的 tmux 名单；`Ok(None)` = 这台没装 tmux；`Err` = 列不成（码照列名单那一发的：过了期限是 `child_timed_out`）。
    pub(crate) list: &'a dyn Fn() -> Result<Option<Vec<TmuxEntry>>, CmdErr>,
    /// `(sid, 账号根)` ⇒ 记录在不在 ＋ 查的是哪棵树。
    pub(crate) record: &'a dyn Fn(&str, Option<&str>) -> Result<(bool, String), String>,
    /// `(会话名, sid)` ⇒ 杀；成品是 `kill` 那一格 `bus`。
    pub(crate) kill: &'a dyn Fn(&str, &str) -> Result<Value, CmdErr>,
    /// 就地键入：`(会话名, sid, 那一行)` ⇒ 同 `launch send-into`（带 sid：落在挂着它的那个窗格，身份按它判）。
    pub(crate) send_into: &'a dyn Fn(&str, &str, &str) -> Result<(), CmdErr>,
    /// 交一行 ccm（argv，`argv[0]` 是 `ccm`）⇒ `(退出码, stdout, stderr)`。生产那一份起这台后端自己（它就是 ccm）。
    pub(crate) run_ccm: &'a dyn Fn(&[String]) -> Result<(i32, String, String), CmdErr>,
    /// 基名从哪来 ⇒ 这台铸的新会话名（同 `terminal-name-mint`：按这台此刻的会话名避让）。
    pub(crate) mint: &'a dyn Fn(NameBase) -> Result<String, CmdErr>,
    /// 这台 ccm 会哪些（渲那一行用）。
    pub(crate) caps: &'a BTreeSet<String>,
    /// 本机那一形的事实（平台 · 目录在不在）。
    pub(crate) local_facts: local::Facts,
    /// 起会话挑号要的事实（这台的账号库 · 某条会话上次用的号）。
    pub(crate) accounts: &'a la::Facts<'a>,
    /// sid ⇒ 此刻持着它的活进程 pid（这台的 pidfile，升序）。起之前问：已有在写的 ⇒ 不再起一个。
    pub(crate) writers: &'a dyn Fn(&str) -> Vec<u32>,
    /// `(号目录, 工作目录)` ⇒ 起之前把工作目录标成那个号信任过（那一家没有「信任」这件事 ⇒ 什么都不做；写不成只出声、照常起）。
    pub(crate) pretrust: &'a dyn Fn(&str, &str),
}

/// 起之前那一下：用的是账号库里的号 ⇒ 工作目录标成那个号信任过（[`Deps::pretrust`]）。账号 0 / 不表态 ⇒ 不写
/// （账号 0 那一份是用户主配置，一个字节不写）。
pub(crate) fn pretrust(account: &Settled, cwd: &str, deps: &Deps) {
    if let Settled::Account(a) = account {
        if !cwd.is_empty() {
            (deps.pretrust)(&a.config_dir, cwd);
        }
    }
}

/// 这台的名单（一批一次）：列名单那一发过了期限（总期限用完了）⇒ 整条 `child_timed_out`（同别的装了总期限的命令）；
/// 别的列不成 ⇒ 看不见（`unobservable`，不是零会话）。
pub(crate) fn listed(deps: &Deps) -> Result<Option<Vec<TmuxEntry>>, CmdErr> {
    (deps.list)().map_err(|(code, said)| {
        if code == crate::platform::child::TIMED_OUT {
            (code, said)
        } else {
            ("unobservable", said)
        }
    })
}

/// 原因码：这条会话已有活进程在写，没起（再起一个就是两个进程同写一份记录）。
pub(crate) const ALREADY_LIVE: &str = "session_already_live";

/// pid 列表的那一串（`detail` 与话里同一形）。
pub(crate) fn pids_said(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 铸名的基名从哪来（同 `terminal-name-mint` 的 `cwd` / `forkOf`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameBase<'a> {
    /// 工作目录 ⇒ `<项目名>-cc`。
    Cwd(&'a str),
    /// 分叉出来的那一条 ⇒ `<…>-fork-cc`（必与源名不同）。
    ForkOf(&'a str),
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
    /// 起的那一条实际用的号（`launch-local` 应答那一格同形；停 / 没起 ⇒ `null`）。
    pub(crate) account: Option<LaunchedAccount>,
    /// 选不了号那一项的那一形（同 `account_unavailable` 的 `data`；别的 ⇒ `null`）。
    pub(crate) unavailable: Option<AccountUnavailable>,
}

/// `sessions-stop` / `sessions-start` 的应答：逐个结果，与入参同序。
#[derive(Debug, serde::Serialize)]
pub(crate) struct Batched {
    pub(crate) results: Vec<Outcome>,
}

/// 一个会话的结局上线那一形（[`Answer`] ＋ 失败那一项多的两格）。可缺的格一律出 `null`，不省键。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Outcome {
    pub(crate) sid: String,
    pub(crate) outcome: &'static str,
    pub(crate) why: Option<String>,
    pub(crate) detail: String,
    pub(crate) said: Option<String>,
    pub(crate) copy_detail: String,
    pub(crate) session: Option<String>,
    pub(crate) bus: Option<Value>,
    pub(crate) cmd: Option<String>,
    pub(crate) account: Option<LaunchedAccount>,
    pub(crate) unavailable: Option<AccountUnavailable>,
}

/// `sessions-where` 的应答：逐个结果，与入参同序。
#[derive(Debug, serde::Serialize)]
pub(crate) struct Whereabouts {
    pub(crate) results: Vec<Whereabout>,
}

/// 一个 sid 此刻在这台 tmux 里的样子（[`Standing`] 上线那一形 ＋ 这台没 tmux）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StandingWord {
    Running,
    Ambiguous,
    Idle,
    None,
    NoTmux,
}

/// `sessions-where` 一项：`terminals` 与 `names` 同序同数。
#[derive(Debug, serde::Serialize)]
pub(crate) struct Whereabout {
    pub(crate) sid: String,
    pub(crate) standing: StandingWord,
    pub(crate) names: Vec<String>,
    pub(crate) terminals: Vec<TerminalAt>,
}

/// 名单里那一行（词同容器那一格与 `terminals-list`）。
#[derive(Debug, serde::Serialize)]
pub(crate) struct TerminalAt {
    pub(crate) host: &'static str,
    pub(crate) terminal: String,
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
            account: None,
            unavailable: None,
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
    /// `cmd`：`sessions-stop` / `sessions-start`。失败那一项多两格：`said`（停的那一句与单条结束同一张表，`crate::stream::said`；
    /// 起的那几句要那台的称呼，界面说）· `copyDetail`（复制详情：码 ＋ 那一项的原话）。
    fn to_wire(self, cmd: &str) -> Outcome {
        let failed = self.outcome == "failed";
        let said = match (&self.why, failed && cmd == "sessions-stop") {
            (Some(why), true) => crate::stream::said::reword(
                "kill",
                &json!({ "name": self.session.as_deref().unwrap_or_default() }),
                why,
            ),
            _ => None,
        };
        let copy_detail = match (&self.why, failed) {
            (Some(why), true) => crate::stream::detail::of(Some(cmd), why, Some(&self.detail)),
            _ => String::new(),
        };
        Outcome {
            sid: self.sid,
            outcome: self.outcome,
            why: self.why,
            detail: self.detail,
            said,
            copy_detail,
            session: self.session,
            bus: self.bus,
            cmd: self.cmd,
            account: self.account,
            unavailable: self.unavailable,
        }
    }
}

pub(crate) fn bad(why: &str) -> CmdErr {
    ("bad_args", crate::common::contract::malformed(why))
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
pub(crate) fn carriers<'a>(rows: &'a [TmuxEntry], sid: &str) -> Standing<&'a TmuxEntry> {
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
    let rows = listed(deps)?;
    let host = crate::stream::wire::TerminalHost::Tmux.as_wire();
    let results = sids
        .iter()
        .map(|sid| {
            let (standing, found) = match rows.as_deref().map(|r| carriers(r, sid)) {
                None => (StandingWord::NoTmux, vec![]),
                Some(Standing::Running(e)) => (StandingWord::Running, vec![e]),
                Some(Standing::Ambiguous(es)) => (StandingWord::Ambiguous, es),
                Some(Standing::Idle(e)) => (StandingWord::Idle, vec![e]),
                Some(Standing::None) => (StandingWord::None, vec![]),
            };
            Whereabout {
                sid: sid.clone(),
                standing,
                names: found.iter().map(|e| e.name.clone()).collect(),
                terminals: found
                    .iter()
                    .map(|e| TerminalAt {
                        host,
                        terminal: e.terminal.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    crate::stream::inbound::spec::wire(&Whereabouts { results })
}

/// `sessions-stop`：`{sids}` ⇒ `{results}`。
pub(crate) fn stop(args: &Value, deps: &Deps) -> Result<Value, CmdErr> {
    let sids = sids_of(args.get("sids"))?;
    let rows = listed(deps)?;
    let results: Vec<Outcome> = sids
        .iter()
        .map(|sid| stop_one(sid, rows.as_deref(), deps).to_wire("sessions-stop"))
        .collect();
    crate::stream::inbound::spec::wire(&Batched { results })
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
pub(crate) struct Item {
    pub(crate) sid: String,
    pub(crate) cwd: String,
    pub(crate) account: AccountAsk,
    /// 必铸新终端名（分叉出来的那一条）：不就地键入已有的终端、不复用任何已有的名字（父会话那个尤其）。
    pub(crate) fresh: bool,
    /// 分叉出来的那一条的源会话 sid：新名从源会话此刻所在终端的名字铸（同 `terminal-name-mint` 的 `forkOf`）。
    pub(crate) fork_of: Option<String>,
}

/// 整批共用的几样：哪一家 · 启动器（用户设置的 resume 命令原值）· 这台的模型偏好表（原值）。
pub(crate) struct Batch {
    pub(crate) agent: String,
    launcher: String,
    default_launcher: String,
    pub(crate) models: std::collections::BTreeMap<String, String>,
}

pub(crate) fn str_of<'v>(o: &'v Map<String, Value>, k: &str) -> Result<&'v str, CmdErr> {
    o.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing string `{k}`")))
}

fn item_of(v: &Value) -> Result<Item, CmdErr> {
    let o = v
        .as_object()
        .ok_or_else(|| bad("each item must be an object"))?;
    for k in o.keys() {
        if !["sid", "cwd", "account", "fresh_terminal", "fork_of"].contains(&k.as_str()) {
            return Err(bad(&format!("unknown item field `{k}`")));
        }
    }
    let sid = str_of(o, "sid")?;
    if !shell_quote_core::session_id_ok(sid) {
        return Err(bad(&format!("not a session id: {sid:?}")));
    }
    // 缺席 ＝ 跟随。
    let account = match o.get("account") {
        None => AccountAsk::Follow,
        Some(a) => serde_json::from_value(a.clone()).map_err(|e| bad(&e.to_string()))?,
    };
    let fresh = match o.get("fresh_terminal") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| bad("`fresh_terminal` must be a bool"))?,
    };
    let fork_of = match o.get("fork_of") {
        None => None,
        Some(v) => {
            let s = v
                .as_str()
                .filter(|s| shell_quote_core::session_id_ok(s))
                .ok_or_else(|| bad("`fork_of` must be a session id"))?;
            if !fresh {
                return Err(bad("`fork_of` goes with `fresh_terminal: true`"));
            }
            Some(s.to_string())
        }
    };
    Ok(Item {
        sid: sid.to_string(),
        cwd: str_of(o, "cwd")?.to_string(),
        account,
        fresh,
        fork_of,
    })
}

pub(crate) fn batch_of(o: &Map<String, Value>) -> Result<Batch, CmdErr> {
    let models = match o.get("models") {
        None => Default::default(),
        Some(m) => serde_json::from_value(m.clone())
            .map_err(|_| bad("`models` must be an object of strings"))?,
    };
    Ok(Batch {
        agent: str_of(o, "agent")?.to_string(),
        launcher: str_of(o, "launcher")?.to_string(),
        default_launcher: str_of(o, "defaultLauncher")?.to_string(),
        models,
    })
}

/// 界面那一行的上线入参（`launch-render-cli`），由这一个起会话项拼：`tmux` = 要新建的会话名（`None` = 直路）。
/// 单个那条在界面拼的就是这一份（直路：cwd 只在开终端那一形带；建进 tmux：带 cwd、打 sid 标记）。
fn wire_req(it: &Item, b: &Batch, tmux: Option<&str>, cwd: bool) -> wire::CliRenderRequest {
    wire::CliRenderRequest {
        agent: b.agent.clone(),
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
        account: it.account.clone(),
        ccm_sid: tmux.map(|_| it.sid.clone()),
        model: None,
        models: b.models.clone(),
        launcher: b.launcher.clone(),
        default_launcher: b.default_launcher.clone(),
        preset_args: Vec::new(),
    }
}

/// 本机那一形的入参（`launch-local`）。
fn local_req(it: &Item, b: &Batch, tmux: Option<String>) -> local::LocalLaunchRequest {
    local::LocalLaunchRequest {
        agent: b.agent.clone(),
        action: local::LocalAction::Resume {
            sid: it.sid.clone(),
        },
        cwd: Some(it.cwd.clone()).filter(|c| !c.is_empty()),
        launcher: Some(b.launcher.clone()).filter(|l| l != &b.default_launcher),
        account: Some(it.account.clone()),
        tmux_name: tmux,
        default_launcher: b.default_launcher.clone(),
        preset_args: Vec::new(),
    }
}

/// `sessions-start`：`{mode, local, agent, launcher, defaultLauncher, models?, items: [{sid, cwd, account?}]}` ⇒ `{results}`。
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
    let batch = batch_of(o)?;
    for (i, it) in items.iter().enumerate() {
        if items[..i].iter().any(|x| x.sid == it.sid) {
            return Err(bad(&format!("duplicate session id: {:?}", it.sid)));
        }
    }
    // tmux 那一形先看一眼这台的名单（一批一次）；开终端那一形只有本机要铸新名的那几项用得着（核新名不落在已有的名字上）。
    let fresh_here = here && !deps.local_facts.windows && items.iter().any(|i| i.fresh);
    let rows = if tmux || fresh_here {
        Some(listed(deps)?)
    } else {
        None
    };
    let results = items
        .iter()
        .map(|it| start_one(it, &batch, tmux, rows.as_ref(), here, deps).to_wire("sessions-start"))
        .collect();
    crate::stream::inbound::spec::wire(&Batched { results })
}

/// 一个：先判用哪个号（选不了 ⇒ 跳过、不挡别的），再问记录在不在（查这个号那棵树），再按那一形做。
/// `rows` = 这台的 tmux 名单（`Some(None)` = 这台没 tmux；没看 ⇒ `None`）。
fn start_one(
    it: &Item,
    b: &Batch,
    tmux: bool,
    rows: Option<&Option<Vec<TmuxEntry>>>,
    here: bool,
    deps: &Deps,
) -> Answer {
    // 模型偏好只用在远端那一行上（本机那一行今天不带模型）。
    let none = Default::default();
    let models = if here { &none } else { &b.models };
    let account = match la::settle(&it.account, Some(&it.sid), models, deps.accounts) {
        Ok(a) => a,
        Err(u) => {
            let requested = u.requested.clone();
            return Answer {
                unavailable: Some(u),
                ..Answer::skipped(&it.sid, "account_unavailable", requested)
            };
        }
    };
    let root = match &account {
        Settled::Account(a) => Some(a.config_dir.as_str()),
        Settled::Base | Settled::Unsaid => None,
    };
    match (deps.record)(&it.sid, root) {
        Err(e) => return Answer::failed(&it.sid, ("bad_args", e)),
        Ok((false, root)) => return Answer::skipped(&it.sid, "record_gone", root),
        Ok((true, _)) => {}
    }
    let rows = rows.and_then(Option::as_deref);
    let live = (deps.writers)(&it.sid);
    let done = match tmux {
        true => start_in_tmux(it, b, &account, rows, here, &live, deps),
        false if !live.is_empty() => already_live(&it.sid, &live),
        false if here => {
            pretrust(&account, &it.cwd, deps);
            start_window_here(it, b, &account, rows, deps)
        }
        false => {
            pretrust(&account, &it.cwd, deps);
            let line = own_entry(deps).and_then(|entry| {
                wire::render_ccm_launch_with(
                    &wire_req(it, b, None, true),
                    &account,
                    deps.caps,
                    &entry,
                )
            });
            match line {
                Ok(cmd) => Answer {
                    cmd: Some(cmd),
                    ..Answer::done(&it.sid)
                },
                Err(said) => Answer::failed(&it.sid, ("refused", said)),
            }
        }
    };
    Answer {
        account: match &account {
            Settled::Account(a) if done.outcome == "done" => Some(a.clone()),
            _ => None,
        },
        ..done
    }
}

/// 已有活进程在写 ⇒ 这一项跳过，`detail` 是那几个 pid。
fn already_live(sid: &str, live: &[u32]) -> Answer {
    Answer::skipped(sid, ALREADY_LIVE, pids_said(live))
}

/// 在 tmux 里起（不接进去）：在跑 ⇒ 不另起；已有活进程在写（不在这台的 tmux 里跑着）⇒ 不起；
/// 空 tmux ⇒ 就地键入直路那一行；都不是 ⇒ 铸名、交那一行 ccm（`--detach`）。
fn start_in_tmux(
    it: &Item,
    b: &Batch,
    account: &Settled,
    rows: Option<&[TmuxEntry]>,
    here: bool,
    live: &[u32],
    deps: &Deps,
) -> Answer {
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
        _ if !live.is_empty() => already_live(&it.sid, live),
        // 必铸新名的那一项不键进已有的终端。
        Standing::Idle(_) | Standing::None if it.fresh => {
            match fresh_name(fork_base(it, rows), rows, deps) {
                Ok(name) => start_named(it, b, account, name, here, deps),
                Err(e) => not_minted(&it.sid, e),
            }
        }
        Standing::Idle(n) => {
            let line = own_entry(deps).and_then(|entry| {
                wire::render_ccm_launch_with(
                    &wire_req(it, b, None, false),
                    account,
                    deps.caps,
                    &entry,
                )
            });
            let done = match line {
                Err(said) => Answer::failed(&it.sid, ("refused", said)),
                Ok(line) => {
                    pretrust(account, &it.cwd, deps);
                    match (deps.send_into)(&n, &it.sid, &line) {
                        Ok(()) => Answer::done(&it.sid),
                        Err(e) => Answer::failed(&it.sid, e),
                    }
                }
            };
            Answer {
                session: Some(n),
                ..done
            }
        }
        Standing::None => match (deps.mint)(NameBase::Cwd(&it.cwd)) {
            Ok(name) => start_named(it, b, account, name, here, deps),
            Err(e) => Answer::failed(&it.sid, e),
        },
    }
}

/// 铸不出新名：落在已有的名字上 ⇒ `session` 带那个名字（同 ccm 说名字被占那一形）。
fn not_minted(sid: &str, e: CmdErr) -> Answer {
    Answer {
        session: (e.0 == "name_taken").then(|| e.1.clone()),
        ..Answer::failed(sid, e)
    }
}

/// 必铸新终端名：问这台铸（避让这台此刻的全部会话名），再核一遍不落在名单里任何一个上 ——
/// 父会话那个名字绝不复用（同名 ⇒ `ccm` 会把新会话接进原会话那个窗口）。落上了 ⇒ `name_taken`，不起。
/// 基名（`base`）由调用方定：后台起那一形按分叉那一形（[`fork_base`]），本机开终端那一形同本机 Resume（按工作目录）。
fn fresh_name(base: NameBase, rows: &[TmuxEntry], deps: &Deps) -> Result<String, CmdErr> {
    let name = (deps.mint)(base)?;
    if rows.iter().any(|r| r.name == name) {
        return Err(("name_taken", name));
    }
    Ok(name)
}

/// 分叉那一形的基名：源会话此刻所在终端的名字（在跑的那一个，同 `sessions-where`；命中多个取第一个）；
/// 源会话不在任何终端里 ⇒ 这一项的工作目录。
fn fork_base<'a>(it: &'a Item, rows: &'a [TmuxEntry]) -> NameBase<'a> {
    let source = it
        .fork_of
        .as_deref()
        .and_then(|sid| match carriers(rows, sid) {
            Standing::Running(e) => Some(e.name.as_str()),
            Standing::Ambiguous(es) => es.first().map(|e| e.name.as_str()),
            Standing::Idle(_) | Standing::None => None,
        })
        .unwrap_or(&it.cwd);
    NameBase::ForkOf(source)
}

/// 这台 `ccm` 的入口（那几行直接叫它，不靠 `PATH`）。
fn own_entry(deps: &Deps) -> Result<String, String> {
    (deps.local_facts.entry)()
        .ok_or_else(|| copy_core::copy_text("beLaunchRender.entry.noHome", &[]))
}

/// 以 `name` 新建一个 tmux 会话、在里面起这一个（交一行 ccm，`--detach`）。换号重启复用让出来的旧名也走这一条。
pub(crate) fn start_named(
    it: &Item,
    b: &Batch,
    account: &Settled,
    name: String,
    here: bool,
    deps: &Deps,
) -> Answer {
    let argv = if here {
        local::plan_argv(
            &local_req(it, b, Some(name.clone())),
            account,
            &deps.local_facts,
            true,
        )
    } else {
        own_entry(deps).and_then(|entry| {
            wire::ccm_launch_argv(
                &wire_req(it, b, Some(&name), true),
                account,
                deps.caps,
                &entry,
                true,
            )
        })
    };
    let done = match argv {
        Err(said) => Answer::failed(&it.sid, ("refused", said)),
        Ok(argv) => {
            pretrust(account, &it.cwd, deps);
            match (deps.run_ccm)(&argv) {
                Ok((0, _, _)) => Answer::done(&it.sid),
                // ccm 的退出码 3 = 会话名被占（它响亮失败，不接回别人的会话）。
                Ok((3, _, _)) => Answer::failed(&it.sid, ("name_taken", name.clone())),
                Ok((_, _, err)) => {
                    Answer::failed(&it.sid, ("start_failed", err.trim().to_string()))
                }
                Err(e) => Answer::failed(&it.sid, e),
            }
        }
    };
    Answer {
        session: Some(name),
        ..done
    }
}

/// 本机开终端：同 `launch-local` 那一条（POSIX 上铸名建进 tmux；Windows 上直路）。`rows` ＝ 要铸新名那一项看过的名单。
fn start_window_here(
    it: &Item,
    b: &Batch,
    account: &Settled,
    rows: Option<&[TmuxEntry]>,
    deps: &Deps,
) -> Answer {
    let minted = match (deps.local_facts.windows, it.fresh) {
        (true, _) => Ok(None),
        (false, true) => {
            fresh_name(NameBase::Cwd(&it.cwd), rows.unwrap_or_default(), deps).map(Some)
        }
        (false, false) => (deps.mint)(NameBase::Cwd(&it.cwd)).map(Some),
    };
    let name = match minted {
        Ok(n) => n,
        Err(e) => return not_minted(&it.sid, e),
    };
    let req = local_req(it, b, name.clone());
    match local::plan(&req, account, &deps.local_facts) {
        Ok(cmd) => Answer {
            cmd: Some(cmd),
            session: name,
            ..Answer::done(&it.sid)
        },
        Err(said) => Answer::failed(&it.sid, ("refused", said)),
    }
}

/// 当 ccm 起自己那一趟的期限：界面等 `sessions-start` 的预算是 10 s ＋ 每个 6 s（单个 16 s），收一档到 15 s。
const SELF_AS_CCM_WITHIN: Deadline = Deadline::secs(15);

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

#[cfg(test)]
#[path = "../../../tests/backend/control/session_batch_tests.rs"]
mod tests;
