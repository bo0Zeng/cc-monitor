//! **一批会话一次问**：「这个 sid 由哪个 tmux 会话在跑」· 停。tab 栏的单个菜单与批量菜单都走这里（单个 = 一个 sid 的一批）。
//!
//! 「这个 sid 此刻在这台 tmux 里是什么样」只在 [`standing`] 判一次，停 / 菜单就绪 / 起会话（`session-open`）共用：
//! 停 = 按名单句柄交 `terminal-end` 那一条（`kill::end_terminal`：三道门、杀句柄；托管的挂断）。一个不成不挡下一个。
//!
//! 要动 tmux / 读记录 / 起 ccm 的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::Settled;
use super::session_open as open;
use crate::platform::child::Deadline;
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

/// 终端名单里的一个终端（认会话只要这五格；tmux 与托管同一形）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TermEntry {
    /// 住在哪种宿主里。
    pub(crate) host: crate::wire::TerminalHost,
    /// 终端名（tmux 会话名 · 托管终端的名字）。
    pub(crate) name: String,
    /// 终端句柄（同 `terminals-list` 那一行）。
    pub(crate) terminal: String,
    /// 挂着的 sid（没设 ⇒ `None`）。
    pub(crate) sid: Option<String>,
    /// 前台是不是 agent 的进程。
    pub(crate) agent: bool,
}

/// 认会话的名单：`tmux` ＝ 这台装没装 tmux（恢复默认放在哪 · `no_tmux` 只看它，与有没有托管行无关）；
/// `rows` ＝ 挂着 sid 的终端各一行（tmux 那几行在前，给了托管的家 ⇒ 托管那几行并在后）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Listed {
    pub(crate) tmux: bool,
    /// 这台能托管终端（给了托管的家、平台能托管）：起会话缺省放在哪 · `run: managed` 收不收只看它。
    pub(crate) managed: bool,
    pub(crate) rows: Vec<TermEntry>,
}

impl Listed {
    /// tmux 那一臂（`None` ＝ 没装 tmux）＋ 托管那几行；能不能托管另由 [`Listed::hosting`] 说（缺省不能）。
    pub(crate) fn of(tmux: Option<Vec<TermEntry>>, hosted: Vec<TermEntry>) -> Self {
        Self {
            tmux: tmux.is_some(),
            managed: false,
            rows: tmux.unwrap_or_default().into_iter().chain(hosted).collect(),
        }
    }

    /// 这台能不能托管终端。
    pub(crate) fn hosting(self, managed: bool) -> Self {
        Self { managed, ..self }
    }
}

/// 做事要用到的几样。生产那一份由入口拼（`stream/inbound/mod.rs`），判据给替身。
pub(crate) struct Deps<'a> {
    /// 这台的终端名单（[`Listed`]）；`Err` = 列不成（码照列名单那一发的：过了期限是 `child_timed_out`）。
    pub(crate) list: &'a dyn Fn() -> Result<Listed, CmdErr>,
    /// `(sid, 账号根)` ⇒ 记录在不在 ＋ 查的是哪棵树。
    pub(crate) record: &'a dyn Fn(&str, Option<&str>) -> Result<(bool, String), String>,
    /// 名单句柄 ⇒ 结束那个终端（同 `terminal-end`）。
    /// 失败带原话（tmux 的 stderr）：那一项的「原话」格收「那一句 ＋ 原话」。
    pub(crate) end: &'a dyn Fn(&str) -> Result<(), crate::wire::spec::Fail>,
    /// 就地键入：`(会话名, sid, 那一行)` ⇒ 同 `launch send-into`（带 sid：落在挂着它的那个窗格，身份按它判）。
    /// 失败带原话（tmux 的 stderr）：那一项的「原话」格收「那一句 ＋ 原话」。
    pub(crate) send_into: &'a dyn Fn(&str, &str, &str) -> Result<(), crate::wire::spec::Fail>,
    /// 交一行 ccm（argv，`argv[0]` 是 `ccm`）⇒ `(退出码, stdout, stderr)`。生产那一份起这台后端自己（它就是 ccm）。
    pub(crate) run_ccm: &'a dyn Fn(&[String]) -> Result<(i32, String, String), CmdErr>,
    /// 基名从哪来 ⇒ 这台铸的新会话名（`control::ccm::terminal_name_mint_with`：按这台此刻的会话名避让）。
    pub(crate) mint: &'a dyn Fn(NameBase) -> Result<String, CmdErr>,
    /// 这台 ccm 会哪些（渲那一行用）。
    pub(crate) caps: &'a BTreeSet<String>,
    /// 本机那一形的事实（平台 · 目录在不在）。
    pub(crate) host: super::launch_render::Host,
    /// 起会话挑号要的事实（这台的账号库 · 某条会话上次用的号）。
    pub(crate) accounts: &'a super::launch_account::Facts<'a>,
    /// sid ⇒ 此刻持着它的活进程 pid（这台的 pidfile，升序）。起之前问：已有在写的 ⇒ 不再起一个。
    pub(crate) writers: &'a dyn Fn(&str) -> Vec<u32>,
    /// `(号目录, 工作目录)` ⇒ 起之前把工作目录标成那个号信任过（那一家没有「信任」这件事 ⇒ 什么都不做；写不成只出声、照常起）。
    pub(crate) pretrust: &'a dyn Fn(&str, &str),
    /// `(会话名, 工作目录)` ⇒ 开一个空 shell 的 tmux 会话（不在才建）；回「这一趟建的」。失败带 tmux 原话。
    pub(crate) open_shell: &'a dyn Fn(&str, Option<&str>) -> Result<bool, crate::wire::spec::Fail>,
    /// 这台的偏好（启动命令 · 模型表；一趟读一次 `prefs.json`）。
    pub(crate) prefs: &'a crate::common::machine_prefs::Prefs,
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

