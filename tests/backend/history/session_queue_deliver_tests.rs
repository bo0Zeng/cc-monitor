//! 会话输入队的投递：一次一条、到「能送」才送 · 送出后等那一轮起来再送下一条 · 在等人不送 · 活过又不活 ⇒ 未送达 ·
//! 没活过 ⇒ 送（恢复那一支） · 被拒 ⇒ 那一条未送达、后面的接着送。
//!
//! ⚠ 家目录一律是临时目录（`Own::issue_at`）；送字与活会话表都注入，不碰 tmux、不碰真 `~/.cc-monitor/`。

use super::*;
use crate::agents::SessionActivity as A;
use crate::control::terminals_wire::SentResult;
use crate::faces::session_send::{Delivery, Undeliverable};
use crate::history::session_queue as q;
use cmd_wire::queue::{QueueItem, QueueState as S, SessionQueuePutReq};
use cmd_wire::Machine;
use std::cell::RefCell;

const SID: &str = "s1";
const T0: i64 = 1_800_000_000_000;

fn own(tag: &str) -> crate::platform::fs::Own {
    let d = std::env::temp_dir().join(format!("ccm-sdeliver-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    crate::platform::fs::Own::issue_at(&d)
}

fn put(o: &crate::platform::fs::Own, id: &str, now: i64) {
    q::put(
        o,
        &SessionQueuePutReq {
            machine: Machine::local(),
            sid: SID.parse().unwrap(),
            id: id.to_string(),
            end: "mobile".to_string(),
            n: None,
            at_ms: now,
            text: format!("text of {id}"),
            attachments: Vec::new(),
            rewind: None,
        },
        now,
    )
    .unwrap();
}

/// 队里那一条带的附件原样交给送字本体（拼进去的那一份照那一家的写法，见 `session_send_tests`）；端名合形状才当自报的前端。
#[test]
fn the_line_sent_carries_its_attachments() {
    let o = own("attach");
    q::put(
        &o,
        &SessionQueuePutReq {
            machine: Machine::local(),
            sid: SID.parse().unwrap(),
            id: "a".into(),
            end: "mobile".into(),
            n: None,
            at_ms: T0,
            text: "看图".into(),
            attachments: vec!["1027-1200-a3f2".into()],
            rewind: None,
        },
        T0,
    )
    .unwrap();
    let it = q::due(&o, SID, T0).unwrap().item.expect("排着");
    let say = say_of(SID.parse().unwrap(), &it, Some("mobile".into()));
    assert_eq!(
        (say.text.as_str(), say.attachments.as_slice()),
        ("看图", ["1027-1200-a3f2".to_string()].as_slice())
    );
}

/// 送字那一侧回 `result`（没被拒）⇒ 投递那一下的结局。
fn sent(result: SentResult) -> Delivery {
    Delivery::of(crate::control::terminals::input_reply(result, None, None))
}

/// 一台假的：此刻那条会话在干什么（`None` ＝ 不活）· 那一态从何时起 · 送字回什么；记下送了哪几句。
struct Rig {
    doing: RefCell<Option<Option<A>>>,
    since: RefCell<Option<u64>>,
    answer: RefCell<Delivery>,
    said: RefCell<Vec<String>>,
}

impl Rig {
    fn new(doing: Option<Option<A>>, since: i64) -> Self {
        Rig {
            doing: RefCell::new(doing),
            since: RefCell::new(Some(since as u64)),
            answer: RefCell::new(sent(SentResult::Delivered)),
            said: RefCell::new(Vec::new()),
        }
    }

    fn set(&self, doing: Option<Option<A>>, since: i64) {
        *self.doing.borrow_mut() = doing;
        *self.since.borrow_mut() = Some(since as u64);
    }

    /// 投递那一遍（这一台的假表 · 假送字）。
    fn pass(&self, o: &crate::platform::fs::Own, memos: &mut Memos, now: i64) -> Option<i64> {
        let look = |_: &str| Look {
            doing: *self.doing.borrow(),
            since_ms: *self.since.borrow(),
        };
        let send = |_: &str, it: &QueueItem| {
            self.said.borrow_mut().push(it.text.clone());
            Ok(self.answer.borrow().clone())
        };
        pass(
            &Deps {
                own: o,
                look: &look,
                send: &send,
            },
            memos,
            now,
        )
    }

    fn said(&self) -> Vec<String> {
        self.said.borrow().clone()
    }
}

fn states(o: &crate::platform::fs::Own) -> Vec<(String, S)> {
    q::read(o, SID, T0)
        .unwrap()
        .items
        .into_iter()
        .map(|i| (i.id, i.state))
        .collect()
}

#[test]
fn two_queued_while_a_turn_runs_go_one_per_turn() {
    let o = own("turns");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 5_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0 + 1);
    rig.pass(&o, &mut memos, T0 + 2);
    assert!(rig.said().is_empty(), "一轮在跑 ⇒ 一条都不送");

    rig.set(Some(Some(A::Idle)), T0 + 10);
    rig.pass(&o, &mut memos, T0 + 20);
    assert_eq!(rig.said(), vec!["text of a"], "那一轮完了先送第一条");
    rig.pass(&o, &mut memos, T0 + 30);
    assert_eq!(
        rig.said(),
        vec!["text of a"],
        "送出后那一态没换过 ⇒ 那一轮还没起来，不送第二条"
    );

    rig.set(Some(Some(A::Working)), T0 + 40);
    rig.pass(&o, &mut memos, T0 + 50);
    rig.set(Some(Some(A::Idle)), T0 + 60);
    rig.pass(&o, &mut memos, T0 + 70);
    assert_eq!(
        rig.said(),
        vec!["text of a", "text of b"],
        "那一轮又完了才送第二条"
    );
    assert!(states(&o).is_empty(), "送到的都摘掉");
}

#[test]
fn a_line_that_starts_no_turn_lets_the_next_go_after_the_confirm_window() {
    let o = own("confirm");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0);
    let wake = rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(rig.said(), vec!["text of a"]);
    assert_eq!(wake, Some(T0 + 1 + CONFIRM_MS), "送出后到确认期限醒一次");
    rig.pass(&o, &mut memos, T0 + CONFIRM_MS);
    assert_eq!(rig.said().len(), 1, "还没到期限");
    rig.pass(&o, &mut memos, T0 + 1 + CONFIRM_MS);
    assert_eq!(
        rig.said(),
        vec!["text of a", "text of b"],
        "过了期限那一态还没换 ⇒ 送下一条"
    );
}

