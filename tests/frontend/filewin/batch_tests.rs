//! `batch.rs` 的判据：一组带票 · 同名照 `data.conflicts` 问一次（默认跳过）· 断了等连回来、同一张票再问 · 撤带票 · 结局照后端画。
//! 后端那一半是替身（按剧本回），真后端那一半的判据住 `tests/backend/control/files_batch_tests.rs`。

use super::*;
use batch_core::{copy_details, run, Failed, Rebuff, Verb, EXISTS};
use cmd_wire::batch::ItemState;
use std::sync::atomic::{AtomicUsize, Ordering};

fn plan() -> Plan {
    Plan {
        verb: Verb::Copy,
        cmd: "files-copy",
        args: serde_json::json!({"root": "/r", "items": [{"from": "a", "to": "b/a"}, {"from": "c", "to": "b/c"}]}),
        names: vec!["a".into(), "c".into()],
        into: Some("/r/b".into()),
        machine: "devbox".into(),
    }
}

fn clash() -> Rebuff {
    Rebuff {
        failed: Failed {
            code: Some(EXISTS.into()),
            said: "同名 2 项".into(),
            detail: String::new(),
        },
        data: Some(serde_json::json!({"conflicts": [
            {"key": "0", "name": "a", "kind": "file", "size": 3, "size_text": "3 字节", "mtime_text": "14:02",
             "there": "file", "there_size": 1, "there_size_text": "1 字节", "there_mtime_text": "10-01 09:10", "answers": ["overwrite", "skip", "keep_both"]},
            {"key": "1", "name": "c", "kind": "dir", "there": "dir", "answers": ["skip", "keep_both"]},
        ]})),
        cut: false,
    }
}

fn done(said: &str) -> serde_json::Value {
    serde_json::json!({"outcomes": [
        {"key": "0", "state": "done"},
        {"key": "1", "state": "failed", "code": "refused", "reason": copy_text("reason.io.denied", &[]), "detail": "那台说：EACCES"},
    ], "said": said})
}

