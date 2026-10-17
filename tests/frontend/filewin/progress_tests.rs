//! 「进度」表（`progress.rs`）：一趟一行、各自的状态与那颗按钮、只留最近 50 行已完成、撤不动的那几类「停」灰着说为什么。
//! 期望全部手写；每一行的读数从它自己那块看板来（夹具直接摆看板的读数）。

use super::*;
use copy_core::copy_text;

fn upload(board: &DropBoard, n: usize) -> Trip {
    let items: Vec<super::super::transfer::Pending> = (0..n)
        .map(|k| {
            super::super::transfer::Pending::into_remote_dir(&format!("/tmp/f{k}.bin"), "/srv/data")
                .unwrap()
        })
        .collect();
    Trip::Upload {
        board: board.clone(),
        ask: crate::transfer::tests::upload_ask(&items),
        items,
        dir: "data".into(),
    }
}

/// 两摞上传、一趟下载同时在表里：各是各的一行，一摞落地不碰另两行；在跑的个数与合计进度只算在跑的。
#[test]
fn each_trip_is_its_own_row_and_one_landing_leaves_the_others_running() {
    let p = Progress::default();
    let (a, b) = (DropBoard::default(), DropBoard::default());
    let d = DownloadBoard::default();
    p.add(upload(&a, 2), Some("/srv/data".into()), None);
    p.add(upload(&b, 1), Some("/srv/data".into()), None);
    p.add(
        Trip::Download {
            board: d.clone(),
            name: "big.iso".into(),
            src: "/srv/big.iso".into(),
            dest: "/tmp/big.iso".into(),
        },
        None,
        None,
    );
    d.begin("big.iso");
    d.progress(25, 100);
    assert_eq!(p.running(), 3);
    a.finish(crate::transfer::DropOutcome {
        ok: 2,
        ..Default::default()
    });
    p.settle(0);
    let states: Vec<State> = p.jobs().iter().map(Job::state).collect();
    assert_eq!(states, vec![State::Done, State::Running, State::Running]);
    assert_eq!(p.running(), 2);
    assert_eq!(p.overall(), Some(0.25), "合计进度没只算在跑的那几趟");
    assert_eq!(
        p.jobs()[0].view().title,
        copy_text("rsFilewinProgress.upload.done", &[("n", "2")])
    );
    // 「清除已完成」只拿掉落了地的那一行。
    p.clear_finished();
    assert_eq!(p.jobs().len(), 2);
}

/// 上传失败 ⇒ 标题「上传失败 n」、按钮「重试失败的」交出**失败的那几个**（不是整摞）；
/// 人按了停 ⇒「已停」那一句、按钮「继续」交出没传的那几个（再开单，续传先对尾块）。
#[test]
fn a_failed_upload_offers_to_retry_exactly_the_ones_that_failed() {
    let p = Progress::default();
    let b = DropBoard::default();
    p.add(upload(&b, 3), Some("/srv/data".into()), None);
    b.finish(crate::transfer::DropOutcome {
        ok: 1,
        failed: vec![
            (
                "f1.bin".into(),
                copy_core::copy_text("reason.io.full", &[]).into(),
            ),
            (
                "f2.bin".into(),
                copy_core::copy_text("reason.io.full", &[]).into(),
            ),
        ],
        ..Default::default()
    });
    p.settle(0);
    let v = p.jobs()[0].view();
    assert_eq!(v.state, State::Failed);
    assert_eq!(
        v.title,
        copy_text("rsFilewinProgress.upload.failed", &[("n", "2")])
    );
    let Some((label, Ok(Act::RetryUpload(again)))) = v.button else {
        panic!("失败那一行没有「重试」：{:?}", v.button);
    };
    assert_eq!(label, copy_text("rsFilewinBatch.btn.retry", &[]));
    let names: Vec<&str> = again.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, vec!["f1.bin", "f2.bin"]);
    // 停了的那一摞。
    let s = DropBoard::default();
    p.add(upload(&s, 3), Some("/srv/data".into()), None);
    s.cancels().request();
    s.finish(crate::transfer::DropOutcome {
        ok: 1,
        failed: vec![
            ("f1.bin".into(), "已取消".into()),
            ("f2.bin".into(), "已取消".into()),
        ],
        ..Default::default()
    });
    let v = p.jobs()[1].view();
    assert_eq!(v.state, State::Stopped);
    assert_eq!(
        v.title,
        copy_text(
            "rsFilewinProgress.upload.stopped",
            &[("a", "1"), ("b", "2")]
        )
    );
    assert!(matches!(v.button, Some((ref l, Ok(Act::Resume(ref r))))
        if r.len() == 2 && *l == copy_text("rsFilewinBatch.btn.resume", &[])));
}

