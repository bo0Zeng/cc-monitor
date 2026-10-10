//! 只读查询帧面宿主的判据。
//!
//! 夹具只造**结构**（目录名、行数、字节边界），行内容是占位的最小 JSON —— 不采任何真会话正文。

use super::*;
use std::path::{Path, PathBuf};

/// 本族的八条帧命令 —— **要求点名的那八条**，
/// 写成帧面名。它是判据的**异源**那一侧：下面那条从 `stream/inbound/` 源码里数「谁把活交给了
/// `read_face::answer`」，两边必须相等。
/// +2：`history-index` / `history-user-inputs`（要求「`--list-user-inputs` 与骨架
/// `--read-session-from-offset --index` 上帧面」那一句 —— 异源仍是手抄的要求，不是 `stream/inbound/`）。
/// +1：`accounts-trust`（「仍在拨号的 `--account-trust` / `--account-trust-zero`」
/// 随账号域一起上帧面 —— 异源是手抄的要求，不是 `stream/inbound/`）。
const FAMILY: &[&str] = &[
    "accounts-list",
    "accounts-sessions",
    "machine-interrupts",
    "accounts-trust",
    "history-index",
    "history-user-inputs",
    // 按行号取回（异源是手抄的要求「后端给『从第 N 行起 k 行』的读口」，不是 `stream/inbound/`）。
    "history-lines",
    // 会话内查找。
    "history-find",
    // 会话事实出成品（异源是手抄的要求「三样由后端出成品」，不是 `stream/inbound/`）。
    "history-facts",
    // 这台上需手动的会话清单（异源是手机端的要求「一次问一台，不是一条一问」，不是 `stream/inbound/`）。
    "sessions-needs",
    // 主线外清单的冷读（共用扫描图）。
    "history-branch",
    "history-turns",
    "history-read",
    // 记录还在不在（resume 一跳先问；异源是手抄的要求，不是 `stream/inbound/`）。
    "history-record",
    "history-search",
    // 各台搜索结果合一份（异源是手抄的要求，不是 `stream/inbound/`）。
    "history-search-merge",
    // 按字节分页出记录行 · 漂移账（异源是手抄的要求，不是 `stream/inbound/`）。
    // 按运行读一个子运行的记录（异源是「子 agent 的流归各自的运行、通用层按运行读」那条要求，不是 `stream/inbound/`）。
    "history-run",
    "history-page",
    "drift-report",
    "history-tail",
    // 这台后端的 stderr 诊断文件尾部（异源是手抄的要求「经那台后端的只读面」，不是 `stream/inbound/`）。
    "backend-log",
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
/// 左边从命令表各族的**生产段源码**切 `CommandSpec {` 块数出来（异源：不读本文件的任何常量）。
#[test]
fn the_registry_hands_exactly_the_eight_to_this_host() {
    let families = crate::guard_support::registry_sources();
    let mut got: Vec<String> = families
        .iter()
        .flat_map(|(_, prod)| prod.split("CommandSpec {").skip(1))
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
        "交给 `read_face::answer` 的帧命令与要求点名的那几条不相等"
    );
    // 那八条也都真在 `hello.commands` 里（`command_names`，从注册表派生）。
    for n in FAMILY {
        assert!(
            crate::stream::inbound::command_names().contains(n),
            "`{n}` 不在 `inbound::command_names()` —— hello 不会宣告它，monitor 的 `accepts` 会拒"
        );
    }
}

/// ★ 本宿主对八条**每一条**都有自己的臂（不是落进兜底那句「本族不认识」）；兜底对别的名字成立（正控）。
#[test]
fn every_member_has_its_own_arm_and_strangers_do_not() {
    let home = scratch("arms");
    for n in FAMILY {
        if let Err((_, m)) = answer_at(&home, n, &serde_json::json!({})) {
            assert!(!m.contains("has no command"), "`{n}` 落进了兜底臂：{m}");
        }
    }
    match answer_at(&home, "history-nope", &serde_json::json!({})) {
        Err((c, m)) => assert!(c == "bad_args" && m.contains("has no command"), "{c}: {m}"),
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

/// ★ F2（→出成品）：骨架索引与大纲清单两条帧命令的应答**就是成品**，
/// 条目与**夹具算出来的**逐条相等（异源：期望的偏移 / 行长 / uuid 从夹具字节自己数，不借被测函数）；
/// 键集合恒等（`{from, end, rows}` / `{from, end, entries}`）—— 不再是按行的头尾三段。
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
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };

    let v = answer_at(
        &home,
        "history-index",
        &serde_json::json!({"path": path, "offset": 0}),
    )
    .unwrap();
    assert_eq!(keys(&v), ["end", "from", "rows"], "骨架索引的成品形状变了");
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
    let got: Vec<(u64, u64)> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["o"].as_u64().unwrap(), r["n"].as_u64().unwrap()))
        .collect();
    assert_eq!(got, want, "索引行的偏移 / 行长与夹具对不上");
    assert_eq!(
        (v["from"].as_u64(), v["end"].as_u64()),
        (Some(0), Some(body.len() as u64))
    );

    let v = answer_at(
        &home,
        "history-user-inputs",
        &serde_json::json!({"path": path}),
    )
    .unwrap();
    assert_eq!(
        keys(&v),
        ["end", "entries", "from"],
        "大纲清单的成品形状变了"
    );
    let uuids: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(uuids, ["in-1"], "清单里的 uuid 与夹具里的 `in-*` 不相等");
    assert_eq!(v["end"].as_u64(), Some(body.len() as u64));
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

