//! **账号的两条别名**（纯）：每个号 `<名>cc`（`-- --account <号>`）与 `<名>cct`（`-- --account <号> --ccm-tmux`）。
//!
//! 只在**建号、删号那一刻**动用户那份别名清单：建号加这两条（名字被占就跳过、说出来）；删号删掉参数指向这个号的
//! 全部（不论名字）。平时不回补 —— 改了名、删掉其中一条，都保持用户改后的样子。账号表不另存别名名字。
//! 「账号那一形」（[`shape_of`]）也住这里：读回口据它给每条归组、说哪个号缺哪一条。

use crate::control::ccm::argv::flag;
use crate::platform::shell::dialect::RestTo;

/// 一条别名：`(名字, ccm 参数, 调用时的词交给谁)`。
pub(crate) type Entry = (String, Vec<String>, RestTo);

/// 一个号那一条别名叫什么：账号名去掉 shell 函数名里放不下的字符（今天只有 `-`）＋ 账号库那一家的 wrapper 名
/// （`agents::wrapper_alias`，Claude：`cc`；`tmux` ⇒ 再加 `t`）；以数字打头 ⇒ 前面补 `_`。
/// 去完什么都不剩、或那一家没有 wrapper ⇒ `None`（不给它起别名）。
pub(crate) fn alias_name(account: &str, tmux: bool) -> Option<String> {
    let cleaned: String = account
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let wrapper = crate::agents::sole_kind(|a| a.accounts.is_some())
        .and_then(crate::agents::wrapper_alias)?;
    let name = format!("{cleaned}{wrapper}{}", if tmux { "t" } else { "" });
    Some(if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    })
}

/// 一个号那一条别名的参数。
pub(crate) fn alias_args(account: &str, tmux: bool) -> Vec<String> {
    let mut v = vec![
        flag::END.to_string(),
        flag::ACCOUNT.to_string(),
        account.to_string(),
    ];
    if tmux {
        v.push(flag::TMUX.to_string());
    }
    v
}

/// 这一条是不是「账号那一形」：`--` 左边没有词、调用时的词交给 claude、右边恰是 `--account <号>`（可再带一个
/// `--ccm-tmux`，先后不论）。是 ⇒ `(号, 是否 tmux)`。**不看名字**（名字是用户可改的）。
pub(crate) fn shape_of(args: &[String], rest: RestTo) -> Option<(String, bool)> {
    if rest != RestTo::Agent || args.first().map(String::as_str) != Some(flag::END) {
        return None;
    }
    let right = &args[1..];
    if right.iter().any(|w| w == flag::END) {
        return None;
    }
    let at = right.iter().position(|w| w == flag::ACCOUNT)?;
    let account = right.get(at + 1)?.clone();
    let others: Vec<&String> = right
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != at && *i != at + 1)
        .map(|(_, w)| w)
        .collect();
    match others.as_slice() {
        [] => Some((account, false)),
        [t] if t.as_str() == flag::TMUX => Some((account, true)),
        _ => None,
    }
}

/// 这一条的参数是不是**指向**这个号（ccm 那一半里 `--account <号>`，不论别的参数、不论名字）。
pub(crate) fn points_at(args: &[String], account: &str) -> bool {
    let Some(k) = args.iter().rposition(|w| w == flag::END) else {
        return false;
    };
    args[k + 1..]
        .windows(2)
        .any(|w| w[0] == flag::ACCOUNT && w[1] == account)
}

/// 建号那一刻并进清单的结局。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Added {
    pub list: Vec<Entry>,
    /// 这一趟加进去的名字。
    pub added: Vec<String>,
    /// 名字被别的别名占着、没加的那几个名字。
    pub skipped: Vec<String>,
}

/// 建号：给 `account` 加上 `<名>cc`（`tmux` 为真时再加 `<名>cct`）。清单里已有同一形的（不论名字）⇒ 不再加；
/// 名字被占（参数不一样）⇒ 跳过、记下。`same_name` 是那种 shell 认不认两个名字是同一个（PowerShell 不分大小写）。
pub(crate) fn on_add(
    current: &[Entry],
    account: &str,
    tmux: bool,
    same_name: &dyn Fn(&str, &str) -> bool,
) -> Added {
    let mut out = Added {
        list: current.to_vec(),
        ..Added::default()
    };
    let wants: &[bool] = if tmux { &[false, true] } else { &[false] };
    for &t in wants {
        let Some(name) = alias_name(account, t) else {
            continue;
        };
        let have = out.list.iter().any(|e| {
            shape_of(&e.1, e.2)
                .as_ref()
                .is_some_and(|(a, tt)| a == account && *tt == t)
        });
        if have {
            continue;
        }
        if out.list.iter().any(|e| same_name(&e.0, &name)) {
            out.skipped.push(name);
            continue;
        }
        out.list
            .push((name.clone(), alias_args(account, t), RestTo::Agent));
        out.added.push(name);
    }
    out
}

/// 删号：删掉参数指向 `account` 的全部（不论名字）。回 `(清单, 删掉的名字)`。
pub(crate) fn on_remove(current: &[Entry], account: &str) -> (Vec<Entry>, Vec<String>) {
    let (gone, kept): (Vec<Entry>, Vec<Entry>) = current
        .iter()
        .cloned()
        .partition(|e| points_at(&e.1, account));
    (kept, gone.into_iter().map(|e| e.0).collect())
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/aliases_tests.rs"]
mod tests;
