//! 通道那一句（`chan/said.rs`）的判据。
//!
//! | 性质 | 判据 |
//! |---|---|
//! | 每一种失败（三层逐格穷举）× 本机 / 远端都有一句：非空、不带枚举名与码；说到那台的称呼它（远端名字 · 本机「本机」） | [`every_failure_has_a_plain_sentence_on_both_kinds_of_machine`] |
//! | 「发没发出去」一定说出来：没发出 ≠ 拿不准；发了与拿不准对人是同一件事 | [`whether_it_was_sent_is_always_said`] |
//! | 对端拒了：它那一句原样；没写 ⇒「后端拒绝 · 原因不明」 | [`a_refusal_says_the_peer_sentence_or_that_it_is_unreadable`] |
//! | 全产品一份：`rsChanSaid.*` 只在本文件取；调它的恰是那三处（壳交主界面 · 文件窗口 · `CallError` 的 `Display`） | [`the_channel_sentence_lives_in_one_file_with_three_callers`] |

use super::{machine, said};
use crate::chan::wire::{Body, CallError, HopFault, HopId, OursFault, PeerFault, Reach};
use crate::origin::Origin;
use std::collections::BTreeSet;

/// 这一格的名字。**穷尽 `match`、不许 `_ =>`**：枚举多一格这里编不过，[`cases`] 也就得补上那一格。
fn arm(e: &CallError) -> String {
    match e {
        CallError::Hop { reach, why, .. } => {
            let r = match reach {
                Reach::NotSent => "notSent",
                Reach::Sent => "sent",
                Reach::Unknown => "unknown",
            };
            let w = match why {
                HopFault::Unreachable => "unreachable",
                HopFault::Dropped => "dropped",
                HopFault::Overrun => "overrun",
            };
            format!("hop/{r}/{w}")
        }
        CallError::Peer {
            why: PeerFault::Unsupported,
        } => "peer/unsupported".into(),
        CallError::Peer {
            why: PeerFault::Refused { .. },
        } => "peer/refused".into(),
        CallError::Ours { why, runs_on } => {
            let w = match why {
                OursFault::Cancelled => "cancelled",
                OursFault::Misuse => "misuse",
                OursFault::Broken => "broken",
            };
            format!("ours/{w}/{runs_on}")
        }
    }
}

