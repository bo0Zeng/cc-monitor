//! 会话外的需手动清单：账号清单成品（夹具，照 `accounts-list` 那一份写）· MCP 读面（夹具）⇒ 每一项带写好的那一句和怎么解。夹具全是合成的。
use super::*;
use serde_json::json;

fn row(name: &str, kind: &str, exists: bool, ready: bool) -> Value {
    json!({ "name": name, "authKind": kind, "exists": exists, "authReady": ready })
}

fn entry(
    name: &str,
    status: crate::agents::McpStatus,
    login_in: &[&str],
) -> crate::agents::McpEntry {
    crate::agents::McpEntry {
        scope: "user",
        name: name.to_string(),
        server: json!({}),
        source: "/h/.claude.json".to_string(),
        status,
        login_in: login_in.iter().map(|s| s.to_string()).collect(),
        seen_ms: None,
    }
}

/// 需登录的号：订阅号 · 目录在 · 还没就绪的那几个（API 号 · 目录不在 · 已登录的不算）；每项 `fix` 指向 `login-start`。
/// 需登录的 MCP：每个号一项，没设账号的那一份 `account: null`；断开 · 停用的不算；MCP 那一项没有 `fix`（在 app 里登 MCP 还没做）。
#[test]
fn the_manual_list_names_each_thing_and_how_to_fix_it() {
    let accounts = json!({ "accounts": [
        row("work", "subscription", true, false),
        row("0", "subscription", true, false),
        row("api", "api-key", true, false),
        row("gone", "subscription", false, false),
        row("ok", "subscription", true, true),
    ]});
    let mcp = crate::agents::McpRead {
        entries: vec![
            entry("notes", crate::agents::McpStatus::NeedsLogin, &["work"]),
            entry("bare", crate::agents::McpStatus::NeedsLogin, &[]),
            entry("down", crate::agents::McpStatus::Failed, &[]),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let got = serde_json::to_value(manual_of("claude", &accounts, Some(&mcp))).unwrap();
    let fix =
        |a: &str| json!({ "command": "login-start", "args": { "agent": "claude", "account": a } });
    assert_eq!(
        got,
        json!({ "items": [
            { "kind": "accountLogin", "agent": "claude", "account": "work",
              "said": copy_text("beManual.list.accountLogin", &[("account", "work")]), "fix": fix("work") },
            { "kind": "accountLogin", "agent": "claude", "account": "0",
              "said": copy_text("beManual.list.accountLogin", &[("account", &copy_text("acct.home.name", &[]))]), "fix": fix("0") },
            { "kind": "mcpLogin", "account": "work", "server": "notes", "scope": "user",
              "said": copy_text("beManual.list.mcpLogin", &[("server", "notes"), ("account", "work")]) },
            { "kind": "mcpLogin", "account": null, "server": "bare", "scope": "user",
              "said": copy_text("beManual.list.mcpLoginBare", &[("server", "bare")]) },
        ]})
    );
    // 那一家不在 app 里登 ⇒ 不列需登录的号（没有解法的不列成「点了能解」）。
    let fake = serde_json::to_value(manual_of("no-such-agent", &accounts, None)).unwrap();
    assert_eq!(fake, json!({ "items": [] }));
}

impl crate::guard_support::Shaped for LoginStarted {
    fn samples() -> Vec<Self> {
        vec![LoginStarted {
            job: "login-1-0".into(),
            url: "https://claude.com/cai/oauth/authorize?state=s".into(),
            paste: PasteKind::Code,
            auto: true,
        }]
    }
}
