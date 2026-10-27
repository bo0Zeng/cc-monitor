//! `machine-interrupts` 并好再答那一层（[`super::interrupts_hub`]）的判据：界面只问本机后端一条，
//! 别的台那一份由本机后端问到（这里喂假的「问那台」，记下问了谁、带什么）。

use super::{interrupts_hub, MachineInterruptsReply as R, MachineInterruptsReq as Q};
use std::sync::{Arc, Mutex};

fn r(relayed: Option<u32>, maybe: u32, live: u32, forwards: u32) -> R {
    R {
        relayed_sessions: relayed,
        relayed_maybe: maybe,
        live_streams: live,
        forwards,
    }
}

type Asked = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// 假的「问那台」：`answers` 里有那台 ⇒ 答那一份，没有 ⇒ 问不到。问了谁、带什么记进 `asked`。
fn there(
    answers: &[(&str, R)],
    asked: &Asked,
) -> impl Fn(String, Q) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<R>> + Send>> {
    let answers: Vec<(String, R)> = answers.iter().map(|(n, a)| (n.to_string(), *a)).collect();
    let asked = asked.clone();
    move |m, q| {
        asked.lock().unwrap().push((
            m.clone(),
            serde_json::json!({"machine": q.machine, "appExit": q.app_exit, "machines": q.machines.map(|v| v.len())}),
        ));
        let got = answers.iter().find(|(n, _)| *n == m).map(|(_, a)| *a);
        Box::pin(async move { got })
    }
}

fn req(v: serde_json::Value) -> Q {
    serde_json::from_value(v).unwrap()
}

#[tokio::test]
async fn stopping_another_machine_takes_its_sessions_and_this_machines_forwards() {
    let asked: Asked = Arc::default();
    let here_saw = Arc::new(Mutex::new(Vec::new()));
    let seen = here_saw.clone();
    let here = move |q: Q| {
        seen.lock().unwrap().push(q.machine.clone());
        async move { r(Some(9), 9, 9, 2) }
    };
    let got = interrupts_hub(
        req(serde_json::json!({"machine": "devbox"})),
        here.clone(),
        there(&[("devbox", r(Some(1), 1, 3, 0))], &asked),
    )
    .await;
    assert_eq!(
        got,
        r(Some(1), 1, 3, 2),
        "会话三格是那台的，转发是这台账上的"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        vec![(
            "devbox".to_string(),
            serde_json::json!({"machine": null, "appExit": null, "machines": null})
        )],
        "问那台只带空的那一形（它只答它自己）"
    );
    assert_eq!(*here_saw.lock().unwrap(), vec![Some("devbox".to_string())]);

    let asked: Asked = Arc::default();
    let got = interrupts_hub(
        req(serde_json::json!({"machine": "devbox"})),
        here,
        there(&[], &asked),
    )
    .await;
    assert_eq!(got, r(None, 0, 0, 2), "那台问不到 ⇒ 会话说不清，转发照数");
}

#[tokio::test]
async fn without_another_machine_only_this_machine_answers() {
    for q in [
        serde_json::json!({}),
        serde_json::json!({"machine": "<local>"}),
    ] {
        let asked: Asked = Arc::default();
        let got = interrupts_hub(
            req(q),
            |q: Q| async move {
                assert_eq!(q.machine, None);
                r(Some(1), 0, 1, 0)
            },
            there(&[("devbox", r(Some(5), 5, 5, 5))], &asked),
        )
        .await;
        assert_eq!(got, r(Some(1), 0, 1, 0));
        assert!(asked.lock().unwrap().is_empty(), "不问别的台");
    }
}

#[tokio::test]
async fn app_exit_adds_this_machine_and_each_listed_one_without_fanning_further() {
    let asked: Asked = Arc::default();
    let got = interrupts_hub(
        req(serde_json::json!({"appExit": true, "machines": ["devbox", "gpu", "devbox", "<local>"]})),
        |q: Q| async move {
            assert_eq!(q.app_exit, Some(true));
            assert!(q.machines.is_none());
            r(Some(0), 0, 1, 2)
        },
        there(&[("devbox", r(Some(2), 1, 3, 1))], &asked),
    )
    .await;
    assert_eq!(got, r(Some(2), 1, 4, 3), "答到的加起来，问不到的 gpu 不算");
    let mut who: Vec<_> = asked.lock().unwrap().clone();
    who.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        who,
        vec![
            (
                "devbox".to_string(),
                serde_json::json!({"machine": null, "appExit": true, "machines": null})
            ),
            (
                "gpu".to_string(),
                serde_json::json!({"machine": null, "appExit": true, "machines": null})
            ),
        ],
        "各台只问一次、只带 appExit（不带名单 ⇒ 不往下扇）；本机不经常驻流问"
    );
}
