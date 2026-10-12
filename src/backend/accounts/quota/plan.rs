//! 推演（E12）：从此刻到视窗止，每个事件时刻跑一遍 E4–E9（拼上下文 · 跑策略 · 排 · 叠手动调整 · 挑 · 换），备忘一路带着。
//!
//! - 在用号卡人那一窗按用速（E2）外推，换号后新号按同一用速涨；其余号只按此刻的数。没有用速 ⇒ 不外推（不出估的到线）。
//! - 事件 ＝ 各号重置 · 事实上恢复 · 策略给的 `back` · 时段起止（取每号 `slots` 参数）· 估的到线时刻（取每号 `line` 参数）·
//!   每 15 分一格 · 策略的字里带的时刻（`at`：等延迟几点换过去这一类）；至多 200 判。
//! - 将来的段与换号点都是估的（`est`）。

use super::decide::{self, Judge, Pickable, Ranked, Verdict};
use super::rotation::{ConfigBody, Manual, Params};
use super::strategy::ctx::{Facts, Stint, WhyNot, Win};
use super::strategy::form;
use super::strategy::Strategy;
use serde_json::Value;

/// 至多判几次。
pub(crate) const MAX_STEPS: usize = 200;
/// 一格多长（秒）：「30 分内到点」这类策略在对的时刻翻转。
pub(crate) const GRID: u64 = 15 * 60;

/// 推演要的。
pub(crate) struct Input<'a> {
    pub(crate) strategy: Option<&'a Strategy>,
    pub(crate) body: &'a ConfigBody,
    pub(crate) accounts: Vec<(Facts, Params)>,
    pub(crate) now: u64,
    pub(crate) until: u64,
    pub(crate) current: String,
    pub(crate) stint: Stint,
    pub(crate) rate: Option<f64>,
    pub(crate) memo: Value,
    pub(crate) manual: Manual,
    pub(crate) can_hold: bool,
    pub(crate) ready: &'a dyn Fn(&str) -> bool,
}

/// 推演里的一个时刻：从 `at` 起（到下一个时刻止）在用哪个号（停发 ⇒ `None`）· 那一判的有效优先级 · 那一刻的事实 · 这一刻有没有换号（换到的那一句）。
#[derive(Debug, Clone)]
pub(crate) struct Snap {
    pub(crate) at: u64,
    pub(crate) account: Option<String>,
    pub(crate) rows: Vec<Ranked>,
    pub(crate) facts: Vec<Facts>,
    pub(crate) switched: Option<Value>,
    pub(crate) strategy: String,
    pub(crate) est: bool,
}

/// 一个号在时刻 `t` 的事实：过了重置的窗口关掉；事实上的 `back` 过了 ⇒ 能用；在用号那一窗按用速涨（涨到 100 ⇒ 用满）。
fn facts_at(f: &Facts, t: u64, grow: Option<(f64, &str)>) -> Facts {
    let mut f = f.clone();
    let close = |w: &mut Win| {
        if w.resets.is_some_and(|r| r <= t) {
            *w = Win::CLOSED;
        }
    };
    close(&mut f.w5h);
    close(&mut f.w7d);
    if let Some((add, w)) = grow {
        let win = if w == "7d" { &mut f.w7d } else { &mut f.w5h };
        if win.open {
            win.used += add;
        }
    }
    if !f.ok && f.back.is_some_and(|b| b <= t) {
        f.ok = true;
        f.why_not = None;
        f.back = None;
    }
    if f.ok && (f.w5h.open && f.w5h.used >= 100.0 || f.w7d.open && f.w7d.used >= 100.0) {
        f.ok = false;
        f.why_not = Some(WhyNot::Full);
        f.back = [f.w5h, f.w7d]
            .iter()
            .filter(|w| w.open && w.used >= 100.0)
            .filter_map(|w| w.resets)
            .max();
    }
    f
}

/// 卡人那一窗（用得多的那个；都没开 ⇒ 5h）。
pub(crate) fn pinch(f: &Facts) -> &'static str {
    if f.w7d.open && (!f.w5h.open || f.w7d.used > f.w5h.used) {
        "7d"
    } else {
        "5h"
    }
}

/// 一行参数此刻那一窗的线（点）：时段内取时段的；`()` ＝ 用满（100）。
pub(crate) fn line_now(p: &Params, w: &str, clock: u32) -> f64 {
    let n = |v: &Value| v.as_f64();
    if let Some(s) = p.get(form::SLOTS).and_then(Value::as_array).and_then(|a| {
        a.iter().find(|x| {
            x.get("at")
                .and_then(Value::as_str)
                .and_then(form::span_of)
                .is_some_and(|(f, t)| {
                    let (f, t) = (u32::from(f), u32::from(t));
                    if f < t {
                        clock >= f && clock < t
                    } else {
                        clock >= f || clock < t
                    }
                })
        })
    }) {
        return s.get("line").and_then(n).unwrap_or(100.0);
    }
    match p.get(form::LINE) {
        Some(Value::Object(m)) => m.get(w).and_then(n).unwrap_or(100.0),
        Some(v) => n(v).unwrap_or(100.0),
        None => 100.0,
    }
}

