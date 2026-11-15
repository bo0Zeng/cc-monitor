//! 待答登记表：挂上 · 第一个答案落下、其余回「别处已答」那一种结局（`gone` ＋ 那一句）· 连接断了撤掉 · 子 agent 的请求照样挂、带 agent_id · needs 配上 ask。
//! 登记表是进程里一份，各条用各自的会话 id，并行跑互不相扰。夹具正文都是编的。

use crate::agents::asks::{product_of, Ask, Asked, NoteNeed};
use crate::agents::AskIn;
use crate::agents::{AskKind, AskOption, AskQuestion, AskSuggestion};
use crate::control::asks::*;
use crate::history::history_annotations::AUTO_MAX_MIN;
use cmd_wire::needs::{AnswerReq, AskEnd, AutoAnswer, AutoChoice};
use copy_core::copy_text;
use serde_json::json;
use serde_json::Value;

/// 一份需手动答的那一格摊开：答得了 ⇒ `(抬头, 翻页抬头, None)`；答不了 ⇒ `(None, None, 那一句)`。
fn reply_of(
    n: &cmd_wire::needs::Needs,
) -> (
    Option<&cmd_wire::cells::Words>,
    Option<&cmd_wire::cells::Words>,
    Option<&cmd_wire::cells::Words>,
) {
    match &n.reply {
        cmd_wire::needs::NeedsReply::Ask { head, page, .. } => (Some(head), page.as_ref(), None),
        cmd_wire::needs::NeedsReply::Manual(said) => (None, None, Some(said)),
    }
}

/// 线上那一份入参照分派那一层解（`typed::take`，`deny_unknown_fields`）再交本体；回包投成线上那一份。
/// 入参形状不对 ⇒ `bad_args`（与本体回的码同一形，判据只比码）。
fn answer(args: &Value) -> Result<Value, crate::common::said::Said> {
    let req: AnswerReq = crate::wire::typed::take(args)
        .map_err(|f| crate::common::said::Said::new("bad_args", f.said))?;
    crate::control::asks::answer_needs(req)
        .map(|a| serde_json::to_value(a).expect("回包总能序列化"))
}

const AGENT: &str = "claude";

/// 那一家（claude）的答题口：直接挂上的那几条带着它。
fn claude() -> crate::agents::AskFace {
    crate::agents::ask_face(AGENT).expect("claude 有答题口")
}

fn approve(sid: &str) -> AskIn {
    AskIn {
        sid: sid.into(),
        kind: AskKind::Approve,
        tool: "Bash".into(),
        what: Some("touch a".into()),
        detail: "touch a\necho b".into(),
        suggestions: vec![AskSuggestion {
            scope: crate::agents::SuggestScope::Session,
            rule: crate::agents::SuggestRule::Dirs(vec!["/w".into()]),
            raw: json!({"type": "addDirectories", "directories": ["/w"], "destination": "session"}),
        }],
        questions: vec![],
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input: json!({"command": "touch a\necho b"}),
    }
}

fn ids(a: &Ask) -> Vec<&str> {
    a.choices.iter().map(|c| c.id.as_str()).collect()
}

/// 别处先答了 / 那一头撤了的那一份回包：结局 `gone` ＋ 后端那一句。
fn gone(said: &str) -> Value {
    json!({"result": "gone", "said": said})
}

/// 上一条请求收场的那一格（`said` 后端写好 · `stays` 切走再回来还留不留着）。
fn end(said: &str, stays: bool) -> AskEnd {
    AskEnd {
        said: cmd_wire::cells::Words(said.into()),
        stays,
    }
}

fn answer_of(sid: &str, ask: &str, choice: &str) -> Result<Value, crate::common::said::Said> {
    answer(&json!({"sid": sid, "ask": ask, "choice": choice}))
}

#[test]
fn approval_choices_are_written_and_the_never_ask_again_one_is_not_first() {
    let a = product_of("x", &approve("t-choices"), true, false, None);
    assert_eq!(
        ids(&a),
        ["allow", "always-0", "deny", "deny-note", "deny-stop"]
    );
    assert!(a.choices[1].text.0.contains("/w"), "规则写成人话");
    assert_eq!(a.choices[3].note, NoteNeed::Required);
    assert_eq!(a.detail, "touch a\necho b");
    assert_eq!(a.kind, AskKind::Approve);
}

#[test]
fn plan_and_question_choices() {
    let mut p = approve("t-plan");
    p.kind = AskKind::Plan;
    p.suggestions.clear();
    assert_eq!(
        ids(&product_of("x", &p, true, false, None)),
        ["plan-auto", "plan-edits", "plan-manual", "plan-keep"]
    );
    let mut q = approve("t-q");
    q.kind = AskKind::Answer;
    q.suggestions.clear();
    assert_eq!(
        ids(&product_of("x", &q, true, false, None)),
        ["dismiss"],
        "没有题 ⇒ 答不了，只能不回答"
    );
    q.questions = vec![AskQuestion {
        header: None,
        question: "Which?".into(),
        multi: true,
        options: vec![
            AskOption {
                label: "A".into(),
                description: None,
            },
            AskOption {
                label: "B".into(),
                description: None,
            },
        ],
    }];
    let pq = product_of("x", &q, true, false, None);
    assert_eq!(ids(&pq), ["answer", "dismiss"]);
    assert_eq!(pq.questions[0].id, "q0");
    assert_eq!(pq.questions[0].options[1].id, "q0o1");
    assert!(pq.questions[0].other);
    // 答题块抬头那一句在配请求那一下写好：提问那一种带题数；别的种类就是钉条第一行。
    let n = crate::observe::recognize::facts::dress_ask_with(
        None,
        Some(Asked {
            ask: pq,
            since_ms: 0,
        }),
        0,
    )
    .unwrap();
    // 一屏一题的那一端（手机抽屉）：抬头带「第几题 / 共几题」，共几题由这里写，第几题那个空留给出口填（它翻到哪一页是它自己的事）。
    assert_eq!(
        reply_of(&n).1.map(|w| w.0.clone()),
        Some(copy_core::copy_text(
            "mobAsk.head.page",
            &[("head", &n.head.0), ("i", "{i}"), ("n", "1")]
        ))
    );
    assert_eq!(
        reply_of(&n).0.map(|w| w.0.clone()),
        Some(copy_core::copy_text(
            "askBlock.head.questions",
            &[("head", &n.head.0), ("n", "1")]
        ))
    );
    let n = crate::observe::recognize::facts::dress_ask_with(
        None,
        Some(Asked {
            ask: product_of("x", &approve("t-a"), true, false, None),
            since_ms: 0,
        }),
        0,
    )
    .unwrap();
    assert_eq!(reply_of(&n).0, Some(&n.head));
    assert!(reply_of(&n).1.is_none(), "不是提问那一种 ⇒ 没有翻页的抬头");
}

#[test]
fn the_first_answer_lands_and_the_rest_are_gone() {
    let sid = "t-first";
    let mut rx = open_id(claude(), approve(sid), 9001).1;
    let a = pending(sid).pop().map(|a| a.ask).expect("挂上了");
    assert_eq!(
        answer_of(sid, &a.id, "always-0").unwrap(),
        json!({"result": "answered"})
    );
    let got = rx.try_recv().expect("hook 那头收到").expect("有决定");
    assert_eq!(
        got["hookSpecificOutput"]["decision"],
        json!({"behavior": "allow", "updatedPermissions": [
            {"type": "addDirectories", "directories": ["/w"], "destination": "session"}
        ]})
    );
    assert_eq!(
        answer_of(sid, &a.id, "deny").unwrap(),
        gone(&copy_text("beAsk.answer.already", &[])),
        "别处先答了是一种结局，不是错"
    );
    assert!(pending(sid).is_empty(), "答掉 ⇒ 不再挂着");
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("beAsk.answer.already", &[]), false)),
        "别的出口看到的那一句后端写好"
    );
}

