use super::testing::{click_at, render_headless_nonvirtual, render_headless_with_events};
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

/// 🔴 **第二刀：那一行真的接得住双击** —— 而且点的是**哪一行**要对。
///
/// 这一条走的是真 egui 输入（`RawInput::events`）＋ **生产那个**画行函数
/// ⇒ 把 `paint_one_row` 里那个 `interact` 摘掉、或者把双击改成别的手势，当场红。
///
/// ⚠ 双击判定要 `input.time` 真的往前走（egui 靠两次 click 的时间差认双击）
/// ⇒ 三帧：移进去 → 第一次 click → 第二次 click。
/// ⚠ 买不到「鼠标在真窗口上按下去是什么手感」；买的是「事件进来之后哪一行被认出来」。
#[test]
fn a_double_click_on_a_row_comes_back_as_that_rows_index() {
    let ctx = egui::Context::default();
    let rows = rows(200);
    // 先跑一帧把布局/字体建起来，行的矩形才有位置。
    let warm = render_headless_with_events(&ctx, &rows, screen(), 0.0, Vec::new());
    assert!(
        warm.rows_materialized > 3,
        "一屏连 4 行都没有，下面按坐标点就没意义"
    );

    // 第 2 行的中心：列表从 y≈0 起，行高 ROW_HEIGHT ＋ item spacing。
    let want = 2usize;
    let y = (want as f32 + 0.5) * (ROW_HEIGHT + 4.0);
    let pos = egui::pos2(60.0, y);

    let _ = render_headless_with_events(
        &ctx,
        &rows,
        screen(),
        0.10,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = render_headless_with_events(&ctx, &rows, screen(), 0.20, click_at(pos));
    let second = render_headless_with_events(&ctx, &rows, screen(), 0.30, click_at(pos));

    let got = second.clicked.expect(
        "双击之后 `RenderTally::clicked` 还是 `None` —— \
         那一行不接点击（`paint_one_row` 里那个 `interact` 没了？），\
         于是「双击目录进去」这条链断在最前面",
    );
    // 允许 ±1 行的取整误差（行高与 spacing 的取整由 egui 定），但不许差得更多。
    assert!(
        got.abs_diff(want) <= 1,
        "点在第 {want} 行的位置上，认出来的是第 {got} 行 —— 差了 {} 行",
        got.abs_diff(want)
    );
}

/// 反空真：**没人点的时候它必须是 `None`。**
/// 没有这一条，上面那条可能只是「每帧都报第某行被点了」。
#[test]
fn a_frame_with_no_input_reports_no_click() {
    let ctx = egui::Context::default();
    let rows = rows(200);
    for t in 0..3 {
        let t = render_headless_with_events(&ctx, &rows, screen(), t as f64 * 0.1, Vec::new());
        assert_eq!(t.clicked, None, "没有任何输入，却报了一次点击");
    }
    // 单击一次（不是双击）也不许算 —— 双击才进目录（同旧面板 `panel.ts` 的口径）。
    let _ = render_headless_with_events(
        &ctx,
        &rows,
        screen(),
        1.0,
        vec![egui::Event::PointerMoved(egui::pos2(60.0, 30.0))],
    );
    let one =
        render_headless_with_events(&ctx, &rows, screen(), 1.1, click_at(egui::pos2(60.0, 30.0)));
    assert_eq!(
        one.clicked, None,
        "单击就进目录了 —— 旧面板是双击进（`panel.ts` 的 `dblclick`），别让两个面板两套手感"
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