/// ★★帧面成品的条目 == CLI 那一臂**中段**的逐行（同一份夹具、两个出口）。
///
/// 异源在：CLI 那一臂照旧写头尾三段（`write_*` 经 stdout 那条路），本条把它的中段剥出来，
/// 与帧面那一臂的成品逐条比 —— 两臂共用的是扫描，不是装配；装配任一边丢一条 / 多一条 / 改一个键都红。
#[test]
fn the_frame_products_carry_exactly_the_rows_the_cli_arm_prints() {
    let home = scratch("c4b-cli-parity");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：两条用户输入（带同一个查找词）、一条助手行、一个空行、一条 meta —— 不采任何真会话正文。
    let body = [
        r#"{"type":"user","uuid":"u-1","timestamp":"t1","message":{"content":"zqx one"}}"#,
        r#"{"type":"assistant","uuid":"a-1","timestamp":"t1a","parentUuid":"u-1","message":{"content":[{"type":"text","text":"zqx two"}]}}"#,
        "",
        r#"{"type":"user","uuid":"m-1","timestamp":"t1aa","parentUuid":"a-1","isMeta":true,"message":{"content":"meta"}}"#,
        r#"{"type":"user","uuid":"u-2","parentUuid":"m-1","timestamp":"t2","message":{"content":"three zqx"}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    let middle = |f: &dyn Fn(&mut Vec<u8>) -> Result<(), String>| -> Vec<serde_json::Value> {
        let mut out: Vec<u8> = Vec::new();
        f(&mut out).unwrap();
        let lines: Vec<serde_json::Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(lines.len() >= 2, "CLI 臂没写头尾");
        lines[1..lines.len() - 1].to_vec()
    };
    let cases: [(&str, serde_json::Value, &str, Vec<serde_json::Value>); 3] = [
        (
            "history-index",
            serde_json::json!({"path": path, "offset": 0}),
            "rows",
            middle(&|o| {
                crate::observe::history_query::session_index_into(&home, &path, 0, None, o)
            }),
        ),
        (
            "history-user-inputs",
            serde_json::json!({"path": path, "from": 0}),
            "entries",
            middle(&|o| crate::observe::history_query::list_user_inputs_into(&home, &path, 0, o)),
        ),
        (
            "history-find",
            serde_json::json!({"path": path, "query": "zqx", "limit": 500}),
            "hits",
            middle(&|o| {
                crate::observe::history_query::find_in_session_into(
                    &home, &path, "zqx", false, 500, o,
                )
            }),
        ),
    ];
    for (cmd, args, key, cli) in cases {
        let v = answer_at(&home, cmd, &args).unwrap();
        assert!(
            !cli.is_empty(),
            "`{cmd}` 的 CLI 臂中段是空的 —— 夹具没打到，本条会空真"
        );
        assert_eq!(
            v[key].as_array().unwrap(),
            &cli,
            "`{cmd}` 的成品条目 != CLI 臂中段"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ →〔C4b 出成品〕`history-find` 的 `hits`：命中集合 == 夹具里 `hit-*` 那几条（异源：期望取自夹具的 uuid 命名），
/// 成品键集合恒等（`{total, hits}`）；围栏同一套。
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
    let mut keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["hits", "total"], "查找的成品形状变了");
    assert_eq!(v["total"].as_u64(), Some(2));
    let hits: Vec<&str> = v["hits"]
        .as_array()
        .unwrap()
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

/// 金样那份夹具会话：结构占位（uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
fn golden_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("g.jsonl");
    let body = [
        r#"{"type":"user","uuid":"in-1","timestamp":"t1","message":{"role":"user","content":"alpha zqx beta"}}"#,
        r#"{"type":"assistant","uuid":"out-1","timestamp":"t1a","parentUuid":"in-1","message":{"role":"assistant","content":[{"type":"text","text":"gamma zqx"},{"type":"tool_use","name":"x","input":{}}]}}"#,
        "",
        r#"{"type":"user","uuid":"meta-1","timestamp":"t1aa","parentUuid":"out-1","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
        r#"{"type":"user","uuid":"in-2","parentUuid":"meta-1","timestamp":"t2","message":{"role":"user","content":"delta"}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// 会话事实那一格的金样夹具：结构占位（id / uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
/// 四格各走到一次：分叉（首条 user 记录）· 一串接上了的重试 ＋ 一串还没下文的· 写类工具 · usage · 项目目录（开头那条的 cwd，后面进了子目录不跟）；派出子运行的调用与它的结果也在，会话事实不认它们。
fn golden_facts_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("f.jsonl");
    let body = [
        r#"{"type":"user","uuid":"f-1","timestamp":"t0a","cwd":"/g/proj","forkedFrom":{"sessionId":"src-0","messageUuid":"m-0"},"message":{"content":"q"}}"#,
        r#"{"type":"user","uuid":"f-1b","timestamp":"t0aa","parentUuid":"f-1","isMeta":true,"origin":{"kind":"peer","from":"ag-7","handback":true,"body":"report"},"message":{"content":"<agent-message from=\"ag-7\">report</agent-message>"}}"#,
        r#"{"type":"system","subtype":"api_error","uuid":"rt-1","timestamp":"t0aaa","parentUuid":"f-1b","retryAttempt":1,"maxRetries":10}"#,
        r#"{"type":"assistant","uuid":"f-2","parentUuid":"rt-1","timestamp":"t3","message":{"model":"m-g","usage":{"input_tokens":1,"cache_creation_input_tokens":2,"cache_read_input_tokens":3},"content":[{"type":"tool_use","id":"tu-1","name":"Edit","input":{"file_path":"/w/a.ts"}},{"type":"tool_use","id":"tu-2","name":"Task","input":{"description":"scan","subagent_type":"Explore"}}]}}"#,
        r#"{"type":"user","uuid":"f-3","timestamp":"t3a","parentUuid":"f-2","cwd":"/g/proj/sub","message":{"content":[{"type":"tool_result","tool_use_id":"tu-2","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"f-4","parentUuid":"f-3","timestamp":"t4","message":{"content":[{"type":"tool_use","id":"tu-3","name":"Agent","input":{"prompt":"p1\np2"}}]}}"#,
        r#"{"type":"assistant","uuid":"f-5","parentUuid":"f-4","timestamp":"t5","message":{"content":[{"type":"text","text":"done\nmore"}]}}"#,
        // 后台命令两条：一条还在跑（make test-all），一条收到了收场通知（住 queue-operation）⇒ 账上只剩前一条。
        r#"{"type":"assistant","uuid":"f-6","parentUuid":"f-5","timestamp":"2026-10-09T08:00:00.000Z","message":{"content":[{"type":"tool_use","id":"tu-b1","name":"Bash","input":{"command":"make test-all","run_in_background":true}}]}}"#,
        r#"{"type":"user","uuid":"f-7","parentUuid":"f-6","timestamp":"2026-10-09T08:00:01.000Z","toolUseResult":{"stdout":"","backgroundTaskId":"bb1"},"message":{"content":[{"type":"tool_result","tool_use_id":"tu-b1","content":"Command running in background with ID: bb1."}]}}"#,
        r#"{"type":"assistant","uuid":"f-8","parentUuid":"f-7","timestamp":"2026-10-09T08:10:00.000Z","message":{"content":[{"type":"tool_use","id":"tu-b2","name":"Bash","input":{"command":"python train.py","run_in_background":true}}]}}"#,
        r#"{"type":"user","uuid":"f-9","parentUuid":"f-8","timestamp":"2026-10-09T08:10:01.000Z","toolUseResult":{"stdout":"","backgroundTaskId":"bb2"},"message":{"content":[{"type":"tool_result","tool_use_id":"tu-b2","content":"Command running in background with ID: bb2."}]}}"#,
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-09T08:20:00.000Z","content":"<task-notification>\n<task-id>bb2</task-id>\n<tool-use-id>tu-b2</tool-use-id>\n<status>completed</status>\n<summary>Background command \"train\" completed (exit code 0)</summary>\n</task-notification>"}"#,
        r#"{"type":"system","subtype":"api_error","uuid":"rt-2","timestamp":"t5a","parentUuid":"f-5","retryAttempt":1,"maxRetries":10}"#,
        r#"{"type":"permission-mode","permissionMode":"acceptEdits","sessionId":"s-g"}"#,
        r#"{"type":"cost-state","totalCostUSD":0.4242,"modelUsage":{},"hasUnknownModelCost":false}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// 一轮的摘要那一格的金样夹具（结构占位）：第一轮 思考 · 两次调用（一次失败、一次被拒）· 中间的话 · 结论（`end_turn`）；
/// 第二轮还在跑（调用发出去、还没结果）。子运行的记录不算进主线的轮。
fn golden_turns_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("t.jsonl");
    let body = [
        r#"{"type":"user","uuid":"t-1","timestamp":"t1","message":{"content":"first line\nsecond"}}"#,
        r#"{"type":"assistant","uuid":"t-2","parentUuid":"t-1","timestamp":"t2","message":{"content":[{"type":"thinking","thinking":"h"}]}}"#,
        r#"{"type":"assistant","uuid":"t-3","parentUuid":"t-2","timestamp":"t3","message":{"content":[{"type":"text","text":"between"}]}}"#,
        r#"{"type":"assistant","uuid":"t-4","parentUuid":"t-3","timestamp":"t4","message":{"content":[{"type":"tool_use","id":"u-1","name":"Bash","input":{"command":"c"}}]}}"#,
        r#"{"type":"user","uuid":"t-5","parentUuid":"t-4","timestamp":"t5","message":{"content":[{"type":"tool_result","tool_use_id":"u-1","content":"Exit code 1","is_error":true}]}}"#,
        r#"{"type":"assistant","uuid":"t-6","parentUuid":"t-5","timestamp":"t6","message":{"content":[{"type":"tool_use","id":"u-2","name":"Edit","input":{"file_path":"/w/a"}}]}}"#,
        r#"{"type":"user","uuid":"t-7","parentUuid":"t-6","timestamp":"t7","message":{"content":[{"type":"tool_result","tool_use_id":"u-2","content":"The user doesn't want to proceed with this tool use.","is_error":true}]}}"#,
        r#"{"type":"assistant","uuid":"t-8","parentUuid":"t-7","timestamp":"t8","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"r1\n\nr2\nr3\nr4"}]}}"#,
        r#"{"type":"user","uuid":"t-9","parentUuid":"t-8","timestamp":"t9","message":{"content":"next"}}"#,
        r#"{"type":"assistant","uuid":"t-10","parentUuid":"t-9","timestamp":"t10","message":{"content":[{"type":"tool_use","id":"u-3","name":"Read","input":{"file_path":"/w/b"}}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// ★★**跨语言金样**：三条帧命令对同一份夹具会话的成品 == `tests/__fixtures__/session-reads.golden.json`。
/// ＋ 第四条 `history-facts`（对它自己那份夹具 [`golden_facts_session`]）。
///
/// 那份金样的另一个读者是 TS 解码器（`tests/frontend/ui/session-reads.vitest.ts` 读同一份文件、逐字段断言）⇒ 两侧**异源**：
/// 后端改一个键名 ⇒ 本条红；TS 解码器改一个键名 ⇒ 那边红。金样是手写落盘的，不是任一侧跑出来就算数的
/// （本条红时印出现打的成品，人读过再改金样）。
#[test]
fn the_three_products_match_the_cross_language_golden() {
    let home = scratch("c4b-golden");
    let path = golden_session(&home);
    let got = serde_json::json!({
        "history-index": answer_at(&home, "history-index", &serde_json::json!({"path": path, "offset": 0})).unwrap(),
        "history-user-inputs": answer_at(&home, "history-user-inputs", &serde_json::json!({"path": path, "from": 0})).unwrap(),
        "history-find": answer_at(&home, "history-find", &serde_json::json!({"path": path, "query": "zqx", "include_tools": false, "limit": 500})).unwrap(),
        "history-facts": answer_at(&home, "history-facts", &serde_json::json!({"path": golden_facts_session(&home)})).unwrap(),
        "history-turns": answer_at(&home, "history-turns", &serde_json::json!({"path": golden_turns_session(&home), "from": 0})).unwrap(),
    });
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/session-reads.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// `history-facts` 的 `writers`：这台 pidfile 里同一个会话 id 有两个活进程 ⇒ 两个 pid 都在（升序）；
/// 一个活一个死 ⇒ 只剩活的那一个（界面那一句只在不止一个时说）。pidfile 照真 claude 的形状写（`procStart` 对得上）。
#[cfg(target_os = "linux")]
#[test]
fn facts_name_every_live_process_writing_the_session() {
    let home = scratch("writers");
    let sid = "bbbbbbbb-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-w");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"user\",\"uuid\":\"w-1\",\"message\":{\"content\":\"q\"}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let pidfile = |pid: u32, ticks: u64| {
        let body = format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}"}}"#
        );
        std::fs::write(pids.join(format!("{pid}.json")), body).unwrap();
    };
    let spawn = || {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
        pidfile(c.id(), ticks);
        c
    };
    let writers = || {
        answer_at(&home, "history-facts", &serde_json::json!({ "path": path })).unwrap()["writers"]
            .clone()
    };
    let (mut a, mut b) = (spawn(), spawn());
    let mut want = vec![a.id(), b.id()];
    want.sort_unstable();
    let two = writers();
    // 一个死了（pidfile 还留着）⇒ 不算。
    let _ = b.kill();
    let _ = b.wait();
    let one = writers();
    let _ = a.kill();
    let _ = a.wait();
    let none = writers();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        two,
        serde_json::json!(want),
        "两个活进程持着同一个会话 id，没都报出来"
    );
    assert_eq!(
        one,
        serde_json::json!([a.id()]),
        "死了的那个进程还被算成在写"
    );
    assert_eq!(none, serde_json::json!([]));
}

