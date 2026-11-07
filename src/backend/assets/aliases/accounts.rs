//! **账号的三条别名**（纯）：每个号 `<名>cc`（`-- --account <号>`）· `<名>cct`（再加 `--ccm-tmux`）· `<名>cch`（再加 `--hosted`）。
//! 名字怎么起是账号域的规则（`accounts::manage::aliases::alias_name`）；按别名清单算加 / 删 / 认形住这里（它们读的是别名清单那套模型）。
//!
//! 只在**建号、删号那一刻**动用户那份配置文件：建号加这三段（基于 `cc` / `cct` / `cch`、只写自己的号；名字被占就跳过、说出来）；
//! 删号删掉合下来用这个号的全部段（不论名字）。平时不回补 —— 改了名、删掉其中一条，都保持用户改后的样子。账号表不另存别名名字。
//! 「账号那一形」（[`form_of`]）也住这里：读回口据它给每条归组、说哪个号缺哪一条。

use super::profile::{self, Book, Change, Profile, ProfileEdit};
use crate::accounts::manage::aliases::{alias_name, Form};
use crate::ccm_line::argv::{flag, Parsed};

/// 一形在命令行上那个词（直路没有）。
fn word_of(form: Form) -> Option<&'static str> {
    match form {
        Form::Plain => None,
        Form::Tmux => Some(flag::TMUX),
        Form::Hosted => Some(flag::HOSTED),
    }
}

/// 这一段是不是「账号那一形」：自己只写了号（可再加 tmux 或托管），没有交给 agent 的词 ⇒ `(号, 哪一形)`，号与形按合并下来的算
/// （`workcct` 基于 `cct` 只写了号，也是「work 号 ＋ tmux」）。**不看名字**（名字是用户可改的）；合不下来的那一段不算。
pub(crate) fn form_of(book: &Book, p: &Profile) -> Option<(String, Form)> {
    let key = |f: &str| f.trim_start_matches('-').to_string();
    let allowed: Vec<String> = std::iter::once(flag::ACCOUNT)
        .chain(Form::IN_ORDER.into_iter().filter_map(word_of))
        .map(key)
        .collect();
    let own: Vec<&str> = p.items.iter().map(|i| i.key.as_str()).collect();
    if !own.contains(&key(flag::ACCOUNT).as_str())
        || own.iter().any(|k| !allowed.iter().any(|a| a == k))
        || !p.agent.is_empty()
    {
        return None;
    }
    match profile::resolve(book, &p.name, &[]).ok()?.parsed {
        Parsed::Opts(o) => Some((
            o.account.clone(),
            match (o.use_tmux, o.hosted) {
                (true, _) => Form::Tmux,
                (_, true) => Form::Hosted,
                _ => Form::Plain,
            },
        )),
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
pub struct Planned {
    pub changes: Vec<Change>,
    /// 这一趟加进去的名字。
    pub added: Vec<String>,
    /// 名字被别的段占着、没加的那几个名字。
    pub skipped: Vec<String>,
    /// 这一趟删掉的名字。
    pub removed: Vec<String>,
}

/// 建号：给 `account` 加上 `<名>cc` · `<名>cct`（`tmux` 为真时）· `<名>cch`，各自基于 wrapper 那三段（`cc` / `cct` / `cch`，在的话）、只写自己的号；
/// 那一形的那一段不在 ⇒ 基于 `cc`（在的话）、自己写上那一形的词。
/// 已有同一形的（不论名字）⇒ 不再加；名字被占 ⇒ 跳过、记下。`same_name` 是那种 shell 认不认两个名字是同一个。
pub fn plan_add(
    book: &Book,
    account: &str,
    tmux: bool,
    same_name: &dyn Fn(&str, &str) -> bool,
) -> Planned {
    let mut out = Planned::default();
    let wrapper =
        crate::agents::sole_kind(|a| a.accounts.is_some()).and_then(crate::agents::wrapper_alias);
    let has = |n: &str| book.find(n).is_some();
    for form in Form::IN_ORDER
        .into_iter()
        .filter(|f| tmux || *f != Form::Tmux)
    {
        let Some(name) = alias_name(account, form) else {
            continue;
        };
        if book
            .profiles
            .iter()
            .any(|p| form_of(book, p).is_some_and(|(a, f)| a == account && f == form))
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
        let own_base = wrapper
            .map(|w| format!("{w}{}", form.suffix()))
            .filter(|b| has(b));
        let from = own_base.or_else(|| {
            ccm.extend(word_of(form).map(str::to_string));
            wrapper.filter(|w| has(w)).map(str::to_string)
        });
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
pub fn plan_remove(book: &Book, account: &str) -> Planned {
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
#[path = "../../../../tests/backend/assets/aliases/accounts_tests.rs"]
mod tests;
