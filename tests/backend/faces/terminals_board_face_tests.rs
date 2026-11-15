use super::*;
use crate::control::terminals::{TermRowed, TermRows};
use crate::wire::SessionFaceExt as _;
use cmd_wire::session::{Ability, Attached, StartedBy, TermSession, TerminalCan};
use std::path::PathBuf;

const NOW: u64 = 10_000;

fn term(handle: &str, sid: Option<&str>, program: &str, clients: usize, last: u64) -> TermRowed {
    TermRowed {
        item: TerminalItem {
            terminal: handle.into(),
            host: cmd_wire::frame::TerminalHost::Tmux,
            name: format!("ccm-{handle}"),
            title: program.into(),
            program: program.into(),
            cwd: "/home/u/src/app".into(),
            session: sid.map(|s| TermSession {
                sid: s.into(),
                agent: None,
            }),
            started_by: StartedBy {
                client: Some("desktop".into()),
                mine: true,
            },
            clients: (0..clients)
                .map(|_| Attached {
                    kind: "terminal-window".into(),
                    since: 1,
                })
                .collect(),
            input: cmd_wire::session::TermInput::Shared,
            state: if sid.is_some() {
                TermState::Running
            } else {
                TermState::Idle
            },
            can: TerminalCan {
                input: Ability::Yes,
                end: Ability::Yes,
            },
        },
        last_activity: last,
    }
}

fn list(ts: Vec<TermRowed>) -> TermRows {
    TermRows {
        rows: ts,
        complete: true,
    }
}

fn live(activity: crate::agents::SessionActivity) -> SessionFace {
    SessionFace::of(crate::wire::Life::Live {
        activity: Some(activity),
        host: Some(crate::wire::Housed::In(crate::wire::TerminalHost::Tmux)),
    })
}

fn bits(sid: &str) -> Option<SessionBits> {
    use crate::agents::SessionActivity::*;
    let face = match sid {
        "wait" => live(NeedsYou),
        _ => live(Working),
    };
    Some(SessionBits {
        title: format!("title-{sid}"),
        account: Some("work".into()),
        face,
    })
}

fn home() -> Option<PathBuf> {
    Some(PathBuf::from("/home/u"))
}

fn keys(h: &BoardHere) -> Vec<&str> {
    h.rows.iter().map(|r| r.key.as_str()).collect()
}

#[test]
fn only_terminals_started_by_cc_monitor_are_listed() {
    let mut foreign = term("t-foreign", None, "bash", 0, NOW);
    foreign.item.started_by.client = None;
    let h = rows_here(
        &list(vec![foreign, term("t-ours", None, "bash", 0, NOW)]),
        &bits,
        NOW,
        home().as_deref(),
    );
    assert_eq!(keys(&h), vec!["t-ours"]);
}

#[test]
fn waiting_then_attached_then_background_newest_first_within_a_band() {
    let h = rows_here(
        &list(vec![
            term("bg-old", Some("a"), "claude", 0, NOW - 500),
            term("bg-new", Some("b"), "claude", 0, NOW - 10),
            term("attached", Some("c"), "claude", 1, NOW - 900),
            term("waiting", Some("wait"), "claude", 0, NOW - 3000),
        ]),
        &bits,
        NOW,
        home().as_deref(),
    );
    assert_eq!(keys(&h), vec!["waiting", "attached", "bg-new", "bg-old"]);
    let w = &h.rows[0];
    assert_eq!(w.dot, SessionDot::NeedsYou);
    assert_eq!(w.badge.as_ref(), Some(&w.dot_name));
    assert!(h.rows[1].badge.is_none());
}

#[test]
fn a_row_is_written_whole() {
    let h = rows_here(
        &list(vec![
            term("s", Some("c"), "claude", 2, NOW - 120),
            term("shell", None, "bash", 0, NOW - 5),
            term("cmd", None, "cargo", 0, NOW - 5),
        ]),
        &bits,
        NOW,
        home().as_deref(),
    );
    let by = |k: &str| h.rows.iter().find(|r| r.key == k).unwrap();
    let s = by("s");
    assert_eq!(s.title.0, "title-c");
    assert!(!s.mono);
    assert_eq!(s.sid.as_deref(), Some("c"));
    assert_eq!(
        s.meta.0,
        copy_text(
            "beTermBoard.meta.line",
            &[("who", "work"), ("cwd", "~/src/app")]
        )
    );
    assert_eq!(
        s.chips,
        vec![Words(copy_text("terminal.tag.windows", &[("n", "2")]))]
    );
    assert_eq!(s.at_text.0, copy_core::short_duration(120_000));
    assert_eq!(s.tags, vec![BoardTag::Session]);

    let shell = by("shell");
    assert!(shell.mono);
    assert_eq!(shell.dot, SessionDot::Idle);
    assert_eq!(shell.at_text.0, copy_core::short_duration(5_000));
    assert_eq!(
        shell.meta.0,
        copy_text(
            "beTermBoard.meta.line",
            &[
                ("who", &copy_text("beTermBoard.meta.noSession", &[])),
                ("cwd", "~/src/app")
            ]
        )
    );
    assert_eq!(
        shell.chips,
        vec![Words(copy_text("terminal.tag.background", &[]))]
    );
    assert_eq!(shell.tags, vec![BoardTag::Bare, BoardTag::Background]);
    assert_eq!(by("cmd").dot, SessionDot::Running);
    assert!(shell.find.contains("~/src/app") && shell.find.contains("ccm-shell"));
}

