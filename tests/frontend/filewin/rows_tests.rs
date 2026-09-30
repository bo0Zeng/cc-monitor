use super::testing::{
    click_at, render_headless_nonvirtual, render_headless_with_events,
    render_headless_with_events_and_text,
};
use super::*;
use crate::copy::testing::rects_of;
use crate::copy::{is_copyable, COPY_LABEL};
use crate::corpus;
use crate::source::Row;

/// 1280×800，与 `真相源/99` 那趟原型同一个视口。
fn screen() -> egui::Vec2 {
    egui::vec2(1280.0, 800.0)
}

fn rows(n: usize) -> Vec<Listed> {
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

/// 🔴〔第四刀〕**命中那一摞也得是虚拟的。**
///
/// 形状照上面那条办（**恒等**断言，不是地板）：50 条与 20 000 条，
/// 物化行数必须一模一样。
///
/// ⚠ 为什么不能靠「`limit` 默认 1000 所以条数有界」偷懒用 `show`：
/// 那个上界住 `src/doc/IPC-PROTOCOL.md §10`（**后端那一侧的默认值**），
/// 不是这一侧给的 —— 哪天调用方开始发一个大 `limit`，这一格就当场变成
/// `真相源/99 §2.3` 那张表里 100 000 行 / 83.6 ms 那一格。
#[test]
fn the_hit_list_materializes_the_same_few_rows_no_matter_how_many_hits() {
    let ctx = egui::Context::default();
    let hits = |n: usize| -> Vec<String> { corpus::synth_paths(n, 0x24F4).into_iter().collect() };
    // 🔴〔第五刀〕收数口是 `HitTally`，**不是** `RenderTally` —— 那个类型里
    //    连一个「谁被点了」的字段都没有，理由逐条住它的头注（命中行上那个
    //    下标索引的是另一摞东西，而第五刀把代价从「复制错地方」升级成「删错东西」）。
    let run = |hs: &[String]| -> HitTally {
        let mut t = HitTally::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen())),
            ..Default::default()
        };
        // 🔴 调的是**生产那个函数**，不是它的副本（同 `show_file_rows` 那条理由）。
        let out = ctx.run_ui(input, |ui| show_hit_rows(ui, hs, &mut t));
        out.drop_without_applying_deltas();
        t
    };
    let _ = run(&hits(64)); // 字体图集先建起来
    let small = run(&hits(50));
    let big = run(&hits(20_000));
    assert_eq!(
        small.rows_materialized, big.rows_materialized,
        "50 条命中与 20 000 条命中物化的行数必须相等 —— 不相等就说明命中那一摞没走虚拟滚动"
    );
    assert!(
        small.rows_materialized > 0,
        "一行命中都没画 —— 0 不是绿（那也会让上面那条恒等成立）"
    );
    assert!(
        big.rows_materialized < 20_000 / 100,
        "物化了 {} 行命中；虚拟滚动应当只物化一屏的量级",
        big.rows_materialized
    );
    assert_eq!(big.total_rows, 20_000, "总条数没喂进去");
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

// ════════════════════════════════════════════════════════════════════════
// 第三刀：行上那颗「复制」—— 点得到，而且**没被整行那块矩形吞掉**
// ════════════════════════════════════════════════════════════════════════

/// 四行，四档齐：目录 · 两个普通文件 · 一个有损名。
///
/// ⚠ 走 `Listed::plain`（链接与时间两格都「没送」）—— 本族判的是那六颗按钮，
/// 新那两列各有自己的判据（下面那两节）。
fn mixed_rows() -> Vec<Listed> {
    let mk = |name: &str, is_dir: bool, lossy: bool, size: u64| {
        crate::source::Listed::plain(Row {
            name: name.to_string(),
            path: format!("/srv/{name}"),
            is_dir,
            size,
            lossy_name: lossy,
        })
    };
    vec![
        mk("adir", true, false, 0),
        mk("one.bin", false, false, 10),
        mk("two.bin", false, false, 20),
        mk("\u{FFFD}odd", false, true, 30),
    ]
}

/// 🔴 **只有能复制的那几行才有那颗按钮** —— 相等断言，两侧都从同一个地方来：
/// 画出来的「复制」几个 · `is_copyable` 说有几个。
///
/// ⚠ 判的是**画出来的东西**，不是「源码里有个 `if is_copyable`」。
#[test]
fn only_the_copyable_rows_get_a_copy_button_painted() {
    let ctx = egui::Context::default();
    let rows = mixed_rows();
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
    let (tally, painted) =
        render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());
    assert_eq!(tally.rows_materialized, 4, "四行没画全，下面的数就没意义");
    assert!(
        !painted.is_empty(),
        "这一帧一个字都没画出来 —— 量具塌了，下面那一比在空转"
    );

    let want = rows.iter().filter(|r| is_copyable(r)).count();
    // 〔W5-FILES〕目录能复制了（`设计/60 §6.2`）⇒ 2 → 3（目录那一行多一颗）。
    assert_eq!(want, 3, "语料自己变了：能复制的行数应当是 3");
    let got = rects_of(&painted, COPY_LABEL.as_str());
    assert_eq!(
        got.len(),
        want,
        "画出来 {} 颗「{COPY_LABEL}」，而 `is_copyable` 说有 {want} 行能复制 —— \
         目录或有损名那两档上长出了一颗不该有的按钮（或者能复制的那几行少了一颗）",
        got.len(),
        COPY_LABEL = COPY_LABEL.as_str()
    );
    // 反空真：它们在三行不同的位置上（不是同一颗被数了几遍）。
    assert!(got[0].center().y < got[1].center().y);
    assert!(got[1].center().y < got[2].center().y);
}

/// 🔴 **这一刀的核心判据**：点那颗「复制」，回来的是**那一行**的下标。
///
/// 它同时是那条真缺陷的钉子：整行那块命中矩形要是拉满整行宽，
/// egui 在平手时取**后登记**的那一个（`egui-0.36.2/src/hit_test.rs` 逐字
/// 「In case of a tie, take the last one = the one on top」）⇒ 按钮恒 false，本条当场红。
///
/// ⚠ 买的是「egui 收到这样一串事件之后认出来的是哪一个控件」；
/// **买不到**「真机上鼠标点得到」（本机 `XDG_SESSION_TYPE=tty`，没有真事件源）。
#[test]
fn clicking_the_copy_button_comes_back_as_that_rows_index() {
    let ctx = egui::Context::default();
    let rows = mixed_rows();
    // 先跑两帧：建字体图集 ＋ 让上一帧的 widget 表有内容（命中测试按上一帧做）。
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
    let (_, painted) = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());

    let buttons = rects_of(&painted, COPY_LABEL.as_str());
    assert_eq!(buttons.len(), 3, "没找到那三颗按钮，下面按坐标点没意义");

    // 第三颗 = 第 2 行（`two.bin`；第 0 行是目录、第 1 行是 `one.bin` —— 〔W5-FILES〕目录那一行也有一颗了）。
    let pos = buttons[2].center();
    let _ = render_headless_with_events(
        &ctx,
        &rows,
        screen(),
        0.2,
        vec![egui::Event::PointerMoved(pos)],
    );
    let hit = render_headless_with_events(&ctx, &rows, screen(), 0.3, click_at(pos));

    assert_eq!(
        hit.copy_clicked,
        Some(2),
        "点在第 2 行那颗「{COPY_LABEL}」上，`copy_clicked` 却是 {:?} —— \
         `None` 多半是整行那块命中矩形把按钮盖住了（它是后登记的，平手时它赢）",
        hit.copy_clicked,
        COPY_LABEL = COPY_LABEL.as_str()
    );
    // 点按钮**不许**同时被读成「双击进目录」。
    assert_eq!(hit.clicked, None, "点一颗按钮竟然还顺手进了目录");
}

