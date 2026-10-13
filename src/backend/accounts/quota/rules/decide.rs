//! ★ 后端的一判（`轮换-v5-算法.md` §3）：叫策略来判，照它交的先后做动作。后端没有任何换号的想法 ——
//! 上限、软硬、启用延迟、回切都在策略里（随包三份调积木库 `src/shared/rotation/lib.rhai`）。
//! 后端只认事实上发不发得出去（[`standing::sendable`]）、用户拖出来的顺序、停发、出错兜底。
//! 纯函数：不读钟、不写盘；真用（中转发之前）与推演调的都是 [`turn`]。

use super::standing;
use super::Acct;
use serde_json::Value;

/// 某号某窗读数到 n 再来问（按读数叫醒）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WakeWhen {
    pub(crate) acct: String,
    /// 0 ＝ 5 小时 · 1 ＝ 7 天。
    pub(crate) w: usize,
    pub(crate) used_at_least: f64,
}

/// 策略这一判交出的。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Verdict {
    /// 此刻愿意用的号，先后即想用的先后；不在里面 ＝ 此刻不用。
    pub(crate) order: Vec<String>,
    /// 每个号一句为什么。
    pub(crate) why: Vec<(String, String)>,
    /// `order` 空时停到几点（不给 ⇒ 停到最早有号事实上发得出去的时刻）。
    pub(crate) hold_until: Option<u64>,
    /// 几点再来问（按时刻叫醒）。
    pub(crate) wake_at: Option<u64>,
    /// 读数到哪再来问（按读数叫醒）。
    pub(crate) wake_when: Vec<WakeWhen>,
    /// 策略要跨判记的小东西（后端原样存回、下一判原样交回）。
    pub(crate) memo: Option<Value>,
}

/// 策略那一侧（沙箱里的脚本，或判据里的假策略）。
pub(crate) trait Strategy {
    /// 出错 · 超时 · 形状不对 ⇒ `Err`（这一判走兜底）。
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, super::super::strategy::Fail>;
    /// 声明了「结论会随时间变」：没约时刻时后端每 [`FALLBACK_STEP`] 来问一次。
    fn time_varying(&self) -> bool {
        false
    }
    /// 这一趟推演给策略的耗时用完了（推演停在那里）。
    fn exhausted(&self) -> bool {
        false
    }
}

/// 交给策略的那一份（算法 §2）。
#[derive(Debug, Clone)]
pub(crate) struct Ctx<'a> {
    pub(crate) now: u64,
    pub(crate) clock: u16,
    pub(crate) start: &'a str,
    pub(crate) current: &'a str,
    pub(crate) settings: &'a Value,
    pub(crate) memo: Option<&'a Value>,
    pub(crate) accounts: Vec<CtxAcct>,
}

/// ctx 里的一个号（只有事实）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CtxAcct {
    pub(crate) id: String,
    pub(crate) pos: usize,
    pub(crate) api: bool,
    pub(crate) sendable: bool,
    pub(crate) used: [f64; 2],
    pub(crate) resets: [Option<u64>; 2],
    pub(crate) refused_until: Option<u64>,
    pub(crate) rate: [Option<f64>; 2],
}

/// 会话状态 Σ（后端存）。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sigma {
    /// 开会话用的号。
    pub(crate) start: String,
    pub(crate) current: String,
    /// 用户拖出来的「接下来先用谁」。
    pub(crate) temp: Vec<String>,
    /// 上一判策略约的读数（记账那一下比对）。
    pub(crate) wake_when: Vec<WakeWhen>,
    pub(crate) memo: Option<Value>,
}

/// 结论。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    Use(String),
    /// 停发到 `until`；`account` 是到时最早恢复的那个号（说得出才有）。
    Hold {
        until: Option<u64>,
        account: Option<String>,
    },
}

/// 一判的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decision {
    pub(crate) pick: Pick,
    pub(crate) sigma: Sigma,
    /// 这一判丢掉的拖出来的条目（轮到时用不了 ⇒ 跳过）。
    pub(crate) skipped: Vec<String>,
    /// 策略交的先后（显示用）与每号一句。
    pub(crate) order: Vec<String>,
    pub(crate) why: Vec<(String, String)>,
    /// 这一号来自用户拖出来的顺序。
    pub(crate) from_temp: bool,
    /// 下一个已知节点（重置 · 被拒到期 · 挡着的到期 · 策略约的时刻 · 停发到期）。
    pub(crate) tn: Option<u64>,
    /// 策略约的时刻（原样存进会话，推演与下一判从它接着）。
    pub(crate) wake_at: Option<u64>,
    pub(crate) strategy_error: Option<super::super::strategy::Fail>,
}

/// 策略声明了结论随时间变、又没约时刻 ⇒ 每隔这么久来问一次。
pub(crate) const FALLBACK_STEP: u64 = 15 * 60;

fn fact<'a>(f: &'a [Acct], a: &str) -> Option<&'a Acct> {
    f.iter().find(|x| x.id == a)
}

/// 拼交给策略的 ctx（号照事实的先后）。
pub(crate) fn ctx_of<'a>(
    sigma: &'a Sigma,
    facts: &[Acct],
    settings: &'a Value,
    clock: u16,
    t: u64,
) -> Ctx<'a> {
    Ctx {
        now: t,
        clock,
        start: &sigma.start,
        current: &sigma.current,
        settings,
        memo: sigma.memo.as_ref(),
        accounts: facts
            .iter()
            .enumerate()
            .map(|(pos, x)| CtxAcct {
                id: x.id.clone(),
                pos,
                api: x.api,
                sendable: standing::sendable(x, t),
                used: [standing::used_at(x, 0, t), standing::used_at(x, 1, t)],
                resets: x.resets.map(|r| r.filter(|r| *r > t)),
                refused_until: x.refused_until.filter(|u| *u > t),
                rate: x.rate,
            })
            .collect(),
    }
}

