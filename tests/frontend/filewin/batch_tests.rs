//! `batch.rs` 的判据：一组带票 · 同名照 `data.conflicts` 问一次（默认跳过）· 断了等连回来、同一张票再问 · 撤带票 · 结局照后端画。
//! 后端那一半是替身（按剧本回），真后端那一半的判据住 `tests/backend/control/files_batch_tests.rs`。

use super::*;
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
        failed: super::super::source::Failed {
            code: Some(EXISTS.into()),
            said: "同名 2 项".into(),
            detail: String::new(),
        },
        data: Some(serde_json::json!({"conflicts": [
            {"key": "0", "name": "a", "kind": "file", "size": 3, "there": "file", "there_size": 1, "dir_into": false},
            {"key": "1", "name": "c", "kind": "dir", "there": "dir", "dir_into": true},
        ]})),
        cut: false,
    }
}

fn done(said: &str) -> serde_json::Value {
    serde_json::json!({"outcomes": [
        {"key": "0", "state": "done"},
        {"key": "1", "state": "failed", "code": "refused", "reason": "无权限", "detail": "那台说：EACCES"},
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
    assert!(c[1].dir_into, "目录同名那一行要写不合并");
    assert_eq!(
        conflict_row(&c[1]).meta,
        copy_text("rsFilewinBatch.ask.dirNoMerge", &[])
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
    assert!(board.answer(None, false), "取消");
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
            &[("name", "c"), ("why", "无权限")]
        )],
        "没成的那一项照 outcomes 列"
    );
    assert!(copy_details(&board.plan(), &r).unwrap().contains("EACCES"));
}

#[test]
fn the_cut_line_says_the_rest_is_unknown() {
    let s = cut_line(
        &plan(),
        &Tick {
            done: 1,
            total: 2,
            bytes: None,
        },
    );
    assert!(s.starts_with("devbox"), "{s}");
    assert!(s.contains("其余待核"), "{s}");
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

#[test]
fn progress_goes_by_bytes_when_the_backend_reports_them() {
    let t = Tick::of_cell(
        &serde_json::json!({"done": 0, "total": 1, "bytes_done": 512, "bytes_total": 2048}),
    )
    .unwrap();
    assert_eq!(t.frac(), Some(0.25), "一份大文件传到四分之一：照字节画");
    let t = Tick::of_cell(&serde_json::json!({"done": 1, "total": 4})).unwrap();
    assert_eq!(t.frac(), Some(0.25));
    assert!(Tick::of_cell(&serde_json::json!({"done": 1})).is_none());
}
