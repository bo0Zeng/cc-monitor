//! 登录令牌的读、写回、续期请求体：秘密进出类型的三处。

use super::*;
use serde_json::json;

fn doc(v: Value) -> Map<String, Value> {
    v.as_object().expect("object").clone()
}

#[test]
fn a_token_is_read_out_of_its_section_as_a_secret_and_blank_is_none() {
    let d = doc(json!({"sec": {"a": "tok-a", "b": "  ", "n": 3}}));
    assert_eq!(secret_in(&d, "sec", "a").map(|s| s.len()), Some(5));
    assert!(secret_in(&d, "sec", "b").is_none());
    assert!(secret_in(&d, "sec", "n").is_none());
    assert!(secret_in(&d, "nope", "a").is_none());
}

#[test]
fn merging_new_tokens_changes_only_those_cells_and_keeps_every_other_key() {
    let d = doc(json!({
        "other": {"keep": true},
        "sec": {"a": "old-a", "r": "old-r", "tier": "x", "extra": [1]},
        "top": 1
    }));
    let out = merge_tokens(
        &d,
        "sec",
        &[
            ("a", &SecretKey::new("new-a")),
            ("r", &SecretKey::new("new-r")),
        ],
        &[("exp", json!(42))],
    );
    assert_eq!(
        Value::Object(out),
        json!({
            "other": {"keep": true},
            "sec": {"a": "new-a", "r": "new-r", "tier": "x", "extra": [1], "exp": 42},
            "top": 1
        })
    );
}

#[test]
fn the_refresh_body_carries_the_plain_cells_and_the_secret() {
    let body = refresh_body(
        &[("grant_type", "refresh_token"), ("client_id", "c")],
        ("refresh_token", &SecretKey::new("r-1")),
    );
    let v: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        v,
        json!({"grant_type": "refresh_token", "client_id": "c", "refresh_token": "r-1"})
    );
}
