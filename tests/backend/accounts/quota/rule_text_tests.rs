//! 规则说明与摘要：按这一份实际的设置挑片段（抢回开 / 关说法不同；最多等几分钟关掉就不提）。期望按文案键拼。

use super::*;
use crate::accounts::quota::rotation::{CapSlot, Caps, StartSlot};
use copy_core::copy_text;

fn base() -> Rotation {
    Rotation {
        order: vec![
            RotationSlot::Start(StartSlot { start: true }),
            RotationSlot::Named("p".into()),
            RotationSlot::Named("w".into()),
        ],
        enabled: vec!["p".into(), "w".into()],
        when: RotationWhen::Threshold { n: 90 },
        ..Rotation::default()
    }
}

fn has(s: &str, key: &str, args: &[(&str, &str)]) -> bool {
    s.split(&copy_text("kit.text.sep", &[]))
        .any(|p| p == copy_text(key, args))
}

/// ★ 抢回开着时说「前面的号恢复即切回」，关着说「不主动换回」（旧那句写死的在抢回开着时是假话）。
#[test]
fn the_explanation_follows_preempt() {
    let mut r = base();
    let off = explain(&r);
    assert!(has(&off, "beRotation.explain.noPreempt", &[]), "{off}");
    assert!(!has(&off, "beRotation.explain.preempt", &[]));
    r.preempt = true;
    let on = explain(&r);
    assert!(has(&on, "beRotation.explain.preempt", &[]), "{on}");
    assert!(!has(&on, "beRotation.explain.noPreempt", &[]));
}

/// 最多等几分钟：开着写出几分钟，关掉（0）不提；时段停用的号写出时段；有上限才说「都到上限即停 / 仍发」。
#[test]
fn the_explanation_names_wait_off_slots_and_at_limit_only_when_they_apply() {
    let mut r = base();
    let ten = copy_core::format_duration(600_000);
    assert!(has(
        &explain(&r),
        "beRotation.explain.wait",
        &[("dur", &ten)]
    ));
    r.wait = 0;
    assert!(!explain(&r).contains(&copy_text("beRotation.explain.wait", &[("dur", &ten)])));
    let mut cap = Caps::new();
    cap.entry("w".into()).or_default().insert(
        "*".into(),
        CapValue::Slots(vec![CapSlot {
            at: "17:00-02:00".into(),
            n: 0,
        }]),
    );
    r.cap = cap;
    assert!(has(
        &explain(&r),
        "beRotation.explain.off",
        &[("acct", "w"), ("at", "17:00-02:00")]
    ));
    assert!(has(&explain(&r), "beRotation.explain.go", &[]));
    r.when = RotationWhen::Full;
    r.cap = Caps::new();
    assert!(
        !has(&explain(&r), "beRotation.explain.go", &[]),
        "满了才换又没封顶 ⇒ 不说无号可换"
    );
    assert!(has(&explain(&r), "beRotation.explain.onRefused", &[]));
}

/// 摘要：顺序（勾上的、前三个）· 触发 · 抢回 · 停 · 封顶几个号。
#[test]
fn the_summary_lists_order_trigger_and_the_non_default_bits() {
    let mut r = base();
    r.preempt = true;
    r.at_limit = AtLimit::Stop;
    let arrow = copy_text("beRotation.sum.arrow", &[]);
    let head = [
        copy_text("beRotation.sum.start", &[]),
        "p".into(),
        "w".into(),
    ]
    .join(&arrow);
    let want = [
        head,
        copy_text("beRotation.sum.pct", &[("n", "90")]),
        copy_text("beRotation.sum.preempt", &[]),
        copy_text("beRotation.sum.stop", &[]),
    ]
    .join(&copy_text("kit.text.sep", &[]));
    assert_eq!(summary(&r), want);
}
