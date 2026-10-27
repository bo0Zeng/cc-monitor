//! `drift-merge` 并好再答那一层（[`super::drift_hub`]）的判据：界面只问本机后端一条，
//! 各台的记录账由本机后端问到（这里喂假的「问那台」，记下问了谁）。

use super::drift_hub;
use crate::agents::drift::{DriftFaceReport, DriftReport};
use std::sync::{Arc, Mutex};

type Asked = Arc<Mutex<Vec<String>>>;

fn report(face: &str) -> DriftReport {
    DriftReport {
        faces: vec![DriftFaceReport {
            face: face.to_string(),
            consequence: "不显示".to_string(),
            entries: Vec::new(),
            overflowed: false,
        }],
    }
}

/// 假的「问那台」：`answers` 里有那台 ⇒ 答那一份，没有 ⇒ 问不到。问了谁记进 `asked`。
fn there(
    answers: &[(&str, DriftReport)],
    asked: &Asked,
) -> impl Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<DriftReport>> + Send>> {
    let answers: Vec<(String, DriftReport)> =
        answers.iter().map(|(n, a)| (n.to_string(), a.clone())).collect();
    let asked = asked.clone();
    move |m| {
        asked.lock().unwrap().push(m.clone());
        let got = answers.iter().find(|(n, _)| *n == m).map(|(_, a)| a.clone());
        Box::pin(async move { got })
    }
}

#[tokio::test]
async fn this_machine_first_then_each_in_table_order_and_the_unanswered_are_null() {
    let asked: Asked = Arc::default();
    let got = drift_hub(
        vec!["devbox".to_string(), "gpu-01".to_string(), "nano".to_string()],
        report("mine"),
        there(&[("nano", report("nano")), ("devbox", report("devbox"))], &asked),
    )
    .await;
    let rows: Vec<(String, Option<DriftReport>)> =
        got.machines.into_iter().map(|m| (m.origin, m.report)).collect();
    assert_eq!(
        rows,
        vec![
            ("<local>".to_string(), Some(report("mine"))),
            ("devbox".to_string(), Some(report("devbox"))),
            ("gpu-01".to_string(), None),
            ("nano".to_string(), Some(report("nano"))),
        ],
        "本机在前、其余照机器表的顺序；问不到的那台是 null（壳照实写读不到）"
    );
    let mut who = asked.lock().unwrap().clone();
    who.sort();
    assert_eq!(who, vec!["devbox", "gpu-01", "nano"], "各台各问一次，本机不经通道");
}

#[tokio::test]
async fn a_table_naming_this_machine_or_twice_asks_each_once() {
    let asked: Asked = Arc::default();
    let got = drift_hub(
        vec!["<local>".to_string(), "devbox".to_string(), "devbox".to_string()],
        report("mine"),
        there(&[("devbox", report("devbox"))], &asked),
    )
    .await;
    assert_eq!(
        got.machines.iter().map(|m| m.origin.as_str()).collect::<Vec<_>>(),
        vec!["<local>", "devbox"]
    );
    assert_eq!(*asked.lock().unwrap(), vec!["devbox".to_string()]);
}