/// `history-facts` 的 `needs`：这台 pidfile 说在等（`status: waiting`）⇒ 配上记录里没结果的那一步出成品（种类 · 那一句 · 何时起等）；
/// 不在等 ⇒ `null`；在等的那个进程死了（pidfile 还留着）⇒ 不算。pidfile 照真 claude 的形状写（`procStart` 对得上）。
#[cfg(target_os = "linux")]
#[test]
fn facts_say_what_the_session_is_waiting_for() {
    let home = scratch("needs");
    let sid = "cccccccc-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"rm -rf build/\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let mut c = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
    let pidfile = |status: &str| {
        let body = format!(
            r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"{status}","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#,
            c.id()
        );
        std::fs::write(pids.join(format!("{}.json", c.id())), body).unwrap();
    };
    let needs = || {
        answer_at(&home, "history-facts", &serde_json::json!({ "path": path })).unwrap()["needs"]
            .clone()
    };
    pidfile("waiting");
    let waiting = needs();
    pidfile("busy");
    let busy = needs();
    pidfile("waiting");
    let _ = c.kill();
    let _ = c.wait();
    let dead = needs();
    let _ = std::fs::remove_dir_all(&home);
    let (waited, rest) = split_waited(&waiting);
    assert_eq!(
        rest,
        serde_json::json!({"kind": "approve", "tool": "Bash", "call": "b1", "what": "rm -rf build/", "sinceMs": 1_700_000_000_000u64, "text": copy_core::copy_text("beSession.needs.approve", &[]), "tone": "need", "rank": 1})
    );
    // 已等多久在这台算（这台读 pidfile 那一刻减那份 pidfile 里的起点），字由时长那一处写。
    let ms = waited.expect("有起点就有已等多久");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let span = now - 1_700_000_000_000;
    assert!(ms <= span && ms + 60_000 > span, "{ms}");
    assert_eq!(
        waiting["waitedText"],
        copy_core::short_duration(ms),
        "与过程行耗时同一种短写法"
    );
    assert_eq!(busy, serde_json::Value::Null, "不在等却报了需要你");
    assert_eq!(dead, serde_json::Value::Null, "在等的进程死了还算需要你");
}

