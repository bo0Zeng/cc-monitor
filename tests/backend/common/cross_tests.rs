//! 纯工具那一层判据里引到上层的那几条：这一层是独立的包、够不到上层，这几条挂在后端库根（上层拆成包时各自带走）。

/// 复制详情本身只写时刻的空位：那一层不知道看的人是谁（上层：wire）。
#[test]
fn the_copy_detail_only_writes_the_time_slot() {
    assert!(crate::wire::detail::of(Some("c"), "x", None).contains("⟦at:"));
}
