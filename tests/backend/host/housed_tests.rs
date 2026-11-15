//! 一条会话住在哪（[`housed`] · [`Housing`]）：照这台的名单（tmux · 托管）认；名单里不挂它 · 列不出 ⇒ 不知道。

use super::*;
use crate::wire::{SessionContainer, TerminalHost};

fn row(host: TerminalHost, id: &str, pane: Option<&str>, sid: &str) -> TermRow {
    TermRow {
        host,
        id: id.into(),
        pane: pane.map(Into::into),
        shared: false,
        name: format!("n-{id}"),
        windows: 1,
        activity: 0,
        sid: sid.into(),
        agent: "claude".into(),
        client: String::new(),
        program: "claude".into(),
        cwd: "/".into(),
        title: String::new(),
        input: Input::Shared,
    }
}

#[test]
fn a_session_lives_where_the_lists_carry_it() {
    let rows = vec![
        row(TerminalHost::Tmux, "$3", Some("%7"), "s-tmux"),
        row(TerminalHost::Tmux, "$4", None, ""),
        row(TerminalHost::Managed, "h1", None, "s-managed"),
    ];
    assert_eq!(
        housed(&rows, "s-tmux"),
        Some(SessionContainer::Tmux {
            terminal: Some(rows[0].handle())
        })
    );
    assert_eq!(
        housed(&rows, "s-managed"),
        Some(SessionContainer::Managed {
            terminal: Some("h1".into())
        })
    );
    assert_eq!(
        housed(&rows, "s-elsewhere"),
        None,
        "不挂在哪一行上 ⇒ 不知道"
    );
    assert_eq!(housed(&rows, ""), None, "空 sid 不认那几行没挂 sid 的");
}

/// 每趟至多列一次；一条都没问 ⇒ 一次都不列；列不出（没有行）⇒ 不知道。
#[test]
fn the_lists_are_read_at_most_once_and_only_when_asked() {
    let calls = std::cell::Cell::new(0u32);
    let list = || {
        calls.set(calls.get() + 1);
        vec![row(TerminalHost::Managed, "h1", None, "s1")]
    };
    let h = Housing::with(&list);
    assert_eq!(calls.get(), 0, "没问 ⇒ 不列");
    for life in [
        crate::wire::WireLife::Reconnectable,
        crate::wire::WireLife::Ended { by: None },
        crate::wire::WireLife::Gone,
        crate::wire::WireLife::Unclear,
    ] {
        assert_eq!(h.live("s1", &life), None, "{life:?}");
    }
    assert_eq!(calls.get(), 0, "一条活会话都没有 ⇒ 不列");
    assert_eq!(
        h.live("s1", &crate::wire::WireLife::Live { activity: None }),
        Some(SessionContainer::Managed {
            terminal: Some("h1".into())
        })
    );
    drop(h);
    calls.set(0);
    let h = Housing::with(&list);
    assert!(h.of("s1").is_some());
    assert_eq!(h.of("s2"), None);
    assert!(h.of("s1").is_some());
    assert_eq!(calls.get(), 1, "一趟只列一次");

    let none = Vec::new;
    let h = Housing::with(&none);
    assert_eq!(h.of("s1"), None, "列不出 ⇒ 不知道");
}
