//! 已经发生的那一截：最上那条照换号记录切段，每号一行只画说得出的（重置 · 被换下），说不出的空着。

use super::*;

const NOW: u64 = 1_760_000_000;
const FROM: u64 = NOW - 7200;

fn log(at: u64, from: &str, to: &str, cause: Cause) -> Log {
    Log {
        at,
        from: from.into(),
        to: to.into(),
        cause,
        why: None,
    }
}

fn acct(id: &str, used5h: f64, reset5h: Option<u64>) -> Acct {
    Acct {
        id: id.into(),
        api: false,
        used: [used5h, 0.0],
        resets: [reset5h, None],
        refused_until: None,
        barred: None,
        rate: [None, None],
        per_pct: [None, None],
    }
}

fn u(from: u64, to: u64, a: Option<&str>) -> Use {
    Use {
        from,
        to,
        account: a.map(str::to_string),
    }
}

/// ★ 照记录切段：换号把段切开；停发停到 `until`，之后接着用原来的号；这一截之前那一条定开头用的是谁。
#[test]
fn the_top_bar_follows_the_log() {
    let l = [
        log(FROM - 600, "a", "b", Cause::Order),
        log(NOW - 5400, "b", "a", Cause::Unusable),
        log(
            NOW - 1800,
            "a",
            "a",
            Cause::Hold {
                until: Some(NOW - 900),
            },
        ),
    ];
    assert_eq!(
        uses(&l, "a", NOW - 900, FROM, NOW, false),
        [
            u(FROM, NOW - 5400, Some("b")),
            u(NOW - 5400, NOW - 1800, Some("a")),
            u(NOW - 1800, NOW - 900, None),
            u(NOW - 900, NOW, Some("a")),
        ]
    );
}

/// 记录一条都没有 ⇒ 从会话起来那一刻起；这一截之前没有记录 ⇒ 开头是第一条换下来的那个号。
#[test]
fn without_a_log_the_bar_starts_when_the_session_did() {
    assert_eq!(
        uses(&[], "a", NOW - 600, FROM, NOW, false),
        [u(NOW - 600, NOW, Some("a"))]
    );
    let l = [log(NOW - 3600, "b", "a", Cause::Temp)];
    assert_eq!(
        uses(&l, "a", NOW - 3600, FROM, NOW, false),
        [
            u(FROM, NOW - 3600, Some("b")),
            u(NOW - 3600, NOW, Some("a"))
        ]
    );
}

/// 停发几点停完说不出：此刻还在停 ⇒ 画到此刻；此刻已经不停了 ⇒ 那一截空着（不编）。
#[test]
fn a_hold_with_no_end_is_drawn_only_while_it_lasts() {
    let l = [log(NOW - 1200, "a", "a", Cause::Hold { until: None })];
    assert_eq!(
        uses(&l, "a", NOW - 7000, FROM, NOW, true),
        [u(FROM, NOW - 1200, Some("a")), u(NOW - 1200, NOW, None)]
    );
    assert_eq!(
        uses(&l, "a", NOW - 7000, FROM, NOW, false),
        [u(FROM, NOW - 1200, Some("a"))]
    );
}

/// ★ 每号一行：窗口开在这一截里 ⇒ 那一刻起是重置那段；之前那一窗说不出 ⇒ 空着。
#[test]
fn a_window_opened_inside_the_span_starts_a_reset_segment() {
    let a = acct("a", 30.0, Some(NOW + 4 * 3600));
    assert_eq!(
        lane(&a, &[], FROM, NOW),
        [Seg {
            from: NOW - 3600,
            to: NOW,
            sendable: true,
            reset: true
        }]
    );
    // 窗口开在这一截之前、此刻发得出去 ⇒ 整截发得出去。
    let a = acct("a", 30.0, Some(NOW + 600));
    assert_eq!(
        lane(&a, &[], FROM, NOW),
        [Seg {
            from: FROM,
            to: NOW,
            sendable: true,
            reset: false
        }]
    );
}

/// ★ 此刻发不出去：记录里它被换下那一刻起发不出去、之前发得出去；记录里没有 ⇒ 说不出，空着。
#[test]
fn an_exhausted_account_is_drawn_out_only_from_when_it_was_switched_away() {
    let b = acct("b", 100.0, Some(NOW + 600));
    let l = [log(NOW - 5400, "b", "a", Cause::Unusable)];
    assert_eq!(
        lane(&b, &l, FROM, NOW),
        [
            Seg {
                from: FROM,
                to: NOW - 5400,
                sendable: true,
                reset: false
            },
            Seg {
                from: NOW - 5400,
                to: NOW,
                sendable: false,
                reset: false
            },
        ]
    );
    assert!(
        lane(&b, &[], FROM, NOW).is_empty(),
        "说不出几点用满的，不该画"
    );
    // 换下它的原因不是「发不出去」⇒ 不算。
    let l = [log(NOW - 5400, "b", "a", Cause::Order)];
    assert!(lane(&b, &l, FROM, NOW).is_empty());
}
