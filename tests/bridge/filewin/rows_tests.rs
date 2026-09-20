use super::testing::render_headless_nonvirtual;
use super::*;
use crate::filewin::corpus;

/// 1280×800，与 `真相源/99` 那趟原型同一个视口。
fn screen() -> egui::Vec2 {
    egui::vec2(1280.0, 800.0)
}

fn rows(n: usize) -> Vec<Row> {
    corpus::synth_rows(n, 0xC0FFEE)
}

/// 🔴 **本刀的核心判据**：物化的行数与总行数**无关**。
///
/// 这是**恒等**断言（不是「小于某个上限」那种在「变少」方向瞎掉的地板）：
/// 1 000 行与 640 413 行两档，物化行数必须**一模一样**。
/// `ScrollArea::show` 结构上做不到这件事 —— 见下面那条对照。
#[test]
fn the_row_count_we_materialize_does_not_depend_on_how_many_rows_there_are() {
    let ctx = egui::Context::default();
    // 先跑一趟把字体图集建起来，免得第一趟的开销混进来。
    let _ = render_headless(&ctx, &rows(64), screen(), 0.0);

    let small = render_headless(&ctx, &rows(1_000), screen(), 0.0);
    let mid = render_headless(&ctx, &rows(100_000), screen(), 0.0);
    let big = render_headless(&ctx, &rows(640_413), screen(), 0.0);

    assert_eq!(
        small.rows_materialized, mid.rows_materialized,
        "1 000 行与 100 000 行物化的行数必须相等 —— 不相等就说明没走虚拟滚动"
    );
    assert_eq!(
        small.rows_materialized, big.rows_materialized,
        "1 000 行与 640 413 行物化的行数必须相等 —— 不相等就说明没走虚拟滚动"
    );
    // 反空真：它真的画了东西，而且远少于总数。
    assert!(
        small.rows_materialized > 0,
        "一行都没画 —— 0 不是绿（那也会让上面三个数恒等）"
    );
    assert!(
        big.rows_materialized < 640_413 / 1_000,
        "物化了 {} 行；虚拟滚动应当只物化一屏的量级",
        big.rows_materialized
    );
    // 总行数确实喂进去了（否则「无关」是因为压根没数据）。
    assert_eq!(big.total_rows, 640_413);
}

/// 🔴 **对照组**：证明上面那把尺子量得出差别。
///
/// 同一个视口、同一份语料、同一个 `paint_one_row`，只把 `show_rows` 换成 `show`
/// ⇒ 物化行数**等于总行数**。没有这一条，上面那条恒等可能两边都成立 ⇒ 空真。
#[test]
fn a_non_virtual_scroll_area_materializes_every_single_row() {
    let ctx = egui::Context::default();
    let _ = render_headless(&ctx, &rows(64), screen(), 0.0);

    // 10 万行走不虚拟的那条路，`真相源/99` 量到 83.6 ms/帧 —— 这里只数行数，不计时。
    let n = 100_000;
    let ctrl = render_headless_nonvirtual(&ctx, &rows(n), screen());
    assert_eq!(
        ctrl.rows_materialized, n,
        "不虚拟的那条路必须物化全部 {n} 行；它要是也只画一屏，这个对照组就废了"
    );

    let virt = render_headless(&ctx, &rows(n), screen(), 0.0);
    assert_eq!(virt.total_rows, ctrl.total_rows);
    // 两条路在**同一个 n** 上的差距：至少三个数量级。
    assert!(
        ctrl.rows_materialized >= virt.rows_materialized * 1_000,
        "虚拟 {} 行 vs 不虚拟 {} 行 —— 差距不到 1000 倍，尺子可疑",
        virt.rows_materialized,
        ctrl.rows_materialized
    );
}

/// 滚到中间时，物化的是**中间那一段**，不是永远的头部。
/// （只断言「窗口移动了」与「宽度不变」，不锚死具体行号 —— 那由 egui 的取整决定。）
#[test]
fn scrolling_moves_the_materialized_window_without_widening_it() {
    let ctx = egui::Context::default();
    let rows = rows(640_413);
    let _ = render_headless(&ctx, &rows, screen(), 0.0);

    let top = render_headless(&ctx, &rows, screen(), 0.0);
    let deep = render_headless(&ctx, &rows, screen(), 500_000.0);

    assert_eq!(
        top.rows_materialized, deep.rows_materialized,
        "滚到深处物化的行数应当与顶部一样宽"
    );
    assert!(
        deep.first_row > top.first_row,
        "滚了 50 万像素，起始行还是 {} —— 视口没动",
        deep.first_row
    );
    assert!(
        deep.last_row <= 640_413,
        "越界了：last_row={}",
        deep.last_row
    );
}

#[test]
fn human_size_is_short_enough_for_a_column() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1024), "1.0 K");
    assert_eq!(human_size(1024 * 1024), "1.0 M");
    assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 G");
}
