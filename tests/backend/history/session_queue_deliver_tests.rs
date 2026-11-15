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
    /// 给了 ⇒ 送那一下回命令级错（不看 `answer`）。
    fail: RefCell<Option<Said>>,
    said: RefCell<Vec<String>>,
}

impl Rig {
    fn new(doing: Option<Option<A>>, since: i64) -> Self {
        Rig {
            doing: RefCell::new(doing),
            since: RefCell::new(Some(since as u64)),
            answer: RefCell::new(sent(SentResult::Delivered)),
            fail: RefCell::new(None),
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
            match self.fail.borrow().clone() {
                Some(f) => Err(f),
                None => Ok(self.answer.borrow().clone()),
            }
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

/// ★ 活会话表说在跑（只有后台子 agent 在跑、主 agent 空着时状态文件也写 busy）：两种「什么时候退」都交给回退那一串，
/// 在不在出字由它读屏判，投递这里不压。实测：后台子 agent 一直在跑的会话，「等这一轮完」那一句一直排着、名单一直写「发送中 · 等确认」。
#[test]
fn an_edit_is_handed_to_the_rewind_walk_whatever_the_table_says_about_the_turn() {
    use cmd_wire::rewind::RewindWhen;
    let o = own("edit-bg-busy");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 60_000);
    let mut memos = Memos::default();
    put_edit(&o, "wait", RewindWhen::AfterTurn, T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        *rig.said.borrow(),
        ["改过的"],
        "等这一轮完 ⇒ 交回退那一串读屏判，不凭活会话表压着"
    );
    put_edit(&o, "now", RewindWhen::Now, T0 + 3);
    rig.pass(&o, &mut memos, T0 + CONFIRM_MS + 4);
    assert_eq!(*rig.said.borrow(), ["改过的", "改过的"], "现在停 ⇒ 照送");
}

/// 回退那一串读屏见在出字、回「等」⇒ 那一条照排着，到点再交给它看一次。
#[test]
fn an_edit_the_walk_says_to_wait_on_stays_queued_and_is_tried_again() {
    use cmd_wire::rewind::RewindWhen;
    let o = own("edit-walk-wait");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 60_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put_edit(&o, "e", RewindWhen::AfterTurn, T0);
    let wake = rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(states(&o), [("e".to_string(), S::Queued)]);
    assert_eq!(wake, Some(T0 + 1 + WAITING_RETRY_MS));
    rig.pass(&o, &mut memos, T0 + 1 + WAITING_RETRY_MS);
    assert_eq!(rig.said().len(), 2, "到点再交回退那一串看一次");
}

/// ★ 回退那一串说「等」（屏上在出字）⇒ 名单上那一条照实说在等这一轮完，不说「发送中 · 等确认」；等过 [`q::TURN_LONG_MS`] ⇒
/// 改说那一轮久未结束（可撤了改选现在退），照排着不标未送达；屏上开着框 ⇒ 照旧说需手动。
#[test]
fn an_edit_waiting_on_the_turn_says_so_and_says_when_the_turn_runs_long() {
    use cmd_wire::rewind::RewindWhen;
    let o = own("edit-said");
    let rig = Rig::new(Some(Some(A::Working)), T0 - 60_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put_edit(&o, "e", RewindWhen::AfterTurn, T0);
    let said = |now: i64| q::read(&o, SID, now).unwrap().items[0].said.clone();
    assert_eq!(
        said(T0),
        copy_core::copy_text("beQueue.said.confirming", &[]),
        "还没交给回退那一串"
    );
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        said(T0 + 2),
        copy_core::copy_text("beQueue.said.waitTurn", &[])
    );
    let late = T0 + 1 + q::TURN_LONG_MS;
    rig.pass(&o, &mut memos, late - 1);
    assert_eq!(
        said(late),
        copy_core::copy_text("beQueue.said.waitTurn", &[]),
        "没到期限"
    );
    rig.pass(&o, &mut memos, late);
    assert_eq!(
        said(late + 1),
        copy_core::copy_text("beQueue.said.turnLong", &[])
    );
    assert_eq!(
        states(&o),
        [("e".to_string(), S::Queued)],
        "久了只改说法、照排着"
    );
    rig.set(Some(Some(A::NeedsYou)), late + 2);
    rig.pass(&o, &mut memos, late + 3);
    assert_eq!(
        said(late + 4),
        copy_core::copy_text("beQueue.said.waitBox", &[])
    );
}

/// 普通的一句送那一下回「等」（框开着）⇒ 不算在等这一轮，名单照旧。
#[test]
fn a_plain_line_told_to_wait_is_not_said_to_wait_for_the_turn() {
    let o = own("plain-wait");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 60_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        q::read(&o, SID, T0 + 2).unwrap().items[0].said,
        copy_core::copy_text("beQueue.said.confirming", &[])
    );
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

/// ★ 每一种送不进，名单那一条的 said 就是那一种自己那一句（`Undeliverable::said()`）：非空、不是兜底、看得出哪一步。
#[test]
fn every_refusal_lands_its_own_sentence_on_the_line() {
    use crate::control::terminals::{input_reply, Refused};
    let cases = [
        Undeliverable::Refused(input_reply(
            SentResult::Refused,
            Some(Refused::NotYours),
            None,
        )),
        Undeliverable::AttachmentGone(vec!["1027-1200-a3f2".into()]),
        Undeliverable::NotResumed,
        Undeliverable::Unseen,
        Undeliverable::NotRewound("回退菜单对不上".into()),
    ];
    for (i, u) in cases.iter().enumerate() {
        let o = own(&format!("why-{i}"));
        let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
        *rig.answer.borrow_mut() = Delivery::No(u.clone());
        let mut memos = Memos::default();
        put(&o, "a", T0);
        rig.pass(&o, &mut memos, T0 + 1);
        let list = q::read(&o, SID, T0 + 2).unwrap();
        assert_eq!(list.items[0].state, S::Undelivered, "{u:?}");
        let said = &list.items[0].said;
        assert_eq!(*said, u.said(), "{u:?} 名单那一句不是它自己那一句");
        assert!(!said.trim().is_empty(), "{u:?} 那一句是空的");
    }
}

/// ★ 送那一下回 `bad_args`（请求本身不合：`payload` 超长这一类，同一句每次必错）⇒ 照它那一句标未送达、不再送；
/// 别的命令级错（tmux 一时起不来）⇒ 照排着、下一次醒再试。
#[test]
fn a_malformed_request_is_given_up_with_its_reason_not_retried() {
    let o = own("malformed");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.fail.borrow_mut() = Some(Said {
        code: Some("bad_args".into()),
        said: "`payload` is too long (12736 > 8192)".into(),
        raw: None,
        data: None,
        written: None,
    });
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    rig.pass(&o, &mut memos, T0 + 2);
    assert_eq!(rig.said().len(), 1, "每次必错的那一句又送了一遍");
    let list = q::read(&o, SID, T0 + 3).unwrap();
    assert_eq!(list.items[0].state, S::Undelivered);
    assert!(
        list.items[0].said.contains("12736 > 8192"),
        "没带那一句原话：{}",
        list.items[0].said
    );

    let o = own("transient");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.fail.borrow_mut() = Some(Said {
        code: Some("tmux_unavailable".into()),
        said: "tmux 起不来".into(),
        raw: None,
        data: None,
        written: None,
    });
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(
        states(&o),
        vec![("a".to_string(), S::Queued)],
        "一时的错就照排着"
    );
}

// ---------------------------------------------------------------- 已交出去的那一条不再撤得了 · 撤的回包照实说

fn said_of(key: &str) -> String {
    copy_core::copy_text(key, &[])
}

/// ★ 投递认定要送、在锁里标成在送之后撤它 ⇒ 撤不成、那台说正在送；名单不列它（实测：送到一半点撤回，回「撤成了」，那一句照样送进去）。
#[test]
fn a_line_taken_for_sending_cannot_be_cancelled_and_is_not_listed() {
    let o = own("taken");
    put(&o, "a", T0);
    assert!(q::take(&o, SID, "a").unwrap(), "排着的那一条交得出来");
    assert!(
        q::read(&o, SID, T0 + 1).unwrap().items.is_empty(),
        "在送的不进名单"
    );
    let c = q::cancel(&o, SID, "a", T0 + 2).unwrap();
    assert!(!c.cancelled);
    assert_eq!(c.said, said_of("beQueue.cancel.sending"));
    assert!(!q::take(&o, SID, "a").unwrap(), "已在送的不再交一次");
}

/// ★ 送字的那一刻有人撤 ⇒ 撤没成，送完那一条进 `sent`。
#[test]
fn a_cancel_that_lands_while_the_line_is_being_sent_does_not_take_it_back() {
    let o = own("cancel-mid-send");
    put(&o, "a", T0);
    let got: RefCell<Option<cmd_wire::queue::QueueCancelled>> = RefCell::new(None);
    let look = |_: &str| Look {
        doing: Some(Some(A::Idle)),
        since_ms: Some((T0 - 1_000) as u64),
    };
    let send = |sid: &str, it: &QueueItem| {
        *got.borrow_mut() = Some(q::cancel(&o, sid, &it.id, T0 + 1).unwrap());
        Ok(sent(SentResult::Delivered))
    };
    let mut memos = Memos::default();
    pass(
        &Deps {
            own: &o,
            look: &look,
            send: &send,
        },
        &mut memos,
        T0 + 1,
    );
    let c = got.borrow().clone().expect("送的时候撤了一次");
    assert!(!c.cancelled, "送到一半撤 ⇒ 撤不成");
    let list = q::read(&o, SID, T0 + 2).unwrap();
    assert!(list.items.is_empty());
    assert_eq!(list.sent, ["a"]);
}

/// 送那一下回「等」· 命令级错 ⇒ 收回成排着：名单照列、撤得了。
#[test]
fn a_line_told_to_wait_or_failing_goes_back_to_queued_and_can_be_cancelled() {
    let o = own("untake-wait");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.answer.borrow_mut() = Delivery::Waiting;
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(states(&o), [("a".to_string(), S::Queued)]);
    let c = q::cancel(&o, SID, "a", T0 + 2).unwrap();
    assert!(c.cancelled);
    assert_eq!(c.said, said_of("beQueue.cancel.done"));

    let o = own("untake-fail");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    *rig.fail.borrow_mut() = Some(Said {
        code: Some("tmux_unavailable".into()),
        said: "tmux 起不来".into(),
        raw: None,
        data: None,
        written: None,
    });
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    assert_eq!(states(&o), [("a".to_string(), S::Queued)]);
    assert!(q::cancel(&o, SID, "a", T0 + 2).unwrap().cancelled);
}

/// 盘上留着在送的那一条（进程在送时没了）⇒ 投递起来时当已送：进 `sent`、不重送。
#[test]
fn a_line_left_sending_by_a_dead_process_counts_as_sent() {
    let o = own("left-sending");
    put(&o, "a", T0);
    put(&o, "b", T0);
    assert!(q::take(&o, SID, "a").unwrap());
    q::settle_sending(&o, SID).unwrap();
    let list = q::read(&o, SID, T0 + 1).unwrap();
    assert_eq!(list.sent, ["a"]);
    assert_eq!(
        list.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        ["b"]
    );
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    let mut memos = Memos::default();
    rig.pass(&o, &mut memos, T0 + 2);
    assert_eq!(rig.said(), ["text of b"], "在送过的那一条不重送");
}

/// ★ 先撤后到：撤一个没见过的 id ⇒ 记作撤过；之后同 id 投进来 ⇒ 回 `Seen`、说已取消，不入队、不送。
#[test]
fn a_cancel_that_arrives_before_its_line_keeps_the_line_out() {
    use cmd_wire::queue::QueuePutOutcome;
    let o = own("cancel-first");
    let c = q::cancel(&o, SID, "a", T0).unwrap();
    assert!(c.cancelled, "这一句不会再送进会话");
    assert_eq!(c.said, said_of("beQueue.cancel.done"));
    let got = q::put(
        &o,
        &SessionQueuePutReq {
            machine: Machine::local(),
            sid: SID.parse().unwrap(),
            id: "a".into(),
            end: "mobile".into(),
            n: None,
            at_ms: T0,
            text: "晚到的".into(),
            attachments: Vec::new(),
            rewind: None,
        },
        T0 + 1,
    )
    .unwrap();
    assert_eq!(got.outcome, QueuePutOutcome::Seen);
    assert_eq!(got.said, said_of("reason.op.cancelled"));
    assert!(q::read(&o, SID, T0 + 2).unwrap().items.is_empty());
}

/// 撤一个已送进会话的 ⇒ 撤不成、那台说已送进会话 · 撤不回。
#[test]
fn cancelling_a_sent_line_says_it_is_already_in() {
    let o = own("cancel-sent");
    let rig = Rig::new(Some(Some(A::Idle)), T0 - 1_000);
    let mut memos = Memos::default();
    put(&o, "a", T0);
    rig.pass(&o, &mut memos, T0 + 1);
    let c = q::cancel(&o, SID, "a", T0 + 2).unwrap();
    assert!(!c.cancelled);
    assert_eq!(c.said, said_of("beQueue.cancel.sent"));
}