#[test]
fn an_exited_program_carries_its_state_as_a_chip() {
    let mut t = term("x", Some("gone"), "bash", 0, NOW);
    t.item.state = TermState::ProgramExited;
    let h = rows_here(&list(vec![t]), &|_| None, NOW, home().as_deref());
    let r = &h.rows[0];
    let face = SessionFace::of(crate::wire::Life::Reconnectable);
    assert_eq!(r.dot, face.dot);
    assert!(r.chips.contains(&face.name));
}

#[test]
fn merge_keeps_machine_order_counts_and_draws_the_silent_one_from_last_time() {
    let last = LastSeen::new();
    let here = |ts| rows_here(&list(ts), &bits, NOW, home().as_deref());
    let gpu = Machine::named("gpu");
    // 第一次 gpu 答了：记下。
    merge(
        vec![(
            gpu.clone(),
            Got::Answered(
                here(vec![term("g", Some("c"), "claude", 0, NOW)]),
                "10:31".into(),
            ),
        )],
        &last,
    );
    let b = merge(
        vec![
            (
                Machine::local(),
                Got::Answered(
                    here(vec![
                        term("a", Some("c"), "claude", 1, NOW),
                        term("b", None, "bash", 0, NOW),
                    ]),
                    "10:42".into(),
                ),
            ),
            (gpu.clone(), Got::Down("offline".into())),
            (Machine::named("never"), Got::Down("offline".into())),
        ],
        &last,
    );
    let names: Vec<&str> = b.groups.iter().map(|g| g.name.0.as_str()).collect();
    assert_eq!(
        names,
        vec![
            copy_text("detail.label.local", &[]).as_str(),
            "gpu",
            "never"
        ]
    );
    assert!(b.groups[0].down.is_none());
    let g = &b.groups[1];
    assert_eq!(
        g.down.as_ref().unwrap().said.0,
        copy_text(
            "beTermBoard.group.downSampled",
            &[("machine", "gpu"), ("at", "10:31")]
        )
    );
    assert_eq!(g.rows.len(), 1);
    assert_eq!(g.rows[0].dot, cmd_wire::frame::SessionDot::Unknown);
    assert!(b.groups[2].rows.is_empty() && b.groups[2].down.is_some());
    // 键跨台不撞。
    assert_ne!(b.groups[0].rows[0].key, "a");
    // 单独终端窗口的系统标题带那台的名字。
    assert_eq!(
        g.rows[0].window_title.0,
        copy_text(
            "beTermBoard.window.title",
            &[("title", &g.rows[0].title.0), ("machine", "gpu")]
        )
    );
    let counts: Vec<(BoardTag, u32)> = b.filters.iter().map(|f| (f.tag, f.count)).collect();
    assert_eq!(
        counts,
        vec![
            (BoardTag::All, 3),
            (BoardTag::Session, 2),
            (BoardTag::Bare, 1),
            (BoardTag::Background, 2),
        ]
    );
    assert_eq!(
        b.groups[0].sub.0,
        copy_text("beTermBoard.group.countBg", &[("n", "2"), ("bg", "1")])
    );
}

/// ★ 「终端」视图的成品只住它的 face：控制面包里没有 `terminals_board` 那一份；认 shell 那张词表住 `platform/shell`。
#[test]
fn the_terminals_board_product_lives_only_in_its_face() {
    let control = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../control");
    assert!(
        control.join("mod.rs").is_file(),
        "控制面住址不对：{}",
        control.display()
    );
    assert!(!control.join("terminals_board.rs").exists());
    let m = guard_core::production_code(&std::fs::read_to_string(control.join("mod.rs")).unwrap());
    assert!(!guard_core::contains_word(&m, "mod terminals_board"));
    assert!(crate::platform::shell::is_shell_program("-bash"));
    assert!(!crate::platform::shell::is_shell_program("vim"));
}

/// ★ 在等人的两颗点（待答请求 · 需手动）都排在最前、都带徽章（字照脸上的名字）。
#[test]
fn both_waiting_dots_lead_and_carry_the_badge() {
    let asks = |sid: &str| {
        let face = live(crate::agents::SessionActivity::NeedsYou).asking(sid == "ask");
        Some(SessionBits {
            title: format!("title-{sid}"),
            account: None,
            face,
        })
    };
    let h = rows_here(
        &list(vec![
            term("attached", Some("c"), "claude", 1, NOW - 10),
            term("asking", Some("ask"), "claude", 0, NOW - 3000),
            term("manual", Some("m"), "claude", 0, NOW - 2000),
        ]),
        &|sid| if sid == "c" { bits(sid) } else { asks(sid) },
        NOW,
        home().as_deref(),
    );
    assert_eq!(keys(&h), vec!["manual", "asking", "attached"]);
    assert_eq!(h.rows[1].dot, SessionDot::Asks);
    let face = asks("ask").unwrap().face;
    assert_eq!(
        (face.name.0.as_str(), face.line.0.as_str()),
        (h.rows[1].dot_name.0.as_str(), h.rows[1].dot_name.0.as_str()),
        "终端板那张脸：那一句与点同说待答请求"
    );
    for r in &h.rows[..2] {
        assert_eq!(r.badge.as_ref(), Some(&r.dot_name), "{}", r.key);
    }
}