/// 一组的结局里有没成的 ⇒ 那一行带「重试失败的」：交出的那一组**只含没成的那几项**（同一台上的那几条照原样的根与参数），
/// 寻址同一台另起一组（窗口那一级照它新起一块看板 ⇒ 新铸一张票）；全成了 ⇒ 不带按钮。
#[test]
fn a_partly_failed_group_offers_to_retry_only_the_failed_items_under_a_new_ticket() {
    use cmd_wire::batch::{BatchOutcome, BatchReply, ItemState};
    let p = Progress::default();
    let b = crate::batch::BatchBoard::new(crate::batch::Plan {
        verb: crate::batch::Verb::Copy,
        cmd: "files-copy",
        args: serde_json::json!({"root": "/srv", "items": [
            {"from": "a", "to": "b/a"}, {"from": "c", "to": "b/c"}, {"from": "d", "to": "b/d"}]}),
        names: vec!["a".into(), "c".into(), "d".into()],
        into: Some("/srv/b".into()),
        machine: "devbox".into(),
    });
    p.add(
        Trip::Batch {
            board: b.clone(),
            icon: egui_phosphor::regular::COPY,
        },
        Some("/srv/b".into()),
        None,
    );
    b.set(crate::batch::Phase::Ended(BatchReply {
        outcomes: vec![
            BatchOutcome::bare("0", ItemState::Done),
            BatchOutcome {
                reason: Some(copy_text("reason.io.denied", &[])),
                ..BatchOutcome::bare("1", ItemState::Failed)
            },
            BatchOutcome::bare("2", ItemState::Skipped),
        ],
        said: "复制 3 项 · 完成 1 · 跳过 1 · 失败 ×1".into(),
    }));
    let (at, home) = (
        crate::source::Origin("devbox".into()),
        crate::source::Origin("devbox".into()),
    );
    b.set_route(at.clone(), home.clone());
    p.settle(0);
    let v = p.jobs()[0].view();
    let Some((
        label,
        Ok(Act::RetryBatch {
            plan,
            at: a,
            home: h,
            ..
        }),
    )) = v.button
    else {
        panic!("部分失败那一行没有「重试失败的」：{:?}", v.button);
    };
    assert_eq!(label, copy_text("rsFilewinBatch.btn.retry", &[]));
    assert_eq!(plan.names, vec!["c".to_string()]);
    assert_eq!(
        plan.args,
        serde_json::json!({"root": "/srv", "items": [{"from": "c", "to": "b/c"}]}),
        "只含没成的那一项、不带原来那张票"
    );
    assert_eq!((a, h), (at, home), "另起的那一组寻址同一台");
    // 全成了 ⇒ 不带按钮。
    let ok = crate::batch::BatchBoard::new(b.plan());
    ok.set_route(
        crate::source::Origin("devbox".into()),
        crate::source::Origin("devbox".into()),
    );
    p.add(
        Trip::Batch {
            board: ok.clone(),
            icon: egui_phosphor::regular::COPY,
        },
        None,
        None,
    );
    ok.set(crate::batch::Phase::Ended(BatchReply {
        outcomes: (0..3)
            .map(|i| BatchOutcome::bare(i.to_string(), ItemState::Done))
            .collect(),
        said: "已复制 3 项 → /srv/b".into(),
    }));
    assert_eq!(p.jobs()[1].view().button, None);
}

fn delete_board() -> crate::batch::BatchBoard {
    crate::batch::BatchBoard::new(crate::batch::Plan {
        verb: crate::batch::Verb::Delete,
        cmd: "files-delete",
        args: serde_json::json!({"root": "/srv", "items": [{"rel": "node_modules", "recursive": true}]}),
        names: vec!["node_modules".into()],
        into: None,
        machine: "devbox".into(),
    })
}

