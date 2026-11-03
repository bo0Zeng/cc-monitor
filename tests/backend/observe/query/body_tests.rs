use super::*;

// 判据照线上的样子读（成品过一遍真序列化器）：下面三个把有类型的那几个口包成 JSON，盖住 `super::*` 里的同名口。
fn record_lines<'b>(
    reader: &mut Reader<'_>,
    path: &Path,
    seq: u64,
    lines: impl IntoIterator<Item = (&'b [u8], u64, Option<u64>)>,
) -> (Vec<Value>, u64) {
    let (got, next) = super::record_lines(reader, path, seq, lines);
    (
        got.iter()
            .map(|l| serde_json::to_value(l).unwrap())
            .collect(),
        next,
    )
}

fn record_lines_of_page(
    reader: &mut Reader<'_>,
    path: &Path,
    seq: u64,
    offset: u64,
    bytes: &[u8],
) -> (Vec<Value>, u64) {
    let (got, next) = super::record_lines_of_page(reader, path, seq, offset, bytes);
    (
        got.iter()
            .map(|l| serde_json::to_value(l).unwrap())
            .collect(),
        next,
    )
}

fn run_rows(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<Value> {
    super::run_rows(reader, offset, bytes)
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect()
}

/// 行摘要按线上的样子看（`ReadRow` 经 serde）。
fn rows_v(reader: &mut Reader<'_>, offset: u64, bytes: &[u8]) -> Vec<Value> {
    rows_of(reader, offset, bytes)
        .into_iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect()
}

fn claude() -> RecordFace {
    crate::agents::claudecode::RECORDS
}

fn rd(face: &RecordFace) -> Reader<'_> {
    Reader::new(face, Default::default())
}

const USER: &str = r#"{"type":"user","uuid":"u1","timestamp":"t","cwd":"/w","message":{"role":"user","content":"q"}}"#;
const MODE: &str = r#"{"type":"mode","mode":"normal"}"#;

/// 行摘要：每个可计行一条；末端是含 `\n` 之后那个字节（CRLF 的 `\r` 计在内）· 残尾 `null` · 空白行不占 ·
/// 不进界面的只有 `{end, hash}`。
#[test]
fn rows_carry_exact_ends_and_only_displayable_messages() {
    let page = format!("{USER}\r\n\n  \n{MODE}\n{{torn");
    let face = claude();
    let rows = rows_v(&mut rd(&face), 100, page.as_bytes());
    assert_eq!(rows.len(), 3, "{rows:?}");
    let u = USER.len() as u64 + 2;
    assert_eq!(rows[0]["end"], 100 + u);
    assert_eq!(rows[0]["cwd"], "/w");
    assert_eq!(rows[0]["record"]["id"], "u1");
    assert_eq!(rows[1]["end"], 100 + u + 1 + 3 + MODE.len() as u64 + 1);
    assert!(
        rows[1].get("record").is_none(),
        "没有读者的元数据记录不带成品"
    );
    assert!(rows[2]["end"].is_null(), "残尾没有末端");
    // 摘要只看正文（去掉 `\r`）：同一行 LF / CRLF 两种收尾摘要相同。
    assert_eq!(rows[0]["hash"], line_hash(USER.as_bytes()));
}

/// 记录行：第 k 个可计行是 `seq + k`，只出进界面的那些；`nextSeq` 数的是可计行。
#[test]
fn record_lines_number_countable_lines_and_keep_only_displayable() {
    let page = format!("{MODE}\n\n{USER}\n");
    let face = claude();
    let (lines, next) = record_lines_of_page(
        &mut rd(&face),
        std::path::Path::new("/p/abc.jsonl"),
        7,
        0,
        page.as_bytes(),
    );
    assert_eq!(next, 9);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["seq"], 8);
    assert_eq!(lines[0]["session_id"], "abc");
    assert_eq!(lines[0]["path"], "/p/abc.jsonl");
    assert_eq!(lines[0]["record"]["t"], "said");
}

