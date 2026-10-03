use super::*;

/// 用 path 字段携带 idx 给测试用（记录体对 monitor 是不透明的成品，这里给一个结构占位）。
/// P5.1：seq 也用 idx，方便排序断言。
fn payload(sid: &str, idx: usize) -> JsonlLinePayload {
    JsonlLinePayload {
        session_id: sid.to_string(),
        cwd: None,
        path: format!("/fake/{sid}/{idx}.jsonl"),
        seq: idx as u64,
        origin: None,
        message: crate::ui_contract::RecordBody::from_json("{}".into()).unwrap(),
        skipped_from: None,
        rid: None,
    }
}

fn idx_of(p: &JsonlLinePayload) -> usize {
    let path = &p.path;
    let last = path.rsplit('/').next().unwrap();
    last.trim_end_matches(".jsonl").parse().unwrap()
}

// `buffered_local_session_ids` / `buffered_remote_sessions`〔散文墓碑〕那两条随函数删了（F5 对账改在各条订阅自己的重放里）。

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
// 一档容量：**每个**会话只留尾巴（原两档：接上骨架的留尾巴、其余不设上限）。
//
// 要求：「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
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
    for (sub, ext) in [("src/frontend/shell/src", "rs"), ("src", "ts")] {
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
        "重放缓冲的「两档」又长回来了（每个订阅侧缓冲都要有上界，不许按「有没有登记」分档）：\n{}",
        offenders.join("\n")
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 会话内容流的句柄（`subscribe` · credit · `Gap` · `Unseen`/`Seen` · 就绪点）
//
// 要求：「**回推优先 · 推不动才丢 · 丢必须说**」「`Gap` 必须在流里的原位」·
// `§3.3.5`「`call` 会失败、`subscribe` 不会」· `§8` 步 6「流那半收口成 `subscribe`」。
// 期望一律手写（位置、条数、格的种类），不从被测的计划函数派生。
// ═══════════════════════════════════════════════════════════════════════

use crate::chan::wire::{Body as WBody, By as WBy, HopFault as WHop, Item as WItem};

/// 记录器：`(webview, 订阅, 一次投递的格)`。
#[derive(Default)]
struct Rec(std::sync::Mutex<Vec<(String, u64, Vec<WItem>)>>);

impl ItemSink for Rec {
    fn deliver(&self, label: &str, sub: u64, items: Vec<WItem>) {
        self.0.lock().unwrap().push((label.to_string(), sub, items));
    }
}

impl Rec {
    fn all(&self) -> Vec<WItem> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, _, i)| i.clone())
            .collect()
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// 一格的体读回来：`("line", seq)` / `("start", 0)` / `("end", 0)`。
fn what(i: &WItem) -> (&'static str, u64, u64) {
    match i {
        WItem::Frame { seq, body } => {
            let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
            if let Some(l) = v.get("line") {
                ("line", *seq, l["seq"].as_u64().unwrap())
            } else if let Some(b) = v.get("batch") {
                match b.as_str().unwrap() {
                    "start" => ("start", *seq, 0),
                    _ => ("end", *seq, 0),
                }
            } else {
                // 起停那几格：按键名认。
                let k = [
                    "live",
                    "activity",
                    "container",
                    "idle",
                    "ended",
                    "unseen",
                    "listed",
                    "snapshot_inflight",
                ]
                .into_iter()
                .find(|k| v.get(*k).is_some())
                .expect("认不出的一格");
                (k, *seq, 0)
            }
        }
        WItem::Gap { from_seq, to_seq } => ("gap", *from_seq, to_seq.unwrap_or(u64::MAX)),
        WItem::Unseen { .. } => ("unseen", 0, 0),
        WItem::Seen { .. } => ("seen", 0, 0),
        WItem::Closed { .. } => ("closed", 0, 0),
    }
}

fn hub() -> (Arc<EventReplay>, Arc<Rec>) {
    let (r, _) = hub_with_book();
    (r.0, r.1)
}

/// 带一本自己的成品缓存（不与并行的判据串味）。
fn hub_with_book() -> (
    (Arc<EventReplay>, Arc<Rec>),
    &'static parking_lot::RwLock<crate::session_book::Book>,
) {
    let book: &'static parking_lot::RwLock<crate::session_book::Book> =
        Box::leak(Box::new(parking_lot::RwLock::new(Default::default())));
    let mut r = EventReplay::new();
    r.book = book;
    let r = Arc::new(r);
    let rec = Arc::new(Rec::default());
    r.attach_sink(rec.clone());
    ((r, rec), book)
}