/// `history-facts` 的 `limits`：设置里的上限表随请求交来、上限在这里定；形状不对 ⇒ `bad_args`。
#[test]
fn history_facts_applies_the_limits_it_is_given() {
    let home = scratch("facts-limits");
    let path = golden_facts_session(&home);
    let ask = |args: serde_json::Value| answer_at(&home, "history-facts", &args);
    let v = ask(serde_json::json!({ "path": path, "limits": {"M-G": 100} })).unwrap();
    assert_eq!(
        (v["usage"]["limit"].clone(), v["usage"]["limitFrom"].clone()),
        (serde_json::json!(100), serde_json::json!("setting"))
    );
    assert_eq!(
        ask(serde_json::json!({ "path": path, "limits": {"m": "x"} }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 上限的真来源是中转：中转看见过这个会话（会话 id ＝ 记录文件名）的请求带了扩展上下文那一项 ⇒ 1M（`relay`），
/// 看见过但没带 ⇒ 默认 200k（`relay`）；没看见过 ⇒ 照原来的次序（这里判不出 ⇒ `assumed`）。
#[test]
fn history_facts_takes_the_context_window_from_what_the_relay_saw() {
    let home = scratch("facts-relay");
    let dir = home.join("projects").join("-relay");
    std::fs::create_dir_all(&dir).unwrap();
    let rec = r#"{"type":"assistant","uuid":"r-1","message":{"model":"claude-x","usage":{"input_tokens":90000},"content":[]}}"#;
    let mut paths = Vec::new();
    for sid in ["rf-wide", "rf-std", "rf-direct"] {
        let p = dir.join(format!("{sid}.jsonl"));
        std::fs::write(&p, format!("{rec}\n")).unwrap();
        paths.push(p.to_string_lossy().to_string());
    }
    let item = ("anthropic-beta", "context-1m");
    crate::observe::relay_marks::note("rf-wide", &[(item, true)]);
    crate::observe::relay_marks::note("rf-std", &[(item, false)]);
    let ask =
        |p: &str| answer_at(&home, "history-facts", &serde_json::json!({ "path": p })).unwrap();
    let got: Vec<(serde_json::Value, serde_json::Value)> = paths
        .iter()
        .map(|p| {
            let v = ask(p);
            (v["usage"]["limit"].clone(), v["usage"]["limitFrom"].clone())
        })
        .collect();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        vec![
            (serde_json::json!(1_000_000), serde_json::json!("relay")),
            (serde_json::json!(200_000), serde_json::json!("relay")),
            (serde_json::json!(1_000_000), serde_json::json!("assumed")),
        ]
    );
}

/// ★`history-facts` 经帧面续传：把上一次的应答**原样**当 `prior` 交回（与线上同形：过一遍 JSON 文本），
/// 文件长了一截之后接着问 == 对长了之后的整份从 0 问（两向：整个值相等）。续点的两道校验：
/// 截断（续点越过文件尾）⇒ `failed`；改写到续点不在行边界上 ⇒ `failed`；`prior` 形状不对 / 缺 `path` ⇒ `bad_args`。
#[test]
fn history_facts_resumes_from_its_own_answer_and_refuses_a_stale_resume_point() {
    let home = scratch("facts");
    let path = golden_facts_session(&home);
    let ask = |args: serde_json::Value| answer_at(&home, "history-facts", &args);
    let first = ask(serde_json::json!({ "path": path })).unwrap();
    let wire: serde_json::Value =
        serde_json::from_str(&first.to_string()).expect("应答过一遍 JSON 文本");
    let more = r#"{"type":"assistant","uuid":"f-5","message":{"content":[{"type":"tool_use","id":"tu-4","name":"Write","input":{"file_path":"/w/b.ts"}}]}}"#;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut f, format!("{more}\n").as_bytes()).unwrap();
    drop(f);
    let resumed = ask(serde_json::json!({ "path": path, "prior": wire })).unwrap();
    let whole = ask(serde_json::json!({ "path": path })).unwrap();
    assert_eq!(resumed, whole, "接着问与从 0 问不相等");
    assert_ne!(
        resumed, first,
        "长了的那一截没进成品（夹具那条写类调用没进改动文件）"
    );
    assert_eq!(
        whole["touchedFiles"],
        serde_json::json!(["/w/a.ts", "/w/b.ts"])
    );

    // 截断：续点越过文件尾。
    std::fs::write(&path, "{}\n").unwrap();
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": wire }))
            .unwrap_err()
            .0,
        "failed"
    );
    // 改写：长度够，但续点前一个字节不是换行。
    let end = wire["end"].as_u64().unwrap() as usize;
    std::fs::write(&path, "x".repeat(end + 10)).unwrap();
    let e = ask(serde_json::json!({ "path": path, "prior": wire })).unwrap_err();
    assert!(e.0 == "failed" && e.1.contains("line boundary"), "{e:?}");
    // 正控：同一个长度、续点恰在行边界上 ⇒ 接着读（证明上一条红的是「不在行边界」而不是别的）。
    std::fs::write(&path, format!("{}\n{}\n", "x".repeat(end - 1), "{}")).unwrap();
    assert!(ask(serde_json::json!({ "path": path, "prior": wire })).is_ok());
    // 形状不对 / 缺 path。
    let mut bad = wire.clone();
    bad.as_object_mut().unwrap().remove("usage");
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": bad }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(
        ask(serde_json::json!({ "prior": wire })).unwrap_err().0,
        "bad_args"
    );
    // 围栏同族：`projects` 之外的文件不读。
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    assert!(ask(serde_json::json!({ "path": outside.to_string_lossy() })).is_err());
    let _ = std::fs::remove_dir_all(&home);
}

/// `history-record`：在 ⇒ `present:true`；不在 ⇒ `present:false`（一个答案，不是错误）；
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

/// `history-record` 按**这次 resume 要用的账号根**查（`GP1.md §4`）。
///
/// 夹具两棵树：这台的家（`home`）与一个账号目录（`acct`），sid 只在账号目录里。
/// 不带 `configDir` ⇒ 答不在（与改之前逐字同一问）；带了 ⇒ 答在，`root` == 那棵树的 `projects`（两向）。
/// 坏 `configDir`（相对 · 上跳 · shell 元字符 · 非字符串）⇒ `bad_args`，且**不碰盘**：同一个坏参数配一个
/// 不存在的目录也是 `bad_args`（碰了盘就会答「不在」而不是拒）。结构夹具，不采会话正文。
#[test]
fn gp1_history_record_looks_in_the_account_root_it_is_given() {
    let home = scratch("record-gp1-home");
    let acct = scratch("record-gp1-acct");
    session(&acct, "-p", "cccc-3333", 1, false);
    let ask = |args: serde_json::Value| answer_at(&home, "history-record", &args);
    let home_root = home.join("projects").to_string_lossy().into_owned();
    let acct_root = acct.join("projects").to_string_lossy().into_owned();
    let acct_dir = acct.to_string_lossy().into_owned();
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333" })).unwrap(),
        serde_json::json!({ "present": false, "root": home_root }),
        "不带 configDir ⇒ 查这台的家目录"
    );
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333", "configDir": null })).unwrap(),
        serde_json::json!({ "present": false, "root": home_root }),
        "null == 缺席"
    );
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333", "configDir": acct_dir })).unwrap(),
        serde_json::json!({ "present": true, "root": acct_root }),
        "带了 configDir ⇒ 在那棵树里找"
    );
    for bad in [
        serde_json::json!("relative/acct"),
        serde_json::json!(format!("{acct_dir}/../x")),
        serde_json::json!(format!("{acct_dir}$(id)")),
        serde_json::json!(7),
    ] {
        match ask(serde_json::json!({ "sid": "cccc-3333", "configDir": bad })) {
            Err(("bad_args", _)) => {}
            other => panic!("坏 configDir {bad} 应当 bad_args，实得 {other:?}"),
        }
    }
    std::fs::remove_dir_all(&home).ok();
    std::fs::remove_dir_all(&acct).ok();
}