/// 一份配置的时段起止在 `(t, ..)` 里最近的那一刻。
fn next_slot_edge(body: &ConfigBody, accounts: &[(Facts, Params)], t: u64) -> Option<u64> {
    let clock = body.clock_at(t);
    accounts
        .iter()
        .filter_map(|(_, p)| p.get(form::SLOTS).and_then(Value::as_array))
        .flatten()
        .filter_map(|x| x.get("at").and_then(Value::as_str).and_then(form::span_of))
        .flat_map(|(f, e)| [u32::from(f), u32::from(e)])
        .map(|edge| {
            let mins = (edge + 1440 - clock) % 1440;
            let mins = if mins == 0 { 1440 } else { mins };
            t - t % 60 + u64::from(mins) * 60
        })
        .min()
}

/// 一句 `why` 里带的时刻（`at` · `*_at`，递归进 `then`）。
fn times_in(v: &Value, out: &mut Vec<u64>) {
    if let Value::Object(m) = v {
        for (k, x) in m {
            if (k == "at" || k.ends_with("_at")) && x.is_u64() {
                out.extend(x.as_u64());
            } else {
                times_in(x, out);
            }
        }
    }
}

/// ★ 推演：从 `now` 到 `until`，每个事件时刻判一次。
pub(crate) fn simulate(i: &Input<'_>) -> Vec<Snap> {
    let mut out: Vec<Snap> = Vec::new();
    let mut t = i.now;
    let mut memo = i.memo.clone();
    let mut manual = i.manual.clone();
    let mut current = i.current.clone();
    let mut holding = false;
    // 在用号从什么时候起按用速涨：（号, 起点, 起点时那一窗的用量增量 ＝ 0）。
    let mut grow_from: (String, u64) = (current.clone(), i.now);
    let mut since = i.stint.since;
    for _ in 0..MAX_STEPS {
        let grow = |f: &Facts| -> Option<(f64, &'static str)> {
            let r = i.rate.filter(|r| *r > 0.0)?;
            (f.id == grow_from.0 && !holding)
                .then(|| (r * (t.saturating_sub(grow_from.1)) as f64 / 60.0, pinch(f)))
        };
        let accounts: Vec<(Facts, Params)> = i
            .accounts
            .iter()
            .map(|(f, p)| (facts_at(f, t, grow(f)), p.clone()))
            .collect();
        let stint_used = accounts
            .iter()
            .find(|(f, _)| f.id == current)
            .zip(i.rate)
            .map_or(i.stint.used, |_| {
                if current == i.current {
                    i.stint.used + i.rate.unwrap_or(0.0) * (t - i.now) as f64 / 60.0
                } else {
                    i.rate.unwrap_or(0.0) * (t.saturating_sub(since)) as f64 / 60.0
                }
            });
        let j = decide::judge(&Judge {
            strategy: i.strategy,
            body: i.body,
            accounts: &accounts,
            now: t,
            current: &current,
            stint: Stint {
                since,
                used: stint_used,
            },
            rate: i.rate,
            memo: &memo,
        });
        let (eff, clear) = decide::effective(&j.rows, &manual, &accounts);
        if clear {
            manual.hot = None;
        }
        let v = decide::decide(
            &eff,
            &Pickable {
                facts: &accounts,
                current: &current,
                tried: &[],
                can_hold: i.can_hold,
            },
            &j.strategy,
            &mut |x| {
                if (i.ready)(x) {
                    Ok(())
                } else {
                    Err(super::rotation::Unready::NeedsLogin)
                }
            },
        );
        let mut switched = None;
        match &v {
            Verdict::Switch { to, pick, .. } => {
                switched = Some(pick.why.clone());
                current = to.clone();
                holding = false;
                grow_from = (current.clone(), t);
                since = t;
            }
            Verdict::Hold { .. } => holding = true,
            Verdict::Stay { .. } => holding = false,
        }
        memo = j.memo.clone().unwrap_or(Value::Null);
        out.push(Snap {
            at: t,
            account: (!holding).then(|| current.clone()),
            rows: eff.clone(),
            facts: accounts.iter().map(|(f, _)| f.clone()).collect(),
            switched,
            strategy: j.strategy.clone(),
            est: t > i.now,
        });
        // 下一个事件。
        let mut ev: Vec<u64> = Vec::new();
        for (f, p) in &accounts {
            ev.extend(f.w5h.resets);
            ev.extend(f.w7d.resets);
            ev.extend(f.back);
            if let (Some(r), true) = (i.rate.filter(|r| *r > 0.0), f.id == current && !holding) {
                let w = pinch(f);
                let used = if w == "7d" { f.w7d.used } else { f.w5h.used };
                let line = line_now(p, w, i.body.clock_at(t));
                if line > used {
                    ev.push(t + ((line - used) / r * 60.0).ceil() as u64);
                }
                if used < 100.0 {
                    ev.push(t + ((100.0 - used) / r * 60.0).ceil() as u64);
                }
            }
        }
        for r in &eff {
            ev.extend(r.back);
            times_in(&r.why, &mut ev);
        }
        ev.extend(next_slot_edge(i.body, &accounts, t));
        ev.push(t - t % GRID + GRID);
        let Some(next) = ev.into_iter().filter(|x| *x > t).min() else {
            break;
        };
        if next >= i.until {
            break;
        }
        t = next;
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/plan_tests.rs"]
mod tests;
