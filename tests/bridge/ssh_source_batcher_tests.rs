use super::*;

fn line(sid: &str, seq: u64) -> JsonlLine {
    JsonlLine {
        session_id: sid.to_string(),
        path: std::path::PathBuf::from(format!("/fake/{sid}.jsonl")),
        seq,
        raw: format!("{{\"seq\":{seq}}}"),
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
