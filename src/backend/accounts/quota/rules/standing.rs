//! 事实上发不发得出去（后端唯一认的「能不能用」）：没被拒 · 登录 · key · 超额不挡 ·（订阅号）两窗都没用到 100。
//! 上限、软硬这些「想不想用」都在策略里（积木库 `src/shared/rotation/lib.rhai`）。

use super::Acct;

/// 这一刻某窗的用量（重置时刻过了 ⇒ 0）。
pub(crate) fn used_at(a: &Acct, w: usize, t: u64) -> f64 {
    match a.resets[w] {
        Some(r) if r <= t => 0.0,
        _ => a.used[w],
    }
}

/// 跟窗口无关的那一半：登录 · key · 超额 · 被拒。
pub(crate) fn barred_at(a: &Acct, t: u64) -> bool {
    a.barred.is_some_and(|b| b.until.is_none_or(|u| u > t))
        || a.refused_until.is_some_and(|u| u > t)
}

/// ★ 用满的唯一判法：一窗此刻的用量（0–100，已重置的算 0）到了 100。显示那一侧（`show.rs`）也问这一处。
pub fn used_up(pct: f64) -> bool {
    pct >= 100.0
}

/// ★ 事实上发得出去（唯一一处）：事实的 `blocked` · `quota-read` 的「可发」· 判 · 推演都照它。
pub fn sendable(a: &Acct, t: u64) -> bool {
    !barred_at(a, t) && (a.api || (0..2).all(|w| !used_up(used_at(a, w, t))))
}

/// 不再用它时几点又发得出去（被拒到期 · 挡着的到期 · 用满的那一窗重置）；说不出 ⇒ 空。
pub(crate) fn sendable_again(a: &Acct, t: u64) -> Option<u64> {
    if sendable(a, t) {
        return Some(t);
    }
    let mut cands: Vec<u64> = Vec::new();
    cands.extend(a.refused_until.filter(|u| *u > t));
    cands.extend(a.barred.and_then(|b| b.until).filter(|u| *u > t));
    cands.extend(a.resets.iter().flatten().copied().filter(|x| *x > t));
    cands.sort_unstable();
    cands.into_iter().find(|at| sendable(a, *at))
}
