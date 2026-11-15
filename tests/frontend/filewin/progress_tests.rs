//! 「进度」表（`progress.rs`）：一趟一行、各自的状态与那颗按钮、只留最近 50 行已完成、撤不动的那几类「停」灰着说为什么。
//! 期望全部手写；每一行的读数从它自己那块看板来（夹具直接摆看板的读数）。

use super::*;
use copy_core::copy_text;

/// 判据读进度表用的那一件（生产那一份里没有它；关窗那一问走 `running_for_close`）。
impl Progress {
    /// 还在跑的那几趟（标题）。
    pub(crate) fn running_titles(&self) -> Vec<String> {
        let g = self.lock();
        g.jobs
            .iter()
            .filter(|j| j.state() == State::Running && j.shown())
            .map(|j| j.title())
            .collect()
    }
}

/// 一组搬到另一台（上传 · 下载）：`n` 项，看板交出去（判据照它摆进度与结局）。
fn moving(verb: batch_core::Verb, n: usize) -> (Trip, crate::batch::BatchBoard) {
    let here = crate::source::Origin::named("devbox");
    let paths: Vec<String> = (0..n).map(|k| format!("/tmp/f{k}.bin")).collect();
    let plan = match verb {
        batch_core::Verb::Upload => {
            crate::transfer::upload_plan(&here, "devbox".into(), &paths, "/srv/data", None).unwrap()
        }
        _ => crate::download::many_plan(
            &here,
            paths
                .iter()
                .map(|p| (cmd_wire::files::WirePath::from(p.as_str()), p.clone()))
                .collect(),
            "/tmp",
        ),
    };
    let board = crate::batch::BatchBoard::new(plan);
    (
        Trip::Batch {
            board: board.clone(),
            icon: egui_phosphor::regular::UPLOAD_SIMPLE,
        },
        board,
    )
}

fn all_done(n: usize) -> batch_core::Phase {
    use cmd_wire::batch::{BatchOutcome, BatchReply, ItemState};
    batch_core::Phase::Ended(BatchReply {
        outcomes: (0..n)
            .map(|i| BatchOutcome::bare(i.to_string(), ItemState::Done))
            .collect(),
        said: format!("已复制 {n} 项"),
    })
}

/// 两组上传、一组下载同时在表里：各是各的一行，一组落地不碰另两行；在跑的个数与合计进度只算在跑的；
/// 右格是字节数 ＋ 本机后端写好的剩多久。
#[test]
fn each_trip_is_its_own_row_and_one_landing_leaves_the_others_running() {
    let p = Progress::default();
    let (ta, a) = moving(batch_core::Verb::Upload, 2);
    let (tb, _b) = moving(batch_core::Verb::Upload, 1);
    let (td, d) = moving(batch_core::Verb::Download, 1);
    p.add(ta, Some("/srv/data".into()), None);
    p.add(tb, Some("/srv/data".into()), None);
    p.add(td, None, None);
    d.progress(
        batch_core::Tick {
            done: 0,
            total: 1,
            bytes: Some((25, 100)),
        },
        Some("约 1 分钟".into()),
    );
    assert_eq!(p.running(), 3);
    assert_eq!(
        p.jobs()[2].view().nums,
        copy_text(
            "rsFilewinProgress.nums.eta",
            &[
                (
                    "bytes",
                    &copy_text(
                        "rsFilewinProgress.nums.bytes",
                        &[
                            ("got", &copy_core::size_text(25)),
                            ("total", &copy_core::size_text(100))
                        ],
                    )
                ),
                ("left", "约 1 分钟"),
            ],
        )
    );
    a.set(all_done(2));
    p.settle(0);
    let states: Vec<State> = p.jobs().iter().map(Job::state).collect();
    assert_eq!(states, vec![State::Done, State::Running, State::Running]);
    assert_eq!(p.running(), 2);
    assert_eq!(p.overall(), Some(0.25), "合计进度没只算在跑的那几趟");
    assert_eq!(p.jobs()[0].view().title, "已复制 2 项");
    // 「清除已完成」只拿掉落了地的那一行。
    p.clear_finished();
    assert_eq!(p.jobs().len(), 2);
}

/// 一组的结局里有没成的 ⇒ 那一行带「重试失败的」：交出的那一组**只含没成的那几项**（同一台上的那几条照原样的根与参数），
/// 寻址同一台另起一组（窗口那一级照它新起一块看板 ⇒ 新铸一张票）；全成了 ⇒ 不带按钮。
#[test]
fn a_partly_failed_group_offers_to_retry_only_the_failed_items_under_a_new_ticket() {
    use cmd_wire::batch::{BatchOutcome, BatchReply, ItemState};
    let p = Progress::default();
    let b = crate::batch::BatchBoard::new(batch_core::Plan {
        verb: batch_core::Verb::Copy,
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
    b.set(batch_core::Phase::Ended(BatchReply {
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
        crate::source::Origin::named("devbox"),
        crate::source::Origin::named("devbox"),
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
        crate::source::Origin::named("devbox"),
        crate::source::Origin::named("devbox"),
    );
    p.add(
        Trip::Batch {
            board: ok.clone(),
            icon: egui_phosphor::regular::COPY,
        },
        None,
        None,
    );
    ok.set(batch_core::Phase::Ended(BatchReply {
        outcomes: (0..3)
            .map(|i| BatchOutcome::bare(i.to_string(), ItemState::Done))
            .collect(),
        said: "已复制 3 项 → /srv/b".into(),
    }));
    assert_eq!(p.jobs()[1].view().button, None);
}

fn delete_board() -> crate::batch::BatchBoard {
    crate::batch::BatchBoard::new(batch_core::Plan {
        verb: batch_core::Verb::Delete,
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
            "beFiles.batch.head",
            &[
                ("action", &copy_text("rsFilewinWriteops.label.delete", &[])),
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
        batch_core::run(
            &b2,
            |_| async {
                Err(batch_core::Rebuff {
                    failed: crate::source::Failed {
                        code: "exists".into(),
                        data: Some(serde_json::json!({"conflicts": [
                        {"key": "0", "name": "x", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]}
                        ]})),
                        ..Default::default()
                    },
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
    let (live, _keep) = moving(batch_core::Verb::Download, 1);
    p.add(live, None, None);
    let mut boards = Vec::new();
    for k in 0..57 {
        let (t, b) = moving(batch_core::Verb::Download, 1);
        p.add(t, None, None);
        b.set(all_done(1));
        p.settle(k as u64);
        boards.push(b);
    }
    let jobs = p.jobs();
    assert_eq!(jobs.len(), 51, "已完成的没只留 50 行");
    assert_eq!(jobs[0].state(), State::Running, "在跑的那一行被清掉了");
    assert!(
        matches!(&jobs[1].trip, Trip::Batch { board, .. } if board.ticket() == boards[7].ticket()),
        "清掉的不是最早的那几行"
    );
}
