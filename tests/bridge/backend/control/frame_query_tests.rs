//! 〔`C1` · 2026-09-24〕只读查询走长连接的判据（monitor 侧）。

use super::*;

/// 题面那八条 —— `设计/15 §3.2` 那一串逐字（`--accounts` 在盘上叫 `--list-accounts`）。
/// **异源**：这张表抄自设计篇，不从 [`MOVED`] 派生。
const DESIGN_EIGHT: &[&str] = &[
    "--list-projects",
    "--list-sessions",
    "--read-session",
    "--read-session-tail",
    "--session-accounts",
    "--list-accounts",
    "--search",
    "--list-subagents",
];

fn sorted(v: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = v.into_iter().collect();
    v.sort();
    v
}

/// 后端 `inbound.rs` 生产段里把活交给 `read_face::answer` 的帧命令名（**从后端源码数**）。
fn backend_read_face_commands() -> Vec<String> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../backend/inbound.rs");
    let src = std::fs::read_to_string(&p).expect("读后端 inbound.rs");
    let prod = guard_core::production_code(&src);
    let got: Vec<String> = prod
        .split("CommandSpec {")
        .skip(1)
        .filter(|blk| blk.contains("read_face::answer"))
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some(blk[at..].split('"').next()?.to_string())
        })
        .collect();
    assert!(!got.is_empty(), "从后端源码一条都没数到 —— 抽取坏了");
    sorted(got)
}

/// ★ 两向相等：[`MOVED`] 的左列 == 设计篇那八条；右列 == 后端真登记上帧面、交给只读宿主的那八条。
#[test]
fn the_moved_table_matches_the_design_list_and_the_backend_registry() {
    assert_eq!(
        sorted(MOVED.iter().map(|(f, _)| f.to_string())),
        sorted(DESIGN_EIGHT.iter().map(|s| s.to_string())),
        "搬上帧面的子命令与题面那八条不相等"
    );
    assert_eq!(
        sorted(MOVED.iter().map(|(_, c)| c.to_string())),
        backend_read_face_commands(),
        "monitor 这边以为搬上去的帧命令，与后端真登记的对不上"
    );
}

/// ★ 拨号那条路只放行 [`STILL_DIALED`]：八条里**一条都过不去**（零命中），登记的那几条过得去（正控）。
#[test]
fn the_dial_path_refuses_every_moved_query() {
    let leaked: Vec<&str> = MOVED
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| dial_allowed(f))
        .collect();
    assert!(
        leaked.is_empty(),
        "这几条已上帧面，拨号那条路却还放行：{leaked:?}"
    );
    for (f, why) in STILL_DIALED {
        assert!(dial_allowed(f), "`{f}` 登记为仍拨号，却被拒了");
        assert!(!why.trim().is_empty(), "`{f}` 没写为什么还在拨");
        assert!(!MOVED.iter().any(|(m, _)| m == f), "`{f}` 同时在两张表里");
    }
    assert!(!dial_allowed("--fork-session"), "没登记的子命令也不许拨");
}

/// ★ 拨号那条路**真的**先问了 [`dial_allowed`]：`run_list_query` 生产段里有这一问，
/// 且在它的 `connect_and_exec_cmd(` 之前（锚串恰好一处）。
#[test]
fn run_list_query_asks_before_it_dials() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/remote_history.rs");
    let prod = guard_core::production_code(&std::fs::read_to_string(p).unwrap());
    let at = prod
        .find("async fn run_list_query(")
        .expect("找不到 run_list_query");
    let body_end = prod[at + 1..]
        .find("\nasync fn ")
        .map_or(prod.len(), |k| at + 1 + k);
    let body = &prod[at..body_end];
    // 恰好一处、两侧有边界（`find_pinned`）—— 裸 `find`/`matches` 在 needle 被撑大时照样绿。
    let ask = guard_core::find_pinned(body, "dial_allowed(")
        .unwrap_or_else(|e| panic!("run_list_query 没（恰好一次地）问 dial_allowed：{e}"));
    let dial = body
        .find("connect_and_exec_cmd(")
        .expect("run_list_query 里没有拨号 —— 本条的前提变了");
    assert!(ask < dial, "先拨号后问，问了等于没问");
}

/// ★ argv 分流认得本仓今天真在发的形状：区间取正文走帧面；骨架索引落到拨号（且拨号放行它）。
#[test]
fn argv_routing_covers_the_shapes_the_repo_actually_sends() {
    let range = crate::session_skeleton::range_argv("/p/s.jsonl", 10, 99);
    let range: Vec<&str> = range.iter().map(String::as_str).collect();
    match route_argv(&range) {
        Some(ArgvRoute::Read { path, from, until }) => {
            assert_eq!((path.as_str(), from, until), ("/p/s.jsonl", 10, Some(99)))
        }
        _ => panic!("按区间取正文那一形没走帧面"),
    }
    let index = crate::session_skeleton::index_argv("/p/s.jsonl", 0);
    let index: Vec<&str> = index.iter().map(String::as_str).collect();
    assert!(
        route_argv(&index).is_none(),
        "索引那一形今天帧面没有对应命令"
    );
    assert!(dial_allowed(index[0]), "索引那一形落到拨号，拨号却不放行它");
    assert!(matches!(
        route_argv(&["--list-subagents", "/p/s.jsonl"]),
        Some(ArgvRoute::Lines("history-subagents", _))
    ));
    assert!(matches!(
        route_argv(&["--read-session", "/p/a.jsonl"]),
        Some(ArgvRoute::Read {
            from: 0,
            until: None,
            ..
        })
    ));
}
