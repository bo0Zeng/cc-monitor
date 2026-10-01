//! # 要求住址：`INVARIANTS §28`（远端行带 origin）＋（记录解释只住后端）
//!
//! 核原文：`INVARIANTS §28` 逐字「上 wire（每条远端行带 `origin`）」—— 远端行带那台的名字；
//! 「前端不许从流上攒全会话事实」连同记录解释搬进后端之后：进不进界面、`cwd` 都是后端给的成品，
//! `lib.rs::batch_to_payloads` 只组载荷、原样转交（没有成品的行照占号、不出 payload）。〔JA1 点址 2026-09-24 · MOD 改写 09-28〕

use super::*;
use std::path::PathBuf;

/// 一行（后端给的成品：`message` 缺 ＝ 不进界面）。
fn jline(
    session_id: &str,
    seq: u64,
    message: Option<&str>,
    cwd: Option<&str>,
) -> ssh_source::JsonlLine {
    ssh_source::JsonlLine {
        session_id: session_id.to_string(),
        path: PathBuf::from("/tmp/projects/proj/s-abc.jsonl"),
        seq,
        message: message.map(|m| {
            crate::ui_contract::RecordBody::from_json(m.to_string()).expect("成品是 JSON")
        }),
        cwd: cwd.map(str::to_string),
        end: None,
        rid: None,
    }
}

/// batch_to_payloads：有成品的才出 payload、成品与 `cwd` 原样转交、seq/session_id 透传；没成品的照占号不出。
#[test]
fn only_lines_with_a_product_become_payloads_and_the_product_passes_through() {
    let user =
        r#"{"type":"user","uuid":"u-1","timestamp":"t","message":{"role":"user","content":"hi"}}"#;
    let lines = vec![
        jline("s-abc", 5, Some(user), Some("/home/me/proj")),
        jline("s-abc", 6, None, None),
    ];
    let payloads = batch_to_payloads(
        lines,
        &crate::origin::Origin::local(),
        &mut SkipRuns::default(),
    );
    assert_eq!(payloads.len(), 1, "只应保留有成品的那 1 条");
    assert_eq!(payloads[0].seq, 5, "seq 必须原样透传");
    assert_eq!(payloads[0].session_id, "s-abc", "session_id 必须原样透传");
    assert_eq!(
        payloads[0].cwd.as_deref(),
        Some("/home/me/proj"),
        "cwd 是后端给的，原样带"
    );
    assert_eq!(
        payloads[0].message.0.get(),
        user,
        "成品原样转交（一个字节都不改）"
    );
    // 本机那台的行载荷上不带 origin（线上形状与原来 `None` 那一档逐字相同）。
    assert_eq!(payloads[0].origin, None, "本地行 origin 应为 None");
    let wire = serde_json::to_string(&payloads[0]).unwrap();
    assert!(
        wire.contains(&format!("\"message\":{user}")),
        "序列化时成品原样嵌进去：{wire}"
    );
}

/// 远端那台的 origin ⇒ 每条 payload 都带上它的名字（远端 Tab 标题前缀用）。
#[test]
fn origin_is_propagated_to_payloads() {
    let lines = vec![jline("s-remote", 0, Some(r#"{"type":"user"}"#), None)];
    let payloads = batch_to_payloads(
        lines,
        &crate::origin::Origin("pi".to_string()),
        &mut SkipRuns::default(),
    );
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        payloads[0].origin.as_deref(),
        Some("pi"),
        "远端行 origin 必须透传 host 标签"
    );
}

/// 〔RENDER2 · 「要封顶得有后端的保证 ……」· 「不可显示的行照占 seq 却不发 payload，
/// 每一处都在集合里留一个洞」〕每条 payload 的 `skipped_from` == 它之前**连着见过**的不可显示那一段的起点（跨批照认；
/// 行号断过 / 别的会话不混）。期望表手写（异源）。
#[test]
fn every_payload_says_the_undisplayable_run_right_before_it() {
    let user = Some(r#"{"type":"user"}"#);
    let mut runs = SkipRuns::default();
    let origin = crate::origin::Origin::local();
    let first = batch_to_payloads(
        vec![
            jline("s", 0, None, None),
            jline("s", 1, user, None), // 前面 [0,1) 连着见过
            jline("s", 2, None, None),
            jline("s", 3, None, None), // 解析不出也照占号（后端同样不给成品）
        ],
        &origin,
        &mut runs,
    );
    let second = batch_to_payloads(
        vec![
            jline("t", 7, None, None),
            jline("s", 4, user, None), // 跨批：[2,4)
            jline("s", 6, user, None), // 5 没见过 ⇒ 不认
            jline("s", 7, None, None),
            jline("s", 8, user, None), // [7,8)
            jline("t", 8, user, None), // 别的会话自己那一段 [7,8)
        ],
        &origin,
        &mut runs,
    );
    let got: Vec<(u64, Option<u64>)> = first
        .iter()
        .chain(&second)
        .map(|p| (p.seq, p.skipped_from))
        .collect();
    assert_eq!(
        got,
        vec![
            (1, Some(0)),
            (4, Some(2)),
            (6, None),
            (8, Some(7)),
            (8, Some(7))
        ]
    );
}
