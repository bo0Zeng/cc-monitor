//! **起会话用哪个号** —— 判定只住 [`pick`]；「这条会话上次用哪个号起的」那份记录也住这里。
//!
//! - 判定：用户点名的号 · 账号 0 · 跟随（这条会话上次的号 → 这台的默认号 → 账号 0）。
//!   跟随时上次那个号选不了 ⇒ 不起、给一个显式的替代（绝不悄悄换成别的号）。
//! - 记录：`ccm` 在最终那一跳给将要跑 agent 的那个进程留一张**便条**（`launch-pending/<pid>.json`：号 ＋ 时刻，
//!   各写各的；起会话时给了 `--tab` 也记在这张上）；观测侧看见那个 pid 的会话时认便条，把 `sid → 号`（与 `sid → 标签页`）记进
//!   `launch-accounts.json`（启动台账：只有观测侧写，锁里原子写）。
//!   便条留到那个 pid 不在了才清（`/clear` 原地换 sid 也记得到）。

use crate::platform::fs::{Own, OwnPath};
use crate::wire::typed::codes;
use cmd_wire::accounts::{AccountRow, AccountsList};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 调用方要的号（账号 0 不用判，在 [`settle`] 那一步就定了）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// 用户刚点名的那个号。
    Named(String),
    /// 跟随：这条会话上次用的号 → 这台的默认号 → 账号 0。
    Follow,
}

/// 账号库里的一个号（只留判定要的几格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) name: String,
    pub(crate) config_dir: Option<String>,
    /// 能拿来起会话：隔离模式 · 鉴权前提就绪 · 目录在 · 有账号目录（账号 0 恒不在此列）。
    pub(crate) usable: bool,
}

/// 这台的账号库。`known = false` ⇒ 清单读不出来（说不清默认号是谁）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Library {
    pub(crate) known: bool,
    pub(crate) accounts: Vec<Candidate>,
    /// 这台的默认号（成品 `meta.effectiveDefault`：规则住 `acct_core::effective_default`，这里不另判）。
    pub(crate) default: Option<String>,
}

impl Library {
    /// `accounts-list` 的成品 ⇒ 判定要的那一份（同一份扫描，不另读 manifest）。
    pub(crate) fn of_product(v: &AccountsList) -> Library {
        Library {
            known: v.meta.enabled,
            accounts: v.accounts.iter().filter_map(candidate_of).collect(),
            default: v.meta.effective_default.clone(),
        }
    }

    fn usable(&self, name: &str) -> Option<&Candidate> {
        self.accounts.iter().find(|a| a.name == name && a.usable)
    }

    /// 这台的默认号（成品写好的那一格）。
    fn default_one(&self) -> Option<&Candidate> {
        let d = self.default.as_deref()?;
        self.accounts.iter().find(|a| a.name == d)
    }

    /// 要的号选不了时给的那个显式替代：默认号（能用、且不是要的那个）；清单读不出 ⇒ 没有。
    fn alternative_to(&self, requested: &str) -> Option<String> {
        self.default_one()
            .filter(|a| self.known && a.usable && a.name != requested)
            .map(|a| a.name.clone())
    }
}

fn candidate_of(a: &AccountRow) -> Option<Candidate> {
    let a = &a.scanned;
    if a.name.is_empty() {
        return None;
    }
    let config_dir = a.config_dir.clone().filter(|d| !d.is_empty());
    // 能不能用是成品写好的那一格（`acct_core::account_selectable`），这里不另判。
    let usable = a.selectable && config_dir.is_some();
    Some(Candidate {
        name: a.name.clone(),
        config_dir,
        usable,
    })
}

/// 判定的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Picked {
    /// 用这个号（名字 ＋ 账号目录）。
    Account { name: String, config_dir: String },
    /// 不指定号：用户点名的账号 0，或跟随时没有上次的号、默认号也用不了。
    Base,
    /// 要的号选不了 ⇒ 不起。
    Unavailable(AccountUnavailable),
}