/// 这台的终端名单 ⇒ 认会话用的那几格（挂着 sid 的窗格各一行）。`managed` ＝ 托管名单的家：给了 ⇒ 托管那一臂也认（停）；
/// `None` ⇒ 只认 tmux 那一形（起会话 · 分叉 · 就地键入 · 换号重启这几处要往 tmux 会话里键入或新开 tmux 会话，托管那一形没有这几样）。
/// `Err` 的码只留「过了期限」这一种（[`listed`] 按它整条回超时），别的列不成一律当看不见。
pub(crate) fn listed_on(
    on: crate::host::tmux::On<'_>,
    managed: Option<&crate::platform::fs::Own>,
) -> Result<Listed, CmdErr> {
    let tmux = crate::host::tmux::rows_found_on(on)
        .map(|got| got.map(|(rows, _, _)| rows))
        .map_err(|f| {
            let code = if f.code == crate::platform::child::TIMED_OUT {
                crate::platform::child::TIMED_OUT
            } else {
                "failed"
            };
            (code, f.into_note())
        })?;
    // 托管那一臂并在 tmux 那几行后面（同 `terminals-list` 的次序）。
    let hosted = managed
        .map(crate::host::managed::rows_at)
        .unwrap_or_default();
    let entries = |rows: Vec<crate::host::TermRow>| rows.into_iter().map(entry_of).collect();
    Ok(Listed::of(tmux.map(entries), entries(hosted)).hosting(hosts_with(managed)))
}

/// 这台能不能托管终端：给了托管的家、平台能托管（[`crate::control::ccm::plan::HOSTS_HERE`]）。名单那一格与起会话框的 `hosts` 都问它。
pub(crate) fn hosts_with(managed: Option<&crate::platform::fs::Own>) -> bool {
    managed.is_some() && crate::control::ccm::plan::HOSTS_HERE
}

/// 名单里一行 ⇒ 认会话用的五格。
fn entry_of(r: crate::host::TermRow) -> TermEntry {
    TermEntry {
        host: r.host,
        agent: crate::agents::is_agent_process(&r.program),
        terminal: r.handle(),
        sid: (!r.sid.is_empty()).then_some(r.sid),
        name: r.name,
    }
}

/// 这台的名单（一批一次）：列名单那一发过了期限（总期限用完了）⇒ 整条 `child_timed_out`（同别的装了总期限的命令）；
/// 别的列不成 ⇒ 看不见（`unobservable`，不是零会话）。
pub(crate) fn listed(deps: &Deps) -> Result<Listed, CmdErr> {
    (deps.list)().map_err(|(code, said)| {
        if code == crate::platform::child::TIMED_OUT {
            (code, said)
        } else {
            ("unobservable", said)
        }
    })
}

