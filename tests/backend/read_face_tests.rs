//! 〔`C1` · 2026-09-24〕只读查询帧面宿主的判据。
//!
//! 夹具只造**结构**（目录名、行数、字节边界），行内容是占位的最小 JSON —— 不采任何真会话正文。

use super::*;
use std::path::{Path, PathBuf};

/// 本族的八条帧命令 —— **题面给的那八条**（`设计/15 §3.2` 那一串 ＋ `99 §4.19.2 ⑥`），
/// 写成帧面名。它是判据的**异源**那一侧：下面那条从 `inbound.rs` 源码里数「谁把活交给了
/// `read_face::answer`」，两边必须相等。
/// 〔SR1a · 09-24〕+2：`history-index` / `history-user-inputs`（题面「`--list-user-inputs` 与骨架
/// `--read-session-from-offset --index` 上帧面」那一句 —— 异源仍是题面，不是 `inbound.rs`）。
const FAMILY: &[&str] = &[
    "accounts-list",
    "accounts-sessions",
    "history-index",
    "history-user-inputs",
    // 〔SR1a × SE2〕会话内查找。
    "history-find",
    "history-projects",
    "history-read",
    // 〔U4b · 第四波〕记录还在不在（resume 一跳先问；异源是题面 `U4b` G1，不是 `inbound.rs`）。
    "history-record",
    "history-search",
    "history-sessions",
    "history-subagents",
    "history-tail",
];

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c1-read-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects")).unwrap();
    d
}

/// 一份会话：`n` 行最小 JSON（结构占位），外加可选的 torn 残尾。
fn session(home: &Path, proj: &str, sid: &str, n: usize, torn: bool) -> PathBuf {
    let dir = home.join("projects").join(proj);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{sid}.jsonl"));
    let mut body = String::new();
    for i in 0..n {
        body.push_str(&format!("{{\"i\":{i}}}\n"));
    }
    if torn {
        body.push_str("{\"torn\":");
    }
    std::fs::write(&p, body).unwrap();
    p
}

/// ★ 两向相等：`inbound::REGISTRY` 里把活交给本宿主的命令 == [`FAMILY`]。
///
/// 左边从 `inbound.rs` **生产段源码**切 `CommandSpec {` 块数出来（异源：不读本文件的任何常量）。
#[test]
fn the_registry_hands_exactly_the_eight_to_this_host() {
    let prod = crate::guard_support::production_code(include_str!("../../src/backend/inbound.rs"));
    let mut got: Vec<String> = prod
        .split("CommandSpec {")
        .skip(1)
        .filter(|blk| blk.contains("read_face::answer"))
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some(blk[at..].split('"').next()?.to_string())
        })
        .collect();
    got.sort();
    let mut want: Vec<String> = FAMILY.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        got, want,
        "交给 `read_face::answer` 的帧命令与题面那几条不相等"
    );
    // 那八条也都真在帧面的镜子里（`hello.commands` 从它出）。
    for n in FAMILY {
        assert!(
            crate::inbound::COMMANDS.contains(n),
            "`{n}` 不在 `inbound::COMMANDS` —— hello 不会宣告它，monitor 的 `accepts` 会拒"
        );
    }
}

