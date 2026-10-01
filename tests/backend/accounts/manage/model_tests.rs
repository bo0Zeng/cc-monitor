//! 清单模型：认得的改、认不得的原样留着；账号 0 读时丢、写时合成在末尾；版本不是 1 不改。
use super::*;
use serde_json::json;

const DIR: &str = "/h/.cc-monitor/accounts";

fn acct(name: &str, default: bool) -> Account {
    Account {
        name: name.into(),
        email: String::new(),
        config_dir: format!("{DIR}/{name}"),
        is_default: default,
        auth_kind: None,
        extra: Default::default(),
    }
}

fn render(m: &Manifest) -> serde_json::Value {
    serde_json::from_str(&m.render(
        "/h/.claude",
        "zero@example.test",
        "2026-01-01T00:00:00Z",
    ))
    .unwrap()
}

#[test]
fn render_writes_the_v1_shape_and_appends_account_zero() {
    let m = Manifest::default().with_added(acct("z", true));
    assert_eq!(
        render(&m),
        json!({
            "version": 1, "updatedAt": "2026-01-01T00:00:00Z", "sharedStore": "/h/.claude",
            "accounts": [
                { "name": "z", "email": "", "configDir": "/h/.cc-monitor/accounts/z", "isDefault": true, "mode": "isolated" },
                { "name": "0", "email": "zero@example.test", "isDefault": false, "mode": "bare" }
            ]
        })
    );
}

#[test]
fn parse_keeps_unknown_keys_and_odd_entries_and_drops_account_zero() {
    let text = format!(
        "\u{feff}{}",
        json!({
            "version": 1, "sharedStore": "/h/.claude", "claudeVersionPinned": "2.1.0",
            "accounts": [
                { "name": "z", "email": "a@x.test", "configDir": "/h/.cc-monitor/accounts/z/", "isDefault": true, "mode": "isolated", "note": 1 },
                { "name": "bad name", "configDir": "/h/.cc-monitor/accounts/b" },
                { "name": "k", "configDir": "/h/.cc-monitor/accounts/k", "authKind": "api-key" },
                { "name": "0", "email": "", "isDefault": false, "mode": "bare" }
            ]
        })
    );
    let m = Manifest::parse(&text).unwrap();
    assert_eq!(m.entries.len(), 3);
    let z = m.find("z").unwrap();
    assert_eq!(z.config_dir, "/h/.cc-monitor/accounts/z");
    assert_eq!(z.extra.get("note"), Some(&json!(1)));
    assert!(m.find("k").unwrap().is_api_key());
    assert!(m.name_taken("bad name"));
    let out = render(&m);
    assert_eq!(out["claudeVersionPinned"], "2.1.0");
    assert_eq!(
        out["accounts"][1],
        json!({ "name": "bad name", "configDir": "/h/.cc-monitor/accounts/b" })
    );
    assert_eq!(out["accounts"][2]["authKind"], "api-key");
    assert_eq!(out["accounts"].as_array().unwrap().len(), 4);
    assert_eq!(
        Manifest::parse(&m.render("/h/.claude", "", "t"))
            .unwrap()
            .entries,
        m.entries
    );
}

#[test]
fn parse_refuses_other_versions_and_non_objects() {
    assert!(Manifest::parse("{\"version\":2,\"accounts\":[]}").is_err());
    assert!(Manifest::parse("{\"accounts\":[]}").is_err());
    assert!(Manifest::parse("[]").is_err());
    assert!(Manifest::parse("{").is_err());
}

#[test]
fn removing_the_default_hands_it_to_the_first_remaining_account() {
    let m = Manifest::default()
        .with_added(acct("a", false))
        .with_added(acct("b", true))
        .with_added(acct("c", false));
    let (n, d) = m.without("b");
    assert_eq!(d.as_deref(), Some("a"));
    assert!(n.find("a").unwrap().is_default && !n.find("c").unwrap().is_default);
    let (n2, d2) = m.without("c");
    assert_eq!(d2, None);
    assert!(n2.find("b").unwrap().is_default);
    let one = m.with_default("c");
    assert_eq!(one.managed().filter(|a| a.is_default).count(), 1);
}
