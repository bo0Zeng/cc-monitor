//! **核对**（只读）：结构 · 隔离 · 共享逐项核一遍 —— 纯：快照进、报告出。命令那一层先对齐账号库再拍快照
//! （`wire::run_verify`），所以这里看见的缺链接都是补不上的。
//!
//! 致命（`fail`）：身份没隔离开（身份文件是链接 · 两个号邮箱相同 · 共享库里留着原生根在家目录的那份身份）、
//! 权限不对（号的目录不是 `0700` · 凭据不是 `0600`）、共享没接上（缺链接 · 链错地方 · 断链 · 共享项在号里是实体文件）。
//! 提示（`warn`）：还没登录 · 共享库本身的源头断了 · 身份本体哪儿都找不到 · 按需目录在号里各有一份自己的。
//! 既不是身份、又不在共享集里的名字（共享库顶层与各号里的实体）各号各用各的：全报告一条 `ok` 级「没归类」列出来，不提示。

use super::layout::{identity_of, of_store_family, share_items_of};
use super::model::Manifest;
use super::scan::{join, Snapshot};
use crate::platform::acct_view::Item;
use cmd_wire::accounts::{CheckLevel, VerifyCheck, VerifyReport};
use copy_core::copy_text;

struct Checks {
    checks: Vec<VerifyCheck>,
}

impl Checks {
    fn push(&mut self, level: CheckLevel, account: Option<&str>, text: String) {
        self.checks.push(VerifyCheck {
            level,
            account: account.map(str::to_string),
            text,
        });
    }
}

/// 核一遍。`names` 非空 ⇒ 只核这几个号（全局那几条照核）。清单不在 / 读不动 ⇒ 一条 `fail` 说清楚。
pub(crate) fn verify(s: &Snapshot, names: &[String]) -> VerifyReport {
    verify_among(crate::agents::REGISTRY, s, names)
}