#[test]
fn when_the_hook_side_goes_away_the_request_is_withdrawn_and_later_answers_are_gone() {
    let sid = "t-gone";
    let desk = Desk::new();
    let _rx = open_id(claude(), approve(sid), desk.conn()).1;
    let a = pending(sid).pop().map(|a| a.ask).unwrap();
    drop(desk); // hook 那条连接断了（终端先答了 · 被打断 · 到点）
    assert!(pending(sid).is_empty());
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false)),
        "别处先答了 ⇒ 会话事实里有写好的那一句"
    );
    assert_eq!(
        answer_of(sid, &a.id, "allow").unwrap(),
        gone(&copy_text("askBlock.gone.elsewhere", &[]))
    );
    assert_eq!(
        answer_of(sid, "ask-nope", "allow")
            .unwrap_err()
            .code
            .as_deref(),
        Some("not_found")
    );
}

#[test]
fn a_dead_receiver_is_gone_not_answered() {
    let sid = "t-deadrx";
    drop(open_id(claude(), approve(sid), 9002).1);
    let a = pending(sid).pop().map(|a| a.ask).unwrap();
    assert_eq!(
        answer_of(sid, &a.id, "allow").unwrap(),
        gone(&copy_text("askBlock.gone.elsewhere", &[]))
    );
}

#[test]
fn a_sub_agent_request_lands_in_needs_carries_its_id_and_is_answered() {
    let sid = "t-sub";
    let mut a = approve(sid);
    a.agent_id = Some("agent-7".into());
    a.agent_type = Some("general-purpose".into());
    let mut rx = open_id(claude(), a, 9003).1;
    // pidfile 不说在等 ⇒ 照挂着的请求补出一份 needs。
    let needs =
        crate::observe::recognize::facts::dress_ask_with(None, pending(sid).into_iter().next(), 0)
            .expect("有挂着的就有 needs");
    let ask = needs.ask().cloned().expect("带 ask");
    assert_eq!(
        needs.kind,
        crate::observe::recognize::facts::NeedsKind::Approve
    );
    assert_eq!(needs.tool.as_deref(), Some("Bash"));
    assert_eq!(ask.run.as_ref().map(|r| r.id.as_str()), Some("agent-7"));
    assert_eq!(
        ask.run
            .as_ref()
            .map(|r| r.text.0.contains("general-purpose")),
        Some(true),
        "谁要批那一句带着类别"
    );
    assert_eq!(
        answer_of(sid, &ask.id, "allow").unwrap(),
        json!({"result": "answered"})
    );
    let got = rx.try_recv().unwrap().unwrap();
    assert_eq!(got["hookSpecificOutput"]["decision"]["behavior"], "allow");
}

