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

/// 夹具里只住**正文**那几格的标记（照那份声明投影 ⇒ 一个都不许剩）。
/// 每个标记只有一个出处：思考 · 说的话 · 工具入参里主参数之外的那一格 · 工具结果正文 · 逐段改动里的一行。
const GONE: &[&str] = &[
    "ZQBODY-think",
    "ZQBODY-say",
    "ZQBODY-input",
    "ZQBODY-result",
    "ZQBODY-patch",
];

/// **折起那一行自己要用的**那几格里的标记（照那份声明投影 ⇒ 一个都不许少）。
/// 它们在原文里也住正文块，但后端判好的成品（`who.text`）里**另有一份** ⇒ 剥正文是**去重**，不是把人说的话弄丢。
/// 这一半不立，「不许有正文」那一半把投影整个弄坏也能恒绿。
/// 结果的首行预览（`results.*.preview`）是核心另出的一格：剥了结果正文它照样在（折起那一行写它）。
const KEPT: &[&str] = &["ZQKEEP-user", "ZQKEEP-queued", "ZQKEEP-preview"];

/// 折起那一行要用的格（照那份声明投影 ⇒ 逐个还在、值不变；它们正是界面画那一行读的那几格）。
const FOLDED_CELLS: &[(&str, &str)] = &[
    ("u1", "who"),
    ("a1", "steps"),
    ("a1", "model"),
    ("u3", "results"),
];

const FOLD_PAGE: &[&str] = &[
    r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","cwd":"/w","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-user"}]}}"#,
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","model":"m","content":[{"type":"thinking","thinking":"ZQBODY-think"},{"type":"text","text":"ZQBODY-say"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt","zq_extra":"ZQBODY-input"}}],"usage":{"input_tokens":11,"output_tokens":22}}}"#,
    r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQKEEP-preview\nZQBODY-result"}]},"toolUseResult":{"type":"text","file":{"numLines":7}}}"#,
    r#"{"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"2026-01-02T03:04:07.500Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"filePath":"/w/f.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+ZQBODY-patch"]}]}}"#,
    r#"{"type":"attachment","uuid":"q1","parentUuid":"u3","timestamp":"2026-01-02T03:04:08.000Z","attachment":{"type":"queued_command","prompt":"ZQKEEP-queued","commandMode":"prompt","origin":{"kind":"human"}}}"#,
];

/// 「折起那一行」那份声明（出口交的 `omit`）：正文住的那几格。
fn folded_view() -> crate::faces::project::View {
    crate::faces::project::parse_view(&serde_json::json!({"omit": {"record": [
        "blocks", "results.*.patch",
    ]}}))
    .unwrap()
    .unwrap()
}

