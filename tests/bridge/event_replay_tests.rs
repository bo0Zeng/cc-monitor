use super::*;
use crate::messages::JsonlRecord;

/// 用 path 字段携带 idx 给测试用（Unknown 变体是 unit struct 不能塞 metadata）。
/// P5.1：seq 也用 idx，方便排序断言。
fn payload(sid: &str, idx: usize) -> JsonlLinePayload {
    JsonlLinePayload {
        session_id: sid.to_string(),
        cwd: None,
        path: format!("/fake/{sid}/{idx}.jsonl"),
        seq: idx as u64,
        origin: None,
        message: JsonlRecord::Unknown,
    }
}

fn idx_of(p: &JsonlLinePayload) -> usize {
    let path = &p.path;
    let last = path.rsplit('/').next().unwrap();
    last.trim_end_matches(".jsonl").parse().unwrap()
}

#[test]
fn buffered_local_session_ids_dedups_and_skips_remote() {
    let replay = EventReplay::new();
    {
        // 子模块可直接访问私有 inner。
        let mut inner = replay.inner.lock();
        inner.history.push_back(payload("s1", 0));
        inner.history.push_back(payload("s1", 1)); // 同 sid 第二行 → 去重
        inner.history.push_back(payload("s2", 0));
        let mut remote = payload("r1", 0);
        remote.origin = Some("nanopi".to_string()); // 远端 → 跳过
        inner.history.push_back(remote);
    }
    let mut ids = replay.buffered_local_session_ids();
    ids.sort();
    assert_eq!(ids, vec!["s1".to_string(), "s2".to_string()]);
}

#[test]
fn buffered_remote_session_ids_dedups_and_skips_local() {
    let replay = EventReplay::new();
    {
        let mut inner = replay.inner.lock();
        inner.history.push_back(payload("s1", 0)); // 本地 → 跳过
        let mut r1a = payload("r1", 0);
        r1a.origin = Some("nanopi".to_string());
        inner.history.push_back(r1a);
        let mut r1b = payload("r1", 1); // 同 sid 第二行 → 去重
        r1b.origin = Some("nanopi".to_string());
        inner.history.push_back(r1b);
        let mut r2 = payload("r2", 0);
        r2.origin = Some("rk3576".to_string()); // 不同 host 也收
        inner.history.push_back(r2);
    }
    let mut ids = replay.buffered_remote_session_ids();
    ids.sort();
    assert_eq!(ids, vec!["r1".to_string(), "r2".to_string()]);
}

// === Batch5-F19：build_priority_chunks ===

#[test]
fn priority_chunks_put_priority_session_first_no_loss_no_dup() {
    // s1 与 s2 交错各 700 条（> CHUNK_SIZE=600，两组都会切多块）
    let mut snapshot = Vec::new();
    for i in 0..700 {
        snapshot.push(payload("s1", i * 2));
        snapshot.push(payload("s2", i * 2 + 1));
    }
    let chunks = build_priority_chunks(snapshot.clone(), Some("s2"));
    let flat: Vec<&JsonlLinePayload> = chunks.iter().flatten().collect();
    assert_eq!(flat.len(), 1400, "no loss");
    // 前 700 条全是 s2（priority 组整体在前）
    assert!(flat[..700].iter().all(|p| p.session_id == "s2"));
    assert!(flat[700..].iter().all(|p| p.session_id == "s1"));
    // 组内末块先发：priority 组第一块的首元素 idx 大于最后一块的首元素 idx
    let first_chunk_first = idx_of(&chunks[0][0]);
    let pri_chunk_count = chunks
        .iter()
        .take_while(|c| c[0].session_id == "s2")
        .count();
    let last_pri_first = idx_of(&chunks[pri_chunk_count - 1][0]);
    assert!(
        first_chunk_first > last_pri_first,
        "newest-first within priority group"
    );
    // rest 组同样末块先发（审计 S3：只验 pri 组会漏掉 rest 组改正序的回归）
    let first_rest_first = idx_of(&chunks[pri_chunk_count][0]);
    let last_rest_first = idx_of(&chunks[chunks.len() - 1][0]);
    assert!(
        first_rest_first > last_rest_first,
        "newest-first within rest group"
    );
    // 去重校验：uuid 级不重复（用 (sid, seq) 对）
    let mut seen = std::collections::HashSet::new();
    for p in &flat {
        assert!(seen.insert((p.session_id.clone(), p.seq)), "no dup");
    }
}

#[test]
fn priority_none_or_miss_equals_plain_build_chunks() {
    let snapshot: Vec<JsonlLinePayload> = (0..1500).map(|i| payload("s1", i)).collect();
    let plain = build_chunks(&snapshot);
    let none = build_priority_chunks(snapshot.clone(), None);
    let miss = build_priority_chunks(snapshot.clone(), Some("nope"));
    let key = |cs: &Vec<Vec<JsonlLinePayload>>| -> Vec<Vec<u64>> {
        cs.iter()
            .map(|c| c.iter().map(|p| p.seq).collect())
            .collect()
    };
    assert_eq!(key(&none), key(&plain), "None → 等价 build_chunks");
    assert_eq!(key(&miss), key(&plain), "sid 不命中 → 退化等价");
}

#[test]
fn build_chunks_small_history_single_chunk() {
    // 50 条 < CHUNK_SIZE → 全进单块
    let payloads: Vec<_> = (0..50).map(|i| payload("s1", i)).collect();
    let chunks = build_chunks(&payloads);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 50);
}

#[test]
fn build_chunks_large_history_splits_by_chunk_size() {
    let payloads: Vec<_> = (0..3000).map(|i| payload("s1", i)).collect();
    let chunks = build_chunks(&payloads);
    // 3000 / 600 = 5 块
    assert_eq!(chunks.len(), 5);
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    assert_eq!(total, 3000);
}

#[test]
fn build_chunks_emits_newest_first() {
    // P5.4 关键：chunks[0] 应当是 input 末段（最新），chunks[N-1] 是 input 头段（最老）。
    // 末块先发 → 前端 timeline 按 seq 自动放，UI 上用户立刻看到最新内容。
    let payloads: Vec<_> = (0..1500).map(|i| payload("s1", i)).collect();
    let chunks = build_chunks(&payloads);
    // chunks[0] 应该含 idx 900..1500（最新 600 条）
    assert_eq!(chunks[0].len(), 600);
    assert_eq!(idx_of(&chunks[0][0]), 900);
    assert_eq!(idx_of(&chunks[0][599]), 1499);
    // chunks 末块应该含 idx 0..300（最老 300 条）
    let last_chunk = chunks.last().unwrap();
    assert_eq!(idx_of(&last_chunk[0]), 0);
}

#[test]
fn build_chunks_preserves_input_order_within_chunks() {
    // chunk 内顺序 = 输入顺序的连续片段，前端按 seq 排即可还原全局序。
    let payloads: Vec<_> = (0..500).map(|i| payload("s1", i)).collect();
    let chunks = build_chunks(&payloads);
    // 反向拼接（块倒序 + 块内升序）= 完整时间序
    let mut reordered: Vec<&JsonlLinePayload> = Vec::new();
    for c in chunks.iter().rev() {
        for p in c {
            reordered.push(p);
        }
    }
    for (i, p) in reordered.iter().enumerate() {
        assert_eq!(
            idx_of(p),
            i,
            "block-reversed order should match input at {i}"
        );
    }
}
