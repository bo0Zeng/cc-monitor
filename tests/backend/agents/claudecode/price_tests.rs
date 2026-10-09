//! 定价：前缀认型号（带日期尾巴的也认）· 读缓存另定的型号照表 · 写缓存两档按输入价倍数 · 认不出 ⇒ 无。
use super::*;

#[test]
fn models_are_recognised_by_prefix_and_priced_in_micro_dollars() {
    let r = rates("claude-sonnet-4-5-20250929").unwrap();
    assert_eq!(
        (r.input, r.output, r.cache_read),
        (3_000_000, 15_000_000, 300_000)
    );
    assert_eq!((r.cache_write5m, r.cache_write1h), (3_750_000, 6_000_000));
    assert_eq!(rates("claude-opus-5-5").unwrap().cache_read, 200_000);
    assert_eq!(rates("claude-opus-4-1-20250805").unwrap().input, 15_000_000);
    assert_eq!(rates("claude-opus-4-6").unwrap().input, 5_000_000);
    assert!(rates("<synthetic>").is_none());
    assert!(rates("gpt-x").is_none());
}
