//! 账号之间同步用户级 MCP：只换配置文件里顶层那一个键，别的键逐字节不变（各号的配置里还有登录与账号状态）。
//!
//! 扫描：一张排版各异的原文表（展开 / 压成一行 / BOM / CRLF / 键在头 · 中 · 尾 · 不在 / 别的键的字符串里带括号与引号）×
//! 几种新值，逐份判「那个键的值之外的字节」与「别的每个成员的原文」都没动。

use super::*;
use serde_json::json;

const KEY: &str = "mcpServers";

/// 原文里除 `key` 之外每个成员的原文（`"键": 值` 那一段，逐字节）。
fn others_raw(text: &str, key: &str) -> Vec<String> {
    let top = scan(text).expect("夹具是 JSON 对象");
    top.members
        .iter()
        .filter(|(k, _)| k != key)
        .map(|(_, m)| text[m.key_start..m.value.1].to_string())
        .collect()
}

/// 原文里 `key` 的值那一段之外的全部字节（键不在 ⇒ `None`）。
fn outside(text: &str, key: &str) -> Option<String> {
    let top = scan(text).expect("夹具是 JSON 对象");
    top.members
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, m)| format!("{}{}", &text[..m.value.0], &text[m.value.1..]))
}

fn shapes() -> Vec<(&'static str, String)> {
    let pretty_mid = "{\n  \"numStartups\": 12,\n  \"mcpServers\": {\n    \"a\": {\n      \"command\": \"x\"\n    }\n  },\n  \"oauthAccount\": {\n    \"emailAddress\": \"one@example.test\"\n  },\n  \"tipsHistory\": {\"x\": 1.0}\n}\n";
    vec![
        ("展开 · 键在中间", pretty_mid.to_string()),
        (
            "展开 · 键在头",
            "{\n  \"mcpServers\": {},\n  \"projects\": {\n    \"/p/{x}\": {\"allowedTools\": [\"a]\", \"b}\"]}\n  }\n}\n".to_string(),
        ),
        (
            "展开 · 键在尾",
            "{\n  \"userID\": \"u\\\"q\\\\\",\n  \"mcpServers\": {\"old\": {\"url\": \"https://x.test\"}}\n}".to_string(),
        ),
        (
            "压成一行",
            "{\"a\":1,\"mcpServers\":{\"k\":{\"command\":\"c\"}},\"b\":[1,2,{\"c\":\"}\"}]}".to_string(),
        ),
        ("BOM ＋ CRLF", format!("\u{feff}{}", pretty_mid.replace('\n', "\r\n"))),
        (
            "键不在 · 展开",
            "{\n  \"oauthAccount\": {\"emailAddress\": \"two@example.test\"},\n  \"projects\": {}\n}\n".to_string(),
        ),
        ("键不在 · 压成一行", "{\"a\":true,\"b\":null}".to_string()),
        ("空对象", "{}\n".to_string()),
        (
            "非 ASCII",
            "{\n  \"名字\": \"中文 \\u00e9 ✓\",\n  \"mcpServers\": {\"搜\": {\"command\": \"s\"}}\n}\n".to_string(),
        ),
    ]
}

fn values() -> Vec<Value> {
    vec![
        json!({}),
        json!({ "anysearch": { "command": "npx", "args": ["-y", "any"], "env": { "API_KEY": "sk-fake-0000" } } }),
        json!({ "a": { "type": "http", "url": "https://h.test/{x}", "headers": { "Authorization": "Bearer \"q\"" } }, "b": { "command": "b" } }),
    ]
}

