//! # 要求住址：`INVARIANTS §18.1`（看不懂的行记在哪台的账上）＋ `INVARIANTS §28`（远端行带 origin）
//!
//! 核原文：`INVARIANTS §18.1` 逐字「账本第一层键是 origin，每台机器只看得到自己那一份」—— `lib.rs::batch_to_payloads`
//! 把看不懂的行记在这一批的 origin 名下（V104「漂移记账按机器分开」）；`INVARIANTS §28` 逐字「上 wire（每条远端行带 `origin`）」
//! —— 远端行带那台的名字。连 JSON 都不成立的行跳过、不 panic，对 `§18.1` 那条的抢救口径。〔JA1 点址 2026-09-24〕

use super::*;
use std::path::PathBuf;

fn jline(session_id: &str, seq: u64, raw: &str) -> ssh_source::JsonlLine {
    ssh_source::JsonlLine {
        session_id: session_id.to_string(),
        path: PathBuf::from("/tmp/projects/proj/s-abc.jsonl"),
        seq,
        raw: raw.to_string(),
    }
}

/// batch_to_payloads 必须：只保留 displayable 行、透传 seq/session_id、
/// 在遇到 malformed 行时 warn-then-continue 不 panic，并对非 displayable 行静默丢弃。
#[test]
fn filters_non_displayable_and_survives_malformed() {
    // (a) 真实 displayable user 行（copy 自 messages.rs golden sample），seq 5
    let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/me/proj"
        }"#;
    // (b) 非 displayable 记录：permission-mode 的 is_displayable() 返回 false
    let non_displayable = r#"{"type":"permission-mode"}"#;
    // (c) 无法解析的行 → parse_line 返回 Err，必须被 warn 后跳过、不 panic
    let malformed = "not json at all";

    let lines = vec![
        jline("s-abc", 5, displayable_user),
        jline("s-abc", 6, non_displayable),
        jline("s-abc", 7, malformed),
    ];

    let payloads = batch_to_payloads(
        lines,
        &crate::origin::Origin::local(),
        &mut SkipRuns::default(),
    );

    // 只有 displayable user 行进 payload
    assert_eq!(payloads.len(), 1, "只应保留 1 条 displayable 记录");
    assert_eq!(payloads[0].seq, 5, "seq 必须原样透传");
    assert_eq!(payloads[0].session_id, "s-abc", "session_id 必须原样透传");
    assert_eq!(
        payloads[0].cwd.as_deref(),
        Some("/home/me/proj"),
        "extract_cwd 应取出 user.cwd"
    );
    // 本机那台的行载荷上不带 origin（线上形状与原来 `None` 那一档逐字相同）。
    assert_eq!(payloads[0].origin, None, "本地行 origin 应为 None");
}

/// 远端那台的 origin ⇒ 每条 payload 都带上它的名字（远端 Tab 标题前缀用）。
#[test]
fn origin_is_propagated_to_payloads() {
    let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/pi/proj"
        }"#;
    let lines = vec![jline("s-remote", 0, displayable_user)];
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

/// 〔ST3〕★ 接缝：批里看不懂的行记在**这一批的 origin** 名下，不在本机名下（两向）。
///
/// 本机 watcher 与 `ssh_source·rs::flush_lines` 都经这一个口；原先它收 `Option<String>`，
/// 记账那一刻不知道是哪台 ⇒ 远端的行全记进了一本不分机器的账。
#[test]
fn unreadable_lines_are_booked_under_the_batch_origin() {
    use crate::drift_ledger::{snapshot, DriftFace};
    let find = |o: &crate::origin::Origin, key: &str| {
        snapshot(o)
            .into_iter()
            .find(|f| f.face == DriftFace::UnknownRecordType)
            .and_then(|f| f.entries.into_iter().find(|e| e.key == key))
    };
    let remote = crate::origin::Origin("st3-batch-probe".to_string());
    let local = crate::origin::Origin::local();
    let _ = batch_to_payloads(
        vec![jline("s-r", 0, r#"{"type":"st3-batch-remote-probe"}"#)],
        &remote,
        &mut SkipRuns::default(),
    );
    let _ = batch_to_payloads(
        vec![jline("s-l", 0, r#"{"type":"st3-batch-local-probe"}"#)],
        &local,
        &mut SkipRuns::default(),
    );
    assert!(
        find(&remote, "st3-batch-remote-probe").is_some(),
        "远端那批没记在那台名下"
    );
    assert!(
        find(&local, "st3-batch-local-probe").is_some(),
        "本机那批没记在本机名下"
    );
    assert!(
        find(&local, "st3-batch-remote-probe").is_none(),
        "远端那批记进了本机那一本"
    );
    assert!(
        find(&remote, "st3-batch-local-probe").is_none(),
        "本机那批记进了远端那一本"
    );
}

/// 〔RENDER2 · `设计/10 §3.2` 逐字「要封顶得有后端的保证 ……」· `真相源/130 §3`「不可显示的行照占 seq 却不发 payload，
/// 每一处都在集合里留一个洞」〕每条 payload 的 `skipped_from` == 它之前**连着见过**的不可显示那一段的起点（跨批照认；
/// 行号断过 / 别的会话不混）。期望表手写（异源）。
#[test]
fn every_payload_says_the_undisplayable_run_right_before_it() {
    let user = |u: &str| {
        format!(r#"{{"type":"user","uuid":"{u}","message":{{"role":"user","content":"x"}}}}"#)
    };
    let meta = r#"{"type":"permission-mode"}"#;
    let mut runs = SkipRuns::default();
    let origin = crate::origin::Origin::local();
    let first = batch_to_payloads(
        vec![
            jline("s", 0, meta),
            jline("s", 1, &user("a")), // 前面 [0,1) 连着见过
            jline("s", 2, meta),
            jline("s", 3, "not json"), // 解析不出也照占号
        ],
        &origin,
        &mut runs,
    );
    let second = batch_to_payloads(
        vec![
            jline("t", 7, meta),
            jline("s", 4, &user("b")), // 跨批：[2,4)
            jline("s", 6, &user("c")), // 5 没见过 ⇒ 不认
            jline("s", 7, meta),
            jline("s", 8, &user("d")), // [7,8)
            jline("t", 8, &user("e")), // 别的会话自己那一段 [7,8)
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
