//! `session-window`（会话在这台电脑上的哪儿：窗口标签 · 进程链，或一条原因）的判据。
//!
//! ★ 本机会话与远端会话同一个形：远端 ⇒ 本机后端经 `--resident-call` 问那台的 `session-terminals`，
//! 那台报来的终端连接拿本机的连接表对出进程链；本机 ⇒ 链从显示它的那个终端进程（tmux 客户端 · 不在 tmux 里就是它自己）起。
//! 两边的原因照搬，那台没问到 ⇒ 那一句照摆。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};
use crate::observe::session_terminals::{Here, SshConnection, Terminal};
use serde_json::json;

const SID: &str = "0b9c3f2e-1111-4222-8333-944455556666";

fn req(machine: &str) -> SessionWindowReq {
    serde_json::from_value(json!({"machine": machine, "sid": SID})).expect("入参")
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
        window: window.map(str::to_string),
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
    )
    .await;
    (got, peers)
}

#[tokio::test]
async fn a_remote_session_asks_that_machines_resident_then_matches_the_connection_here() {
    let shown = json!({"terminals": [ssh_terminal(Some("600-20"))]});
    let (got, peers) = remote_answer(shown).await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v["labels"], json!(["600-20"]), "{v}");
    assert_eq!(pids(&v), vec![700, 600, 500], "开着那条连接的进程往上：{v}");
    assert!(v.get("why").is_none(), "{v}");
    let asked = peers.asked();
    assert_eq!(asked.len(), 1, "只问那台一次");
    assert!(asked[0].0.contains("--resident-call"), "{}", asked[0].0);
    assert!(Peers::within_ms(&asked[0].0) <= 9_000, "期限是发起方的");
    assert_eq!(asked[0].1["cmd"], "session-terminals");
    assert_eq!(asked[0].1["args"], json!({"sid": SID}));
}

#[tokio::test]
async fn that_machines_reason_is_passed_on_and_nothing_is_matched_here() {
    for why in ["detached", "no-terminal", "unreadable"] {
        let (got, _) = remote_answer(json!({"terminals": [], "why": why})).await;
        let v = serde_json::to_value(got.expect("答得出")).unwrap();
        assert_eq!(v, json!({"labels": [], "chain": [], "why": why}), "{why}");
    }
}

#[tokio::test]
async fn a_connection_not_opened_here_keeps_the_labels_and_says_why() {
    let mut t = ssh_terminal(Some("600-20"));
    t.ssh.as_mut().unwrap().client_addr = "203.0.113.8".into();
    let (got, _) = remote_answer(json!({"terminals": [t]})).await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(
        v,
        json!({"labels": ["600-20"], "chain": [], "why": "elsewhere", "addr": "203.0.113.8"}),
        "标签照带（壳先按它找）；链对不上说为什么"
    );
}

#[tokio::test]
async fn that_machine_not_reached_is_its_own_code_with_the_sentence_as_said() {
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
    )
    .await;
    match got {
        Err(SessionWindowErr::Unreached(said)) => assert!(said.contains("devbox"), "{said}"),
        other => panic!("那台常驻不在 ⇒ unreached：{other:?}"),
    }
}

#[tokio::test]
async fn a_local_session_starts_the_chain_at_the_terminal_showing_it_and_asks_no_other_machine() {
    let peers = Peers::new(&[]);
    let t = Peers::table(&[]);
    let local = Terminal {
        ssh: None,
        activity: Some(9),
        window: Some("600-20".into()),
    };
    let got = answer_with(
        &req(cmd_wire::LOCAL),
        move |sid| {
            assert_eq!(sid, SID);
            Ok(Here::By(vec![(4242, local)]))
        },
        tables,
        &Default::default(),
        until(9_000),
        &t,
        &peers,
    )
    .await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v["labels"], json!(["600-20"]), "{v}");
    assert_eq!(pids(&v), vec![4242, 600, 500], "链从 tmux 客户端起：{v}");
    assert!(peers.asked().is_empty(), "本机会话不问别的台");
}

#[tokio::test]
async fn a_local_session_nobody_is_attached_to_is_detached() {
    let got = answer_with(
        &req(cmd_wire::LOCAL),
        |_| Ok(Here::Detached),
        tables,
        &Default::default(),
        None,
        &Peers::table(&[]),
        &Peers::new(&[]),
    )
    .await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v, json!({"labels": [], "chain": [], "why": "detached"}));
}

#[tokio::test]
async fn a_local_terminal_process_missing_from_the_table_is_query_failed() {
    let got = answer_with(
        &req(cmd_wire::LOCAL),
        |_| {
            Ok(Here::By(vec![(
                9999,
                Terminal {
                    ssh: None,
                    activity: None,
                    window: None,
                },
            )]))
        },
        tables,
        &Default::default(),
        None,
        &Peers::table(&[]),
        &Peers::new(&[]),
    )
    .await;
    let v = serde_json::to_value(got.expect("答得出")).unwrap();
    assert_eq!(v, json!({"labels": [], "chain": [], "why": "query-failed"}));
}