async fn until(f: impl Fn() -> bool) {
    for _ in 0..2000 {
        if f() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("等不到");
}

#[tokio::test]
async fn every_group_carries_a_ticket_minted_by_the_window() {
    let board = BatchBoard::new(plan());
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let s2 = seen.clone();
    run(
        &board,
        |a| {
            s2.lock().unwrap().push(a);
            async { Ok(done("复制 2 项 · 完成 1 · 跳过 0 · 失败 ×1")) }
        },
        || async {},
    )
    .await;
    let t = seen.lock().unwrap()[0]["ticket"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(t, board.ticket());
    assert!(uuid::Uuid::parse_str(&t).is_ok(), "票是一张 UUID：{t}");
    assert_ne!(BatchBoard::new(plan()).ticket(), t, "一组一张");
}

#[tokio::test]
async fn a_clash_is_asked_once_from_the_conflicts_and_skip_is_the_default() {
    let board = BatchBoard::new(plan());
    let calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let c2 = calls.clone();
    let b2 = board.clone();
    let task = tokio::spawn(async move {
        run(
            &b2,
            move |a| {
                let first = c2.lock().unwrap().is_empty();
                c2.lock().unwrap().push(a);
                async move {
                    if first {
                        Err(clash())
                    } else {
                        Ok(done("x"))
                    }
                }
            },
            || async {},
        )
        .await
    });
    until(|| board.is_asking()).await;
    let c = board.conflicts();
    assert_eq!(c.len(), 2, "照 data.conflicts 画：{c:?}");
    assert!(
        !c[1].answers.contains(&Decide::Overwrite),
        "目录同名那一行不给覆盖"
    );
    assert_eq!(
        conflict_row(&c[1]).meta,
        copy_text("rsFilewinBatch.ask.dirNoMerge", &[])
    );
    // 两头的大小与时刻照抄后端写好的字（按看的那一台的钟），窗口不换算、不另写。
    let sep = copy_text("rsFilewinCopy.batch.listSep", &[]);
    assert_eq!(
        conflict_row(&c[0]).meta,
        copy_text(
            "rsFilewinBatch.meta.vs",
            &[
                ("mine", &format!("3 字节{sep}14:02")),
                ("there", &format!("1 字节{sep}10-01 09:10")),
            ]
        )
    );
    // 打开时焦点落在「跳过同名」（不毁东西的那一个）；按下它 ⇒ 整组都答跳过。
    assert_eq!(DEFAULT_ANSWER, 1);
    assert!(board.answer(Some(Decide::Skip), false));
    task.await.unwrap();
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "只问一次，答完再发一次");
    assert_eq!(
        calls[0]["ticket"], calls[1]["ticket"],
        "答完那一发是同一张票"
    );
    assert_eq!(
        calls[1]["decide"],
        serde_json::json!({"0": "skip", "1": "skip"})
    );
    assert!(matches!(board.phase(), Phase::Ended(_)));
}

#[tokio::test]
async fn picking_each_uses_the_per_row_answers_and_cancel_does_nothing() {
    let board = BatchBoard::new(plan());
    let b2 = board.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let c2 = calls.clone();
    let task = tokio::spawn(async move {
        run(
            &b2,
            move |_| {
                c2.fetch_add(1, Ordering::SeqCst);
                async { Err(clash()) }
            },
            || async {},
        )
        .await
    });
    until(|| board.is_asking()).await;
    assert!(board.answer(None, false), "冲突框上答了取消却没送出去");
    task.await.unwrap();
    assert_eq!(board.phase(), Phase::Dropped, "取消 ⇒ 一项都没做");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let board = BatchBoard::new(plan());
    let b2 = board.clone();
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let s2 = seen.clone();
    let task = tokio::spawn(async move {
        run(
            &b2,
            move |a| {
                let first = s2.lock().unwrap().is_empty();
                s2.lock().unwrap().push(a);
                async move {
                    if first {
                        Err(clash())
                    } else {
                        Ok(done("x"))
                    }
                }
            },
            || async {},
        )
        .await
    });
    until(|| board.is_asking()).await;
    board.pick("0", Decide::Overwrite);
    assert!(board.answer(None, true));
    task.await.unwrap();
    assert_eq!(
        seen.lock().unwrap()[1]["decide"],
        serde_json::json!({"0": "overwrite", "1": "skip"}),
        "逐项选：点了的照点的，没点的跳过"
    );
}

#[tokio::test]
async fn a_cut_waits_for_the_link_and_asks_again_with_the_same_ticket() {
    let board = BatchBoard::new(plan());
    let tickets: Arc<Mutex<Vec<String>>> = Arc::default();
    let t2 = tickets.clone();
    let ups = Arc::new(AtomicUsize::new(0));
    let u2 = ups.clone();
    let b2 = board.clone();
    run(
        &board,
        move |a| {
            let n = {
                let mut g = t2.lock().unwrap();
                g.push(a["ticket"].as_str().unwrap().to_string());
                g.len()
            };
            async move {
                if n == 1 {
                    Err(Rebuff {
                        failed: "断了".into(),
                        data: None,
                        cut: true,
                    })
                } else {
                    Ok(done("复制 2 项 · 完成 1 · 跳过 0 · 失败 ×1"))
                }
            }
        },
        move || {
            // 等连回来那一刻：这一组摆着「其余待核」。
            assert!(matches!(b2.phase(), Phase::Cut(_)), "{:?}", b2.phase());
            u2.fetch_add(1, Ordering::SeqCst);
            async {}
        },
    )
    .await;
    let t = tickets.lock().unwrap().clone();
    assert_eq!(t.len(), 2);
    assert_eq!(t[0], t[1], "断开后带同一张票再问");
    assert_eq!(ups.load(Ordering::SeqCst), 1, "先等那台连回来");
    let Phase::Ended(r) = board.phase() else {
        panic!("{:?}", board.phase())
    };
    assert_eq!(
        r.said, "复制 2 项 · 完成 1 · 跳过 0 · 失败 ×1",
        "结局直接是后端那一行"
    );
    assert_eq!(
        board.failed_lines(),
        vec![copy_text(
            "rsFilewinProgress.detail.failedOne",
            &[("name", "c"), ("why", &copy_text("reason.io.denied", &[]))]
        )],
        "没成的那一项照 outcomes 列"
    );
    assert!(copy_details(&board.plan(), &r).unwrap().contains("EACCES"));
}

#[test]
fn stopping_goes_through_the_stopper_once() {
    let board = BatchBoard::new(plan());
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    board.set_stopper(Arc::new(move || {
        n2.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(board.stop());
    assert!(!board.stop(), "第二下不再发");
    assert_eq!(n.load(Ordering::SeqCst), 1);
}

// ════════════════════════════════════════════════════════════════════════
// 「停」那一跳，端到端：进度表上按下「停」→ 线上一条 `files-batch-stop`（同一张票）→ 那一组照回包收场
// ════════════════════════════════════════════════════════════════════════

/// 合成对端：一组删除在做（不回），直到收到同一张票的 `files-batch-stop` 才回 —— 手上那一项做完、剩下的 `cancelled`。
struct StopHost {
    log: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    stop: Arc<tokio::sync::Notify>,
}

impl comms_inward::chan::router::Backends for StopHost {
    fn call(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        payload: comms_inward::chan::wire::Body,
        _view: Option<serde_json::Value>,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<comms_inward::chan::wire::Body, comms_inward::chan::wire::CallError>,
    > {
        let args: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        self.log.lock().unwrap().push((op.0.clone(), args.clone()));
        let (log, stop) = (self.log.clone(), self.stop.clone());
        let body =
            |v: serde_json::Value| comms_inward::chan::wire::Body(serde_json::to_vec(&v).unwrap());
        Box::pin(async move {
            match op.0.as_str() {
                "files-delete" => {
                    let ticket = args["ticket"].clone();
                    // 等到撤的那一条（同一张票）到了才收场：没有它，这一组永远在做。
                    loop {
                        let wait = stop.notified();
                        let stopped = log
                            .lock()
                            .unwrap()
                            .iter()
                            .any(|(c, a)| c == "files-batch-stop" && a["ticket"] == ticket);
                        if stopped {
                            break;
                        }
                        wait.await;
                    }
                    Ok(body(serde_json::json!({"outcomes": [
                        {"key": "0", "state": "done", "removed": 4096},
                        {"key": "1", "state": "cancelled"},
                        {"key": "2", "state": "cancelled"},
                    ], "said": "删除 3 项 · 完成 1 · 跳过 0 · 失败 ×0 · 已取消 2"})))
                }
                "files-batch-stop" => {
                    stop.notify_waiters();
                    Ok(body(serde_json::json!({"stopping": true})))
                }
                _ => Ok(body(serde_json::json!({}))),
            }
        })
    }

    fn subscribe(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        _kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        use futures::StreamExt as _;
        futures::stream::pending().boxed()
    }
}

/// 🔴 删一棵大树那一组在做：「进度」表上按下「停」（那颗按钮交出的就是 [`super::super::progress::Act::Stop`]，窗口那一级照它调这一处）
/// ⇒ 线上出现一条 `files-batch-stop`、票与那一组发出去的是同一张 ⇒ 对端收场，那一行照回包画：剩下的那几项 `cancelled`、整行是「已停」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pressing_stop_on_a_tree_delete_reaches_the_backend_with_its_ticket_and_the_rest_comes_back_cancelled(
) {
    use super::super::progress::{Progress, State, Trip};
    let log: Arc<Mutex<Vec<(String, serde_json::Value)>>> = Arc::default();
    let line = crate::find::testing::wire(
        Arc::new(StopHost {
            log: log.clone(),
            stop: Arc::default(),
        }),
        1 << 20,
    );
    let origin = comms_inward::chan::wire::Origin("判据机器·撤一组".into());
    let board = BatchBoard::new(Plan {
        verb: Verb::Delete,
        cmd: "files-delete",
        args: serde_json::json!({"root": "/srv", "items": [
            {"rel": "node_modules", "recursive": true}, {"rel": "dist", "recursive": true}, {"rel": "out", "recursive": true}]}),
        names: vec!["node_modules".into(), "dist".into(), "out".into()],
        into: None,
        machine: "devbox".into(),
    });
    let progress = Progress::default();
    let id = progress.add(
        Trip::Batch {
            board: board.clone(),
            icon: egui_phosphor::regular::TRASH,
        },
        Some("/srv".into()),
        None,
    );
    let run = tokio::spawn(go(line, origin.clone(), origin, board.clone()));
    let sent = |cmd: &str| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == cmd)
            .map(|(_, a)| a.clone())
            .collect::<Vec<_>>()
    };
    tokio::time::timeout(guard_core::wall::CEILING, async {
        while sent("files-delete").is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("那一组没发出去");
    assert_eq!(progress.jobs()[0].state(), State::Running);
    assert!(progress.stop(id), "「停」没按下去");
    tokio::time::timeout(guard_core::wall::CEILING, run)
        .await
        .expect("按了停，那一组一直没收场")
        .expect("任务没 panic");
    let stops = sent("files-batch-stop");
    assert_eq!(stops.len(), 1, "撤那一条不是恰好一次：{stops:?}");
    assert_eq!(
        stops[0],
        serde_json::json!({"ticket": board.ticket()}),
        "撤的不是那一组的票"
    );
    assert_eq!(sent("files-delete")[0]["ticket"], board.ticket());
    let Phase::Ended(r) = board.phase() else {
        panic!("收场不是一组的结局：{:?}", board.phase())
    };
    let left: Vec<ItemState> = r.outcomes.iter().map(|o| o.state).collect();
    assert_eq!(
        left,
        [ItemState::Done, ItemState::Cancelled, ItemState::Cancelled]
    );
    let v = progress.jobs()[0].view();
    assert_eq!(v.state, State::Stopped);
    assert_eq!(v.title, r.said, "那一行没照后端那一行画");
}

/// 下载 · 上传开单那两组不带票（开了单的每一项各是传输台里的一单，后端不按票记）：发出去的那一份照它的线上类型解得开
/// （多一个键就拒的那一种 —— 带了票，monitor 那一跳当场回「请求格式不对」）。
#[tokio::test]
async fn the_download_and_upload_openers_carry_no_ticket_and_decode_as_their_wire_type() {
    let ask = cmd_wire::transfer::TransferDownloadAsk {
        items: vec![cmd_wire::transfer::DownloadItem {
            remote_path: "/r/a".into(),
            name: None,
        }],
        local_dir: cmd_wire::files::WirePath::from("/tmp"),
        on_exists: None,
        decide: None,
    };
    let up = crate::transfer::group_ask(
        &[crate::transfer::Pending::into_remote_dir("/tmp/a.bin", "/srv").unwrap()],
        None,
        None,
    );
    for (verb, args) in [
        (Verb::Download, serde_json::to_value(&ask).unwrap()),
        (Verb::Upload, serde_json::to_value(&up).unwrap()),
    ] {
        let board = BatchBoard::new(Plan {
            verb,
            cmd: "x",
            args,
            names: vec!["a".into()],
            into: None,
            machine: "devbox".into(),
        });
        let sent: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let s2 = sent.clone();
        run(
            &board,
            |a| {
                s2.lock().unwrap().push(a);
                async { Ok(serde_json::json!({"items": []})) }
            },
            || async {},
        )
        .await;
        let got = sent.lock().unwrap()[0].clone();
        assert!(got.get("ticket").is_none(), "{verb:?} 那一组带了票：{got}");
        match verb {
            Verb::Download => {
                serde_json::from_value::<cmd_wire::transfer::TransferDownloadAsk>(got).unwrap();
            }
            _ => {
                serde_json::from_value::<cmd_wire::transfer::TransferUploadAsk>(got).unwrap();
            }
        }
        assert_eq!(board.phase(), Phase::Opened);
    }
}
