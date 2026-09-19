/// ⚠ 这里**刻意不用 `#[cfg(not(linux))]`**：那样本守卫在本机就永远不跑，
/// 而它存在的全部理由就是「本机跑不到那份代码」。`include_str!` 与平台无关。
const FALLBACK: &str = include_str!("../../../src/backend/platform/pidwatch/fallback.rs");

#[test]
fn the_non_linux_stub_stays_an_honest_stub() {
    let prod = guard_core::production_code(FALLBACK);
    // ★ 抽取器自检：文件被清空/改名时，下面两条会零命中地绿。
    assert!(
        prod.lines().count() >= 5,
        "`fallback.rs` 的生产段只剩 {} 行 —— 读法坏了或文件被掏空",
        prod.lines().count()
    );

    // ① `on_dead` 只许被**丢弃**，不许被调用。
    //    这一行变了，就说明有人改了「永远不调 on_dead」这个刻意的保守方向。
    guard_core::pin_line(&prod, "let _ = (expected_start, on_dead);").unwrap_or_else(|e| {
        panic!(
            "{e}\n\
                 ⇒ `fallback.rs` 里那句「`on_dead` 只丢弃、不调用」不见了。\n\
                 它的头注把「立刻调 `on_dead`」逐字列为**最坏**的选项：\n\
                 进程活得好好的，会话却被判死（误归档）。\n\
                 真要改成会调用，请先把 U4b（OpenProcess + WaitForSingleObject）做出来。"
        )
    });

    // ② 级别必须是 `error!`。E4：这不是可容忍的降级，是缺了一整条判活路径。
    guard_core::pin_line(&prod, "tracing::error!(").unwrap_or_else(|e| {
        panic!(
            "{e}\n\
                 ⇒ 那条日志不再是 `error!` 了。头注逐字写着为什么不是 `warn!`：\n\
                 「这不是『可容忍的降级』，是**缺了一整条判活路径**，级别要与事实相称」。"
        )
    });
}