#[test]
fn notes_and_picks_are_checked_before_anything_is_delivered() {
    let sid = "t-check";
    let mut rx = open_id(claude(), approve(sid), 9005).1;
    let id = pending(sid).pop().map(|a| a.ask).unwrap().id;
    assert_eq!(
        answer_of(sid, &id, "deny-note")
            .unwrap_err()
            .code
            .as_deref(),
        Some("note_required")
    );
    assert_eq!(
        answer_of(sid, &id, "always-3").unwrap_err().code.as_deref(),
        Some("bad_choice")
    );
    assert_eq!(
        answer_of("t-other", &id, "allow")
            .unwrap_err()
            .code
            .as_deref(),
        Some("not_found"),
        "会话对不上"
    );
    assert!(rx.try_recv().is_err(), "没交出去");
    answer(&json!({"sid": sid, "ask": id, "choice": "deny-note", "note": "use b"})).unwrap();
    assert_eq!(
        rx.try_recv().unwrap().unwrap()["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "use b"})
    );

    let qsid = "t-picks";
    let mut q = approve(qsid);
    q.kind = AskKind::Answer;
    q.suggestions.clear();
    q.input = json!({"questions": []});
    q.questions = vec![
        AskQuestion {
            header: None,
            question: "One?".into(),
            multi: false,
            options: vec![
                AskOption {
                    label: "A".into(),
                    description: None,
                },
                AskOption {
                    label: "B".into(),
                    description: None,
                },
            ],
        },
        AskQuestion {
            header: None,
            question: "Many?".into(),
            multi: true,
            options: vec![
                AskOption {
                    label: "X".into(),
                    description: None,
                },
                AskOption {
                    label: "Y".into(),
                    description: None,
                },
            ],
        },
    ];
    let mut rx = open_id(claude(), q, 9006).1;
    let id = pending(qsid).pop().map(|a| a.ask).unwrap().id;
    let try_picks =
        |p: Value| answer(&json!({"sid": qsid, "ask": id, "choice": "answer", "picks": p}));
    assert_eq!(
        try_picks(json!({"q0": {"options": ["q0o0"]}}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args"),
        "每题都要答"
    );
    assert_eq!(
        try_picks(json!({"q0": {"options": ["q0o0", "q0o1"]}, "q1": {"options": ["q1o0"]}}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args"),
        "单选至多一项"
    );
    try_picks(json!({"q0": {"text": "free"}, "q1": {"options": ["q1o0", "q1o1"]}})).unwrap();
    assert_eq!(
        rx.try_recv().unwrap().unwrap()["hookSpecificOutput"]["decision"]["updatedInput"]
            ["answers"],
        json!({"One?": "free", "Many?": "X, Y"})
    );
}

#[test]
fn needs_without_a_pending_request_say_only_in_the_terminal() {
    let n = crate::observe::recognize::facts::needs_of(
        Some(crate::agents::claudecode::AGENT_KIND),
        &[],
        Some(&crate::observe::recognize::facts::PidWait {
            waiting_for: Some(crate::agents::WaitOn::Dialog),
            since_ms: None,
            read_at_ms: 0,
        }),
    );
    let n = crate::observe::recognize::facts::dress_ask_with(n, None, 0).unwrap();
    assert!(n.ask().is_none());
    assert!(reply_of(&n).0.is_none(), "没有挂着的请求 ⇒ 没有答题块抬头");
    assert_eq!(
        reply_of(&n).2.map(|w| w.0.clone()),
        Some(copy_core::copy_text("beAsk.manual.terminal", &[]))
    );
    assert!(crate::observe::recognize::facts::dress_ask_with(None, None, 0).is_none());
}

/// ★ 挂着的每一条带它自己挂上的那一刻（这台的钟），按挂上的先后交；照它补出的那一份需手动从最早那一条起等、已等多久照它减。
#[test]
fn each_hung_ask_carries_its_own_hang_time_in_the_order_hung() {
    use crate::common::time::now_ms;
    let sid = "t-fake-since";
    let face = crate::agents::fake::ASK;
    let fake = |sid: &str| AskIn {
        tool: "Run".into(),
        ..approve(sid)
    };
    let after = |t: u64| {
        while now_ms() <= t {
            std::hint::spin_loop();
        }
    };
    let desk = Desk::new();
    let t0 = now_ms();
    let (first, _rx1) = open_id(face, fake(sid), desk.conn());
    let t1 = now_ms();
    after(t1);
    let t2 = now_ms();
    let (second, _rx2) = open_id(face, fake(sid), desk.conn());
    let t3 = now_ms();

    let p = pending(sid);
    assert_eq!(
        p.iter().map(|a| a.ask.id.as_str()).collect::<Vec<_>>(),
        [first.as_str(), second.as_str()],
        "按挂上的先后"
    );
    assert!(
        (t0..=t1).contains(&p[0].since_ms) && (t2..=t3).contains(&p[1].since_ms),
        "各带自己挂上的那一刻：{} 在 {t0}..={t1} · {} 在 {t2}..={t3}",
        p[0].since_ms,
        p[1].since_ms
    );

    let before = now_ms();
    let w = crate::faces::read_face::dress_ask(None, sid, None).expect("照挂着的补出一份");
    let after_ms = now_ms();
    assert_eq!(w.ask().map(|a| a.id.as_str()), Some(first.as_str()));
    assert_eq!(w.since_ms, Some(p[0].since_ms), "从最早那一条挂上起等");
    let waited = w.waited_ms.expect("已等多久");
    assert!(
        (before - p[0].since_ms..=after_ms - p[0].since_ms).contains(&waited),
        "已等多久照最早那一条挂上的那一刻减：{waited}"
    );
    drop(desk);
    assert!(pending(sid).is_empty());
}

#[tokio::test]
async fn an_agent_without_an_answer_door_gets_no_decision_at_once() {
    let v = ask(
        9007,
        crate::wire::typed::take(&json!({"agent": "nobody", "hook": {}})).unwrap(),
        |_| None,
    )
    .await;
    assert_eq!(serde_json::to_value(v).unwrap(), json!({"decision": null}));
    assert_eq!(
        crate::wire::typed::take::<AskReq>(&json!({"hook": {}}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 「终端那边答了」只此一判（hook 挂着等的那一种 · 常驻记下的挡着终端的那一种都照它）。
#[test]
fn the_terminal_answered_is_judged_by_waiting_seen_and_left_or_a_later_update() {
    use crate::agents::SessionActivity as A;
    use hook::{closed_now, Seen};
    let started = 1000;
    let mut seen = Seen::default();
    assert!(!closed_now(&[], started, &mut seen), "还没见过进程：不判");
    assert!(
        !closed_now(&[(Some(A::Working), Some(900))], started, &mut seen),
        "状态是旧的：没关"
    );
    assert!(
        !closed_now(&[(Some(A::NeedsYou), Some(1001))], started, &mut seen),
        "框出来了"
    );
    assert!(
        closed_now(&[(Some(A::Working), Some(900))], started, &mut seen),
        "离开在等 ⇒ 关了"
    );

    // 框出来又答掉落在同一阵动静里（没见过在等）：状态更新晚于记下的那一刻、且不在等 ⇒ 关了。
    let mut seen = Seen::default();
    assert!(closed_now(
        &[(Some(A::Working), Some(1500))],
        started,
        &mut seen
    ));

    // 会话没了：见过进程 ⇒ 关了（框随它没了）。
    let mut seen = Seen::default();
    assert!(!closed_now(&[(Some(A::Idle), None)], started, &mut seen));
    assert!(closed_now(&[], started, &mut seen));

    // 还有一个进程在等 ⇒ 没关。
    let mut seen = Seen::default();
    assert!(!closed_now(
        &[(Some(A::NeedsYou), None), (Some(A::Working), Some(2000))],
        started,
        &mut seen
    ));
}

/// hook 认「是不是 ccm 起的那一个会话」：只有经环境那一路来的 hook（自己的环境里那个变量带着我们的插件目录）才要认 ——
/// `--settings` 那一路不会传给子进程。认法（常驻那一侧）：先看常驻那份表（发问那个进程有 ccm 留的便条 · 会话 id 记在起会话那份记录里），
/// 再看发问那个进程起来时的环境：带着那一家的嵌套标记 ⇒ 是会话里 Bash 起的子进程；读不出 ⇒ 认不出。认不出 ⇒ 不挂、不给决定。
#[test]
fn a_hook_that_came_by_the_environment_answers_only_for_the_session_ccm_started() {
    use crate::platform::proc::EnvRead;
    let ours = "/h/.cc-monitor/answer-plugin";
    // `--settings` 那一路（环境里没有我们的目录）：不用认。
    assert!(!hook::from_env(None, ours));
    assert!(!hook::from_env(Some("/u/mine"), ours));
    assert!(hook::from_env(
        Some("/u/mine:/h/.cc-monitor/answer-plugin"),
        ours
    ));
    // 经环境来的：表里有 ⇒ 是。
    assert!(hook::admitted(true, EnvRead::Unreadable));
    // 表里没有：发问那个进程起来时没有嵌套标记 ⇒ 是（ccm 起会话前清掉了它）；带着 ⇒ 子进程；读不出 ⇒ 认不出。
    assert!(hook::admitted(false, EnvRead::Unset));
    assert!(!hook::admitted(false, EnvRead::Value("1".into())));
    assert!(!hook::admitted(false, EnvRead::Unreadable));
}

// ───────────────────────── 无人答时自动答 ─────────────────────────
// 假时钟（tokio 停着的钟，`advance` 才走）；hook 那一份照那一家的形状编（不起真的会话）。

/// 那一家（claude）的 hook 交来的那一份：`tool` 是工具名，提问那一种带一道题。
fn hook_of(sid: &str, tool: &str) -> AskReq {
    let input = if tool == "AskUserQuestion" {
        json!({"questions": [{"question": "Which?", "header": "h", "multiSelect": false,
            "options": [{"label": "A", "description": "d"}]}]})
    } else {
        json!({"command": "touch a", "plan": "p"})
    };
    crate::wire::typed::take(&json!({"agent": AGENT, "hook": {
        "hook_event_name": "PermissionRequest", "session_id": sid, "tool_name": tool, "tool_input": input,
    }}))
    .unwrap()
}

fn on(min: u32, say: Option<&str>) -> Option<AutoAnswer> {
    Some(AutoAnswer {
        after_min: min,
        say: say.map(str::to_string),
    })
}

/// 挂上（在别的任务里等），等它真挂上了（或已收场）再交回。
async fn hang(
    conn: u64,
    req: AskReq,
    setting: Option<AutoAnswer>,
) -> tokio::task::JoinHandle<AskReply> {
    let sid = req.hook["session_id"].as_str().unwrap().to_string();
    let h = tokio::spawn(ask(conn, req, move |_| setting));
    // 挂上了，或已经收场（到点即答的那种：不在这里空转）。
    while pending(&sid).is_empty() && !h.is_finished() {
        tokio::task::yield_now().await;
    }
    h
}

async fn pass(min: u64, secs: u64) {
    tokio::time::advance(std::time::Duration::from_secs(min * 60 + secs)).await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

/// 同一组 hook 交来的一轮的那几种：当场回不给决定（hook 一个字节不打），活会话表记上那一件；不挂请求。
#[tokio::test]
async fn a_turn_hook_is_heard_by_the_live_table_and_gets_no_decision_at_once() {
    let sid = "t-turn-heard";
    let mut bell = crate::history::needs_changed::moved_rx();
    let stop = json!({"agent": "claude", "hook": {"session_id": sid, "hook_event_name": "Stop"}});
    let v = ask(9008, crate::wire::typed::take(&stop).unwrap(), |_| None).await;
    assert_eq!(serde_json::to_value(v).unwrap(), json!({"decision": null}));
    let rang = rang_for(&mut bell, sid);
    assert!(rang, "活会话表收下（`live_table::hear` 敲这条会话的铃）");
    assert_eq!(
        crate::history::live_table::heard_of(sid).map(|h| h.event),
        Some(crate::agents::AgentEvent::TurnEnded)
    );
    assert!(!crate::history::needs_changed::asked_sids().contains(sid));
}

/// 挂着请求的会话：活会话表那一侧（`history::needs_changed::asked_sids`）读到的就是登记表此刻那几条 —— 挂上就在、连接断了就不在。
#[tokio::test]
async fn the_live_table_sees_the_sessions_that_have_an_ask_hanging() {
    let sid = "t-asked-seen";
    let desk = Desk::new();
    let h = hang(desk.conn(), hook_of(sid, "Bash"), None).await;
    assert!(crate::history::needs_changed::asked_sids().contains(sid));
    drop(desk);
    assert_eq!(h.await.unwrap().decision, None);
    assert!(!crate::history::needs_changed::asked_sids().contains(sid));
}

#[tokio::test(start_paused = true)]
async fn a_question_nobody_answers_gets_the_sentence_when_the_time_is_up() {
    let sid = "t-auto-due";
    let h = hang(
        9101,
        hook_of(sid, "AskUserQuestion"),
        on(10, Some("照推荐的来")),
    )
    .await;
    pass(9, 59).await;
    assert!(!h.is_finished(), "没到点不答");
    assert_eq!(pending(sid).len(), 1);
    let id = pending(sid)[0].ask.id.clone();
    pass(0, 2).await;
    assert!(h.is_finished(), "到点替人答了");
    let got = h.await.unwrap().decision.expect("有决定");
    let d = &got["hookSpecificOutput"]["decision"];
    assert_eq!(d["behavior"], "allow");
    assert_eq!(
        d["updatedInput"]["answers"],
        json!({"Which?": "照推荐的来"}),
        "每一题答成「其他」＋ 那一句"
    );
    assert!(pending(sid).is_empty());
    let said = copy_core::copy_text(
        "beAsk.auto.answered",
        &[
            ("dur", &copy_core::format_duration(600_000)),
            ("say", "照推荐的来"),
        ],
    );
    assert_eq!(ended(sid), Some(end(&said, true)), "替人答的那一句留着");
    assert!(ended_rows()
        .iter()
        .any(|r| r.sid == sid && r.end == end(&said, true)));
    // 之后桌面 · 手机再答：已有人答过，那一句是记录那一句。
    assert_eq!(answer_of(sid, &id, "dismiss").unwrap(), gone(&said));
    // 下一次挂上提问 ⇒ 那条记录换掉。
    let _rx = open_id(claude(), approve(sid), 9102).1;
    assert_eq!(ended(sid), None);
}

#[tokio::test(start_paused = true)]
async fn without_its_own_sentence_it_says_the_default_one() {
    let sid = "t-auto-default";
    let h = hang(9103, hook_of(sid, "AskUserQuestion"), on(1, None)).await;
    pass(1, 1).await;
    assert!(h.is_finished(), "到点替人答了");
    let got = h.await.unwrap().decision.unwrap();
    assert_eq!(
        got["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Which?"],
        auto_say()
    );
}

#[tokio::test(start_paused = true)]
async fn a_human_who_answers_first_wins_and_nothing_is_said_for_them() {
    let sid = "t-auto-human";
    let h = hang(9104, hook_of(sid, "AskUserQuestion"), on(5, None)).await;
    pass(4, 0).await;
    let a = pending(sid).pop().map(|a| a.ask).unwrap();
    let q = &a.questions[0];
    answer(&json!({"sid": sid, "ask": a.id, "choice": "answer",
        "picks": {q.id.clone(): {"options": [q.options[0].id.clone()]}}}))
    .unwrap();
    pass(10, 0).await;
    let got = h.await.unwrap().decision.unwrap();
    assert_eq!(
        got["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Which?"], "A",
        "人的答案"
    );
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("beAsk.answer.already", &[]), false)),
        "人答了 ⇒ 别的出口照「已有人答过」那一句，不留着"
    );
}

#[tokio::test(start_paused = true)]
async fn the_terminal_answering_first_means_no_decision_and_no_record() {
    let sid = "t-auto-term";
    let desk = Desk::new();
    let h = hang(desk.conn(), hook_of(sid, "AskUserQuestion"), on(5, None)).await;
    drop(desk); // 终端那边先答了：hook 退了、连接断了
    pass(6, 0).await;
    assert_eq!(h.await.unwrap().decision, None);
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false)),
        "终端先答了 ⇒ 那一句后端写好，不替人答"
    );
}

#[tokio::test(start_paused = true)]
async fn approvals_and_plans_are_never_answered_for_anyone() {
    for (i, tool) in ["Bash", "ExitPlanMode"].into_iter().enumerate() {
        let sid = format!("t-auto-gate-{i}");
        let desk = Desk::new();
        let h = hang(desk.conn(), hook_of(&sid, tool), on(1, None)).await;
        pass(AUTO_MAX_MIN.into(), 1).await;
        assert!(!h.is_finished(), "{tool}：安全闸不自动答");
        assert_eq!(pending(&sid).len(), 1, "{tool}");
        assert_eq!(ended(&sid), None, "{tool}：还挂着 ⇒ 没有收场那一句");
        drop(desk);
        assert_eq!(h.await.unwrap().decision, None);
    }
}

#[tokio::test(start_paused = true)]
async fn with_the_setting_off_a_question_just_waits() {
    for (i, setting) in [None, on(0, Some("x"))].into_iter().enumerate() {
        let sid = format!("t-auto-off-{i}");
        let desk = Desk::new();
        let h = hang(desk.conn(), hook_of(&sid, "AskUserQuestion"), setting).await;
        pass(AUTO_MAX_MIN.into(), 1).await;
        assert!(!h.is_finished(), "设置关着不答（第 {i} 种）");
        drop(desk);
        assert_eq!(h.await.unwrap().decision, None);
        assert_eq!(
            ended(&sid),
            Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false))
        );
    }
}

/// ★ 替人答的缺省那一句不预设有推荐项：有推荐 · 没推荐两种都说得通（选项都没标推荐时也对得上）。
#[test]
fn the_default_auto_answer_holds_with_or_without_a_recommended_option() {
    let say = auto_say();
    assert!(say.contains("有推荐就"), "{say}");
    assert!(say.contains("无推荐就"), "{say}");
    assert!(!say.contains("你"), "缺省那一句用了第二人称：{say}");
}

/// ★ 自动答可选的几档随会话事实交（档位与字一起）：关 · 5m · 15m · 30m；此刻那一格不在几档里（命令行写的 7 分钟）⇒ 多列一项、按分钟排好。
#[test]
fn auto_answer_choices_carry_the_current_minute_in_order_with_text() {
    let mins = |c: &[AutoChoice]| c.iter().map(|c| c.min).collect::<Vec<_>>();
    let texts = |c: &[AutoChoice]| c.iter().map(|c| c.text.0.clone()).collect::<Vec<_>>();
    let short = |m: u64| copy_core::short_duration(m * 60_000);

    let seven = auto_choices(Some(7));
    assert_eq!(mins(&seven), [0, 5, 7, 15, 30]);
    assert_eq!(
        texts(&seven),
        [
            copy_text("beAsk.auto.off", &[]),
            short(5),
            short(7),
            short(15),
            short(30)
        ]
    );

    for now in [None, Some(0), Some(5), Some(30)] {
        assert_eq!(
            mins(&auto_choices(now)),
            [0, 5, 15, 30],
            "此刻 {now:?} 在几档里，不多列"
        );
    }
    assert!(
        auto_choices(None).iter().all(|c| !c.text.0.is_empty()),
        "每一档都带写好的字"
    );
}

// ───────────────────────── 一家声明的回包 · 答不了的那一种（替身家） ─────────────────────────
// 替身家（`agents::fake::ASK`）：允许回 `{decision: "allow"}`、拒绝回 `{decision: "deny"}`；提问那一种它不给经钩子答。
// 通用层只照它声明的走：回包不自己拼、答不了的不挂、即刻退、落需手动（码 `NoAnswer`）。

/// 一张只有替身家那一行的注册表（`'static`：挂着的那一条在别的任务里等）。
fn fake_registry() -> &'static [crate::agents::Adapter] {
    Box::leak(Box::new([crate::agents::Adapter {
        asks: Some(crate::agents::fake::ASK),
        ..crate::agents::fake::full_row(|| None)
    }]))
}

/// 替身家的 hook 交来的那一份：`kind` 是 `permit`（批准）或 `query`（提问）。
fn fake_hook(sid: &str, kind: &str) -> Value {
    json!({"sess": sid, "event": "ask", "kind": kind, "tool": "Run", "args": {"line": "make"}})
}

fn fake_req(sid: &str, kind: &str) -> AskReq {
    crate::wire::typed::take(
        &json!({"agent": crate::agents::fake::AGENT_KIND, "hook": fake_hook(sid, kind)}),
    )
    .unwrap()
}

/// 挂一条替身家的批准、人照 `choice` 答 ⇒ hook 从常驻回包里取出来、要打到 stdout 的那一串。
async fn fake_printed(sid: &str, choice: &str) -> Option<String> {
    let reg = fake_registry();
    let req = fake_req(sid, "permit");
    let h = tokio::spawn(ask_among(reg, 9201, req, |_| None, no_states));
    while pending(sid).is_empty() {
        tokio::task::yield_now().await;
    }
    let id = pending(sid)[0].ask.id.clone();
    assert_eq!(
        answer_of(sid, &id, choice).unwrap(),
        json!({"result": "answered"})
    );
    let reply = h.await.unwrap();
    // 常驻那一侧的线上回包（`{ok, data}`）⇒ hook 客户端照它取。
    let wire =
        json!({"id": "answer-hook", "ok": true, "data": serde_json::to_value(reply).unwrap()});
    hook::decision_of(&wire).map(|d| d.to_string())
}

/// ★ 人答「允许」⇒ hook 打到 stdout 的恰是那一家声明的那一形；「拒绝」同样。
#[tokio::test]
async fn the_hook_prints_exactly_the_reply_the_agent_declares() {
    assert_eq!(
        fake_printed("t-fake-allow", "allow").await.as_deref(),
        Some(r#"{"decision":"allow"}"#)
    );
    assert_eq!(
        fake_printed("t-fake-deny", "deny").await.as_deref(),
        Some(r#"{"decision":"deny"}"#)
    );
}

/// ★ 拒绝时停不下的那一家（替身家）：挂上的成品里就没有「拒绝并停下」那一项，照它答 ⇒ 不认这一项。
#[tokio::test]
async fn a_family_that_cannot_stop_gets_no_deny_stop_in_the_product() {
    let sid = "t-fake-nostop";
    let h = tokio::spawn(ask_among(
        fake_registry(),
        9204,
        fake_req(sid, "permit"),
        |_| None,
        no_states,
    ));
    while pending(sid).is_empty() {
        tokio::task::yield_now().await;
    }
    let ask = pending(sid)[0].ask.clone();
    assert_eq!(ids(&ask), ["allow", "deny", "deny-note"]);
    assert_eq!(
        answer_of(sid, &ask.id, "deny-stop")
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_choice")
    );
    answer_of(sid, &ask.id, "deny").unwrap();
    h.await.unwrap();
}

/// ★ 那一家答不了的那一种（替身家的提问）：hook 认出来就不挂着等（交给常驻就走）；常驻当场不给决定、不挂进登记表，
/// 这条会话记一条需手动、码 `NoAnswer`，铃响这条会话；needs 里没有挂着的请求 ⇒ 那一句照这个码说。
#[tokio::test]
async fn a_kind_the_agent_cannot_answer_is_not_hung_and_lands_in_needs_manual() {
    use crate::agents::asks::{hook_off_words, HookOff};
    let sid = "t-fake-cannot";
    let face = crate::agents::fake::ASK;
    assert_eq!(
        hook::route(&face, &fake_hook(sid, "query")),
        hook::Route::Refused(sid.to_string()),
        "hook 不挂着等"
    );
    assert!(matches!(
        hook::route(&face, &fake_hook(sid, "permit")),
        hook::Route::Hang(_)
    ));

    let mut bell = crate::history::needs_changed::moved_rx();
    let v = ask_among(
        fake_registry(),
        9202,
        fake_req(sid, "query"),
        |_| None,
        no_states,
    )
    .await;
    assert_eq!(v.decision, None);
    assert!(pending(sid).is_empty(), "不挂进登记表");
    assert!(!crate::history::needs_changed::asked_sids().contains(sid));
    assert_eq!(refused(sid), Some(HookOff::NoAnswer));
    let rang = rang_for(&mut bell, sid);
    assert!(rang, "需手动可能变了：敲这条会话的铃");

    let n = crate::observe::recognize::facts::needs_of(
        Some(crate::agents::fake::AGENT_KIND),
        &[],
        Some(&crate::observe::recognize::facts::PidWait {
            waiting_for: Some(crate::agents::WaitOn::Dialog),
            since_ms: None,
            read_at_ms: 0,
        }),
    );
    let off = cmd_wire::cells::Words("答题口没接上的那一句".into());
    let n = crate::faces::read_face::dress_ask(n, sid, Some(&off)).unwrap();
    assert!(n.ask().is_none());
    assert_eq!(
        reply_of(&n).2.map(|w| w.0.clone()),
        Some(hook_off_words(HookOff::NoAnswer.env_value()).0),
        "照 NoAnswer 那个码说，不说成别的原因"
    );
}

/// 记着的那一条需手动：这条会话又挂上一条请求、或一轮的那几种来了 ⇒ 摘掉（那一问已经过去了）。
#[tokio::test]
async fn the_cannot_mark_goes_once_the_session_moves_on() {
    let reg = fake_registry();
    let sid = "t-fake-cannot-gone";
    ask_among(reg, 9203, fake_req(sid, "query"), |_| None, no_states).await;
    assert!(refused(sid).is_some());
    let desk = Desk::new();
    let h = tokio::spawn(ask_among(
        reg,
        desk.conn(),
        fake_req(sid, "permit"),
        |_| None,
        no_states,
    ));
    while pending(sid).is_empty() {
        tokio::task::yield_now().await;
    }
    assert_eq!(refused(sid), None, "又挂上一条 ⇒ 摘掉");
    drop(desk);
    h.await.unwrap();

    ask_among(reg, 9204, fake_req(sid, "query"), |_| None, no_states).await;
    assert!(refused(sid).is_some());
    let rest: AskReq = crate::wire::typed::take(&json!({
        "agent": crate::agents::fake::AGENT_KIND, "hook": {"sess": sid, "event": "rest"}
    }))
    .unwrap();
    ask_among(reg, 9205, rest, |_| None, no_states).await;
    assert_eq!(refused(sid), None, "一轮结束 ⇒ 摘掉");
}

// ───────────────────────── 挡着终端的那一种：hook 记下就放手 · 常驻记成「终端里有框」· 终端答了收起 ─────────────────────────
// 替身家只多一格（[`BEHIND`]）：hook 交来的那一份带 `behind: true` ⇒ 挡着终端（那一家先跑 hook、hook 没给决定才弹框的那一种）。

type States = Vec<(Option<crate::agents::SessionActivity>, Option<u64>)>;

/// 判据里不看态的那几条交的（一个进程都没有）。
fn no_states(_: &crate::agents::AgentHome, _: &str) -> States {
    Vec::new()
}

/// 判据摆的「这条会话各进程此刻的态」（替身家没有真 pidfile 读法）：会话 id ⇒（那一份, 被读了几次）。
fn staged() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<String, (States, u32)>> {
    static S: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<String, (States, u32)>>,
    > = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn stage(sid: &str, now: States) {
    staged().entry(sid.to_string()).or_default().0 = now;
}

fn staged_states(_: &crate::agents::AgentHome, sid: &str) -> States {
    let mut g = staged();
    let e = g.entry(sid.to_string()).or_default();
    e.1 += 1;
    e.0.clone()
}

fn reads(sid: &str) -> u32 {
    staged().get(sid).map_or(0, |e| e.1)
}

fn behind_read(v: &Value) -> Option<AskIn> {
    let mut a = (crate::agents::fake::ASK.read)(v)?;
    a.blocks_terminal = v.get("behind").and_then(Value::as_bool) == Some(true);
    Some(a)
}

const BEHIND: crate::agents::AskFace = crate::agents::AskFace {
    read: behind_read,
    ..crate::agents::fake::ASK
};

fn behind_home() -> Option<std::path::PathBuf> {
    static H: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    Some(H.get_or_init(|| test_home::scratch("asks-behind")).clone())
}

/// 替身家那一行（答题口换成 [`BEHIND`]）；`status` ＝ 这一家有没有会话状态文件（本机布局那一格）。
fn behind_registry(status: bool) -> &'static [crate::agents::Adapter] {
    let row = crate::agents::fake::full_row(behind_home);
    // 替身家的本机布局认环境里的家；这里钉成判据自己那一个（不动进程环境）。
    let local = row.local.filter(|_| status).map(|mut l| {
        l.home_at = |_, _| behind_home().unwrap_or_default();
        l
    });
    Box::leak(Box::new([crate::agents::Adapter {
        asks: Some(BEHIND),
        local,
        ..row
    }]))
}

/// 替身家的状态文件目录（建好）。
fn behind_dir() -> std::path::PathBuf {
    let local = crate::agents::fake::full_row(behind_home)
        .local
        .expect("替身家有本机布局");
    let dir = (local.pidfile_dir)(&behind_home().unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 状态文件那个目录里动一下（会话进程改了它的状态文件）。
fn poke(sid: &str) {
    std::fs::write(behind_dir().join(format!("{sid}.json")), "{}").unwrap();
}

fn behind_hook(sid: &str) -> Value {
    let mut h = fake_hook(sid, "permit");
    h["behind"] = json!(true);
    h
}

fn behind_req(sid: &str) -> AskReq {
    crate::wire::typed::take(
        &json!({"agent": crate::agents::fake::AGENT_KIND, "hook": behind_hook(sid)}),
    )
    .unwrap()
}

fn rest_req(sid: &str) -> AskReq {
    crate::wire::typed::take(&json!({
        "agent": crate::agents::fake::AGENT_KIND, "hook": {"sess": sid, "event": "rest"}
    }))
    .unwrap()
}

/// 在 `until` 成立之前一直让（`guard_core::wall::CEILING` 之内不成就红）。
async fn soon(what: &str, until: impl Fn() -> bool) {
    tokio::time::timeout(guard_core::wall::CEILING, async {
        while !until() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("等不到：{what}"));
}

/// 永不回包的常驻（同进程，绑 `<家>/run/backend.sock`）：每条连接给 hello、回「接上了」，读一条请求交出来，之后一个字也不回、连接不关。
#[cfg(unix)]
fn mute_resident(dh: &std::path::Path) -> tokio::sync::mpsc::UnboundedReceiver<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let run = relay_route_core::listen_dir_for(dh);
    std::fs::create_dir_all(&run).unwrap();
    test_home::set_dir_mode(&run, 0o700);
    let l = own_chan::bind(&relay_route_core::listen_socket_for(dh)).expect("绑");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let Ok(own_chan::Accepted::Ours(s)) = l.accept().await else {
                continue;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let (rd, mut wr) = tokio::io::split(s);
                let mut rd = tokio::io::BufReader::new(rd).lines();
                wr.write_all(b"{\"kind\":\"hello\",\"build_id\":\"x\"}\n")
                    .await
                    .unwrap();
                rd.next_line().await.unwrap();
                wr.write_all(b"{\"attach\":\"ok\"}\n").await.unwrap();
                if let Ok(Some(line)) = rd.next_line().await {
                    tx.send(serde_json::from_str::<Value>(&line).unwrap()).ok();
                }
                let _ = rd.next_line().await;
                drop(wr);
            });
        }
    });
    rx
}

/// ★ hook 一侧：挡着终端的那一种交给常驻就空手走（不等回包、stdout 一个字节都不打）⇒ 终端照没装 cc-monitor 那样弹框；
/// 不挡终端的（主会话那种）照旧挂着等回包。常驻永不回包。
#[tokio::test]
#[cfg(unix)]
async fn the_hook_lets_go_of_a_request_that_blocks_the_terminal_and_still_waits_on_the_rest() {
    let dh = test_home::scratch("hook-behind");
    let mut got = mute_resident(&dh);
    let agent = crate::agents::fake::AGENT_KIND;

    let behind = behind_hook("t-behind-hook");
    let out = tokio::time::timeout(
        guard_core::wall::CEILING,
        hook::deliver(
            &dh,
            agent,
            hook::route(&BEHIND, &behind),
            &behind,
            None,
            no_states,
        ),
    )
    .await
    .expect("挡着终端的那一种不挂着等");
    assert_eq!(out, None, "stdout 一个字节都不打");
    let req = got.recv().await.expect("交给了常驻");
    assert_eq!(req["cmd"], "needs-ask");
    assert_eq!(req["args"]["hook"], behind);

    let main = fake_hook("t-behind-main", "permit");
    let waiting = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        hook::deliver(
            &dh,
            agent,
            hook::route(&BEHIND, &main),
            &main,
            None,
            no_states,
        ),
    )
    .await;
    assert!(waiting.is_err(), "主会话那一种仍挂着等回包");
    assert_eq!(
        got.recv().await.expect("也交给了常驻")["args"]["hook"],
        main
    );
    let _ = std::fs::remove_dir_all(&dh);
}

