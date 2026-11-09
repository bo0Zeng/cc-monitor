//! **内置的起法别名**（纯）：三个入口 `cc` · `cct`（再加 `--ccm-tmux`）· `cch`（再加 `--hosted`）与每个号的三条
//! `<名>cc` · `<名>cct` · `<名>cch`（再加 `--account <号>`），照这台的账号表现算（[`with_builtins`]），**不写进用户的配置文件**。
//! 用户那份只放自己改过的段，同名即盖过内置；建号 / 删号不动用户的文件，只照合并后的清单重生成别名文件与链接（`aliases::regenerate_via`）。
//! 名字怎么起是账号域的规则（`accounts::manage::aliases::alias_name`）。「账号那一形」（[`form_of`]）也住这里：读回口据它给每条归组。

use super::profile::{self, Book, Profile, Setting};
use crate::accounts::manage::aliases::{alias_name, suffix, IN_ORDER};
use crate::ccm_line::argv::{flag, Parsed};
use cmd_wire::session::Launch;

/// 一形在命令行上那个词（直路没有）。
fn word_of(form: Launch) -> Option<&'static str> {
    match form {
        Launch::Direct => None,
        Launch::Tmux => Some(flag::TMUX),
        Launch::Managed => Some(flag::HOSTED),
    }
}

/// 这一段是不是「账号那一形」：自己只写了号（可再加 tmux 或托管），没有交给 agent 的词 ⇒ `(号, 哪一形)`，号与形按合并下来的算
/// （`workcct` 基于 `cct` 只写了号，也是「work 号 ＋ tmux」）。**不看名字**（名字是用户可改的）；合不下来的那一段不算。
pub(crate) fn form_of(book: &Book, p: &Profile) -> Option<(String, Launch)> {
    let key = |f: &str| f.trim_start_matches('-').to_string();
    let allowed: Vec<String> = std::iter::once(flag::ACCOUNT)
        .chain(IN_ORDER.into_iter().filter_map(word_of))
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
                (true, _) => Launch::Tmux,
                (_, true) => Launch::Managed,
                _ => Launch::Direct,
            },
        )),
        Parsed::Early(_) => None,
    }
}

/// 这台起得了的几形（没有 tmux 的不带 tmux 那一形）。
fn forms(tmux: bool) -> impl Iterator<Item = Launch> {
    IN_ORDER
        .into_iter()
        .filter(move |f| tmux || *f != Launch::Tmux)
}

/// 一形的入口叫什么：默认那一家的 wrapper 名（`agents::wrapper_alias`，Claude：`cc`）＋ 那一形的后缀；那一家没有 wrapper ⇒ `None`。
fn entry(form: Launch) -> Option<String> {
    crate::agents::wrapper_alias(crate::agents::default_kind())
        .map(|w| format!("{w}{}", suffix(form)))
}

/// 一段内置的：`words` 是那几项（一项一组命令行词）。
fn built(name: String, from: Option<String>, words: Vec<Vec<String>>) -> Profile {
    Profile {
        name,
        line: 0,
        from,
        from_line: 0,
        agent: Vec::new(),
        items: words
            .into_iter()
            .map(|g| Setting {
                key: g[0].trim_start_matches('-').to_string(),
                groups: vec![g],
                line: 0,
            })
            .collect(),
        builtin: true,
    }
}

/// 内置那几段：三个入口（直路那一个不基于谁，另两形基于它、自己写上那一形的词）；再每个号三条，各基于同一形的入口、只写自己的号
/// （入口没有 ⇒ 不基于谁、自己写上那一形的词）。
fn builtins(accounts: &[String], tmux: bool) -> Vec<Profile> {
    let word = |form: Launch| word_of(form).map(|w| vec![w.to_string()]);
    let mut out = Vec::new();
    for form in forms(tmux) {
        let Some(name) = entry(form) else { continue };
        let from = (form != Launch::Direct)
            .then(|| entry(Launch::Direct))
            .flatten();
        out.push(built(name, from, word(form).into_iter().collect()));
    }
    for account in accounts {
        for form in forms(tmux) {
            let Some(name) = alias_name(account, form) else {
                continue;
            };
            let from = entry(form);
            let mut words = vec![vec![flag::ACCOUNT.to_string(), account.clone()]];
            if from.is_none() {
                words.extend(word(form));
            }
            out.push(built(name, from, words));
        }
    }
    out
}

/// **合并后的清单**：用户那份 ＋ 内置那几段（照这台的账号表 `accounts` 现算）。用户文件里写了同名的一段（写错了的也算）⇒
/// 用用户那段、内置那条不出。先后：内置那几个位子在前（三个入口，再按账号表的先后每号三条；被盖着的那一位放用户那段），其余用户的照文件里的先后。
/// 整份文件写坏了（TOML 解不开）⇒ 原样交回（一段都不认，内置的也不出：哪一段盖着内置此刻说不清）。
/// 「同名」照那种 shell 认名字的规矩（`same`；PowerShell 不分大小写）。
/// 读清单的每一处都经 `aliases::book_in`，不另合。
pub fn with_builtins(
    user: Book,
    accounts: &[String],
    tmux: bool,
    same: &dyn Fn(&str, &str) -> bool,
) -> Book {
    if user.file_problem().is_some() {
        return user;
    }
    let own = |n: &str| user.profiles.iter().find(|p| same(&p.name, n));
    let broken = |n: &str| {
        user.problems
            .iter()
            .any(|p| p.profile.as_deref().is_some_and(|x| same(x, n)))
    };
    let builtin = builtins(accounts, tmux);
    let mut profiles: Vec<Profile> = Vec::new();
    for b in &builtin {
        match own(&b.name) {
            Some(p) => profiles.push(p.clone()),
            None if !broken(&b.name) => {
                // 基于的那一位被用户那段（名字写法可不同，如 PowerShell 上的 `CC`）盖着 ⇒ 照用户那段的名字基于它。
                let mut b = b.clone();
                if let Some(f) = b.from.as_deref().and_then(|f| own(f)) {
                    b.from = Some(f.name.clone());
                }
                profiles.push(b);
            }
            None => {}
        }
    }
    profiles.extend(
        user.profiles
            .iter()
            .filter(|p| !builtin.iter().any(|b| same(&b.name, &p.name)))
            .cloned(),
    );
    Book { profiles, ..user }
}

/// 一个号那几条内置的名字（照内置的先后）：建号 / 删号那一刻说「多了 / 少了哪几条」用。
pub fn names_of(account: &str, tmux: bool) -> Vec<String> {
    forms(tmux).filter_map(|f| alias_name(account, f)).collect()
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/accounts_tests.rs"]
mod tests;
