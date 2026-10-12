//! ★ 一判（算法 §2）：纯函数 —— 不读钟、不写盘；同样的 (Σ, F, 设置, 策略, t) 回同样的结果。真用（中转发之前）与推演调的都是 [`decide`]。

use super::settings::Rules;
use super::standing;
use super::Acct;
use std::collections::BTreeMap;

/// 策略这一判交出的：每个号的分（缺 ＝ 按 0 分）与一句为什么 · 次序最早可能在什么时刻变。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Scored {
    pub(crate) scores: BTreeMap<String, (Option<f64>, String)>,
    pub(crate) next_change_at: Option<u64>,
}

/// 策略那一侧（沙箱里的脚本，或判据里的假策略）。
pub(crate) trait Score {
    /// 出错 · 超时 · 形状不对 ⇒ `Err`（这一判所有号当没给分）。
    fn score(&self, ctx: &Ctx<'_>) -> Result<Scored, super::super::strategy::Fail>;
    /// 声明了「分会随时间变」。
    fn time_varying(&self) -> bool {
        false
    }
    /// 这一趟推演给策略的耗时用完了（推演停在那里）。
    fn exhausted(&self) -> bool {
        false
    }
}

/// 什么都不给的策略（所有号同分 ⇒ 完全按列表）。
pub(crate) struct NoScores;

impl Score for NoScores {
    fn score(&self, _: &Ctx<'_>) -> Result<Scored, super::super::strategy::Fail> {
        Ok(Scored::default())
    }
}

/// 交给策略的那一份（算法 §7）。
#[derive(Debug, Clone)]
pub(crate) struct Ctx<'a> {
    pub(crate) now: u64,
    pub(crate) clock: u16,
    pub(crate) current: &'a str,
    pub(crate) temp: &'a [String],
    pub(crate) accounts: Vec<CtxAcct>,
}

/// ctx 里的一个号。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CtxAcct {
    pub(crate) id: String,
    pub(crate) pos: usize,
    pub(crate) api: bool,
    pub(crate) ok: bool,
    pub(crate) h: f64,
    pub(crate) used: [f64; 2],
    pub(crate) resets: [Option<u64>; 2],
    pub(crate) cap: [f64; 2],
    pub(crate) refused_until: Option<u64>,
    pub(crate) rate: [Option<f64>; 2],
}

/// 会话状态 Σ。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Sigma {
    /// 开会话用的号（号列表里 `start` 那一行）。
    pub(crate) start: String,
    pub(crate) current: String,
    pub(crate) temp: Vec<String>,
    /// 启用延迟从什么时候起计（空 ＝ 没在计）。
    pub(crate) since: BTreeMap<String, u64>,
}

/// 候选次序里的一行。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ranked {
    pub(crate) account: String,
    pub(crate) score: Option<f64>,
    pub(crate) why: String,
    pub(crate) from_temp: bool,
    pub(crate) ok: bool,
}

/// 结论。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    /// 用这个号（可能就是在用的那个）。
    Use(String),
    /// 停发，到 `until`；`account` 是到时恢复的那个号（说得出才有）。
    Hold {
        until: Option<u64>,
        account: Option<String>,
    },
}

/// 为什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Cause {
    /// 在用的号还能用，没有该回切的。
    Stay,
    /// R1：在用的号用不了，换到候选次序里第一个能用的。
    Unusable,
    /// R3：前面的号余量回到阈值以上，切回去。
    Back,
    /// 有能用的号，但还在启用延迟里。
    Delay,
    /// R4：一个号都用不了。
    Hold,
}

/// 一判的结果（算法 §2）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decision {
    pub(crate) pick: Pick,
    pub(crate) cause: Cause,
    pub(crate) sigma: Sigma,
    /// 这一判丢掉的临时条目（轮到时用不了 ⇒ 跳过）。
    pub(crate) skipped: Vec<String>,
    pub(crate) order: Vec<Ranked>,
    /// 放开了软上限（所有号都到了上限）。
    pub(crate) lift: bool,
    /// 下一个已知节点（重置 · 分段边界 · 启用延迟到期 · 被拒到期 · 策略给的时刻 / 兜底 15 分钟 · 停发到期）。
    pub(crate) tn: Option<u64>,
    pub(crate) strategy_error: Option<super::super::strategy::Fail>,
}

/// 策略没给 `next_change_at`、又声明了分随时间变 ⇒ 每隔这么久一个兜底节点。
pub(crate) const FALLBACK_STEP: u64 = 15 * 60;

/// 找这个号的事实（不在 ⇒ 当作用不了）。
fn fact<'a>(f: &'a [Acct], a: &str) -> Option<&'a Acct> {
    f.iter().find(|x| x.id == a)
}