/// 🔴 **另一侧**：让开按钮之后，名字那一段**照旧**双击进目录。
///
/// 没有这一条，「让开」可以退化成「整行那块矩形干脆不要了」——
/// 那样上面那条照样绿，而双击进目录悄悄没了。
#[test]
fn making_way_for_the_button_does_not_kill_the_rest_of_the_row() {
    let ctx = egui::Context::default();
    let rows = mixed_rows();
    let _ = render_headless_with_events(&ctx, &rows, screen(), 0.0, Vec::new());

    // 第 1 行（`one.bin`，有按钮）名字那一段：x=60 在图标/名字上，远在按钮左边。
    let y = 1.5 * (ROW_HEIGHT + 4.0);
    let pos = egui::pos2(60.0, y);
    let _ = render_headless_with_events(
        &ctx,
        &rows,
        screen(),
        0.1,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = render_headless_with_events(&ctx, &rows, screen(), 0.2, click_at(pos));
    let second = render_headless_with_events(&ctx, &rows, screen(), 0.3, click_at(pos));

    let got = second
        .clicked
        .expect("名字那一段双击不回任何行 —— 整行那块命中矩形被让没了");
    assert!(
        got.abs_diff(1) <= 1,
        "点在第 1 行的名字上，认出来的是第 {got} 行"
    );
    assert_eq!(second.copy_clicked, None, "点名字竟然算成点了「复制」");
}

#[test]
fn human_size_is_short_enough_for_a_column() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1024), "1.0 K");
    assert_eq!(human_size(1024 * 1024), "1.0 M");
    assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 G");
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第四刀 2026-09-20〕**真事件**：真 X 事件走完整条路，点得动那颗按钮
// ════════════════════════════════════════════════════════════════════════
//
// # 为什么这一格最要紧
//
// 今天这一面**所有**交互判据喂的都是**合成的 egui 事件**
// （`render_headless_with_events` 那条路装一份 `RawInput` 直接塞进去）。
// 而 `真相源/99` 里第二刀与第三刀各逮到一次「控件画得出、`clicked()` 恒 false」：
//
// - `§9.1`：`ui.horizontal(…)` 那个布局作用域响应上再 `interact` 出来的一份，
//   三个数全 false；
// - `§10.1`：整行那块命中矩形横向拉满 ⇒ 把行上那颗按钮**整个盖住**，
//   egui 平手时取后登记的那一个 ⇒ 按钮永远点不到。
//
// 两次都是**合成事件那一层看得见**的（判据当场红），但那一层看不见的还有一整族：
// **winit → egui 那一段**（真 X 事件怎么被翻成 `egui::Event`、
// 按下/松开的时间与位置怎么被 `InputState` 认成一次 click / double click、
// 缩放因子怎么把物理像素换成点）。那一段今天**一趟都没跑过**。
//
// ⇒ 本段把它跑一趟：**真 Xvfb ＋ 真 XTEST 注入的鼠标事件 ＋ 真 winit ＋
// 生产那个 `show_file_rows`**，从生产那个 `RenderTally` 里读回「认出来的是哪一个控件」。
//
// # 🔴 被测对象仍然**只有一份**（这一条是承重的，同 `真相源/99 §8.5` 那条教训）
//
// 本段那个 `eframe::App` **只做两件事**：调生产那个 `show_file_rows`（参数逐字同
// `render_headless_with_events_and_text` 里那一行）· 把生产那个 `RenderTally` 抄到
// 一个共享格子里让驱动线程读得到。
// **画一行的那段代码、命中矩形那段代码，一个字节都不在这儿。**
// ⇒ 把 `show_rows` 换成 `show`、把整行那块矩形改回拉满、把那个 `interact` 摘掉，
// 本段照样当场红。
//
// ⚠ 为什么不直接驱动生产那个窗口（`FileWindow`）：它没有可观测出口
// （判据读不到它的 `tally`），而生产那棵树本刀**只许调不许改**。
// ⇒ 窗口壳那一层由 `shell` 那一格买（同一条 `run_native` 路），
// 控件那一层由本段买（同一个生产行画函数）。**两格合起来盖住这条链，各自都说得清射程。**
//
// # ⚠ 它买不到什么（逐条，别读宽）
//
// - **真 GPU / 字体回落 / DPI / 合成器四样一个都买不到**（逐格住台架头注）。
//   本段还**额外**要求缩放因子恰好是 1 —— 不是 1 就报「这一格判不了」，
//   因为按钮的坐标是拿另一把尺子（合成台架）量的，两把尺子必须同轴。
// - **它不量帧时也不量内存**：`真相源/99` 那些数是另一个分母。
// - **真物理鼠标买不到**：XTEST 注进去的是真 X 事件，但按下那一刻的
//   抖动、加速曲线、触控板的手势合成，这里一样都没有。
// - **Windows 一趟没跑过**（本族整条 `cfg(not(windows))`）。

/// 真事件那一趟从窗口里带出来的全部读数。
///
/// 🔴 点击那两个数是**累计**的，不是「这一帧的」：`RenderTally` 里那两个
/// `Option` 只活一帧，驱动线程隔着一条线程去读，必然读到下一帧的 `None`。
/// ⇒ 这里**latch** 起来，并且**同时记次数** —— 次数才谈得上相等断言
/// （「点一下算一次」与「点一下算三次」在只看最后一个下标时分不开）。
#[cfg(not(windows))]
#[derive(Default)]
struct RealEventProbe {
    frames: u64,
    screen_w: f32,
    screen_h: f32,
    ppp: f32,
    materialized: usize,
    copy_clicks: u32,
    last_copy: Option<usize>,
    row_clicks: u32,
    last_row: Option<usize>,
    /// egui **输入层**自己数出来的点击／双击次数，以及指针最后落在哪。
    ///
    /// 🔴 它存在的理由是**归因**：上面那两个数是「控件收到了吗」，
    /// 这两个是「事件到没到 egui」。一条真事件路上失效点有两处
    /// （X → winit → egui 那一段 · egui 的命中测试那一段），
    /// 只量后一个的话红了说不出是哪一段坏了 —— `真相源/99 §9.1`
    /// 那趟逐帧现打就是这么把病根从「headless 收不到事件」改判成
    /// 「那个 id 没进到能被命中的那一档」的。
    input_clicks: u32,
    input_double_clicks: u32,
    last_pointer: Option<egui::Pos2>,
    /// 驱动线程量完了 ⇒ 让窗口**自己**把自己关掉（见本段头注）。
    close_now: bool,
}

