//! 〔C2 · U3 第 3 件〕续传的判据。
//!
//! 买到：续点在时**只读锚之后那一截、第一条发出去的行号恰好是续点**，每条发出去的行号与它的正文逐行对得上
//! （夹具是一份自己数出行边界的假会话，期望值由夹具独立数出，不调被测函数）· 续点作废的三形都回整份 ·
//! 实时行不连续时续点不前推 · `fetch_snapshot` / `flush_lines` 真的走这一份（`find_pinned` 锚恰好一处）。
//! **买不到**：真远端的重连（帧面那两问的真往返）；那一圈由 `frame_query` 自己的判据与真机读数管。

use super::*;

/// 造一份假会话：`n` 行，每 7 行插一个空行（空行不占号 —— 与后端 `line_counts` 同口径）。
/// 回 `(全文, 每个可计行的 (起字节, 正文))`。
fn fixture(n: usize) -> (String, Vec<(u64, String)>) {
    let mut text = String::new();
    let mut rows = Vec::new();
    for i in 0..n {
        if i % 7 == 3 {
            text.push_str("   \n");
        }
        let body = format!("{{\"i\":{i},\"pad\":\"{}\"}}", "x".repeat(i % 5));
        rows.push((text.len() as u64, body.clone()));
        text.push_str(&body);
        text.push('\n');
    }
    (text, rows)
}

/// 照后端 `history_query::tail_plan` 的定义从夹具**自己数**出那张图（不调被测代码）。
fn plan_of(text: &str, rows: &[(u64, String)], n_tail: usize) -> TailPlan {
    let total = rows.len() as u64;
    let keep = n_tail.max(1) as u64;
    let tail_from = total - total.min(keep);
    let end = text.len() as u64;
    let split_at = rows.get(tail_from as usize).map_or(end, |r| r.0);
    TailPlan {
        total,
        tail_from,
        split_at,
        end,
    }
}

/// 按 [`Walk`] 走一遍夹具，回「发出去的 (行号, 正文)」。
fn run(text: &str, how: &Read, plan: &TailPlan) -> (Vec<(u64, String)>, Walk) {
    let mut walk = Walk::new(how, plan);
    let mut sent = Vec::new();
    for (from, upto) in walk.segments().to_vec() {
        let seg = &text[from as usize..upto as usize];
        for line in seg.split('\n') {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(seq) = walk.step() {
                sent.push((seq, line.to_string()));
            }
        }
    }
    (sent, walk)
}

fn cursor(path: &str, anchor_total: u64, anchor_end: u64, next: u64) -> Cursor {
    Cursor {
        path: path.into(),
        anchor_total,
        anchor_end,
        next,
    }
}

/// ★ 核心：续点在 ⇒ 不从 0 重发。第一条发出去的就是续点那一行，之后逐行连续到末尾，每行正文对得上。
#[test]
fn a_reconnect_resumes_from_the_cursor_not_from_line_zero() {
    let (text, rows) = fixture(300);
    // 上一条连接：做完快照时 120 行（锚），之后实时行连续推到 170。
    let anchor = plan_of(&text[..rows[120].0 as usize], &rows[..120], 500);
    assert_eq!((anchor.total, anchor.end), (120, rows[120].0));
    let c = cursor("/p.jsonl", anchor.total, anchor.end, 170);
    // 重连：文件长到 300 行；尾段 500 行（整份都在尾段里 ⇒ 这次的锚是第 0 行，挑的是续点那个锚）。
    let plan = plan_of(&text, &rows, 500);
    let how = plan_read(Some(&c), "/p.jsonl", &plan);
    let Read::Resume {
        from_byte,
        first_seq,
        skip_below,
        upto,
    } = how
    else {
        panic!("续点在、文件只长不短，却要整份重拉：{how:?}");
    };
    assert_eq!(
        (from_byte, first_seq, skip_below, upto),
        (rows[120].0, 120, 170, plan.end)
    );
    assert_ne!(from_byte, 0, "续传从第 0 个字节读起 = 没续");
    let (sent, walk) = run(&text, &how, &plan);
    let want: Vec<(u64, String)> = (170..300).map(|i| (i as u64, rows[i].1.clone())).collect();
    assert_eq!(
        sent, want,
        "发出去的应恰好是续点到末尾、行号与正文逐行对得上"
    );
    assert_eq!(
        (walk.arrived(), walk.want()),
        (180, 180),
        "完整性校验数的是锚之后那一截"
    );
}

