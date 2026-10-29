//! **启动台账**（`launch-accounts.json`）的形状与读：`sid → 号` ＋ 起会话时给了 `--tab` 的那几条 `sid → 标签页` ＋ 经别名起的那几条 `sid → 别名`
//! ＋ 起在宿主里的那几条 `sid → 宿主`。
//! 历史清单并进每行的「上次用哪个号」· 隐藏的判定（起会话时那一格）· 起会话跟随上次的号 · 恢复照起时那一种都读它。
//! 只有观测侧看见会话那一刻写它（认便条、锁里原子写：`control::launch_account::adopt`）。

use crate::wire::typed::codes;
use cmd_wire::session::LaunchTab;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 记录文件名（落点住契约常量）。
pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::LAUNCH_ACCOUNTS_REL);

/// 读盘的上限（一条会话一行）：照表。
pub(crate) const MAX_BYTES: u64 = relay_route_core::LAUNCH_ACCOUNTS_FILE.row.cap;

/// 启动台账：状态（读不懂不覆盖；落点 · 版本 · 读上限照表）。
const BOOK: crate::common::own_state::Doc<Book> =
    crate::common::own_state::Doc::of(&relay_route_core::LAUNCH_ACCOUNTS_FILE);

/// `launch-accounts.json`（启动台账）：`sid → 号` ＋ 起会话时给了 `--tab` 的那几条 `sid → 标签页`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Book {
    pub(crate) sessions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) tabs: BTreeMap<String, LaunchTab>,
    /// 经别名起的那几条 `sid → 别名`（补足一组时按它数「用这一行的别名起的」）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) aliases: BTreeMap<String, String>,
    /// 起在宿主里的那几条 `sid → 宿主`（tmux · 托管；直路起的不记）：恢复照起时那一种读它（[`host_of`]）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) hosts: BTreeMap<String, cmd_wire::frame::TerminalHost>,
}

/// 读那份记录。没有 ⇒ 空；读不懂 ⇒ `Err`（不覆盖它）。
pub(crate) fn read_book(home: &Path) -> Result<Book, crate::common::said::Said> {
    use crate::common::own_state::Read;
    match BOOK.read(&home.join(FILE_NAME)) {
        Read::Absent => Ok(Book::default()),
        Read::Present(b) => Ok(b),
        Read::Unreadable(why) => Err(why),
    }
}

/// 整份写回（`dir` 是数据目录那一根，`held` 是它的锁；顶层带声明的版本）。写者只有 `control::launch_account::adopt`。
pub(crate) fn write_book(
    own: &crate::platform::fs::Own,
    dir: &crate::platform::fs::OwnPath,
    held: &crate::platform::fs::OwnLock,
    book: &Book,
) -> Result<(), crate::common::said::Said> {
    BOOK.write(own, dir, held, book)
}

/// 这条会话上次用的号（读不出 / 没记 ⇒ `None`）。
pub(crate) fn last_of(home: Option<&Path>, sid: &str) -> Option<String> {
    read_book(home?).ok()?.sessions.get(sid).cloned()
}

/// 这条会话上次起在哪种宿主里（读不出 / 没记 · 直路起的 ⇒ `None`）。
pub(crate) fn host_of(home: Option<&Path>, sid: &str) -> Option<cmd_wire::frame::TerminalHost> {
    read_book(home?).ok()?.hosts.get(sid).copied()
}

codes! {
    /// 读「每条会话上次用哪个号起的」那份记录的码。
    pub(crate) enum LastAccountsErr {
        /// 这台那份起会话账号记录读不懂。
        Unreadable = "unreadable",
    }
}

/// 这台记着的「每条会话上次用哪个号起的」（历史清单并进每行；一条会话的号那一份成品逐条读 [`last_of`]），
/// 家是参数（推不出家 ⇒ 空表）。读不懂 ⇒ `unreadable`。
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

/// [`last_accounts_at`] 读出来的那一份。
#[derive(Debug, Serialize)]
pub(crate) struct LastAccounts {
    /// `{sid: 账号名}`：只含真记过的那几条。
    pub(crate) accounts: BTreeMap<String, String>,
}