/// 选不了的那一形（线上 `account_unavailable` 的 `data`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AccountUnavailable {
    /// 要的那个号。
    pub requested: String,
    /// 它是这条会话上次用的号（跟随），不是这一次点名的。
    pub pinned: bool,
    /// 这台的账号清单读得出来。
    pub list_known: bool,
    /// 给的显式替代（这台的默认号）；`null` ⇒ 只能「不指定号」。
    pub alternative: Option<String>,
}

/// **起会话用哪个号 —— 唯一的判定。**
///
/// `last` ＝ 这条会话上次用的号（[`last_of`]）。`pinned_now` ＝ 会话此刻钉着的号（额度轮换用，今天恒空、不参与判定）。
pub(crate) fn pick(
    asked: &Asked,
    lib: &Library,
    last: Option<&str>,
    pinned_now: Option<&str>,
) -> Picked {
    let _ = pinned_now;
    let refuse = |requested: &str, pinned: bool| {
        Picked::Unavailable(AccountUnavailable {
            requested: requested.to_string(),
            pinned,
            list_known: lib.known,
            alternative: lib.alternative_to(requested),
        })
    };
    let take = |a: &Candidate| Picked::Account {
        name: a.name.clone(),
        config_dir: a.config_dir.clone().unwrap_or_default(),
    };
    match asked {
        Asked::Named(n) => lib.usable(n).map_or_else(|| refuse(n, false), take),
        Asked::Follow => match last {
            Some(p) => lib.usable(p).map_or_else(|| refuse(p, true), take),
            None => lib
                .default_one()
                .filter(|a| a.usable)
                .map_or(Picked::Base, take),
        },
    }
}

// ───────────────────────────── 起会话那几条共用的一步 ─────────────────────────────

/// 线上 `account` 那一格（三条起会话请求同一份）：跟随 · 账号 0 · 用户点名。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum AccountAsk {
    /// 这条会话上次用的号 → 这台的默认号 → 不指定（新起的会话没有上次的号）。
    Follow,
    Base,
    /// 用户点名的那个号（按名字判；分叉沿用源会话的号也是名字，由这台从源会话推出）。
    Named {
        name: String,
    },
}

/// 判定要的事实，由入口交（control 不读观测层）。
pub(crate) struct Facts<'a> {
    /// 这一家有没有账号这一维（没有 ⇒ 跟随什么都不选）。
    pub(crate) has_accounts: bool,
    /// 这台的账号库（只在要判的时候读）。
    pub(crate) library: &'a dyn Fn() -> Library,
    /// 某条会话上次用的号。
    pub(crate) last: &'a dyn Fn(&str) -> Option<String>,
}

/// 这一趟实际用的号（应答里的 `account`）：线上形住 `cmd-wire::session`。
pub use cmd_wire::session::LaunchedAccount;

/// 判完之后交给渲染那一步的号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Settled {
    /// 不表态（继承）：跟随却什么都没选上 / 这一家没有账号这一维。
    Unsaid,
    /// 账号 0。
    Base,
    /// 账号库里的一个号。
    Account(LaunchedAccount),
}

/// 三条起会话请求共用：线上那一格 ＋ 这条会话（resume 才有）⇒ 用哪个号。选不了 ⇒ `Err`（`account_unavailable` 的 `data`）。
pub(crate) fn settle(
    asked: &AccountAsk,
    sid: Option<&str>,
    models: &BTreeMap<String, String>,
    facts: &Facts,
) -> Result<Settled, AccountUnavailable> {
    let asked = match asked {
        AccountAsk::Base => return Ok(Settled::Base),
        AccountAsk::Follow if !facts.has_accounts => return Ok(Settled::Unsaid),
        AccountAsk::Follow => Asked::Follow,
        AccountAsk::Named { name } => Asked::Named(name.clone()),
    };
    let last = match (&asked, sid) {
        (Asked::Follow, Some(sid)) => (facts.last)(sid),
        _ => None,
    };
    match pick(&asked, &(facts.library)(), last.as_deref(), None) {
        Picked::Account { name, config_dir } => Ok(Settled::Account(LaunchedAccount {
            model: models.get(&name).filter(|m| !m.is_empty()).cloned(),
            name,
            config_dir,
        })),
        Picked::Base if asked == Asked::Follow => Ok(Settled::Unsaid),
        Picked::Base => Ok(Settled::Base),
        Picked::Unavailable(u) => Err(u),
    }
}

