//! Claude 的 MCP 布局读法：三段条目 ＋ 项目表 ＋ 坏的那份说出来。
use super::*;
use crate::agents::drift::DriftFaceWord;

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
    let cfg = serde_json::json!({
        "mcpServers": {
            "h1": {"type": "http", "url": "http://h1"},
            "h2": {"type": "sse", "url": "http://h2"},
            "h3": {"type": "http", "url": "http://h3"},
            "s1": {"command": "s1"},
        },
        "projects": { d.clone(): {
            "mcpServers": {"l1": {"type": "http", "url": "http://l1"}},
            "disabledMcpServers": ["h2", "p1"],
        }},
    });
    std::fs::write(
        dir.join(".mcp.json"),
        r#"{"mcpServers":{"p1":{"type":"http","url":"http://p1"},"p2":{"type":"http","url":"http://p2"}}}"#,
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
    // 配置只在 a 的家里；b · 没设账号那一份的缓存里记着同名的，不算到 a 那几条上（项目级除外：各号都用它）。
    let a = home(
        "a",
        serde_json::json!({
            "h1": {"timestamp": now - min},
            "h3": {"timestamp": now - 20 * min},
            "s1": {"timestamp": now - min},
            "l1": {"timestamp": now - 30 * min, "ttlMs": 60 * min},
            "h2": {"timestamp": now - min},
        }),
    );
    std::fs::write(a.join(".claude.json"), cfg.to_string()).unwrap();
    let b = home(
        "b",
        serde_json::json!({
            "h3": {"timestamp": now - min},
            "p2": {"timestamp": now - 2 * min},
        }),
    );
    let bare = home(
        "bare",
        serde_json::json!({ "h3": {"timestamp": now - 2 * min} }),
    );
    let look = McpLook {
        homes: vec![
            (Some("b".into()), b),
            (Some("a".into()), a),
            (None, bare.clone()),
        ],
        bare: Some(bare),
        now_ms: now,
    };
    let got = read_at(None, Some(&dir), &look);
    let _ = std::fs::remove_dir_all(&root);
    let mut seen: Vec<(String, McpStatus, Vec<Option<String>>, Option<u64>)> = got
        .entries
        .iter()
        .map(|e| (e.name.clone(), e.status, e.login_in.clone(), e.seen_ms))
        .collect();
    seen.sort_by(|x, y| x.0.cmp(&y.0));
    let s = |n: &str, st, who: &[&str], at: Option<u64>| {
        (
            n.to_string(),
            st,
            who.iter().map(|w| Some(w.to_string())).collect::<Vec<_>>(),
            at,
        )
    };
    assert_eq!(
        seen,
        vec![
            s("h1", McpStatus::NeedsLogin, &["a"], Some(now - min)),
            // 停用压过缓存（Claude 自己也是先看停用再看缓存）。
            s("h2", McpStatus::Disabled, &[], None),
            // a 那一笔过期了；b · 没设账号那一份记着新鲜的，但这一条出自 a 的家 ⇒ 不算。
            s("h3", McpStatus::Unknown, &[], None),
            // 带 ttlMs 的按它算：30 分钟前记的、有效期 1 小时 ⇒ 还算。
            s("l1", McpStatus::NeedsLogin, &["a"], Some(now - 30 * min)),
            s("p1", McpStatus::Disabled, &[], None),
            // 项目级各号都用 ⇒ 哪个号的缓存记着都算。
            s("p2", McpStatus::NeedsLogin, &["b"], Some(now - 2 * min)),
            // stdio 的记了也不算（Claude 不拿这份缓存跳过 stdio）。
            s("s1", McpStatus::Unknown, &[], None),
        ]
    );
    assert!(got.problems.is_empty(), "{:?}", got.problems);
}

