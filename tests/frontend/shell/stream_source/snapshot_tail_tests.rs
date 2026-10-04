// 快照的尾段图由帧面 `history-tail` 给（校验住 `frame_query::tail`）。

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