/// 一个**只转发不实现**的 `eframe::App`：调生产那个行画函数，抄出生产那个读数。
#[cfg(not(windows))]
struct RowProbeApp {
    rows: Vec<Listed>,
    shared: std::sync::Arc<std::sync::Mutex<RealEventProbe>>,
}

#[cfg(not(windows))]
impl eframe::App for RowProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 🔴 这一行与 `render_headless_with_events_and_text` 里那一行**逐字相同**，
        //    被测对象因此是同一个。
        let mut t = RenderTally::default();
        show_file_rows(ui, &self.rows, &mut t, Some(0.0), None, None);

        let ctx = ui.ctx().clone();
        let mut p = self.shared.lock().unwrap();
        p.frames += 1;
        p.screen_w = ctx.viewport_rect().width();
        p.screen_h = ctx.viewport_rect().height();
        p.ppp = ctx.pixels_per_point();
        p.materialized = t.rows_materialized;
        if let Some(i) = t.copy_clicked {
            p.copy_clicks += 1;
            p.last_copy = Some(i);
        }
        if let Some(i) = t.clicked {
            p.row_clicks += 1;
            p.last_row = Some(i);
        }
        ctx.input(|i| {
            if i.pointer.button_clicked(egui::PointerButton::Primary) {
                p.input_clicks += 1;
            }
            if i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
            {
                p.input_double_clicks += 1;
            }
            if let Some(pos) = i.pointer.interact_pos() {
                p.last_pointer = Some(pos);
            }
        });
        let close = p.close_now;
        drop(p);
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // 🔴 **连续画**：驱动线程点完之后要能立刻读到那一帧的读数。
        //    egui 默认只在有事发生时画下一帧（`真相源/99 §9.6` 那条教训同族），
        //    而这里读数是隔着线程取的 ⇒ 台架自己负责把帧推起来。
        //    ⚠ 这是**台架**的选择，不是生产的行为，别读成「生产在连续画」。
        ctx.request_repaint();
    }
}

/// 这台实景台架那个窗口的标题。
///
/// 🔴 **刻意全 ASCII** —— 现打栽过一次：标题带中文时
/// `xdotool search --name` 一个窗口都找不着（它按 C 区域设置编译那条正则），
/// 于是驱动线程等满 20 秒空手而归，而那一趟的读数长得像「点了没反应」。
/// ⇒ 那是**台架**塌了，不是被测那一半坏了。台架自己的名字不许给它添这一维。
#[cfg(not(windows))]
const REAL_EVENT_WINDOW_TITLE: &str = "ccm-filewin-real-event-probe";

/// 点名字那一段要双击的那一行（`one.bin`，有按钮、名字在按钮左边）。
#[cfg(not(windows))]
const REAL_EVENT_ROW_FOR_DOUBLE_CLICK: usize = 1;
/// 要点那颗「复制」的那一行（`two.bin`，`mixed_rows` 里第二颗按钮）。
#[cfg(not(windows))]
const REAL_EVENT_ROW_FOR_COPY: usize = 2;

/// **实景工作面**：真 X 事件打在生产那个行画函数上。
#[cfg(not(windows))]
#[test]
#[ignore = "实景工作面：由 a_real_pointer_click_on_the_copy_button_comes_back_as_that_row 在它自己的进程里点起来"]
fn xvfb_worker_real_pointer_events_on_a_row() {
    use super::testing::xvfb;
    use std::sync::{Arc, Mutex};
    let display = xvfb::child_display();
    let rows = mixed_rows();
    let screen = screen();

    // ── ① 先用**已经在用的那台合成台架**量出两个落点 ─────────────────────
    // 🔴 刻意复用 `render_headless_with_events_and_text`：它量出来的矩形正是
    //    第三刀那两条判据点的那两个点 ⇒ 实景这一趟点的是**同一个地方**，
    //    两条判据于是可比（一条喂合成事件、一条喂真事件，落点同一个）。
    let ctx = egui::Context::default();
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen, 0.0, Vec::new());
    let (probe_tally, painted) =
        render_headless_with_events_and_text(&ctx, &rows, screen, 0.1, Vec::new());
    let buttons = rects_of(&painted, COPY_LABEL.as_str());
    let names = rects_of(&painted, &rows[REAL_EVENT_ROW_FOR_DOUBLE_CLICK].name);
    xvfb::emit("b.buttons_found", buttons.len());
    xvfb::emit("b.names_found", names.len());
    xvfb::emit("b.probe_materialized", probe_tally.rows_materialized);
    assert_eq!(
        buttons.len(),
        3,
        "合成台架上没量到那三颗按钮 —— 落点算不出来，这一格判不了"
    );
    assert_eq!(names.len(), 1, "合成台架上没量到那一行的名字 —— 同上");
    // `mixed_rows` 里能复制的是第 0、1、2 行（〔W5-FILES〕目录也能复制）⇒ 第三颗按钮是第 2 行的。
    let copy_at = buttons[2].center();
    let name_at = names[0].center();
    xvfb::emit("b.copy_at", format!("{:.1},{:.1}", copy_at.x, copy_at.y));
    xvfb::emit("b.name_at", format!("{:.1},{:.1}", name_at.x, name_at.y));

    // ── ② 真窗口 ───────────────────────────────────────────────────────
    let shared = Arc::new(Mutex::new(RealEventProbe::default()));
    let app = RowProbeApp {
        rows: rows.clone(),
        shared: Arc::clone(&shared),
    };
    let driver_shared = Arc::clone(&shared);
    let driver_display = display.clone();
    let driver = std::thread::spawn(move || {
        drive_real_pointer(&driver_display, &driver_shared, copy_at, name_at)
    });

    let opts = eframe::NativeOptions {
        // 生产那个 hook，一个字没换 —— 次线程上建事件循环走的是同一条路。
        event_loop_builder: Some(Box::new(crate::shell::any_thread_hook)),
        viewport: egui::ViewportBuilder::default()
            // 🔴 与 ① 量按钮时用的那张屏**同尺寸** —— 两把尺子必须同轴。
            .with_inner_size([screen.x, screen.y])
            .with_title(REAL_EVENT_WINDOW_TITLE),
        ..Default::default()
    };
    let ran = eframe::run_native(
        REAL_EVENT_WINDOW_TITLE,
        opts,
        Box::new(move |_cc| Ok(Box::new(app))),
    );
    xvfb::emit(
        "b.run_native",
        match &ran {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("err:{e}"),
        },
    );
    for line in driver
        .join()
        .unwrap_or_else(|_| vec!["驱动线程炸了".into()])
    {
        println!("  驱动线程：{line}");
    }

    let p = shared.lock().unwrap();
    xvfb::emit("b.frames", p.frames);
    xvfb::emit("b.screen", format!("{:.0}x{:.0}", p.screen_w, p.screen_h));
    xvfb::emit("b.ppp", format!("{:.3}", p.ppp));
    xvfb::emit("b.materialized", p.materialized);
    xvfb::emit("b.copy_clicks", p.copy_clicks);
    xvfb::emit(
        "b.copy_row",
        p.last_copy
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".into()),
    );
    xvfb::emit("b.row_clicks", p.row_clicks);
    xvfb::emit("b.input_clicks", p.input_clicks);
    xvfb::emit("b.input_double_clicks", p.input_double_clicks);
    xvfb::emit(
        "b.last_pointer",
        p.last_pointer
            .map(|q| format!("{:.1},{:.1}", q.x, q.y))
            .unwrap_or_else(|| "none".into()),
    );
    xvfb::emit(
        "b.row_row",
        p.last_row
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".into()),
    );
}

