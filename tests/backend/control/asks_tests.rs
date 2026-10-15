//! 待答登记表：挂上 · 第一个答案落下其余 already_answered · 连接断了撤掉 · 子 agent 的请求照样挂、带 agent_id · needs 配上 ask。
//! 登记表是进程里一份，各条用各自的会话 id，并行跑互不相扰。夹具正文都是编的。

use super::*;
use crate::agents::{AskKind, AskOption, AskQuestion, AskSuggestion};
use serde_json::json;

/// 线上那一份入参照分派那一层解（`typed::take`，`deny_unknown_fields`）再交本体；回包投成线上那一份。
/// 入参形状不对 ⇒ `bad_args`（与本体回的码同一形，判据只比码）。
fn answer(args: &Value) -> Result<Value, (&'static str, String)> {
    let req: AnswerReq = crate::wire::typed::take(args).map_err(|f| ("bad_args", f.message))?;
    super::answer(req).map(|a| serde_json::to_value(a).expect("回包总能序列化"))
}

const AGENT: &str = "claude";

fn approve(sid: &str) -> AskIn {
    AskIn {
        sid: sid.into(),
        kind: AskKind::Approve,
        tool: "Bash".into(),
        what: Some("touch a".into()),
        detail: "touch a\necho b".into(),
        suggestions: vec![AskSuggestion {
            text: "/w".into(),
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

fn answer_of(sid: &str, ask: &str, choice: &str) -> Result<Value, (&'static str, String)> {
    answer(&json!({"sid": sid, "ask": ask, "choice": choice}))
}

#[test]
fn approval_choices_are_written_and_the_never_ask_again_one_is_not_first() {
    let a = product_of("x", &approve("t-choices"), 5);
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
        ids(&product_of("x", &p, 0)),
        ["plan-auto", "plan-edits", "plan-manual", "plan-keep"]
    );
    let mut q = approve("t-q");
    q.kind = AskKind::Question;
    q.suggestions.clear();
    assert_eq!(
        ids(&product_of("x", &q, 0)),
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
    let pq = product_of("x", &q, 0);
    assert_eq!(ids(&pq), ["answer", "dismiss"]);
    assert_eq!(pq.questions[0].id, "q0");
    assert_eq!(pq.questions[0].options[1].id, "q0o1");
    assert!(pq.questions[0].other);
}

#[test]
fn the_first_answer_lands_and_the_rest_are_already_answered() {
    let sid = "t-first";
    let mut rx = open(AGENT, approve(sid), 9001);
    let a = pending(sid).pop().expect("挂上了");
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
        answer_of(sid, &a.id, "deny").unwrap_err().0,
        "already_answered"
    );
    assert!(pending(sid).is_empty(), "答掉 ⇒ 不再挂着");
}

#[test]
fn when_the_hook_side_goes_away_the_request_is_withdrawn_and_closed_elsewhere() {
    let sid = "t-gone";
    let desk = Desk::new();
    let _rx = open(AGENT, approve(sid), desk.conn());
    let a = pending(sid).pop().unwrap();
    drop(desk); // hook 那条连接断了（终端先答了 · 被打断 · 到点）
    assert!(pending(sid).is_empty());
    assert_eq!(
        answer_of(sid, &a.id, "allow").unwrap_err().0,
        "closed_elsewhere"
    );
    assert_eq!(
        answer_of(sid, "ask-nope", "allow").unwrap_err().0,
        "not_found"
    );
}

#[test]
fn a_dead_receiver_is_closed_elsewhere_not_answered() {
    let sid = "t-deadrx";
    drop(open(AGENT, approve(sid), 9002));
    let a = pending(sid).pop().unwrap();
    assert_eq!(
        answer_of(sid, &a.id, "allow").unwrap_err().0,
        "closed_elsewhere"
    );
}

#[test]
fn a_sub_agent_request_lands_in_needs_carries_its_id_and_is_answered() {
    let sid = "t-sub";
    let mut a = approve(sid);
    a.agent_id = Some("agent-7".into());
    a.agent_type = Some("general-purpose".into());
    a.blocks_terminal = true;
    let mut rx = open(AGENT, a, 9003);
    // 挡着终端：pidfile 不说在等 ⇒ 照挂着的请求补出一份 needs。
    let needs =
        crate::observe::facts_query::dress_ask_with(None, pending(sid).into_iter().next(), 0)
            .expect("有挂着的就有 needs");
    let ask = needs.ask.clone().expect("带 ask");
    assert_eq!(needs.kind, crate::observe::facts_query::NeedsKind::Approve);
    assert_eq!(needs.tool.as_deref(), Some("Bash"));
    assert_eq!(ask.run.as_ref().map(|r| r.id.as_str()), Some("agent-7"));
    assert_eq!(
        ask.run.as_ref().and_then(|r| r.kind.as_deref()),
        Some("general-purpose")
    );
    assert_eq!(
        ids(&ask).last(),
        Some(&"release"),
        "挡着终端 ⇒ 多一项改到终端里答"
    );
    assert_eq!(
        answer_of(sid, &ask.id, "allow").unwrap(),
        json!({"result": "answered"})
    );
    let got = rx.try_recv().unwrap().unwrap();
    assert_eq!(got["hookSpecificOutput"]["decision"]["behavior"], "allow");
}

#[test]
fn release_hands_back_no_decision() {
    let sid = "t-release";
    let mut a = approve(sid);
    a.agent_id = Some("a".into());
    a.blocks_terminal = true;
    let mut rx = open(AGENT, a, 9004);
    let id = pending(sid).pop().unwrap().id;
    answer_of(sid, &id, "release").unwrap();
    assert_eq!(rx.try_recv().unwrap(), None, "不给决定 ⇒ 框在终端里出来");
    let id2 = {
        let _rx2 = open(AGENT, approve(sid), 9004);
        pending(sid).pop().unwrap().id
    };
    assert_eq!(
        answer_of(sid, &id2, "release").unwrap_err().0,
        "bad_choice",
        "不挡终端的没有这一项"
    );
}

#[test]
fn notes_and_picks_are_checked_before_anything_is_delivered() {
    let sid = "t-check";
    let mut rx = open(AGENT, approve(sid), 9005);
    let id = pending(sid).pop().unwrap().id;
    assert_eq!(
        answer_of(sid, &id, "deny-note").unwrap_err().0,
        "note_required"
    );
    assert_eq!(answer_of(sid, &id, "always-3").unwrap_err().0, "bad_choice");
    assert_eq!(
        answer_of("t-other", &id, "allow").unwrap_err().0,
        "not_found",
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
    q.kind = AskKind::Question;
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
    let mut rx = open(AGENT, q, 9006);
    let id = pending(qsid).pop().unwrap().id;
    let try_picks =
        |p: Value| answer(&json!({"sid": qsid, "ask": id, "choice": "answer", "picks": p}));
    assert_eq!(
        try_picks(json!({"q0": {"options": ["q0o0"]}}))
            .unwrap_err()
            .0,
        "bad_args",
        "每题都要答"
    );
    assert_eq!(
        try_picks(json!({"q0": {"options": ["q0o0", "q0o1"]}, "q1": {"options": ["q1o0"]}}))
            .unwrap_err()
            .0,
        "bad_args",
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
    let n = crate::observe::facts_query::needs_of(
        &[],
        Some(&crate::observe::facts_query::PidWait {
            waiting_for: Some(crate::agents::WaitOn::Dialog),
            since_ms: None,
            read_at_ms: 0,
        }),
    );
    let n = crate::observe::facts_query::dress_ask_with(n, None, 0).unwrap();
    assert!(n.ask.is_none());
    assert_eq!(
        n.manual_text.as_ref().map(|w| w.0.clone()),
        Some(copy_core::copy_text("beAsk.manual.terminal", &[]))
    );
    assert!(crate::observe::facts_query::dress_ask_with(None, None, 0).is_none());
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
            .code,
        "bad_args"
    );
}

#[test]
fn the_hook_client_calls_it_closed_only_after_leaving_waiting() {
    use crate::agents::SessionActivity as A;
    let started = 1000;
    let mut seen = false;
    // 起来时还没轮到它（状态是旧的）⇒ 没关。
    assert!(!hook::closed_now(
        &[(Some(A::Working), Some(900))],
        started,
        &mut seen
    ));
    // 框出来了 ⇒ 在等。
    assert!(!hook::closed_now(
        &[(Some(A::NeedsYou), Some(1001))],
        started,
        &mut seen
    ));
    assert!(seen);
    // 终端那边答了 ⇒ 离开在等 ⇒ 关了。
    assert!(hook::closed_now(
        &[(Some(A::Working), Some(1002))],
        started,
        &mut seen
    ));
    // 没见过在等，但状态更新晚于起来那一刻、且不在等 ⇒ 关了（那一下的「在等」没赶上看）。
    let mut seen = false;
    assert!(hook::closed_now(
        &[(Some(A::Working), Some(1500))],
        started,
        &mut seen
    ));
    // 会话没了：见过在等 ⇒ 关了；没见过 ⇒ 不判。
    let mut seen = true;
    assert!(hook::closed_now(&[], started, &mut seen));
    let mut seen = false;
    assert!(!hook::closed_now(&[], started, &mut seen));
}

/// hook 认「是不是 ccm 起的那一个会话」：只有经环境那一路来的 hook（自己的环境里那个变量带着我们的插件目录）才要认 ——
/// `--settings` 那一路不会传给子进程。认法：先看常驻那份表（发问那个进程有 ccm 留的便条 · 会话 id 记在起会话那份记录里），
/// 再看发问那个进程起来时的环境：带着那一家的嵌套标记 ⇒ 是会话里 Bash 起的子进程；读不出 ⇒ 认不出。认不出 ⇒ 空手退。
#[test]
fn a_hook_that_came_by_the_environment_answers_only_for_the_session_ccm_started() {
    use crate::platform::proc::EnvRead;
    let ours = "/h/.cc-monitor/answer-plugin";
    let via = Some("/u/mine:/h/.cc-monitor/answer-plugin");
    // `--settings` 那一路（环境里没有我们的目录）：不认，照答。
    assert!(hook::admitted(None, ours, false, EnvRead::Unreadable));
    assert!(hook::admitted(
        Some("/u/mine"),
        ours,
        false,
        EnvRead::Value("1".into())
    ));
    // 经环境来的：表里有 ⇒ 是。
    assert!(hook::admitted(via, ours, true, EnvRead::Unreadable));
    // 表里没有：发问那个进程起来时没有嵌套标记 ⇒ 是（ccm 起会话前清掉了它）；带着 ⇒ 子进程；读不出 ⇒ 认不出。
    assert!(hook::admitted(via, ours, false, EnvRead::Unset));
    assert!(!hook::admitted(
        via,
        ours,
        false,
        EnvRead::Value("1".into())
    ));
    assert!(!hook::admitted(via, ours, false, EnvRead::Unreadable));
}

// ───────────────────────── 没人答时自动答 ─────────────────────────
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

/// 挂上（在别的任务里等），等它真挂上了再交回。
async fn hang(
    conn: u64,
    req: AskReq,
    setting: Option<AutoAnswer>,
) -> tokio::task::JoinHandle<AskReply> {
    let sid = req.hook["session_id"].as_str().unwrap().to_string();
    let h = tokio::spawn(ask(conn, req, move |_| setting));
    while pending(&sid).is_empty() {
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
    let id = pending(sid)[0].id.clone();
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
        &[("min", "10"), ("say", "照推荐的来")],
    );
    assert_eq!(answered(sid).map(|w| w.0), Some(said.clone()));
    assert!(answered_rows()
        .iter()
        .any(|r| r.sid == sid && r.said.0 == said));
    // 之后桌面 · 手机再答：已有人答过，那一句是记录那一句。
    let e = answer_of(sid, &id, "dismiss").unwrap_err();
    assert_eq!(e, ("already_answered", said));
    // 下一次挂上提问 ⇒ 那条记录换掉。
    let _rx = open(AGENT, approve(sid), 9102);
    assert_eq!(answered(sid), None);
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
        AUTO_SAY
    );
}

#[tokio::test(start_paused = true)]
async fn a_human_who_answers_first_wins_and_nothing_is_said_for_them() {
    let sid = "t-auto-human";
    let h = hang(9104, hook_of(sid, "AskUserQuestion"), on(5, None)).await;
    pass(4, 0).await;
    let a = pending(sid).pop().unwrap();
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
    assert_eq!(answered(sid), None, "人答了 ⇒ 不留自动答的记录");
}

#[tokio::test(start_paused = true)]
async fn the_terminal_answering_first_means_no_decision_and_no_record() {
    let sid = "t-auto-term";
    let desk = Desk::new();
    let h = hang(desk.conn(), hook_of(sid, "AskUserQuestion"), on(5, None)).await;
    drop(desk); // 终端那边先答了：hook 退了、连接断了
    pass(6, 0).await;
    assert_eq!(h.await.unwrap().decision, None);
    assert_eq!(answered(sid), None);
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
        assert_eq!(answered(&sid), None, "{tool}");
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
        assert_eq!(answered(&sid), None);
    }
}

#[test]
fn the_setting_is_checked_before_it_is_written() {
    assert_eq!(auto_checked(on(0, Some("x")).unwrap()), Ok(None), "0 ＝ 关");
    assert!(auto_checked(on(AUTO_MAX_MIN + 1, None).unwrap()).is_err());
    assert_eq!(
        auto_checked(on(3, Some("  继续  ")).unwrap()),
        Ok(on(3, Some("继续")))
    );
    assert_eq!(auto_checked(on(3, Some("   ")).unwrap()), Ok(on(3, None)));
    assert!(auto_checked(on(3, Some(&"字".repeat(501))).unwrap()).is_err());
    // 到点时 hook 还活着：至多等的那几分钟短于那一家给 hook 的期限。
    assert!(u64::from(AUTO_MAX_MIN) * 60 < crate::agents::claudecode::asks::HOOK_TIMEOUT_SECS);
}