/// ★ 本宿主对八条**每一条**都有自己的臂（不是落进兜底那句「本族不认识」）；兜底对别的名字成立（正控）。
#[test]
fn every_member_has_its_own_arm_and_strangers_do_not() {
    let home = scratch("arms");
    for n in FAMILY {
        if let Err((_, m)) = answer_at(&home, n, &serde_json::json!({})) {
            assert!(!m.contains("本族不认识"), "`{n}` 落进了兜底臂：{m}");
        }
    }
    match answer_at(&home, "history-nope", &serde_json::json!({})) {
        Err((c, m)) => assert!(c == "bad_args" && m.contains("本族不认识"), "{c}: {m}"),
        Ok(v) => panic!("陌生命令被答了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 分页读：拼起来**逐字节**等于原文件（含 torn 残尾）；非末页都切在行尾；续点单调。
#[test]
fn paged_read_reassembles_the_file_byte_for_byte() {
    let home = scratch("pages");
    let p = session(&home, "-p", "s1", 50, true);
    let want = std::fs::read(&p).unwrap();
    let path = p.to_string_lossy().into_owned();
    let mut got: Vec<u8> = Vec::new();
    let mut off = 0u64;
    let mut pages = 0;
    loop {
        let pg = crate::observe::history_query::read_page(&home, &path, off, None, 37, 1 << 20)
            .expect("读页");
        assert_eq!(
            pg.next,
            off + pg.bytes.len() as u64,
            "续点必须恰好推进这一页的字节数"
        );
        if !pg.eof {
            assert_eq!(pg.bytes.last(), Some(&b'\n'), "非末页必须切在行尾");
        }
        got.extend_from_slice(&pg.bytes);
        off = pg.next;
        pages += 1;
        if pg.eof {
            break;
        }
        assert!(pages < 10_000, "分页不收敛");
    }
    assert!(pages > 1, "页太大，本条没测到切页");
    assert_eq!(got, want);
    // `until` 收口：[a, b) 恰好是那一段。
    let (a, b) = (8u64, 60u64);
    let pg = crate::observe::history_query::read_page(&home, &path, a, Some(b), 1 << 20, 1 << 20)
        .unwrap();
    assert!(pg.eof);
    assert_eq!(pg.bytes, want[a as usize..b as usize].to_vec());
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 单行比一页还长：续读到行尾；超过 `line_cap` ⇒ `oversized_line`，**不交半行**。
#[test]
fn an_oversized_line_is_refused_not_cut() {
    let home = scratch("big");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s2.jsonl");
    let long = format!("{{\"x\":\"{}\"}}\n{{\"i\":1}}\n", "a".repeat(5000));
    std::fs::write(&p, &long).unwrap();
    let path = p.to_string_lossy().into_owned();
    let ok = crate::observe::history_query::read_page(&home, &path, 0, None, 100, 1 << 20).unwrap();
    assert_eq!(ok.bytes.last(), Some(&b'\n'));
    assert_eq!(
        ok.bytes.len(),
        long.find('\n').unwrap() + 1,
        "应恰好是那一整行"
    );
    match crate::observe::history_query::read_page(&home, &path, 0, None, 100, 1000) {
        Err((c, _)) => assert_eq!(c, "oversized_line"),
        Ok(pg) => panic!("超长行被交出去了（{} 字节）", pg.bytes.len()),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 围栏照旧：`projects/` 之外的路径被拒（帧面没有绕开 `fence_under_projects`）。
#[test]
fn the_frame_read_keeps_the_projects_fence() {
    let home = scratch("fence");
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    let args = serde_json::json!({"path": outside.to_string_lossy()});
    match answer_at(&home, "history-read", &args) {
        Err((c, _)) => assert_eq!(c, "refused"),
        Ok(v) => panic!("围栏外的文件被读了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 按行那几条：`{"lines": [...]}`，内容与夹具**结构**对得上（异源：期望值来自夹具，不来自被测函数）。
#[test]
fn line_shaped_answers_carry_the_rows() {
    let home = scratch("rows");
    session(&home, "-alpha", "a1", 3, false);
    session(&home, "-alpha", "a2", 1, false);
    session(&home, "-beta", "b1", 2, false);
    let v = answer_at(&home, "history-projects", &serde_json::json!({})).unwrap();
    let mut dirs: Vec<(String, u64)> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let row: serde_json::Value = serde_json::from_str(l.as_str().unwrap()).unwrap();
            (
                row["dirName"].as_str().unwrap().to_string(),
                row["sessionCount"].as_u64().unwrap(),
            )
        })
        .collect();
    dirs.sort();
    assert_eq!(
        dirs,
        vec![("-alpha".to_string(), 2), ("-beta".to_string(), 1)]
    );
    let v = answer_at(
        &home,
        "history-sessions",
        &serde_json::json!({"project_dir": "-alpha"}),
    )
    .unwrap();
    assert_eq!(v["lines"].as_array().unwrap().len(), 2);
    // 缺参 ⇒ bad_args（不是 panic、不是空清单冒充成功）。
    assert_eq!(
        answer_at(&home, "history-sessions", &serde_json::json!({}))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 尾段那张图：按它的两段区间读回来 ＝ CLI `--read-session-tail` 印出的那两段（meta 之后的字节）。
///
/// 期望值由测试自己从夹具算（最后 `n` 个可计行的起点），不借被测函数。
#[test]
fn the_tail_plan_matches_an_independent_count() {
    let home = scratch("tail");
    let p = session(&home, "-p", "t1", 20, true);
    let bytes = std::fs::read(&p).unwrap();
    let path = p.to_string_lossy().into_owned();
    let v = answer_at(
        &home,
        "history-tail",
        &serde_json::json!({"path": path, "n": 5}),
    )
    .unwrap();
    // 独立算：完整行的起点
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').unwrap() + 1;
    let mut starts = vec![0usize];
    for (i, b) in bytes[..complete_end].iter().enumerate() {
        if *b == b'\n' && i + 1 < complete_end {
            starts.push(i + 1);
        }
    }
    assert_eq!(v["total"].as_u64(), Some(20));
    assert_eq!(v["tail_from"].as_u64(), Some(15));
    assert_eq!(v["split_at"].as_u64(), Some(starts[15] as u64));
    assert_eq!(v["end"].as_u64(), Some(complete_end as u64));
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 输出超上限 ⇒ `too_large`，不交截断的清单（正控：刚好不超时照常答）。
#[test]
fn an_oversized_listing_is_refused_not_truncated() {
    let big = vec![b'x'; LINES_CAP_BYTES + 1];
    match lines(|out| {
        use std::io::Write;
        out.write_all(&big).map_err(|e| ("failed", e.to_string()))
    }) {
        Err((c, _)) => assert_eq!(c, "too_large"),
        Ok(_) => panic!("超上限的输出被当成完整清单交出去了"),
    }
    let ok = lines(|out| {
        use std::io::Write;
        out.write_all(b"a\n\n b \n")
            .map_err(|e| ("failed", e.to_string()))
    })
    .unwrap();
    assert_eq!(ok, serde_json::json!({"lines": ["a", "b"]}));
}

/// ★ F2（〔SR1a〕）：骨架索引与大纲清单两条帧命令的 `lines`，与**夹具算出来的**三段逐行相等
/// （异源：期望的偏移 / 行长 / uuid 从夹具字节自己数，不借被测函数）。
#[test]
fn index_and_user_input_answers_carry_the_rows_the_fixture_predicts() {
    let home = scratch("sr1a-index");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：一条用户输入（uuid 命名 `in-*`）、一条 meta 用户行、一条助手行 —— 不采任何真会话正文。
    let rows = [
        r#"{"type":"user","uuid":"in-1","timestamp":"t1","message":{"content":"ask"}}"#,
        r#"{"type":"user","uuid":"out-meta","isMeta":true,"message":{"content":"x"}}"#,
        r#"{"type":"assistant","uuid":"out-asst","message":{"content":[]}}"#,
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();

    let v = answer_at(
        &home,
        "history-index",
        &serde_json::json!({"path": path, "offset": 0}),
    )
    .unwrap();
    let lines: Vec<serde_json::Value> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(lines.first().unwrap()["kind"], "session_index");
    let mut o = 0u64;
    let want: Vec<(u64, u64)> = rows
        .iter()
        .map(|r| {
            let n = r.len() as u64 + 1;
            let at = o;
            o += n;
            (at, n)
        })
        .collect();
    let got: Vec<(u64, u64)> = lines[1..lines.len() - 1]
        .iter()
        .map(|r| (r["o"].as_u64().unwrap(), r["n"].as_u64().unwrap()))
        .collect();
    assert_eq!(got, want, "索引行的偏移 / 行长与夹具对不上");
    let tail = lines.last().unwrap();
    assert_eq!(
        (
            tail["kind"].as_str(),
            tail["count"].as_u64(),
            tail["end"].as_u64()
        ),
        (Some("session_index_end"), Some(3), Some(body.len() as u64))
    );

    let v = answer_at(
        &home,
        "history-user-inputs",
        &serde_json::json!({"path": path}),
    )
    .unwrap();
    let lines: Vec<serde_json::Value> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(lines.first().unwrap()["kind"], "user_inputs");
    let uuids: Vec<&str> = lines[1..lines.len() - 1]
        .iter()
        .map(|r| r["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(uuids, ["in-1"], "清单里的 uuid 与夹具里的 `in-*` 不相等");
    assert_eq!(
        lines.last().unwrap()["end"].as_u64(),
        Some(body.len() as u64)
    );
    // 起点越过文件尾 ⇒ failed（不回一份空清单冒充「没有新的」）。
    let e = answer_at(
        &home,
        "history-user-inputs",
        &serde_json::json!({"path": path, "from": body.len() as u64 + 1}),
    )
    .unwrap_err();
    assert_eq!(e.0, "failed");
    // 围栏：同一套（围栏外 ⇒ 拒）。
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    for cmd in ["history-index", "history-user-inputs"] {
        assert!(
            answer_at(
                &home,
                cmd,
                &serde_json::json!({"path": outside.to_string_lossy()})
            )
            .is_err(),
            "`{cmd}` 读了围栏外的文件"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 〔SR1a × SE2〕`history-find` 的 `lines`：命中集合 == 夹具里 `hit-*` 那几条（异源：期望取自夹具的 uuid 命名），
/// 头尾两段在；围栏同一套。
#[test]
fn the_find_answer_hits_exactly_the_fixture_hits() {
    let home = scratch("sr1a-find");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：只有 `hit-*` 那几条的正文里有那个词 —— 不采任何真会话正文。
    let rows = [
        r#"{"type":"user","uuid":"hit-1","message":{"content":"zqxneedle one"}}"#,
        r#"{"type":"user","uuid":"miss-1","message":{"content":"nothing here"}}"#,
        r#"{"type":"user","uuid":"hit-2","message":{"content":"two zqxneedle"}}"#,
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    let v = answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": path, "query": "zqxneedle"}),
    )
    .unwrap();
    let lines: Vec<serde_json::Value> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(lines.first().unwrap()["kind"], "session_find");
    assert_eq!(lines.last().unwrap()["kind"], "session_find_end");
    let hits: Vec<&str> = lines[1..lines.len() - 1]
        .iter()
        .map(|r| r["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(
        hits,
        ["hit-1", "hit-2"],
        "命中集合与夹具里的 `hit-*` 不相等"
    );
    assert_eq!(
        answer_at(&home, "history-find", &serde_json::json!({"path": path}))
            .unwrap_err()
            .0,
        "bad_args",
        "缺 query 该是 bad_args"
    );
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    assert!(answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": outside.to_string_lossy(), "query": "x"})
    )
    .is_err());
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔U4b · 第四波 · B4〕`history-record`：在 ⇒ `present:true`；不在 ⇒ `present:false`（一个答案，不是错误）；
/// 坏 sid ⇒ `bad_args`；`root` == 这棵夹具树的 `projects`。
///
/// 夹具只造结构（目录名 ＋ 占位行），不采真会话正文。符号链接那一格由 `branch_core::find_session_file`
/// 自己的两侧判据管（本条不重验它）。
#[test]
fn history_record_answers_present_absent_and_refuses_a_bad_sid() {
    let home = scratch("record");
    session(&home, "-p", "aaaa-1111", 1, false);
    let ask = |sid: &str| answer_at(&home, "history-record", &serde_json::json!({ "sid": sid }));
    let root = home.join("projects").to_string_lossy().into_owned();
    assert_eq!(
        ask("aaaa-1111").unwrap(),
        serde_json::json!({ "present": true, "root": root })
    );
    assert_eq!(
        ask("bbbb-2222").unwrap(),
        serde_json::json!({ "present": false, "root": root })
    );
    for bad in ["", "../x", "a/b", "a b"] {
        match ask(bad) {
            Err(("bad_args", _)) => {}
            other => panic!("坏 sid {bad:?} 应当 bad_args，实得 {other:?}"),
        }
    }
    // 缺 `sid` 也是 bad_args。
    assert!(matches!(
        answer_at(&home, "history-record", &serde_json::json!({})),
        Err(("bad_args", _))
    ));
    std::fs::remove_dir_all(&home).ok();
}
