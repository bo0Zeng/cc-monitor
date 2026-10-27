//! ↗ 浮层那一张由 `cmd_wire::front` 一处写：壳的窗口结局 · 会话为什么没有链 ⇒ 标题 · 正文 · 灰字 · 语气 · 按钮。
//! 逐种手写期望；句子取文案表（同一把 `copy_text`），判的是「哪一种出哪几句、哪几颗按钮、什么语气」。
//! 挂在 `session_window_face.rs` 底下跑（那一张的两个写家：壳 · 本机后端 `session-window`，都调这一份）。

use cmd_wire::cells::{Tone, Words};
use cmd_wire::front::*;
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

fn said(o: FrontOutcome, tmux: bool) -> FrontFace {
    match o.told(tmux) {
        FrontTold::Said(f) => f,
        FrontTold::Switched => panic!("没切过去的结局写成了「切过去了」"),
    }
}

fn t(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn switched_says_nothing() {
    assert_eq!(FrontOutcome::Switched.told(false), FrontTold::Switched);
    assert_eq!(
        serde_json::to_value(FrontTold::Switched).unwrap(),
        serde_json::json!({"kind": "switched"})
    );
}

#[test]
fn several_says_how_many_and_why_and_does_not_pick_one() {
    let n = |why| FrontOutcome::Several {
        program: "WindowsTerminal.exe".into(),
        count: 3,
        why,
    };
    let args: &[(&str, &str)] = &[("program", "WindowsTerminal.exe"), ("n", "3")];
    assert_eq!(
        said(n(SeveralWhy::ConsoleUnreachable), false),
        face(
            &t("front.title.unsure"),
            &copy_text("front.body.severalConsole", args),
            Some(&t("front.hint.severalConsole")),
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        said(n(SeveralWhy::Elevated), false),
        face(
            &t("front.title.unsure"),
            &copy_text("front.body.severalElevated", args),
            Some(&t("front.hint.severalElevated")),
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        said(n(SeveralWhy::Foreign), false),
        face(
            &t("front.title.unsure"),
            &copy_text("front.body.severalForeign", args),
            None,
            Tone::Warn,
            vec![]
        )
    );
    assert_eq!(
        said(n(SeveralWhy::Unregistered), false),
        face(
            &t("front.title.unsure"),
            &copy_text("front.body.severalUnregistered", args),
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
        said(
            FrontOutcome::SwitchedWindow {
                why: TabMiss::TitleIgnored
            },
            false
        ),
        face(
            &t("front.title.tabMissed"),
            &t("front.body.tabTitleIgnored"),
            Some(&t("front.hint.tabTitleIgnored")),
            Tone::Plain,
            vec![]
        )
    );
    for (why, body) in [
        (TabMiss::NoUia, "front.body.tabNoUia"),
        (TabMiss::Elevated, "front.body.tabElevated"),
        (TabMiss::NoConsole, "front.body.tabNoConsole"),
    ] {
        assert_eq!(
            said(FrontOutcome::SwitchedWindow { why }, false),
            face(
                &t("front.title.tabMissed"),
                &t(body),
                None,
                Tone::Plain,
                vec![]
            ),
            "{why:?}"
        );
    }
    assert_eq!(
        said(FrontOutcome::Refused, false),
        face(
            &t("front.title.flashing"),
            &t("front.body.refused"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        said(
            FrontOutcome::NoWindow {
                program: "ssh.exe".into()
            },
            false
        ),
        face(
            &t("front.title.noTerminal"),
            &t("front.body.background"),
            None,
            Tone::Plain,
            vec![]
        )
    );
    assert_eq!(
        said(FrontOutcome::Unclear, false),
        face(&t("front.title.unsure"), "", None, Tone::Plain, vec![]),
        "句柄 / 进程号复用：不上细节"
    );
    assert_eq!(
        said(
            FrontOutcome::DesktopWontSwitch {
                desktop: "GNOME".into()
            },
            false
        ),
        face(
            &t("front.title.desktopWontSwitch"),
            &copy_text("front.body.desktopWontSwitch", &[("desktop", "GNOME")]),
            None,
            Tone::Plain,
            vec![FrontAct::OpenHere]
        )
    );
    assert_eq!(
        said(
            FrontOutcome::DesktopWontSwitch {
                desktop: String::new()
            },
            false
        )
        .body,
        Words(t("front.body.desktopWontSwitchBare")),
        "桌面名读不到：不写空名字"
    );
    assert_eq!(
        said(FrontOutcome::Unsupported, false),
        face(
            &t("front.title.failed"),
            &t("terminalFront.unavailable.short"),
            None,
            Tone::Plain,
            vec![]
        )
    );
}

#[test]
fn a_closed_window_offers_open_in_terminal_only_in_tmux() {
    assert_eq!(
        said(FrontOutcome::WindowGone, true),
        face(
            &t("front.title.gone"),
            "",
            None,
            Tone::Plain,
            vec![FrontAct::OpenInTerminal]
        )
    );
    assert!(
        said(FrontOutcome::WindowGone, false).acts.is_empty(),
        "不在 tmux 里：没有可接回的"
    );
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
        SeveralWhy::ConsoleUnreachable,
        SeveralWhy::Elevated,
        SeveralWhy::Foreign,
    ]
    .map(|why| {
        said(
            FrontOutcome::Several {
                program: "WindowsTerminal.exe".into(),
                count: 2,
                why,
            },
            false,
        )
    });
    let others = [
        Unplaced::NotLive.face(&copy_core::local_machine()),
        said(
            FrontOutcome::NoWindow {
                program: "claude.exe".into(),
            },
            false,
        ),
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
fn the_wire_shape_of_a_said_face() {
    let v = serde_json::to_value(FrontOutcome::WindowGone.told(true)).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "kind": "said",
            "title": t("front.title.gone"),
            "body": "",
            "tone": "plain",
            "acts": [{"kind": "open-in-terminal"}],
        })
    );
    let copy = serde_json::to_value(FrontAct::Copy { detail: "x".into() }).unwrap();
    assert_eq!(copy, serde_json::json!({"kind": "copy", "detail": "x"}));
}