/// 判据用：号照请求原样当已判好（判定本身由 `launch_account_tests.rs` 钉；渲染那几族只看映射）。点名的号的目录取 `/h/.cc/<名>`。
#[cfg(test)]
pub(crate) fn settled_as_asked(a: &AccountAsk, models: &BTreeMap<String, String>) -> Settled {
    match a {
        AccountAsk::Follow => Settled::Unsaid,
        AccountAsk::Base => Settled::Base,
        AccountAsk::Named { name } => Settled::Account(LaunchedAccount {
            name: name.clone(),
            config_dir: format!("/h/.cc/{name}"),
            model: models.get(name).cloned(),
        }),
    }
}

/// `account_unavailable` 那一句（界面照 `data` 自己说，这一句给命令行面与日志）。
pub(crate) fn unavailable_said(u: &AccountUnavailable) -> String {
    copy_text(
        "beLaunchAccount.unavailable.said",
        &[("name", &u.requested)],
    )
}

// ───────────────────────────── 记录 ─────────────────────────────

/// 记录文件名（落点住契约常量）。
pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::LAUNCH_ACCOUNTS_REL);
/// 便条目录名。
pub(crate) const NOTES_DIR: &str =
    relay_route_core::file_name_of(relay_route_core::LAUNCH_NOTES_DIR_REL);

/// `launch-accounts.json`（启动台账）：`sid → 号` ＋ 起会话时给了 `--tab` 的那几条 `sid → 标签页`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Book {
    pub(crate) sessions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) tabs: BTreeMap<String, LaunchTab>,
}

/// 起会话时那一格 `--tab`（线上形住 `cmd-wire`：那一台的清单每行带它）。隐藏的判定读它（`control/session_hidden.rs`）。
pub(crate) use cmd_wire::session::LaunchTab;

/// 一张便条：起这个进程时用的号 ＋ 写下的时刻（unix 秒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Note {
    /// 用的号；空串 ＝ 没用账号库里说得出名字的号（只为下一格留的便条）。
    pub(crate) account: String,
    pub(crate) at: u64,
    /// 这个进程起来时答题插件经环境交下去了（换了启动器那一支）：hook 认「是 ccm 起的那一个」· 会话事实判「启动器没把环境传下去」都看它。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) plugin: bool,
    /// 起这个进程时给了 `--tab`（别名里写的也算）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tab: Option<LaunchTab>,
}

/// 读那份记录。没有 ⇒ 空；读不懂 ⇒ `Err`（不覆盖它）。
pub(crate) fn read_book(home: &Path) -> Result<Book, crate::common::said::Said> {
    use crate::common::own_state::{read_json, Read};
    match read_json(&home.join(FILE_NAME), MAX_BYTES) {
        Read::Absent => Ok(Book::default()),
        Read::Present(b) => Ok(b),
        Read::Unreadable(why) => Err(why),
    }
}

/// 读盘的上限（一条会话一行）。
const MAX_BYTES: u64 = 16 << 20;

/// 这条会话上次用的号（读不出 / 没记 ⇒ `None`）。
pub(crate) fn last_of(home: Option<&Path>, sid: &str) -> Option<String> {
    read_book(home?).ok()?.sessions.get(sid).cloned()
}

/// `ccm` 最终那一跳：给将要跑 agent 的那个进程（`pid`）留便条（号说得出名字才记 · 答题插件是不是经环境交下去的 · `--tab`）。
/// 失败只出声（不挡起会话）。落点：写句柄的数据目录那一根（`home`）底下的便条目录。
pub(crate) fn leave_note(
    own: &Own,
    pid: u32,
    account: Option<&str>,
    plugin: bool,
    tab: Option<LaunchTab>,
    now: u64,
) -> Result<(), crate::common::said::Said> {
    write_note(
        own,
        pid,
        &Note {
            account: account.unwrap_or_default().to_string(),
            at: now,
            plugin,
            tab,
        },
    )
}

