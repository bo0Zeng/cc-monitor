//! 秤（读数不是判据）：真规模本机历史上帧面 `history-search` 的冷首趟与热态，同一进程连问（与常驻后端同形）。
//! 只出耗时 · 条数 · 输出行 sha256 前 16 位（前后两棵树逐问比摘要 ⇒ 逐条相等），不打印正文。跑法（`src/backend` 下）：
//! `SX1_EVICT=<仓根>/tests/evidence/SX1-evict-page-cache.py cargo test --release --lib sx1_real_history_search_reading -- --ignored --nocapture`

use serde_json::{json, Value};

/// 与 LOC1b §3.1 同一组查询词（最后一个无命中）；`limit` 同前端那一问。
const QUERIES: &[&str] = &["the", "error", "cargo test", "设计", "zq-sx1-无此词-7731"];
const LIMIT: u64 = 300;

fn ask(phase: &str, round: usize, q: &str) {
    let t = std::time::Instant::now();
    let v = crate::faces::read_face::answer(
        "history-search",
        &json!({ "query": q, "limit": LIMIT }),
        &Default::default(),
    )
    .unwrap_or_else(|(c, m)| panic!("history-search 答错了（{c}）：{m}"));
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let lines: Vec<&str> = v["lines"]
        .as_array()
        .expect("应答不是按行那一形 —— 秤接错了口")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let (mut hits, mut snippets) = (0u64, 0u64);
    for l in &lines {
        let row: Value = serde_json::from_str(l).expect("每行一个 JSON 对象");
        hits += row["hitCount"].as_u64().unwrap_or(0);
        snippets += row["hits"].as_array().map_or(0, |a| a.len() as u64);
    }
    let d = ring::digest::digest(&ring::digest::SHA256, lines.join("\n").as_bytes());
    let digest: String = d.as_ref()[..8].iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "SX1 phase={phase} round={round} q={q:?} ms={ms:.1} sessions={} hits={hits} snippets={snippets} digest={digest}",
        lines.len()
    );
}

#[test]
#[ignore = "SX1 读数：真规模本机历史（只读）只量耗时与条数；跑法住本文件头注"]
fn sx1_real_history_search_reading() {
    let script = std::env::var("SX1_EVICT").expect("要 `SX1_EVICT` 指向 SX1-evict-page-cache.py");
    let root = crate::agents::claudecode::paths::projects_root(
        &crate::observe::history_query::agent_home(),
    );
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("SX1 build={} load={}", crate::BUILD_ID, load.trim());
    // 冷首趟：先把人群文件丢出页缓存（只读，posix_fadvise）。
    let out = std::process::Command::new("python3")
        .arg(&script)
        .arg(&root)
        .output()
        .expect("起不来丢页缓存那份脚本");
    assert!(out.status.success(), "丢页缓存失败");
    print!("{}", String::from_utf8_lossy(&out.stdout));
    ask("cold-first", 0, QUERIES[0]);
    // 热态：五个词各三趟。
    for round in 1..=3 {
        for q in QUERIES {
            ask("warm", round, q);
        }
    }
}
