//! Claude 的 MCP 布局读法：三段条目 ＋ 项目表 ＋ 坏的那份说出来。
use super::*;

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sh1-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

/// 缺 ⇒ 那一段空、不出声；坏 ⇒ 那一段空并说出来。
#[test]
fn a_missing_file_is_silent_and_a_broken_one_is_said() {
    let root = fixture("broken");
    let cj = root.join(".claude.json");
    std::fs::write(&cj, "{ not json").unwrap();
    let got = read_at(
        Some(&cj),
        Some(&root.join("absent")),
        &crate::agents::McpLook::default(),
    );
    let _ = std::fs::remove_dir_all(&root);
    assert!(got.entries.is_empty() && got.dirs.is_empty());
    assert_eq!(
        got.problems.len(),
        1,
        "坏的 .claude.json 要说一句、缺的 .mcp.json 不说：{:?}",
        got.problems
    );
}

/// 状态只说配置层说得准的：停用（`projects[目录].disabledMcpServers`）· 要登录（各号家目录里那份「要登录」缓存、
/// 还在 Claude 自己的有效期内，缺省 15 分钟、条目带 `ttlMs` 就按它；只对 http / sse 算数，stdio 那种 Claude 自己也不看）；
/// 过期的、stdio 的、没记的 ⇒ `unknown`，不说「连上了」。
/// 要求：「mcp-read 带上每个服务器的 status，配置层能说真话的那一份，其余都写 unknown，不说 connected」。
#[test]
fn status_says_only_what_the_config_side_can_know() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("status");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string();
    let cj = root.join(".claude.json");
    let cfg = serde_json::json!({
        "mcpServers": {
            "h1": {"type": "http", "url": "http://h1"},
            "h2": {"type": "sse", "url": "http://h2"},
            "s1": {"command": "s1"},
        },
        "projects": { d.clone(): {
            "mcpServers": {"l1": {"type": "http", "url": "http://l1"}},
            "disabledMcpServers": ["h2", "p1"],
        }},
    });
    std::fs::write(&cj, cfg.to_string()).unwrap();
    std::fs::write(
        dir.join(".mcp.json"),
        r#"{"mcpServers":{"p1":{"type":"http","url":"http://p1"}}}"#,
    )
    .unwrap();
    let now: u64 = 1_800_000_000_000;
    let min = 60_000;
    let home = |tag: &str, cache: serde_json::Value| {
        let h = root.join(tag);
        std::fs::create_dir_all(&h).unwrap();
        std::fs::write(h.join("mcp-needs-auth-cache.json"), cache.to_string()).unwrap();
        h
    };
    let a = home(
        "a",
        serde_json::json!({
            "h1": {"timestamp": now - min},
            "s1": {"timestamp": now - min},
        }),
    );
    let b = home(
        "b",
        serde_json::json!({
            "h1": {"timestamp": now - 20 * min},
            "l1": {"timestamp": now - 30 * min, "ttlMs": 60 * min},
            "h2": {"timestamp": now - min},
        }),
    );
    let bare = home(
        "bare",
        serde_json::json!({ "h1": {"timestamp": now - 2 * min} }),
    );
    let look = McpLook {
        homes: vec![(Some("b".into()), b), (Some("a".into()), a), (None, bare)],
        now_ms: now,
    };
    let got = read_at(Some(&cj), Some(&dir), &look);
    let _ = std::fs::remove_dir_all(&root);
    let mut seen: Vec<(String, McpStatus, Vec<String>, Option<u64>)> = got
        .entries
        .iter()
        .map(|e| (e.name.clone(), e.status, e.login_in.clone(), e.seen_ms))
        .collect();
    seen.sort_by(|x, y| x.0.cmp(&y.0));
    let s = |n: &str, st, who: &[&str], at: Option<u64>| {
        (
            n.to_string(),
            st,
            who.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
            at,
        )
    };
    assert_eq!(
        seen,
        vec![
            // 两个号里有记：a 的新鲜、b 的过期 ⇒ 只算 a；没设账号的那一份也新鲜，算数但不出名字；时刻取最近一次。
            s("h1", McpStatus::NeedsLogin, &["a"], Some(now - min)),
            // 停用压过缓存（Claude 自己也是先看停用再看缓存）。
            s("h2", McpStatus::Disabled, &[], None),
            // 带 ttlMs 的按它算：30 分钟前记的、有效期 1 小时 ⇒ 还算。
            s("l1", McpStatus::NeedsLogin, &["b"], Some(now - 30 * min)),
            s("p1", McpStatus::Disabled, &[], None),
            // stdio 的记了也不算（Claude 不拿这份缓存跳过 stdio）。
            s("s1", McpStatus::Unknown, &[], None),
        ]
    );
    assert!(got.problems.is_empty(), "{:?}", got.problems);
}

/// 缓存坏了 ⇒ 说一句，状态照旧判不出（不当成「没有要登录的」）。
#[test]
fn a_broken_login_cache_is_said() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("badcache");
    let cj = root.join(".claude.json");
    std::fs::write(
        &cj,
        r#"{"mcpServers":{"h1":{"type":"http","url":"http://h1"}}}"#,
    )
    .unwrap();
    let h = root.join("a");
    std::fs::create_dir_all(&h).unwrap();
    std::fs::write(h.join("mcp-needs-auth-cache.json"), "{ nope").unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), h)],
        now_ms: 1,
    };
    let got = read_at(Some(&cj), None, &look);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(got.entries[0].status, McpStatus::Unknown);
    assert_eq!(got.problems.len(), 1, "{:?}", got.problems);
}

/// 会话记录里那一家说的 MCP 状态（「延后加载的工具变了」附件）：三张表逐格翻成中立的说法；这一条没写的那一格 ⇒ `None`（沿用上一条）；
/// 认不出的 `…McpServers` 表 ⇒ 记漂移账（Claude Code 多了一种状态）；别的记录 ⇒ `None`。
#[test]
fn the_records_mcp_lists_are_read_and_an_unknown_one_is_drift() {
    use crate::agents::claudecode::drift::{snapshot, DriftFace};
    let rec = |a: serde_json::Value| serde_json::json!({"type": "attachment", "attachment": a});
    let got = said_of(&rec(serde_json::json!({
        "type": "deferred_tools_delta",
        "pendingMcpServers": ["m-p"],
        "failedMcpServers": [{"name": "m-f", "error": "e-1"}, {"name": "m-g"}],
        "c2NewMcpServers": ["m-x"],
    })))
    .expect("那一条认得");
    assert_eq!(got.pending, Some(vec!["m-p".to_string()]));
    assert_eq!(got.needs_login, None, "没写 ⇒ 沿用上一条");
    assert_eq!(
        got.failed,
        Some(vec![
            ("m-f".to_string(), Some("e-1".to_string())),
            ("m-g".to_string(), None)
        ])
    );
    assert!(snapshot()
        .iter()
        .any(|f| f.face == DriftFace::UnknownMcpList.word()
            && f.entries.iter().any(|e| e.key == "c2NewMcpServers")));
    assert_eq!(said_of(&rec(serde_json::json!({"type": "date"}))), None);
    assert_eq!(said_of(&serde_json::json!({"type": "user"})), None);
}
