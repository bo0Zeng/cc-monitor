//! 续传的判据。
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
        next_byte: None, // 说不准第 `next` 行的起点 ⇒ 挑锚那一形（确知起点那一形另有一格）
        witness: None,
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
    note_flushed(o, [(s, 0, None)]);
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
    note_flushed(o, [(s, 10, None), (s, 11, None)]);
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12));
    note_flushed(o, [(s, 20, None)]); // 跳号：中间那几行没拿到
    assert_eq!(cursor_of(o, s).map(|c| c.next), Some(12), "跳号不许前推");
    note_flushed(o, [(s, 5, None)]); // 重复的旧行
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
    let prod = crate::guard_support::stream_source_production();
    for anchor in [
        "crate::snapshot_resume::plan_read(",
        "crate::snapshot_resume::cursor_of(&origin, sid)",
        "crate::snapshot_resume::Walk::new(&how, &plan)",
        "let Some(seq) = walk.step() else {",
        "crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);",
        "crate::snapshot_resume::note_flushed(",
        // 「会话结束真的忘」搬进了 `LineIntake::removed`（远端与本机两个帧源共用那一处）。
        "crate::snapshot_resume::forget(&crate::origin::Origin(self.origin_label.clone()), sid);",
    ] {
        guard_core::find_pinned(&prod, anchor)
            .unwrap_or_else(|e| panic!("stream_source 生产段里 `{anchor}` 不是恰好一处：{e}"));
    }
}

// ═══ 截断 / 改写检测：续传之前先核锚那一行 ═══════════════════════════
//
// （逐字）「它另一半用处 —— **截断检测**（远端 jsonl 在断连期间被截断/分叉，
// `(sid,seq)` 会指向不同的行而没有东西会发现）—— **仍开**（W5-VIS）」。

/// 把一份全文按 `page` 字节左右切成若干页（每页切在行尾 —— 同 `frame_query::Page` 的约定），回 `(页起点, 页文本)`。
fn pages(text: &str, from: usize, upto: usize, page: usize) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut at = from;
    while at < upto {
        let want = (at + page).min(upto);
        let cut = text[at..upto]
            .char_indices()
            .filter(|(i, c)| *c == '\n' && at + i + 1 >= want)
            .map(|(i, _)| at + i + 1)
            .next()
            .unwrap_or(upto);
        out.push((at as u64, text[at..cut].to_string()));
        at = cut;
    }
    out
}

