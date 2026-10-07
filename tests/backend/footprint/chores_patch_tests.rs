//! 按文件现在的内容算改法：合好的整份（别的设置原样保留、排版不动）＋ 行号 ＋ diff。只算不写。

use super::*;
use serde_json::{json, Value};

fn parsed(s: &str) -> Value {
    serde_json::from_str(s).expect("合好的整份要是合法 JSON")
}

#[test]
fn env_里已有别的键_插进去_其余一字不动() {
    let src = "{\n  \"model\": \"opus\",\n  \"env\": {\n    \"A\": \"1\"\n  }\n}\n";
    let p = set_member(src, &["env", "K"], &json!("v")).unwrap();
    assert_eq!(
        p.whole,
        "{\n  \"model\": \"opus\",\n  \"env\": {\n    \"K\": \"v\",\n    \"A\": \"1\"\n  }\n}\n"
    );
    assert_eq!(parsed(&p.whole)["env"], json!({"K": "v", "A": "1"}));
    // diff：上下各一行原文，加的那一行是绿的，行号按现在的文件算。
    assert_eq!(
        p.diff,
        vec![
            DiffLine::Same(3, "  \"env\": {".into()),
            DiffLine::Add("    \"K\": \"v\",".into()),
            DiffLine::Same(4, "    \"A\": \"1\"".into()),
        ]
    );
    assert_eq!(p.at_line, 3, "在第 3 行后面加");
}

#[test]
fn 键已在_换掉那一行的值() {
    let src = "{\n  \"env\": {\n    \"K\": \"old\",\n    \"A\": \"1\"\n  }\n}";
    let p = set_member(src, &["env", "K"], &json!("new")).unwrap();
    assert_eq!(parsed(&p.whole)["env"], json!({"K": "new", "A": "1"}));
    assert_eq!(
        p.diff,
        vec![
            DiffLine::Same(2, "  \"env\": {".into()),
            DiffLine::Del(3, "    \"K\": \"old\",".into()),
            DiffLine::Add("    \"K\": \"new\",".into()),
            DiffLine::Same(4, "    \"A\": \"1\"".into()),
        ]
    );
}

#[test]
fn 没有_env_在根上加一段() {
    let src = "{\n  \"model\": \"opus\"\n}\n";
    let p = set_member(src, &["env", "K"], &json!("v")).unwrap();
    assert_eq!(
        parsed(&p.whole),
        json!({"model": "opus", "env": {"K": "v"}})
    );
    assert!(
        p.whole
            .starts_with("{\n  \"env\": {\n    \"K\": \"v\"\n  },\n  \"model\""),
        "{}",
        p.whole
    );
}

#[test]
fn 文件不在或空_整份就是新的() {
    for src in ["", "  \n"] {
        let p = set_member(src, &["env", "K"], &json!("v")).unwrap();
        assert_eq!(parsed(&p.whole), json!({"env": {"K": "v"}}));
    }
}

#[test]
fn 数组里追加一项() {
    let src = "{\n  \"hooks\": {\n    \"Stop\": [\n      {\"x\": 1}\n    ]\n  }\n}\n";
    let p = push_item(src, &["hooks", "Stop"], &json!({"y": 2})).unwrap();
    assert_eq!(
        parsed(&p.whole)["hooks"]["Stop"],
        json!([{"x": 1}, {"y": 2}])
    );
    let p2 = push_item(src, &["hooks", "SessionStart"], &json!({"y": 2})).unwrap();
    assert_eq!(
        parsed(&p2.whole)["hooks"]["SessionStart"],
        json!([{"y": 2}])
    );
    assert_eq!(parsed(&p2.whole)["hooks"]["Stop"], json!([{"x": 1}]));
}

#[test]
fn 连着改两处_第二处按第一处改完的文本算() {
    let src = "{\n  \"env\": {}\n}\n";
    let a = set_member(src, &["env", "K"], &json!("v")).unwrap();
    let b = push_item(&a.whole, &["hooks", "Stop"], &json!(1)).unwrap();
    assert_eq!(
        parsed(&b.whole),
        json!({"env": {"K": "v"}, "hooks": {"Stop": [1]}})
    );
}

#[test]
fn 坏_JSON_或根不是对象_不给改法() {
    for src in ["{", "[1]", "{\"env\": 3}"] {
        assert!(
            set_member(src, &["env", "K"], &json!("v")).is_none(),
            "{src}"
        );
    }
}

#[test]
fn 字符串里的括号和引号不搅乱() {
    let src = "{\n  \"note\": \"a } \\\" { b\",\n  \"env\": {\n    \"A\": \"]\"\n  }\n}";
    let p = set_member(src, &["env", "K"], &json!("v")).unwrap();
    assert_eq!(parsed(&p.whole)["note"], json!("a } \" { b"));
    assert_eq!(parsed(&p.whole)["env"], json!({"K": "v", "A": "]"}));
}