// ════════════════════════════════════════════════════════════════════════════
// `history-lines`：按行号取回
//
//  要求：「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
//  **不许静默堆**」· 「无索引会话的重放缓冲上界（要先有不依赖索引的取回路）」——
//  本族是那条「不依赖索引的取回路」，它取回来的行号必须与实时 `seq` 同一个空间，否则取回的正文落错位置。
// ════════════════════════════════════════════════════════════════════════════

/// 手写夹具：可计行、空行、全空白行、只有 BOM 的行、带 BOM 的行、CRLF、一行超长、torn 残尾。
/// 行内容是结构占位（不采真会话正文）。
fn lines_fixture() -> (String, Vec<String>) {
    let big = format!("{{\"big\":\"{}\"}}", "x".repeat(200));
    let body = format!(
        "{{\"i\":0}}\n\n\u{feff}{{\"i\":1}}\n   \t\n\u{feff}\n{{\"i\":2}}\r\n{{\"i\":3}}\n{big}\n{{\"i\":5}}\n{{\"torn\":"
    );
    // **手写**的期望：可计行按出现顺序（不从被测函数派生）。
    let want = vec![
        "{\"i\":0}".to_string(),
        "\u{feff}{\"i\":1}".to_string(),
        "{\"i\":2}\r".to_string(),
        "{\"i\":3}".to_string(),
        big,
        "{\"i\":5}".to_string(),
    ];
    (body, want)
}

/// ★ L1：任意 `[from, until)`（含越界、含 `until` 缺席）取回的 == 手写期望的那一段；
/// `next == from + 条数`；`eof` 当且仅当读过了最后一个完整行。
#[test]
fn lines_by_number_match_the_hand_written_countable_rows() {
    let (body, want) = lines_fixture();
    let n = want.len() as u64;
    for from in 0..=n + 1 {
        for until in (from..=n + 2).map(Some).chain([None]) {
            let got = crate::observe::history_query::read_lines_from(
                std::io::Cursor::new(body.as_bytes()),
                from,
                until,
                1 << 20,
                1 << 20,
            )
            .unwrap();
            let hi = until.unwrap_or(u64::MAX).min(n).max(from.min(n));
            let exp: Vec<String> = want[(from.min(n) as usize)..(hi as usize)].to_vec();
            assert_eq!(got.lines, exp, "[{from}, {until:?})");
            assert_eq!(got.from, from);
            assert_eq!(
                got.next,
                from + exp.len() as u64,
                "[{from}, {until:?}) 的续点"
            );
            assert_eq!(
                got.eof,
                until.is_none_or(|u| u > n),
                "[{from}, {until:?}) 的 eof"
            );
        }
    }
}

/// ★ L1：一页装不下就停在行边界、至少交一行；按续点接着要，拼起来 == 全部可计行（逐条）。
#[test]
fn lines_by_number_page_on_row_boundaries_and_reassemble() {
    let (body, want) = lines_fixture();
    for page in [1usize, 7, 30, 250, 1 << 20] {
        let mut got: Vec<String> = Vec::new();
        let mut from = 0u64;
        let mut rounds = 0;
        loop {
            let pg = crate::observe::history_query::read_lines_from(
                std::io::Cursor::new(body.as_bytes()),
                from,
                None,
                page,
                1 << 20,
            )
            .unwrap();
            assert!(
                pg.eof || !pg.lines.is_empty(),
                "page={page}：没到头却一行没交（死循环）"
            );
            got.extend(pg.lines);
            from = pg.next;
            rounds += 1;
            assert!(rounds < 100, "page={page}：翻不完");
            if pg.eof {
                break;
            }
        }
        assert_eq!(got, want, "page={page} 拼回来的不对");
    }
}