/// 逐号读：每个号的家各读一份 `<家>/.claude.json`（用户级 ＋ `projects[目录]` 那一段本地级）＋ 没设账号那一份；
/// 经核心 `mcp_held` ⇒ 两个号里同名同配置的一条带两个号，配置不同的两条都标不一致；要登录只算条目出自的那个号。
#[test]
fn each_account_home_is_read_and_one_server_in_two_homes_is_one_row() {
    use crate::agents::{mcp_held, McpLook, McpStatus};
    let root = fixture("perhome");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string();
    let now: u64 = 1_800_000_000_000;
    let home = |tag: &str, db: &str, cache: serde_json::Value| {
        let h = root.join(tag);
        std::fs::create_dir_all(&h).unwrap();
        let cfg = serde_json::json!({
            "mcpServers": {"notes": {"type": "http", "url": "http://n"}},
            "projects": { d.clone(): { "mcpServers": {"db": {"command": db}} } },
        });
        std::fs::write(h.join(".claude.json"), cfg.to_string()).unwrap();
        std::fs::write(h.join("mcp-needs-auth-cache.json"), cache.to_string()).unwrap();
        h
    };
    let a = home(
        "a",
        "db-a",
        serde_json::json!({ "notes": {"timestamp": now - 60_000} }),
    );
    let b = home("b", "db-b", serde_json::json!({}));
    let cj = root.join(".claude.json");
    std::fs::write(&cj, r#"{"mcpServers":{"bare-only":{"command":"x"}}}"#).unwrap();
    let look = McpLook {
        homes: vec![(Some("b".into()), b.clone()), (Some("a".into()), a.clone())],
        bare: None,
        now_ms: now,
    };
    let got = read_at(Some(&cj), Some(&dir), &look);
    let held = mcp_held(&got.entries, &look.homes);
    let _ = std::fs::remove_dir_all(&root);
    assert!(got.problems.is_empty(), "{:?}", got.problems);
    assert_eq!(
        got.dirs,
        vec![d.clone()],
        "两个号都记着同一个目录 ⇒ 只列一次"
    );
    let mut seen: Vec<(
        &str,
        String,
        Vec<String>,
        bool,
        McpStatus,
        Vec<Option<String>>,
    )> = held
        .iter()
        .map(|h| {
            (
                h.entry.scope,
                h.entry.name.clone(),
                h.accounts.clone(),
                h.mismatch,
                h.entry.status,
                h.entry.login_in.clone(),
            )
        })
        .collect();
    seen.sort_by(|x, y| (x.1.as_str(), &x.2).cmp(&(y.1.as_str(), &y.2)));
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let w = |xs: &[&str]| xs.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
    assert_eq!(
        seen,
        vec![
            (
                "user",
                "bare-only".into(),
                v(&[]),
                false,
                McpStatus::Unknown,
                w(&[])
            ),
            (
                "local",
                "db".into(),
                v(&["a"]),
                true,
                McpStatus::Unknown,
                w(&[])
            ),
            (
                "local",
                "db".into(),
                v(&["b"]),
                true,
                McpStatus::Unknown,
                w(&[])
            ),
            (
                "user",
                "notes".into(),
                v(&["a", "b"]),
                false,
                McpStatus::NeedsLogin,
                w(&["a"])
            ),
        ]
    );
}

/// 一个号的「要登录」缓存只算到出自那个号的条目上：b 的缓存记着 `notes`、b 的配置里却没有它 ⇒ a 那一条照旧说不准；
/// 没设账号那一份就是某个号的那一份（同一个文件）⇒ 只读一次、照那个号认。
#[test]
fn a_login_only_counts_in_the_home_the_entry_came_from() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("narrow");
    let now: u64 = 1_800_000_000_000;
    let a = root.join("a");
    let b = root.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(
        a.join(".claude.json"),
        r#"{"mcpServers":{"notes":{"type":"http","url":"http://n"}}}"#,
    )
    .unwrap();
    std::fs::write(
        b.join("mcp-needs-auth-cache.json"),
        serde_json::json!({ "notes": {"timestamp": now - 60_000} }).to_string(),
    )
    .unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), a.clone()), (Some("b".into()), b)],
        bare: None,
        now_ms: now,
    };
    let got = read_at(Some(&a.join(".claude.json")), None, &look);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(got.entries.len(), 1, "{:?}", got.entries);
    assert_eq!(got.entries[0].status, McpStatus::Unknown);
    assert!(got.entries[0].login_in.is_empty());
}

/// 没设账号那一份只看没设账号时那个家的缓存：某个号的缓存记着同名的 ⇒ 仍说不准；没设账号那个家的缓存记着 ⇒ 要登录，不带号。
#[test]
fn the_bare_config_counts_only_the_bare_home_cache() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("barecache");
    let now: u64 = 1_800_000_000_000;
    let cj = root.join(".claude.json");
    std::fs::write(
        &cj,
        r#"{"mcpServers":{"x":{"type":"http","url":"http://x"}}}"#,
    )
    .unwrap();
    let a = root.join("a");
    let bh = root.join("bare");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&bh).unwrap();
    let hit = serde_json::json!({ "x": {"timestamp": now - 60_000} }).to_string();
    std::fs::write(a.join("mcp-needs-auth-cache.json"), &hit).unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), a), (None, bh.clone())],
        bare: Some(bh.clone()),
        now_ms: now,
    };
    let before = read_at(Some(&cj), None, &look);
    std::fs::write(bh.join("mcp-needs-auth-cache.json"), &hit).unwrap();
    let after = read_at(Some(&cj), None, &look);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(before.entries.len(), 1, "{:?}", before.entries);
    assert_eq!(
        before.entries[0].status,
        McpStatus::Unknown,
        "号 a 的缓存不算到没设账号那一份上"
    );
    assert_eq!(after.entries[0].status, McpStatus::NeedsLogin);
    assert_eq!(after.entries[0].login_in, vec![None], "不带号");
}

