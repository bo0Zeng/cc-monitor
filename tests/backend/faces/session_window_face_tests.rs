//! `session-window`（会话在这台电脑上的哪儿：窗口标签 · 进程链，或写好的那一张）的判据。
//!
//! ★ 本机会话与远端会话同一个形：远端 ⇒ 本机后端经 `--resident-call` 问那台的 `session-terminals`，
//! 那台报来的终端连接拿本机的连接表对出进程链；本机 ⇒ 链从显示它的那个终端进程（tmux 客户端 · 不在 tmux 里就是它自己）起。
//! 两边的原因照搬、用那台的名字写成那一张（`face_core::front::Unplaced::face`，句子在 `face-core` 的 `front_tests.rs` 判）；
//! 那台没问到 ⇒ 连接层那一句照摆。在不在 tmux 里写进 `missed`（窗口关了那张才带［在终端里打开］）。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};
use crate::observe::session_terminals::{Here, SshConnection, Terminal};
use serde_json::json;

const SID: &str = "0b9c3f2e-1111-4222-8333-944455556666";

fn req(machine: &str) -> SessionWindowReq {
    serde_json::from_value(json!({"machine": machine, "sid": SID})).expect("入参")
}

/// 那一张照线上形（判这里选了哪一种原因、用了哪台的名字）。
fn face_of(why: Unplaced, machine: &str) -> serde_json::Value {
    serde_json::to_value(why.face(machine)).unwrap()
}

/// 窗口那一跳没成的那几张照线上形（判这里照会话在不在 tmux 写了哪一份）。
fn missed_of(tmux: bool) -> serde_json::Value {
    serde_json::to_value(face_core::front::missed_faces(tmux)).unwrap()
}

/// 这台的连接表 ＋ 进程表：700（ssh）开着 192.0.2.5:62414 → 192.0.2.9:22，往上 600（shell）· 500（终端）；4242 是 tmux 客户端。
fn tables() -> Result<String, String> {
    Ok(json!({
        "tcp": [{"la": "192.0.2.5", "lp": 62414, "ra": "192.0.2.9", "rp": 22, "pid": 700}],
        "proc": [
            {"pid": 700, "ppid": 600, "name": "ssh", "start": 30},
            {"pid": 600, "ppid": 500, "name": "bash", "start": 20},
            {"pid": 500, "ppid": 1, "name": "xterm", "start": 10},
            {"pid": 4242, "ppid": 600, "name": "tmux", "start": 25},
        ],
    })
    .to_string())
}

fn ssh_terminal(window: Option<&str>) -> Terminal {
    Terminal {
        ssh: Some(SshConnection {
            client_addr: "192.0.2.5".into(),
            client_port: 62414,
            server_addr: "192.0.2.9".into(),
            server_port: 22,
        }),
        activity: Some(9),
        window: window.map(|w| w.parse().unwrap()),
    }
}

fn pids(v: &serde_json::Value) -> Vec<u64> {
    v["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["pid"].as_u64().unwrap())
        .collect()
}

async fn remote_answer(
    shown: serde_json::Value,
) -> (Result<SessionWindow, SessionWindowErr>, Peers) {
    let peers = Peers::new(&[("devbox", shown.to_string())]);
    let t = Peers::table(&["devbox"]);
    let got = answer_with(
        &req("devbox"),
        |_| unreachable!("远端会话不读本机的会话"),
        tables,
        &Default::default(),
        until(9_000),
        &t,
        &peers,
        Shelf::empty(),
    )
    .await;
    (got, peers)
}

async fn local_answer(
    here: Result<Option<Here>, SessionWindowErr>,
) -> Result<SessionWindow, SessionWindowErr> {
    answer_with(
        &req(cmd_wire::LOCAL),
        move |_| here,
        tables,
        &Default::default(),
        None,
        &Peers::table(&[]),
        &Peers::new(&[]),
        Shelf::empty(),
    )
    .await
}

#[tokio::test]
async fn a_remote_session_asks_that_machines_resident_then_matches_the_connection_here() {
    let shown = json!({"terminals": [ssh_terminal(Some("600-20"))], "tmux": true});
    let (got, peers) = remote_answer(shown).await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v["labels"], json!(["600-20"]), "{v}");
    assert_eq!(pids(&v), vec![700, 600, 500], "开着那条连接的进程往上：{v}");
    assert_eq!(
        v["missed"],
        missed_of(true),
        "那台说在 tmux 里 ⇒ 窗口关了那张带［在终端里打开］：{v}"
    );
    assert!(v.get("face").is_none(), "{v}");
    let asked = peers.asked();
    assert_eq!(asked.len(), 1, "只问那台一次");
    assert!(asked[0].0.contains("--resident-call"), "{}", asked[0].0);
    assert!(Peers::within_ms(&asked[0].0) <= 9_000, "期限是发起方的");
    assert_eq!(asked[0].1["cmd"], "session-terminals");
    assert_eq!(asked[0].1["args"], json!({"sid": SID}));
}

