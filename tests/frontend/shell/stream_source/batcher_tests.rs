use super::*;

fn line(sid: &str, seq: u64) -> JsonlLine {
    JsonlLine {
        session_id: sid.to_string(),
        path: std::path::PathBuf::from(format!("/fake/{sid}.jsonl")),
        seq,
        record: crate::ui_contract::RecordBody::from_json(format!("{{\"seq\":{seq}}}")),
        cwd: None,
        end: None,
        rid: None,
        catch_up: false,
    }
}

#[test]
fn push_below_cap_accumulates_take_drains_in_order() {
    let mut b = Batcher::new(600);
    assert!(b.push(line("s1", 0)).is_none());
    assert!(b.push(line("s2", 0)).is_none()); // 跨 session 混流不拆
    assert!(b.push(line("s1", 1)).is_none());
    let out = b.take().expect("non-empty");
    assert_eq!(
        out.iter()
            .map(|l| (l.session_id.as_str(), l.seq))
            .collect::<Vec<_>>(),
        vec![("s1", 0), ("s2", 0), ("s1", 1)],
        "到达顺序 = 发出顺序"
    );
    assert!(b.take().is_none(), "drained");
}

#[test]
fn cap_triggers_immediate_full_flush() {
    let mut b = Batcher::new(3);
    assert!(b.push(line("s", 0)).is_none());
    assert!(b.push(line("s", 1)).is_none());
    let full = b.push(line("s", 2)).expect("cap reached → full batch out");
    assert_eq!(full.len(), 3);
    assert!(b.take().is_none(), "buffer empty after cap flush");
    // 继续攒下一批不受影响
    assert!(b.push(line("s", 3)).is_none());
    assert_eq!(b.take().unwrap().len(), 1);
}

#[test]
fn take_on_empty_is_none() {
    let mut b = Batcher::new(600);
    assert!(b.take().is_none());
}

/// 记录文件从头重读：后端不逐行重发，壳照宣告那样排一份旁路快照（带重读完的行数）；「不见了」不排。
#[tokio::test]
async fn a_reread_queues_a_fresh_tail_snapshot_and_gone_does_not() {
    let snapshots = SnapshotQueue::new();
    let mut intake = LineIntake {
        origin_label: crate::origin::LOCAL.to_string(),
        replay: std::sync::Arc::new(crate::event_replay::EventReplay::new()),
        batcher: Batcher::new(BATCH_CAP),
        snapshots: snapshots.clone(),
        runs: crate::SkipRuns::default(),
        _closer: SnapshotQueueCloser(snapshots.clone()),
    };
    intake.announced("s", Some("/p/s.jsonl".into()), Some(10));
    let first = snapshots.pop(None).await.unwrap();
    assert_eq!(first.expected_lines, Some(10), "宣告那一份");
    intake
        .notice("s", "/p/s.jsonl", FileChange::Gone, None)
        .await;
    intake
        .notice("s", "/p/s.jsonl", FileChange::Truncated, Some(3))
        .await;
    let again = tokio::time::timeout(guard_core::wall::CEILING, snapshots.pop(None))
        .await
        .expect("从头重读之后没排旁路快照")
        .unwrap();
    assert_eq!(
        (
            again.sid.as_str(),
            again.path.as_str(),
            again.expected_lines
        ),
        ("s", "/p/s.jsonl", Some(3)),
        "从头重读 ⇒ 照宣告那样重拉尾段"
    );
    snapshots.close();
    assert!(snapshots.pop(None).await.is_none(), "不见了那一下不该排");
}
