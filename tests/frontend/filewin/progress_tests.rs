//! 「进度」表（`progress.rs`）：一趟一行、各自的状态与那颗按钮、只留最近 50 行已完成、撤不动的那几类「停」灰着说为什么。
//! 期望全部手写；每一行的读数从它自己那块看板来（夹具直接摆看板的读数）。

use super::*;
use copy_core::copy_text;

fn upload(board: &DropBoard, n: usize) -> Trip {
    let items = (0..n)
        .map(|k| {
            super::super::transfer::Pending::into_remote_dir(&format!("/tmp/f{k}.bin"), "/srv/data")
                .unwrap()
        })
        .collect();
    Trip::Upload {
        board: board.clone(),
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

/// 上传失败 ⇒ 标题「上传失败 n」、按钮「重试 n 个」交出**失败的那几个**（不是整摞）；
/// 人按了停 ⇒「已停」那一句、按钮「接着传」交出没传的那几个。
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
    assert_eq!(
        label,
        copy_text("rsFilewinProgress.action.retryN", &[("n", "2")])
    );
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
    assert!(matches!(v.button, Some((_, Ok(Act::Resume(ref r)))) if r.len() == 2));
}

/// 那台后端的阻塞档（机器上复制 · 解压 · 算大小 · 删除）：「停」在、灰着、悬停说那句（开这一行时交进来的事实）；
/// 走传输台的那几类（上传 · 下载 · 复制到另一台）停得了。
#[test]
fn trips_the_machine_cannot_stop_show_a_greyed_stop_that_says_why() {
    let p = Progress::default();
    let why = copy_text(
        "rsFilewinProgress.stop.uncancellable",
        &[("machine", "devbox")],
    );
    let wb = WriteBoard::default();
    p.add(
        Trip::Delete {
            board: wb.clone(),
            base: 0,
            name: "node_modules".into(),
            n: 1,
        },
        Some("/srv".into()),
        Some(why.clone()),
    );
    let v = p.jobs()[0].view();
    assert_eq!(
        v.title,
        copy_text(
            "rsFilewinProgress.delete.runningOne",
            &[("name", "node_modules")]
        )
    );
    assert_eq!(v.frac, Some(None), "删除那一行没画不确定进度");
    assert_eq!(
        v.button,
        Some((copy_text("rsFilewinProgress.action.stop", &[]), Err(why)))
    );
    assert!(!p.stop(p.jobs()[0].id), "撤不动的那一行被「停」了");
    let d = DownloadBoard::default();
    let id = p.add(
        Trip::Download {
            board: d.clone(),
            name: "a".into(),
            src: "/srv/a".into(),
            dest: "/tmp/a".into(),
        },
        None,
        None,
    );
    assert!(matches!(p.jobs()[1].view().button, Some((_, Ok(Act::Stop(x)))) if x == id));
}

/// 删除那一问摆着时还不算开跑（表里不画）；人答了「取消」⇒ 那一行整个拿掉（没开过）。
#[test]
fn a_delete_the_user_cancelled_never_shows_up() {
    let p = Progress::default();
    let wb = WriteBoard::default();
    let _rx = wb.ask(vec![crate::writeops::WriteOp::Delete {
        path: "/srv/x".into(),
        is_dir: true,
        raw: None,
    }]);
    p.add(
        Trip::Delete {
            board: wb.clone(),
            base: 0,
            name: "x".into(),
            n: 1,
        },
        Some("/srv".into()),
        None,
    );
    assert!(p.is_empty(), "那一问还摆着，表里就有了一行");
    wb.settle(false);
    wb.finish(crate::writeops::WriteOutcome {
        asked: 1,
        skipped: 1,
        ..Default::default()
    });
    p.settle(0);
    assert!(p.jobs().is_empty(), "答了取消的删除留在了表里");
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
        Some(copy_text("rsFilewinProgress.eta.secs", &[("n", "10")]))
    );
    assert_eq!(
        eta(10, 100, 10.0),
        Some(copy_text("rsFilewinProgress.eta.mins", &[("n", "2")]))
    );
    assert_eq!(
        eta(1, 1000, 10.0),
        Some(copy_text("rsFilewinProgress.eta.hours", &[("n", "3")]))
    );
}
