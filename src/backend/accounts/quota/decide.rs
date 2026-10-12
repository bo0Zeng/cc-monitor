//! ★ 换号的**唯一判定**：引擎只管事实与机械动作，偏好一样都不在这里（全在加载的策略里）。
//!
//! 一判（唯一一处，[`judge`] ＋ [`decide`]）：
//! 1. 事实（E3，[`standing`]）：每个号被拒 · 用满 · 超额在兜 · 要重新登录 —— 线、时段不在这里（在策略与 `lib` 里）。
//! 2. 拼上下文、跑策略（E4 · E5）：出错 / 超时 / 形状不对 ⇒ 这一判照内置策略 1 排（参数取它的缺省，每行的 `line` · `slots` 取这份配置写的），
//!    带上 `strategyError`。
//! 3. 排（E6，[`rank`]）：`p` 从大到小 · 同分按号 id · 策略没给的排在 `()` 之前 · `()` 最后、按 `back` 早的先。
//! 4. 叠手动调整（E7，[`overlay`]）：手动热换的号放最前；拖过的号放回它被放的位置（越界取末尾）。⇒ **有效优先级**。
//! 5. 挑（E8，[`decide`]）：从头往下取第一个「事实上能用 · 接得上 · `p` 不是 `()`」的号。
//! 6. 换（E9）：目标不是在用的号 ⇒ 换；没有目标 ⇒ 停发（回来的时刻 ＝ 各号 `back` 里最早的）；在用的号在超额上兜着 ⇒ 留着。
//!    手动热换的号事实上不能用了或策略给它 `()` ⇒ 清掉热换、再判一次。
//!
//! 「什么时候换」也不是引擎的偏好：目标不是在用的号就换。要「还能用就不换」「等一会儿再换」「都到上限后接着用」都在策略里。

use super::reset_since_seen;
use super::rotation::{ConfigBody, Leave, Manual, Params, Pick, Unready};
use super::strategy::ctx::{self, Facts, Input, Stint, WhyNot, Win};
use super::strategy::form;
use super::strategy::run::{self, Given};
use super::strategy::{Fail, FailKind, Strategy};
use crate::agents::{QuotaReading, QuotaWindow};
use serde_json::Value;
use std::collections::BTreeMap;

/// 号的种类：线上 `sub` · `api`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaKind"
    )
)]
pub enum Kind {
    Sub,
    Api,
}

fn live(t: Option<u64>, now: u64) -> bool {
    !reset_since_seen(t, now)
}

/// 「额度账上这个号此刻被拒着」的唯一判法：上次那一发被拒、它说的回来时刻还没到（说不出时刻的旧账 ⇒ 不算）。显示态同问这一处。
pub(crate) fn refused_at(r: &QuotaReading, now: u64) -> bool {
    r.refused && r.resets_at.is_some_and(|t| t > now)
}

/// 订阅号正在用付费超额、卡着的窗口还没重置。
pub(crate) fn overage_in_use(r: &QuotaReading, kind: Kind, now: u64) -> bool {
    kind == Kind::Sub && r.overage.as_ref().is_some_and(|o| o.in_use) && live(r.resets_at, now)
}

/// 一个窗口此刻的用量（已重置、未计时 ⇒ 0）。
pub(crate) fn used_now(w: &QuotaWindow, now: u64) -> f64 {
    if reset_since_seen(w.resets_at, now) {
        0.0
    } else {
        w.used.unwrap_or(0.0)
    }
}

