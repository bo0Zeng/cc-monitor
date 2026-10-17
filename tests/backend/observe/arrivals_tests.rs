//! 报到那一本：只认 `since` 之后报到的 · 按 sid / 按目录 · 早到的当场交 · 已活着的重宣告不算新报到。

use super::*;
use crate::wire::SessionFaceExt as _;
use crate::wire::{RemovalCause, SessionAdded, SessionRemoved};

fn added(sid: &str, cwd: Option<&str>) -> Frame {
    Frame::SessionAdded(SessionAdded {
        sid: sid.into(),
        agent_kind: "claude".to_string(),
        background: false,
        attachable: None,
        cwd: cwd.map(str::to_string),
        project_dir: None,
        name: None,
        path: None,
        face: crate::wire::SessionFace::of(crate::wire::Life::Live {
            activity: None,
            tmux: None,
        }),
        name_face: crate::history::search_rules::name_face(&Default::default()),
        waiting_for: None,
        hidden: None,
        container: None,
        pid: 0,
    })
}

fn removed(sid: &str) -> Frame {
    Frame::SessionRemoved(SessionRemoved {
        sid: sid.into(),
        cause: RemovalCause::Gone,
        by: None,
    })
}

fn ear() -> (Tell, tokio::sync::oneshot::Receiver<String>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (Arc::new(Mutex::new(Some(tx))), rx)
}

fn by_cwd(c: &str) -> ArriveBy {
    ArriveBy::Cwd { cwd: c.into() }
}

fn by_sid(s: &str) -> ArriveBy {
    ArriveBy::Sid { sid: s.into() }
}

#[test]
fn hit_takes_only_what_arrived_after_since_by_sid_or_by_directory() {
    assert!(hit(&by_sid("a"), 3, "a", 4, None));
    assert!(!hit(&by_sid("a"), 4, "a", 4, None), "那一刻之前就在的不算");
    assert!(!hit(&by_sid("a"), 3, "b", 4, None));
    assert!(
        hit(&by_cwd("/w/p"), 3, "x", 4, Some("/w/p/")),
        "末尾的 / 不算"
    );
    assert!(!hit(&by_cwd("/w/p"), 3, "x", 4, Some("/w/q")));
    assert!(!hit(&by_cwd("/w/p"), 3, "x", 4, None), "说不出目录的不认");
}

#[test]
fn a_session_arriving_after_the_ask_is_handed_over() {
    let mut a = Arrivals::default();
    let since = crate::common::arrival_seq::mark();
    let (t, mut rx) = ear();
    a.expect(by_sid("s1"), since, t);
    a.on_frame(&added("s0", Some("/w")));
    assert!(rx.try_recv().is_err(), "别的会话不交");
    a.on_frame(&added("s1", Some("/w")));
    assert_eq!(rx.try_recv().ok().as_deref(), Some("s1"));
}

#[test]
fn by_directory_an_old_session_there_does_not_count_and_a_new_one_does() {
    let mut a = Arrivals::default();
    a.on_frame(&added("old", Some("/w/p")));
    let since = crate::common::arrival_seq::mark();
    let (t, mut rx) = ear();
    a.expect(by_cwd("/w/p"), since, t);
    assert!(rx.try_recv().is_err(), "起之前就在这个目录里的不算");
    // 已活着的那条又宣告一次（对齐重报）：不是新报到。
    a.on_frame(&added("old", Some("/w/p")));
    assert!(rx.try_recv().is_err());
    a.on_frame(&added("new", Some("/w/p")));
    assert_eq!(rx.try_recv().ok().as_deref(), Some("new"));
}

#[test]
fn one_that_arrived_before_the_ask_but_after_since_is_handed_at_once() {
    let mut a = Arrivals::default();
    let since = crate::common::arrival_seq::mark();
    a.on_frame(&added("early", Some("/w/e")));
    let (t, mut rx) = ear();
    a.expect(by_cwd("/w/e"), since, t);
    assert_eq!(rx.try_recv().ok().as_deref(), Some("early"));
}

#[test]
fn a_session_that_left_and_came_back_arrives_anew() {
    let mut a = Arrivals::default();
    a.on_frame(&added("r", None));
    let since = crate::common::arrival_seq::mark();
    let (t, mut rx) = ear();
    a.expect(by_sid("r"), since, t);
    assert!(rx.try_recv().is_err());
    a.on_frame(&removed("r"));
    a.on_frame(&added("r", None));
    assert_eq!(rx.try_recv().ok().as_deref(), Some("r"));
}
