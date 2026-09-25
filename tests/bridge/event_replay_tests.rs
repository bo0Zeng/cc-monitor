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

// ═══════════════════════════════════════════════════════════════════════
// 〔CF2 · 第四波 4B〕一档容量：**每个**会话只留尾巴（原〔U3b〕两档：接上骨架的留尾巴、其余不设上限）。
//
// 要求住址：`设计/05 §3.3.4`「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
// **不许静默堆**」。丢掉的正文前端按字节（骨架）或按行号（`read_session_lines`）要得回来 ⇒ 这一档是级 2。
// ═══════════════════════════════════════════════════════════════════════

fn seqs_of(replay: &EventReplay, sid: &str) -> Vec<u64> {
    let mut v: Vec<u64> = replay
        .inner
        .lock()
        .history
        .iter()
        .filter(|p| p.session_id == sid)
        .map(|p| p.seq)
        .collect();
    v.sort_unstable();
    v
}

/// ★ R1：**不登记任何东西**的会话也有上界 —— 长到 `KEEP + SLACK` 不修，再来一条修回 `KEEP`（摊还）；读数跟着走。
#[test]
fn every_session_is_trimmed_back_to_keep_after_the_slack() {
    let replay = EventReplay::new();
    let upto = REPLAY_TAIL_KEEP + TRIM_SLACK;
    let batch: Vec<_> = (0..upto).map(|i| payload("s", i)).collect();
    push_and_trim(&mut replay.inner.lock(), &batch);
    assert_eq!(seqs_of(&replay, "s").len(), upto, "余量之内不修");
    push_and_trim(&mut replay.inner.lock(), &[payload("s", upto)]);
    let got = seqs_of(&replay, "s");
    assert_eq!(got.len(), REPLAY_TAIL_KEEP);
    assert_eq!(*got.first().unwrap(), (upto + 1 - REPLAY_TAIL_KEEP) as u64);
    assert_eq!(*got.last().unwrap(), upto as u64);
    assert_eq!(
        replay.stats(),
        ReplayStats {
            history_len: REPLAY_TAIL_KEEP,
            sessions: 1,
            trimmed_total: (TRIM_SLACK + 1) as u64,
        }
    );
}

/// ★ R1：长到多长都不越过 `KEEP + SLACK`（原来「没骨架的会话」这里是 `KEEP × 3` 条原样留着）。
#[test]
fn no_session_ever_holds_more_than_keep_plus_slack() {
    let replay = EventReplay::new();
    for i in 0..REPLAY_TAIL_KEEP * 5 {
        push_and_trim(&mut replay.inner.lock(), &[payload("free", i)]);
        assert!(
            seqs_of(&replay, "free").len() <= REPLAY_TAIL_KEEP + TRIM_SLACK,
            "第 {i} 行之后越界"
        );
    }
    assert!(replay.stats().trimmed_total > 0);
}

/// 🔴 R1：按 seq 留尾巴，不按到达序：快照是尾部优先（尾块先到、头块后到）⇒ 留下的恰是 seq 最高的那些；
/// 修剪之后才到的头段（seq 低于最低留存）**不进缓冲**；别的会话一条不动。
#[test]
fn the_highest_seqs_are_kept_even_when_the_tail_arrived_first() {
    let replay = EventReplay::new();
    let n = REPLAY_TAIL_KEEP * 2;
    let tail_first: Vec<_> = (REPLAY_TAIL_KEEP..n)
        .chain(0..REPLAY_TAIL_KEEP)
        .map(|i| payload("s", i))
        .collect();
    push_and_trim(&mut replay.inner.lock(), &[payload("other", 0)]);
    // 逐行进（修剪在每一次进账的末尾判；一批之内会暂时越过上界，越过的量 ≤ 那一批的条数）
    for p in &tail_first {
        push_and_trim(&mut replay.inner.lock(), std::slice::from_ref(p));
    }
    let want: Vec<u64> = (REPLAY_TAIL_KEEP as u64..n as u64).collect();
    assert_eq!(seqs_of(&replay, "s"), want);
    assert_eq!(seqs_of(&replay, "other"), vec![0], "别的会话一条不动");
    // 头段前 `SLACK + 1` 条进过缓冲、随那次修剪丢掉；其余 `KEEP − SLACK − 1` 条到的时候已在最低留存之下，没进过。
    assert_eq!(replay.stats().trimmed_total, (TRIM_SLACK + 1) as u64);
    // 最低留存之上的新行照进（实时追加）
    push_and_trim(&mut replay.inner.lock(), &[payload("s", n)]);
    assert_eq!(seqs_of(&replay, "s").last().copied(), Some(n as u64));
}

