//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「功能侧只读查询」节（任务列表 · 插件市场）
//!
//! 核原文：该节逐字「宿主是 `feature_face`（不是 `read_face`，理由在它头注），本体在 `observe/`」·「应答一律**按行**」；
//! `plugins-marketplaces` 条逐字「文件不在 ⇒ `file_absent: true`（诚实的空）；读 / 解析失败 ⇒ `failed`」；`tasks-list` 条 `sid` 行逐字
//! 「只许一段普通路径名（空 / 含分隔符 / `.` / `..` ⇒ `bad_args`）」—— 本族三条一一对上。〔JA1 点址 2026-09-24〕
//!
//! 功能侧帧面宿主的判据。夹具只造结构，不采任何真会话正文。

use super::*;
use serde_json::json;

/// `tasks-list` 照分派那一层解入参（`typed::take`）再交本体；回包投成线上那一份，失败只留码。
fn tasks(home: &std::path::Path, args: &Value) -> Result<Value, String> {
    use crate::wire::typed::Codes as _;
    let req = crate::wire::typed::take::<TasksListReq>(args).map_err(|f| f.code)?;
    let got = tasks_at(home, req).map_err(|e| e.into_fail().code)?;
    Ok(serde_json::to_value(got).expect("回包总能序列化"))
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("rm1b-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn tasks_list_answers_the_product_and_refuses_a_missing_sid() {
    let h = scratch("tasks");
    let dir = crate::observe::tasks_query::tasks_root(&h).join("s");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.json"),
        r#"{"id":"1","subject":"占位","status":"pending"}"#,
    )
    .unwrap();
    let v = tasks(&h, &json!({"sid": "s"})).unwrap();
    // 应答是成品 `{tasks}`，不再是原样对象的 `lines`。
    let rows = v["tasks"].as_array().expect("tasks");
    assert_eq!(rows.len(), 1);
    assert!(v.get("lines").is_none(), "还在回原始行：{v}");
    // 反向：缺 sid ⇒ bad_args；sid 走出任务根 ⇒ bad_args（围栏在本体里，这里只证它透得上来）。
    assert_eq!(tasks(&h, &json!({})).unwrap_err(), "bad_args");
    assert_eq!(tasks(&h, &json!({"sid": "../s"})).unwrap_err(), "bad_args");
    // 多一格 ⇒ bad_args（入参是类型：`deny_unknown_fields`）。
    assert_eq!(
        tasks(&h, &json!({"sid": "s", "x": 1})).unwrap_err(),
        "bad_args"
    );
}

/// ★ `tasks-list` 的成品 == 跨语言金样（`tests/__fixtures__/tasks-list.golden.json`）。
///
/// 要求：「正路是把解释挪进后端、直接出成品……线上形状由一份跨语言金样钉住（后端测试产出 == 金样 ·
/// TS 解码器读同一份）」。异源：金样里的 `files` 由**生产**路径（`answer_at` → `session_tasks` → `task_entry`）现算，
/// 与手写的 `product` 逐格相等；界面那一侧（`tests/frontend/ui/tasks-decode.vitest.ts`）读同一份。
/// 生产路径现在是 `tasks_at` → `session_tasks` → `task_entry`（typed）。
/// 金样的 `files` 覆盖了旧口径（serde `TaskEntry`）的每一档：多余键不带 · BOM · `null` 可选格不出现 · id 不是串 / blocks 是 null /
/// 缺必填 / 半截 JSON / 不是数字名 ⇒ 都不算任务。
#[test]
fn the_tasks_product_matches_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tasks-list.golden.json")).unwrap();
    let h = scratch("tasks-golden");
    let sid = g["request"]["sid"].as_str().unwrap();
    let dir = crate::observe::tasks_query::tasks_root(&h).join(sid);
    std::fs::create_dir_all(&dir).unwrap();
    for f in g["files"].as_array().unwrap() {
        std::fs::write(
            dir.join(f["name"].as_str().unwrap()),
            f["body"].as_str().unwrap(),
        )
        .unwrap();
    }
    let got = tasks(&h, &g["request"]).unwrap();
    assert_eq!(got, g["product"], "成品与金样对不上");
    // 线上字节：帧里的回包先过 `to_value`（键排好序）再写出 ⇒ 与金样那一份重新序列化后逐字节相等。
    assert_eq!(
        serde_json::to_string(&got).unwrap(),
        serde_json::to_string(&g["product"]).unwrap()
    );
}

/// ★ `mcp-read` 每条写好的字（扩展页小标与抽屉那一行照抄）：需登录那一句（号名 · 几点记下）· 断开那一句（这台活会话里最近说它的那一条：
/// 会话名 · 几点 · 原话）· 小标取哪一种（断开压过需登录；停用的不连 ⇒ 什么都不标）。成品 == 金样 `marked`（界面读同一份）。
/// 要求：MCP 状态那一块「判定还是 mcp-read 一处，每条再加两格写好的字」。
#[test]
fn the_mcp_marks_are_written_once_on_the_backend() {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    use crate::observe::accounts_query::LiveMcpFailed;
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/mcp-read.golden.json")).unwrap();
    let ms = |s: &str| crate::common::time::parse_iso8601_ms(s).unwrap();
    let http = json!({"type": "http", "url": "http://x"});
    let e = |name: &str, server: &Value, status, login_in: &[&str], seen: Option<u64>| McpEntry {
        scope: "user",
        name: name.into(),
        server: server.clone(),
        source: "<CJ>".into(),
        status,
        login_in: login_in.iter().map(|s| s.to_string()).collect(),
        seen_ms: seen,
    };
    let seen = Some(ms("2026-10-09T10:42:00Z") as u64);
    let read = McpRead {
        entries: vec![
            e(
                "s-login",
                &http,
                McpStatus::NeedsLogin,
                &["acct-a", "acct-b"],
                seen,
            ),
            e("s-both", &http, McpStatus::NeedsLogin, &[], seen),
            e(
                "s-failed",
                &json!({"command": "s-failed"}),
                McpStatus::Unknown,
                &[],
                None,
            ),
            e("s-off", &http, McpStatus::Disabled, &[], None),
            e(
                "s-plain",
                &json!({"command": "s-plain"}),
                McpStatus::Unknown,
                &[],
                None,
            ),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let f = |name: &str, at: Option<i64>, detail: Option<&str>, title: &str| LiveMcpFailed {
        name: name.into(),
        at_ms: at,
        detail: detail.map(str::to_string),
        title: title.into(),
    };
    let failed = vec![
        f(
            "s-both",
            Some(ms("2026-10-09T10:31:00Z")),
            Some("err-1"),
            "会话-甲",
        ),
        f("s-failed", None, None, "会话-乙"),
        f(
            "s-off",
            Some(ms("2026-10-09T10:00:00Z")),
            Some("err-2"),
            "会话-丙",
        ),
    ];
    let say = McpSay {
        failed: &failed,
        now_s: ms("2026-10-09T12:00:00Z") / 1000,
        tz: Default::default(),
        login_command: "/mcp",
    };
    assert_eq!(
        serde_json::to_value(mcp_reply(&read, &say)).unwrap(),
        g["marked"]
    );
}
