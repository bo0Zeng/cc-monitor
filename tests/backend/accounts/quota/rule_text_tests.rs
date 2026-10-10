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

/// ★ 抢回开着时说「前面的号有额度就换回它」，关着说「不主动换回」（旧那句写死的在抢回开着时是假话）。
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

/// 兜底：标了才说「b 只兜底 · … · 40 分钟内有号恢复就先等」，等待关掉（0）只说兜底；没标兜底不提等待。时段停用的号写出时段；
/// 有上限才说「都到上限即停 / 仍发」。
#[test]
fn the_explanation_names_wait_off_slots_and_at_limit_only_when_they_apply() {
    let mut r = base();
    let forty = copy_core::format_duration(40 * 60_000);
    assert!(
        !explain(&r).contains(&copy_text("beRotation.explain.wait", &[("dur", &forty)])),
        "没标兜底不提等待"
    );
    r.fallback = vec!["w".into()];
    assert!(has(
        &explain(&r),
        "beRotation.explain.fallback",
        &[("list", "w")]
    ));
    assert!(has(
        &explain(&r),
        "beRotation.explain.wait",
        &[("dur", &forty)]
    ));
    r.wait = 0;
    assert!(!explain(&r).contains(&copy_text("beRotation.explain.wait", &[("dur", &forty)])));
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

/// ★ 标了兜底就从「这个号」说「别的号有额度就不用 b」（点兜底号的名），紧跟兜底那一段、在等待之前（不管换法：兜底只临时用）；没标不说。
#[test]
fn the_explanation_says_the_fallback_hands_back() {
    let mut r = base();
    assert!(!has(
        &explain(&r),
        "beRotation.explain.leave",
        &[("list", "w")]
    ));
    r.fallback = vec!["w".into()];
    let forty = copy_core::format_duration(40 * 60_000);
    let want = [
        copy_text("beRotation.explain.fallback", &[("list", "w")]),
        copy_text("beRotation.explain.leave", &[("list", "w")]),
        copy_text("beRotation.explain.wait", &[("dur", &forty)]),
    ]
    .join(&copy_text("kit.text.sep", &[]));
    assert!(explain(&r).contains(&want), "{}", explain(&r));
    r.wait = 0;
    assert!(has(
        &explain(&r),
        "beRotation.explain.leave",
        &[("list", "w")]
    ));
}

/// ★ 抢回与兜底是两件事，说明里分开说：抢回那一段与兜底那几段不共用一个词「切回」；兜底那一段点兜底号的名。
/// 时长那一格是成品字（`format_duration` 出的「40 分钟」），不是「40m」这种缩写。
#[test]
fn preempt_and_fallback_read_apart_and_the_wait_is_spelled_out() {
    let mut r = base();
    r.preempt = true;
    r.fallback = vec!["w".into()];
    let text = explain(&r);
    let sep = copy_text("kit.text.sep", &[]);
    let parts: Vec<&str> = text.split(sep.as_str()).collect();
    assert!(parts.iter().all(|p| !p.contains("切回")), "{text}");
    let leave = copy_text("beRotation.explain.leave", &[("list", "w")]);
    assert!(parts.contains(&leave.as_str()), "{text}");
    assert!(leave.contains('w'), "兜底那一段点兜底号的名：{leave}");
    assert_ne!(
        leave,
        copy_text("beRotation.explain.preempt", &[]),
        "两件事一句话"
    );
    let forty = copy_text("durationFormat.unit.min", &[("n", "40")]);
    let wait = parts
        .iter()
        .find(|p| p.contains(&forty))
        .unwrap_or_else(|| panic!("等待那一段写成品字 {forty}：{text}"));
    assert!(!wait.contains("40m"), "{wait}");
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