/// R1：`forget` 连同那个会话的账一起抹掉（之后同名会话从头计数、没有最低留存）。
#[test]
fn forget_also_drops_the_session_accounting() {
    let replay = EventReplay::new();
    let batch: Vec<_> = (0..REPLAY_TAIL_KEEP * 2).map(|i| payload("s", i)).collect();
    push_and_trim(&mut replay.inner.lock(), &batch);
    assert_eq!(replay.stats().sessions, 1);
    replay.forget("s");
    assert_eq!(replay.stats().sessions, 0);
    assert!(seqs_of(&replay, "s").is_empty());
    push_and_trim(&mut replay.inner.lock(), &[payload("s", 0)]);
    assert_eq!(
        seqs_of(&replay, "s"),
        vec![0],
        "忘掉之后低 seq 的行照进（最低留存跟着忘了）"
    );
}

/// 两档那一套的名字：它们在任何一份生产代码里出现 = 分档（或那条登记命令）长回来了。
/// 运行时拼，免得本文件自己的散文命中自己（本文件不在扫描面里，但 `include_str` 之类将来可能把它拉进来）。
/// ⚠ 那张表的字段名（裸的 `tail_only`）**不进**针表：流模式旗标 `--tail-only` 在 `ssh_source.rs` 里也叫这个名字，
/// 与重放缓冲无关（首跑现打：它是唯一一处命中）。那张表删掉之后编译器就管着它。
fn two_tier_needles() -> [String; 2] {
    [
        format!("keep_tail{}", "_only"),
        format!("replay_keep{}", "_tail_only"),
    ]
}

fn two_tier_hits(prod: &str) -> Vec<String> {
    two_tier_needles()
        .into_iter()
        .filter(|n| guard_core::contains_word(prod, n))
        .collect()
}

/// ★ R2：分档那一套（登记「只留尾巴」的方法、那张表、那条 Tauri 命令）在 monitor Rust 生产段与前端 TS 生产段**零命中**；
/// 正控：同一识别器在一段合成代码上两针全中、在「那条退役了」那种注释里不中（剥生产段真的在跑）。
#[test]
fn the_two_tier_registration_leaves_no_trace_in_production() {
    let synthetic = format!(
        "fn a() {{ replay.{}(\"s\"); }}\ninvoke(\"{}\")\n",
        two_tier_needles()[0],
        two_tier_needles()[1]
    );
    assert_eq!(
        two_tier_hits(&guard_core::production_code(&synthetic)).len(),
        2,
        "识别器空转"
    );
    assert!(
        two_tier_hits(&guard_core::production_code(&format!(
            "// 那条（`{}`〔散文{}〕）退役了\n",
            two_tier_needles()[1],
            "墓碑"
        )))
        .is_empty(),
        "注释里的墓碑被当成了生产代码 —— 剥生产段没在跑"
    );
    let root = crate::guard_support::repo_root();
    let generated = root.join("src").join("generated"); // ts-rs 生成物不是手写的生产代码
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for (sub, ext) in [("src/bridge/src", "rs"), ("src", "ts")] {
        for (p, text) in guard_core::scan_tree_excluding(&root.join(sub), &[ext], &[]) {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if p.starts_with(&generated) {
                continue;
            }
            scanned += 1;
            let hits = two_tier_hits(&guard_core::production_code(&text));
            if !hits.is_empty() {
                offenders.push(format!("{rel}：{hits:?}"));
            }
        }
    }
    assert!(scanned > 200, "只扫到 {scanned} 份 —— 扫描面坏了");
    assert!(
        offenders.is_empty(),
        "重放缓冲的「两档」又长回来了（`设计/05 §3.3.4`：每个订阅侧缓冲都要有上界，不许按「有没有登记」分档）：\n{}",
        offenders.join("\n")
    );
}