/// [`verify`] 的可喂注册表那一半：共享库那一侧照缺省那一家（清单里没写是哪一家的号归它）的身份 · 共享两张表核，
/// 每个号照它那一家的身份表核身份与凭据；共享库不是它那一家的（[`of_store_family`]）⇒ 不按共享表核链接、不报缺链。
pub(crate) fn verify_among(
    registry: &[crate::agents::Adapter],
    s: &Snapshot,
    names: &[String],
) -> VerifyReport {
    let mut o = Checks { checks: Vec::new() };
    let r = s.roots();
    let face_of = |agent: Option<&str>| {
        crate::agents::account_kind_among(registry, agent)
            .and_then(|k| crate::agents::accounts_face_among(registry, k))
    };
    let lib = face_of(None);
    let lib_identity = lib.as_ref().map(identity_of).unwrap_or_default();
    let is_identity = |n: &str| lib_identity.iter().any(|(i, _, _)| *i == n);
    let config_file = lib.as_ref().map_or("", |f| f.config_file);
    let on_demand = lib.as_ref().map_or(&[][..], |f| f.shared_on_demand);
    let m: &Manifest = match &s.manifest {
        None => {
            o.push(
                CheckLevel::Fail,
                None,
                copy_text("beAcctVerify.global.noManifest", &[("path", &r.manifest())]),
            );
            return finish(o);
        }
        Some(Err(e)) => {
            o.push(
                CheckLevel::Fail,
                None,
                copy_text("beAcctVerify.global.roots", &[("why", e)]),
            );
            return finish(o);
        }
        Some(Ok(m)) => m,
    };
    if let Err(e) = r.check() {
        o.push(
            CheckLevel::Fail,
            None,
            copy_text("beAcctVerify.global.roots", &[("why", &e)]),
        );
        return finish(o);
    }
    // 全局那几条只数共享库那一家的号：别家的号不链共享库，也不照这一家的身份表看。
    let store_family: Vec<_> = m
        .managed()
        .filter(|a| of_store_family(registry, a.agent.as_deref()))
        .collect();
    let inplace = store_family.iter().any(|a| a.config_dir == r.shared);
    let isolated = store_family.iter().any(|a| a.config_dir != r.shared);
    let home_cj = join(&r.home, config_file);
    if inplace {
        let shared_cj = join(&r.shared, config_file);
        let split = shared_cj != home_cj
            && s.shared.get(config_file).exists()
            && s.home_items.get(config_file).is_some_and(Item::exists);
        let text = if split {
            copy_text(
                "beAcctVerify.global.inPlaceSplit",
                &[("shared", &shared_cj), ("home", &home_cj)],
            )
        } else {
            copy_text("beAcctVerify.global.inPlace", &[("home", &home_cj)])
        };
        o.push(CheckLevel::Warn, None, text);
    }
    if isolated {
        let mut zero = false;
        for (item, home_rooted, secret) in lib_identity.iter().copied() {
            if !secret || !s.shared.get(item).exists() {
                continue;
            }
            if home_rooted {
                if inplace {
                    o.push(
                        CheckLevel::Skip,
                        None,
                        copy_text("beAcctVerify.global.residueInPlace", &[("item", item)]),
                    );
                } else {
                    o.push(
                        CheckLevel::Fail,
                        None,
                        copy_text(
                            "beAcctVerify.global.residue",
                            &[("item", item), ("shared", &r.shared)],
                        ),
                    );
                }
            } else {
                zero = true;
            }
        }
        let text = if zero {
            copy_text("beAcctVerify.global.zeroIn", &[])
        } else {
            copy_text("beAcctVerify.global.zeroOut", &[])
        };
        o.push(CheckLevel::Ok, None, text);
        if !inplace && s.home_items.get(config_file).is_some_and(Item::exists) {
            o.push(
                CheckLevel::Warn,
                None,
                copy_text("beAcctVerify.global.homeConfig", &[("path", &home_cj)]),
            );
        }
    }
    for x in names {
        if m.find(x).is_none() {
            o.push(
                CheckLevel::Fail,
                None,
                copy_text("beAcctPlan.account.unknown", &[("name", x)]),
            );
        }
    }

    let items = lib
        .as_ref()
        .map(|f| share_items_of(f, &s.shared))
        .unwrap_or_default();
    // `(号, 邮箱, 那一家)`：重复只在同一家的号之间判（同一个人两家各登一个号是常态）。
    let mut emails: Vec<(String, String, Option<&'static str>)> = Vec::new();
    // 按需才建的共用目录在号里是自己的一份（两个号以上各有一份，起来时那一步不替人合并）：`(项, 哪几个号)`，最后一项一条提示。
    let mut own_copies: Vec<(String, Vec<String>)> = Vec::new();
    // 既不是身份、又不在共享集里的名字（共享库顶层的 ＋ 各号里的实体），去重排好序，最后一条「没归类」。
    let mut unclassified: std::collections::BTreeSet<String> = s
        .shared
        .entries
        .keys()
        .filter(|n| !is_identity(n) && !items.contains(n))
        .cloned()
        .collect();
    for a in m.managed() {
        if !names.is_empty() && !names.contains(&a.name) {
            continue;
        }
        let n = Some(a.name.as_str());
        let c = &a.config_dir;
        if !r.controls(c) {
            o.push(
                CheckLevel::Fail,
                n,
                copy_text(
                    "beAcctVerify.acct.outside",
                    &[("path", c), ("accts", &r.accts)],
                ),
            );
            continue;
        }
        let d = s.dir(c);
        if !d.is_dir() {
            o.push(
                CheckLevel::Fail,
                n,
                copy_text("beExt.uninstall.notThere", &[("path", c)]),
            );
            continue;
        }
        let in_place = *c == r.shared;
        match d.mode() {
            Some(0o700) | None => o.push(
                CheckLevel::Ok,
                n,
                copy_text("beAcctVerify.acct.dirMode", &[]),
            ),
            Some(mo) if in_place => o.push(
                CheckLevel::Warn,
                n,
                copy_text(
                    "beAcctVerify.acct.dirModeInPlace",
                    &[("mode", &format!("{mo:o}"))],
                ),
            ),
            Some(mo) => o.push(
                CheckLevel::Fail,
                n,
                copy_text(
                    "beAcctVerify.acct.dirModeBad",
                    &[("mode", &format!("{mo:o}"))],
                ),
            ),
        }
        // 凭据那一项：这个号那一家声明的凭据文件名（号表判「已登录」问的同一格，`agents::creds_file_among`）；没声明 ⇒ 不判。
        let agent = a.agent.as_deref();
        let kind = crate::agents::account_kind_among(registry, agent);
        let creds = kind.and_then(|k| crate::agents::creds_file_among(registry, k));
        let own = face_of(agent).as_ref().map(identity_of).unwrap_or_default();
        for (item, _, secret) in own {
            match d.get(item) {
                Item::Link { .. } => o.push(
                    CheckLevel::Fail,
                    n,
                    copy_text("beAcctVerify.acct.identityLink", &[("item", item)]),
                ),
                Item::File { mode, .. } if secret && Some(item) == creds => match mode {
                    Some(0o600) | None => o.push(
                        CheckLevel::Ok,
                        n,
                        copy_text("beAcctVerify.acct.credOk", &[("item", item)]),
                    ),
                    Some(mo) => o.push(
                        CheckLevel::Fail,
                        n,
                        copy_text(
                            "beAcctVerify.acct.credMode",
                            &[("mode", &format!("{mo:o}"))],
                        ),
                    ),
                },
                Item::Absent if Some(item) == creds => {
                    if !a.is_api_key() {
                        o.push(
                            CheckLevel::Warn,
                            n,
                            copy_text("beAcctVerify.acct.notLoggedIn", &[]),
                        );
                    }
                }
                Item::Absent => {}
                _ => o.push(
                    CheckLevel::Ok,
                    n,
                    copy_text("beAcctVerify.acct.identityOwn", &[("item", item)]),
                ),
            }
        }
        if in_place {
            o.push(
                CheckLevel::Skip,
                n,
                copy_text("beAcctVerify.acct.inPlaceLinks", &[]),
            );
        } else if of_store_family(registry, agent) {
            let (mut missing, mut bad) = (0usize, false);
            for it in &items {
                let want = join(&r.shared, it);
                match d.get(it) {
                    Item::Absent => missing += 1,
                    Item::Link { target, .. } if *target != want => {
                        bad = true;
                        o.push(
                            CheckLevel::Fail,
                            n,
                            copy_text(
                                "beAcctVerify.acct.linkWrong",
                                &[("item", it), ("target", target), ("want", &want)],
                            ),
                        );
                    }
                    Item::Link { dangling: true, .. } => {
                        if let Item::Link {
                            target,
                            dangling: true,
                        } = s.shared.get(it)
                        {
                            o.push(
                                CheckLevel::Warn,
                                n,
                                copy_text(
                                    "beAcctVerify.acct.sourceBroken",
                                    &[("item", it), ("source", &want), ("target", target)],
                                ),
                            );
                        } else {
                            bad = true;
                            o.push(
                                CheckLevel::Fail,
                                n,
                                copy_text("beAcctVerify.acct.linkBroken", &[("item", it)]),
                            );
                        }
                    }
                    Item::Link { .. } => {}
                    _ if on_demand.contains(&it.as_str()) => {
                        match own_copies.iter_mut().find(|(i, _)| i == it) {
                            Some((_, names)) => names.push(a.name.clone()),
                            None => own_copies.push((it.clone(), vec![a.name.clone()])),
                        }
                    }
                    _ => {
                        bad = true;
                        o.push(
                            CheckLevel::Fail,
                            n,
                            copy_text("beAcctVerify.acct.linkEntity", &[("item", it)]),
                        );
                    }
                }
            }
            if missing > 0 {
                o.push(
                    CheckLevel::Fail,
                    n,
                    copy_text(
                        "beAcctVerify.acct.linkMissing",
                        &[("n", &missing.to_string())],
                    ),
                );
            } else if !bad {
                o.push(
                    CheckLevel::Ok,
                    n,
                    copy_text(
                        "beAcctVerify.acct.linksOk",
                        &[("n", &items.len().to_string())],
                    ),
                );
            }
            unclassified.extend(
                d.entries
                    .iter()
                    .filter(|(base, it)| {
                        !it.is_link() && !is_identity(base) && !items.contains(base)
                    })
                    .map(|(base, _)| base.clone()),
            );
        }
        match s.email_of(c) {
            Some(e) => {
                o.push(
                    CheckLevel::Ok,
                    n,
                    copy_text("beAcctVerify.acct.email", &[("email", e)]),
                );
                emails.push((a.name.clone(), e.to_string(), kind));
            }
            None => o.push(
                CheckLevel::Skip,
                n,
                copy_text("beAcctVerify.acct.noEmail", &[]),
            ),
        }
    }
    for (item, names) in &own_copies {
        o.push(
            CheckLevel::Warn,
            None,
            copy_text(
                "beAcctVerify.global.ownCopies",
                &[
                    ("item", item),
                    ("names", &names.join(&copy_text("kit.text.sep", &[]))),
                ],
            ),
        );
    }

    if emails.len() < 2 {
        o.push(
            CheckLevel::Skip,
            None,
            copy_text("beAcctVerify.iso.tooFew", &[]),
        );
    } else {
        let mut dup = false;
        for (i, (na, ea, ka)) in emails.iter().enumerate() {
            for (nb, eb, kb) in &emails[i + 1..] {
                if ea == eb && ka == kb {
                    dup = true;
                    o.push(
                        CheckLevel::Fail,
                        None,
                        copy_text(
                            "beAcctVerify.iso.dup",
                            &[("a", na), ("b", nb), ("email", ea)],
                        ),
                    );
                }
            }
        }
        if !dup {
            o.push(
                CheckLevel::Ok,
                None,
                copy_text(
                    "beAcctVerify.iso.distinct",
                    &[("n", &emails.len().to_string())],
                ),
            );
        }
    }

    // 只问身份本体：本机状态那几项是那一家用到才建的，哪儿都没有是常态，说不明这张表过期了。
    // 共享库那一家一个号都没有 ⇒ 不问（找不到是常态）。
    for (item, _, _) in lib_identity
        .iter()
        .copied()
        .filter(|(_, _, secret)| *secret && !store_family.is_empty())
    {
        let found = s.shared.get(item).exists()
            || s.home_items.get(item).is_some_and(Item::exists)
            || store_family
                .iter()
                .any(|a| s.dir(&a.config_dir).get(item).exists());
        if !found {
            o.push(
                CheckLevel::Warn,
                None,
                copy_text("beAcctVerify.drift.nowhere", &[("item", item)]),
            );
        }
    }
    if !unclassified.is_empty() {
        let sep = copy_text("kit.text.sep", &[]);
        let list: Vec<&str> = unclassified.iter().map(String::as_str).collect();
        o.push(
            CheckLevel::Ok,
            None,
            copy_text(
                "beAcctVerify.global.unclassified",
                &[
                    ("n", &unclassified.len().to_string()),
                    ("items", &list.join(&sep)),
                ],
            ),
        );
    }
    finish(o)
}

fn finish(o: Checks) -> VerifyReport {
    let failed: Vec<&VerifyCheck> = o
        .checks
        .iter()
        .filter(|c| c.level == CheckLevel::Fail)
        .collect();
    let fails = failed.len();
    let headline = failed.first().map(|first| {
        let what = first.text.as_str();
        let line = match first.account.as_deref() {
            Some(name) => copy_text("acctPage.verify.bar", &[("name", name), ("what", what)]),
            None => copy_text("acctPage.verify.barGlobal", &[("what", what)]),
        };
        if fails > 1 {
            let more = copy_text("kit.dialog.more", &[("n", &(fails - 1).to_string())]);
            [line, more].join(&copy_text("kit.text.sep", &[]))
        } else {
            line
        }
    });
    VerifyReport {
        pass: fails == 0,
        fails: fails as u32,
        checks: o.checks,
        headline,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/verify_tests.rs"]
mod tests;