/// 一个语义位（`5h` / `7d`）此刻的窗口：在计时吗 · 用了几点 · 几点重置。那一位有几个窗口（`7d` 分档）⇒ 取用得最多的那个。
pub(crate) fn window_fact(
    r: Option<&QuotaReading>,
    w: &str,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Win {
    let Some(r) = r else { return Win::CLOSED };
    r.windows
        .iter()
        .filter(|x| slot(&x.name) == Some(w))
        .filter(|x| x.used.is_some() && x.resets_at.is_some_and(|t| t > now))
        .map(|x| Win {
            open: true,
            used: used_now(x, now) * 100.0,
            resets: x.resets_at,
        })
        .max_by(|a, b| a.used.total_cmp(&b.used))
        .unwrap_or(Win::CLOSED)
}

/// ★ E3 一个号此刻**事实上**能不能用（唯一一处）：被拒（`heard` ⇒ 刚回来的回包被拒，不看时刻）· 超额在兜 · 用满（某个窗口用到 100%、
/// 还没重置）· 要重新登录（`login_ok` 由调用方问）。回 `(为什么不能用, 几点回来)`；能用 ⇒ `None`。
pub(crate) fn standing(
    r: Option<&QuotaReading>,
    heard: bool,
    kind: Kind,
    login_ok: bool,
    now: u64,
) -> Option<(WhyNot, Option<u64>)> {
    if let Some(r) = r {
        if (heard && r.refused) || refused_at(r, now) {
            return Some((WhyNot::Refused, r.resets_at));
        }
        if overage_in_use(r, kind, now) {
            return Some((WhyNot::Overage, r.resets_at));
        }
        let full: Vec<Option<u64>> = r
            .windows
            .iter()
            .filter(|w| w.used.is_some_and(|u| u >= 1.0) && live(w.resets_at, now))
            .map(|w| w.resets_at)
            .collect();
        if !full.is_empty() {
            // 几个窗口都满着 ⇒ 都重置的那一刻；有一个说不出 ⇒ 说不出。
            let back = full
                .iter()
                .copied()
                .collect::<Option<Vec<u64>>>()
                .and_then(|v| v.into_iter().max());
            return Some((WhyNot::Full, back));
        }
    }
    (!login_ok).then_some((WhyNot::Login, None))
}

/// 一个号的事实（拼进 `ctx` 的那一份）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn facts_of(
    id: &str,
    r: Option<&QuotaReading>,
    heard: bool,
    kind: Kind,
    login_ok: bool,
    others: u32,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Facts {
    let st = standing(r, heard, kind, login_ok, now);
    Facts {
        id: id.to_string(),
        kind,
        ok: st.is_none(),
        why_not: st.map(|s| s.0),
        back: st.and_then(|s| s.1),
        w5h: window_fact(r, "5h", now, slot),
        w7d: window_fact(r, "7d", now, slot),
        others,
    }
}

/// 有效优先级里的一行。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ranked {
    pub(crate) account: String,
    /// `None` ＝ `()`：此刻不挑。
    pub(crate) p: Option<f64>,
    pub(crate) why: Value,
    pub(crate) back: Option<u64>,
    /// 策略没给这个号（原因「策略没给」）。
    pub(crate) missing: bool,
}

/// ★ E6 排：`p` 降序 · 同分按号 id · 没给的在 `()` 之前 · `()` 最后、按 `back` 升序（说不出的最后）、再按号 id。
pub(crate) fn rank(rows: &BTreeMap<String, Given>, accounts: &[String]) -> Vec<Ranked> {
    let mut v: Vec<Ranked> = accounts
        .iter()
        .map(|a| match rows.get(a) {
            Some(r) => Ranked {
                account: a.clone(),
                p: r.p,
                why: r.why.clone(),
                back: r.back,
                missing: false,
            },
            None => Ranked {
                account: a.clone(),
                p: None,
                why: Value::Null,
                back: None,
                missing: true,
            },
        })
        .collect();
    let tier = |r: &Ranked| match (r.p, r.missing) {
        (Some(_), _) => 0,
        (None, true) => 1,
        (None, false) => 2,
    };
    v.sort_by(|x, y| {
        tier(x)
            .cmp(&tier(y))
            .then_with(|| match (x.p, y.p) {
                (Some(a), Some(b)) => b.total_cmp(&a),
                _ => std::cmp::Ordering::Equal,
            })
            .then_with(|| match (x.back, y.back) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| x.account.cmp(&y.account))
    });
    v
}

/// ★ E7 叠手动调整：`hot` 放最前；`place[{account, at}]` 拖过的号钉在第 `at` 位（数的是除 `hot` 之外的那张列表；越界取末尾），
/// 其余的号照策略排的先后补空位。几个号钉在同一位 / 越界 ⇒ 按 `at` 升序往后挪到最近的空位。不在列表里的号不动。
pub(crate) fn overlay(mut rows: Vec<Ranked>, manual: &Manual) -> Vec<Ranked> {
    let hot = manual
        .hot
        .as_ref()
        .and_then(|h| rows.iter().position(|r| r.account == h.account))
        .map(|i| rows.remove(i));
    let mut place = manual.place.clone();
    place.sort_by_key(|p| p.at);
    let mut pins = Vec::new();
    for p in &place {
        if let Some(i) = rows.iter().position(|r| r.account == p.account) {
            pins.push((p.at, rows.remove(i)));
        }
    }
    let n = rows.len() + pins.len();
    let m = pins.len();
    let mut slots: Vec<Option<Ranked>> = (0..n).map(|_| None).collect();
    let mut next = 0;
    for (j, (at, r)) in pins.into_iter().enumerate() {
        let k = at.max(next).min(n - (m - j));
        slots[k] = Some(r);
        next = k + 1;
    }
    let mut rest = rows.into_iter();
    let mut out: Vec<Ranked> = slots
        .into_iter()
        .filter_map(|s| s.or_else(|| rest.next()))
        .collect();
    if let Some(h) = hot {
        out.insert(0, h);
    }
    out
}

