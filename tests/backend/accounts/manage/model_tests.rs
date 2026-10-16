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
        color: None,
        extra: Default::default(),
    }
}

fn render(m: &Manifest) -> serde_json::Value {
    serde_json::from_str(&m.render("/h/.claude", "zero@example.test", "2026-01-01T00:00:00Z"))
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
                { "name": "z", "email": "", "configDir": "/h/.cc-monitor/accounts/z", "isDefault": true, "mode": "isolated", "color": 0 },
                { "name": "0", "email": "zero@example.test", "isDefault": false, "mode": "bare", "color": 1 }
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

// ── 号色：账号库里的事实（加号时分第一个空格，旧号补一次；8 格用满才按名字哈希兜底）──

fn colors(m: &Manifest) -> Vec<(String, Option<u8>)> {
    m.managed().map(|a| (a.name.clone(), a.color)).collect()
}

/// ★ 名字哈希撞同一格的两个号（team / lab 都落第 4 格）加进来分到不同的色；色写进清单，账号 0 也带一格
/// （建库那一下它跟着头一个号分到，排在库序末尾）。
#[test]
fn accounts_whose_names_hash_alike_still_get_different_colors() {
    assert_eq!(
        color_fallback("team"),
        color_fallback("lab"),
        "夹具前提：两名哈希同格"
    );
    let m = Manifest::default()
        .with_added(acct("team", true))
        .with_added(acct("lab", false));
    assert_eq!(
        colors(&m),
        [("team".into(), Some(0)), ("lab".into(), Some(2))]
    );
    let out = render(&m);
    assert_eq!(out["accounts"][0]["color"], 0);
    assert_eq!(out["accounts"][1]["color"], 2);
    assert_eq!(out["accounts"][2]["name"], "0");
    assert_eq!(out["accounts"][2]["color"], 1, "账号 0 也有一格：{out}");
    let back = Manifest::parse(&out.to_string()).unwrap();
    assert_eq!(colors(&back), colors(&m), "读回来色不变");
    assert_eq!(
        render(&back)["accounts"][2]["color"],
        1,
        "账号 0 那一格读回来也不变"
    );
}

/// ★ 删一个号，其余号的色不变；再加的号补进空出来的那一格。
#[test]
fn removing_an_account_leaves_the_others_colors_alone() {
    let m = Manifest::default()
        .with_added(acct("team", true))
        .with_added(acct("lab", false))
        .with_added(acct("work", false));
    assert_eq!(
        colors(&m),
        [
            ("team".into(), Some(0)),
            ("lab".into(), Some(2)),
            ("work".into(), Some(3))
        ]
    );
    let (m, _) = m.without("lab");
    assert_eq!(
        colors(&m),
        [("team".into(), Some(0)), ("work".into(), Some(3))]
    );
    assert_eq!(render(&m)["accounts"][2]["color"], 1, "账号 0 的色也不变");
    let m = m.with_added(acct("next", false));
    assert_eq!(
        m.find("next").unwrap().color,
        Some(2),
        "空出来的第 2 格给新号"
    );
    assert_eq!(m.find("work").unwrap().color, Some(3));
}

/// 旧清单（没有 `color`）补一次：照库序分空格，已有的色与原样留着那几条占的格都不动；补过就不再补。
#[test]
fn an_old_manifest_is_colored_once_in_library_order() {
    let text = json!({
        "version": 1, "sharedStore": "/h/.claude",
        "accounts": [
            { "name": "team", "configDir": "/h/.cc-monitor/accounts/team" },
            { "name": "old", "configDir": "/h/.cc-monitor/accounts/old", "color": 0 },
            { "name": "bad name", "configDir": "/h/.cc-monitor/accounts/b", "color": 1 },
            { "name": "lab", "configDir": "/h/.cc-monitor/accounts/lab", "color": 9 },
            { "name": "0", "email": "", "isDefault": false, "mode": "bare" }
        ]
    })
    .to_string();
    let m = Manifest::parse(&text).unwrap();
    assert!(m.needs_colors());
    let c = m.with_colors();
    assert!(!c.needs_colors());
    assert_eq!(
        colors(&c),
        [
            ("team".into(), Some(2)),
            ("old".into(), Some(0)),
            ("lab".into(), Some(3))
        ],
        "原样留着那条占的第 1 格不给；读不懂的 9 当没有"
    );
    assert_eq!(render(&c)["accounts"][4]["color"], 4, "账号 0 排在最后");
    assert_eq!(c.with_colors(), c, "补过就不再动");
}

/// 8 格都用了 ⇒ 第 9 个号按名字哈希兜底（哈希只住这一处）。
#[test]
fn past_eight_accounts_the_name_hash_is_the_fallback() {
    let mut m = Manifest::default();
    for n in ["a1", "a2", "a3", "a4", "a5", "a6", "a7"] {
        m = m.with_added(acct(n, false));
    }
    let used: Vec<u8> = m.managed().filter_map(|a| a.color).collect();
    assert_eq!(used, [0, 2, 3, 4, 5, 6, 7], "第 1 格是账号 0 的");
    let m = m.with_added(acct("team", false));
    assert_eq!(m.find("team").unwrap().color, Some(color_fallback("team")));
    assert!(color_fallback("team") < COLOR_SLOTS);
}

/// 兜底号色会存进清单：钉几个另行独立算出的格（换了算法 ⇒ 同一个号兜底到别的格）。
#[test]
fn color_fallback_slots_are_pinned() {
    for (name, slot) in [("team", 4), ("lab", 4), ("x", 7), ("work", 0), ("b", 5)] {
        assert_eq!(color_fallback(name), slot, "{name}");
    }
}