/// 驱动那一侧：等窗口起来 → 把真事件打进去 → 让窗口自己关掉。回一路日记。
///
/// ⚠ 它跑在**另一条线程**上：`run_native` 在调用它的那条线程上是阻塞的。
#[cfg(not(windows))]
fn drive_real_pointer(
    display: &str,
    shared: &std::sync::Arc<std::sync::Mutex<RealEventProbe>>,
    copy_at: egui::Pos2,
    name_at: egui::Pos2,
) -> Vec<String> {
    use super::testing::xvfb;
    let mut log = Vec::new();
    let sleep = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));

    // 等「窗口在屏幕上」且「已经画过几帧」两件同时成立。
    let ids = xvfb::wait_for_windows(display, REAL_EVENT_WINDOW_TITLE, 20_000);
    log.push(format!("窗口 {ids:?}"));
    let Some(id) = ids.first().cloned() else {
        // 关不掉的话 `run_native` 永远不回，那一格会以超时收场 —— 让它快点收。
        shared.lock().unwrap().close_now = true;
        log.push("一个窗口都没等到".into());
        return log;
    };
    for _ in 0..100 {
        if shared.lock().unwrap().frames >= 5 {
            break;
        }
        sleep(50);
    }
    let geom = match xvfb::geometry(display, &id) {
        Ok(g) => g,
        Err(e) => {
            shared.lock().unwrap().close_now = true;
            log.push(format!("几何问不到：{e}"));
            return log;
        }
    };
    let ppp = shared.lock().unwrap().ppp;
    log.push(format!("几何 {geom:?} · 缩放 {ppp}"));

    // 点：逻辑点 × 缩放 ＋ 窗口在根坐标里的原点。
    let at = |p: egui::Pos2| -> (String, String) {
        (
            ((geom.x as f32) + p.x * ppp).round().to_string(),
            ((geom.y as f32) + p.y * ppp).round().to_string(),
        )
    };
    // 🔴 反空真：**点之前**那两个计数必须是 0（否则下面的「等于 1」是个摆设）。
    {
        let p = shared.lock().unwrap();
        xvfb::emit("b.copy_clicks_before", p.copy_clicks);
        xvfb::emit("b.row_clicks_before", p.row_clicks);
    }

    // ── 单击那颗「复制」 ────────────────────────────────────────────────
    let (cx, cy) = at(copy_at);
    log.push(format!("点「复制」→ 根坐标 {cx},{cy}"));
    if let Err(e) = xvfb::xdotool_on(display, &["mousemove", "--sync", &cx, &cy]) {
        log.push(format!("移不过去：{e}"));
    }
    sleep(200);
    if let Err(e) = xvfb::xdotool_on(display, &["click", "1"]) {
        log.push(format!("点不下去：{e}"));
    }
    for _ in 0..60 {
        if shared.lock().unwrap().copy_clicks > 0 {
            break;
        }
        sleep(50);
    }

    // ── 双击名字那一段 ─────────────────────────────────────────────────
    let (nx, ny) = at(name_at);
    log.push(format!("双击名字 → 根坐标 {nx},{ny}"));
    if let Err(e) = xvfb::xdotool_on(display, &["mousemove", "--sync", &nx, &ny]) {
        log.push(format!("移不过去：{e}"));
    }
    sleep(200);
    // 🔴 两下**分两趟发，中间由这一侧掐一个 80 毫秒的表**。现打栽过两次，
    //    两次方向相反，所以这个间隔是**夹出来**的，不是随手写的：
    //    · `click --repeat 2 --delay 60`：三下 egui **全认成单击**
    //      （`input_double_clicks=0`）⇒ 两下之间真实间隔越过了 egui 的双击时间窗；
    //    · 一趟 `xdotool` 里串 `mousedown/mouseup ×2`：名字那一下只数出 **1** 次点击
    //      ⇒ 四条全落进**同一帧**，egui 的点击状态机只认出一次。
    //    ⇒ 要的是「**跨帧**，但在双击窗以内」。这台机器上一帧约 1 毫秒
    //      （本趟 `b.frames` 自己印着），80 毫秒**两侧都有量级余量**。
    //    ⚠ 它是**这台机器上的**一个夹值，不是常量；歪了会以
    //      「台架没造出双击 ⇒ 判不了」出声，**不会**冒充「按钮死了」。
    for nth in 1..=2 {
        if let Err(e) = xvfb::xdotool_on(display, &["click", "1"]) {
            log.push(format!("第 {nth} 下点不下去：{e}"));
        }
        if nth == 1 {
            sleep(80);
        }
    }
    for _ in 0..60 {
        if shared.lock().unwrap().row_clicks > 0 {
            break;
        }
        sleep(50);
    }

    sleep(150);
    shared.lock().unwrap().close_now = true;
    log
}

/// 这一趟实景子进程的读数（**只跑一趟**，下面几条判据共用）。
#[cfg(not(windows))]
fn scenario_b() -> &'static super::testing::xvfb::ChildRun {
    use super::testing::xvfb;
    static RUN: std::sync::OnceLock<xvfb::ChildRun> = std::sync::OnceLock::new();
    RUN.get_or_init(|| {
        xvfb::require_toolbox("「真事件下点得动」");
        let screen = xvfb::Screen::start()
            .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
        xvfb::run_scenario(
            screen.display(),
            "filewin::rows::tests::xvfb_worker_real_pointer_events_on_a_row",
        )
    })
}

