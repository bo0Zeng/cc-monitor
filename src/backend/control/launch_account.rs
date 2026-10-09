//! **起会话用哪个号** —— 判定只住 [`pick`]；「这条会话上次用哪个号起的」那份记录也住这里。
//!
//! - 判定：用户点名的号 · 账号 0 · 跟随（这条会话上次的号 → 这台的默认号 → 账号 0）。
//!   跟随时上次那个号选不了 ⇒ 不起、给一个显式的替代（绝不悄悄换成别的号）。
//! - 记录：`ccm` 在最终那一跳给将要跑 agent 的那个进程留一张**便条**（`launch-pending/<pid>.json`：号 ＋ 时刻，
//!   各写各的）；观测侧看见那个 pid 的会话时认便条，把 `sid → 号` 记进 `launch-accounts.json`（只有观测侧写，锁里原子写）。
//!   便条留到那个 pid 不在了才清（`/clear` 原地换 sid 也记得到）。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    pub(crate) is_default: bool,
    /// 能拿来起会话：隔离模式 · 鉴权前提就绪 · 目录在 · 有账号目录（账号 0 恒不在此列）。
    pub(crate) usable: bool,
}

/// 这台的账号库。`known = false` ⇒ 清单读不出来（说不清默认号是谁）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Library {
    pub(crate) known: bool,
    pub(crate) accounts: Vec<Candidate>,
}

impl Library {
    /// `accounts-list` 的成品 `{meta, accounts, notice}` ⇒ 判定要的那一份（同一份扫描，不另读 manifest）。
    pub(crate) fn of_product(v: &Value) -> Library {
        let known = v["meta"]["enabled"].as_bool() == Some(true);
        let accounts = v["accounts"]
            .as_array()
            .map(|a| a.iter().filter_map(candidate_of).collect())
            .unwrap_or_default();
        Library { known, accounts }
    }

    fn usable(&self, name: &str) -> Option<&Candidate> {
        self.accounts.iter().find(|a| a.name == name && a.usable)
    }

    /// 这台的默认号：manifest 里 `isDefault` 的第一个，没有 ⇒ 第一个。
    fn default_one(&self) -> Option<&Candidate> {
        self.accounts
            .iter()
            .find(|a| a.is_default)
            .or_else(|| self.accounts.first())
    }

    /// 要的号选不了时给的那个显式替代：默认号（能用、且不是要的那个）；清单读不出 ⇒ 没有。
    fn alternative_to(&self, requested: &str) -> Option<String> {
        self.default_one()
            .filter(|a| self.known && a.usable && a.name != requested)
            .map(|a| a.name.clone())
    }
}

fn candidate_of(a: &Value) -> Option<Candidate> {
    let name = a["name"].as_str().filter(|n| !n.is_empty())?.to_string();
    let config_dir = a["configDir"]
        .as_str()
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    let usable = a["mode"].as_str() == Some("isolated")
        && a["authReady"].as_bool() == Some(true)
        && a["exists"].as_bool() == Some(true)
        && config_dir.is_some();
    Some(Candidate {
        name,
        config_dir,
        is_default: a["isDefault"].as_bool() == Some(true),
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// 这一趟实际用的号（应答里的 `account`；账号 0 / 不指定 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct LaunchedAccount {
    pub name: String,
    pub config_dir: String,
    /// 这台模型偏好表里这个号的那一条（没有 ⇒ `null`）。
    pub model: Option<String>,
}

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

/// `launch-accounts.json`：`sid → 号`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Book {
    pub(crate) sessions: BTreeMap<String, String>,
}

/// 一张便条：起这个进程时用的号 ＋ 写下的时刻（unix 秒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Note {
    pub(crate) account: String,
    pub(crate) at: u64,
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

/// `ccm` 最终那一跳：给将要跑 agent 的那个进程（`pid`）留便条。失败只出声（不挡起会话）。
pub(crate) fn leave_note(
    home: &Path,
    pid: u32,
    account: &str,
    now: u64,
) -> Result<(), crate::common::said::Said> {
    let dir = home.join(NOTES_DIR);
    for d in [home, dir.as_path()] {
        crate::common::own_dir::ensure_private_dir(d).map_err(|e| failed(d, &e))?;
    }
    crate::common::own_state::write_json(
        &dir.join(format!("{pid}.json")),
        &Note {
            account: account.to_string(),
            at: now,
        },
    )
}

/// 观测侧看见 `pid` 的会话 `sid`：有它的便条、且便条不早于这个进程 ⇒ 记 `sid → 号`；顺手清掉进程已不在的便条。
/// `started` ＝ 这个进程的起始时刻（unix 秒；说不出 ⇒ `None`）。`alive(p)` ＝ 别的 pid：不在 ⇒ `None`，在 ⇒ 它的起始时刻（说不出 ⇒ `Some(None)`）。
pub(crate) fn adopt(
    home: &Path,
    pid: u32,
    sid: &str,
    started: Option<u64>,
    alive: &dyn Fn(u32) -> Option<Option<u64>>,
) -> Result<bool, crate::common::said::Said> {
    let dir = home.join(NOTES_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
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
        let read = std::fs::read_to_string(&path)
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
            let _ = std::fs::remove_file(&path);
        }
    }
    let Some(note) = note else {
        return Ok(false);
    };
    if read_book(home)?.sessions.get(sid) == Some(&note.account) {
        return Ok(false);
    }
    record(home, sid, &note.account)?;
    Ok(true)
}

/// 便条对这个进程作数：进程不晚于便条起（同一秒内算对上）。起始时刻说不出 ⇒ 作数。
fn fresh(n: &Note, started: Option<u64>) -> bool {
    started.is_none_or(|s| s <= n.at)
}

/// 在跨进程锁里：读盘 → 换掉那一条 → 原子写回。读不懂的那份不覆盖。
fn record(home: &Path, sid: &str, account: &str) -> Result<(), crate::common::said::Said> {
    crate::common::own_dir::ensure_private_dir(home).map_err(|e| failed(home, &e))?;
    let _lock = crate::platform::lock::hold(home).map_err(crate::common::said::Said::from)?;
    let mut book = read_book(home)?;
    book.sessions.insert(sid.to_string(), account.to_string());
    crate::common::own_state::write_json(&home.join(FILE_NAME), &book)
}

fn failed(path: &Path, e: &dyn std::fmt::Display) -> String {
    copy_text(
        "beLaunchAccount.write.failed",
        &[("path", &path.display().to_string()), ("e", &e.to_string())],
    )
}

/// `history-last-accounts`：这台记着的 `{accounts: {sid: 号}}`。读不懂 ⇒ `unreadable`。
pub(crate) fn answer_last_accounts() -> Result<Value, crate::stream::inbound::spec::Fail> {
    let book = match crate::platform::paths::data_home() {
        Some(h) => read_book(&h)
            .map_err(|e| crate::stream::inbound::spec::Fail::from(("unreadable", e)))?,
        None => Book::default(),
    };
    Ok(serde_json::json!({ "accounts": book.sessions }))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/launch_account_tests.rs"]
mod tests;