/// 这个进程（`pid`，起始时刻 `started`）的便条，且对它作数（不早于它起）。`data` ＝ 数据目录。没有 ⇒ `None`。
pub(crate) fn note_of(data: &Path, pid: u32, started: Option<u64>) -> Option<Note> {
    let f = data.join(NOTES_DIR).join(format!("{pid}.json"));
    crate::platform::fs::Ro::read_to_string(f)
        .ok()
        .and_then(|s| serde_json::from_str::<Note>(&s).ok())
        .filter(|n| fresh(n, started))
}

fn write_note(own: &Own, pid: u32, note: &Note) -> Result<(), crate::common::said::Said> {
    let (home, dir) = places(own)?;
    for d in [&home, &dir] {
        own.ensure_dir(d).map_err(|e| failed(d.as_path(), &e))?;
    }
    let file = dir.join(&format!("{pid}.json"))?;
    let body = crate::common::own_state::json_body(file.as_path(), note)?;
    // 一个进程一张、各写各的：拿便条目录那一把锁（原子写要它），不跨便条串。
    let held = own.lock(&dir)?;
    own.write(&file, &held, &body)
}

/// 数据目录本身（记录住这里）与它底下的便条目录。没有数据目录 ⇒ 拒。
fn places(own: &Own) -> Result<(OwnPath, OwnPath), crate::common::said::Said> {
    let home = own
        .root()
        .ok_or_else(|| crate::common::said::Said::from(copy_text("beOwnState.path.noHome", &[])))?;
    let dir = home.join(NOTES_DIR)?;
    Ok((home, dir))
}

/// 观测侧看见 `pid` 的会话 `sid` 那一刻（`observe::watcher::Reactions::session_seen`，由 `main.rs` 接上）：照这个进程与别的进程此刻在不在 [`adopt`]；写不成只出声。
pub fn adopt_seen(own: &Own, pid: u32, sid: &str) {
    let started_at = |p: u32| {
        crate::platform::proc::start_epoch_from_ticks(crate::platform::proc::proc_starttime(p))
    };
    let alive = |p: u32| crate::platform::proc::pid_alive(p).then(|| started_at(p));
    if let Err(e) = adopt(own, pid, sid, started_at(pid), &alive) {
        tracing::warn!("起会话账号没记上（pid {pid} · sid {sid}）：{}", e.logged());
    }
}

/// 观测侧看见 `pid` 的会话 `sid`：有它的便条、且便条不早于这个进程 ⇒ 记 `sid → 号`（便条带 `--tab` ⇒ 连同 `sid → 标签页`）；
/// 顺手清掉进程已不在的便条。
/// `started` ＝ 这个进程的起始时刻（unix 秒；说不出 ⇒ `None`）。`alive(p)` ＝ 别的 pid：不在 ⇒ `None`，在 ⇒ 它的起始时刻（说不出 ⇒ `Some(None)`）。
pub(crate) fn adopt(
    own: &Own,
    pid: u32,
    sid: &str,
    started: Option<u64>,
    alive: &dyn Fn(u32) -> Option<Option<u64>>,
) -> Result<bool, crate::common::said::Said> {
    let (home_dir, notes) = places(own)?;
    let home = home_dir.as_path();
    let Ok(rd) = crate::platform::fs::Ro::read_dir(notes.as_path()) else {
        return Ok(false);
    };
    let mut note: Option<Note> = None;
    for ent in rd.flatten() {
        let path = ent.path();
        let Some(p) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let read = crate::platform::fs::Ro::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Note>(&s).ok());
        if p == pid {
            note = read.filter(|n| fresh(n, started));
            continue;
        }
        // 别的 pid：进程不在了，或在的那个比便条晚起（pid 被复用）⇒ 陈旧，清掉。
        let stale = match (read, alive(p)) {
            (Some(n), Some(at)) => !fresh(&n, at),
            (None, _) | (_, None) => true,
        };
        if stale {
            if let Some(f) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| notes.join(n).ok())
            {
                let _ = own.delete_file(&f);
            }
        }
    }
    let Some(note) = note.filter(|n| !n.account.is_empty() || n.tab.is_some()) else {
        return Ok(false);
    };
    let book = read_book(home)?;
    let account = (!note.account.is_empty()).then_some(note.account.as_str());
    if account.is_none_or(|a| book.sessions.get(sid).map(String::as_str) == Some(a))
        && note
            .tab
            .as_ref()
            .is_none_or(|t| book.tabs.get(sid) == Some(t))
    {
        return Ok(false);
    }
    record(own, &home_dir, sid, account, note.tab.as_ref())?;
    Ok(true)
}