/// 🔴 **两把尺子同轴** —— 这一条先判，否则下面两条点的是别的地方。
///
/// 按钮的落点是拿**合成台架**量的（同一个进程、同一套字体、同一张屏尺寸），
/// 而实景那一趟点的是**根坐标**。中间那两步换算只有在
/// 「真窗口的 `viewport_rect` == 量按钮时那张屏」且「缩放恰好 1」时才成立。
///
/// ⚠ 不成立就报「这一格判不了」，**不报「过了」** —— 同 `真相源/99 §9.6`
/// 那条口径（那把尺子在这台机器上量不了，就说出来）。
#[cfg(not(windows))]
#[test]
fn the_real_window_and_the_synthetic_ruler_share_one_coordinate_system() {
    let run = scenario_b();
    run.must_have_passed("「真事件下点得动」");
    let want = format!("{:.0}x{:.0}", screen().x, screen().y);
    assert_eq!(
        run.reading("b.screen"),
        want,
        "真窗口的 `viewport_rect` 是 {} 而量按钮用的那张屏是 {want} —— \
         两把尺子不同轴，这一格**判不了**（不是过了）",
        run.reading("b.screen")
    );
    assert_eq!(
        run.reading("b.ppp"),
        "1.000",
        "缩放因子是 {} 而不是 1 —— 逻辑点换物理像素那一步不可靠，这一格**判不了**",
        run.reading("b.ppp")
    );
    // 反空真：生产那个行画函数在**真窗口里**真的画了那四行
    //（相等断言；合成台架那一侧同一个数由 `b.probe_materialized` 带出来）。
    assert_eq!(
        run.reading("b.materialized"),
        "4",
        "真窗口里物化的行数是 {} 而不是 4 —— 量的不是同一份语料",
        run.reading("b.materialized")
    );
    assert_eq!(
        run.reading("b.materialized"),
        run.reading("b.probe_materialized"),
        "合成台架与真窗口物化的行数不等 —— 两边画的不是同一个东西"
    );
    assert_eq!(
        run.reading("b.run_native"),
        "ok",
        "窗口自己关掉自己之后事件循环没干净退出：{}",
        run.reading("b.run_native")
    );
    let frames: u64 = run.reading("b.frames").parse().unwrap_or(0);
    assert!(frames >= 5, "只画了 {frames} 帧 —— 这一趟窗口压根没跑起来");
}

/// 🔴 **一次真 X 鼠标点击打在那颗「复制」上，回来的是那一行的下标。**
///
/// 这是第三刀那条真缺陷（整行那块命中矩形把按钮吃掉）的**实景**钉子：
/// 此前它只被合成事件钉着，`真相源/99 §10.1` 逐字写着
/// 「真机上鼠标点那颗『复制』会不会触发 —— **判不了**」。
/// ⇒ 这一格从此有数：真 X 事件 → winit → egui → 生产那个行画函数。
///
/// ⚠ 计数按**相等**判（恰好 1 次），不是「至少 1 次」：
/// 一次点击被认成两三次（按下/松开各算一次那一族失效）在只看下标时分不开。
#[cfg(not(windows))]
#[test]
fn a_real_pointer_click_on_the_copy_button_comes_back_as_that_row() {
    let run = scenario_b();
    run.must_have_passed("「真事件下点得动那颗按钮」");
    assert_eq!(
        run.reading("b.copy_clicks_before"),
        "0",
        "还没点的时候那个计数就已经是 {} —— 下面那条「恰好 1」是个摆设",
        run.reading("b.copy_clicks_before")
    );
    assert_eq!(
        run.reading("b.copy_row"),
        REAL_EVENT_ROW_FOR_COPY.to_string(),
        "真鼠标点在第 {REAL_EVENT_ROW_FOR_COPY} 行那颗「{COPY_LABEL}」上，\
         认出来的却是 {:?} —— `none` 多半是整行那块命中矩形把按钮盖住了\
         （它是后登记的，平手时它赢）",
        run.reading("b.copy_row"),
        COPY_LABEL = COPY_LABEL.as_str()
    );
    assert_eq!(
        run.reading("b.copy_clicks"),
        "1",
        "一次真点击被认成了 {} 次",
        run.reading("b.copy_clicks")
    );
}

/// 🔴 **另一侧**：真事件**到得了** —— 而「真双击认成那一行」这一维**判不了**。
///
/// # 这一条刻意拆成「买到的」与「判不了的」两半，不合成一个绿
///
/// `真相源/99 §10.4` 刀 1 的读数逐字记着「**它只钉红了一侧**，那正是要的证据」：
/// 整行那块矩形拉满 ⇒ 只有按钮那条红；整行那块干脆取消 ⇒ 只有这条红。
/// ⇒ 这一侧本来该由「真双击回那一行」来钉。**今天钉不上**，理由在下面。
///
/// ## ✅ 它买到的（都是相等断言）
///
/// - **三次真 X 点击全都走到了 egui 的输入层**（`b.input_clicks == 3`）——
///   X → winit → egui 那一段是通的。此前这一段**一趟都没跑过**
///   （所有交互判据喂的都是合成 `RawInput`）。
/// - **坐标换算端到端对得上**：指针最后落在 egui 眼里的逻辑点，与判据
///   拿合成台架量出来的那个点**逐位相等**。⇒ 「物理像素 → 逻辑点」
///   那一步没歪 —— 而按钮那一格的绿正是压在这一步上。
///
/// ## 🔴 判不了的（缺什么证据，写死）
///
/// **「真的双击一下那一行，`double_clicked()` 会不会回那一行」—— 判不了。**
/// 缺的是**一台能把两次真按下打进 egui 那个双击时间窗的注入器**。
/// 本机 `xdotool` 三种打法现打全试过，egui 输入层的双击标志**恒 false**：
///
/// | 打法 | egui 输入层数到的点击 | 双击 |
/// |---|---:|---:|
/// | `click --repeat 2 --delay 60` | 3 | **0** |
/// | 一趟里串 `mousedown/mouseup` ×2 | 2（名字那两下塌成一下） | **0** |
/// | 分两趟发、中间掐 80 毫秒 | 3 | **0** |
///
/// ⇒ 两个方向都撞墙：发太快 ⇒ 四条事件落进同一帧，egui 的点击状态机只认出一次；
/// 发太慢 ⇒ 越过双击时间窗。**中间那一档本台架打不出来**（每趟 `xdotool`
/// 要起一个进程，抖动本身就和那个窗同量级）。
/// ⇒ 要补这一格，缺的是**在进程内直接发 XTEST 的路子**（一条 X 绑定），
/// 而本仓不许为一条判据加依赖 ⇒ **如实欠着，不假装覆盖了。**
///
/// ⚠ **别把这一格读成「那一侧没人看着」**：合成事件那一层照旧钉着它
/// （`making_way_for_the_button_does_not_kill_the_rest_of_the_row`）。
/// 这里欠的只是**真事件**那一层的同一条。
#[cfg(not(windows))]
#[test]
fn real_pointer_events_reach_egui_and_the_double_click_side_is_still_unjudgeable() {
    let run = scenario_b();
    run.must_have_passed("「真事件到得了」");

    // ── ✅ 买到的一：三次真点击全都走到了 egui 的输入层 ──────────────────
    assert_eq!(
        run.reading("b.input_clicks"),
        "3",
        "打进去三次真 X 点击，egui 的输入层只数到 {} 次 ——          X → winit → egui 那一段漏事件了",
        run.reading("b.input_clicks")
    );
    // ── ✅ 买到的二：坐标换算端到端对得上（相等断言）────────────────────
    assert_eq!(
        run.reading("b.last_pointer"),
        run.reading("b.name_at"),
        "真指针最后落在 egui 眼里的逻辑点是 {}，而判据瞄的是 {} ——          「物理像素 → 逻辑点」那一步歪了（按钮那一格的绿也压在这一步上）",
        run.reading("b.last_pointer"),
        run.reading("b.name_at")
    );

    // ── 🔴 判不了的那一维：把读数印出来**登记**，不判 ────────────────────
    println!(
        "  〔登记·判不了〕真双击认不认得出那一行：egui 输入层双击标志 {} 次 ·          那一行 `double_clicked()` {} 次 · 回的是 {} —— 缺的证据见本判据头注",
        run.reading("b.input_double_clicks"),
        run.reading("b.row_clicks"),
        run.reading("b.row_row")
    );
}

