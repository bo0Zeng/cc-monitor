use super::*;

// 「快照那一行计不计号」那一条随函数删了：后端只交可计行，口径只住后端 `history_query::line_counts`。

/// 队列语义：sid 幂等、priority 优先出队、close 后清空账再 None。
#[tokio::test]
async fn snapshot_queue_priority_idempotent_and_close() {
    fn item(sid: &str, path: &str) -> SnapshotItem {
        SnapshotItem {
            sid: sid.into(),
            path: path.into(),
            expected_lines: None,
        }
    }
    let q = SnapshotQueue::new();
    q.push(item("s1", "/p1"));
    q.push(item("s2", "/p2"));
    q.push(item("s3", "/p3"));
    q.push(item("s2", "/p2-dup")); // 幂等：不重拉
                                   // priority=s2 → 先出 s2
    let got = q.pop(Some("s2".into())).await.unwrap();
    assert_eq!((got.sid.as_str(), got.path.as_str()), ("s2", "/p2"));
    // priority 不在队 → FIFO
    let got = q.pop(Some("nope".into())).await.unwrap();
    assert_eq!(got.sid, "s1");
    // Batch8 审计 D-B1：cancel 摘排队项 + 标记取消 + seen 可重入队
    q.cancel("s3");
    assert!(q.is_cancelled("s3"), "cancel 后 inflight 检查命中");
    q.push(item("s3", "/p3-again")); // removed→re-added：解除取消、重新入队
    assert!(!q.is_cancelled("s3"), "重新宣告解除取消标记");
    let got = q.pop(None).await.unwrap();
    assert_eq!(got.path, "/p3-again");
    // Batch8 审计 D-B1：close 立即作废排队项（不清账）
    q.push(item("s4", "/p4"));
    q.close();
    assert!(q.pop(None).await.is_none(), "close 后排队项作废");
    assert!(q.is_cancelled("s4"), "close 后 inflight 检查也命中（作废）");
}

/// close 唤醒等待中的 pop（分发器不悬挂）。
#[tokio::test]
async fn snapshot_queue_close_wakes_waiting_pop() {
    let q = SnapshotQueue::new();
    let q2 = q.clone();
    let waiter = tokio::spawn(async move { q2.pop(None).await });
    // 单线程运行时：让一拍 ⇒ 它跑到 `pop` 里停住（没东西可取）；没停住就不是在测「唤醒」。
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished(), "空队列上的 pop 没停住");
    q.close();
    assert!(tokio::time::timeout(guard_core::wall::CEILING, waiter)
        .await
        .expect("pop 必须被 close 唤醒")
        .unwrap()
        .is_none());
}

/// 只拉在看的会话：那台报过名单 ⇒ 名单外的不拉；没报过（全看）⇒ 都拉。
#[test]
fn only_watched_sessions_get_a_snapshot() {
    let w: std::collections::BTreeSet<String> = ["a".to_string()].into();
    assert!(super::wants_snapshot(Some(&w), "a"));
    assert!(!super::wants_snapshot(Some(&w), "b"), "名单外的也拉了");
    assert!(
        !super::wants_snapshot(Some(&Default::default()), "a"),
        "空名单 ＝ 一个都不看"
    );
    assert!(super::wants_snapshot(None, "b"), "没报过名单 ＝ 全看");
}

/// 两处接线：分发器出队后过 [`super::wants_snapshot`] 这道门（名单从 `EventReplay::watched` 来，与 `stream_watch` 报给那台的同一份）；
/// 总循环拿到那台的名单就作废名单外的续点（`snapshot_resume::keep_only`）。哪一句被删，上面两条纯函数判据照绿，这一条红。
#[test]
fn the_dispatcher_and_the_watch_loop_use_the_watched_list() {
    let snapshot = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_source/snapshot.rs"
    ));
    guard_core::find_pinned(
        &snapshot,
        "if !wants_snapshot(replay.watched(&origin).as_ref(), &item.sid) {",
    )
    .unwrap_or_else(|e| panic!("分发器没按名单拉：{e}"));
    let watch = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_watch.rs"
    ));
    guard_core::find_pinned(&watch, "crate::snapshot_resume::keep_only(&origin, &want);")
        .unwrap_or_else(|e| panic!("名单外的续点没作废：{e}"));
}

/// ★ 每个会话的补历史边界：宣告时那台说的行数 L（`session_added.lines`）以前的是补历史；快照读到 L 以后的
/// 是宣告之后才写进来的（实时那条流也会交它，谁先到算谁），不是补历史。L 不知道 ⇒ 快照里的都算补历史（宁可不跟）。
#[test]
fn snapshot_lines_before_the_announced_count_are_catch_up_and_later_ones_are_live() {
    assert!(catch_up_at(Some(10), 0));
    assert!(catch_up_at(Some(10), 9));
    assert!(!catch_up_at(Some(10), 10));
    assert!(!catch_up_at(Some(10), 11));
    assert!(catch_up_at(None, 12345));
}

/// **快照在途电平那把锁与会话流的锁只许一个先后**：电平变的那一刻持着 [`SNAPSHOT_INFLIGHT`] 交给会话流（要 replay 的锁，
/// [`snapshot_inflight_change`]）⇒ 反过来那一向不许有 —— 持着 replay 的锁的那几条路（登记一条只跟一个会话的订阅 · 就绪点）不许再要电平锁。
/// 起因：截图台架偶发「一个 tab 都不来 / 少一个 / 收场等到上限」，抓栈是这一对互等（主线程在 `subscribe` 里持 replay 的锁等电平锁，
/// 交电平的那条线程持电平锁等 replay 的锁），之后整条会话流与帧命令一起停住；产品里是独立窗口开窗 / F5 撞上一份旁路快照开始或收尾，整个窗口永远不再动。
/// 判法：本线程持着电平锁（同交电平那一刻），另一条线程登记 ＋ 过就绪点，必须不等这把锁就做完。
#[test]
fn subscribing_and_the_ready_point_never_wait_for_the_inflight_level_lock() {
    struct Quiet;
    impl crate::event_replay::ItemSink for Quiet {
        fn deliver(&self, _: &str, _: u64, _: Vec<crate::chan::wire::Item>) {}
    }
    let replay = Arc::new(EventReplay::new());
    replay.attach_sink(Arc::new(Quiet));
    let held = SNAPSHOT_INFLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    let (tx, rx) = std::sync::mpsc::channel();
    let r = replay.clone();
    let worker = std::thread::spawn(move || {
        let local = crate::origin::Origin::local();
        r.subscribe("viewer", 1, &local, "session-lines/s1", None, 8);
        tx.send("登记一条只跟一个会话的订阅").ok();
        r.subscribe("main", 2, &local, "session-lines", None, 8);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("起一个单线程运行时")
            .block_on(r.ready_point(None));
        tx.send("过就绪点").ok();
    });
    let mut done = Vec::new();
    while done.len() < 2 {
        match rx.recv_timeout(guard_core::wall::CEILING) {
            Ok(step) => done.push(step),
            Err(_) => break,
        }
    }
    drop(held);
    worker.join().expect("登记 / 就绪点那条线程没崩");
    assert_eq!(
        done,
        vec!["登记一条只跟一个会话的订阅", "过就绪点"],
        "持着快照在途电平锁的时候，登记 / 就绪点卡住了（要的是 replay 锁 → 电平锁，与交电平那一向相反：两条线程会互等卡死）"
    );
}
