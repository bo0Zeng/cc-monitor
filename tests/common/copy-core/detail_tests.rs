//! 复制详情那几行：只列有值的项 · 原话截断 · 时刻写法 · 项名闭集与文案表两向相等。

use super::*;

#[test]
fn only_items_with_values_render_one_per_line() {
    let d = Detail::new()
        .item(Label::At, "2026-10-08 14:32:07 +08:00")
        .item(Label::Path, "   ")
        .maybe(Label::Target, None::<&str>)
        .item(Label::Code, "kill_failed")
        .item(Label::Raw, "  can't find window\n");
    assert_eq!(
        d.render(),
        "时刻：2026-10-08 14:32:07 +08:00\n码：kill_failed\n原话：can't find window"
    );
    assert_eq!(Detail::new().render(), "");
}

#[test]
fn raw_is_cut_at_the_cap_on_a_char_boundary() {
    let long = "错".repeat(RAW_CAP); // 每字 3 字节
    let cut = truncate_raw(&long);
    let tail = copy_text("detail.value.truncated", &[]);
    assert!(cut.ends_with(&tail));
    let body = &cut[..cut.len() - tail.len()];
    assert!(body.len() <= RAW_CAP && body.len() > RAW_CAP - 3);
    assert_eq!(truncate_raw("short"), "short");
}

#[test]
fn append_adds_one_line_and_skips_empty_values() {
    assert_eq!(
        append("码：x", Label::Local, "cc-monitor 1"),
        "码：x\n本机：cc-monitor 1"
    );
    assert_eq!(append("", Label::Local, "a"), "本机：a");
    assert_eq!(append("码：x", Label::Local, " "), "码：x");
}

#[test]
fn stamp_writes_local_time_with_offset() {
    // 2026-10-08 06:32:07 UTC
    let t = 1_791_440_000 - 1_791_440_000 % 86_400 + 6 * 3600 + 32 * 60 + 7;
    let day = copy_core_day(t);
    assert_eq!(stamp(t, 8 * 3600), format!("{day} 14:32:07 +08:00"));
    assert_eq!(
        stamp(t, -(5 * 3600 + 30 * 60)),
        format!("{day} 01:02:07 -05:30")
    );
    assert_eq!(stamp(0, 0), "1970-01-01 00:00:00 +00:00");
}

fn copy_core_day(t: i64) -> String {
    let (y, m, d) = crate::civil_from_days(t.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

#[test]
fn labels_are_a_closed_set_equal_to_the_copy_table() {
    let table: serde_json::Value = serde_json::from_str(crate::TABLE_JSON).unwrap();
    let mut in_table: Vec<String> = table["entries"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("detail.label."))
        .cloned()
        .collect();
    in_table.sort();
    let mut said: Vec<String> = Label::ALL.iter().map(|l| l.said()).collect();
    let mut want: Vec<String> = in_table
        .iter()
        .map(|k| table["entries"][k]["zh"].as_str().unwrap().to_string())
        .collect();
    said.sort();
    want.sort();
    assert_eq!(said, want, "Label 闭集 ≠ 文案表 detail.label.*");
    assert_eq!(in_table.len(), Label::ALL.len());
}