#[test]
fn a_session_waiting_on_a_prompt_gets_nothing() {
    let o = own("waiting");
    let rig = Rig::new(Some(Some(A::NeedsYou)), T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    rig.set(Some(None), T0 + 2);
    rig.pass(&o, &mut memos, T0 + 3);
    assert!(rig.said().is_empty(), "在等人 · 说不清 ⇒ 不送");
    assert_eq!(states(&o), vec![("a".to_string(), S::Queued)]);
}

#[test]
fn a_session_that_ends_with_lines_left_marks_them_undelivered() {
    let o = own("ended");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    rig.set(None, T0 + 2);
    rig.pass(&o, &mut memos, T0 + 3);
    assert!(rig.said().is_empty(), "活过又不活 ⇒ 不去恢复它");
    assert_eq!(
        states(&o),
        vec![
            ("a".to_string(), S::Undelivered),
            ("b".to_string(), S::Undelivered)
        ]
    );
}

#[test]
fn a_line_for_a_session_not_seen_alive_is_sent_to_resume_it() {
    let o = own("resume");
    let rig = Rig::new(None, T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        rig.said(),
        vec!["text of a"],
        "没看见它活过 ⇒ 送（送字本体恢复它）"
    );
    rig.pass(&o, &mut memos, T0 + 2);
    assert_eq!(rig.said().len(), 1, "恢复起来之前不送第二条");
    assert_eq!(states(&o), vec![("b".to_string(), S::Queued)]);
}

#[test]
fn a_refused_line_is_marked_undelivered_and_the_next_one_goes() {
    let o = own("refused");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::No(Undeliverable::NotResumed);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        states(&o),
        vec![
            ("b".to_string(), S::Queued),
            ("a".to_string(), S::Undelivered)
        ]
    );
    *rig.answer.borrow_mut() = sent(SentResult::Delivered);
    rig.pass(&o, &mut memos, T0 + 2);
    assert_eq!(
        rig.said(),
        vec!["text of a", "text of b"],
        "被拒的那一条不挡后面的"
    );
}