/// 后端那一页的逐行成品，在夹具这一侧**自己**造（异源：按原文数行尾、自己算 FNV-1a 64，不调后端也不调被测代码）：
/// 每个可计行 `{end, hash}`（不进界面 ⇒ 没有成品，这里用不着）。
fn rows_of_page(off: u64, body: &str) -> Vec<crate::frame_query::Row> {
    let fnv = |b: &[u8]| {
        b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &x| {
            (h ^ u64::from(x)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    for seg in body.split_inclusive('\n') {
        let had_nl = seg.ends_with('\n');
        let line = seg.strip_suffix('\n').unwrap_or(seg);
        let line = line.strip_suffix('\r').unwrap_or(line);
        at += seg.len();
        if line.trim_start_matches('\u{feff}').trim().is_empty() {
            continue;
        }
        out.push(crate::frame_query::Row {
            end: had_nl.then_some(off + at as u64),
            hash: fnv(line.as_bytes()),
            record: None,
            cwd: None,
        });
    }
    out
}

/// 照生产那一遍（`fetch_snapshot`）走：逐页的行成品 → 区间（`row_spans`）→ `WitnessPick`。
fn pick_over(text: &str, how: &Read, plan: &TailPlan, page: usize) -> Option<Option<Witness>> {
    let walk = Walk::new(how, plan);
    let mut pick = WitnessPick::default();
    for (from, upto) in walk.segments().to_vec() {
        for (off, body) in pages(text, from as usize, upto as usize, page) {
            let rows = rows_of_page(off, &body);
            for (row, span) in rows.iter().zip(row_spans(off, &rows)) {
                pick.see(upto, plan.end, row.hash, span);
            }
        }
    }
    pick.done()
}

/// ① 逐行的字节区间：起点 ＝ 上一个可计行的末端（中间的空白行算进下一行的区间）、末端是后端给的；与**从原文独立数出**的相等；
/// 残尾（末端 `None`）那一行 ⇒ `None`。
#[test]
fn w5vis_row_spans_run_from_one_countable_end_to_the_next() {
    let text = "{\"a\":1}\r\n\n  \n{\"中文\":\"é\"}\n{\"z\":0}\n{\"torn";
    let rows = rows_of_page(100, text);
    let got = row_spans(100, &rows);
    let e1 = 100 + "{\"a\":1}\r\n".len() as u64;
    let e2 = 100 + "{\"a\":1}\r\n\n  \n{\"中文\":\"é\"}\n".len() as u64;
    let e3 = e2 + "{\"z\":0}\n".len() as u64;
    assert_eq!(
        got,
        vec![Some((100, e1)), Some((e1, e2)), Some((e2, e3)), None],
        "区间与原文独立数出的不相等"
    );
}

/// ② ★ 见证两向：快照走完挑出来的见证 == 文件最后一个可计行（区间由夹具独立数出）；
/// 盘上只是**追加**了新行 ⇒ 还是那一行（续传照接）；**整份改写而且变长**（上一道「没变短」拦不住的那一形）⇒ 不是了。
#[test]
fn w5vis_the_witness_tells_an_append_from_a_rewrite_that_grew() {
    let (text, rows) = fixture(120);
    let plan = plan_of(&text, &rows, 40);
    let read_back =
        |t: &str, w: &Witness| rows_of_page(w.start, &t[w.start as usize..w.end as usize]);
    for page in [64usize, 1_000, 1 << 20] {
        let w = pick_over(&text, &Read::Full, &plan, page)
            .expect("末端那一段读到了可计行")
            .expect("末端说得准");
        let (_, last_body) = rows.last().unwrap();
        assert_eq!(
            w.end,
            text.len() as u64,
            "页大小 {page}：见证不是最后一个可计行"
        );
        assert_eq!(
            &text[w.start as usize..w.end as usize].trim_start(),
            &format!("{last_body}\n"),
            "页大小 {page}：见证区间不是最后那一行（前面只许夹空白行）"
        );
        assert!(witness_holds(&w, &read_back(&text, &w)));
        // 追加：前缀原样 ⇒ 同一行。
        let appended = format!("{text}{{\"more\":1}}\n");
        assert!(witness_holds(&w, &read_back(&appended, &w)));
        // 整份改写而且变长：最后那一行改了一个字、后面又追加了很多 ⇒ 文件比锚长（`plan_read` 照续传），但见证对不上。
        let mut rewritten = text.clone();
        let at = w.end as usize - 3;
        rewritten.replace_range(at..at + 1, "#");
        rewritten.push_str(&"{\"grown\":true}\n".repeat(50));
        assert!(rewritten.len() > text.len());
        assert!(
            !witness_holds(&w, &read_back(&rewritten, &w)),
            "页大小 {page}：改写过的那一行没被认出"
        );
    }
    // 读回来的那一段不是恰好一行 / 末端对不上 ⇒ 不是。
    let one = rows_of_page(0, "abc\n");
    let w = Witness {
        start: 0,
        end: 4,
        hash: one[0].hash,
    };
    assert!(witness_holds(&w, &one));
    assert_eq!(
        rows_of_page(0, "abc\r\n")[0].hash,
        w.hash,
        "行尾的 `\\r` 不进摘要"
    );
    assert!(
        !witness_holds(&w, &rows_of_page(0, "ab\nc\n"))
            && !witness_holds(&w, &rows_of_page(0, "abc"))
    );
}

/// ③ 三形：末端那一段一行可计行都没读到 ⇒ 旧见证照留（锚那一行没变）；读到了但说不准 ⇒ 清掉；读到了 ⇒ 换新。
/// 立锚那一步也把旧见证带过去（不在 `note_snapshot_done` 里悄悄丢掉）。
#[test]
fn w5vis_the_witness_is_kept_cleared_or_replaced_by_what_the_walk_saw() {
    let o = &Origin("w5vis-host".into());
    let s = "w5vis-sid";
    let plan = TailPlan {
        total: 3,
        tail_from: 0,
        split_at: 0,
        end: 30,
    };
    let w1 = Witness {
        start: 20,
        end: 30,
        hash: 7,
    };
    note_snapshot_done(o, s, "/p.jsonl", &plan);
    note_witness(o, s, Some(Some(w1.clone())));
    assert_eq!(cursor_of(o, s).unwrap().witness, Some(w1.clone()));
    note_snapshot_done(o, s, "/p.jsonl", &plan);
    note_witness(o, s, None);
    assert_eq!(
        cursor_of(o, s).unwrap().witness,
        Some(w1),
        "没读到新行却把见证丢了"
    );
    note_witness(o, s, Some(None));
    assert_eq!(
        cursor_of(o, s).unwrap().witness,
        None,
        "说不准的那一次没清掉旧见证"
    );
    // 续传一段、末端那一段里没有可计行（只长了空行）⇒ `None`。
    let text = "{\"a\":1}\n\n\n";
    let p = TailPlan {
        total: 1,
        tail_from: 0,
        split_at: 0,
        end: text.len() as u64,
    };
    let how = Read::Resume {
        from_byte: 8,
        upto: p.end,
        first_seq: 1,
        skip_below: 1,
    };
    assert_eq!(pick_over(text, &how, &p, 64), None);
    forget(o, s);
}

/// ④ 接线（剥注释后的 `stream_source` 生产段，锚各恰好一处）：续传之前先读回见证那一段并核（在 `Walk::new` 之前）；
/// 对不上 ⇒ 续点作废、改整份、交「被改过」那一格；走读时挑见证、立锚之后记下。正控：缺核那一步的合成语料必须被认出。
#[test]
fn w5vis_fetch_snapshot_checks_the_witness_before_it_resumes() {
    fn wired(prod: &str) -> Result<(), String> {
        let at = |a: &str| guard_core::find_pinned(prod, a).map_err(|e| format!("`{a}`：{e}"));
        let check = at("crate::snapshot_resume::witness_holds(&w, &page.rows)")?;
        let walk = at("crate::snapshot_resume::Walk::new(&how, &plan)")?;
        let full = at("how = crate::snapshot_resume::Read::Full;")?;
        let told = at("change: FileChange::Rewritten.as_wire().to_string(),")?;
        let forgot = at("crate::snapshot_resume::forget(&origin, sid);")?;
        at("pick.see(upto, plan.end, row.hash, span);")?;
        let done = at("crate::snapshot_resume::note_snapshot_done(&origin, sid, path, &plan);")?;
        let noted = at("crate::snapshot_resume::note_witness(&origin, sid, pick.done());")?;
        if !(check < forgot && forgot < told && told < full && full < walk) {
            return Err("核 → 作废 → 出声 → 改整份 → 走读 的次序不对".into());
        }
        if noted < done {
            return Err("见证在立锚之前就记了（立锚会带着旧的盖过去）".into());
        }
        Ok(())
    }
    let prod = crate::guard_support::stream_source_production();
    wired(&prod).unwrap_or_else(|e| panic!("{e}"));
    let old = prod.replacen(
        "crate::snapshot_resume::witness_holds(&w, &page.rows)",
        "true",
        1,
    );
    assert!(wired(&old).is_err(), "摘掉核那一步没被认出 —— 量具瞎了");
}

/// 逐字「续传：『锚到续点那一截照样过线』要修 —— 续订从续点 seq 起发」：
/// 推续点的那一行带着自己的末端 ⇒ 续传只读 `[第 next 行的起点, end)`，一行都不数掉（过线的 == 发出去的）。
/// 阴性：推的那一行说不准末端 ⇒ 退回挑锚（锚到续点那一截照样过线）。
#[test]
fn a_resume_reads_from_the_cursor_line_itself() {
    let (text, rows) = fixture(300);
    let o = &crate::origin::Origin("r2-resume-origin".to_string());
    let s = "r2-resume-sid";
    forget(o, s);
    let anchor = plan_of(&text[..rows[120].0 as usize], &rows[..120], 500);
    note_snapshot_done(o, s, "/p.jsonl", &anchor);
    // 实时行 120..170 连续到达，各带末端（= 下一行的起点）
    note_flushed(o, (120..170).map(|i| (s, i as u64, Some(rows[i + 1].0))));
    let plan = plan_of(&text, &rows, 500);
    let how = plan_read(cursor_of(o, s).as_ref(), "/p.jsonl", &plan);
    assert_eq!(
        how,
        Read::Resume {
            from_byte: rows[170].0,
            upto: plan.end,
            first_seq: 170,
            skip_below: 170
        }
    );
    let (sent, walk) = run(&text, &how, &plan);
    let want: Vec<(u64, String)> = (170..300).map(|i| (i as u64, rows[i].1.clone())).collect();
    assert_eq!(sent, want);
    assert_eq!(
        walk.arrived(),
        130,
        "过线的行数 == 发出去的行数（一行都没数掉）"
    );
    // 阴性：末端说不准
    note_flushed(o, [(s, 170, None)]);
    let how = plan_read(cursor_of(o, s).as_ref(), "/p.jsonl", &plan);
    assert!(matches!(how, Read::Resume { skip_below: 171, first_seq, .. } if first_seq < 171));
    forget(o, s);
}