fn local() -> crate::origin::Origin {
    crate::origin::Origin::local()
}

fn lines(sid: &str, seqs: std::ops::Range<usize>) -> Vec<JsonlLinePayload> {
    seqs.map(|i| payload(sid, i)).collect()
}

/// ★ S1：实时那一份 —— 有 credit 当场交、没 credit 就丢（位置照占）；丢了之后**下一次交出去之前**原位给 `Gap`；
/// `want` 回来、手里有没说的丢失 ⇒ 当场给 `Gap`；`Gap` 与 `Unseen` 不占 credit。
#[tokio::test]
async fn live_frames_go_out_with_credit_and_drops_are_said_in_place() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("w", 1, &local(), "session-lines", None, 2);
    r.ready_point(None).await; // 留存为空 ⇒ 只是过就绪点
    rec.clear();
    r.on_line_batch_awaited(lines("s", 0..3)).await; // credit 2 ⇒ 交 0、1，丢 2
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![("line", 0, 0), ("line", 1, 1)]
    );
    rec.clear();
    r.on_line_batch_awaited(lines("s", 3..4)).await; // credit 0 ⇒ 丢，位置 3
    assert!(rec.all().is_empty(), "没 credit 却交了");
    r.want("w", 1, 5); // 有 credit 了 ⇒ 当场说丢在哪：位置 [2, 4)
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![("gap", 2, 4)]
    );
    rec.clear();
    r.on_line_batch_awaited(lines("s", 4..5)).await;
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![("line", 4, 4)]
    );
    // credit：5 − 1（Gap 不占）＝ 4 ⇒ 再来 4 行都交、第 5 行丢
    rec.clear();
    r.on_line_batch_awaited(lines("s", 5..10)).await;
    assert_eq!(rec.all().len(), 4, "Gap 占了 credit");
}

/// ★ S1：一次攒出 ≥ 50 行 ⇒ 成批交：首块 `start` 开头、每块 `end` 收尾，末块先发（最新一段先到）。
#[tokio::test]
async fn a_bulk_increment_goes_out_as_a_batch_with_edges() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("w", 1, &local(), "session-lines", None, 10_000);
    r.ready_point(None).await;
    rec.clear();
    r.on_line_batch_awaited(lines("s", 0..700)).await;
    let got: Vec<_> = rec.all().iter().map(what).collect();
    assert_eq!(got.len(), 700 + 3, "700 行 ＋ 首块 start ＋ 两块各一个 end");
    assert_eq!(got[0].0, "start");
    assert_eq!(
        got[1],
        ("line", 1, 100),
        "末块先发：第一块是最新的 600 行（100..700）"
    );
    assert_eq!(got[601].0, "end");
    assert_eq!(got[602], ("line", 602, 0));
    assert_eq!(got[702].0, "end");
    // 位置连续
    for (k, g) in got.iter().enumerate() {
        assert_eq!(g.1, k as u64, "第 {k} 格的位置");
    }
}

/// ★ S2：就绪点之前一格都不交（实时的行也不交，只进留存）；就绪点按 credit 交留存（不够就等 `want`，**不丢**），
/// 交完才返回；过了就绪点的 webview 再订整台机器 ⇒ 当场交。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ready_point_replays_retained_lines_by_credit_and_waits_for_want() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("main", 1, &local(), "session-lines", None, 3);
    r.on_line_batch_awaited(lines("s", 0..5)).await;
    assert!(rec.all().is_empty(), "就绪点之前交了格");
    let r2 = r.clone();
    let done = tokio::spawn(async move { r2.ready_point(None).await });
    // 等它交掉 credit 那 3 格、停下来等 want
    for _ in 0..200 {
        if rec.all().len() >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![("start", 0, 0), ("line", 1, 0), ("line", 2, 1)]
    );
    assert!(
        !done.is_finished(),
        "credit 用完了还没等 —— 那就是丢了或越过了 credit"
    );
    r.want("main", 1, 100);
    done.await.unwrap();
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![
            ("start", 0, 0),
            ("line", 1, 0),
            ("line", 2, 1),
            ("line", 3, 2),
            ("line", 4, 3),
            ("line", 5, 4),
            ("end", 6, 0),
            // 有行、成品缓存里没说过、那台没报完清单 ⇒ 终局是「说不清」（不吃 credit，排在行后）。
            ("unseen", 7, 0)
        ]
    );
    // 过了就绪点：同一个 webview 再订一条（另一台机器的整台流）⇒ 当场交它的留存
    let mut remote = payload("rs", 0);
    remote.origin = Some("box".to_string());
    r.on_line_batch_awaited(vec![remote]).await;
    rec.clear();
    let boxo = crate::origin::Origin("box".to_string());
    r.origin_seen(&boxo, true);
    r.subscribe("main", 2, &boxo, "session-lines", None, 10);
    for _ in 0..200 {
        if rec.all().iter().any(|i| what(i).0 == "unseen") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![
            ("start", 0, 0),
            ("line", 1, 0),
            ("end", 2, 0),
            ("unseen", 3, 0)
        ]
    );
}