/// 这一次的尾段起点比续点那个锚更近 ⇒ 用它（少过线）。
#[test]
fn the_nearer_of_the_two_anchors_is_used() {
    let (text, rows) = fixture(300);
    let c = cursor("/p.jsonl", 10, rows[10].0, 260);
    let plan = plan_of(&text, &rows, 60); // tail_from = 240 ≤ 260
    let how = plan_read(Some(&c), "/p.jsonl", &plan);
    assert_eq!(
        how,
        Read::Resume {
            from_byte: rows[240].0,
            upto: plan.end,
            first_seq: 240,
            skip_below: 260
        }
    );
    let (sent, _) = run(&text, &how, &plan);
    assert_eq!(sent.first().map(|s| s.0), Some(260));
    assert_eq!(sent.len(), 40);
    assert!(sent
        .iter()
        .all(|(seq, body)| rows[*seq as usize].1 == *body));
}

/// 续点作废的三形 ⇒ 整份（正控：整份那条也逐行对得上、覆盖 `0..total`）。
#[test]
fn a_stale_cursor_falls_back_to_the_whole_file() {
    let (text, rows) = fixture(200);
    let plan = plan_of(&text, &rows, 50);
    // 没有续点
    assert_eq!(plan_read(None, "/p.jsonl", &plan), Read::Full);
    // 换了路径
    let c = cursor("/other.jsonl", 100, rows[100].0, 120);
    assert_eq!(plan_read(Some(&c), "/p.jsonl", &plan), Read::Full);
    // 文件变短（截断重写）：锚比现在的末字节还远 / 续点超过现在的总行数
    let c = cursor("/p.jsonl", 100, plan.end + 1, 120);
    assert_eq!(plan_read(Some(&c), "/p.jsonl", &plan), Read::Full);
    let c = cursor("/p.jsonl", 100, rows[100].0, 201);
    assert_eq!(plan_read(Some(&c), "/p.jsonl", &plan), Read::Full);
    // 整份：尾段先到、头段回填，行号覆盖 0..total 各一次、正文对得上
    let (sent, walk) = run(&text, &Read::Full, &plan);
    let mut seqs: Vec<u64> = sent.iter().map(|s| s.0).collect();
    assert_eq!(seqs[0], 150, "尾段先到");
    seqs.sort_unstable();
    assert_eq!(seqs, (0..200).collect::<Vec<u64>>());
    assert!(sent
        .iter()
        .all(|(seq, body)| rows[*seq as usize].1 == *body));
    assert_eq!((walk.arrived(), walk.want()), (200, 200));
}

/// 续点只在**连续**到达时前推；立锚取「原有的」与 `total` 的大者；会话真结束就忘掉。
#[test]
fn the_cursor_only_moves_on_contiguous_lines() {
    let o = &crate::origin::Origin("c2-test-origin".to_string());
    let s = "c2-test-sid";
    forget(o, s);
    note_flushed(o, [(s, 0)]);
    assert_eq!(
        cursor_of(o, s),
        None,
        "没立锚之前不记（实时行之前那一段还没拿到）"
    );
    let plan = TailPlan {
        total: 10,
        tail_from: 0,
        split_at: 0,
        end: 100,
    };
    note_snapshot_done(o, s, "/p.jsonl", &plan);
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(10));
    note_flushed(o, [(s, 10), (s, 11)]);
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12));
    note_flushed(o, [(s, 20)]); // 跳号：中间那几行没拿到
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12), "跳号不许前推");
    note_flushed(o, [(s, 5)]); // 重复的旧行
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12));
    // 再做完一次（总数没超过已有）⇒ 不回退
    note_snapshot_done(o, s, "/p.jsonl", &plan);
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12));
    forget(o, s);
    assert_eq!(cursor_of(o, s), None);
}

/// ★ 接线：快照那条路真的问续点、真的按 `Walk` 走、做完真的立锚；发行那条真的推续点；会话结束真的忘。
/// 锚串各恰好一处（`find_pinned`：裸 `contains` 在锚被撑大时照样绿）。
#[test]
fn the_snapshot_path_is_wired_through_the_cursor() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    for anchor in [
        "crate::snapshot_resume::plan_read(",
        "crate::snapshot_resume::cursor_of(&origin, sid)",
        "crate::snapshot_resume::Walk::new(&how, &plan)",
        "let Some(seq) = walk.step() else {",
        "crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);",
        "crate::snapshot_resume::note_flushed(",
        // 〔CF1〕「会话结束真的忘」搬进了 `LineIntake::removed`（远端与本机两个帧源共用那一处）。
        "crate::snapshot_resume::forget(&crate::origin::Origin(self.origin_label.clone()), sid);",
    ] {
        guard_core::find_pinned(&prod, anchor)
            .unwrap_or_else(|e| panic!("ssh_source 生产段里 `{anchor}` 不是恰好一处：{e}"));
    }
}
