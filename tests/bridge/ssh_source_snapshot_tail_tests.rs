use super::parse_snapshot_meta;

#[test]
fn meta_parses_and_content_lines_dont() {
    assert_eq!(
        parse_snapshot_meta(r#"{"kind":"snapshot_meta","total":100,"tail_from":95}"#),
        Some((100, 95))
    );
    // 普通 jsonl 行（含 kind 字段的行也不行——kind 值不匹配）
    assert_eq!(parse_snapshot_meta(r#"{"type":"user","uuid":"u1"}"#), None);
    assert_eq!(parse_snapshot_meta(r#"{"kind":"line","seq":0}"#), None);
    assert_eq!(parse_snapshot_meta("not json"), None);
}

/// 两段编号映射（真函数）：到达序 → 行号（尾段先到）。
#[test]
fn tail_numbering_maps_arrival_to_line_numbers() {
    use super::tail_seq;
    // 到达序：尾段 [3,4]，头段 [0,1,2]
    assert_eq!(
        (0..5).map(|i| tail_seq(i, 5, 3)).collect::<Vec<_>>(),
        vec![3, 4, 0, 1, 2],
        "全部行号恰覆盖 0..total 且顺序=尾部优先"
    );
    // 全尾（tail_from=0）与空文件退化
    assert_eq!(
        (0..3).map(|i| tail_seq(i, 3, 0)).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(tail_seq(0, 0, 0), 0);
}

/// meta 关系防御：tail_from > total 拒收（防远端损坏输出打乱 seq 空间）。
#[test]
fn malformed_meta_rejected() {
    use super::parse_snapshot_meta;
    assert_eq!(
        parse_snapshot_meta(r#"{"kind":"snapshot_meta","total":5,"tail_from":10}"#),
        None
    );
}
