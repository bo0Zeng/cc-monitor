//! 由事实 ＋ 设置派生的几样（算法 §1）：`capEff` · `H` · `ok` · 几点恢复。只有这一处判，判 · 推演 · 界面都问它。

use super::settings::{CapKind, Rules};
use super::Acct;

/// 这一刻某窗的用量（重置时刻过了 ⇒ 0）。
pub(crate) fn used_at(a: &Acct, w: usize, t: u64) -> f64 {
    match a.resets[w] {
        Some(r) if r <= t => 0.0,
        _ => a.used[w],
    }
}

/// `capEff(a, w, t, lift)`：放开软上限、且这个号是软上限 ⇒ 100；否则 `C(a, w, t)`。
pub(crate) fn cap_eff(r: &Rules, start: &str, a: &Acct, w: usize, t: u64, lift: bool) -> f64 {
    if lift && r.kind(start, &a.id) == CapKind::Soft {
        100.0
    } else {
        r.cap(start, &a.id, w, t)
    }
}

/// 余量 `H`：两窗 `capEff − used` 取小。
pub(crate) fn headroom(r: &Rules, start: &str, a: &Acct, t: u64, lift: bool) -> f64 {
    (0..2)
        .map(|w| cap_eff(r, start, a, w, t, lift) - used_at(a, w, t))
        .fold(f64::INFINITY, f64::min)
}

/// 跟设置无关的那一半：登录 · key · 超额 · 被拒。
pub(crate) fn barred_at(a: &Acct, t: u64) -> bool {
    a.barred.is_some_and(|b| b.until.is_none_or(|u| u > t))
        || a.refused_until.is_some_and(|u| u > t)
}

/// ★ `ok(a, t, lift)`：没被挡 ·（订阅号）两窗都没用到 100 · `H > 0`。按量号没有窗口：只看第一项。
pub(crate) fn ok(r: &Rules, start: &str, a: &Acct, t: u64, lift: bool) -> bool {
    if barred_at(a, t) {
        return false;
    }
    if a.api {
        return true;
    }
    (0..2).all(|w| used_at(a, w, t) < 100.0) && headroom(r, start, a, t, lift) > 0.0
}

/// 几点恢复：不再用它时 `ok(a, ·, lift)` 变真的最早时刻（被拒到期 · 窗口重置 · 跨进上限更高的分段）。8 天内说不出 ⇒ 空。
/// 停发时问的是放开软上限之后（`lift`）；「现在用」问的是此刻的那一种。
pub(crate) fn recover_at(r: &Rules, start: &str, a: &Acct, t: u64, lift: bool) -> Option<u64> {
    if ok(r, start, a, t, lift) {
        return Some(t);
    }
    let horizon = t + 8 * 86_400;
    let mut at = t;
    loop {
        let mut cands: Vec<u64> = Vec::new();
        cands.extend(a.refused_until.filter(|u| *u > at));
        cands.extend(a.barred.and_then(|b| b.until).filter(|u| *u > at));
        cands.extend(a.resets.iter().flatten().copied().filter(|x| *x > at));
        cands.extend(r.next_edge(start, &a.id, at));
        let next = cands.into_iter().min()?;
        if next > horizon {
            return None;
        }
        if ok(r, start, a, next, lift) {
            return Some(next);
        }
        at = next;
    }
}