#[test]
fn only_that_key_changes_and_every_other_byte_stays() {
    let mut checked = 0;
    for (label, text) in shapes() {
        let before_others = others_raw(&text, KEY);
        let before_outside = outside(&text, KEY);
        for v in values() {
            let out = set_top_key(&text, KEY, &v).unwrap_or_else(|e| panic!("{label}：{e}"));
            let parsed: Value =
                serde_json::from_str(out.trim_start_matches('\u{feff}')).expect("新原文是 JSON");
            assert_eq!(parsed[KEY], v, "{label}：那个键是新值");
            assert_eq!(
                others_raw(&out, KEY),
                before_others,
                "{label}：别的成员逐字节没动"
            );
            if let Some(o) = &before_outside {
                assert_eq!(
                    outside(&out, KEY).as_ref(),
                    Some(o),
                    "{label}：键原来就在 ⇒ 值那一段之外一个字节都没动"
                );
            } else if let Some((_, last)) = scan(&text).unwrap().members.last() {
                let (head, tail) = (&text[..last.value.1], &text[last.value.1..]);
                assert!(
                    out.starts_with(head) && out.ends_with(tail),
                    "{label}：键原来不在 ⇒ 补在最后一个成员之后，前后的字节不动"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, shapes().len() * values().len());
}

#[test]
fn the_new_value_follows_the_layout_of_the_line() {
    let pretty = "{\n  \"a\": 1,\n  \"mcpServers\": {}\n}\n";
    let out = set_top_key(pretty, KEY, &json!({ "s": { "command": "c" } })).unwrap();
    assert_eq!(
        out,
        "{\n  \"a\": 1,\n  \"mcpServers\": {\n    \"s\": {\n      \"command\": \"c\"\n    }\n  }\n}\n"
    );
    let compact = "{\"a\":1}";
    assert_eq!(
        set_top_key(compact, KEY, &json!({})).unwrap(),
        "{\"a\":1,\"mcpServers\": {}}"
    );
    assert_eq!(
        fresh_with(KEY, &json!({ "s": {} })).unwrap(),
        "{\n  \"mcpServers\": {\n    \"s\": {}\n  }\n}\n"
    );
}

#[test]
fn what_is_not_one_json_object_is_refused_not_rewritten() {
    for bad in [
        "",
        "[]",
        "{\"a\":1",
        "{\"a\":1} trailing",
        "{\"mcpServers\":{},\"mcpServers\":{}}",
        "not json",
    ] {
        assert!(set_top_key(bad, KEY, &json!({})).is_err(), "{bad:?} 该拒");
    }
}

fn path(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

/// 往下几层换一格：沿途那几层原样（键序与排版不动），只多出 / 换掉那一格；缺的层补成只含这条路的对象。
#[test]
fn a_nested_cell_changes_and_the_layers_on_the_way_stay() {
    let text = "{\n  \"b\": 1,\n  \"t\": {\n    \"/z\": {\n      \"y\": 1,\n      \"f\": false\n    },\n    \"/a\": {}\n  },\n  \"a\": [1, 2]\n}\n";
    let out = set_paths(
        text,
        &[
            (path(&["t", "/z", "f"]), json!(true)),
            (path(&["t", "/n", "f"]), json!(true)),
            (path(&["u", "/q", "f"]), json!(true)),
        ],
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap(),
        json!({ "b": 1, "t": { "/z": { "y": 1, "f": true }, "/a": {}, "/n": { "f": true } }, "a": [1, 2], "u": { "/q": { "f": true } } })
    );
    assert!(out.starts_with("{\n  \"b\": 1,\n  \"t\": {\n    \"/z\": {\n      \"y\": 1,\n      \"f\": true\n    },\n    \"/a\": {},\n    \"/n\": {"), "{out}");
    assert!(out.contains("\"a\": [1, 2]"), "{out}");
}

#[test]
fn a_nested_path_through_a_non_object_or_a_duplicate_is_refused() {
    let one = |t: &str| set_paths(t, &[(path(&["t", "/z", "f"]), json!(true))]);
    assert!(one("{\"t\": [1]}").is_err());
    assert!(one("{\"t\": {\"/z\": 3}}").is_err());
    assert!(one("{\"t\": {}, \"t\": {}}").is_err());
    assert!(one("{\"t\": {\"/z\": {}, \"/z\": {}}}").is_err());
}
