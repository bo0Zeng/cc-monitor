use super::*;
use crate::wire::SessionFaceExt as _;
use cmd_wire::session::{Ability, Attached, StartedBy, TermSession, TerminalCan};
use std::path::PathBuf;

const NOW: u64 = 10_000;

fn term(handle: &str, sid: Option<&str>, program: &str, clients: usize, last: u64) -> TerminalItem {
    TerminalItem {
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
                last_activity: 1,
            })
            .collect(),
        input: cmd_wire::session::TermInput::Shared,
        state: if sid.is_some() {
            TermState::Running
        } else {
            TermState::Idle
        },
        last_activity: last,
        can: TerminalCan {
            input: Ability::Yes,
            end: Ability::Yes,
        },
    }
}

fn list(ts: Vec<TerminalItem>) -> TerminalsList {
    TerminalsList {
        terminals: ts,
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
    foreign.started_by.client = None;
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
    t.state = TermState::ProgramExited;
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