/// pid 列表的那一串（`detail` 与话里同一形）。
pub(crate) fn pids_said(pids: &[u32]) -> String {
    pids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 铸名的基名从哪来：工作目录 · 分叉的源。
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
}

/// `sessions-stop` 的应答：逐个结果，与入参同序。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Batched {
    pub(crate) results: Vec<Outcome>,
}

/// 一个会话的结局上线那一形（[`Answer`] ＋ 失败那一项多的两格）。可缺的格一律出 `null`，不省键。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Outcome {
    /// 会话 id（与入参同序）。
    pub(crate) sid: String,
    /// `done` 做成了 · `skipped` 这一个不用做 / 做不了 · `failed` 做了没成。
    #[cfg_attr(test, ts(type = "\"done\" | \"skipped\" | \"failed\""))]
    pub(crate) outcome: &'static str,
    /// 跳过 / 失败的原因码（做成了 ⇒ `null`）。
    pub(crate) why: Option<String>,
    /// 停的那一项没做成的那一句（失败的与单个结束被拒同一张表）；做成了 ⇒ `null`。
    pub(crate) said: Option<String>,
    /// 失败那一个的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
    pub(crate) copy_detail: String,
    /// 落在哪个 tmux 会话上。
    pub(crate) session: Option<String>,
}

/// `sessions-where` 的应答：逐个结果，与入参同序。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Whereabouts {
    pub(crate) results: Vec<Whereabout>,
    /// 这台恢复默认放在哪（这台的偏好「恢复时进 tmux」× 有没有 tmux）。
    pub(crate) run: open::Run,
}

/// 一个 sid 此刻在这台 tmux 里的样子（[`Standing`] 上线那一形 ＋ 这台没 tmux）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Whereabout {
    /// 会话 id（与入参同序）。
    pub(crate) sid: String,
    /// `running` · `ambiguous` · `idle` · `none` · `no_tmux`。
    pub(crate) standing: StandingWord,
    /// 带着它的 tmux 会话名。
    pub(crate) names: Vec<String>,
    /// 与 `names` 同序同数：名单里那一行的句柄。
    pub(crate) terminals: Vec<TerminalAt>,
}

/// 名单里那一行（词同容器那一格与 `terminals-list`）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct TerminalAt {
    /// 终端宿主。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) host: &'static str,
    /// 名单里那一行的句柄。
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
    /// 停的每个没做成的那一项带 `said`（失败那几句与单条结束同一张表，[`crate::control::kill::said`]）；
    /// 失败那一项另带 `copyDetail`（复制详情：码 ＋ 那一项的原话）。
    fn to_wire(self) -> Outcome {
        let failed = self.outcome == "failed";
        let said = match self.why.as_deref() {
            Some(why) => Some(stop_said(why, &self)),
            _ => None,
        };
        let copy_detail = match (&self.why, failed) {
            (Some(why), true) => {
                crate::wire::detail::of(Some("sessions-stop"), why, Some(&self.detail))
            }
            _ => String::new(),
        };
        Outcome {
            sid: self.sid,
            outcome: self.outcome,
            why: self.why,
            said,
            copy_detail,
            session: self.session,
        }
    }
}

pub(crate) fn bad(why: &str) -> CmdErr {
    ("bad_args", crate::common::contract::malformed(why))
}

