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
        rows: Vec::new(),
    }
}

type Asked = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// 假的「问那台」：`answers` 里有那台 ⇒ 答那一份，没有 ⇒ 问不到。问了谁、带什么记进 `asked`。
fn there(
    answers: &[(&str, R)],
    asked: &Asked,
) -> impl Fn(String, Q) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<R>> + Send>> {
    let answers: Vec<(String, R)> = answers
        .iter()
        .map(|(n, a)| (n.to_string(), a.clone()))
        .collect();
    let asked = asked.clone();
    move |m, q| {
        asked.lock().unwrap().push((
            m.clone(),
            serde_json::json!({"machine": q.machine, "appExit": q.app_exit, "fleet": q.fleet}),
        ));
        let got = answers
            .iter()
            .find(|(n, _)| *n == m)
            .map(|(_, a)| a.clone());
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
        &[],
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
            serde_json::json!({"machine": null, "appExit": null, "fleet": null})
        )],
        "问那台只带空的那一形（它只答它自己）"
    );
    assert_eq!(*here_saw.lock().unwrap(), vec![Some("devbox".to_string())]);

    let asked: Asked = Arc::default();
    let got = interrupts_hub(
        req(serde_json::json!({"machine": "devbox"})),
        &[],
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
            &[],
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
    let fleet = ["<local>", "devbox", "gpu"].map(cmd_wire::Machine::named);
    let asked: Asked = Arc::default();
    let alone = interrupts_hub(
        req(serde_json::json!({"appExit": true})),
        &fleet,
        |_q: Q| async move { r(Some(0), 0, 1, 2) },
        there(&[("devbox", r(Some(2), 1, 3, 1))], &asked),
    )
    .await;
    assert_eq!(alone, r(Some(0), 0, 1, 2), "不带 fleet ⇒ 只答这台");
    assert!(asked.lock().unwrap().is_empty());
    let got = interrupts_hub(
        req(serde_json::json!({"appExit": true, "fleet": true})),
        &fleet,
        |q: Q| async move {
            assert_eq!(q.app_exit, Some(true));
            assert_eq!(q.fleet, None);
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
                serde_json::json!({"machine": null, "appExit": true, "fleet": null})
            ),
            (
                "gpu".to_string(),
                serde_json::json!({"machine": null, "appExit": true, "fleet": null})
            ),
        ],
        "各台只问一次、只带 appExit（不带名单 ⇒ 不往下扇）；本机不经常驻流问"
    );
}

/// 「中断 · 保留」两行由后端排好（[`super::rows_of`]）：壳 · 界面只画。
mod rows {
    use super::super::rows_of;
    use super::{r, req};
    use cmd_wire::machine::InterruptRow;
    use copy_core::copy_text;

    fn t(key: &str) -> String {
        copy_text(key, &[])
    }

    fn n(key: &str, v: u32) -> String {
        copy_text(key, &[("n", &v.to_string())])
    }

    fn two(label: &str, items: Vec<String>) -> Vec<InterruptRow> {
        vec![
            InterruptRow {
                label: t(label),
                items,
            },
            InterruptRow {
                label: t("kit.interrupts.keep"),
                items: vec![t("interrupts.item.sessionsRun")],
            },
        ]
    }

    #[test]
    fn quitting_lists_file_windows_first_then_sessions_live_and_forwards_everywhere() {
        let q = req(
            serde_json::json!({"appExit": true, "fleet": true, "files": {"unsaved": 2, "running": 1}}),
        );
        let everywhere = t("restartNow.dialog.everywhere");
        assert_eq!(
            rows_of(&q, &r(Some(3), 1, 4, 5)),
            two(
                "interrupts.row.untilStart",
                vec![
                    n("interrupts.item.unsaved", 2),
                    n("interrupts.item.filewinRunning", 1),
                    n("interrupts.item.relayed", 3),
                    n("interrupts.item.relayedMaybe", 1),
                    copy_text("interrupts.item.liveGone", &[("machine", &everywhere)]),
                    n("interrupts.item.forwards", 5),
                ]
            ),
            "缺 act ⇒ 停到再启动；次序：文件窗口 · 会话 · 实时 · 转发"
        );
    }

