//! ↗ 浮层那一张由 `face_core::front` 一处写（只本机后端 `session-window` 调）：会话为什么没有链（`Unplaced::face`）·
//! 壳窗口那一跳没成的每一种结局码（`missed_faces`）⇒ 标题 · 正文 · 灰字 · 语气 · 按钮。
//! 逐种手写期望；句子取文案表（同一把 `copy_text`），判的是「哪一种出哪几句、哪几颗按钮、什么语气」。

use crate::front::*;
use cmd_wire::cells::{Tone, Words};
use cmd_wire::front::{FrontAct, FrontCode, FrontFace};
use copy_core::copy_text;

fn face(title: &str, body: &str, hint: Option<&str>, tone: Tone, acts: Vec<FrontAct>) -> FrontFace {
    FrontFace {
        title: Words(title.to_string()),
        body: Words(body.to_string()),
        hint: hint.map(|h| Words(h.to_string())),
        tone,
        acts,
    }
}

/// 那一串里结局码 `code` 的那一张。
fn missed(code: FrontCode, tmux: bool) -> FrontFace {
    let all = missed_faces(tmux);
    let mut hits = all.iter().filter(|m| m.code == code);
    let hit = hits.next().unwrap_or_else(|| panic!("{code:?} 没写那一张"));
    assert!(hits.next().is_none(), "{code:?} 写了两张");
    hit.face.clone()
}