/// ★ S3：`session-lines/<sid>` 只交那一个会话（当场交留存、之后只交它的实时行）；别的机器的行一格都不交。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_scoped_subscription_only_gets_its_own_session() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    let mut far = payload("b", 9);
    far.origin = Some("box".to_string()); // 同名会话、别的机器
    let mut early = lines("a", 0..2);
    early.extend(lines("b", 0..2));
    early.push(far.clone());
    r.on_line_batch_awaited(early).await;
    r.subscribe("viewer-b", 1, &local(), "session-lines/b", None, 100);
    for _ in 0..200 {
        if rec.all().iter().any(|i| what(i).0 == "unseen") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![
            ("start", 0, 0),
            ("line", 1, 0),
            ("line", 2, 1),
            ("end", 3, 0),
            ("unseen", 4, 0) // 只它那一个会话的终局
        ]
    );
    rec.clear();
    let mut mix = lines("a", 2..3);
    mix.extend(lines("b", 2..3));
    mix.push(far);
    r.on_line_batch_awaited(mix).await;
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![("line", 5, 2)] // 位置 4 是上面那格终局
    );
}

/// ★ S4：说不了的原位说（`subscribe` 不失败）：`from` 给了 ⇒ `Closed{Peer(bad_args)}`；认不出的 `kind` ⇒
/// `Closed{Peer(no-such-stream)}`；空白名 ⇒ `Closed{Ours(Misuse)}` —— 三者都不登记（之后的行一格都不交）。
/// 那台此刻看不见 ⇒ 第一格 `Unseen{1 open Unreachable}`；看得见 / 又看不见各原位说一次，状态没变不重复说。
#[tokio::test]
async fn what_cannot_be_served_is_said_in_place_and_unseen_is_not_the_end() {
    let (r, rec) = hub();
    r.subscribe(
        "w",
        1,
        &local(),
        "session-lines",
        Some(crate::chan::wire::Cursor(vec![1])),
        9,
    );
    r.subscribe("w", 2, &local(), "nope", None, 9);
    r.subscribe(
        "w",
        3,
        &crate::origin::Origin(" ".into()),
        "session-lines",
        None,
        9,
    );
    let got = rec.0.lock().unwrap().clone();
    let peer_code = |i: &WItem| match i {
        WItem::Closed {
            by: WBy::Peer(WBody(b)),
        } => serde_json::from_slice::<serde_json::Value>(b).unwrap()["code"]
            .as_str()
            .unwrap()
            .to_string(),
        WItem::Closed { by: WBy::Ours(f) } => format!("ours:{f:?}"),
        other => panic!("不是 Closed：{other:?}"),
    };
    assert_eq!(
        got.iter()
            .map(|(_, id, i)| (*id, peer_code(&i[0])))
            .collect::<Vec<_>>(),
        vec![
            (1, "bad_args".to_string()),
            (2, "no-such-stream".to_string()),
            (3, "ours:Misuse".to_string())
        ]
    );
    r.ready_point(None).await;
    rec.clear();
    r.on_line_batch_awaited(lines("s", 0..1)).await;
    assert!(rec.all().is_empty(), "没登记成的订阅收到了行");

    r.subscribe("w", 4, &local(), "session-lines", None, 9);
    assert!(matches!(
        rec.all()[..],
        [WItem::Unseen { at, why: WHop::Unreachable }] if at.idx == 1 && at.tag == "open"
    ));
    rec.clear();
    r.origin_seen(&local(), true);
    r.origin_seen(&local(), true); // 没变 ⇒ 不重复说
    r.origin_seen(&local(), false);
    let seen: Vec<_> = rec.all().iter().map(what).map(|w| w.0).collect();
    assert_eq!(seen, vec!["seen", "unseen"]);
    assert!(matches!(
        rec.all()[1],
        WItem::Unseen { at, why: WHop::Dropped } if at.tag == "read"
    ));
}

