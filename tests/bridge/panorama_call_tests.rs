//! 〔RM1c · 第四波〕`panorama_call.rs` 的判据。

use super::*;

/// 后端适配层里一个 `const NAME: u64 = <数>;` 的值（运行时读，异源）。
fn backend_const(src: &str, name: &str) -> u64 {
    let key = format!("const {name}: u64 = ");
    let at = src
        .find(&key)
        .unwrap_or_else(|| panic!("后端适配层里找不到 `{key}` —— 改了写法，本条跟着改"));
    let rest = &src[at + key.len()..];
    rest[..rest.find(';').expect("常量没收尾")]
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("`{name}` 不是一个数：{e}"))
}

/// ★ 两档期限**长于**后端给子进程的期限（加上那一次探测），而且「哪几个 op 是建索引那一档」两边相等。
///
/// 异源：一侧是本文件的常量，另一侧运行时读 `src/backend/control/panorama.rs`（不走 `include_str!`：
/// 那会给 `cross_half_edge_registry` 添一条跨半边 —— 同 `panorama_locus_guard` 的取舍）。
#[test]
fn the_budgets_outlast_the_backend_deadlines() {
    let p = crate::guard_support::repo_src_root().join("backend/control/panorama.rs");
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    assert!(src.len() > 3_000, "{p:?} 只有 {} 字节", src.len());
    let prod = guard_core::production_code(&src);
    let probe = backend_const(&prod, "PROBE_DEADLINE_SECS");
    let build = backend_const(&prod, "BUILD_DEADLINE_SECS");
    let query = backend_const(&prod, "QUERY_DEADLINE_SECS");
    assert!(
        budget_for("index").as_secs() > probe + build,
        "建索引那一档：这边等 {} s，后端探测 {probe} s ＋ 子进程 {build} s —— 这边会先超时，\
         人听到的是「没回来」而不是后端那句 `timed_out`",
        budget_for("index").as_secs()
    );
    assert!(
        budget_for("overview").as_secs() > probe + query,
        "查询那一档：这边等 {} s，后端探测 {probe} s ＋ 子进程 {query} s",
        budget_for("overview").as_secs()
    );
    // 建索引那一档的 op：后端 OPS 表里期限写成 `BUILD_DEADLINE_SECS` 的那几行。
    let at = prod
        .find("const OPS: &[(&str, u64)] = &[")
        .expect("后端 op 表改了写法");
    let body = &prod[at..at + prod[at..].find("];").unwrap()];
    let mut theirs: Vec<&str> = body
        .lines()
        .filter(|l| l.contains("BUILD_DEADLINE_SECS"))
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("(\"")?;
            Some(&rest[..rest.find('"')?])
        })
        .collect();
    theirs.sort();
    let mut ours = BUILD_OPS.to_vec();
    ours.sort();
    assert!(
        !theirs.is_empty(),
        "后端那一侧一个建索引的 op 都没抽到 —— 抽取坏了"
    );
    assert_eq!(ours, theirs, "「哪几个 op 是建索引那一档」两边对不上");
}

#[test]
fn the_frame_payload_carries_only_what_was_given() {
    assert_eq!(
        frame_args("diagram_kinds", None, None),
        json!({"op": "diagram_kinds"})
    );
    assert_eq!(
        frame_args("node", Some("/r"), Some(json!({"symbol": "a#f"}))),
        json!({"op": "node", "repo": "/r", "args": {"symbol": "a#f"}})
    );
}