/// 落在 Codex 记录根下 ⇒ Codex 那一家解释；否则 ⇒ 记录树那一家（Claude）。
#[test]
fn the_record_face_follows_the_root_the_file_lives_under() {
    let codex_msg = r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}"#;
    fn codex_root() -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from("/codex-root"))
    }
    fn no_sessions() -> Vec<crate::agents::SynthSession> {
        Vec::new()
    }
    fn no_excerpt(_: &std::path::Path) -> String {
        String::new()
    }
    fn home() -> Option<std::path::PathBuf> {
        None
    }
    fn bare(kind: &'static str) -> crate::agents::Adapter {
        crate::agents::Adapter {
            kind,
            home,
            account_env: None,
            assets: None,
            history: None,
            upstream: None,
            mcp: None,
            footprint: None,
            accounts: None,
            records: None,
            processes: None,
            launch: None,
            compact_request: None,
            clear_request: None,
            local: None,
            live: crate::agents::LiveBy::Unknown,
            asks: None,
            attach: None,
        }
    }
    let reg = [
        crate::agents::Adapter {
            history: Some(crate::agents::HistoryFace {
                sessions: no_sessions,
                excerpt: no_excerpt,
                root: codex_root,
            }),
            records: Some(crate::agents::codex::RECORDS),
            ..bare("codex-like")
        },
        crate::agents::Adapter {
            records: Some(claude()),
            ..bare("claude-like")
        },
    ];
    let face =
        crate::agents::record_face_among(&reg, std::path::Path::new("/codex-root/2026/x.jsonl"))
            .expect("Codex 根下的文件没人认");
    let p = (face.parse)(&crate::agents::Line::new(codex_msg, 0))
        .unwrap()
        .unwrap();
    let r = p.record.expect("Codex 那一家没出记录");
    assert_eq!((r.agent.as_str(), &r.body), ("codex", &r.body));
    assert!(matches!(r.body, crate::agents::record::Body::Reply { .. }));
    let face = crate::agents::record_face_among(
        &reg,
        std::path::Path::new("/home/u/.claude/projects/a/s.jsonl"),
    )
    .expect("记录树下的文件没人认");
    let p = (face.parse)(&crate::agents::Line::new(USER, 0))
        .unwrap()
        .unwrap();
    assert_eq!(p.cwd.as_deref(), Some("/w"));
    assert_eq!(p.record.unwrap().agent, "claude");
    // Codex 的事件行 ⇒ 不出记录（不上线）；空行两家都 `Ok(None)`。
    let evt =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_complete","turn_id":"x"}}"#;
    let p = (crate::agents::codex::RECORDS.parse)(&crate::agents::Line::new(evt, 0))
        .unwrap()
        .unwrap();
    assert!(p.record.is_none());
    assert!(
        (crate::agents::codex::RECORDS.parse)(&crate::agents::Line::new("  ", 0))
            .unwrap()
            .is_none()
    );
    assert!((claude().parse)(&crate::agents::Line::new("", 0))
        .unwrap()
        .is_none());
}

// 拿帧面那一层的投影（`faces::project`）去剥一页成品的那两条挂主 crate：`body_on_upper_tests.rs`。

const ENQ: &str = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-01-02T03:00:00.000Z","content":"also this"}"#;
const REM: &str = r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-01-02T03:02:00.000Z","content":"also this","reason":"absorbed_mid_turn","commandUuid":"c1"}"#;
const SLIPPED: &str = r#"{"type":"attachment","uuid":"q1","parentUuid":"u1","timestamp":"2026-01-02T03:00:00.000Z","attachment":{"type":"queued_command","prompt":"also this","source_uuid":"c1","commandMode":"prompt","origin":{"kind":"human"}}}"#;