/// S4：撤了就一格都不交（在等 credit 的重放也随之停）；同一个 `(webview, 编号)` 再订一次 ⇒ 旧的那条作废。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_and_resubscribe_leave_no_orphans() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.on_line_batch_awaited(lines("s", 0..4)).await;
    r.subscribe("main", 1, &local(), "session-lines", None, 1);
    let r2 = r.clone();
    let done = tokio::spawn(async move { r2.ready_point(None).await });
    for _ in 0..200 {
        if !rec.all().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    r.stop("main", 1);
    done.await.unwrap(); // 等 credit 的重放随撤而停，不挂着
    rec.clear();
    r.want("main", 1, 10);
    r.on_line_batch_awaited(lines("s", 4..5)).await;
    assert!(rec.all().is_empty(), "撤了之后还在交");
    // 重订同一个编号：新的那条从位置 0 起，旧的那条不再收
    r.subscribe("main", 1, &local(), "session-lines", None, 10);
    for _ in 0..200 {
        if rec.all().iter().any(|i| what(i).0 == "end") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let got: Vec<_> = rec.all().iter().map(what).collect();
    assert_eq!(got.first(), Some(&("start", 0, 0)), "新订阅不是从位置 0 起");
    assert_eq!(r.inner.lock().subs.len(), 1, "旧的那条还挂着");
}

/// ★**`accounts-changed` 那条流**（替掉裸事件 `remote-backend-ready`）只收三样：那台看不看得见（`Unseen` / `Seen`，
/// 与同台 `session-lines` **同一个来源** `origin_seen`）· 那台账号清单变了（恰好一格 `Frame`，体是约定那一串）· 丢过几格（`Gap`）。
///
/// 守的要求：「前端只有两个动作」· 看不见 ⇒ 第一格 `Unseen`，订阅照样成立 ·
/// `§3.3.4`（丢必须说，`Gap` 在原位）· `§15.3`（kind 由宿主注入的那一侧认）。
/// 两向隔离：会话行**不进** `accounts-changed`；账号那一格**不进** `session-lines`；别台的都不收。
/// 期望手写（位置、格的种类、体的原文），不从被测的计划函数派生。
#[tokio::test]
async fn the_accounts_changed_stream_carries_only_reachability_and_account_notices() {
    let (r, rec) = hub();
    let box_a = crate::origin::Origin("box-a".into());
    let box_b = crate::origin::Origin("box-b".into());
    let by_sub = |rec: &Rec| -> Vec<(u64, &'static str, u64)> {
        rec.0
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, id, items)| {
                items.iter().map(move |i| match i {
                    WItem::Frame { seq, body } => {
                        let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
                        let kind = if v.get("line").is_some() {
                            "line"
                        } else if v == serde_json::json!({"accounts_changed": true}) {
                            "accounts"
                        } else {
                            "other-frame"
                        };
                        (*id, kind, *seq)
                    }
                    WItem::Gap { from_seq, to_seq } => (
                        *id,
                        "gap",
                        from_seq * 100 + to_seq.expect("这里只会有有界的 Gap"),
                    ),
                    WItem::Unseen { .. } => (*id, "unseen", 0),
                    WItem::Seen { .. } => (*id, "seen", 0),
                    WItem::Closed { .. } => (*id, "closed", 0),
                })
            })
            .collect()
    };

    // 看不见时订 ⇒ 每条第一格都是 `Unseen`（订阅照样成立）。
    r.subscribe("w", 1, &box_a, "accounts-changed", None, 1);
    r.subscribe("w", 2, &box_a, "session-lines", None, 10);
    r.subscribe("w", 3, &box_b, "accounts-changed", None, 5);
    assert_eq!(
        by_sub(&rec),
        vec![(1, "unseen", 0), (2, "unseen", 0), (3, "unseen", 0)]
    );
    rec.clear();

    // 连上 ⇒ 这台的两条订阅都收 `Seen`（同一个来源）；别台的不收。
    r.origin_seen(&box_a, true);
    assert_eq!(by_sub(&rec), vec![(1, "seen", 0), (2, "seen", 0)]);
    rec.clear();

    // 账号清单变了 ⇒ 只有这台的 `accounts-changed` 收一格、位置 0、体是约定那一串。
    r.accounts_changed(&box_a);
    assert_eq!(by_sub(&rec), vec![(1, "accounts", 0)]);
    rec.clear();

    // 会话行 ⇒ 只进 `session-lines`（它过了就绪点之后）；`accounts-changed` 一格都不收，就绪点也不给它重放。
    r.ready_point(None).await;
    let mut remote_lines = lines("s", 0..2);
    for p in &mut remote_lines {
        p.origin = Some("box-a".into());
    }
    r.on_line_batch_awaited(remote_lines).await;
    assert_eq!(by_sub(&rec), vec![(2, "line", 0), (2, "line", 1)]);
    rec.clear();

    // credit：第 1 条一开始只给了 1 格（已被上面那一格用掉）⇒ 再来一次就丢、位置照占；
    // `want` 回来 ⇒ 原位说 `Gap[1, 2)`；之后那一格从位置 2 起。
    r.accounts_changed(&box_a);
    assert!(by_sub(&rec).is_empty(), "没 credit 却交了");
    r.want("w", 1, 3);
    assert_eq!(by_sub(&rec), vec![(1, "gap", 102)]);
    rec.clear();
    r.accounts_changed(&box_a);
    assert_eq!(by_sub(&rec), vec![(1, "accounts", 2)]);
    rec.clear();

    // 断了 ⇒ `Unseen`（不是终点）。
    r.origin_seen(&box_a, false);
    assert_eq!(by_sub(&rec), vec![(1, "unseen", 0), (2, "unseen", 0)]);
}