/// ★ 常驻一侧：挡着终端的那一种当场不给决定、记成「终端里有框」（不跟 hook 那条连接走）；这一家认不得终端里那个框 ⇒ app 里一颗按钮都不给、写「在终端里答」；
/// 一轮的那几种来了不收（后台子 agent 的框跟主会话的一轮无关）；状态文件见过在等、又离开在等 ⇒ 收起那一张、记「别处已答」那一句。
#[tokio::test]
async fn a_terminal_ask_is_noted_without_choices_and_folds_once_the_terminal_answers() {
    use crate::agents::SessionActivity as A;
    let sid = "t-behind-fold";
    stage(sid, vec![(Some(A::Working), Some(1))]);
    let reg = behind_registry(true);
    behind_dir();
    let desk = Desk::new();
    let v = tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(reg, desk.conn(), behind_req(sid), |_| None, staged_states),
    )
    .await
    .expect("不等人答");
    assert_eq!(v.decision, None, "不给决定 ⇒ 框在终端里出");
    drop(desk);
    let p = pending(sid);
    assert_eq!(p.len(), 1, "hook 走了、连接断了也记着：终端里有框");
    assert!(p[0].ask.choices.is_empty(), "app 里不给答的按钮");
    // app 照记下的那一份显示：界面答不了 ⇒ 不交 ask（出口不从 ask · choices 自己推答不答得了），
    // 只交那一句「在终端里答」；钉条照这一条的工具与命令写（pidfile 说不说在等都一样）。
    let at_terminal = Some(copy_text("beAsk.manual.inTerminal", &[]));
    let alone = crate::observe::recognize::facts::dress_ask_with(None, p.first().cloned(), 0)
        .expect("有 needs");
    assert!(alone.ask().is_none(), "界面答不了 ⇒ 不交 ask");
    assert!(reply_of(&alone).0.is_none());
    assert_eq!(alone.tool.as_deref(), Some("Run"));
    assert_eq!(alone.head_code.as_deref(), Some("make"));
    assert_eq!(reply_of(&alone).2.map(|w| w.0.clone()), at_terminal);
    // 三处经的那一道（`faces::read_face::dress_ask`）也照它：挂着这一条时，不拿「答题口没接上」那类原因盖掉那一句。
    let off = cmd_wire::cells::Words("答题口没接上的那一句".into());
    let dressed = crate::faces::read_face::dress_ask(None, sid, Some(&off)).expect("有 needs");
    assert_eq!(reply_of(&dressed).2.map(|w| w.0.clone()), at_terminal);
    let waiting = crate::observe::recognize::facts::needs_of(
        Some(crate::agents::fake::AGENT_KIND),
        &[],
        Some(&crate::observe::recognize::facts::PidWait {
            waiting_for: Some(crate::agents::WaitOn::Permission),
            since_ms: None,
            read_at_ms: 0,
        }),
    );
    let both = crate::observe::recognize::facts::dress_ask_with(waiting, p.first().cloned(), 0)
        .expect("有 needs");
    assert!(both.ask().is_none());
    assert_eq!(reply_of(&both).2.map(|w| w.0.clone()), at_terminal);
    assert_eq!(
        answer_of(sid, &p[0].ask.id, "allow")
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_choice")
    );

    ask_among(reg, 9311, rest_req(sid), |_| None, staged_states).await;
    assert_eq!(pending(sid).len(), 1, "主会话一轮结束不算答了");

    soon("挂上那一下先读一次", || reads(sid) >= 2).await;
    let before = reads(sid);
    stage(sid, vec![(Some(A::NeedsYou), Some(2))]);
    poke(sid);
    soon("读到框出来了", || reads(sid) > before).await;
    assert_eq!(pending(sid).len(), 1, "框还在终端里");

    stage(sid, vec![(Some(A::Working), Some(3))]);
    poke(sid);
    soon("终端那边答了 ⇒ 收起", || pending(sid).is_empty()).await;
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false))
    );
}