/// 某个号的家恰是没设账号时的那个家（`homes` 里不另列没设账号那一位）：没设账号那一份照样照那个家的缓存判出要登录，不带号。
#[test]
fn the_bare_config_finds_its_cache_when_an_account_shares_the_bare_home() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("sharedhome");
    let now: u64 = 1_800_000_000_000;
    let cj = root.join(".claude.json");
    std::fs::write(
        &cj,
        r#"{"mcpServers":{"x":{"type":"http","url":"http://x"}}}"#,
    )
    .unwrap();
    let h = root.join("dot-claude");
    std::fs::create_dir_all(&h).unwrap();
    std::fs::write(
        h.join("mcp-needs-auth-cache.json"),
        serde_json::json!({ "x": {"timestamp": now - 60_000} }).to_string(),
    )
    .unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), h.clone())],
        bare: Some(h),
        now_ms: now,
    };
    let got = read_at(Some(&cj), None, &look);
    let _ = std::fs::remove_dir_all(&root);
    assert!(got.problems.is_empty(), "{:?}", got.problems);
    assert_eq!(got.entries.len(), 1, "{:?}", got.entries);
    assert_eq!(got.entries[0].status, McpStatus::NeedsLogin);
    assert_eq!(got.entries[0].login_in, vec![None], "不带号");
}

/// 项目级停用取交集：两个号的家里只有一份停用 p1 ⇒ p1 不算停用；两份都停用 p2 ⇒ p2 停用。
#[test]
fn a_project_server_is_disabled_only_when_every_home_disables_it() {
    use crate::agents::{McpLook, McpStatus};
    let root = fixture("offboth");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string();
    std::fs::write(
        dir.join(".mcp.json"),
        r#"{"mcpServers":{"p1":{"command":"p1"},"p2":{"command":"p2"}}}"#,
    )
    .unwrap();
    let home = |tag: &str, off: &[&str]| {
        let h = root.join(tag);
        std::fs::create_dir_all(&h).unwrap();
        let cfg = serde_json::json!({ "projects": { d.clone(): { "disabledMcpServers": off } } });
        std::fs::write(h.join(".claude.json"), cfg.to_string()).unwrap();
        h
    };
    let a = home("a", &["p1", "p2"]);
    let b = home("b", &["p2"]);
    let look = McpLook {
        homes: vec![(Some("a".into()), a), (Some("b".into()), b)],
        bare: None,
        now_ms: 1,
    };
    let got = read_at(None, Some(&dir), &look);
    let _ = std::fs::remove_dir_all(&root);
    let st = |n: &str| got.entries.iter().find(|e| e.name == n).map(|e| e.status);
    assert_eq!(st("p1"), Some(McpStatus::Unknown), "{:?}", got.entries);
    assert_eq!(st("p2"), Some(McpStatus::Disabled), "{:?}", got.entries);
}

/// 项目级那一条各号都用：没设账号时那个家与号 a 的家的缓存都记着它 ⇒ 要登录的两位都在（不带号那一位在前），经核心合并也不丢。
#[test]
fn a_project_server_needing_login_bare_and_in_an_account_keeps_both() {
    use crate::agents::{mcp_held, McpLook, McpStatus};
    let root = fixture("projboth");
    let now: u64 = 1_800_000_000_000;
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".mcp.json"),
        r#"{"mcpServers":{"p":{"type":"http","url":"http://p"}}}"#,
    )
    .unwrap();
    let hit = serde_json::json!({ "p": {"timestamp": now - 60_000} }).to_string();
    let home = |tag: &str| {
        let h = root.join(tag);
        std::fs::create_dir_all(&h).unwrap();
        std::fs::write(h.join("mcp-needs-auth-cache.json"), &hit).unwrap();
        h
    };
    let a = home("a");
    let bh = home("bare");
    let look = McpLook {
        homes: vec![(Some("a".into()), a), (None, bh.clone())],
        bare: Some(bh),
        now_ms: now,
    };
    let got = read_at(None, Some(&dir), &look);
    let held = mcp_held(&got.entries, &look.homes);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].entry.status, McpStatus::NeedsLogin);
    assert_eq!(held[0].entry.login_in, vec![None, Some("a".to_string())]);
}

/// 没设账号那一份与某号那一份是同一个文件、只是写法不同（`a/../a/.claude.json`）⇒ 也只读一次。
#[test]
fn the_same_file_spelt_another_way_is_read_once() {
    use crate::agents::McpLook;
    let root = fixture("canon");
    let a = root.join("a");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(
        a.join(".claude.json"),
        r#"{"mcpServers":{"notes":{"command":"n"}}}"#,
    )
    .unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), a.clone())],
        bare: None,
        now_ms: 1,
    };
    let other = a.join("..").join("a").join(".claude.json");
    let got = read_at(Some(&other), None, &look);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(got.entries.len(), 1, "{:?}", got.entries);
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
        bare: None,
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
