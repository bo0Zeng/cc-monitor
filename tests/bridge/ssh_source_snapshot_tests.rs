use super::*;

/// 行计数口径必须与 daemon read_new_lines 一字一致：BOM+全空白跳过。
#[test]
fn snapshot_line_countable_matches_daemon_semantics() {
    assert!(snapshot_line_countable(r#"{"a":1}"#));
    assert!(snapshot_line_countable("\u{feff}{\"a\":1}")); // BOM+内容 → 计
    assert!(!snapshot_line_countable("")); // 空行 → 跳
    assert!(!snapshot_line_countable("   ")); // 全空白 → 跳
    assert!(!snapshot_line_countable("\u{feff}")); // 纯 BOM → 跳
    assert!(!snapshot_line_countable("\u{feff}  \t")); // BOM+空白 → 跳
}

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
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    q.close();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .expect("pop 必须被 close 唤醒")
            .unwrap()
            .is_none()
    );
}