// ═══════════════════════════════════════════════════════════════════
// Xvfb 台架自己的两条 —— 🔴 **量具会自己烂掉，所以量具也要有人看着**
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **占用判定必须分得清「有人在用」与「有人留了个死锁」。**
///
/// # 它治的是一条真发生过的病（2026-09-21 现打）
///
/// 上一版的判定是「`/tmp/.X<n>-lock` 在不在」两态。而台架的 `Drop` 只 `kill`
/// 掉 Xvfb、**没删那个锁**（SIGKILL 之下 X 服务器不自己收尾）⇒ **每跑一趟漏一个号**。
/// 号池只有 30 个 ⇒ **跑够 30 趟，那两格永久红**。
/// 现打当时：`:90`–`:119` **30/30 全被锁**，这些锁记的 pid **全死了**，机器上真在跑的
/// `Xvfb` 是 **0 个**。四趟里红两趟 —— 那不是抖动，是号池在那一刻见底。
///
/// 🔴 更贵的一层：**它红的样子是「这一格判不了」，而那与「这一格过了」在终端上分不开**
/// ⇒ 上一次门禁「27 格全绿」是在号池还没见底时取的读数。
///
/// # 为什么判纯函数而不判 `/tmp`
///
/// 往 `/tmp` 里种 30 个假锁会**砸掉同时并行跑的别的格**。
/// ⇒ 把判定抽成纯函数（内容 ＋ 一个「这个 pid 活着吗」的判定），
/// 两个方向都用合成输入打，**一个字节都不碰文件系统**。
// 台架整份是 `#[cfg(not(windows))]` ⇒ 本条在 Windows target 上导入解析不了。
// 〔现打：`winchk` 那一格交叉编到 `x86_64-pc-windows-gnu` 时 E0432〕
#[cfg(not(windows))]
#[test]
fn a_stale_lock_and_a_live_lock_are_told_apart() {
    use crate::rows::testing::xvfb::{classify_lock, Slot};

    // 阳性方向：pid 活着 ⇒ 别碰
    assert_eq!(
        classify_lock(Some("      1234\n"), |p| p == 1234),
        Slot::Live(1234),
        "锁记的 pid 活着，却没判成「有人在用」—— 会把别人正在用的屏回收掉"
    );
    // 🔴 阴性方向：pid 已死 ⇒ 可回收。**没有这一条，整条判据就是上一版那个恒「别碰」**
    assert_eq!(
        classify_lock(Some("      1234\n"), |_| false),
        Slot::Stale(1234),
        "锁记的 pid 已经不在，却没判成「可回收」—— 这个号从此永久报废，\
         而它报废的样子是「这一格判不了」，与「过了」在终端上分不开"
    );
    // 没有锁
    assert_eq!(classify_lock(None, |_| true), Slot::Free);
    // 🔴 读不懂的内容**一律保守判「有人在用」** —— 宁可放弃一个号，
    //    也不要把别人的屏当垃圾回收。
    for junk in ["", "  \n", "not-a-pid", "0", "-7"] {
        assert_eq!(
            classify_lock(Some(junk), |_| false),
            Slot::Live(0),
            "锁内容 {junk:?} 解不出 pid，却没保守判成「有人在用」"
        );
    }
}