/// ★ 一判。`tried` ＝ 这一发已经试过的号（429 同一发重判时排除），平时空。
pub(crate) fn decide(
    sigma: &Sigma,
    facts: &[Acct],
    rules: &Rules,
    p: &dyn Score,
    t: u64,
    tried: &[String],
) -> Decision {
    let start = sigma.start.as_str();
    let list = rules.list(start);
    let usable = |a: &str, lift: bool| {
        !tried.iter().any(|x| x == a)
            && fact(facts, a).is_some_and(|x| standing::ok(rules, start, x, t, lift))
    };
    // 1–2：余量与能不能用；一个都没有 ⇒ 放开软上限。
    let lift = !list.iter().any(|a| usable(a, false));
    let ok = |a: &str| usable(a, lift);
    // 3：排候选次序。
    let ctx_for = |current: &'_ str| -> Vec<CtxAcct> {
        list.iter()
            .enumerate()
            .filter_map(|(pos, a)| {
                let x = fact(facts, a)?;
                Some(CtxAcct {
                    id: a.clone(),
                    pos,
                    api: x.api,
                    ok: ok(a),
                    h: standing::headroom(rules, start, x, t, lift),
                    used: [standing::used_at(x, 0, t), standing::used_at(x, 1, t)],
                    resets: x.resets.map(|r| r.filter(|r| *r > t)),
                    cap: [
                        standing::cap_eff(rules, start, x, 0, t, lift),
                        standing::cap_eff(rules, start, x, 1, t, lift),
                    ],
                    refused_until: x.refused_until.filter(|u| *u > t),
                    rate: if a == current { x.rate } else { [None, None] },
                })
            })
            .collect()
    };
    let ctx = Ctx {
        now: t,
        clock: rules.minute(t),
        current: &sigma.current,
        temp: &sigma.temp,
        accounts: ctx_for(&sigma.current),
    };
    let (scored, strategy_error) = match p.score(&ctx) {
        Ok(s) => (s, None),
        Err(e) => (Scored::default(), Some(e)),
    };
    let mut fin: Vec<(usize, &String)> = list.iter().enumerate().collect();
    let sc = |a: &str| {
        scored
            .scores
            .get(a)
            .and_then(|(s, _)| *s)
            .filter(|s| s.is_finite())
    };
    fin.sort_by(|(pa, a), (pb, b)| {
        sc(b)
            .unwrap_or(0.0)
            .total_cmp(&sc(a).unwrap_or(0.0))
            .then(pa.cmp(pb))
    });
    // 在用的号用不了、它又是临时条目 ⇒ 先拿掉（用过一轮），再排候选次序：同一刻把交回的 Σ′ 再判一次，结论不变。
    let c = sigma.current.as_str();
    let c_ok = ok(c) && list.iter().any(|a| a == c);
    let mut temp: Vec<String> = Vec::new();
    for a in &sigma.temp {
        if list.contains(a) && !temp.contains(a) && (c_ok || a != c) {
            temp.push(a.clone());
        }
    }
    let mut k: Vec<String> = temp.clone();
    k.extend(
        fin.iter()
            .map(|(_, a)| (*a).clone())
            .filter(|a| !temp.contains(a)),
    );
    // 4：启用延迟。排在最前的那个号前面没有号 ⇒ 恒允许。
    let mut since = BTreeMap::new();
    let mut all_bad = true;
    let mut allowed: BTreeMap<&str, bool> = BTreeMap::new();
    for (i, a) in k.iter().enumerate() {
        let d = rules.delay_secs(start, a);
        let al = if i == 0 || d == 0 {
            true
        } else if all_bad {
            let s = sigma.since.get(a).copied().unwrap_or(t);
            since.insert(a.clone(), s);
            t.saturating_sub(s) >= d
        } else {
            false
        };
        allowed.insert(a.as_str(), al);
        all_bad &= !ok(a);
    }
    let order: Vec<Ranked> = k
        .iter()
        .map(|a| Ranked {
            account: a.clone(),
            score: sc(a),
            why: scored
                .scores
                .get(a)
                .map(|(_, w)| w.clone())
                .unwrap_or_default(),
            from_temp: temp.contains(a),
            ok: ok(a),
        })
        .collect();
    let mut next = Sigma {
        start: sigma.start.clone(),
        current: sigma.current.clone(),
        temp: temp.clone(),
        since,
    };
    let mut skipped = Vec::new();
    let hold_all = |k: &[String]| -> Pick {
        let mut best: Option<(u64, &str)> = None;
        for a in k {
            let Some(x) = fact(facts, a) else { continue };
            if let Some(at) = standing::recover_at(rules, start, x, t, true) {
                if best.is_none_or(|(b, _)| at < b) {
                    best = Some((at, a.as_str()));
                }
            }
        }
        Pick::Hold {
            until: best.map(|b| b.0),
            account: best.map(|b| b.1.to_string()),
        }
    };
    let (pick, cause) = if !c_ok {
        // R1。
        match k
            .iter()
            .position(|a| a != c && ok(a) && allowed[a.as_str()])
        {
            Some(i) => {
                let to = k[i].clone();
                for a in &k[..i] {
                    if next.temp.contains(a) && !ok(a) {
                        skipped.push(a.clone());
                    }
                }
                next.temp.retain(|a| !skipped.contains(a));
                (Pick::Use(to), Cause::Unusable)
            }
            None if k.iter().any(|a| a != c && ok(a)) => {
                let until = next
                    .since
                    .iter()
                    .filter(|(a, _)| ok(a))
                    .map(|(a, s)| s + rules.delay_secs(start, a))
                    .min();
                let account = next
                    .since
                    .iter()
                    .filter(|(a, _)| ok(a))
                    .min_by_key(|(a, s)| *s + rules.delay_secs(start, a))
                    .map(|(a, _)| a.clone());
                (Pick::Hold { until, account }, Cause::Delay)
            }
            None => (hold_all(&k), Cause::Hold),
        }
    } else {
        // R3（只在没放开软上限时）。
        let ci = k.iter().position(|a| a == c).unwrap_or(k.len());
        let back = (!lift)
            .then(|| {
                k[..ci].iter().find(|a| {
                    ok(a)
                        && allowed[a.as_str()]
                        && rules.back(start, a).is_some_and(|r| {
                            fact(facts, a)
                                .is_some_and(|x| standing::headroom(rules, start, x, t, false) >= r)
                        })
                })
            })
            .flatten();
        match back {
            Some(a) => (Pick::Use(a.clone()), Cause::Back),
            None => (Pick::Use(c.to_string()), Cause::Stay),
        }
    };
    if let Pick::Use(a) = &pick {
        next.current = a.clone();
    }
    // 下一个已知节点。
    let mut tn: Vec<u64> = Vec::new();
    for a in &list {
        let Some(x) = fact(facts, a) else { continue };
        tn.extend(x.resets.iter().flatten().copied());
        tn.extend(x.refused_until);
        tn.extend(x.barred.and_then(|b| b.until));
        tn.extend(rules.next_edge(start, a, t));
    }
    for (a, s) in &next.since {
        tn.push(s + rules.delay_secs(start, a));
    }
    // 换了号 ⇒ 在用的号变了，策略那句「次序最早在什么时刻变」要照新的在用号再问一次（不然要等到兜底那一格）。
    let mut next_change = scored.next_change_at;
    if let Pick::Use(a) = &pick {
        if a != c && strategy_error.is_none() {
            let again = Ctx {
                now: t,
                clock: ctx.clock,
                current: a,
                temp: &next.temp,
                accounts: ctx_for(a),
            };
            next_change = p.score(&again).ok().and_then(|s| s.next_change_at);
        }
    }
    // 策略给的时刻至少隔一分钟（太近的挪到一分钟后，免得事件炸开）；过去的不算。
    match next_change {
        Some(n) if n > t => tn.push(n.max(t + 60)),
        _ if p.time_varying() => tn.push(t + FALLBACK_STEP),
        _ => {}
    }
    if let Pick::Hold { until: Some(u), .. } = &pick {
        tn.push(*u);
    }
    Decision {
        pick,
        cause,
        sigma: next,
        skipped,
        order,
        lift,
        tn: tn.into_iter().filter(|x| *x > t).min(),
        strategy_error,
    }
}

/// 「现在用 a」（后端的动作，不是规则）：`ok(a)` ⇒ 放到临时条目最前（已在就挪过去）、换过去；不 ok ⇒ 什么都不改，回为什么 · 几点恢复。
pub(crate) fn use_now(
    sigma: &Sigma,
    facts: &[Acct],
    rules: &Rules,
    p: &dyn Score,
    t: u64,
    a: &str,
) -> Result<Decision, Option<u64>> {
    let start = sigma.start.as_str();
    let listed = rules.list(start).iter().any(|x| x == a);
    let x = fact(facts, a).filter(|_| listed).ok_or(None)?;
    let lift = !rules
        .list(start)
        .iter()
        .any(|b| fact(facts, b).is_some_and(|y| standing::ok(rules, start, y, t, false)));
    if !standing::ok(rules, start, x, t, lift) {
        return Err(standing::recover_at(rules, start, x, t, lift));
    }
    let mut s = sigma.clone();
    s.temp.retain(|b| b != a);
    s.temp.insert(0, a.to_string());
    s.current = a.to_string();
    Ok(decide(&s, facts, rules, p, t, &[]))
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/decide_tests.rs"]
mod tests;
