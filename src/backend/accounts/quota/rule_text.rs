//! 一份轮换写成给人看的两句（界面照抄，不自己拼）：
//! - [`explain`]：规则说明（面板 ⓘ · 编辑器头下那一行），按这一份**实际**的设置挑片段、` · ` 连 —— 抢回开着就说「前面的号恢复即切回」，
//!   关着说「不主动换回」，不再一句写死；标了兜底说「b 兜底 · 其余号恢复即切回」（不管换法）。
//! - [`summary`]：规则列表那一行摘要 `personal → work → team · ≥90% · 抢回 · 兜底 b · 停 · 封顶 2`。
//!
//! 片段都在文案表（`beRotation.explain.*` · `beRotation.sum.*`），这里只挑、只连。

use super::rotation::{AtLimit, CapValue, Rotation, RotationSlot, RotationWhen, ALL_WINDOWS};
use copy_core::copy_text;

fn sep() -> String {
    copy_text("kit.text.sep", &[])
}

/// 这份轮换有没有上限（触发是到 N%，或给号设了封顶）：「无号可换」那一格只在有上限时作数。
pub(crate) fn at_limit_applies(r: &Rotation) -> bool {
    matches!(r.when, RotationWhen::Threshold { .. }) || !r.cap.is_empty()
}

/// 单段预算对「所有号所有窗口」写的那一个数（`stint: {"*": {"*": n}}`）。
pub(crate) fn stint_all(r: &Rotation) -> Option<u8> {
    r.stint
        .get(ALL_WINDOWS)
        .and_then(|m| m.get(ALL_WINDOWS))
        .copied()
}

/// 规则说明。
pub(crate) fn explain(r: &Rotation) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(match r.order.first() {
        Some(RotationSlot::Named(a)) => copy_text("beRotation.explain.firstNamed", &[("acct", a)]),
        _ => copy_text("beRotation.explain.firstStart", &[]),
    });
    parts.push(match r.when {
        RotationWhen::Threshold { n } => {
            copy_text("beRotation.explain.atPct", &[("n", &n.to_string())])
        }
        RotationWhen::Full => copy_text("beRotation.explain.onRefused", &[]),
    });
    parts.push(if r.preempt {
        copy_text("beRotation.explain.preempt", &[])
    } else {
        copy_text("beRotation.explain.noPreempt", &[])
    });
    if let Some(n) = stint_all(r) {
        parts.push(copy_text(
            "beRotation.explain.stint",
            &[("n", &n.to_string())],
        ));
    }
    if !r.fallback.is_empty() {
        let list = r.fallback.join(", ");
        parts.push(copy_text("beRotation.explain.fallback", &[("list", &list)]));
        parts.push(copy_text("beRotation.explain.leave", &[]));
        if r.wait > 0 {
            let dur = copy_core::format_duration(u64::from(r.wait) * 60_000);
            parts.push(copy_text("beRotation.explain.wait", &[("dur", &dur)]));
        }
    }
    for (acct, per) in &r.cap {
        for v in per.values() {
            if let CapValue::Slots(slots) = v {
                for s in slots.iter().filter(|s| s.n == 0) {
                    parts.push(copy_text(
                        "beRotation.explain.off",
                        &[("acct", acct), ("at", &s.at)],
                    ));
                }
            }
        }
    }
    if at_limit_applies(r) {
        parts.push(match r.at_limit {
            AtLimit::Stop => copy_text("beRotation.explain.stop", &[]),
            AtLimit::Continue => copy_text("beRotation.explain.go", &[]),
        });
    }
    parts.join(&sep())
}

/// 摘要一行：顺序前三个（勾上的，→ 连，多的写 ＋N）· 触发 · 换法（不是按顺序时）· 停 · 封顶几个号。
pub(crate) fn summary(r: &Rotation) -> String {
    let names: Vec<String> = r
        .order
        .iter()
        .filter_map(|s| match s {
            RotationSlot::Start(_) => Some(copy_text("beRotation.sum.start", &[])),
            RotationSlot::Named(a) if r.enabled.iter().any(|e| e == a) => Some(a.clone()),
            RotationSlot::Named(_) => None,
        })
        .collect();
    let mut head = names
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(&copy_text("beRotation.sum.arrow", &[]));
    if names.len() > 3 {
        head.push_str(&copy_text(
            "beRotation.sum.more",
            &[("n", &(names.len() - 3).to_string())],
        ));
    }
    let mut parts = vec![head];
    parts.push(match r.when {
        RotationWhen::Threshold { n } => copy_text("beRotation.sum.pct", &[("n", &n.to_string())]),
        RotationWhen::Full => copy_text("beRotation.sum.full", &[]),
    });
    if r.preempt {
        parts.push(copy_text("beRotation.sum.preempt", &[]));
    }
    if let Some(n) = stint_all(r) {
        parts.push(copy_text("beRotation.sum.stint", &[("n", &n.to_string())]));
    }
    if !r.fallback.is_empty() {
        let list = r.fallback.join(", ");
        parts.push(copy_text("beRotation.sum.fallback", &[("list", &list)]));
    }
    if at_limit_applies(r) && r.at_limit == AtLimit::Stop {
        parts.push(copy_text("beRotation.sum.stop", &[]));
    }
    if !r.cap.is_empty() {
        parts.push(copy_text(
            "beRotation.sum.cap",
            &[("n", &r.cap.len().to_string())],
        ));
    }
    parts.join(&sep())
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rule_text_tests.rs"]
mod tests;