#[tokio::test]
async fn that_machines_reason_is_written_with_its_name_and_nothing_is_matched_here() {
    for (why, unplaced, tmux) in [
        ("detached", Unplaced::Detached, true),
        ("no-terminal", Unplaced::NoTerminal, false),
        ("unreadable", Unplaced::Unreadable, false),
    ] {
        let (got, _) = remote_answer(json!({"terminals": [], "why": why, "tmux": tmux})).await;
        let v = serde_json::to_value(got.expect("答得出")).unwrap();
        assert_eq!(
            v,
            json!({"labels": [], "chain": [], "missed": missed_of(tmux), "face": face_of(unplaced, "devbox")}),
            "{why}"
        );
    }
}

#[tokio::test]
async fn an_older_machine_without_the_tmux_cell_counts_as_not_in_tmux() {
    let (got, _) = remote_answer(json!({"terminals": [], "why": "no-terminal"})).await;
    assert_eq!(
        serde_json::to_value(got.expect("答得出")).unwrap()["missed"],
        missed_of(false)
    );
}

#[tokio::test]
async fn a_connection_not_opened_here_keeps_the_labels_and_says_why() {
    let mut t = ssh_terminal(Some("600-20"));
    t.ssh.as_mut().unwrap().client_addr = "203.0.113.8".into();
    let (got, _) = remote_answer(json!({"terminals": [t]})).await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(
        v,
        json!({
            "labels": ["600-20"],
            "chain": [],
            "missed": missed_of(false),
            "face": face_of(Unplaced::Elsewhere { addr: "203.0.113.8".into() }, "devbox"),
        }),
        "标签照带（壳先按它找）；链对不上说为什么"
    );
}

#[tokio::test]
async fn that_machine_not_reached_is_that_face_with_the_sentence_as_said() {
    let peers = Peers::new(&[]);
    let t = Peers::table(&["devbox"]);
    let got = answer_with(
        &req("devbox"),
        |_| unreachable!(),
        tables,
        &Default::default(),
        until(9_000),
        &t,
        &peers,
        Shelf::empty(),
    )
    .await
    .expect("没问到那台是那一张，不是码");
    assert!(got.chain.is_empty());
    let face = got.face.expect("那一张");
    assert_eq!(
        face.acts,
        vec![cmd_wire::front::FrontAct::Reconnect],
        "{face:?}"
    );
    assert!(face.body.0.contains("devbox"), "连接层那一句照摆：{face:?}");
}