// ═══════════════════════════════════════════════════════════════════════
// 会话流 `session-tap`：同一张订阅表、同一套 credit 与 `Gap`（级 2），
// 不进留存（`history`）、不混进会话行那条流。设计住仓外。期望手写。
// ═══════════════════════════════════════════════════════════════════════

fn tap(origin: &str, n: u64) -> crate::ui_contract::SessionTapPayload {
    crate::ui_contract::SessionTapPayload {
        origin: crate::origin::Origin(origin.to_string()),
        stream: "sid-t".into(),
        run: None,
        resp: 0,
        n,
        ev: crate::ui_contract::RecordBody::from_json(format!(
            "{{\"t\":\"text\",\"i\":{n},\"s\":\"x\"}}"
        )),
        end: None,
    }
}

fn tap_what(i: &WItem) -> (&'static str, u64, u64) {
    match i {
        WItem::Frame { seq, body } => {
            let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
            match v.get("stream") {
                Some(_) => ("tap", *seq, v["n"].as_u64().unwrap()),
                None => what(i),
            }
        }
        other => what(other),
    }
}

/// T5b：订了 `session-tap` ⇒ 当场就是实时的（没有就绪点、没有重放）；有 credit 交、没 credit 丢且位置照占、`want` 回来原位 `Gap`；
/// 别的机器的 tap 不交；tap **不进留存**（`stats().history_len` 不变）。
#[tokio::test]
async fn tap_frames_ride_the_same_subscription_table_with_credit_and_in_place_gaps() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("w", 7, &local(), "session-tap", None, 2);
    rec.clear();
    for n in 0..3 {
        r.on_tap(tap("<local>", n));
    }
    r.on_tap(tap("pi", 9)); // 别的机器
    assert_eq!(
        rec.all().iter().map(tap_what).collect::<Vec<_>>(),
        vec![("tap", 0, 0), ("tap", 1, 1)]
    );
    rec.clear();
    r.want("w", 7, 4);
    assert_eq!(
        rec.all().iter().map(tap_what).collect::<Vec<_>>(),
        vec![("gap", 2, 3)]
    );
    rec.clear();
    r.on_tap(tap("<local>", 3));
    assert_eq!(
        rec.all().iter().map(tap_what).collect::<Vec<_>>(),
        vec![("tap", 3, 3)]
    );
    assert_eq!(r.stats().history_len, 0, "tap 进了重放留存");
}

/// T5c：两条流互不串：会话行不交给 tap 订阅、tap 不交给会话行订阅（两向）。
#[tokio::test]
async fn tap_and_lines_never_cross_into_each_others_subscriptions() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("w", 1, &local(), "session-lines", None, 100);
    r.subscribe("w", 2, &local(), "session-tap", None, 100);
    r.ready_point(None).await;
    rec.clear();
    r.on_line_batch_awaited(lines("s", 0..2)).await;
    r.on_tap(tap("<local>", 0));
    let got = rec.0.lock().unwrap().clone();
    let to = |sub: u64| -> Vec<(&'static str, u64, u64)> {
        got.iter()
            .filter(|(_, s, _)| *s == sub)
            .flat_map(|(_, _, i)| i.iter().map(tap_what).collect::<Vec<_>>())
            .collect()
    };
    assert_eq!(to(1), vec![("line", 0, 0), ("line", 1, 1)]);
    assert_eq!(to(2), vec![("tap", 0, 0)]);
}