/// ★ L1：要的那一段里有超长行 ⇒ `oversized_line`（不截半行）；只是**数过**它（不交）⇒ 照常答。
#[test]
fn lines_by_number_refuse_an_oversized_row_but_can_count_past_it() {
    let (body, want) = lines_fixture();
    let cap = 100; // 夹具那一行 > 200 字节
    match crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(body.as_bytes()),
        3,
        Some(5),
        1 << 20,
        cap,
    ) {
        Err(("oversized_line", _)) => {}
        other => panic!("超长行被交出去了：{other:?}"),
    }
    let after = crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(body.as_bytes()),
        5,
        None,
        1 << 20,
        cap,
    )
    .unwrap();
    assert_eq!(after.lines, want[5..].to_vec());
}

/// ★ L1：同一份夹具的三个出口数的是**同一个**行号空间 —— `history-tail` 的 `total` ·
/// `history-index` 的行数 · `history-lines` 从 0 取到底的条数，两两相等（三个扫描各写各的循环，只共用
/// `line_counts` 那一个判定）；帧面那一臂的键集合恒等；围栏照旧。
#[test]
fn lines_by_number_share_the_seq_space_with_tail_and_index() {
    let home = scratch("cf2-lines");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let (body, want) = lines_fixture();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().into_owned();
    let tail = answer_at(
        &home,
        "history-tail",
        &serde_json::json!({"path": path, "n": 2}),
    )
    .unwrap();
    let index = answer_at(
        &home,
        "history-index",
        &serde_json::json!({"path": path, "offset": 0}),
    )
    .unwrap();
    let lines = answer_at(&home, "history-lines", &serde_json::json!({"path": path})).unwrap();
    let mut keys: Vec<&String> = lines.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["eof", "from", "lines", "next"], "成品形状变了");
    let n = want.len() as u64;
    assert_eq!(tail["total"].as_u64(), Some(n), "history-tail 的 total");
    assert_eq!(
        index["rows"].as_array().map(|r| r.len() as u64),
        Some(n),
        "history-index 的行数"
    );
    // `history-lines` 出的是**记录行**（只装进界面的那些；这份结构占位语料一条都不进）⇒ 条数不再等于行数，
    //   行号空间由 `next`（数的是可计行）钉。
    assert_eq!(lines["lines"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        lines["next"].as_u64(),
        Some(n),
        "history-lines 的 next（可计行数）"
    );
    assert_eq!(lines["eof"].as_bool(), Some(true));
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    match answer_at(
        &home,
        "history-lines",
        &serde_json::json!({"path": outside.to_string_lossy()}),
    ) {
        Err((c, _)) => assert_eq!(c, "refused"),
        Ok(v) => panic!("围栏外的文件被读了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ L1 跨 crate：`history-lines` 从 0 取到底，第 k 条的 uuid == 金标准第 k 行（`skeleton-seq-space.golden`，手算；
/// monitor 那一侧 `LineNumberer` 与后端索引对的也是它）；条数 == `#count`（torn 残尾不交）。
#[test]
fn lines_by_number_match_the_shared_seq_space_golden() {
    let data: &[u8] = include_bytes!("../../__fixtures__/skeleton-seq-space.jsonl");
    let golden = include_str!("../../__fixtures__/skeleton-seq-space.golden");
    let mut want: Vec<Option<String>> = Vec::new();
    let mut count = None;
    for l in golden.lines() {
        if let Some(v) = l.strip_prefix("#count\t") {
            count = v.parse::<usize>().ok();
        } else if !l.starts_with('#') {
            let (_, u) = l.split_once('\t').unwrap();
            want.push((u != "-").then(|| u.to_string()));
        }
    }
    assert!(
        !want.is_empty(),
        "金标准一行都没抽到 —— 下面的相等在空集上绿"
    );
    let pg = crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(data),
        0,
        None,
        1 << 20,
        1 << 20,
    )
    .unwrap();
    let got: Vec<Option<String>> = pg
        .lines
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l.trim_start_matches('\u{feff}').trim())
                .ok()
                .and_then(|v| v.get("uuid").and_then(|u| u.as_str()).map(str::to_string))
        })
        .collect();
    assert_eq!(got, want);
    assert_eq!(Some(pg.lines.len()), count);
    assert!(pg.eof);
}