#[test]
fn what_that_machine_said_when_it_was_not_asked() {
    let said = |code: Option<&str>| match code {
        Some(c) => Said::new(c, "那一句".to_string()),
        None => Said::from("那一句"),
    };
    assert_eq!(
        not_asked(said(Some("unknown_command"))).unwrap(),
        Unplaced::PeerOld
    );
    assert_eq!(
        not_asked(said(Some(cmd_wire::SHAPE_DIFFERS))).unwrap(),
        Unplaced::PeerOld
    );
    assert_eq!(
        not_asked(said(Some(crate::wire::listen::CALL_TIMED_OUT))).unwrap(),
        Unplaced::PeerTimedOut
    );
    assert_eq!(
        not_asked(said(Some("no_such_session"))).unwrap(),
        Unplaced::NotLive
    );
    assert_eq!(
        not_asked(said(None)).unwrap(),
        Unplaced::Unreached {
            said: "那一句".into()
        }
    );
    assert!(matches!(
        not_asked(said(Some("failed"))),
        Err(SessionWindowErr::Failed(_))
    ));
    assert!(matches!(
        not_asked(said(Some(crate::platform::child::TIMED_OUT))),
        Err(SessionWindowErr::ChildTimedOut(_))
    ));
}

#[tokio::test]
async fn a_local_session_starts_the_chain_at_the_terminal_showing_it_and_asks_no_other_machine() {
    let peers = Peers::new(&[]);
    let t = Peers::table(&[]);
    let local = Terminal {
        ssh: None,
        activity: Some(9),
        window: "600-20".parse().ok(),
    };
    let got = answer_with(
        &req(cmd_wire::LOCAL),
        move |sid| {
            assert_eq!(sid, SID);
            Ok(Some(Here::By {
                seen: vec![(4242, local)],
                tmux: true,
            }))
        },
        tables,
        &Default::default(),
        until(9_000),
        &t,
        &peers,
        Shelf::empty(),
    )
    .await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v["labels"], json!(["600-20"]), "{v}");
    assert_eq!(pids(&v), vec![4242, 600, 500], "链从 tmux 客户端起：{v}");
    assert_eq!(v["missed"], missed_of(true), "{v}");
    assert!(peers.asked().is_empty(), "本机会话不问别的台");
}

#[tokio::test]
async fn a_local_session_nobody_is_attached_to_is_detached_in_tmux() {
    let v = serde_json::to_value(
        local_answer(Ok(Some(Here::Detached)))
            .await
            .expect("答得出"),
    )
    .unwrap();
    let here = copy_core::local_machine();
    assert_eq!(
        v,
        json!({"labels": [], "chain": [], "missed": missed_of(true), "face": face_of(Unplaced::Detached, &here)})
    );
}

#[tokio::test]
async fn a_local_session_not_running_is_that_face_not_a_code() {
    let v = serde_json::to_value(local_answer(Ok(None)).await.expect("答得出")).unwrap();
    let here = copy_core::local_machine();
    assert_eq!(
        v,
        json!({"labels": [], "chain": [], "missed": missed_of(false), "face": face_of(Unplaced::NotLive, &here)})
    );
}

#[tokio::test]
async fn a_local_terminal_process_missing_from_the_table_is_query_failed() {
    let got = local_answer(Ok(Some(Here::By {
        seen: vec![(
            9999,
            Terminal {
                ssh: None,
                activity: None,
                window: None,
            },
        )],
        tmux: false,
    })))
    .await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    let here = copy_core::local_machine();
    assert_eq!(
        v,
        json!({"labels": [], "chain": [], "missed": missed_of(false), "face": face_of(Unplaced::QueryFailed, &here)})
    );
}

// ───── 本机开过的那个终端：先照记下的标签在本机找，对不上才问那台 ─────

use crate::dial::opened_windows::{Opened, Records};
use std::sync::{Arc, Mutex};

/// 替身记录：至多一份（`sid` 不论），记下删过哪几份。
#[derive(Clone, Default)]
struct Shelf {
    held: Arc<Mutex<Option<Opened>>>,
    forgot: Arc<Mutex<Vec<String>>>,
}

impl Shelf {
    fn empty() -> Shelf {
        Shelf::default()
    }
    fn holding(label: &str, machine: &str) -> Shelf {
        let s = Shelf::default();
        *s.held.lock().unwrap() = Some(Opened {
            label: label.parse().unwrap(),
            machine: machine.into(),
        });
        s
    }
    fn forgot(&self) -> Vec<String> {
        self.forgot.lock().unwrap().clone()
    }
}