/// 记录文件的出声交给**订了那台 / 那一个会话**的实时订阅，占 credit、原位 `Gap` 与行同一套；
/// 别的机器 / 别的会话的订阅收不到；不进留存（就绪点之后才订的那条，拿不到之前那一句）。
#[tokio::test]
async fn a_session_file_notice_goes_to_the_subscribers_of_that_session_only() {
    let (r, rec) = hub();
    r.origin_seen(&local(), true);
    r.subscribe("w", 1, &local(), "session-lines", None, 10);
    r.subscribe("w", 2, &local(), "session-lines/other", None, 10);
    r.subscribe(
        "w",
        3,
        &crate::origin::Origin("far".into()),
        "session-lines",
        None,
        10,
    );
    r.ready_point(None).await;
    rec.clear();
    r.on_session_notice(crate::ui_contract::SessionFileNoticePayload {
        session_id: "s".into(),
        origin: crate::origin::LOCAL.into(),
        path: "/p/s.jsonl".into(),
        change: "gone".into(),
    })
    .await;
    let got = rec.all();
    assert_eq!(got.len(), 1, "不是恰好交给订了本机整台的那一条：{got:?}");
    match &got[0] {
        WItem::Frame { body, .. } => {
            let v: serde_json::Value = serde_json::from_slice(&body.0).unwrap();
            assert_eq!(v["file_notice"]["session_id"], "s");
            assert_eq!(v["file_notice"]["change"], "gone");
        }
        other => panic!("{other:?}"),
    }
}

/// 「已从头重读」（截短 / 改写）⇒ 后端行号从 0 重数 ⇒ 留存里这个会话旧的一代
/// 同一拍丢（别的会话不动）；「不见了」不丢（没重读、号没换代）。
#[tokio::test]
async fn a_reread_notice_drops_the_old_generation_of_that_session_only() {
    for (change, s_left) in [
        ("truncated", vec![]),
        ("rewritten", vec![]),
        ("gone", vec![0u64, 1, 2]),
    ] {
        let replay = EventReplay::new();
        push_and_trim(&mut replay.inner.lock(), &lines("s", 0..3));
        push_and_trim(&mut replay.inner.lock(), &lines("t", 0..2));
        replay
            .on_session_notice(crate::ui_contract::SessionFileNoticePayload {
                session_id: "s".into(),
                origin: crate::origin::LOCAL.into(),
                path: "/p/s.jsonl".into(),
                change: change.into(),
            })
            .await;
        assert_eq!(seqs_of(&replay, "s"), s_left, "{change}");
        assert_eq!(
            seqs_of(&replay, "t"),
            vec![0, 1],
            "{change}：别的会话被连带丢了"
        );
    }
}

/// 逐字「超长行在流里**原位**给 `Gap`，加『知道丢了、不知道丢到哪』一形（`to_seq` 缺省）」：
/// 丢在第 2、3 行之间 ⇒ 这台的订阅在那两格之间恰好收一格 `Gap{from=2, to 缺}`，不占位置（下一行仍是位置 2）、不占 credit；
/// 别的机器的订阅一格不收。
#[tokio::test]
async fn a_line_lost_somewhere_is_said_in_place_as_an_open_gap() {
    let (r, rec) = hub();
    let far = crate::origin::Origin("far".to_string());
    r.origin_seen(&local(), true);
    r.origin_seen(&far, true);
    r.subscribe("w", 1, &local(), "session-lines", None, 3);
    r.subscribe("w", 2, &far, "session-lines", None, 3);
    r.ready_point(None).await;
    rec.clear();
    r.on_line_batch_awaited(lines("s", 0..2)).await;
    r.on_lost_somewhere(crate::origin::LOCAL);
    r.on_line_batch_awaited(lines("s", 2..3)).await;
    assert_eq!(
        rec.all().iter().map(what).collect::<Vec<_>>(),
        vec![
            ("line", 0, 0),
            ("line", 1, 1),
            ("gap", 2, u64::MAX),
            ("line", 2, 2)
        ]
    );
    let subs: Vec<u64> = rec.0.lock().unwrap().iter().map(|(_, id, _)| *id).collect();
    assert!(
        subs.iter().all(|id| *id == 1),
        "别的机器的订阅收到了：{subs:?}"
    );
}