/// ★ **两头都断**：照「折起那一行」那份声明投影 ⇒ 正文那几格一个不剩、折起那一行要用的一格不少；不投影 ⇒ 正文**必须**在。
///
/// 两头各自都不够：只断「投影 ⇒ 不许有正文」的话，把投影整个弄坏（一条都不出成品）也能让它绿；
/// 只断「不投影 ⇒ 有正文」的话，声明根本没接上也能绿。⇒ 本条逐标记两向都判，再加上**条数 · 行号 · 身份一格不变**。
/// 记录成品真出的格（本页）与声明里写的格名（格目录那一套）对不上 ⇒ 投影落空 ⇒ 红。
#[test]
fn the_folded_view_drops_every_body_cell_and_keeps_every_folded_cell() {
    let page = FOLD_PAGE.join("\n") + "\n";
    let at = std::path::Path::new("/p/zq.jsonl");
    let face = claude();
    let view = folded_view();
    let shape = |fold: bool| {
        let (mut lines, next) = record_lines_of_page(&mut rd(&face), at, 5, 0, page.as_bytes());
        let mut rows = rows_v(&mut rd(&face), 0, page.as_bytes());
        if fold {
            for l in lines.iter_mut().chain(rows.iter_mut()) {
                if let Some(r) = l.get_mut("record") {
                    crate::faces::project::project("record", r, &view);
                }
            }
        }
        let text = serde_json::to_string(&lines).unwrap();
        (lines, next, rows, text)
    };
    let (full_lines, full_next, full_rows, full_text) = shape(false);
    let (fold_lines, fold_next, fold_rows, fold_text) = shape(true);

    for m in GONE.iter().chain(KEPT) {
        assert!(
            full_text.contains(m),
            "不投影时 `{m}` 不在成品里 —— 夹具或投影坏了，下面那一半会恒绿"
        );
    }
    for m in GONE {
        assert!(
            !fold_text.contains(m),
            "投影过，正文标记 `{m}` 还在成品里：{fold_text}"
        );
    }
    for m in KEPT {
        assert!(
            fold_text.contains(m),
            "投影把折起那一行要显示的 `{m}` 也剥掉了"
        );
    }
    let cell = |lines: &[serde_json::Value], id: &str, key: &str| {
        lines
            .iter()
            .find(|l| l["record"]["id"] == id)
            .unwrap_or_else(|| panic!("夹具里没有 id={id} 那一条"))["record"][key]
            .clone()
    };
    for (id, key) in FOLDED_CELLS {
        let (fold, full) = (cell(&fold_lines, id, key), cell(&full_lines, id, key));
        assert!(
            !full.is_null(),
            "夹具里 `{key}`（id={id}）没有 —— 下面那一条会恒绿"
        );
        if *key == "results" {
            // 结果一句留着，只少了逐段改动。
            assert_eq!(fold["t2"]["added"], full["t2"]["added"]);
            assert!(fold["t2"].get("patch").is_none() && full["t2"].get("patch").is_some());
        } else {
            assert_eq!(fold, full, "`{key}`（id={id}）在折起那一形里变了");
        }
    }
    let a1 = fold_lines
        .iter()
        .find(|l| l["record"]["id"] == "a1")
        .unwrap();
    assert!(
        a1["record"].get("blocks").is_none(),
        "`blocks` 该是**删掉**而不是给空值（给空值等于说这一条没有正文，那是假话）：{a1}"
    );

    assert_eq!(fold_next, full_next, "剥正文不许动行号");
    assert_eq!(fold_lines.len(), full_lines.len(), "剥正文不许少出一条");
    assert_eq!(fold_lines.len(), FOLD_PAGE.len(), "夹具每一行都该出成品");
    let ident = |lines: &[serde_json::Value]| -> Vec<serde_json::Value> {
        lines
            .iter()
            .map(|l| {
                serde_json::json!([
                    l["seq"].clone(),
                    l["session_id"].clone(),
                    l["path"].clone(),
                    l["cwd"].clone(),
                    l["record"]["id"].clone(),
                    l["record"]["t"].clone(),
                    l["record"]["timeText"].clone(),
                ])
            })
            .collect()
    };
    assert_eq!(
        ident(&fold_lines),
        ident(&full_lines),
        "行标识 / 时刻字格变了"
    );
    let ends = |rows: &[serde_json::Value]| -> Vec<serde_json::Value> {
        rows.iter()
            .map(|r| serde_json::json!([r["end"].clone(), r["hash"].clone(), r["cwd"].clone()]))
            .collect()
    };
    assert_eq!(
        ends(&fold_rows),
        ends(&full_rows),
        "行摘要的 `end` / `hash` / `cwd` 变了（续传要靠它们核「还是不是那一行」）"
    );
    assert!(
        !fold_text.contains("\"blocks\""),
        "折起那一形里还剩 `blocks`：{fold_text}"
    );
    assert!(
        full_text.contains("\"blocks\""),
        "不投影时连 `blocks` 都没有 —— 上面那一条会恒绿"
    );
}

/// ★ 桌面交的那份声明（`src/shared/views/folded-record.json`，界面 `record-reads.ts` 与壳的旁路快照读同一份）：
/// 核心认得它的每一格（`parse_view` 不拒）；照它投影 ⇒ 工具入参 · 结果正文 · 逐段改动没了，思考 · 说的话 · 预览 · 主参数那一句还在。
#[test]
fn the_desktop_folded_view_in_shared_parses_and_drops_only_the_tool_bodies() {
    let decl: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../src/shared/views/folded-record.json"
    ))
    .expect("那份声明是 JSON");
    let view = crate::faces::project::parse_view(&decl)
        .unwrap_or_else(|e| panic!("核心不认那份声明：{e}"))
        .expect("声明不是空的");
    // 再加一步改文件：改前改后进了 `steps[id].body`，那一格也是正文。
    let edit = r#"{"type":"assistant","uuid":"a2","parentUuid":"u3","timestamp":"2026-01-02T03:04:07.700Z","message":{"role":"assistant","model":"m","content":[{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"/w/f.txt","old_string":"a","new_string":"ZQBODY-change"}}],"usage":{"input_tokens":1,"output_tokens":1}}}"#;
    let page = FOLD_PAGE.join("\n") + "\n" + edit + "\n";
    let at = std::path::Path::new("/p/zq.jsonl");
    let face = claude();
    let (mut lines, _) = record_lines_of_page(&mut rd(&face), at, 5, 0, page.as_bytes());
    let full = serde_json::to_string(&lines).unwrap();
    for l in &mut lines {
        if let Some(r) = l.get_mut("record") {
            crate::faces::project::project("record", r, &view);
        }
    }
    let fold = serde_json::to_string(&lines).unwrap();
    for m in [
        "ZQBODY-input",
        "ZQBODY-result",
        "ZQBODY-patch",
        "ZQBODY-change",
    ] {
        assert!(full.contains(m), "不投影时 `{m}` 不在 —— 下面那一条会恒绿");
        assert!(!fold.contains(m), "照桌面那份声明投影，`{m}` 还在：{fold}");
    }
    for m in [
        "ZQBODY-think",
        "ZQBODY-say",
        "ZQKEEP-user",
        "ZQKEEP-preview",
        "/w/f.txt",
    ] {
        assert!(fold.contains(m), "桌面那份声明把 `{m}` 也剥了");
    }
}

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
        read_back(&home, &path, None, 64, 100).unwrap_err().code.as_deref(),
        Some("oversized_line")
    );
    let _ = std::fs::remove_dir_all(&home);
}