/// 便条对这个进程作数：进程不晚于便条起（同一秒内算对上）。起始时刻说不出 ⇒ 作数。
fn fresh(n: &Note, started: Option<u64>) -> bool {
    started.is_none_or(|s| s <= n.at)
}

/// 在跨进程锁里：读盘 → 换掉那一条 → 原子写回。读不懂的那份不覆盖。
fn record(
    own: &Own,
    home: &OwnPath,
    sid: &str,
    account: Option<&str>,
    tab: Option<&LaunchTab>,
) -> Result<(), crate::common::said::Said> {
    own.ensure_dir(home)
        .map_err(|e| failed(home.as_path(), &e))?;
    let held = own.lock(home)?;
    let mut book = read_book(home.as_path())?;
    if let Some(a) = account {
        book.sessions.insert(sid.to_string(), a.to_string());
    }
    if let Some(t) = tab {
        book.tabs.insert(sid.to_string(), t.clone());
    }
    let file = home.join(FILE_NAME)?;
    let body = crate::common::own_state::json_body(file.as_path(), &book)?;
    own.write(&file, &held, &body)
}

fn failed(path: &Path, e: &std::io::Error) -> crate::common::said::Said {
    crate::common::said::Said::with_raw(
        copy_text(
            "beLaunchAccount.write.failed",
            &[
                ("path", &path.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

codes! {
    /// `history-last-accounts` 的码。
    pub(crate) enum LastAccountsErr {
        /// 这台那份起会话账号记录读不懂。
        Unreadable = "unreadable",
    }
}

/// `history-last-accounts`：这台记着的「每条会话上次用哪个号起的」。
pub(crate) struct HistoryLastAccountsCmd;

impl crate::wire::typed::Command for HistoryLastAccountsCmd {
    const NAME: &'static str = "history-last-accounts";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = LastAccounts;
    type Err = LastAccountsErr;
}

impl crate::wire::typed::Handle for HistoryLastAccountsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<LastAccounts, LastAccountsErr>> + Send + 'static
    {
        async move { last_accounts_at(crate::platform::paths::data_home().as_deref()) }
    }
}

/// [`HistoryLastAccountsCmd`] 的本体，家是参数（推不出家 ⇒ 空表）。读不懂 ⇒ `unreadable`。
pub(crate) fn last_accounts_at(home: Option<&Path>) -> Result<LastAccounts, LastAccountsErr> {
    let book = match home {
        Some(h) => read_book(h).map_err(|e| {
            crate::wire::typed::of_fail(
                crate::wire::spec::Fail::from(("unreadable", e)),
                LastAccountsErr::Unreadable,
            )
        })?,
        None => Book::default(),
    };
    Ok(LastAccounts {
        accounts: book.sessions,
    })
}

/// `history-last-accounts` 的回包。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LastAccounts {
    /// `{sid: 账号名}`：只含真记过的那几条。
    pub(crate) accounts: BTreeMap<String, String>,
}

#[cfg(test)]
#[path = "../../../tests/backend/control/launch_account_tests.rs"]
mod tests;
