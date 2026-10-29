//! **起会话用哪个号** —— 判定只住 [`pick`]；「这条会话上次用哪个号起的」那份记录也住这里。
//!
//! - 判定：用户点名的号 · 账号 0 · 跟随（这条会话上次的号 → 这台的默认号 → 账号 0）。
//!   跟随时上次那个号选不了 ⇒ 不起、给一个显式的替代（绝不悄悄换成别的号）。
//! - 记录：`ccm` 在最终那一跳给将要跑 agent 的那个进程留一张**便条**（`launch-pending/<pid>.json`：号 ＋ 时刻，
//!   各写各的；起会话时给了 `--tab` 也记在这张上，起在宿主里的记宿主）；观测侧看见那个 pid 的会话时认便条，把 `sid → 号`（与 `sid → 标签页` · `sid → 宿主`）记进
//!   `launch-accounts.json`（启动台账：只有观测侧写，锁里原子写；形状与读住 `history/launch_book.rs`）。
//!   便条留到那个 pid 不在了才清（`/clear` 原地换 sid 也记得到）。

use crate::history::launch_book::{read_book, Book};
use crate::platform::fs::{Own, OwnPath};
use cmd_wire::accounts::{AccountRow, AccountsList};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

// 判定直接读 `accounts-list` 的成品（同一份扫描，不另读清单、不另转一张表）：能不能用是成品写好的 `selectable`
//（`acct_core::account_selectable`，已含「写了号目录」），默认号是 `meta.effectiveDefault`（`acct_core::effective_default`），这里都不另判。
// `meta.enabled = false` ⇒ 清单读不出来（说不清默认号是谁）。界面起会话 · 换号重启 · `ccm --account` / 裸 `ccm` 都只经 [`pick`]。

/// 能拿来起会话：成品说选得了（账号 0 恒不在此列）。
fn usable(a: &AccountRow) -> bool {
    !a.scanned.name.is_empty() && a.scanned.selectable
}

fn usable_named<'a>(list: &'a AccountsList, name: &str) -> Option<&'a AccountRow> {
    list.accounts
        .iter()
        .find(|a| a.scanned.name == name && usable(a))
}

/// 这台的默认号（成品写好的那一格）。
fn default_one(list: &AccountsList) -> Option<&AccountRow> {
    let d = list.meta.effective_default.as_deref()?;
    list.accounts.iter().find(|a| a.scanned.name == d)
}

/// 要的号选不了时给的那个显式替代：默认号（能用、且不是要的那个）；清单读不出 ⇒ 没有。
fn alternative_to(list: &AccountsList, requested: &str) -> Option<String> {
    default_one(list)
        .filter(|a| list.meta.enabled && usable(a) && a.scanned.name != requested)
        .map(|a| a.scanned.name.clone())
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
/// `last` ＝ 这条会话上次用的号（[`last_of`]；只有跟随看它）。跟随却什么都没选上（没有上次的号、默认号也用不了）⇒ [`Settled::Unsaid`]；
/// 点名的 · 上次的号选不了 ⇒ `Err`（不起，给一个显式的替代）。判出来的号不带模型（模型由调用方照号补，[`settle`]）。
pub(crate) fn pick(
    asked: &AccountAsk,
    list: &AccountsList,
    last: Option<&str>,
) -> Result<Settled, AccountUnavailable> {
    let refuse = |requested: &str, pinned: bool| AccountUnavailable {
        requested: requested.to_string(),
        pinned,
        list_known: list.meta.enabled,
        alternative: alternative_to(list, requested),
    };
    let take = |a: &AccountRow| {
        Settled::Account(LaunchedAccount {
            name: a.scanned.name.clone(),
            config_dir: a.scanned.config_dir.clone().unwrap_or_default(),
            model: None,
        })
    };
    match asked {
        AccountAsk::Base => Ok(Settled::Base),
        AccountAsk::Named { name } => usable_named(list, name)
            .map(take)
            .ok_or_else(|| refuse(name, false)),
        AccountAsk::Follow => match last {
            Some(p) => usable_named(list, p)
                .map(take)
                .ok_or_else(|| refuse(p, true)),
            None => Ok(default_one(list)
                .filter(|a| usable(a))
                .map_or(Settled::Unsaid, take)),
        },
    }
}

// ───────────────────────────── 起会话那几条共用的一步 ─────────────────────────────

/// 线上 `account` 那一格（三条起会话请求同一份）：线上形住 `cmd-wire::open`。
pub use cmd_wire::open::AccountAsk;

/// 判定要的事实，由入口交（control 不读观测层）。
pub(crate) struct Facts<'a> {
    /// 这一家有没有账号这一维（没有 ⇒ 跟随什么都不选）。
    pub(crate) has_accounts: bool,
    /// 这台的账号清单成品（`accounts-list`，只在要判的时候读）。
    pub(crate) library: &'a dyn Fn() -> AccountsList,
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
    match asked {
        AccountAsk::Base => return Ok(Settled::Base),
        AccountAsk::Follow if !facts.has_accounts => return Ok(Settled::Unsaid),
        _ => {}
    }
    let last = match (asked, sid) {
        (AccountAsk::Follow, Some(sid)) => (facts.last)(sid),
        _ => None,
    };
    Ok(match pick(asked, &(facts.library)(), last.as_deref())? {
        Settled::Account(mut a) => {
            a.model = models.get(&a.name).filter(|m| !m.is_empty()).cloned();
            Settled::Account(a)
        }
        other => other,
    })
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
    let name = u.requested.as_str();
    match (u.list_known, u.pinned, u.alternative.as_deref()) {
        (false, ..) => copy_text("beLaunchAccount.unavailable.listUnknown", &[("name", name)]),
        (true, true, Some(alt)) => copy_text(
            "beLaunchAccount.unavailable.lastToDefault",
            &[("name", name), ("alt", alt)],
        ),
        (true, true, None) => {
            copy_text("beLaunchAccount.unavailable.lastToBase", &[("name", name)])
        }
        (true, false, Some(alt)) => copy_text(
            "beLaunchAccount.unavailable.namedToDefault",
            &[("name", name), ("alt", alt)],
        ),
        (true, false, None) => {
            copy_text("beLaunchAccount.unavailable.namedToBase", &[("name", name)])
        }
    }
}

