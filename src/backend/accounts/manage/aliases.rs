//! **账号的两条别名**（纯）：每个号 `<名>cc`（`-- --account <号>`）与 `<名>cct`（`-- --account <号> --ccm-tmux`）。
//!
//! 只在**建号、删号那一刻**动用户那份配置文件：建号加这两段（基于 `cc` / `cct`、只写自己的号；名字被占就跳过、说出来）；
//! 删号删掉合下来用这个号的全部段（不论名字）。平时不回补 —— 改了名、删掉其中一条，都保持用户改后的样子。账号表不另存别名名字。
//! 「账号那一形」（[`shape_of`]）也住这里：读回口据它给每条归组、说哪个号缺哪一条。

use crate::assets::aliases::profile::{self, Book, Change, Profile, ProfileEdit};
use crate::control::ccm::argv::{flag, Parsed};

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

/// 这一段是不是「账号那一形」：自己只写了号（可再加 tmux），没有交给 agent 的词 ⇒ `(号, 是否 tmux)`，号与 tmux 按合并下来的算
/// （`workcct` 基于 `cct` 只写了号，也是「work 号 ＋ tmux」）。**不看名字**（名字是用户可改的）；合不下来的那一段不算。
pub(crate) fn shape_of(book: &Book, p: &Profile) -> Option<(String, bool)> {
    let acct = flag::ACCOUNT.trim_start_matches('-');
    let tmux = flag::TMUX.trim_start_matches('-');
    let own: Vec<&str> = p.items.iter().map(|i| i.key.as_str()).collect();
    if !own.contains(&acct) || own.iter().any(|k| *k != acct && *k != tmux) || !p.agent.is_empty() {
        return None;
    }
    match profile::resolve(book, &p.name, &[]).ok()?.parsed {
        Parsed::Opts(o) => Some((o.account.clone(), o.use_tmux)),
        Parsed::Early(_) => None,
    }
}

/// 这一段合下来用的是不是这个号（不论名字、不论号是自己写的还是基于来的）。
fn uses(book: &Book, p: &Profile, account: &str) -> bool {
    matches!(
        profile::resolve(book, &p.name, &[]).map(|r| r.parsed),
        Ok(Parsed::Opts(o)) if o.account == account
    )
}

/// 建号 / 删号那一刻要对配置文件做的改动，与说给人听的那几个名字。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Planned {
    pub changes: Vec<Change>,
    /// 这一趟加进去的名字。
    pub added: Vec<String>,
    /// 名字被别的段占着、没加的那几个名字。
    pub skipped: Vec<String>,
    /// 这一趟删掉的名字。
    pub removed: Vec<String>,
}

/// 建号：给 `account` 加上 `<名>cc`（`tmux` 为真时再加 `<名>cct`），各自基于 wrapper 那两段（`cc` / `cct`，在的话）、只写自己的号。
/// 已有同一形的（不论名字）⇒ 不再加；名字被占 ⇒ 跳过、记下。`same_name` 是那种 shell 认不认两个名字是同一个。
pub(crate) fn plan_add(
    book: &Book,
    account: &str,
    tmux: bool,
    same_name: &dyn Fn(&str, &str) -> bool,
) -> Planned {
    let mut out = Planned::default();
    let wants: &[bool] = if tmux { &[false, true] } else { &[false] };
    let wrapper =
        crate::agents::sole_kind(|a| a.accounts.is_some()).and_then(crate::agents::wrapper_alias);
    let has = |n: &str| book.find(n).is_some();
    for &t in wants {
        let Some(name) = alias_name(account, t) else {
            continue;
        };
        if book
            .profiles
            .iter()
            .any(|p| shape_of(book, p).is_some_and(|(a, tt)| a == account && tt == t))
        {
            continue;
        }
        if book.profiles.iter().any(|p| same_name(&p.name, &name))
            || out.added.iter().any(|a| same_name(a, &name))
        {
            out.skipped.push(name);
            continue;
        }
        let mut ccm = vec![flag::ACCOUNT.to_string(), account.to_string()];
        let from = match (wrapper, t) {
            (Some(w), true) if has(&format!("{w}t")) => Some(format!("{w}t")),
            (Some(w), _) if has(w) => {
                if t {
                    ccm.push(flag::TMUX.to_string());
                }
                Some(w.to_string())
            }
            _ => {
                if t {
                    ccm.push(flag::TMUX.to_string());
                }
                None
            }
        };
        out.changes.push(Change::Set(ProfileEdit {
            name: name.clone(),
            from,
            agent: Vec::new(),
            ccm,
        }));
        out.added.push(name);
    }
    out
}

/// 删号：删掉合下来用这个号的全部段（不论名字）；剩下的段里基于被删那一段的，改成基于它基于的那一段（往上找到第一个没删的）。
pub(crate) fn plan_remove(book: &Book, account: &str) -> Planned {
    let mut out = Planned::default();
    let gone: Vec<&str> = book
        .profiles
        .iter()
        .filter(|p| uses(book, p, account))
        .map(|p| p.name.as_str())
        .collect();
    let up = |mut f: Option<String>| {
        while let Some(n) = f.as_deref().filter(|n| gone.contains(n)) {
            f = book.find(n).and_then(|p| p.from.clone());
        }
        f
    };
    for p in &book.profiles {
        if gone.contains(&p.name.as_str()) {
            out.changes.push(Change::Remove(p.name.clone()));
            out.removed.push(p.name.clone());
        } else if p.from.as_deref().is_some_and(|f| gone.contains(&f)) {
            let mut e = profile::edit_of(p);
            e.from = up(p.from.clone());
            out.changes.push(Change::Set(e));
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/aliases_tests.rs"]
mod tests;
