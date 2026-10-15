//! **号名与语义位名的字**（唯一一处）：起会话时没说是哪个号的那一个（`_`）· 账号 0（不设配置目录的那一份登录，`~/.claude`）叫什么、
//! `5h` / `7d` 两个语义位叫什么。`_` 只说「没表态」：落在哪一份登录说不准（建了账号库的机器上可能是 shell rc 导出的那个配置根），
//! 所以它不叫 `~/.claude`，叫「未指定账号」（与额度行那一格同一条键）。
//!
//! 后端自己拼句子时用 [`account_text`] / [`slot_text`]；出口要画号名 / 位名时读成品上的 `names`（[`names`]：
//! `{accounts: {码: 字}, slots: {码: 字}}`，`accounts` 只列与原名不同的那几个 —— 不在表里的号就叫它自己的名字）。
//! 出口不认 `_`、不认 `5h` / `7d`。

use crate::common::cells::Words;
use copy_core::copy_text;

/// 起会话时没说是哪个号 ⇒ 记成这个（是哪一份登录说不准；不等于账号 0）。
pub(crate) const HOME_ACCOUNT: &str = "_";

/// 一个号能不能当换号 · 加号的候选（唯一一处）：`_`（起会话时没说是哪个号）不是 —— 它不顶任何一份登录，放上去只能回「需登录」。
/// `quota-read` 的 `candidates` 与会话那一份的候选行都调它，界面只照抄。
pub(crate) fn is_candidate(account: &str) -> bool {
    account != HOME_ACCOUNT
}

/// 一个号在人眼里叫什么：`_` ⇒「未指定账号」· 账号 0 ⇒ `~/.claude`；其余照原名。
pub(crate) fn account_text(name: &str) -> String {
    if name == HOME_ACCOUNT {
        copy_text("acct.tag.home", &[])
    } else if name == acct_core::BARE_ACCOUNT {
        copy_text("acct.home.name", &[])
    } else {
        name.to_string()
    }
}

/// 号牌上那一两个字（号色那一格里画的）：字母数字开头 ⇒ 前两个字；别的 ⇒ 第一个字；空名 ⇒ `?`。
pub(crate) fn plate_text(name: &str) -> String {
    let mut cs = name.chars();
    match cs.next() {
        None => "?".to_string(),
        Some(c) if c.is_ascii_alphanumeric() => name.chars().take(2).collect(),
        Some(c) => c.to_string(),
    }
}

/// 一个语义位在人眼里叫什么：`5h` · `7d` 各一个字；认不出的照原样。
pub(crate) fn slot_text(w: &str) -> String {
    match w {
        "5h" => copy_text("acct.slot.fiveHour", &[]),
        "7d" => copy_text("acct.slot.sevenDay", &[]),
        other => other.to_string(),
    }
}

/// 成品上的 `names` 那一格（线上形住 `cmd-wire`）。
pub(crate) use cmd_wire::quota::Names;

/// 唯一一处造 [`Names`]。
pub(crate) fn names() -> Names {
    Names {
        accounts: [HOME_ACCOUNT, acct_core::BARE_ACCOUNT]
            .iter()
            .map(|a| (a.to_string(), Words(account_text(a))))
            .collect(),
        slots: ["5h", "7d"]
            .iter()
            .map(|w| (w.to_string(), Words(slot_text(w))))
            .collect(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/name_words_tests.rs"]
mod tests;