/// 一串 sid（每个的形状解入参时已判，`cmd_wire::Sid`）：非空、不超上界、不重复。
fn sids_of(arr: &[cmd_wire::Sid]) -> Result<Vec<String>, CmdErr> {
    let mut out: Vec<String> = Vec::new();
    for s in arr {
        if out.iter().any(|x| x == s.as_str()) {
            return Err(bad(&format!("duplicate session id: {:?}", s.as_str())));
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
pub(crate) fn carriers<'a>(rows: &'a [TermEntry], sid: &str) -> Standing<&'a TermEntry> {
    let carrying = |agent: bool| -> Vec<&TermEntry> {
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
pub(crate) fn standing(rows: &[TermEntry], sid: &str) -> Standing {
    match carriers(rows, sid) {
        Standing::Running(e) => Standing::Running(e.name.clone()),
        Standing::Ambiguous(es) => Standing::Ambiguous(es.iter().map(|e| e.name.clone()).collect()),
        Standing::Idle(e) => Standing::Idle(e.name.clone()),
        Standing::None => Standing::None,
    }
}

/// `sessions-where` / `sessions-stop` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SidsReq {
    /// 那几个会话（1–64 个，不重复）。
    pub(crate) sids: Vec<cmd_wire::Sid>,
    /// 自报的前端（「哪个前端的会话」那一维，同 `terminal-end`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// `sessions-where`：每个 sid 此刻在这台哪个终端里（菜单就绪时问：这一项亮不亮、写哪个名字）。
/// `terminals` 与 `names` 同序同数，每一项 `{host, terminal}`（词同容器那一格与 `terminals-list`，宿主照名单那一行）。
/// 哪个终端都不带着它、这台又没装 tmux ⇒ `no_tmux`。
pub(crate) fn where_(req: &SidsReq, deps: &Deps) -> Result<Whereabouts, CmdErr> {
    let sids = sids_of(&req.sids)?;
    let listed = listed(deps)?;
    let results = sids
        .iter()
        .map(|sid| {
            let (standing, found) = match carriers(&listed.rows, sid) {
                Standing::Running(e) => (StandingWord::Running, vec![e]),
                Standing::Ambiguous(es) => (StandingWord::Ambiguous, es),
                Standing::Idle(e) => (StandingWord::Idle, vec![e]),
                Standing::None if !listed.tmux => (StandingWord::NoTmux, vec![]),
                Standing::None => (StandingWord::None, vec![]),
            };
            Whereabout {
                sid: sid.clone(),
                standing,
                names: found.iter().map(|e| e.name.clone()).collect(),
                terminals: found
                    .iter()
                    .map(|e| TerminalAt {
                        host: e.host.as_wire(),
                        terminal: e.terminal.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    let run = open::default_run(
        open::Intent::Resume,
        deps.prefs.resume_in_tmux,
        listed.tmux,
        listed.managed,
    );
    Ok(Whereabouts { results, run })
}

/// 停的那一项没做成的那一句：跳过的三种这里说，停了没成的与单条结束同一张表。
fn stop_said(why: &str, a: &Answer) -> String {
    match why {
        "no_tmux" => copy_core::copy_text("tabBatch.why.noTmux", &[]),
        "not_hosted" => copy_core::copy_text("tabBatch.why.notHosted", &[]),
        "ambiguous" => copy_core::copy_text("tabBatch.why.ambiguous", &[("names", &a.detail)]),
        _ => crate::control::kill::said(a.session.as_deref().unwrap_or_default(), why),
    }
}

/// `sessions-stop`：逐个停，与入参同序答。
pub(crate) fn stop(req: &SidsReq, deps: &Deps) -> Result<Batched, CmdErr> {
    let sids = sids_of(&req.sids)?;
    let listed = listed(deps)?;
    let results: Vec<Outcome> = sids
        .iter()
        .map(|sid| stop_one(sid, &listed, deps).to_wire())
        .collect();
    Ok(Batched { results })
}

fn stop_one(sid: &str, listed: &Listed, deps: &Deps) -> Answer {
    let at = match carriers(&listed.rows, sid) {
        Standing::Running(e) | Standing::Idle(e) => e,
        Standing::None if !listed.tmux => return Answer::skipped(sid, "no_tmux", String::new()),
        Standing::None => return Answer::skipped(sid, "not_hosted", String::new()),
        // 命中多个不结束（选错了不可逆）。
        Standing::Ambiguous(es) => {
            let ns: Vec<String> = es.iter().map(|e| e.name.clone()).collect();
            return Answer {
                session: ns.first().cloned(),
                ..Answer::skipped(sid, "ambiguous", ns.join(", "))
            };
        }
    };
    let name = at.name.clone();
    match (deps.end)(&at.terminal) {
        Ok(()) => Answer {
            session: Some(name),
            ..Answer::done(sid)
        },
        Err(f) => {
            let code = f.code.clone();
            Answer {
                session: Some(name),
                ..Answer::failed(sid, (&code, crate::common::said::Said::from(f).logged()))
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_batch_tests.rs"]
mod tests;