/// 没有会话状态文件的那一家：判不了框在不在 ⇒ 这条会话一轮的那几种来了（那一轮走完 / 又提交了一句）才收起。
#[tokio::test]
async fn without_a_status_file_a_terminal_ask_folds_when_the_turn_moves_on() {
    let sid = "t-behind-nostatus";
    let reg = behind_registry(false);
    tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(reg, 9321, behind_req(sid), |_| None, no_states),
    )
    .await
    .expect("不等人答");
    assert_eq!(pending(sid).len(), 1);
    ask_among(reg, 9322, rest_req(sid), |_| None, no_states).await;
    assert!(pending(sid).is_empty(), "一轮走完 ⇒ 那一问已经过去了");
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false))
    );
}

/// ★ 同一条会话先挂着一条在终端里答的（后台子 agent），又挂上一条界面答得了的（主会话那种）：
/// 界面答得了的排前头 ⇒ 会话事实交它的 ask，不被前面那条「在终端里答」盖住。
#[tokio::test]
async fn an_answerable_ask_is_not_hidden_behind_one_answered_in_the_terminal() {
    let sid = "t-behind-first";
    let reg = behind_registry(false);
    tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(reg, 9331, behind_req(sid), |_| None, no_states),
    )
    .await
    .expect("不等人答");
    let desk = Desk::new();
    let h = tokio::spawn(ask_among(
        reg,
        desk.conn(),
        fake_req(sid, "permit"),
        |_| None,
        no_states,
    ));
    soon("主会话那条挂上", || pending(sid).len() == 2).await;
    let p = pending(sid);
    assert!(!p[0].ask.choices.is_empty(), "界面答得了的在前");
    let n = crate::observe::recognize::facts::dress_ask_with(None, p.first().cloned(), 0).unwrap();
    assert!(n.ask().is_some(), "交答得了的那一条");
    drop(desk);
    h.await.unwrap();
}