/// 拼一判要的：选的那份策略（读不懂也交进来，好兜底）· 打开的配置内容 · 参与的号（按写下的先后，事实 ＋ 这一行的参数）·
/// 此刻 · 在用的号 · 这一段 · 用速 · 备忘。
pub(crate) struct Judge<'a> {
    pub(crate) strategy: Option<&'a Strategy>,
    pub(crate) body: &'a ConfigBody,
    pub(crate) accounts: &'a [(Facts, Params)],
    pub(crate) now: u64,
    pub(crate) current: &'a str,
    pub(crate) stint: Stint,
    pub(crate) rate: Option<f64>,
    pub(crate) memo: &'a Value,
}

/// 一判跑完：排好的（还没叠手动调整）· 这一判的备忘 · 实际是哪份策略排的 · 出错的话为什么。
#[derive(Debug, Clone)]
pub(crate) struct Judged {
    pub(crate) rows: Vec<Ranked>,
    pub(crate) memo: Option<Value>,
    pub(crate) strategy: String,
    pub(crate) error: Option<Fail>,
    pub(crate) ops: u64,
}

fn run_one(
    s: &Strategy,
    j: &Judge<'_>,
    accounts: &[(Facts, Params)],
    params: &Params,
    memo: &Value,
) -> Result<run::Priorities, Fail> {
    let d = ctx::build(
        s,
        &Input {
            now: j.now,
            clock: j.body.clock_at(j.now),
            current: j.current,
            stint: j.stint,
            rate: j.rate,
            memo,
            params,
            accounts,
        },
    )
    .map_err(Fail::shape)?;
    let ids: Vec<String> = accounts.iter().map(|(f, _)| f.id.clone()).collect();
    run::priority(s, d, &ids)
}

/// ★ E4 · E5 · E6：拼上下文 → 跑策略（出错照内置策略 1 兜底）→ 排。
pub(crate) fn judge(j: &Judge<'_>) -> Judged {
    let ids: Vec<String> = j.accounts.iter().map(|(f, _)| f.id.clone()).collect();
    let picked = j.strategy.filter(|s| s.usable());
    let first = match picked {
        Some(s) => {
            let params = ctx::whole_params(&s.params, &j.body.params);
            let accounts: Vec<(Facts, Params)> = j
                .accounts
                .iter()
                .map(|(f, p)| {
                    (
                        f.clone(),
                        ctx::acct_params(&s.acct_params, p, Some(&j.body.acct_init)),
                    )
                })
                .collect();
            run_one(s, j, &accounts, &params, j.memo).map(|o| (s.id.clone(), o))
        }
        None => Err(j.strategy.and_then(|s| s.error.clone()).unwrap_or(Fail {
            kind: FailKind::Gone,
            line: None,
            col: None,
            msg: format!("strategy `{}` is not loaded", j.body.strategy),
        })),
    };
    let (strategy, out, error) = match first {
        Ok((id, o)) => (id, o, None),
        Err(e) => match fallback(j) {
            Some(o) => (super::strategy::FALLBACK_ID.to_string(), o, Some(e)),
            None => (
                super::strategy::FALLBACK_ID.to_string(),
                run::Priorities {
                    rows: BTreeMap::new(),
                    memo: None,
                    ops: 0,
                },
                Some(e),
            ),
        },
    };
    Judged {
        rows: rank(&out.rows, &ids),
        memo: out.memo,
        strategy,
        error,
        ops: out.ops,
    }
}

/// 兜底那一判：内置策略 1 ＋ 它的 `init` 参数 ＋ 这份配置每行写的 `line` · `slots` ＋ 写下的先后（备忘不带：那是另一份策略的）。
fn fallback(j: &Judge<'_>) -> Option<run::Priorities> {
    let one = super::strategy::fallback()?;
    let params = one.param_init();
    let accounts: Vec<(Facts, Params)> = j
        .accounts
        .iter()
        .map(|(f, p)| {
            let keep: Params = [form::LINE, form::SLOTS]
                .iter()
                .filter_map(|k| p.get(*k).map(|v| (k.to_string(), v.clone())))
                .collect();
            (f.clone(), ctx::acct_params(&one.acct_params, &keep, None))
        })
        .collect();
    run_one(&one, j, &accounts, &params, &Value::Null).ok()
}

/// 判的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// 不用换（目标就是在用的号；或在超额上兜着没别的号；或这一家给不出「用满」回包）。
    Stay {
        /// 在用的号在超额上兜着、没号可挑 ⇒ 记一条「留在超额」。
        overage: bool,
    },
    /// 换到 `to`。
    Switch {
        to: String,
        leave: Leave,
        pick: Pick,
    },
    /// 没号可挑 ⇒ 停发：回那一家的「用满」回包，重置时刻 ＝ `back`（最早回来的那个号 ＋ 时刻；说不出 ⇒ `None`）；
    /// `w` ＝ 卡着那个号的那一窗（策略那一句里的 `w`，回包照它写卡在哪；说不出 ⇒ `None`）。
    Hold {
        back: Option<(String, u64)>,
        w: Option<String>,
    },
}