/// ★ 「起停帧不吃 credit、不许丢（登记一条例外）」：一格 credit 都没有时，行照丢（原位 `Gap`），
/// 起停那几格照交、照占位置；就绪点把起停按成品缓存原位重放 —— 骨架在留存行之前、终局在之后。
#[tokio::test]
async fn mig1_lifecycle_frames_take_no_credit_and_the_ready_point_puts_them_around_the_lines() {
    use crate::session_book::{Fate, In, LiveMeta};
    let ((r, rec), book) = hub_with_book();
    r.origin_seen(&local(), true);
    book.write().step(In::Live {
        origin: "<local>".into(),
        sid: "a".into(),
        meta: LiveMeta::default(),
    });
    book.write().step(In::Left {
        origin: "<local>".into(),
        sid: "b".into(),
        fate: Fate::Ended,
    });
    r.on_line_batch_awaited(lines("b", 0..2)).await; // 就绪点之前：只进留存
    r.subscribe("w", 1, &local(), "session-lines", None, 10);
    r.ready_point(None).await;
    assert_eq!(
        rec.all().iter().map(|i| what(i).0).collect::<Vec<_>>(),
        vec!["live", "activity", "start", "line", "line", "end", "ended"],
        "就绪点：骨架 → 留存行 → 终局"
    );
    rec.clear();
    r.want("w", 1, 0);
    // 把 credit 用光（10 − 2 行 ＝ 8）。
    r.on_line_batch_awaited(lines("c", 0..8)).await;
    rec.clear();
    r.on_line_batch_awaited(lines("c", 8..9)).await; // 没 credit ⇒ 丢
    assert!(rec.all().is_empty());
    r.on_lifecycle(
        "<local>",
        vec![crate::ui_contract::SessionStreamFrame::Ended(
            crate::ui_contract::SessionEndedPayload {
                session_id: "c".into(),
            },
        )],
    );
    assert_eq!(
        rec.all()
            .iter()
            .map(what)
            .map(|(k, _, _)| k)
            .collect::<Vec<_>>(),
        vec!["gap", "ended"],
        "没 credit 也照交起停（先原位说丢在哪）"
    );
}

/// ★ 「起停帧不吃 credit、不许丢（登记一条例外）」—— 例外表就是 `SessionStreamFrame::takes_credit`：
/// 每一种格造一个、读它上线的键名，按吃不吃 credit 分两摞 == 金样（TS 那一侧 `CREDIT_EXEMPT_FRAMES` 读同一份，异源）。
#[test]
fn mig1_the_credit_exemption_is_exactly_the_registered_lifecycle_frames() {
    use crate::ui_contract::{self as b, SessionStreamFrame as F};
    let sid = || "s".to_string();
    let all = vec![
        F::Line(payload("s", 0)),
        F::Batch(b::BatchEdge::Start),
        F::FileNotice(b::SessionFileNoticePayload {
            session_id: sid(),
            origin: "<local>".into(),
            path: "/p".into(),
            change: "gone".into(),
        }),
        F::Live(b::SessionLivePayload {
            session_id: sid(),
            origin: "<local>".into(),
            kind: None,
            attachable: None,
            cwd: None,
            project_dir: None,
            name: None,
        }),
        F::Activity(b::SessionActivityPayload {
            session_id: sid(),
            status: None,
            waiting_for: None,
        }),
        F::Container(b::SessionContainerPayload {
            session_id: sid(),
            container: "tmux".into(),
        }),
        F::Idle(b::SessionIdlePayload { session_id: sid() }),
        F::Ended(b::SessionEndedPayload { session_id: sid() }),
        F::Unseen(b::SessionUnseenPayload {
            origin: crate::origin::Origin::local(),
        }),
        F::Listed(b::OriginSessionsListedPayload {
            origin: crate::origin::Origin::local(),
        }),
        F::SnapshotInflight(b::SnapshotInflightPayload { count: 1 }),
        F::Runs(b::SessionRunsPayload {
            session_id: "s".into(),
            runs: b::RecordBody::from_json("[]".into()).unwrap(),
            ended: b::RecordBody::from_json("[]".into()).unwrap(),
        }),
    ];
    let key = |f: &F| -> String {
        let v = serde_json::to_value(f).unwrap();
        v.as_object().unwrap().keys().next().unwrap().clone()
    };
    let mut credit: Vec<String> = all.iter().filter(|f| f.takes_credit()).map(key).collect();
    let mut exempt: Vec<String> = all.iter().filter(|f| !f.takes_credit()).map(key).collect();
    credit.sort();
    exempt.sort();
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/session-stream-credit.golden.json"
    ))
    .unwrap();
    assert_eq!(
        serde_json::json!({ "credit": credit, "exempt": exempt }),
        golden
    );
}