// ───────────────────────────── 记录 ─────────────────────────────

/// 便条目录名。
pub(crate) const NOTES_DIR: &str =
    relay_route_core::file_name_of(relay_route_core::LAUNCH_NOTES_DIR_REL);

/// 起会话时那一格 `--tab`（线上形住 `cmd-wire`：那一台的清单每行带它）。隐藏的判定读启动台账里记下的那一格。
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
    /// 这个进程经哪条别名起的（补足一组时按它数）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) alias: Option<String>,
    /// 这个进程起在哪种宿主里（最终那一跳自己的环境：托管终端 · tmux）；直路 ⇒ 不带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) host: Option<cmd_wire::frame::TerminalHost>,
}

/// 一个进程一张便条：缓存（丢了只是那一次起会话没记上号；读不懂 · 版本对不上 ⇒ 当没有）。落点 · 版本 · 读上限照表。
const NOTE: crate::common::own_state::Doc<Note> =
    crate::common::own_state::Doc::of(&relay_route_core::LAUNCH_NOTE_FILE);

/// `ccm` 最终那一跳：给将要跑 agent 的那个进程（`pid`）留便条（号说得出名字才记 · 答题插件是不是经环境交下去的 · `--tab` · 别名）。
/// 失败只出声（不挡起会话）。落点：写句柄的数据目录那一根（`home`）底下的便条目录。
pub(crate) fn leave(own: &Own, pid: u32, note: &Note) -> Result<(), crate::common::said::Said> {
    write_note(own, pid, note)
}

/// [`leave`] 不带别名的那几格写法（判据夹具用）。
#[cfg(test)]
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
            alias: None,
            host: None,
        },
    )
}

/// 这个进程（`pid`，起始时刻 `started`）的便条，且对它作数（不早于它起）。`data` ＝ 数据目录。没有 ⇒ `None`。
pub(crate) fn note_of(data: &Path, pid: u32, started: Option<u64>) -> Option<Note> {
    let f = data.join(NOTES_DIR).join(format!("{pid}.json"));
    match NOTE.read(&f) {
        crate::common::own_state::Read::Present(n) => Some(n).filter(|n| fresh(n, started)),
        _ => None,
    }
}

fn write_note(own: &Own, pid: u32, note: &Note) -> Result<(), crate::common::said::Said> {
    let (home, dir) = places(own)?;
    for d in [&home, &dir] {
        own.ensure_dir(d).map_err(|e| failed(d.as_path(), &e))?;
    }
    // 一个进程一张、各写各的：拿便条目录那一把锁（原子写要它），不跨便条串。
    let held = own.lock(&dir)?;
    NOTE.write_named(own, &dir, &format!("{pid}.json"), &held, note)
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
        let read = match NOTE.read(&path) {
            crate::common::own_state::Read::Present(n) => Some(n),
            _ => None,
        };
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
    let Some(note) = note else {
        return Ok(false);
    };
    if entered(read_book(home)?, sid, &note).is_none() {
        return Ok(false);
    }
    record(own, &home_dir, sid, &note)?;
    Ok(true)
}

/// 便条记进台账之后的那一份；跟原来一样 ⇒ `None`。号 · 标签页 · 别名有才记；宿主照便条（直路起的便条没宿主 ⇒ 摘掉旧的那一条）。
fn entered(mut book: Book, sid: &str, note: &Note) -> Option<Book> {
    let before = book.clone();
    if !note.account.is_empty() {
        book.sessions.insert(sid.to_string(), note.account.clone());
    }
    if let Some(t) = &note.tab {
        book.tabs.insert(sid.to_string(), t.clone());
    }
    if let Some(a) = &note.alias {
        book.aliases.insert(sid.to_string(), a.clone());
    }
    match note.host {
        Some(h) => book.hosts.insert(sid.to_string(), h),
        None => book.hosts.remove(sid),
    };
    (book != before).then_some(book)
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
    note: &Note,
) -> Result<(), crate::common::said::Said> {
    own.ensure_dir(home)
        .map_err(|e| failed(home.as_path(), &e))?;
    let held = own.lock(home)?;
    let Some(book) = entered(read_book(home.as_path())?, sid, note) else {
        return Ok(());
    };
    crate::history::launch_book::write_book(own, home, &held, &book)
}

fn failed(path: &Path, e: &std::io::Error) -> crate::common::said::Said {
    crate::common::said::Said::with_raw(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &path.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/control/launch_account_tests.rs"]
mod tests;