/// ★ 一判。`tried` ＝ 这一发已经试过的号（429 同一发重判时排除），平时空。`clock` ＝ 策略时区里 t 是一天的第几分钟。
pub(crate) fn turn(
    sigma: &Sigma,
    facts: &[Acct],
    settings: &Value,
    p: &dyn Strategy,
    clock: u16,
    t: u64,
    tried: &[String],
) -> Decision {
    let ctx = ctx_of(sigma, facts, settings, clock, t);
    let send = |a: &str| {
        !tried.iter().any(|x| x == a) && fact(facts, a).is_some_and(|x| standing::sendable(x, t))
    };
    // 1：叫策略；出错 ⇒ 兜底：此刻的号发得出去就照用，否则照事实的先后取第一个发得出去的。
    let (v, strategy_error) = match p.decide(&ctx) {
        Ok(v) => (v, None),
        Err(e) => {
            let mut order: Vec<String> = vec![sigma.current.clone()];
            order.extend(
                facts
                    .iter()
                    .map(|x| x.id.clone())
                    .filter(|a| *a != sigma.current),
            );
            (
                Verdict {
                    order,
                    memo: sigma.memo.clone(),
                    ..Verdict::default()
                },
                Some(e),
            )
        }
    };
    // 2：候选 ＝ order 里事实上发得出去的。
    let cands: Vec<&String> = v.order.iter().filter(|a| send(a)).collect();
    let c = sigma.current.as_str();
    let mut next = Sigma {
        start: sigma.start.clone(),
        current: sigma.current.clone(),
        temp: sigma.temp.clone(),
        wake_when: v.wake_when.clone(),
        memo: v.memo.clone(),
    };
    let mut skipped = Vec::new();
    let mut from_temp = false;
    // 3：该换了（当前号不在候选里）⇒ 先照用户拖出来的顺序：第一个在候选里的被选中，排在它前面的丢掉。
    let mut chosen: Option<String> = None;
    if !cands.iter().any(|a| *a == c) {
        next.temp.retain(|a| a != c);
        if let Some(i) = next.temp.iter().position(|a| cands.iter().any(|x| *x == a)) {
            skipped = next.temp[..i].to_vec();
            next.temp.drain(..i);
            chosen = Some(next.temp[0].clone());
            from_temp = true;
        }
    } else if next.temp.first().map(String::as_str) == Some(c) {
        from_temp = true;
    }
    // 4：否则取候选第一个（当前号排第一 ⇒ 不换）。
    let pick = match chosen.or_else(|| cands.first().map(|a| (*a).clone())) {
        Some(a) => Pick::Use(a),
        None => {
            // 5：候选空 ⇒ 停发。
            let mut best: Option<(u64, &str)> = None;
            for x in facts {
                if let Some(at) = standing::sendable_again(x, t) {
                    if best.is_none_or(|(b, _)| at < b) {
                        best = Some((at, &x.id));
                    }
                }
            }
            Pick::Hold {
                until: v.hold_until.or(best.map(|b| b.0)),
                account: best.map(|b| b.1.to_string()),
            }
        }
    };
    let mut wake_at = v.wake_at;
    if let Pick::Use(a) = &pick {
        next.current = a.clone();
        // 换了号 ⇒ 在用的号变了，这是一件事实：照新的在用号再问策略一次它约的叫醒（不然要等到别的节点才知道新在用号该几点再判）。
        if a != c && strategy_error.is_none() {
            if let Ok(v2) = p.decide(&ctx_of(&next, facts, settings, clock, t)) {
                next.wake_when = v2.wake_when;
                next.memo = v2.memo;
                wake_at = v2.wake_at;
            }
        }
    }
    // 6：下一个已知节点。
    let mut tn: Vec<u64> = Vec::new();
    for x in facts {
        tn.extend(x.resets.iter().flatten().copied());
        tn.extend(x.refused_until);
        tn.extend(x.barred.and_then(|b| b.until));
    }
    match wake_at {
        Some(n) if n > t => tn.push(n.max(t + 60)),
        _ if p.time_varying() => tn.push(t + FALLBACK_STEP),
        _ => {}
    }
    if let Pick::Hold { until: Some(u), .. } = &pick {
        tn.push(*u);
    }
    Decision {
        pick,
        sigma: next,
        skipped,
        order: v.order,
        why: v.why,
        from_temp,
        tn: tn.into_iter().filter(|x| *x > t).min(),
        wake_at,
        strategy_error,
    }
}

/// 「现在用 a」（用户的手动操作）：事实上发得出去 ⇒ 放到拖出来的顺序最前、换过去；发不出去 ⇒ 不改，回几点恢复。
/// 策略的上限不拦它。
pub(crate) fn use_now(
    sigma: &Sigma,
    facts: &[Acct],
    t: u64,
    a: &str,
) -> Result<Sigma, Option<u64>> {
    let x = fact(facts, a).ok_or(None)?;
    if !standing::sendable(x, t) {
        return Err(standing::sendable_again(x, t));
    }
    let mut s = sigma.clone();
    s.temp.retain(|b| b != a);
    s.temp.insert(0, a.to_string());
    s.current = a.to_string();
    Ok(s)
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/decide_tests.rs"]
mod tests;
