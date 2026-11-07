//! 会话内查找判据里经帧面宿主问 `history-find` 的那几条（`faces::history_face::find_at`：常驻索引那一臂 · 不归它管就现扫）：
//! 观测面是独立的包（`backend-observe`）、够不到帧面 ⇒ 挂在主 crate（经 `cross/observe.rs`）。夹具是**合成的结构**，不含任何真会话正文。

use crate::agents::UserTextExt as _;
use crate::observe::search_query::test_support::{
    bytes_of, find, fixture, resident_reads, uuids, Q,
};
use crate::observe::search_query::{scan_session_find, FindPage, FIND_DEFAULT_LIMIT};

/// 照登记表那一层问一次 `history-find`（入参经 typed 那一处解，回包过一遍真序列化器）。
fn find_wire(
    home: &std::path::Path,
    cmd: &str,
    args: &serde_json::Value,
    tz: &crate::common::time::Tz,
) -> Result<serde_json::Value, crate::common::said::Said> {
    assert_eq!(cmd, "history-find");
    let req = crate::wire::typed::take(args)?;
    crate::faces::history_face::find_at(home, req, tz)
        .map(|r| serde_json::to_value(r).unwrap())
        .map_err(crate::wire::typed::Codes::into_said)
}

/// ★ 「会话内查找每次从头扫一遍文件」⇒ `history-find` 走常驻索引：
/// 应答 == 现扫（`scan_session_find`，两态 `include_tools`；torn 残尾与没 uuid 的行照样不列），追加之后那一问只读尾巴。
#[test]
fn gap1_history_find_rides_the_resident_index_and_equals_the_plain_scan() {
    let home = std::env::temp_dir().join(format!("gap1-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let rec = |uuid: &str, text: &str| {
        serde_json::json!({"type":"user","uuid":uuid,"message":{"content":[{"type":"text","text":text},
            {"type":"tool_result","tool_use_id":"t","content":format!("tool {text}")}]}})
        .to_string()
    };
    let mut body = [
        rec("a", "zqx one"),
        rec("", "zqx no uuid"),
        rec("b", "plain"),
    ]
    .join("\n");
    body.push('\n');
    body.push_str(&rec("torn", "zqx torn")); // 没换行 ⇒ 不列
    std::fs::write(&path, &body).unwrap();
    let p = path.display().to_string();
    let scan = |tools: bool| {
        let r = crate::observe::query::locate::open_session_at(&home, &p, 0).unwrap();
        let mut hits = Vec::new();
        let (_, total) = crate::observe::search_query::scan_session_find(
            crate::agents::claudecode::AGENT_KIND,
            r,
            "zqx",
            tools,
            None,
            crate::observe::search_query::FindPage {
                skip: 0,
                limit: 500,
            },
            |h| {
                hits.push(crate::observe::search_query::with_hit_text(
                    h,
                    &Default::default(),
                ));
                Ok(())
            },
        )
        .unwrap();
        serde_json::json!({ "total": total, "hits": hits })
    };
    // 两臂比的是命中与全量（结果头由这两样在出口那一处写，判据住 `history_find_head_tests`）。
    let ask = |tools: bool| {
        let v = find_wire(
            &home,
            "history-find",
            &serde_json::json!({"path": p, "query": "zqx", "include_tools": tools, "limit": 500}),
            &Default::default(),
        )
        .unwrap();
        serde_json::json!({ "total": v["total"], "hits": v["hits"] })
    };
    assert_eq!(ask(false), scan(false));
    assert_eq!(ask(true), scan(true));
    // 残尾补完 ＋ 再追加一行 ⇒ 这一问只追加读（不整份重读），应答仍 == 现扫。
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    use std::io::Write as _;
    writeln!(f, "\n{}", rec("c", "zqx later")).unwrap();
    drop(f);
    assert_eq!(ask(true), scan(true));
    assert_eq!(
        resident_reads(&home),
        Some((0, 1, 0)),
        "(整份, 追加, 没读)：该只追加读这一份"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 每条命中带「第几轮」（这条之前含它你说过几句，口径同大纲）与那条记录的时刻；`skip` 续下一页、`total` 照报全量。
/// 常驻索引那一臂与现扫那一臂逐字相等（期望的轮号取自夹具 uuid 的命名 `t{轮}-*`，不取自判定函数）。
#[test]
fn find_hits_carry_turn_and_time_and_page_by_skip_on_both_arms() {
    let home = std::env::temp_dir().join(format!("find-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let user = |uuid: &str, text: &str, ts: &str| {
        serde_json::json!({"type":"user","uuid":uuid,"timestamp":ts,"message":{"content":text}})
            .to_string()
    };
    let asst = |uuid: &str, text: &str, ts: &str| {
        serde_json::json!({"type":"assistant","uuid":uuid,"timestamp":ts,"message":{"content":[{"type":"text","text":text}]}})
            .to_string()
    };
    // 结构占位：只有带 zqx 的几条会中；第 2 轮你那句不含 zqx（它也要算一轮）。
    let body = [
        asst("t0-a", "zqx before any", "2026-01-01T00:00:00.000Z"),
        user("t1-u", "zqx first", "2026-01-01T00:01:00.000Z"),
        asst("t1-a", "zqx reply", "2026-01-01T00:02:00.000Z"),
        user("t2-u", "plain second", "2026-01-01T00:03:00.000Z"),
        asst("t2-a", "zqx again", "2026-01-01T00:04:00.000Z"),
        user("t3-u", "zqx third", "bad-time"),
    ]
    .join("\n")
        + "\n";
    std::fs::write(&path, &body).unwrap();
    let p = path.display().to_string();
    let scan = |skip: usize, limit: usize| {
        let r = crate::observe::query::locate::open_session_at(&home, &p, 0).unwrap();
        let mut hits = Vec::new();
        let page = crate::observe::search_query::FindPage { skip, limit };
        let (_, total) = crate::observe::search_query::scan_session_find(
            crate::agents::claudecode::AGENT_KIND,
            r,
            "zqx",
            false,
            None,
            page,
            |h| {
                hits.push(crate::observe::search_query::with_hit_text(
                    h,
                    &Default::default(),
                ));
                Ok(())
            },
        )
        .unwrap();
        serde_json::json!({ "total": total, "hits": hits })
    };
    let ask = |skip: usize, limit: usize| {
        let v = find_wire(
            &home,
            "history-find",
            &serde_json::json!({"path": p, "query": "zqx", "skip": skip, "limit": limit}),
            &Default::default(),
        )
        .unwrap();
        serde_json::json!({ "total": v["total"], "hits": v["hits"] })
    };
    let all = ask(0, 100);
    assert_eq!(all, scan(0, 100), "索引那一臂 == 现扫");
    let turns: Vec<(String, u64)> = all["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            (
                h["uuid"].as_str().unwrap().to_string(),
                h["turn"].as_u64().unwrap(),
            )
        })
        .collect();
    let want: Vec<(String, u64)> = ["t0-a", "t1-u", "t1-a", "t2-a", "t3-u"]
        .iter()
        .map(|u| (u.to_string(), u[1..2].parse().unwrap()))
        .collect();
    assert_eq!(turns, want);
    assert!(
        all["hits"][1].get("tsMs").is_none(),
        "时刻只给后端写字，不上线"
    );
    assert!(!all["hits"][1]["tsText"].as_str().unwrap().is_empty());
    assert_eq!(
        all["hits"][4]["tsText"].as_str(),
        Some(""),
        "读不出的时刻 ⇒ 空串"
    );
    let page = ask(2, 2);
    assert_eq!(page, scan(2, 2));
    assert_eq!(page["total"].as_u64(), Some(5), "续页照报全量");
    let uuids: Vec<&str> = page["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(uuids, ["t1-a", "t2-a"]);
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 「只搜用户」：`scope: "user"` 的命中 ＝ 流里画成「人说的」那几条里找得到这个词的（同一份夹具两边各算一遍）。
/// 「流里画成什么」取自流的成品本身（`parse_line` → `record_of` 出的 `Said { who }`，`who.speech()` 有话 ⇒ 人说的：
/// 用户气泡 · 斜杠命令 · `!` 命令），不取自查找那一侧的判定函数。系统注入 · 工具结果 · 斜杠命令展开（`isMeta`）·
/// agent 回报 · 后台通知 · 派给子 agent 的活 · 那一家说的都不算；`include_tools` 在用户范围里当假（用户说的话没有工具输出）。
/// 常驻索引那一臂（帧面 `history-find`）与现扫那一臂逐字相等。
#[test]
fn user_scope_hits_are_what_the_stream_draws_as_said_by_the_person() {
    let lines: Vec<String> = vec![
        r#"{"type":"user","uuid":"u-plain","timestamp":"2026-10-01T00:00:00Z","message":{"role":"user","content":"find the zqx here"}}"#.into(),
        r#"{"type":"assistant","uuid":"a-says","timestamp":"2026-10-01T00:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"repeat zqx back"}]}}"#.into(),
        r#"{"type":"user","uuid":"u-slash","timestamp":"2026-10-01T00:00:02Z","message":{"role":"user","content":"<command-name>/review</command-name>\n<command-message>review</command-message>\n<command-args>zqx please</command-args>"}}"#.into(),
        r#"{"type":"user","uuid":"u-expand","isMeta":true,"timestamp":"2026-10-01T00:00:03Z","message":{"role":"user","content":[{"type":"text","text":"expanded prompt about zqx"}]}}"#.into(),
        r#"{"type":"user","uuid":"u-bash","timestamp":"2026-10-01T00:00:04Z","message":{"role":"user","content":"<bash-input>grep zqx</bash-input>"}}"#.into(),
        r#"{"type":"user","uuid":"u-bash-out","timestamp":"2026-10-01T00:00:05Z","message":{"role":"user","content":"<bash-stdout>zqx found</bash-stdout><bash-stderr></bash-stderr>"}}"#.into(),
        r#"{"type":"user","uuid":"u-injected","timestamp":"2026-10-01T00:00:06Z","message":{"role":"user","content":"<system-reminder>zqx</system-reminder>"}}"#.into(),
        r#"{"type":"user","uuid":"u-tool","timestamp":"2026-10-01T00:00:07Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"tool says zqx"}]}}"#.into(),
        r#"{"type":"user","uuid":"u-report","isMeta":true,"origin":{"kind":"peer","from":"a1"},"timestamp":"2026-10-01T00:00:08Z","message":{"role":"user","content":"<agent-message from=\"a1\">zqx done</agent-message>"}}"#.into(),
        r#"{"type":"user","uuid":"u-notice","origin":{"kind":"task-notification"},"timestamp":"2026-10-01T00:00:09Z","message":{"role":"user","content":"<task-notification><summary>zqx</summary></task-notification>"}}"#.into(),
        r#"{"type":"user","uuid":"u-mixed","timestamp":"2026-10-01T00:00:10Z","message":{"role":"user","content":[{"type":"text","text":"ZQX in caps"},{"type":"tool_result","tool_use_id":"t2","content":"zqx again"}]}}"#.into(),
        r#"{"type":"user","uuid":"u-other","timestamp":"2026-10-01T00:00:11Z","message":{"role":"user","content":"nothing to find"}}"#.into(),
    ];
    // 期望：流的成品里「人说的」且话里有这个词的（文件序）。
    let q = "zqx";
    let want: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            let rec = crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(l, 0))
                .ok()??;
            let r = crate::agents::claudecode::record_of::record_of(rec, 0)?;
            match r.body {
                crate::agents::record::Body::Said { who, .. } => who
                    .speech()
                    .filter(|s| s.to_lowercase().contains(q))
                    .map(|_| r.id),
                _ => None,
            }
        })
        .collect();
    assert_eq!(
        want,
        ["u-plain", "u-slash", "u-bash", "u-mixed"],
        "夹具自检：流里画成人说的、且有这个词的恰是这四条（不然这一格比的是空集）"
    );
    let data = bytes_of(&lines);
    // 现扫那一臂：带不带工具都一样（用户范围里工具当假）。
    for tools in [false, true] {
        let mut got = Vec::new();
        let (_, total) = scan_session_find(
            crate::agents::claudecode::AGENT_KIND,
            &data[..],
            q,
            tools,
            Some("user"),
            FindPage {
                skip: 0,
                limit: 500,
            },
            |h| {
                got.push(h.uuid.clone());
                assert_eq!(h.kind, "user", "用户范围里每条都是 user 命中");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(got, want, "现扫 · include_tools={tools}");
        assert_eq!(total as usize, want.len());
    }
    // 范围「全部」照旧（对照：那一家复述的 · agent 回报也在）。
    let all = uuids(&find(&data, q, false, 500));
    assert!(
        all.contains(&"a-says".to_string()) && all.contains(&"u-report".to_string()),
        "全部：{all:?}"
    );
    // 帧面（常驻索引那一臂）：同一份答案；认不出的范围 ⇒ bad_args。
    let home = std::env::temp_dir().join(format!("ui3-find-scope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    std::fs::write(&path, &data).unwrap();
    let p = path.display().to_string();
    let ask = |scope: &str, tools: bool| {
        find_wire(
            &home,
            "history-find",
            &serde_json::json!({"path": p, "query": q, "include_tools": tools, "scope": scope, "limit": 500}),
            &Default::default(),
        )
    };
    for tools in [false, true] {
        let v = ask("user", tools).unwrap();
        let got: Vec<String> = v["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["uuid"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(got, want, "帧面 · include_tools={tools}");
        assert_eq!(v["total"].as_u64(), Some(want.len() as u64));
    }
    assert_eq!(
        ask("bogus", false).unwrap_err().code.as_deref(),
        Some("bad_args")
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 帧面那一臂（常驻索引）的每条命中带的行号 == 现扫那一臂的（现扫那一臂的行号 == 骨架索引里的下标，包里那一条判）。
/// 夹具里有坏行 · 空行 · 带 BOM 的行，行号与物理行号不同。
#[test]
fn hits_from_the_frame_carry_the_same_index_row_as_the_plain_scan() {
    let home = std::env::temp_dir().join(format!("find-line-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let data = bytes_of(&fixture());
    std::fs::write(&path, &data).unwrap();
    let pairs = |hits: &[serde_json::Value]| -> Vec<(String, u64)> {
        hits.iter()
            .map(|h| {
                (
                    h["uuid"].as_str().unwrap().to_string(),
                    h["line"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let scanned = find(&data, Q, true, FIND_DEFAULT_LIMIT);
    let wire = find_wire(
        &home,
        "history-find",
        &serde_json::json!({"path": path.display().to_string(), "query": Q, "include_tools": true, "limit": 500}),
        &Default::default(),
    )
    .unwrap();
    let want = pairs(&scanned[1..scanned.len() - 1]);
    assert!(want.len() >= 3, "夹具该有几条命中");
    assert_eq!(
        pairs(wire["hits"].as_array().unwrap()),
        want,
        "常驻索引那一臂的行号与现扫的不一样"
    );
    let _ = std::fs::remove_dir_all(&home);
}