fn t(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn every_code_has_exactly_one_face() {
    for tmux in [false, true] {
        let all = missed_faces(tmux);
        assert_eq!(all.len(), FrontCode::ALL.len(), "一个码一张");
        for code in FrontCode::ALL {
            assert_eq!(all.iter().filter(|m| m.code == code).count(), 1, "{code:?}");
        }
    }
}

#[test]
fn several_says_why_and_does_not_pick_one() {
    assert_eq!(
        missed(FrontCode::SeveralConsole, false),
        face(
            &t("front.title.unsure"),
            &t("front.body.severalConsole"),
            Some(&t("front.hint.severalConsole")),
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::SeveralElevated, false),
        face(
            &t("front.title.unsure"),
            &t("front.body.severalElevated"),
            Some(&t("front.hint.severalElevated")),
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::SeveralForeign, false),
        face(
            &t("front.title.unsure"),
            &t("front.body.severalForeign"),
            None,
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::SeveralUnregistered, false),
        face(
            &t("front.title.unsure"),
            &t("front.body.severalUnregistered"),
            Some(&t("front.hint.unregistered")),
            Tone::Warn,
            vec![FrontAct::Connect]
        ),
        "Linux：没登记 ⇒ 给［front.act.connect］"
    );
}

#[test]
fn the_window_outcomes() {
    assert_eq!(
        missed(FrontCode::TabTitleIgnored, false),
        face(
            &t("front.title.tabMissed"),
            &t("front.body.tabTitleIgnored"),
            Some(&t("front.hint.tabTitleIgnored")),
            Tone::Plain,
            vec![]
        )
    );
    for (code, body) in [
        (FrontCode::TabNoUia, "front.body.tabNoUia"),
        (FrontCode::TabElevated, "front.body.tabElevated"),
        (FrontCode::TabNoConsole, "front.body.tabNoConsole"),
    ] {
        assert_eq!(
            missed(code, false),
            face(
                &t("front.title.tabMissed"),
                &t(body),
                None,
                Tone::Plain,
                vec![]
            ),
            "{code:?}"
        );
    }
    assert_eq!(
        missed(FrontCode::Refused, false),
        face(
            &t("front.title.flashing"),
            &t("front.body.refused"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::NoWindow, false),
        face(
            &t("front.title.noTerminal"),
            &t("front.body.background"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::Unclear, false),
        face(&t("front.title.unsure"), "", None, Tone::Plain, vec![]),
        "句柄 / 进程号复用：不上细节"
    );
    assert_eq!(
        missed(FrontCode::DesktopWontSwitch, false),
        face(
            &t("front.title.desktopWontSwitch"),
            &t("front.body.desktopWontSwitchBare"),
            None,
            Tone::Plain,
            vec![FrontAct::OpenHere]
        )
    );
    assert_eq!(
        missed(FrontCode::Unsupported, false),
        face(
            &t("front.title.failed"),
            &t("terminalFront.unavailable.short"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        missed(FrontCode::TimedOut, false),
        face(
            &t("front.title.failed"),
            &copy_text(
                "front.body.timeout",
                &[("machine", &copy_core::local_machine())]
            ),
            None,
            Tone::Fail,
            vec![FrontAct::Retry]
        ),
        "窗口那一跳卡住：这台没在期限内答，可重试"
    );
}

#[test]
fn a_closed_window_offers_open_in_terminal_only_in_tmux() {
    assert_eq!(
        missed(FrontCode::Gone, true),
        face(
            &t("front.title.gone"),
            "",
            None,
            Tone::Plain,
            vec![FrontAct::OpenInTerminal]
        )
    );
    assert!(
        missed(FrontCode::Gone, false).acts.is_empty(),
        "不在 tmux 里：没有可接回的"
    );
    for code in FrontCode::ALL.into_iter().filter(|c| *c != FrontCode::Gone) {
        assert_eq!(
            missed(code, true),
            missed(code, false),
            "{code:?} 与 tmux 无关"
        );
    }
}

#[test]
fn why_there_is_no_chain_with_that_machines_name() {
    let m = "gpu-01";
    let f = |u: Unplaced| u.face(m);
    assert_eq!(
        f(Unplaced::NotLive),
        face(
            &t("front.title.unbound"),
            &t("front.body.notLive"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        f(Unplaced::Detached),
        face(
            &t("front.title.noTerminal"),
            &t("front.body.detached"),
            None,
            Tone::Plain,
            vec![FrontAct::OpenInTerminal]
        )
    );
    assert_eq!(
        f(Unplaced::NoTerminal),
        face(
            &t("front.title.noTerminal"),
            &t("front.body.background"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        f(Unplaced::Elsewhere {
            addr: "203.0.113.8".into()
        }),
        face(
            &t("front.title.elsewhere"),
            &copy_text("front.body.elsewhere", &[("addr", "203.0.113.8")]),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        f(Unplaced::NotSsh),
        face(
            &t("front.title.elsewhere"),
            &copy_text("front.body.notSsh", &[("machine", m)]),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        f(Unplaced::Mismatch),
        face(
            &t("front.title.unsure"),
            &t("front.body.mismatch"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    let unreadable = copy_text("front.body.unreadable", &[("machine", m)]);
    assert_eq!(
        f(Unplaced::Unreadable),
        face(
            &t("front.title.unsure"),
            &unreadable,
            None,
            Tone::Plain,
            vec![FrontAct::Copy {
                detail: unreadable.clone()
            }]
        )
    );
    assert_eq!(
        f(Unplaced::QueryFailed),
        face(
            &t("front.title.unknown"),
            "",
            None,
            Tone::Fail,
            vec![
                FrontAct::Retry,
                FrontAct::Copy {
                    detail: "session-window: query-failed".into()
                }
            ]
        )
    );
    assert_eq!(
        f(Unplaced::PeerOld),
        face(
            &copy_text("front.title.update", &[("machine", m)]),
            &t("front.body.update"),
            None,
            Tone::Warn,
            vec![FrontAct::Update]
        )
    );
    assert_eq!(
        f(Unplaced::PeerTimedOut),
        face(
            &t("front.title.failed"),
            &copy_text("front.body.timeout", &[("machine", m)]),
            None,
            Tone::Fail,
            vec![FrontAct::Retry]
        )
    );
    assert_eq!(
        f(Unplaced::Unreached {
            said: "连接层写的那一句".into()
        }),
        face(
            &t("front.title.failed"),
            "连接层写的那一句",
            None,
            Tone::Fail,
            vec![FrontAct::Reconnect]
        ),
        "没问到那台：连接层那一句照摆"
    );
}

#[test]
fn windows_sentences_do_not_mention_registering() {
    let several = [
        FrontCode::SeveralConsole,
        FrontCode::SeveralElevated,
        FrontCode::SeveralForeign,
    ]
    .map(|code| missed(code, false));
    let others = [
        Unplaced::NotLive.face(&copy_core::local_machine()),
        missed(FrontCode::NoWindow, false),
    ];
    for f in several.into_iter().chain(others) {
        let text = format!(
            "{} {} {}",
            f.title.0,
            f.body.0,
            f.hint.clone().map(|h| h.0).unwrap_or_default()
        );
        let connect = t("front.act.connect");
        assert!(
            !["登记", "新开", connect.as_str()]
                .iter()
                .any(|w| text.contains(w)),
            "{text}"
        );
        assert!(!f.acts.contains(&FrontAct::Connect), "{text}");
    }
}

#[test]
fn the_wire_shape_of_a_missed_face() {
    let gone = missed_faces(true)
        .into_iter()
        .find(|m| m.code == FrontCode::Gone)
        .expect("gone 那一张");
    assert_eq!(
        serde_json::to_value(gone).unwrap(),
        serde_json::json!({
            "code": "gone",
            "face": {
                "title": t("front.title.gone"),
                "body": "",
                "tone": "plain",
                "acts": [{"kind": "open-in-terminal"}],
            },
        })
    );
    let copy = serde_json::to_value(FrontAct::Copy { detail: "x".into() }).unwrap();
    assert_eq!(copy, serde_json::json!({"kind": "copy", "detail": "x"}));
}
