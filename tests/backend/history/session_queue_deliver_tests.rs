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
    put_line(o, id, "mobile", &format!("text of {id}"), now);
}

/// 发自 `end` 的一句（编号 · 正文照给）：人发的 · cc-bus 敲门 · 计划审面退回 · 斜杠命令各照那一家投进来的形状。
fn put_line(o: &crate::platform::fs::Own, id: &str, end: &str, text: &str, now: i64) {
    q::put(
        o,
        &SessionQueuePutReq {
            machine: Machine::local(),
            sid: SID.parse().unwrap(),
            id: id.to_string(),
            end: end.to_string(),
            n: None,
            at_ms: now,
            text: text.to_string(),
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

/// ★ 一轮在跑也送（agent 自己处理插进来的话），一次一条：送出后那一态没换过、没过确认期限 ⇒ 下一条先等（免得几句粘成一行）。
#[test]
fn two_queued_while_a_turn_runs_go_one_at_a_time() {
    let o = own("turns");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 5_000);
    let mut memos = Memos::default();
    put_line(&o, "a", "cc-bus", "text of a", T0);
    put_line(&o, "b", "monitor", "text of b", T0 + 1);
    rig.pass(&o, &mut memos, T0 + 2);
    assert_eq!(rig.said(), vec!["text of a"], "在跑 ⇒ 照送第一条");
    rig.pass(&o, &mut memos, T0 + 30);
    assert_eq!(
        rig.said(),
        vec!["text of a"],
        "送出后那一态没换过 ⇒ 第二条先等"
    );

    rig.set(Some(Some(A::Idle)), T0 + 40);
    rig.pass(&o, &mut memos, T0 + 50);
    assert_eq!(
        rig.said(),
        vec!["text of a", "text of b"],
        "那一态换过 ⇒ 送第二条"
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

/// ★ 屏上正有批准框 · 问答框 ⇒ 压着（送字带回车会被当成对框的回答）；框答完（在跑 · 闲着）就送。
#[test]
fn a_line_is_held_while_a_prompt_is_open_and_goes_when_it_closes() {
    let o = own("waiting");
    let rig = Rig::new(Some(Some(A::NeedsYou)), T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    put(&o, "b", T0);
    let said = || -> Vec<String> {
        q::read(&o, SID, T0)
            .unwrap()
            .items
            .iter()
            .map(|i| i.said.clone())
            .collect()
    };
    rig.pass(&o, &mut memos, T0 + 1);
    assert!(rig.said().is_empty(), "框开着 ⇒ 不送");
    assert_eq!(
        said(),
        vec![copy_core::copy_text("beQueue.said.waitBox", &[]); 2],
        "★ 框开着 ⇒ 名单那一句说等需手动答完，不写第几"
    );
    rig.set(Some(Some(A::Working)), T0 + 2);
    rig.pass(&o, &mut memos, T0 + 3);
    assert_eq!(rig.said(), vec!["text of a"], "框答完、接着跑 ⇒ 送");
    assert_eq!(
        said(),
        vec![copy_core::copy_text("beQueue.said.confirming", &[])],
        "框答完那一遍 ⇒ 换回「发送中 · 等确认」"
    );
}

/// ★ 说不清（冻住 · pidfile 没 `status` · 认不出的词）⇒ 照送，不停在排着。
#[test]
fn a_session_whose_state_cannot_be_told_gets_the_line() {
    let o = own("unclear");
    let rig = Rig::new(Some(None), T0 - 5_000);
    let mut memos = Memos::default();
    put_line(&o, "a", "cc-bus", "敲门", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(rig.said(), vec!["敲门"]);
    assert!(states(&o).is_empty());
}

/// ★ 来源不分：人发的 · cc-bus 投的 · 斜杠命令 · 计划审面退回，在跑时都送。
#[test]
fn every_source_is_sent_while_a_turn_runs() {
    for (tag, id, end, text) in [
        ("human", "monitor-r1", "monitor", "从桌面说的"),
        ("phone", "local-r2", "mobile", "从手机说的"),
        ("bus", "cc-bus-e1", "cc-bus", "cc-bus 敲门"),
        ("slash", "monitor-s1", "monitor", "/compact"),
        ("plan", "plan-1-sl-c1", "monitor", "计划退回"),
    ] {
        let o = own(tag);
        let rig = Rig::new(Some(Some(A::Working)), T0 - 5_000);
        let mut memos = Memos::default();
        put_line(&o, id, end, text, T0);
        rig.pass(&o, &mut memos, T0 + 1);
        assert_eq!(rig.said(), vec![text.to_string()], "{tag}：在跑 ⇒ 送");
    }
}

#[test]
fn a_session_that_ends_with_lines_left_marks_them_undelivered() {
    let o = own("ended");
    let rig = Rig::new(Some(Some(A::NeedsYou)), T0 - 1_000);
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
fn a_waiting_line_is_tried_again_when_its_retry_deadline_comes_with_no_other_event() {
    let o = own("retry");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put(&o, "a", T0);
    let wake = rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        wake,
        Some(T0 + 1 + WAITING_RETRY_MS),
        "人敲了半句没回车 ⇒ 不只等下一次表变动：到期限再醒一次"
    );
    // 人把半句删回空行、不回车：没有任何事件，只有那个期限叫醒。
    *rig.answer.borrow_mut() = sent(SentResult::Delivered);
    let at = T0 + 1 + WAITING_RETRY_MS;
    let wake = rig.pass(&o, &mut memos, at);
    assert_eq!(rig.said().len(), 2, "期限到了再试一次");
    assert!(states(&o).is_empty(), "这回送进去了 ⇒ 摘掉");
    assert_eq!(
        wake,
        Some(at + CONFIRM_MS),
        "送进去之后只剩等那一轮起来的期限，不再按等的期限醒"
    );
}

#[test]
fn a_retry_deadline_that_passes_while_a_prompt_is_open_does_not_keep_waking() {
    let o = own("retry-run");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    rig.set(Some(Some(A::NeedsYou)), T0 + 2);
    let at = T0 + 1 + WAITING_RETRY_MS;
    assert_eq!(
        rig.pass(&o, &mut memos, at),
        None,
        "到点那一遍框开着、不送 ⇒ 期限用掉了，睡到下一次事件（过了点的期限不一遍遍立刻叫醒）"
    );
    assert_eq!(rig.said().len(), 1);
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
fn a_line_an_old_hosted_terminal_cannot_take_is_undelivered_saying_so() {
    use crate::control::terminals::{input_reply, Refused};
    let o = own("outdated");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    let sent = input_reply(SentResult::Refused, Some(Refused::HostOutdated), None);
    *rig.answer.borrow_mut() = Delivery::of(sent);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    let list = q::read(&o, SID, T0 + 2).unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].state, S::Undelivered);
    assert_eq!(
        list.items[0].said,
        copy_core::copy_text("beTerminal.no.hostOutdated", &[]),
        "那一条写明是旧版托管终端、要重开"
    );
}

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