/// 🔴 **台架收场之后不许留下锁文件** —— 这是「号池只减不增」那条病的另一头。
///
/// ⚠ 它买的是**行为**：真起一台 Xvfb、真析构、再看盘上。
/// 上面那条纯函数判据**买不到这一条**（它不碰文件系统）——
/// 两条各买一半，合起来才是「号能还回去」。
// 台架整份是 `#[cfg(not(windows))]` ⇒ 本条在 Windows target 上导入解析不了。
// 〔现打：`winchk` 那一格交叉编到 `x86_64-pc-windows-gnu` 时 E0432〕
#[cfg(not(windows))]
#[test]
fn the_rig_leaves_no_lock_behind_when_it_is_dropped() {
    use crate::rows::testing::xvfb;
    xvfb::require_toolbox("「台架收场不漏锁」");

    let (display, lock) = {
        let screen = xvfb::Screen::start()
            .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
        let d = screen.display().to_string();
        let num: u32 = d.trim_start_matches(':').parse().expect("显示号该是个数");
        let lock = format!("/tmp/.X{num}-lock");
        // 反空真：**这一趟真的产生过那个锁**，否则下面「锁没了」是句空话。
        assert!(
            std::path::Path::new(&lock).exists(),
            "Xvfb 起在 {d} 上，却没有 {lock} —— 那么下面那条「收场后锁没了」\
             会在一个从来不存在的东西上恒真"
        );
        (d, lock)
    }; // ← `screen` 在这里析构

    assert!(
        !std::path::Path::new(&lock).exists(),
        "台架在 {display} 上收场了，{lock} 还在 —— 这个号从此还不回去。\
         号池只有 30 个（`:90`–`:119`），漏够 30 次这一族判据就永久红，\
         而它红的样子是「判不了」，与「过了」在终端上长得一样"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第五刀 2026-09-21〕行上那三颗**写**按钮
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **相等断言**：三颗写按钮各自的处数 == `is_writable` 说的行数。
///
/// ⚠ 与「复制」那一条**刻意分开**：两个判准不同（目录能写、不能复制），
/// 合起来判会让「目录上少了删除」与「目录上多了复制」互相抵消。
#[test]
fn only_the_writable_rows_get_the_three_write_buttons_painted() {
    use crate::writeops::{is_writable, CHMOD_LABEL, DELETE_LABEL, RENAME_LABEL};
    let ctx = egui::Context::default();
    let rows = mixed_rows();
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
    let (tally, painted) =
        render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());
    assert_eq!(tally.rows_materialized, 4, "四行没画全，下面的数就没意义");
    assert!(
        !painted.is_empty(),
        "这一帧一个字都没画出来 —— 量具塌了，下面那一比在空转"
    );

    let want = rows.iter().filter(|r| is_writable(r)).count();
    assert_eq!(want, 3, "语料自己变了：能写的行数应当是 3（目录也能写）");
    for label in [
        RENAME_LABEL.as_str(),
        DELETE_LABEL.as_str(),
        CHMOD_LABEL.as_str(),
    ] {
        let got = rects_of(&painted, label);
        assert_eq!(
            got.len(),
            want,
            "画出来 {} 颗「{label}」，而 `is_writable` 说有 {want} 行能写 —— \
             有损名那一档上长出了一颗不该有的按钮（或者能写的那几行少了一颗）",
            got.len()
        );
        // 反空真：它们在三行不同的位置上（不是同一颗被数了三遍）。
        assert!(got[0].center().y < got[1].center().y);
        assert!(got[1].center().y < got[2].center().y);
    }
    // 〔W5-FILES〕目录那一行：三颗写按钮**有**，「复制」**也有**了（后端 `recursive: true`，`设计/60 §6.2`）。
    //   在这份语料上两个判准重合（有损名那一行没带字节，两边都不给）；分得开的那一格住 `writeops_tests`（带字节的有损名能写、不能复制）。
    let copies = rects_of(&painted, COPY_LABEL.as_str());
    assert_eq!(copies.len(), 3, "能复制的行数应当是 3");
    let renames = rects_of(&painted, RENAME_LABEL.as_str());
    assert!(
        (renames[0].center().y - copies[0].center().y).abs() < 1.0,
        "第 0 行（目录）上「{}」与「{}」不在同一行 —— 目录那一行少了一颗",
        RENAME_LABEL.as_str(),
        COPY_LABEL.as_str()
    );
}

/// 🔴 **这一刀最要紧的判据**：点那三颗写按钮，回来的各是**那一行**的下标。
///
/// # 它同时钉着第三刀那条真缺陷的第五刀版本
///
/// 整行那块命中矩形是**后**登记的，egui 在平手时逐字「take the last one」
/// ⇒ 它盖住的按钮**永远点不到**。第三刀让开了「复制」那一颗；第五刀一行上有四颗，
/// **只让开一颗的话另外三颗照旧是死的** —— 而它们是写操作。
/// ⇒ `paint_one_row` 取的是那几颗里**最左**那个左边界（`RowButtons::leftmost_left`），
/// 而本条逐颗点一遍。少了它，把那个 `min` 写成「让开复制那一颗」不会红。
#[test]
fn clicking_each_write_button_comes_back_as_that_rows_index() {
    use crate::writeops::{CHMOD_LABEL, DELETE_LABEL, RENAME_LABEL};
    let rows = mixed_rows();
    // 🔴🔴 **两行都要点，而「目录那一行」是承重的那一行。**
    //
    // 〔死值验 `M7` 现打逼出来的〕第一版只点第 2 行（`two.bin`，一个**文件**）。
    // 把 `RowButtons::leftmost_left` 改成「只看复制那一颗」⇒ **一条判据都没红**，
    // 而那个改动是有后果的：文件行上「复制」按布局本来就是最左那一颗
    // ⇒ 在**有复制**的那几行上，那个 `min` 取谁都一样，那一版是个恒真的判据。
    // 真正会坏的是**目录那一行** —— 它没有「复制」（`copy-data` 吃文件句柄）
    // ⇒ `leftmost_left` 回 `None` ⇒ 整行那块矩形拉满整行宽 ⇒ 它那三颗写按钮
    // **全成了死的**：目录点得到「删除」这件事悄悄没了，而目录删除是不可撤销的。
    // ⇒ 本条现在逐行点：第 0 行（目录，**没有**复制）＋ 第 2 行（文件，有复制）。
    for (row_index, button_slot) in [(0usize, 0usize), (2usize, 2usize)] {
        for (label, pick) in [
            (
                RENAME_LABEL.as_str(),
                (|t: &RenderTally| t.rename_clicked) as fn(&RenderTally) -> Option<usize>,
            ),
            (DELETE_LABEL.as_str(), |t: &RenderTally| t.delete_clicked),
            (CHMOD_LABEL.as_str(), |t: &RenderTally| t.chmod_clicked),
        ] {
            let ctx = egui::Context::default();
            let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
            let (_, painted) =
                render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());
            let buttons = rects_of(&painted, label);
            assert_eq!(
                buttons.len(),
                3,
                "没找到那三颗「{label}」，下面按坐标点没意义"
            );
            // 那三颗按出现顺序对应第 0 / 1 / 2 行（第 3 行是有损名，不画）。
            let pos = buttons[button_slot].center();
            let _ = render_headless_with_events(
                &ctx,
                &rows,
                screen(),
                0.2,
                vec![egui::Event::PointerMoved(pos)],
            );
            let hit = render_headless_with_events(&ctx, &rows, screen(), 0.3, click_at(pos));
            assert_eq!(
                pick(&hit),
                Some(row_index),
                "点在第 {row_index} 行那颗「{label}」上，回来的却是 {:?} —— \
                 `None` 多半是整行那块命中矩形把它盖住了（它是后登记的，平手时它赢）；\
                 这一颗是**写**操作，点不到就等于这条功能没接上。\
                 ⚠ 第 0 行是**目录**（没有「复制」那一颗）—— 死值验 `M7` 证明\
                 只让开「复制」时坏的正是这一行",
                pick(&hit)
            );
            // 点一颗按钮不许同时被读成「双击进目录」，也不许串到别的那几颗上。
            assert_eq!(hit.clicked, None, "点一颗按钮竟然还顺手进了目录");
            let others: Vec<Option<usize>> = [
                hit.copy_clicked,
                hit.rename_clicked,
                hit.delete_clicked,
                hit.chmod_clicked,
            ]
            .into_iter()
            .filter(|x| *x == Some(row_index))
            .collect();
            assert_eq!(
                others.len(),
                1,
                "点一颗「{label}」，却有 {} 个格子同时报了第 {row_index} 行 —— 四颗按钮串了线",
                others.len()
            );
        }
    }
}

/// 🔴 **另一侧**：让开四颗之后，名字那一段**照旧**双击进目录。
///
/// 没有这一条，「让开」可以退化成「整行那块矩形干脆不要了」——
/// 那样上面那几条照样绿，而双击进目录悄悄没了（同第三刀那条的形状）。
#[test]
fn making_way_for_four_buttons_does_not_kill_the_rest_of_the_row() {
    let ctx = egui::Context::default();
    let rows = mixed_rows();
    let _ = render_headless_with_events(&ctx, &rows, screen(), 0.0, Vec::new());
    // 第 1 行（`one.bin`，四颗按钮都有）名字那一段：x=60 远在最左那颗按钮左边。
    let y = 1.5 * (ROW_HEIGHT + 4.0);
    let pos = egui::pos2(60.0, y);
    let _ = render_headless_with_events(
        &ctx,
        &rows,
        screen(),
        0.1,
        vec![egui::Event::PointerMoved(pos)],
    );
    let _ = render_headless_with_events(&ctx, &rows, screen(), 0.2, click_at(pos));
    let second = render_headless_with_events(&ctx, &rows, screen(), 0.3, click_at(pos));
    let got = second
        .clicked
        .expect("名字那一段双击不回任何行 —— 整行那块命中矩形被让没了");
    assert!(
        got.abs_diff(1) <= 1,
        "点在第 1 行的名字上，认出来的是第 {got} 行"
    );
    assert_eq!(second.copy_clicked, None, "点名字竟然算成点了「复制」");
    assert_eq!(second.rename_clicked, None, "点名字竟然算成点了「改名」");
    assert_eq!(second.delete_clicked, None, "点名字竟然算成点了「删除」");
    assert_eq!(second.chmod_clicked, None, "点名字竟然算成点了「权限」");
}