/// 删一整棵树那一行带票：「停」按得下去，发的是那一组的 `files-batch-stop`（撤的那一手由起跑那一侧装）；
/// 那台阻塞档撤不动的那几类（解压 · 算大小）照旧灰着说为什么。
#[test]
fn a_tree_delete_row_stops_through_its_ticket() {
    let p = Progress::default();
    let b = delete_board();
    let hit = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let h2 = hit.clone();
    b.set_stopper(std::sync::Arc::new(move || {
        h2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let id = p.add(
        Trip::Batch {
            board: b.clone(),
            icon: egui_phosphor::regular::TRASH,
        },
        Some("/srv".into()),
        None,
    );
    let v = p.jobs()[0].view();
    assert_eq!(
        v.title,
        copy_text(
            "rsFilewinBatch.row.title",
            &[
                ("action", &copy_text("beFiles.batch.verbDelete", &[])),
                ("n", "1")
            ]
        )
    );
    assert!(
        matches!(v.button, Some((_, Ok(Act::Stop(x)))) if x == id),
        "{:?}",
        v.button
    );
    assert!(p.stop(id), "删大树那一行撤不了");
    assert_eq!(hit.load(std::sync::atomic::Ordering::SeqCst), 1);
    let why = copy_text(
        "rsFilewinProgress.stop.uncancellable",
        &[("machine", "devbox")],
    );
    let s = crate::size::SizeBoard::default();
    p.add(
        Trip::Size {
            board: s,
            name: "x".into(),
        },
        None,
        Some(why.clone()),
    );
    assert_eq!(
        p.jobs()[1].view().button,
        Some((copy_text("rsFilewinProgress.action.stop", &[]), Err(why)))
    );
}

/// 冲突框摆着时还不算开跑（表里不画）；人答了「取消」⇒ 那一行整个拿掉（没开过）。
#[tokio::test]
async fn a_batch_the_user_cancelled_never_shows_up() {
    let p = Progress::default();
    let b = delete_board();
    p.add(
        Trip::Batch {
            board: b.clone(),
            icon: egui_phosphor::regular::COPY,
        },
        Some("/srv".into()),
        None,
    );
    let b2 = b.clone();
    let task = tokio::spawn(async move {
        crate::batch::run(
            &b2,
            |_| async {
                Err(crate::batch::Rebuff {
                    failed: crate::source::Failed {
                        code: Some("exists".into()),
                        said: String::new(),
                        detail: String::new(),
                    },
                    data: Some(serde_json::json!({"conflicts": [
                        {"key": "0", "name": "x", "kind": "file", "there": "file", "dir_into": false}
                    ]})),
                    cut: false,
                })
            },
            || async {},
        )
        .await
    });
    while !b.is_asking() {
        tokio::task::yield_now().await;
    }
    assert!(p.is_empty(), "那一问还摆着，表里就有了一行");
    b.answer(None, false);
    task.await.unwrap();
    p.settle(0);
    assert!(p.jobs().is_empty(), "答了取消的那一组留在了表里");
}

/// 已完成的只留最近 50 行（更早的自动清掉）；在跑的一行都不清。
#[test]
fn only_the_last_fifty_finished_rows_are_kept() {
    let p = Progress::default();
    let live = DownloadBoard::default();
    p.add(
        Trip::Download {
            board: live,
            name: "live".into(),
            src: "/srv/live".into(),
            dest: "/tmp/live".into(),
        },
        None,
        None,
    );
    for k in 0..57 {
        let b = DownloadBoard::default();
        p.add(
            Trip::Download {
                board: b.clone(),
                name: format!("f{k}"),
                src: format!("/srv/f{k}"),
                dest: format!("/tmp/f{k}"),
            },
            None,
            None,
        );
        b.finish(crate::download::Outcome::Done {
            dest: format!("/tmp/f{k}"),
            bytes: 1,
        });
        p.settle(k as u64);
    }
    let jobs = p.jobs();
    assert_eq!(jobs.len(), 51, "已完成的没只留 50 行");
    assert_eq!(jobs[0].state(), State::Running, "在跑的那一行被清掉了");
    assert!(
        matches!(&jobs[1].trip, Trip::Download { name, .. } if name == "f7"),
        "清掉的不是最早的那几行"
    );
}

/// 约多久：刚开头（不足 2 秒 / 一个字节没走）不说；之后照平均速度、按秒 · 分 · 时取整往上。
#[test]
fn the_time_left_is_said_only_once_there_is_a_rate() {
    assert_eq!(eta(0, 100, 10.0), None);
    assert_eq!(eta(10, 100, 1.0), None);
    assert_eq!(eta(100, 100, 10.0), None);
    assert_eq!(
        eta(50, 100, 10.0),
        Some(copy_text("rsFilewinProgress.eta.about", &[("dur", "10s")]))
    );
    assert_eq!(
        eta(10, 100, 10.0),
        Some(copy_text("rsFilewinProgress.eta.about", &[("dur", "2m")]))
    );
    assert_eq!(
        eta(1, 1000, 10.0),
        Some(copy_text("rsFilewinProgress.eta.about", &[("dur", "3h")]))
    );
}