impl Records for Shelf {
    fn get(&self, sid: &str) -> Option<Opened> {
        assert_eq!(sid, SID);
        self.held.lock().unwrap().clone()
    }
    fn forget(&self, sid: &str) {
        self.forgot.lock().unwrap().push(sid.to_string());
    }
}

/// 那台永远不答（问了就一直等）：数问了几次。
#[derive(Default)]
struct Silent(Mutex<usize>);

impl crate::dial::remote_ask::Remote for Silent {
    fn run<'a>(
        &'a self,
        _dial: &'a serde_json::Value,
        _command: String,
        _stdin: Option<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        unreachable!("问常驻走 run_coded")
    }
    fn run_coded<'a>(
        &'a self,
        _dial: &'a serde_json::Value,
        _command: String,
        _stdin: Option<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, Said>> + Send + 'a>>
    {
        *self.0.lock().unwrap() += 1;
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn a_terminal_opened_here_is_found_here_even_when_that_machine_never_answers() {
    let silent = Silent::default();
    let got = tokio::time::timeout(
        guard_core::wall::CEILING,
        answer_with(
            &req("devbox"),
            |_| unreachable!("远端会话不读本机的会话"),
            tables,
            &Default::default(),
            until(9_000),
            &Peers::table(&["devbox"]),
            &silent,
            Shelf::holding("600-20", "devbox"),
        ),
    )
    .await
    .expect("本机开过的那一个不等那台");
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v["labels"], json!(["600-20"]), "{v}");
    assert_eq!(
        pids(&v),
        vec![700, 600, 500],
        "从那个 shell 底下的 ssh 往上：{v}"
    );
    assert!(v.get("face").is_none(), "{v}");
    assert_eq!(*silent.0.lock().unwrap(), 0, "不问那台");
}

async fn remote_answer_with(shelf: Shelf) -> (SessionWindow, Peers) {
    let shown = json!({"terminals": [ssh_terminal(Some("600-20"))], "tmux": true});
    let peers = Peers::new(&[("devbox", shown.to_string())]);
    let got = answer_with(
        &req("devbox"),
        |_| unreachable!(),
        tables,
        &Default::default(),
        until(9_000),
        &Peers::table(&["devbox"]),
        &peers,
        shelf,
    )
    .await
    .expect("答得出");
    (got, peers)
}

#[tokio::test]
async fn a_record_written_for_another_machine_is_not_used() {
    let shelf = Shelf::holding("600-20", "elsewhere");
    let (got, peers) = remote_answer_with(shelf.clone()).await;
    assert_eq!(peers.asked().len(), 1, "照今天问那台");
    assert_eq!(
        serde_json::to_value(&got).unwrap()["missed"],
        missed_of(true),
        "那台的答话照用"
    );
    assert!(shelf.forgot().is_empty(), "别台的那一份不删");
}

#[tokio::test]
async fn a_shell_that_is_gone_is_forgotten_and_that_machine_is_asked() {
    let shelf = Shelf::holding("600-99", "devbox");
    let (got, peers) = remote_answer_with(shelf.clone()).await;
    assert_eq!(peers.asked().len(), 1);
    assert_eq!(
        got.chain.iter().map(|l| l.pid).collect::<Vec<_>>(),
        vec![700, 600, 500]
    );
    assert_eq!(
        shelf.forgot(),
        vec![SID.to_string()],
        "起始时刻对不上 ⇒ 那一份删"
    );
}

#[tokio::test]
async fn a_shell_still_open_with_no_ssh_under_it_asks_that_machine_and_keeps_the_record() {
    let shelf = Shelf::holding("500-10", "devbox");
    let (_, peers) = remote_answer_with(shelf.clone()).await;
    assert_eq!(peers.asked().len(), 1);
    assert!(
        shelf.forgot().is_empty(),
        "窗口还开着 ⇒ 留着（下一回也许又开着 ssh）"
    );
}