/// 全部格（[`arm`] 的每一种值各一个样本）＋ 这一句该不该称呼那台。
fn cases() -> Vec<(CallError, bool)> {
    let mut out = Vec::new();
    for reach in [Reach::NotSent, Reach::Sent, Reach::Unknown] {
        for why in [HopFault::Unreachable, HopFault::Dropped, HopFault::Overrun] {
            let e = CallError::Hop {
                at: HopId {
                    idx: 1,
                    tag: "wait",
                },
                reach,
                why,
            };
            // 期限早已过、一个字节都没发：与是哪台无关。
            out.push((e, !(reach == Reach::NotSent && why == HopFault::Overrun)));
        }
    }
    out.push((
        CallError::Peer {
            why: PeerFault::Unsupported,
        },
        true,
    ));
    out.push((
        CallError::Peer {
            why: PeerFault::Refused {
                body: Body(br#"{"code":"no_tmux"}"#.to_vec()),
            },
        },
        true,
    ));
    for why in [OursFault::Cancelled, OursFault::Misuse, OursFault::Broken] {
        for runs_on in [false, true] {
            let names = why == OursFault::Cancelled && runs_on;
            out.push((CallError::Ours { why, runs_on }, names));
        }
    }
    out
}

/// 一句话里不许出现的：枚举名（`{:?}`）· 跳的标签 · 码。
const NOT_ON_SCREEN: &[&str] = &[
    "Hop",
    "Peer",
    "Ours",
    "NotSent",
    "Sent",
    "Unknown",
    "Unreachable",
    "Dropped",
    "Overrun",
    "Unsupported",
    "Refused",
    "Cancelled",
    "Misuse",
    "Broken",
    "runs_on",
    "wait",
    "no_tmux",
    "<local>",
];

#[test]
fn every_failure_has_a_plain_sentence_on_both_kinds_of_machine() {
    let all = cases();
    let got: BTreeSet<String> = all.iter().map(|(e, _)| arm(e)).collect();
    // 两向：样本覆盖了每一格（3×3 跳 ＋ 2 对端 ＋ 3×2 本侧），也没有重复格。
    assert_eq!(got.len(), all.len(), "样本里有重复的格：{got:?}");
    assert_eq!(got.len(), 9 + 2 + 6, "样本没覆盖全部格：{got:?}");
    let remote = Origin("devbox".into());
    let local = Origin::local();
    for (e, names) in &all {
        for origin in [&remote, &local] {
            let s = said(e, origin);
            let who = arm(e);
            assert!(!s.trim().is_empty(), "{who} @ {}：没有那一句", origin.0);
            for w in NOT_ON_SCREEN {
                assert!(!s.contains(w), "{who} @ {}：{s:?} 里带了 {w:?}", origin.0);
            }
            if *names {
                let call = machine(origin);
                assert!(
                    s.contains(&call),
                    "{who} @ {}：{s:?} 没称呼那台（{call}）",
                    origin.0
                );
            }
        }
    }
    assert_eq!(machine(&local), copy_core::local_machine());
    assert_eq!(machine(&remote), "devbox");
}

/// 写面那几条据此决定要不要再点一次：没发出 ＝ 这一下没生效；发了 / 拿不准 ＝ 对面可能已经做了。
#[test]
fn whether_it_was_sent_is_always_said() {
    let hop = |reach, why| CallError::Hop {
        at: HopId {
            idx: 1,
            tag: "read",
        },
        reach,
        why,
    };
    for origin in [Origin("devbox".into()), Origin::local()] {
        for why in [HopFault::Unreachable, HopFault::Dropped, HopFault::Overrun] {
            let not_sent = said(&hop(Reach::NotSent, why), &origin);
            let unknown = said(&hop(Reach::Unknown, why), &origin);
            assert_ne!(
                not_sent, unknown,
                "{why:?} @ {}：没发出与拿不准说成了同一句",
                origin.0
            );
            assert_eq!(
                unknown,
                said(&hop(Reach::Sent, why), &origin),
                "{why:?} @ {}",
                origin.0
            );
        }
    }
}

#[test]
fn a_refusal_says_the_peer_sentence_or_that_it_is_unreadable() {
    let refused = |body: &[u8]| CallError::Peer {
        why: PeerFault::Refused {
            body: Body(body.to_vec()),
        },
    };
    let devbox = Origin("devbox".into());
    assert_eq!(
        said(
            &refused(r#"{"code":"no_tmux","message":"devbox 无 tmux"}"#.as_bytes()),
            &devbox
        ),
        "devbox 无 tmux"
    );
    let unreadable =
        copy_core::copy_text("rsChanSaid.peer.refusedNoReason", &[("machine", "devbox")]);
    for body in [&b"not json"[..], br#"{"code":"x","message":"  "}"#, b""] {
        assert_eq!(said(&refused(body), &devbox), unreadable, "{body:?}");
    }
}

#[test]
fn the_channel_sentence_lives_in_one_file_with_three_callers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut keys = BTreeSet::new();
    let mut callers = BTreeSet::new();
    for rel in guard_core::files_by_extension(&root.join("src"), "rs") {
        if rel.starts_with("vendor/") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(root.join("src").join(&rel)) else {
            continue;
        };
        let code = guard_core::production_code(&raw);
        if code.contains("\"rsChanSaid.") {
            keys.insert(rel.clone());
        }
        if code.contains("said::said(") {
            callers.insert(rel);
        }
    }
    assert_eq!(
        keys,
        BTreeSet::from(["comms/inward/chan/said.rs".to_string()]),
        "通道那一句只许在 `chan/said.rs` 取"
    );
    assert_eq!(
        callers,
        BTreeSet::from([
            "comms/inward/chan/wire.rs".to_string(),
            "frontend/filewin/src/source.rs".to_string(),
            "frontend/shell/src/chan/webview.rs".to_string(),
        ]),
        "调通道那一句的应恰是这三处：壳交给主界面的失败 · 文件窗口 · `CallError` 的 `Display`"
    );
}
