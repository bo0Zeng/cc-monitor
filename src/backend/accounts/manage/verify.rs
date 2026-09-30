//! **核对**（只读）：结构 · 隔离 · 共享逐项核一遍 —— 纯：快照进、报告出。
//!
//! 致命（`fail`）：身份没隔离开（身份文件是链接 · 两个号邮箱相同 · 共享库里留着原生根在家目录的那份身份）、
//! 权限不对（号的目录不是 `0700` · 凭据不是 `0600`）、共享没接上（缺链接 · 链错地方 · 断链 · 共享项在号里是实体文件 · 一个共享项都没有）。
//! 提示（`warn`）：还没登录 · 号里有意料之外的实体项 · 共享库本身的源头断了 · 身份表里的某项哪儿都找不到 ·
//! 共享库顶层有一份 `0600` 的文件却不在身份表里（它会被链给每个号）。

use super::layout::{identity, is_excluded, is_identity, share_items};
use super::model::Manifest;
use super::scan::{join, Snapshot};
use crate::platform::acct_view::Item;
use acct_core::wire::{CheckLevel, VerifyCheck, VerifyReport};
use copy_core::copy_text;

struct Out {
    checks: Vec<VerifyCheck>,
}

impl Out {
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
    let mut o = Out { checks: Vec::new() };
    let r = s.roots();
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
                copy_text("beAcctVerify.global.badManifest", &[("e", e)]),
            );
            return finish(o);
        }
        Some(Ok(m)) => m,
    };
    if let Err(e) = r.check() {
        o.push(
            CheckLevel::Fail,
            None,
            copy_text("beAcctVerify.global.roots", &[("e", &e)]),
        );
        return finish(o);
    }
    let inplace = m.managed().any(|a| a.config_dir == r.shared);
    let isolated = m.managed().any(|a| a.config_dir != r.shared);
    let home_cj = join(&r.home, super::layout::identity_config_file());
    if inplace {
        let shared_cj = join(&r.shared, super::layout::identity_config_file());
        let split = shared_cj != home_cj
            && s.shared.get(super::layout::identity_config_file()).exists()
            && s.home_items
                .get(super::layout::identity_config_file())
                .is_some_and(Item::exists);
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
        for (item, home_rooted, secret) in identity() {
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
        if !inplace
            && s.home_items
                .get(super::layout::identity_config_file())
                .is_some_and(Item::exists)
        {
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
                copy_text("beAcctVerify.global.unknown", &[("name", x)]),
            );
        }
    }

    let items = share_items(&s.shared);
    let mut emails: Vec<(String, String)> = Vec::new();
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
                copy_text("beAcctVerify.acct.missing", &[("path", c)]),
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
        for (item, _, secret) in identity() {
            match d.get(item) {
                Item::Link { .. } => o.push(
                    CheckLevel::Fail,
                    n,
                    copy_text("beAcctVerify.acct.identityLink", &[("item", item)]),
                ),
                Item::File { mode, .. } if secret && item == acct_core::CREDENTIALS_NAME => {
                    match mode {
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
                    }
                }
                Item::Absent if item == acct_core::CREDENTIALS_NAME => {
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
        } else {
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
            if items.is_empty() {
                o.push(
                    CheckLevel::Fail,
                    n,
                    copy_text("beAcctVerify.acct.noShared", &[("shared", &r.shared)]),
                );
            } else if missing > 0 {
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
            for (base, it) in &d.entries {
                if it.is_link() || is_identity(base) || base.contains(".ccm-tmp-") {
                    continue;
                }
                if items.contains(base) {
                    continue;
                }
                o.push(
                    CheckLevel::Warn,
                    n,
                    copy_text("beAcctVerify.acct.unexpected", &[("item", base)]),
                );
            }
        }
        match s.email_of(c) {
            Some(e) => {
                o.push(
                    CheckLevel::Ok,
                    n,
                    copy_text("beAcctVerify.acct.email", &[("email", e)]),
                );
                emails.push((a.name.clone(), e.to_string()));
            }
            None => o.push(
                CheckLevel::Skip,
                n,
                copy_text("beAcctVerify.acct.noEmail", &[]),
            ),
        }
    }

    if emails.len() < 2 {
        o.push(
            CheckLevel::Skip,
            None,
            copy_text("beAcctVerify.iso.tooFew", &[]),
        );
    } else {
        let mut dup = false;
        for (i, (na, ea)) in emails.iter().enumerate() {
            for (nb, eb) in &emails[i + 1..] {
                if ea == eb {
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

    for (item, _, _) in identity() {
        let found = s.shared.get(item).exists()
            || s.home_items.get(item).is_some_and(Item::exists)
            || m.managed().any(|a| s.dir(&a.config_dir).get(item).exists());
        if !found {
            o.push(
                CheckLevel::Warn,
                None,
                copy_text("beAcctVerify.drift.nowhere", &[("item", item)]),
            );
        }
    }
    for (base, it) in &s.shared.entries {
        if is_identity(base) || is_excluded(base) {
            continue;
        }
        if let Item::File {
            mode: Some(0o600), ..
        } = it
        {
            o.push(
                CheckLevel::Warn,
                None,
                copy_text("beAcctVerify.drift.secretLike", &[("item", base)]),
            );
        }
    }
    finish(o)
}

fn finish(o: Out) -> VerifyReport {
    let fails = o
        .checks
        .iter()
        .filter(|c| c.level == CheckLevel::Fail)
        .count();
    let warns = o
        .checks
        .iter()
        .filter(|c| c.level == CheckLevel::Warn)
        .count();
    VerifyReport {
        pass: fails == 0,
        fails: fails as u32,
        warns: warns as u32,
        checks: o.checks,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/verify_tests.rs"]
mod tests;