    #[test]
    fn nothing_to_cut_is_no_rows_and_unknowns_are_said_not_zeroed() {
        let quit = req(serde_json::json!({"appExit": true, "files": {"unsaved": 0, "running": 0}}));
        assert_eq!(
            rows_of(&quit, &r(Some(0), 0, 0, 0)),
            Vec::new(),
            "什么都不会断 ⇒ 空"
        );
        let blind = req(serde_json::json!({"appExit": true}));
        assert_eq!(
            rows_of(&blind, &r(Some(0), 0, 0, 0)),
            two(
                "interrupts.row.untilStart",
                vec![t("interrupts.item.filewinUnknown")]
            ),
            "退出那一问没带文件窗口 ⇒ 说数不出"
        );
        let far = req(serde_json::json!({"machine": "devbox"}));
        assert_eq!(
            rows_of(&far, &r(None, 0, 0, 0)),
            two(
                "interrupts.row.untilStart",
                vec![t("interrupts.item.relayedUnknown")]
            ),
            "那台问不到 ⇒ 会话说不清；停一台不数文件窗口"
        );
    }

    #[test]
    fn restart_and_update_are_brief_and_name_the_machine() {
        for act in ["restart", "update"] {
            let q = req(serde_json::json!({"machine": "devbox", "act": act}));
            assert_eq!(
                rows_of(&q, &r(Some(0), 0, 2, 0)),
                two(
                    "interrupts.row.brief",
                    vec![copy_text(
                        "interrupts.item.liveBrief",
                        &[("machine", "devbox")]
                    )]
                ),
                "{act} ⇒ 中断几秒"
            );
        }
        let here = req(serde_json::json!({"act": "uninstall"}));
        let local = t("detail.label.local");
        assert_eq!(
            rows_of(&here, &r(Some(0), 0, 1, 0)),
            two(
                "interrupts.row.untilStart",
                vec![copy_text(
                    "interrupts.item.liveGone",
                    &[("machine", &local)]
                )]
            ),
            "本机那一台叫本机"
        );
    }
}

/// 这台那一份（[`super::interrupts_at`]）数「活着的会话」照活会话表那一判（会话脸上说它活着也照这一判），
/// 不照账号归属那一格更严的「启动时刻核上」：Windows 上 pidfile 的启动时刻记法与这台读的恒不等，核不上，
/// 那几条照样活着、照样会被打断。核不上 ⇒ 不读进程环境 ⇒ 走不走中转说不清，落 `relayedMaybe`；
/// 后台任务（`kind: bg`）不是脸上那一条会话，不数；同一条会话几个进程只数一个。
#[cfg(target_os = "linux")]
#[test]
fn live_sessions_are_counted_by_the_live_table_not_by_the_stricter_identity_check() {
    use std::os::unix::process::CommandExt;
    let home = test_home::scratch("interrupts-live");
    let sessions = crate::agents::IntoHomes::into_homes(&home)[0]
        .pidfile_dir()
        .unwrap();
    std::fs::create_dir_all(&sessions).unwrap();
    let mut kids = Vec::new();
    // 启动时刻写成另一种记法（核不上）、进程像那一家 ⇒ 活会话表说活着。
    for (sid, kind) in [
        ("s-a", None),
        ("s-a", None),
        ("s-b", None),
        ("s-bg", Some("bg")),
    ] {
        let kid = crate::platform::proc::test_support::spawn_settled_sleep(|c| {
            c.arg0("claude");
        });
        let kind = kind.map_or(String::new(), |k| format!(r#","kind":"{k}""#));
        std::fs::write(
            sessions.join(format!("{}.json", kid.id())),
            format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"638000000000000000"{kind}}}"#),
        )
        .unwrap();
        kids.push(kid);
    }
    let got = super::interrupts_at(&home, &Q::default());
    for mut k in kids {
        let _ = k.kill();
        let _ = k.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        (got.live_streams, got.relayed_maybe, got.relayed_sessions),
        (2, 2, Some(0)),
        "脸上活着两条会话（s-a 两个进程算一条 · 后台那一条不算），都说不清走不走中转：{got:?}"
    );
}
