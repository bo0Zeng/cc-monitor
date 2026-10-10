//! **号名与语义位名的字**（唯一一处）：起会话时没说是哪个号的那一个（`_`，即 `~/.claude`）叫什么、`5h` / `7d` 两个语义位叫什么。
//!
//! 后端自己拼句子时用 [`account_text`] / [`slot_text`]；出口要画号名 / 位名时读成品上的 `names`（[`with_names`]：
//! `{accounts: {码: 字}, slots: {码: 字}}`，`accounts` 只列与原名不同的那几个 —— 不在表里的号就叫它自己的名字）。
//! 出口不认 `_`、不认 `5h` / `7d`。

use copy_core::copy_text;
use serde_json::{json, Map, Value};

/// 起会话时没说是哪个号 ⇒ 记成这个（`~/.claude`）。
pub(crate) const HOME_ACCOUNT: &str = "_";

/// 一个号在人眼里叫什么：`_` ⇒「默认号」那个字；其余照原名。
pub(crate) fn account_text(name: &str) -> String {
    if name == HOME_ACCOUNT {
        copy_text("acct.home.name", &[])
    } else {
        name.to_string()
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

/// 成品上的 `names` 那一格：`accounts` 只列与原名不同的号（今天只有 `_`），`slots` 列每个语义位。
pub(crate) fn names_cell() -> Value {
    let accounts: Map<String, Value> = [HOME_ACCOUNT]
        .iter()
        .map(|a| (a.to_string(), json!(account_text(a))))
        .collect();
    let slots: Map<String, Value> = super::rotation::LINE_SLOTS
        .iter()
        .map(|w| (w.to_string(), json!(slot_text(w))))
        .collect();
    json!({"accounts": accounts, "slots": slots})
}

/// 出口那一遍：成品顶层添上 `names`（不是对象的成品不动）。
pub(crate) fn with_names(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        o.insert("names".into(), names_cell());
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/name_words_tests.rs"]
mod tests;