#[test]
fn a_waiting_refusal_keeps_the_line_queued_and_unsure_is_not_resent() {
    let o = own("unsure");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        states(&o),
        vec![("a".to_string(), S::Queued)],
        "送的那一刻框开了 ⇒ 照排着"
    );
    *rig.answer.borrow_mut() = sent(SentResult::Unsure);
    rig.pass(&o, &mut memos, T0 + 2);
    assert!(states(&o).is_empty(), "不知道送没送到 ⇒ 摘掉、不重送");
}

#[test]
fn nothing_pending_means_no_wake_up() {
    let o = own("idle");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    let mut memos = Memos::default();
    assert_eq!(
        rig.pass(&o, &mut memos, T0),
        None,
        "队是空的 ⇒ 一直睡到有事"
    );
}

/// 编辑已发的那一句（带 `rewind`）投进队。
fn put_edit(o: &crate::platform::fs::Own, id: &str, when: cmd_wire::rewind::RewindWhen, now: i64) {
    q::put(
        o,
        &SessionQueuePutReq {
            machine: Machine::local(),
            sid: SID.parse().unwrap(),
            id: id.to_string(),
            end: "mobile".to_string(),
            n: None,
            at_ms: now,
            text: "改过的".to_string(),
            attachments: Vec::new(),
            rewind: Some(cmd_wire::rewind::Rewind {
                to: "u2".to_string(),
                how: cmd_wire::rewind::RewindHow::Conversation,
                when,
            }),
        },
        now,
    )
    .unwrap();
}

/// 在跑：等这一轮完的那一句照等；现在停的那一句当场送（停下这一轮是回退那一串的第一步）。
#[test]
fn an_edit_that_stops_now_is_sent_while_a_turn_runs_and_one_that_waits_is_not() {
    use cmd_wire::rewind::RewindWhen;
    let o = own("edit-now");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 1_000);
    let mut memos = Memos::default();
    put_edit(&o, "wait", RewindWhen::AfterTurn, T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert!(rig.said.borrow().is_empty(), "等这一轮完 ⇒ 在跑时不送");
    q::cancel(&o, SID, "wait", T0 + 2).unwrap();
    put_edit(&o, "now", RewindWhen::Now, T0 + 3);
    rig.pass(&o, &mut memos, T0 + 4);
    assert_eq!(*rig.said.borrow(), ["改过的"], "现在停 ⇒ 在跑也送");
}

/// 没退成 ⇒ 那一条标未送达、写上回退那一串说的为什么（不是笼统的「未送进会话」）。
#[test]
fn a_failed_rewind_is_undelivered_with_its_own_reason() {
    use cmd_wire::rewind::RewindWhen;
    let o = own("edit-no");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::No(Undeliverable::NotRewound("回退菜单对不上".into()));
    let mut memos = Memos::default();
    put_edit(&o, "e", RewindWhen::AfterTurn, T0);
    rig.pass(&o, &mut memos, T0 + 1);
    let list = q::read(&o, SID, T0 + 2).unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].state, S::Undelivered);
    assert_eq!(list.items[0].said, "回退菜单对不上");
    assert!(
        list.items[0].rewind.is_some(),
        "名单里那一条还带着怎么退（发件端据它重编辑）"
    );
}

/// 回退那一串的结局 ⇒ 投递的结局：成了 · 送进了没读回 ⇒ 摘掉；等 ⇒ 照排着；没退成 · 退了没送进 ⇒ 未送达。
#[test]
fn rewind_outcomes_map_to_deliveries() {
    use crate::control::rewind::{Walked, Why};
    assert_eq!(rewound(Walked::Done), Delivery::In { resumed: false });
    assert_eq!(
        rewound(Walked::Unsure(Why::NotReadBack)),
        Delivery::In { resumed: false }
    );
    assert_eq!(rewound(Walked::Wait), Delivery::Waiting);
    assert!(matches!(
        rewound(Walked::No(Why::MenuUnlike)),
        Delivery::No(Undeliverable::NotRewound(_))
    ));
    assert!(matches!(
        rewound(Walked::Unsure(Why::NotSent)),
        Delivery::No(Undeliverable::NotRewound(_))
    ));
}