// ───────────────────────── 这条会话那一家的号另有家：判「终端答了」盯它自己那一家的状态文件 ─────────────────────────

fn apart_home() -> Option<std::path::PathBuf> {
    static H: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    Some(H.get_or_init(|| test_home::scratch("asks-apart")).clone())
}

/// 替身家那一行：常驻自己环境里那一家的家是 [`behind_home`]；这一家的号（那一行自己的家）住 [`apart_home`]，
/// 那个号的 pidfile 目录不链到前一个（各是各的目录）。
fn apart_registry() -> &'static [crate::agents::Adapter] {
    let row = crate::agents::fake::full_row(apart_home);
    let local = row.local.map(|mut l| {
        l.home_at = |config_dir, _| {
            config_dir
                .map(std::path::Path::to_path_buf)
                .or_else(behind_home)
                .unwrap_or_default()
        };
        l
    });
    Box::leak(Box::new([crate::agents::Adapter {
        asks: Some(BEHIND),
        local,
        ..row
    }]))
}

/// 那个号的状态文件目录（建好）。
fn apart_dir() -> std::path::PathBuf {
    let local = crate::agents::fake::full_row(apart_home)
        .local
        .expect("替身家有本机布局");
    let dir = (local.pidfile_dir)(&apart_home().unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 这条会话的进程只在那个号的状态文件目录里（别处读出零个进程）。
fn apart_states(home: &crate::agents::AgentHome, sid: &str) -> States {
    if Some(&home.home) == apart_home().as_ref() {
        staged_states(home, sid)
    } else {
        Vec::new()
    }
}

/// ★ 这条会话的号不是常驻自己环境里那一家、那个号的状态文件目录也不链到那一家的：终端答了 ⇒ 照样收起那一张
/// （盯的是这条会话自己那一家的状态文件，不是常驻自己那一家的）。
#[tokio::test]
async fn a_terminal_ask_from_an_account_with_its_own_status_dir_folds_once_the_terminal_answers() {
    use crate::agents::SessionActivity as A;
    let sid = "t-behind-apart";
    stage(sid, vec![(Some(A::Working), Some(1))]);
    let reg = apart_registry();
    behind_dir();
    let dir = apart_dir();
    tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(reg, 9341, behind_req(sid), |_| None, apart_states),
    )
    .await
    .expect("不等人答");
    assert_eq!(pending(sid).len(), 1, "终端里有框");

    soon("挂上那一下先读一次那个号的状态文件", || reads(sid) >= 2).await;
    let before = reads(sid);
    stage(sid, vec![(Some(A::NeedsYou), Some(2))]);
    std::fs::write(dir.join(format!("{sid}.json")), "{}").unwrap();
    soon("读到框出来了", || reads(sid) > before).await;
    assert_eq!(pending(sid).len(), 1, "框还在终端里");

    stage(sid, vec![(Some(A::Working), Some(3))]);
    std::fs::write(dir.join(format!("{sid}.json")), "{}").unwrap();
    soon("终端那边答了 ⇒ 收起", || pending(sid).is_empty()).await;
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("askBlock.gone.elsewhere", &[]), false))
    );
}