/// 一轮中途插进去的那一句：排队那几行（进队 · 被吸进那一轮）不出记录；紧跟着的那一行 attachment 出一条人说的，
/// 身份 ＝ 它自己的 uuid，时刻 ＝ 它自己的（就是打字那一刻）——按页 · 按行摘要两条读路一样，不靠往回看去配。
#[test]
fn a_line_slipped_in_mid_turn_comes_out_once_with_its_own_identity_and_moment() {
    let face = claude();
    let at = std::path::Path::new("/p/q.jsonl");
    let page = format!("{ENQ}\n{USER}\n{REM}\n{SLIPPED}\n");
    let (lines, _) = record_lines_of_page(&mut rd(&face), at, 0, 0, page.as_bytes());
    let got: Vec<(&str, &str, &str)> = lines
        .iter()
        .filter(|l| l.get("record").is_some())
        .map(|l| {
            let r = &l["record"];
            (
                r["t"].as_str().unwrap_or(""),
                r["id"].as_str().unwrap_or(""),
                r["at"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("said", "u1", "t"),
            ("said", "q1", "2026-01-02T03:00:00.000Z")
        ],
        "{lines:?}"
    );
    let rows: Vec<_> = rows_v(&mut rd(&face), 0, page.as_bytes())
        .into_iter()
        .filter(|r| r.get("record").is_some())
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[1]["record"]["id"], "q1");
}

/// 没有自己身份的那几条（标题）`id` 按这一行的起点偏移合成：各条读路（按页 · 按行摘要）给出的一样、互不相撞。
#[test]
fn records_without_their_own_id_get_one_from_where_the_line_starts() {
    let face = claude();
    let title = r#"{"type":"ai-title","aiTitle":"x","sessionId":"s"}"#;
    let renamed = r#"{"type":"custom-title","customTitle":"y","sessionId":"s"}"#;
    let page = format!("{title}\n{renamed}\n");
    let rows = rows_v(&mut rd(&face), 1000, page.as_bytes());
    let (lines, _) = record_lines_of_page(
        &mut rd(&face),
        std::path::Path::new("/p/s.jsonl"),
        0,
        1000,
        page.as_bytes(),
    );
    let ids: Vec<_> = rows.iter().map(|r| r["record"]["id"].clone()).collect();
    assert_eq!(
        ids,
        [
            serde_json::json!("@1000"),
            serde_json::json!(format!("@{}", 1001 + title.len()))
        ]
    );
    let ids2: Vec<_> = lines.iter().map(|l| l["record"]["id"].clone()).collect();
    assert_eq!(ids, ids2);
}

/// 认不出的行：相邻、同一个原因同一个类型的并成一条（`count` 记几条、`text` 带数）；中间隔着不上屏的行也算相邻；
/// 换了类型 ⇒ 另起一条。行摘要里被并掉的那一行照占号、不带记录；记录行与子运行那一页只出并好的那一条。
#[test]
fn adjacent_unread_lines_fold_into_one() {
    // 带链上身份（没身份的未知类型是簿记行、不成卡，`record_of_tests::a_bookkeeping_line_without_identity_is_no_card`）。
    const NEW: &str = r#"{"type":"brand-new","uuid":"n","sessionId":"s"}"#;
    const OTHER: &str = r#"{"type":"other-new","uuid":"o","sessionId":"s"}"#;
    let page = format!("{NEW}\n{MODE}\n{NEW}\n{NEW}\n{OTHER}\n{USER}\n{NEW}\n");
    let face = claude();
    let rows = rows_v(&mut rd(&face), 0, page.as_bytes());
    let recs: Vec<&Value> = rows.iter().filter_map(|r| r.get("record")).collect();
    let shape: Vec<(String, u64)> = recs
        .iter()
        .map(|r| {
            (
                format!(
                    "{}:{}",
                    r["t"].as_str().unwrap(),
                    r["type"].as_str().unwrap_or("")
                ),
                r["count"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    let want = vec![
        ("unread:brand-new".to_string(), 3),
        ("unread:other-new".to_string(), 1),
        ("said:".to_string(), 0),
        ("unread:brand-new".to_string(), 1),
    ];
    assert_eq!(shape, want, "{rows:?}");
    assert_eq!(rows.len(), 7, "每个可计行仍占一条");
    assert!(
        recs[0]["text"].as_str().unwrap().contains('3'),
        "{}",
        recs[0]["text"]
    );

    let p = std::path::Path::new("/x/projects/-p/s.jsonl");
    // 第 i 行的末端记成 100 + i：并好的那一条末端跟到被并的最后一行（第 3 行）。
    let lines: Vec<(&[u8], u64, Option<u64>)> = page
        .lines()
        .enumerate()
        .map(|(i, l)| (l.as_bytes(), 0, Some(100 + i as u64)))
        .collect();
    let (out, next) = record_lines(&mut rd(&face), p, 10, lines);
    let seqs: Vec<u64> = out.iter().map(|l| l["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, [10, 14, 15, 16], "并好的那一条用第一行的行号");
    let ends: Vec<u64> = out.iter().map(|l| l["end"].as_u64().unwrap()).collect();
    assert_eq!(
        ends,
        [103, 104, 105, 106],
        "并好的那一条末端是被并的最后一行的"
    );
    assert_eq!(next, 17);
    assert_eq!(out[0]["record"]["count"], 3);

    let runs = run_rows(&mut rd(&face), 0, page.as_bytes());
    assert_eq!(runs.len(), 4);
    assert_eq!(runs[0]["record"]["count"], 3);
}

/// 从尾巴往前读一页（`history-run` 的 `tail` / `before`）：切在行头、整行交出；起点 `from` 拿去当下一次的 `before`，
/// 一页一页往前拼起来 == 整份；残尾（没写完的最后一行）不交；一行比一页还长 ⇒ 往前续读到它的行头，过 `line_cap` 就拒。
#[test]
fn read_back_pages_from_the_tail_cut_at_line_starts() {
    let home = std::env::temp_dir().join(format!("body-back-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let lines: Vec<String> = (0..40)
        .map(|i| format!("{{\"n\":{i},\"pad\":\"{}\"}}\n", "x".repeat(i % 7 * 5)))
        .collect();
    let whole: String = lines.concat();
    let p = dir.join("s.jsonl");
    std::fs::write(&p, format!("{whole}{{\"torn\":")).unwrap();
    let path = p.to_string_lossy().into_owned();
    let last = read_back(&home, &path, None, 120, 1 << 20).unwrap();
    assert_eq!(last.end, whole.len() as u64, "残尾不交");
    assert!(last.from > 0 && last.bytes.len() <= 120);
    assert!(whole[..last.from as usize].ends_with('\n'), "切在行头");
    let mut got = Vec::new();
    let mut before = None;
    loop {
        let pg = read_back(&home, &path, before, 120, 1 << 20).unwrap();
        got.insert(0, String::from_utf8(pg.bytes).unwrap());
        if pg.from == 0 {
            break;
        }
        before = Some(pg.from);
    }
    assert_eq!(got.concat(), whole, "往前一页页拼起来就是整份");
    // 一行比一页长：往前续读到它的行头。
    let long = format!("{{\"a\":1}}\n{{\"big\":\"{}\"}}\n", "y".repeat(500));
    std::fs::write(&p, &long).unwrap();
    let pg = read_back(&home, &path, None, 64, 1 << 20).unwrap();
    assert_eq!(pg.from, 8);
    assert_eq!(pg.end, long.len() as u64);
    assert!(!pg.more, "末行写完了 ⇒ 后面没有了");
    assert!(last.more, "有残尾 ⇒ end 之后还有字节");
    assert_eq!(
        read_back(&home, &path, None, 64, 100)
            .unwrap_err()
            .code
            .as_deref(),
        Some("oversized_line")
    );
    let _ = std::fs::remove_dir_all(&home);
}

// ───────────── 一步用时：出页时配对 ─────────────
// 夹具只造结构（调用 id、时刻、一句占位的输出），不采任何真会话正文。

/// 一条回复：一次工具调用 `id`，时刻 `ts`。
fn call_line(id: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"a-{id}","timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{id}","name":"Bash","input":{{"command":"true"}}}}]}}}}"#
    )
}

/// 一条结果：`id` 那次调用的结果，时刻 `ts`。
fn result_line(id: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"user","uuid":"u-{id}","timestamp":"{ts}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"x"}}]}}}}"#
    )
}