/// 🔴〔`P25` 2026-09-22〕**号被别人抢走时，台架不许拿着别人的屏回来。**
///
/// # 这一条钉的是残余那条 flake 的病根
///
/// `slot(num)` 与「Xvfb 真建锁」之间有一段 TOCTOU；`NEXT_BASE` 只错开**起点**，
/// 两格的扫描路径照样会在下一步汇到同一个号。两格同时 spawn `Xvfb :N` 时，
/// **X 服务器自己的锁是仲裁者**：输的那台当场退出，stderr 逐字
/// `Cannot establish any listening sockets`＋`exit=1`（2026-09-22 现打）。
///
/// 🔴 **而「问几何」那一跳会成功** —— 答话的是赢家那台。
/// ⇒ 两个观测量**同时成立**，顺序一反（或者干脆不看第一个，那正是修之前的样子）
/// 就会返回一个「子进程已死、屏是别人的」`Screen`
/// ⇒ 两格窗口挤在一台屏上 ⇒ 数窗口那条判据数出 2 个，或赢家 `Drop` 杀屏 ⇒ `BadWindow`。
///
/// ⚠ 本条是**纯**的：不起任何 Xvfb、不撞号（同 `classify_lock` 那条的理由）。
/// 它买不到「真并发下确实不撞」—— 那要两台真服务器，而那一格由
/// `the_toolbox_hands_out_a_different_display_to_each_screen` 顶着。
#[test]
#[cfg(not(windows))]
fn a_display_number_won_by_someone_else_is_never_reported_as_ours() {
    use crate::rows::testing::xvfb::{judge_claim, Claim};
    // 🔴 承重的那一格：**我们那台死了，而几何答得出**（赢家在答话）。
    assert_eq!(
        judge_claim(true, true),
        Claim::TakenByAnother,
        "我们那台 Xvfb 已经退出，却因为「屏答得出几何」被当成了我们的 —— \
         那台答话的是赢家，两格的窗口会挤在一起"
    );
    // 死了、几何也答不出 ⇒ 同样不是我们的。
    assert_eq!(judge_claim(true, false), Claim::TakenByAnother);
    // 活着、答得出 ⇒ 是我们的。
    assert_eq!(judge_claim(false, true), Claim::Ours);
    // 活着、还没答得出 ⇒ 再等等（**不是**失败 —— Xvfb 要一会儿才就绪）。
    assert_eq!(judge_claim(false, false), Claim::NotReadyYet);
}

/// 🔴 **真并发**：两台 `Screen` 同时起，拿到的号必须是两个。
///
/// ⚠ 这一条真起两台 Xvfb（所以它要那两件现物，缺件照旧**红**不跳过）。
/// 它买的是上一条纯判据买不到的那一半：**修完之后真的不撞了**。
#[test]
#[cfg(not(windows))]
fn the_toolbox_hands_out_a_different_display_to_each_screen() {
    use crate::rows::testing::xvfb::{exclusive, require_toolbox, Screen};
    // 🔴 拿独占闸：本条**在它自己内部**真并发（一次起两台），
    //    但它不与那几格实景开窗重叠（逐条理由住 `xvfb::exclusive`）。
    let _guard = exclusive();
    require_toolbox("两台屏各拿一个号");
    // 同一条线程上连起两台 —— 号池那个原子计数器与「服务器自己的锁」两道都在射程里。
    let a = Screen::start().expect("第一台起不来");
    let b = Screen::start().expect("第二台起不来");
    assert_ne!(
        a.display(),
        b.display(),
        "两台屏拿到了同一个号 —— 那正是两格窗口挤在一起那一形"
    );
    // 两台都真的答得出几何（否则上面那一比可以靠「两个都是空串」成立）。
    for s in [&a, &b] {
        let g = s
            .xdotool(&["getdisplaygeometry"])
            .unwrap_or_else(|e| panic!("{} 答不出几何：{e}", s.display()));
        assert_eq!(
            g.split_whitespace().count(),
            2,
            "{} 的几何不是两个数：{g}",
            s.display()
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔补齐五项 2026-09-23〕符号链接那个标记 · 修改时间那一列
// ════════════════════════════════════════════════════════════════════════

/// 三行：一条链接（带时间）· 一个目录（带时间）· 一个文件（时间没送）。
fn linked_rows() -> Vec<Listed> {
    let mk = |name: &str, is_dir: bool, link: bool, t: Option<u64>| Listed {
        row: Row {
            name: name.to_string(),
            path: format!("/srv/{name}"),
            is_dir,
            size: 3,
            lossy_name: false,
        },
        link,
        mtime_secs: t,
        raw_name: None,
    };
    vec![
        mk("to-elsewhere", false, true, Some(1_700_000_000)),
        mk("adir", true, false, Some(951_782_400)),
        mk("plain.txt", false, false, None),
    ]
}

/// 🔴 **链接那一行有自己的字形，而且只有链接那几行有** —— 画出来的 `🔗` 个数 == 链接行数。
///
/// 反空真：目录与普通文件那两行的字形照旧各画一次（这把尺子认得出字形）。
#[test]
fn only_the_link_rows_get_the_link_mark_painted() {
    let ctx = egui::Context::default();
    let rows = linked_rows();
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
    let (tally, painted) =
        render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());
    assert_eq!(tally.rows_materialized, 3);
    let want = rows.iter().filter(|r| r.link).count();
    assert_eq!(want, 1, "语料自己变了");
    assert_eq!(
        rects_of(&painted, "🔗").len(),
        want,
        "链接标记的个数与链接行数不等"
    );
    assert_eq!(rects_of(&painted, "📁").len(), 1);
    assert_eq!(
        rects_of(&painted, "📄").len(),
        1,
        "链接那一行不该**同时**画成普通文件"
    );
}

/// 🔴 **修改时间那一列：送了就画、没送就一个字都不画** —— 画出来的时间串 == 送了的那几格。
///
/// ⚠ 判的是「没送 ⇒ 不画一个编出来的时间」：如果哪天有人把 `None` 兜底成 0，
/// 屏幕上就会多一条 `1970-01-01 00:00Z`，本条当场红（那一格在阴性对照里点了名）。
#[test]
fn the_time_column_shows_exactly_the_times_that_were_sent() {
    let ctx = egui::Context::default();
    let rows = linked_rows();
    let _ = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.0, Vec::new());
    let (_, painted) = render_headless_with_events_and_text(&ctx, &rows, screen(), 0.1, Vec::new());
    for r in &rows {
        if let Some(t) = r.mtime_secs {
            let s = crate::source::format_mtime(t);
            assert_eq!(
                rects_of(&painted, &s).len(),
                1,
                "`{}` 那一格时间 `{s}` 没画出来",
                r.name
            );
        }
    }
    let times = painted
        .iter()
        .filter(|(t, _)| t.ends_with('Z') && t.len() == 17)
        .count();
    assert_eq!(
        times,
        rows.iter().filter(|r| r.mtime_secs.is_some()).count(),
        "画出来的时间条数与送了时间的行数不等"
    );
    assert_eq!(
        rects_of(&painted, "1970-01-01 00:00Z").len(),
        0,
        "没送的那一格被兜底成了 1970"
    );
}