/// 有状态文件的那一家，可各号的家里都找不到这条会话的进程（判不了盯哪一处）⇒ 不盯，一轮的那几种来了才收起。
#[tokio::test]
async fn a_terminal_ask_whose_session_no_home_holds_folds_when_the_turn_moves_on() {
    let sid = "t-behind-nohome";
    let reg = behind_registry(true);
    behind_dir();
    tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(reg, 9351, behind_req(sid), |_| None, no_states),
    )
    .await
    .expect("不等人答");
    assert_eq!(pending(sid).len(), 1);
    ask_among(reg, 9352, rest_req(sid), |_| None, no_states).await;
    assert!(pending(sid).is_empty(), "没盯 ⇒ 一轮走完就当那一问过去了");
}

// ── app 里答挡着终端的那一条：经读屏认准这一条的框才按（替身家拼上一份认框 [`FAKE_SCREEN`] ＋ 替身屏幕）──────────────

/// 替身家在终端里的框（与 Claude 不同形，夹具是编的）：一行 `[permit] <工具> <line>`；只认批准；允许按回车、拒绝按 Esc。
const FAKE_SCREEN: crate::agents::OnScreen = crate::agents::OnScreen {
    covers: |a| a.kind == AskKind::Approve,
    seen: |lines, a| {
        let want = box_line(a);
        lines.iter().any(|l| l.trim() == want)
    },
    // 屏上一行 `[queue] <n>` ⇒ 还排着 n 条（连正显示的这一条）。
    queued: |lines| {
        lines
            .iter()
            .find_map(|l| l.trim().strip_prefix("[queue] ")?.parse().ok())
    },
    allow: crate::agents::Keystroke::Key("enter"),
    deny: crate::agents::Keystroke::Key("esc"),
};

fn box_line(a: &AskIn) -> String {
    format!(
        "[permit] {} {}",
        a.tool,
        a.what.as_deref().unwrap_or_default()
    )
}

const ON_SCREEN_FACE: crate::agents::AskFace = crate::agents::AskFace {
    read: behind_read,
    on_screen: Some(FAKE_SCREEN),
    ..crate::agents::fake::ASK
};

/// 替身家那一行（答题口认得出终端里那个框），没有状态文件（收不收只看这里答没答）。
fn screen_registry() -> &'static [crate::agents::Adapter] {
    let row = crate::agents::fake::full_row(behind_home);
    Box::leak(Box::new([crate::agents::Adapter {
        asks: Some(ON_SCREEN_FACE),
        local: None,
        ..row
    }]))
}

/// 挡着终端的一条批准，命令（替身家的 `line`）是 `line`。
fn screen_req(sid: &str, line: &str) -> AskReq {
    let mut h = behind_hook(sid);
    h["args"] = json!({"line": line});
    crate::wire::typed::take(&json!({"agent": crate::agents::fake::AGENT_KIND, "hook": h})).unwrap()
}

/// 替身屏幕：此刻那一屏起每次重画换一样；按一下之后换成 `after` 那几屏。记下按了哪几下。
struct Term {
    now: Vec<String>,
    ahead: std::collections::VecDeque<Vec<String>>,
    after: Option<Vec<Vec<String>>>,
    taps: Vec<crate::agents::Keystroke>,
}

impl Term {
    fn new(screens: &[&str], after: &[&str]) -> Term {
        let shot = |s: &&str| s.lines().map(str::to_string).collect::<Vec<_>>();
        let mut ahead: std::collections::VecDeque<_> = screens.iter().map(shot).collect();
        Term {
            now: ahead.pop_front().unwrap_or_default(),
            ahead,
            after: Some(after.iter().map(shot).collect()),
            taps: Vec::new(),
        }
    }
}

impl crate::control::screen_drive::BoxIo for Term {
    fn open(
        &mut self,
    ) -> Result<Option<crate::control::terminals::Refused>, crate::common::said::Said> {
        Ok(None)
    }
    fn screen(&mut self) -> Result<Vec<String>, crate::common::said::Said> {
        Ok(self.now.clone())
    }
    fn redrawn(&mut self) -> bool {
        match self.ahead.pop_front() {
            Some(s) => {
                self.now = s;
                true
            }
            None => false,
        }
    }
    fn tap(
        &mut self,
        k: crate::agents::Keystroke,
    ) -> Result<crate::control::terminals_wire::Sent, crate::common::said::Said> {
        self.taps.push(k);
        if let Some(a) = self.after.take() {
            self.ahead = a.into();
        }
        Ok(crate::control::terminals::input_reply(
            cmd_wire::session::SentResult::Delivered,
            None,
            None,
        ))
    }
}