const T0: &str = "2026-01-01T00:00:00.000Z";
const T90: &str = "2026-01-01T00:01:30.000Z";

/// 成品里那一格结果（最后一条带 `record.results.<id>` 的）。
fn result_cell(rows: &[Value], id: &str) -> Value {
    rows.iter()
        .rev()
        .find_map(|l| l["record"]["results"].get(id).cloned())
        .unwrap_or_else(|| panic!("没有 {id} 的结果那一格: {rows:?}"))
}

/// 一份文件（在临时目录里），回它的路径与 `marker` 那一行的起点。
fn back_file(name: &str, body: &str, marker: &str) -> (std::path::PathBuf, u64) {
    let dir = std::env::temp_dir().join(format!("body-steppage-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(&p, body).unwrap();
    let at = body.find(marker).unwrap() as u64;
    (p, at)
}

/// 同一页里调用与结果隔 90 秒 ⇒ 结果那一格带写好的用时（线上只有 `text` 那一句）；三种成品一个样。
#[test]
fn a_result_on_the_same_page_as_its_call_carries_the_step_duration() {
    let page = format!("{}\n{}\n", call_line("c1", T0), result_line("c1", T90));
    let face = claude();
    let want = copy_core::short_duration(90_000);
    let (lines, _) = record_lines_of_page(
        &mut rd(&face),
        std::path::Path::new("/p/abc.jsonl"),
        0,
        0,
        page.as_bytes(),
    );
    let c = result_cell(&lines, "c1");
    assert_eq!(c["text"], want.as_str(), "{c}");
    assert!(c.get("timed").is_none(), "线上不带耗时那一形：{c}");
    let rows = rows_v(&mut rd(&face), 0, page.as_bytes());
    assert_eq!(result_cell(&rows, "c1")["text"], want.as_str());
    let runs = run_rows(&mut rd(&face), 0, page.as_bytes());
    assert_eq!(result_cell(&runs, "c1")["text"], want.as_str());
}

/// 调用在前一页、离这一页起点不到一页的额度 ⇒ 往回读那一段配上，照样写好用时。
#[test]
fn a_call_within_one_page_before_the_page_start_still_times_its_result() {
    let res = result_line("c2", T90);
    let body = format!(
        "{}\n{}\n{USER}\n{res}\n",
        call_line("other", T0),
        call_line("c2", T0),
    );
    let (file, at) = back_file("near", &body, &res);
    let face = claude();
    let page = &body.as_bytes()[at as usize..];
    let (lines, _) = record_lines_of_page(&mut rd(&face).back(&file, 4096), &file, 0, at, page);
    let c = result_cell(&lines, "c2");
    assert_eq!(c["text"], copy_core::short_duration(90_000).as_str(), "{c}");
    let rows = rows_v(&mut rd(&face).back(&file, 4096), at, page);
    assert_eq!(
        result_cell(&rows, "c2")["text"],
        copy_core::short_duration(90_000).as_str()
    );
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

/// 调用在一页额度以外 ⇒ 再往前不读：不写用时，照没有用时的那一形（线上也没有带耗时那一形）。
#[test]
fn a_call_beyond_one_page_before_the_page_start_leaves_the_result_untimed() {
    let res = result_line("c3", T90);
    let filler: String = (0..20).map(|_| format!("{USER}\n")).collect();
    let body = format!("{}\n{filler}{res}\n", call_line("c3", T0));
    let (file, at) = back_file("far", &body, &res);
    let face = claude();
    let page = &body.as_bytes()[at as usize..];
    // 额度够到填充那一段的中间，够不到调用那一行。
    let budget = filler.len() - USER.len() / 2;
    let (lines, _) = record_lines_of_page(&mut rd(&face).back(&file, budget), &file, 0, at, page);
    let c = result_cell(&lines, "c3");
    assert_eq!(c["text"], "", "{c}");
    assert!(c.get("timed").is_none(), "{c}");
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