/// 挑号要的事实：每个号事实上能不能用（`ok`）与几点回来。
pub(crate) struct Pickable<'a> {
    pub(crate) facts: &'a [(Facts, Params)],
    pub(crate) current: &'a str,
    /// 这一发已经试过的号（不再挑；在用的号不算）。
    pub(crate) tried: &'a [String],
    /// 这一家给得出「用满」回包（停发要它）。
    pub(crate) can_hold: bool,
}

/// ★ E8 · E9：从有效优先级头往下挑第一个「事实上能用 · 接得上 · `p` 不是 `()`」的号；目标不是在用的号 ⇒ 换；没有 ⇒ 停发。
/// `strategy` ＝ 排它的那份（换号记录的 `pick` 记它）。热换的处理在调用方（[`effective`]）。
pub(crate) fn decide(
    eff: &[Ranked],
    k: &Pickable<'_>,
    strategy: &str,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> Verdict {
    let fact = |a: &str| k.facts.iter().find(|(f, _)| f.id == a).map(|(f, _)| f);
    let ok = |a: &str| fact(a).is_some_and(|f| f.ok);
    let mut target: Option<&Ranked> = None;
    for r in eff {
        if r.p.is_none() || !ok(&r.account) {
            continue;
        }
        if r.account == k.current {
            target = Some(r);
            break;
        }
        if k.tried.contains(&r.account) {
            continue;
        }
        if ready(&r.account).is_ok() {
            target = Some(r);
            break;
        }
    }
    let cur_row = eff.iter().find(|r| r.account == k.current);
    match target {
        Some(t) if t.account == k.current => Verdict::Stay { overage: false },
        Some(t) => {
            let cur_ok = ok(k.current) && cur_row.is_some_and(|r| r.p.is_some());
            let leave = if cur_ok {
                Leave::Outranked
            } else {
                Leave::Unusable {
                    why: cur_row
                        .map(|r| r.why.clone())
                        .filter(|w| !w.is_null())
                        .or_else(|| {
                            fact(k.current)
                                .and_then(|f| f.why_not)
                                .map(|w| serde_json::json!({"k": format!("@{}", w.word()), "at": fact(k.current).and_then(|f| f.back)}))
                        })
                        .unwrap_or(Value::Null),
                }
            };
            Verdict::Switch {
                to: t.account.clone(),
                leave,
                pick: Pick {
                    strategy: strategy.to_string(),
                    why: t.why.clone(),
                },
            }
        }
        None => {
            if fact(k.current).is_some_and(|f| f.why_not == Some(WhyNot::Overage)) {
                return Verdict::Stay { overage: true };
            }
            if !k.can_hold {
                return Verdict::Stay { overage: false };
            }
            let first = eff
                .iter()
                .filter_map(|r| {
                    let at = r.back.or_else(|| fact(&r.account).and_then(|f| f.back))?;
                    (at > 0).then_some((at, r))
                })
                .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.account.cmp(&b.1.account)));
            Verdict::Hold {
                back: first.map(|(at, r)| (r.account.clone(), at)),
                w: first.and_then(|(_, r)| {
                    r.why
                        .get("w")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            // 策略那一句没说哪一窗 ⇒ 照事实：用得多的那一窗。
                            let f = fact(&r.account)?;
                            if f.w7d.open && (!f.w5h.open || f.w7d.used > f.w5h.used) {
                                Some("7d".to_string())
                            } else {
                                f.w5h.open.then(|| "5h".to_string())
                            }
                        })
                }),
            }
        }
    }
}

/// 有效优先级 ＋ 热换要不要清：热换的号事实上不能用了或策略给它 `()` ⇒ 不叠热换（调用方清掉）。
pub(crate) fn effective(
    rows: &[Ranked],
    manual: &Manual,
    facts: &[(Facts, Params)],
) -> (Vec<Ranked>, bool) {
    let stale = manual.hot.as_ref().is_some_and(|h| {
        let ok = facts.iter().any(|(f, _)| f.id == h.account && f.ok);
        let given = rows.iter().any(|r| r.account == h.account && r.p.is_some());
        !(ok && given)
    });
    if stale {
        let m = Manual {
            hot: None,
            place: manual.place.clone(),
        };
        (overlay(rows.to_vec(), &m), true)
    } else {
        (overlay(rows.to_vec(), manual), false)
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/decide_tests.rs"]
mod tests;