/// 经替身屏幕答 `sid` 挂着的头一条（`choice`）。
fn answer_on(sid: &str, choice: &str, term: &mut Term) -> Result<Value, crate::common::said::Said> {
    let id = pending(sid)[0].ask.id.clone();
    let req: AnswerReq =
        crate::wire::typed::take(&json!({"sid": sid, "ask": id, "choice": choice})).unwrap();
    crate::control::asks::answer_needs_through(req, |_, seen, doubt, k| {
        crate::control::screen_drive::answer_box(term, seen, doubt, k)
    })
    .map(|a| serde_json::to_value(a).unwrap())
}

async fn hang_on_screen(sid: &str, line: &str) {
    tokio::time::timeout(
        guard_core::wall::CEILING,
        ask_among(
            screen_registry(),
            9401,
            screen_req(sid, line),
            |_| None,
            no_states,
        ),
    )
    .await
    .expect("不等人答");
}

fn box_of(sid: &str, line: &str) -> String {
    let a = (ON_SCREEN_FACE.read)(&screen_req(sid, line).hook).unwrap();
    box_line(&a)
}

/// ★ 认准才按：终端上正是这一条的框 ⇒ 允许按那一家声明的那一下、只按一下；按完再读一屏框没了 ⇒ 已答、那一张收起。
/// 成品给「允许 · 拒绝」两项（界面答得了 ⇒ 会话事实交 ask）。这一路不读会话在不在等人（切档那道「在等人不送」的门不在这一路上）。
#[tokio::test]
async fn an_ask_answered_in_the_app_presses_its_box_only_once_seen_and_checks_it_gone() {
    let sid = "t-screen-yes";
    hang_on_screen(sid, "make").await;
    let p = pending(sid);
    assert_eq!(
        ids(&p[0].ask),
        ["allow", "deny"],
        "认得出框 ⇒ 给允许 · 拒绝"
    );
    let n = crate::observe::recognize::facts::dress_ask_with(None, p.first().cloned(), 0).unwrap();
    assert!(n.ask().is_some(), "界面答得了 ⇒ 交 ask");
    let mine = box_of(sid, "make");
    let mut t = Term::new(&[&format!("✻ …\n{mine}")], &["$"]);
    assert_eq!(
        answer_on(sid, "allow", &mut t).unwrap(),
        json!({"result": "answered"})
    );
    assert_eq!(
        t.taps,
        [FAKE_SCREEN.allow],
        "允许只按一下、按那一家声明的键"
    );
    assert!(pending(sid).is_empty(), "答上了 ⇒ 收起");
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("beAsk.answer.already", &[]), false))
    );

    let sid = "t-screen-no";
    hang_on_screen(sid, "make").await;
    let mut t = Term::new(&[&box_of(sid, "make")], &["$"]);
    assert_eq!(
        answer_on(sid, "deny", &mut t).unwrap(),
        json!({"result": "answered"})
    );
    assert_eq!(t.taps, [FAKE_SCREEN.deny]);
}

/// ★ 认不出不按：屏上没有框 · 是别的命令的框（`1 of 2` 正显示的是排着的另一条）⇒ 照实说没认出，替身记下的按键为零，那一张还挂着。
#[tokio::test]
async fn an_unseen_box_is_not_pressed_and_the_ask_stays() {
    let sid = "t-screen-unseen";
    hang_on_screen(sid, "make").await;
    for screen in ["$ make", box_of(sid, "make test").as_str()] {
        let mut t = Term::new(&[screen, screen, screen], &[]);
        let e = answer_on(sid, "allow", &mut t).unwrap_err();
        assert_eq!(e.code.as_deref(), Some("not_seen"), "{screen}");
        assert!(t.taps.is_empty(), "认不出也按了：{screen}");
        assert_eq!(pending(sid).len(), 1);
    }
}

/// ★ 按完复核：按了、框还在 ⇒ 不确定（不说已答），那一张还挂着。
#[tokio::test]
async fn a_box_still_there_after_the_press_answers_unsure() {
    let sid = "t-screen-stays";
    hang_on_screen(sid, "make").await;
    let mine = box_of(sid, "make");
    let mut t = Term::new(&[&mine], &[&mine, &mine]);
    let e = answer_on(sid, "allow", &mut t).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("unsure"));
    assert_eq!(t.taps.len(), 1);
    assert_eq!(pending(sid).len(), 1);
}

/// ★ 同一条会话挂着两条一样的（同 agent · 同工具 · 同命令）：屏上分不出是哪一条 ⇒ 一个键都不按、照实说，两张照旧挂着。
#[tokio::test]
async fn two_alike_asks_press_nothing() {
    let sid = "t-screen-twin";
    hang_on_screen(sid, "make").await;
    hang_on_screen(sid, "make").await;
    let mut t = Term::new(&[&box_of(sid, "make")], &["$"]);
    let e = answer_on(sid, "allow", &mut t).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("twin"));
    assert!(t.taps.is_empty());
    assert_eq!(pending(sid).len(), 2);
}

/// 只给认框的那两项：附话 · 停下这一轮那几项终端里按不出来 ⇒ `bad_choice`，一个键不按。
#[tokio::test]
async fn only_allow_and_deny_are_pressed_on_screen() {
    let sid = "t-screen-badchoice";
    hang_on_screen(sid, "make").await;
    let mut t = Term::new(&[&box_of(sid, "make")], &["$"]);
    let e = answer_on(sid, "deny-note", &mut t).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("bad_choice"));
    assert!(t.taps.is_empty());
}

/// 铃响过 `sid` 没有（收下此刻排着的全部）：别的判据并行敲铃挤掉了最早的几声（`Lagged`）⇒ 跳过那一下接着收，不当收完了。
fn rang_for(bell: &mut tokio::sync::broadcast::Receiver<Option<String>>, sid: &str) -> bool {
    use tokio::sync::broadcast::error::TryRecvError;
    loop {
        match bell.try_recv() {
            Ok(m) if m.as_deref() == Some(sid) => return true,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(_) => return false,
        }
    }
}

/// ★ 屏上说还排着两条、登记表只认得这一条（后端重启前挂上的那条不在表里）：分不出正显示的是哪一条 ⇒ 一个键都不按、照实说。
/// 两条都在表里（屏上说的条数对得上）⇒ 照常认、照常按。
#[tokio::test]
async fn a_queue_longer_than_the_book_knows_presses_nothing() {
    let sid = "t-screen-unknown-queue";
    hang_on_screen(sid, "make").await;
    let mine = box_of(sid, "make");
    let mut t = Term::new(&[&format!("[queue] 2\n{mine}")], &["$"]);
    let e = answer_on(sid, "allow", &mut t).unwrap_err();
    assert_eq!(e.code.as_deref(), Some("unknown_queue"));
    assert!(t.taps.is_empty(), "排着认不得的那一条也按了");

    hang_on_screen(sid, "make test").await;
    let mut t = Term::new(&[&format!("[queue] 2\n{mine}")], &["[queue] 1\n$"]);
    assert_eq!(
        answer_on(sid, "allow", &mut t).unwrap(),
        json!({"result": "answered"})
    );
    assert_eq!(t.taps.len(), 1);
}

/// ★ 按完那一刻、那一条已被盯着的那一侧（这里是一轮结束）先收掉记成「别处已答」：读屏复核框没了 ⇒ 那一格改记「已答」。
#[tokio::test(flavor = "multi_thread")]
async fn a_press_whose_ask_was_folded_meanwhile_still_ends_as_answered() {
    let sid = "t-screen-folded";
    hang_on_screen(sid, "make").await;
    let mut t = Term::new(&[&box_of(sid, "make")], &["$"]);
    let id = pending(sid)[0].ask.id.clone();
    let req: AnswerReq =
        crate::wire::typed::take(&json!({"sid": sid, "ask": id, "choice": "allow"})).unwrap();
    let got = tokio::task::block_in_place(|| {
        crate::control::asks::answer_needs_through(req, |_, seen, doubt, k| {
            let r = crate::control::screen_drive::answer_box(&mut t, seen, doubt, k);
            let h = tokio::runtime::Handle::current();
            h.block_on(ask_among(
                screen_registry(),
                9411,
                rest_req(sid),
                |_| None,
                no_states,
            ));
            assert!(pending(sid).is_empty(), "一轮结束该先收掉没盯的那一条");
            r
        })
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(got).unwrap(),
        json!({"result": "answered"})
    );
    assert_eq!(
        ended(sid),
        Some(end(&copy_text("beAsk.answer.already", &[]), false))
    );
}
