//! 按文件现在的内容算改法：合好的整份（别的设置原样保留、排版不动）＋ 行号 ＋ diff。只算不写。

use super::*;
use cmd_wire::record::LineDiffKind::{self, Add, Ctx, Del};
use serde_json::{json, Value};

fn row(kind: LineDiffKind, text: &str, old_no: Option<u32>, new_no: Option<u32>) -> LineDiffRow {
    LineDiffRow {
        kind,
        text: text.into(),
        old_no,
        new_no,
    }
}

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
            row(Ctx, "  \"env\": {", Some(3), Some(3)),
            row(Add, "    \"K\": \"v\",", None, Some(4)),
            row(Ctx, "    \"A\": \"1\"", Some(4), Some(5)),
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
            row(Ctx, "  \"env\": {", Some(2), Some(2)),
            row(Del, "    \"K\": \"old\",", Some(3), None),
            row(Add, "    \"K\": \"new\",", None, Some(3)),
            row(Ctx, "    \"A\": \"1\"", Some(4), Some(4)),
        ]
    );
    assert_eq!(p.at_line, 2);
}

/// 行尾风格不算差别（`\r\n` · `\r` 当 `\n`）：只出真改了的那一删一增，行号照断好的行数。
#[test]
fn 两份逐行比_行尾风格不算差别() {
    let p = plan_of("a\r\nb\r\n", "a\nc\n".to_string());
    assert_eq!(
        p.diff,
        vec![
            row(Ctx, "a", Some(1), Some(1)),
            row(Del, "b", Some(2), None),
            row(Add, "c", None, Some(2)),
        ]
    );
    assert_eq!(p.at_line, 1);
    assert_eq!(p.notice, None);
    let p = plan_of("x\ry", "x\ny\nz".to_string());
    assert_eq!(p.diff.iter().filter(|r| r.kind == Del).count(), 0);
}

/// 改动隔得远 ⇒ 只留改动那几行 ＋ 上下各一行原文，中间没变的不带。
#[test]
fn 改动之外只带上下各一行原文() {
    let old = "1\n2\n3\n4\n5\n6\n7\n";
    let p = plan_of(old, "1\n2\nX\n4\n5\n6\n7\n".to_string());
    let texts: Vec<&str> = p.diff.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, vec!["2", "3", "X", "4"]);
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