/// 账号那两条的入参闸：`accounts-list` 缺 `agent` ⇒ `bad_args`（不猜是哪一家 ——
/// 猜错就是把别家的号按 apikey 号报）；`accounts-trust` 缺 `cwd` / `configDir` 类型不对 ⇒ `bad_args`。
/// 要求：「② 对端错 —— 通道是通的，答案是『不行』」（入参错是对端的明拒，不是一个空答案）。
#[test]
fn the_account_pair_refuses_missing_or_mistyped_arguments() {
    let home = scratch("c4c-args");
    for (cmd, args) in [
        ("accounts-list", serde_json::json!({})),
        ("accounts-list", serde_json::json!({"agent": 7})),
        ("accounts-list", serde_json::json!({"agent": "claud-code"})),
        ("accounts-trust", serde_json::json!({"configDir": null})),
        (
            "accounts-trust",
            serde_json::json!({"configDir": 3, "cwd": "/w"}),
        ),
    ] {
        match answer_at(&home, cmd, &args) {
            Err((c, _)) => assert_eq!(c, "bad_args", "{cmd} {args}"),
            Ok(v) => panic!("{cmd} {args} 被答了：{v}"),
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `backend-log`：没装 ⇒ `path: null`；装了 ⇒ 尾部，截断时从截点后第一个换行起（不给半行）。期望手写。
#[test]
fn gap1_backend_log_returns_the_tail_cut_at_a_line() {
    let home = scratch("gap1-log");
    let p = home.join("stderr.log");
    std::fs::write(&p, "first line\nsecond line\nthird\n").unwrap();
    assert_eq!(
        log_tail(None, 100).unwrap(),
        serde_json::json!({"path": null, "size": 0, "text": "", "truncated": false})
    );
    let all = log_tail(Some(&p), 1000).unwrap();
    assert_eq!(
        (
            all["text"].as_str(),
            all["size"].as_u64(),
            all["truncated"].as_bool()
        ),
        (
            Some("first line\nsecond line\nthird\n"),
            Some(29),
            Some(false)
        )
    );
    // 尾 10 字节 = "ine\nthird\n" ⇒ 截点落在 second line 中间 ⇒ 从下一行起。
    let tail = log_tail(Some(&p), 10).unwrap();
    assert_eq!(
        (tail["text"].as_str(), tail["truncated"].as_bool()),
        (Some("third\n"), Some(true))
    );
    // 帧面那一臂：没被交路径的进程（判据进程就是）⇒ `path: null`，`maxBytes` 形状不对 ⇒ `bad_args`。
    assert_eq!(
        answer_at(&home, "backend-log", &serde_json::json!({})).unwrap()["path"],
        Value::Null
    );
    assert_eq!(
        answer_at(&home, "backend-log", &serde_json::json!({"maxBytes": "x"}))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

// ════════════════════════════════════════════════════════════════════════════
// 会话正文那几条出成品：跨语言金样
// ════════════════════════════════════════════════════════════════════════════

/// 金样夹具：结构占位（uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
/// 四形各一次：不进界面的元数据（占号不出）· 空白行（不占号）· user（带 `cwd`）· assistant；外加一个子 agent。
fn golden_record_session(home: &Path) -> PathBuf {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("r.jsonl");
    let body = [
        r#"{"type":"permission-mode","permissionMode":"default"}"#,
        "",
        r#"{"type":"user","uuid":"r-1","timestamp":"t1","cwd":"/w","message":{"role":"user","content":"q"}}"#,
        r#"{"type":"assistant","uuid":"r-2","timestamp":"t2","message":{"role":"assistant","content":[{"type":"text","text":"a"}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    let sub = dir.join("r").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(
        sub.join("agent-a1.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"s-1","timestamp":"t3","isSidechain":true,"agentId":"a1","message":{"role":"user","content":"go"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"s-2","timestamp":"t4","isSidechain":true,"agentId":"a1","message":{"id":"m-s2","role":"assistant","content":[{"type":"text","text":"ok"}]}}"#,
            "\n"
        ),
    )
    .unwrap();
    p
}

/// ★★**跨语言金样**：`history-read`（monitor 旁路快照收）· `history-page` · `history-lines` · `history-branch` · `history-run`
/// （界面收）对同一份夹具的成品 == `tests/__fixtures__/record-reads.golden.json`（路径里夹具那一截换成 `<home>`）。
///
/// 另两个读者读同一份：monitor `frame_query::row_of`（`tests/frontend/shell/frame_query_tests.rs`）·
/// TS 解码器（`tests/frontend/ui/session-reads.vitest.ts` 那一节）⇒ 三侧**异源**：后端改一个键名本条红，收的那两侧改一个键名各自红。
#[test]
fn the_record_products_match_the_cross_language_golden() {
    let home = scratch("mod-golden");
    let p = golden_record_session(&home);
    let path = p.to_string_lossy().into_owned();
    let got = serde_json::json!({
        "history-read": answer_at(&home, "history-read", &serde_json::json!({"path": path})).unwrap(),
        "history-page": answer_at(&home, "history-page", &serde_json::json!({"path": path, "whole": true})).unwrap(),
        "history-lines": answer_at(&home, "history-lines", &serde_json::json!({"path": path, "from": 1})).unwrap(),
        "history-branch": answer_at(&home, "history-branch", &serde_json::json!({"path": path})).unwrap(),
        "history-run": answer_at(
            &home,
            "history-run",
            &serde_json::json!({"parent": path, "run": "a1"}),
        )
        .unwrap(),
    });
    let canon = std::fs::canonicalize(&home)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let got: serde_json::Value =
        serde_json::from_str(&got.to_string().replace(&canon, "<home>")).unwrap();
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/record-reads.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// 全文搜索的应答把「读不动几份」与「内容搜索不覆盖的那几家」一起交回（此前只进日志，界面只能说「共 N 条」像是全的）；
/// `titles: true` 只比标题与第一句：只在标题里出现的词也搜得到那一个会话。
#[test]
fn the_search_answer_says_what_it_could_not_search_and_titles_mode_finds_title_only_words() {
    let home = scratch("search-said");
    let dir = home.join("projects").join("-w-x");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("0000aaaa-0000-4000-8000-0000000000aa.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/w/x","message":{"role":"user","content":"占位正文"}}"#,
            "\n",
            r#"{"type":"ai-title","aiTitle":"订单重试"}"#,
            "\n"
        ),
    )
    .unwrap();
    let ask = |args: Value| answer_at(&home, "history-search", &args).expect("搜索");
    let v = ask(json!({ "query": "订单" }));
    assert_eq!(v["unreadable"], json!(0), "读不动几份没交回：{v}");
    assert!(v["skipped"].is_array(), "内容搜索不覆盖的那几家没交回：{v}");
    assert_eq!(v["lines"], json!([]), "只在标题里的词被当成了内容命中：{v}");
    let v = ask(json!({ "query": "订单", "titles": true }));
    let lines = v["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 1, "按标题搜没搜到那一个会话：{v}");
    let row: Value = serde_json::from_str(lines[0].as_str().unwrap()).unwrap();
    assert_eq!(
        (row["title"].as_str(), row["hitCount"].as_u64()),
        (Some("订单重试"), Some(0))
    );
    std::fs::remove_dir_all(&home).ok();
}

/// `history-find` 每条命中旁边一格 `tsText`：那条记录的时刻按这台本地钟写好（今天 `HH:MM` · 昨天 · 更早带日期），界面照抄；读不出时刻 ⇒ 空串。
#[test]
fn find_hits_carry_their_time_text() {
    let home = scratch("p15-find-ts");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let ts = "2026-10-07T20:30:15.123Z";
    let rows = [
        format!(
            r#"{{"type":"user","uuid":"hit-1","timestamp":"{ts}","message":{{"content":"zqxneedle one"}}}}"#
        ),
        r#"{"type":"user","uuid":"hit-2","message":{"content":"two zqxneedle"}}"#.to_string(),
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let v = answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": p.to_string_lossy(), "query": "zqxneedle"}),
    )
    .unwrap();
    let ms = crate::common::time::parse_iso8601_ms(ts).unwrap();
    assert_eq!(
        v["hits"][0]["tsText"],
        crate::common::time::hit_text_here(ms).as_str()
    );
    assert!(v["hits"][0]["tsText"]
        .as_str()
        .is_some_and(|t| t.contains(':')));
    assert_eq!(v["hits"][1]["tsText"], "", "读不出时刻 ⇒ 空串");
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **`summaryOnly` 那一位真的接到了三条读记录命令上，而且两头都断。**
///
/// 核那一层（剥哪几格 · 折起那一行要用的那几格一格不少）由
/// `observe/record_page_tests.rs::the_summary_only_product_drops_every_body_cell_and_keeps_every_folded_cell`
/// 逐标记两向钉着。本条钉的是**宿主这一跳**：三条命令各自把 `args` 里那一位读出来、原样递进核。
/// 核那条判据单独立不住这件事 —— `read_face` 一行都不读那一位，它照样全绿。
///
/// 两头都断：置真 ⇒ 正文标记一个不剩且**条数不变**；缺席（= 今天的行为）⇒ 正文标记**必须**在。
/// 只断前一头的话，宿主把成品整个弄空也能绿。
#[test]
fn the_three_record_reads_each_honour_summary_only_both_ways() {
    let home = scratch("summary-only");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：`ZQBODY-*` 只住正文（思考 · 说的话 · 工具结果），`ZQKEEP-*` 另有一份住后端判好的 `userText`
    //   —— 不采任何真会话正文。
    let body = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-asked"}]}}"#,
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"ZQBODY-think"},{"type":"text","text":"ZQBODY-said"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt"}}]}}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQBODY-result"}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    const GONE: &[&str] = &["ZQBODY-think", "ZQBODY-said", "ZQBODY-result"];

    // 三条各自的入参与装成品的那一格。
    let cases: [(&str, serde_json::Value, &str); 3] = [
        ("history-read", serde_json::json!({"path": path}), "rows"),
        ("history-page", serde_json::json!({"path": path}), "lines"),
        (
            "history-lines",
            serde_json::json!({"path": path, "from": 0}),
            "lines",
        ),
    ];
    for (cmd, base, key) in cases {
        let ask = |summary_only: Option<bool>| {
            let mut args = base.clone();
            if let Some(b) = summary_only {
                args[&"summaryOnly".to_string()] = serde_json::json!(b);
            }
            answer_at(&home, cmd, &args)
                .unwrap_or_else(|(c, m)| panic!("`{cmd}` 答错了（{c}）：{m}"))
        };
        let full = ask(None);
        let fold = ask(Some(true));
        let off = ask(Some(false));
        let text = |v: &serde_json::Value| serde_json::to_string(&v[key]).unwrap();
        let count = |v: &serde_json::Value| v[key].as_array().map_or(0, Vec::len);

        // ── 缺席（今天的行为）＋ 显式 false：正文**必须**在 ──
        for (what, v) in [("缺席", &full), ("显式 false", &off)] {
            let t = text(v);
            for m in GONE {
                assert!(
                    t.contains(m),
                    "`{cmd}`（`summaryOnly` {what}）的成品里没有正文标记 `{m}` —— \
                     默认那一形变了，或者夹具没打到（下面那一半会恒绿）"
                );
            }
            assert!(
                t.contains("ZQKEEP-asked"),
                "`{cmd}`（{what}）连人说的话都没有"
            );
        }
        // ── 置真：正文一个不剩，条数一条不少 ──
        let t = text(&fold);
        for m in GONE {
            assert!(
                !t.contains(m),
                "`{cmd}` 收了 `summaryOnly: true` 还在给正文 `{m}`：{t}"
            );
        }
        assert!(
            t.contains("ZQKEEP-asked"),
            "`{cmd}` 把折起那一行要显示的人说的话也剥掉了"
        );
        assert_eq!(
            count(&fold),
            count(&full),
            "`{cmd}` 收了 `summaryOnly` 之后条数变了 —— 剥的该是内容，不是行"
        );
        assert_eq!(count(&fold), 3, "`{cmd}` 的夹具三行都该出成品");
        // 两形除了 `{key}` 那一格之外逐格相同（`next` / `eof` / `nextSeq` / `from` 不许受它影响）。
        let strip = |v: &serde_json::Value| {
            let mut o = v.as_object().unwrap().clone();
            o.remove(key);
            o
        };
        assert_eq!(
            strip(&fold),
            strip(&full),
            "`{cmd}` 收了 `summaryOnly` 之后 `{key}` 之外的格也变了"
        );
        assert!(
            t.len() < text(&full).len(),
            "`{cmd}` 的折起那一形没比全文小"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **这台上需手动的会话清单**（`sessions-needs`）：恰好是这台此刻活着、那台 pidfile 说在等人的那几个会话；
/// 每一个的 `needs` 就是 `history-facts` 对同一份记录答的那一格（同一处判、同一份字，清单不另判）；
/// 记录找不到的照列（判不出是哪一步，但框是哪种照那台说的）；在跑 · 空闲 · 进程死了的都不在。
#[cfg(target_os = "linux")]
#[test]
fn sessions_needs_lists_the_waiting_sessions_each_with_its_history_facts_needs() {
    let home = scratch("sessions-needs");
    let ask = "dddddddd-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{ask}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"rm -rf build/\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let mut kids = Vec::new();
    let mut put_as = |sid: &str, status: &str, wait: &str, since: u64| {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("起始时刻");
        std::fs::write(
            pids.join(format!("{}.json", c.id())),
            format!(r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"{status}","waitingFor":"{wait}","statusUpdatedAt":{since}}}"#, c.id()),
        )
        .unwrap();
        kids.push(c);
    };
    let at = 1_700_000_000_000u64;
    put_as("s-busy", "busy", "permission prompt", at);
    put_as("s-idle", "idle", "permission prompt", at);
    put_as(ask, "waiting", "permission prompt", at);
    // 同是批准、起等得晚一些 ⇒ 排在 `ask` 后面；要联网的那个起等得最晚，但危险度最高 ⇒ 排第一。
    put_as("s-norecord", "waiting", "permission prompt", at + 60_000);
    put_as("s-net", "waiting", "sandbox request", at + 120_000);
    put_as("s-dead", "waiting", "permission prompt", at);
    let mut dead = kids.pop().unwrap();
    let _ = dead.kill();
    let _ = dead.wait();
    let list = answer_at(&home, "sessions-needs", &serde_json::json!({})).expect("答了");
    let facts = answer_at(&home, "history-facts", &serde_json::json!({ "path": path })).unwrap();
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    let rows = list["waiting"].as_array().expect("waiting 是数组");
    // 序照核心那一处（先答哪个：危险度 · 再等得久的在前），清单不另排。
    let sids: Vec<&str> = rows.iter().map(|r| r["sid"].as_str().unwrap()).collect();
    assert_eq!(sids, vec!["s-net", ask, "s-norecord"], "{list}");
    let row = |sid: &str| rows.iter().find(|r| r["sid"] == sid).unwrap();
    assert!(!facts["needs"].is_null(), "夹具：history-facts 该说在等");
    // 两次问各自读一遍 pidfile ⇒ 已等多久差几毫秒；别的格逐字相同。
    assert_eq!(
        split_waited(&row(ask)["needs"]).1,
        split_waited(&facts["needs"]).1,
        "清单那一格与 history-facts 不是同一份"
    );
    assert_eq!(row("s-norecord")["needs"]["kind"], "approve");
    assert_eq!(row("s-norecord")["needs"]["call"], serde_json::Value::Null);
    for r in rows {
        let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["needs", "sid"], "一行只有这两格：{r}");
    }
}

/// 一份 `needs` 拆成「已等多久（毫秒）」与其余各格（已等的那两格随问的时刻变，单拿出来判）。
fn split_waited(n: &serde_json::Value) -> (Option<u64>, serde_json::Value) {
    let mut rest = n.clone();
    let o = rest.as_object_mut().expect("needs 是对象");
    let ms = o.remove("waitedMs").and_then(|v| v.as_u64());
    o.remove("waitedText");
    (ms, rest)
}