/// 只跟一个会话的订阅（`session-lines/<sid>`）收**机器级**「说不清」（它跟的那一条所在的那台看不见了），
/// 不收「清单报完了」这种整台的簿记；说别的会话的格照旧不收。
#[test]
fn a_single_session_subscription_hears_the_machine_level_unseen_only() {
    use crate::ui_contract::{self as b, SessionStreamFrame as F};
    let unseen = F::Unseen(b::SessionUnseenPayload {
        origin: crate::origin::Origin::local(),
    });
    let listed = F::Listed(b::OriginSessionsListedPayload {
        origin: crate::origin::Origin::local(),
    });
    let other = F::Ended(b::SessionEndedPayload {
        session_id: "other".into(),
    });
    assert!(unseen.reaches(Some("mine")) && unseen.reaches(None));
    assert!(!listed.reaches(Some("mine")) && listed.reaches(None));
    assert!(!other.reaches(Some("mine")) && other.reaches(Some("other")));
}

/// 〔要求住址 「`session.tasks` 推送改 `chan.subscribe(origin, …)`」〕`session-tasks` 流：
/// 这台某个会话的任务变了 ⇒ 只有订了这台 `session-tasks` 的收一格、体恰是 `{"sid": …}`；别台的 · 同台 `accounts-changed` 的都不收。期望手写。
#[tokio::test]
async fn the_session_tasks_stream_carries_the_sid_that_changed_and_nothing_else() {
    let (r, rec) = hub();
    let box_a = crate::origin::Origin("box-a".into());
    let box_b = crate::origin::Origin("box-b".into());
    r.subscribe("w", 1, &box_a, "session-tasks", None, 4);
    r.subscribe("w", 2, &box_a, "accounts-changed", None, 4);
    r.subscribe("w", 3, &box_b, "session-tasks", None, 4);
    rec.clear();
    r.tasks_changed(&box_a, "s1");
    let got: Vec<(u64, serde_json::Value)> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(_, id, items)| {
            items.iter().filter_map(move |i| match i {
                WItem::Frame { body, .. } => Some((*id, serde_json::from_slice(&body.0).unwrap())),
                _ => None,
            })
        })
        .collect();
    assert_eq!(got, vec![(1, serde_json::json!({"sid": "s1"}))]);
}

/// 〔「测试连接的进度不许倒退」〕`probe-progress/<票>` 流：本机后端推来一格 ⇒ 只有订了**那张票**的收、
/// 体原样（monitor 不解释）；别的票 · 别台同名 · 空票都不收（空票那一形订不上：`no-such-stream`）。期望手写。
#[tokio::test]
async fn the_probe_progress_stream_carries_the_cell_to_the_one_ticket_only() {
    let (r, rec) = hub();
    let local = crate::origin::Origin::local();
    let box_a = crate::origin::Origin("box-a".into());
    r.subscribe("w", 1, &local, "probe-progress/t-1", None, 4);
    r.subscribe("w", 2, &local, "probe-progress/t-2", None, 4);
    r.subscribe("w", 3, &box_a, "probe-progress/t-1", None, 4);
    rec.clear();
    r.subscribe("w", 4, &local, "probe-progress/", None, 4);
    let refused =
        rec.0.lock().unwrap().iter().any(|(_, id, items)| {
            *id == 4 && items.iter().any(|i| matches!(i, WItem::Closed { .. }))
        });
    assert!(refused, "空票那一形该当场说没有这条流");
    rec.clear();
    r.on_probe("t-1", r#"{"reached":"ssh"}"#.to_string());
    let got: Vec<(u64, serde_json::Value)> = rec
        .0
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(_, id, items)| {
            items.iter().filter_map(move |i| match i {
                WItem::Frame { body, .. } => Some((*id, serde_json::from_slice(&body.0).unwrap())),
                _ => None,
            })
        })
        .collect();
    assert_eq!(got, vec![(1, serde_json::json!({"reached": "ssh"}))]);
}
