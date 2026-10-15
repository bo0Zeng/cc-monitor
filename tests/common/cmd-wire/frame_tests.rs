//! 帧的线上形：金样（后端序列化器写的那几份）每一行按这里的类型解得开、重写逐字节相同；`line` 帧的记录那一格另三种解法各解一遍。

use super::*;

/// 后端那两份会话流金样（仓根相对）：每种帧全格 · 最少格各一行，以及一段真适配层产出的流。
const GOLDENS: [&str; 2] = [
    "tests/__fixtures__/session-stream.golden.jsonl",
    "tests/__fixtures__/codex-session-stream.golden.jsonl",
];

fn golden(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {}：{e}", path.display()))
}

fn lines() -> Vec<(String, String)> {
    GOLDENS
        .iter()
        .flat_map(|g| {
            golden(g)
                .lines()
                .map(|l| (g.to_string(), l.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 解开再写回，与原行逐字节相同（记录那一格是 `R`）。
fn same_bytes<R>(rel: &str, line: &str)
where
    R: Serialize + for<'de> Deserialize<'de>,
{
    let f: Frame<R> =
        decode(line).unwrap_or_else(|e| panic!("{rel} 这一行解不开（{e}）：{line}"));
    let back = serde_json::to_string(&f).unwrap();
    assert_eq!(back, line, "{rel} 这一行解开再写回变了");
}

#[test]
fn every_golden_line_decodes_and_writes_back_byte_for_byte() {
    let all = lines();
    let kinds: std::collections::BTreeSet<String> = all
        .iter()
        .map(|(_, l)| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    // 正控：会话流金样每种帧都有（新加一种帧、金样没补，后端那条判据先红）。
    assert!(kinds.contains("hello") && kinds.contains("terminal_follow_end"));
    for (rel, line) in &all {
        same_bytes::<crate::record::Record>(rel, line);
    }
}

#[test]
fn the_line_frames_decode_with_each_record_reading() {
    let line_frames: Vec<(String, String)> = lines()
        .into_iter()
        .filter(|(_, l)| l.starts_with(r#"{"kind":"line","#))
        .collect();
    assert!(
        line_frames.iter().any(|(_, l)| l.contains(r#""record":"#))
            && line_frames.iter().any(|(_, l)| !l.contains(r#""record":"#)),
        "金样里要有带记录与不带记录的 line 帧各一行"
    );
    for (rel, line) in &line_frames {
        same_bytes::<crate::record::OrUnread>(rel, line);
        same_bytes::<Box<serde_json::value::RawValue>>(rel, line);
        // 记录当一团 JSON 值：解得开，值相等（值的键序是字典序，字节不比）。
        let f: Frame<serde_json::Value> = decode(line).unwrap();
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            serde_json::from_str::<serde_json::Value>(line).unwrap(),
            "{rel} 这一行按 JSON 值解开再写回变了"
        );
    }
}

#[test]
fn a_record_that_does_not_read_becomes_the_unread_one_and_the_frame_still_decodes() {
    let line = r#"{"kind":"line","session_id":"s","path":"/p","seq":1,"record":{"agent":"a","id":"x","t":"later"},"byte_offset":9}"#;
    assert!(decode::<crate::record::Record>(line).is_err());
    let Frame::Line(l) = decode::<crate::record::OrUnread>(line).unwrap() else {
        panic!("不是 line 帧");
    };
    assert!(matches!(
        l.record.unwrap().0.body,
        crate::record::Body::Unread { .. }
    ));
    // 原文照转那一种：记录那一格逐字不动。
    let Frame::Line(l) = decode::<Box<serde_json::value::RawValue>>(line).unwrap() else {
        panic!("不是 line 帧");
    };
    assert_eq!(
        l.record.unwrap().get(),
        r#"{"agent":"a","id":"x","t":"later"}"#
    );
}

#[test]
fn the_kind_need_not_come_first() {
    let line = r#"{"session_id":"s","path":"/p","seq":1,"kind":"line","record":{"x":1},"byte_offset":9}"#;
    let Frame::Line(l) = decode::<Box<serde_json::value::RawValue>>(line).unwrap() else {
        panic!("不是 line 帧");
    };
    assert_eq!(l.record.unwrap().get(), r#"{"x":1}"#);
    let line = r#"{"id":"q","kind":"cancelled"}"#;
    assert!(matches!(
        decode::<serde_json::Value>(line).unwrap(),
        Frame::Cancelled(_)
    ));
}
