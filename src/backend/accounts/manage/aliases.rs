//! **账号别名**（纯）：每个号一条 `<名>cc` —— `zcc() { ccm "$@" -- --account z; }` 那一形 ——
//! 由账号表推出来，并进用户那份别名清单（`~/.cc-monitor/aliases.sh`）：只增删「账号那一形」的条目，用户自己的别名一条不动。
//!
//! 「账号那一形」= 名字是 [`alias_name`] 推出来的那个、参数恰好是 `-- --account <号>`。
//! 号没了 ⇒ 它那一条删掉；号有了而清单里没有 ⇒ 追加在末尾；同名的被用户占着（参数不一样）⇒ 不盖，记一句提示。

use crate::control::ccm::argv::flag;

/// 一条别名：`(名字, ccm 参数)`。
pub(crate) type Entry = (String, Vec<String>);

/// 一个号的别名叫什么：账号名去掉 shell 函数名里放不下的字符（今天只有 `-`）＋ `cc`；以数字打头 ⇒ 前面补 `_`。
/// 去完什么都不剩 ⇒ `None`（不给它起别名）。
pub(crate) fn alias_name(account: &str) -> Option<String> {
    let cleaned: String = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let with_cc = format!("{cleaned}cc");
    Some(if with_cc.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{with_cc}")
    } else {
        with_cc
    })
}

/// 一个号那一条别名的参数。
pub(crate) fn alias_args(account: &str) -> Vec<String> {
    vec![
        flag::END.to_string(),
        flag::ACCOUNT.to_string(),
        account.to_string(),
    ]
}

/// 这一条是不是「账号那一形」：参数恰好是 `-- --account <号>`、名字恰好是那个号推出来的名字。是 ⇒ 回那个号。
fn account_of(e: &Entry) -> Option<&str> {
    match e.1.as_slice() {
        [end, acc, name] if end == flag::END && acc == flag::ACCOUNT => {
            (alias_name(name).as_deref() == Some(e.0.as_str())).then_some(name.as_str())
        }
        _ => None,
    }
}

/// 并账号表的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reconciled {
    pub list: Vec<Entry>,
    /// 这一趟之后清单里每个号那一条的名字（按账号表的顺序）。
    pub names: Vec<String>,
    /// 名字被用户别的别名占着、没有给它加的那几个号：`(号, 别名名)`。
    pub skipped: Vec<(String, String)>,
}

/// 把账号表并进清单（**纯**）。`accounts` = 此刻清单里的具名号（按清单顺序）。
pub(crate) fn reconcile(current: &[Entry], accounts: &[String]) -> Reconciled {
    let mut list: Vec<Entry> = current
        .iter()
        .filter(|e| account_of(e).is_none_or(|a| accounts.iter().any(|x| x == a)))
        .cloned()
        .collect();
    let mut names = Vec::new();
    let mut skipped = Vec::new();
    for acc in accounts {
        let Some(name) = alias_name(acc) else {
            continue;
        };
        match list.iter().find(|e| e.0 == name) {
            Some(e) if account_of(e) == Some(acc.as_str()) => names.push(name),
            Some(_) => skipped.push((acc.clone(), name)),
            None => {
                list.push((name.clone(), alias_args(acc)));
                names.push(name);
            }
        }
    }
    Reconciled {
        list,
        names,
        skipped,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/aliases_tests.rs"]
mod tests;
