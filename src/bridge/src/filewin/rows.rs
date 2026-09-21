//! `24e` 的列表：**虚拟滚动**的文件行。
//!
//! # 🔴 这个模块存在的全部理由，是那一个 API 名字
//!
//! `真相源/99 §2.3` 那组对照读数（**同一趟、同一台机器、同一份语料**）：
//!
//! | 行数 | `ScrollArea::show_rows`（虚拟） | `ScrollArea::show`（不虚拟） |
//! |---:|---:|---:|
//! | 10 000 | 0.90 ms | 4.16 ms |
//! | 100 000 | **0.90 ms** | **83.63 ms（12 fps）** |
//! | 640 413 | **0.90 ms** | 刻意没跑（按斜率是秒级/帧） |
//!
//! ⇒ `设计/60 §4 戊` 逐字：**「egui 扛得住」这句话的主语是 `show_rows`，不是 egui。
//! 用错 API，egui 一样死。**
//!
//! # ⚠ 所以判据必须钉住「用的是虚拟滚动」，而不是「盘上写着 show_rows」
//!
//! 一条 `grep -q show_rows` 只证明**盘上有**，不证明**被走到**
//! （本仓自己的说法：`K-R18` 语料八「盘上有 ≠ 被走到」）。
//! ⇒ [`tests`] 里那几条**真跑一趟 egui**（headless，`Context::run_ui`），
//! 数**这一趟到底物化了多少行**，并且：
//!
//! 1. **恒等**：1 000 行那档与 640 413 行那档物化的行数**相等**
//!    —— 这条性质 `show` 结构上给不出来（它必然是 O(n)）。
//! 2. **对照组就在判据里**：同一个测试里放一份**故意不虚拟**的实现，
//!    断言它在 100 000 行上物化 **100 000** 行 —— 证明这把尺子量得出差别，
//!    不是两边都恒真。

use egui::{ScrollArea, Ui};

use super::source::Row;

/// 一行的高度（不含 item spacing）。与 `真相源/99` 那趟原型同值，
/// 那趟的「一屏约 44 行 @ 1280×800」就是按这个数算的。
pub const ROW_HEIGHT: f32 = 18.0;

/// 这一趟画了什么 —— 判据靠它说话，生产也靠它做诊断。
///
/// 🔴 `rows_materialized` 是**这一趟 row-painter 真的被调用的次数**，
/// 不是「应该是多少」的推算值。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderTally {
    pub rows_materialized: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub total_rows: usize,
    /// 🔴 这一帧被点开的那一行的**下标**（`None` = 没人点）。
    ///
    /// **点击要从这里出来，不许在生产里另画一遍列表** —— [`show_file_rows`] 是
    /// 唯一一条画列表的路（见它的头注），所以「谁被点了」也只能从它带出来。
    /// 上一刀差点栽在同一形上：判据自己抄了一份 `ScrollArea`，于是它钉的是副本。
    pub clicked: Option<usize>,
}

/// 画一屏文件行。**这里是 `show_rows`，改成 `show` 会被判据当场逮住。**
///
/// 🔴 **这是画列表的唯一一条路** —— 生产（[`super::shell::FileWindow::ui`]）与
/// 判据（[`render_headless`]）调的是同一个函数。
/// ⚠ 这一条是**刻意的、承重的**：一开始 `render_headless` 自己抄了一份
/// `ScrollArea` 调用，于是「虚拟滚动」那条判据钉的是**判据自己那份副本**，
/// 生产那份改成 `show` 一条都不会红（死值验第 1 刀就是这么露出来的）。
/// **别再为了省一个参数把它抄回去。**
///
/// `scroll_offset_y`：`None` = 由 egui 自己管（生产）；`Some(y)` = 钉死偏移（量帧时用）。
pub fn show_file_rows(
    ui: &mut Ui,
    rows: &[Row],
    tally: &mut RenderTally,
    scroll_offset_y: Option<f32>,
) {
    tally.total_rows = rows.len();
    let mut area = ScrollArea::vertical().auto_shrink([false; 2]);
    if let Some(y) = scroll_offset_y {
        area = area.vertical_scroll_offset(y);
    }
    area.show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
        tally.first_row = range.start;
        tally.last_row = range.end;
        for i in range {
            let r = &rows[i];
            tally.rows_materialized += 1;
            if paint_one_row(ui, i, r) {
                tally.clicked = Some(i);
            }
        }
    });
}

/// 一行的长相。**刻意抽出来**：虚拟与不虚拟两条路要画的是同一样东西，
/// 否则对照组比的就不是「虚不虚拟」而是「画得多不多」。
///
/// 回值 = 这一帧这一行被**双击**了（同旧面板 `panel.ts` 的 `dblclick`，别让两个面板两套手感）。
///
/// ⚠ **刻意不在这里做「点了之后干什么」** —— 那是窗口状态机的事
/// （[`super::shell::FileWindow::activate`]），画一行的函数不许知道「换目录」这回事。
///
/// # 🔴 为什么是 `ui.interact(rect, 自己造的 Id, …)`，而不是 `响应.interact(…)`
///
/// 直觉写法是 `ui.horizontal(…).response.interact(Sense::click())`。
/// **那一版在 headless 下现打是死的**：同一趟里、同一个位置上，
/// `ui.button()` 拿得到 `hovered/clicked`，而 `ui.horizontal(…)` 那个**布局作用域
/// 响应**上再 `interact` 出来的那一份 `hovered` 恒 `false`
/// （逐帧读数见 `真相源/99 §9.1`）——
/// 命中测试在 `begin_pass` 时按上一帧的 widget 表做，而那条路上那个 id 没进到能被命中的那一档。
///
/// ⇒ 换成**给这一行自己造一个 `Id`、用 `ui.interact` 正经登记一个 widget**，
/// 当场活（`hov=true` / `click=true` / 第三帧 `dbl=true`）。
/// ⚠ 这不是「换个写法凑绿」：换之前那一版**在真窗口上也一样不接点击**，
/// 判据逮住的是一条真缺陷 —— 只是它在本机只能以 headless 的形式被看见。
///
/// ⚠ 买不到的那一半照旧写在这儿：**真机上鼠标双击能不能触发，本机判不了**
/// （`XDG_SESSION_TYPE=tty`，没有图形会话，也就没有真事件源）。
/// 这里买到的是「**egui 收到这样一串事件之后，认出来的是哪一行**」。
fn paint_one_row(ui: &mut Ui, index: usize, r: &Row) -> bool {
    let inner = ui.horizontal(|ui| {
        ui.label(if r.is_dir { "📁" } else { "📄" });
        ui.label(&r.name);
        if !r.is_dir {
            ui.label(human_size(r.size));
        }
        if r.lossy_name {
            // 非 UTF-8 名：SFTP 那侧寻址不到真字节 ⇒ 写操作要灰置。
            // 这一刀只做标记，灰置逻辑等有写操作那一刀。
            ui.label("⚠");
        }
    });
    // 整行都可点（不是只有名字那几个像素）—— 文件管理器的常规手感
    // ⇒ 命中矩形在横向拉满整行宽。
    let band = inner.response.rect;
    let full = egui::Rect::from_min_max(
        egui::pos2(ui.max_rect().left(), band.top()),
        egui::pos2(ui.max_rect().right().max(band.right()), band.bottom()),
    );
    // ⚠ `Id` 按**行下标**造（不是按名字）：下标随滚动是绝对的、且同一行跨帧稳定，
    //   而名字会重（同名文件在不同目录、或列表里刚好两行同名）。
    ui.interact(
        full,
        ui.id().with(("filewin-row", index)),
        egui::Sense::click(),
    )
    .double_clicked()
}

/// 人读的大小。**不是** `format!("{size}")` —— 列表里一列宽度有限。
pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Headless 跑一趟 egui 并收 [`RenderTally`]。
///
/// ⚠ **这是 CPU 段**：布局 ＋ 生成绘制命令 ＋ 文字整形，不含把三角形交给 GPU。
/// 理由与射程照 `真相源/99 §一`（本机 `XDG_SESSION_TYPE=tty`，没有图形会话）。
/// 那一节的旁证也照抄下来：虚拟滚动那一档三角形数与总行数无关
/// ⇒ 交给 GPU 的活不随行数涨，**它不是 64 万行的风险点**。
pub fn render_headless(
    ctx: &egui::Context,
    rows: &[Row],
    screen: egui::Vec2,
    scroll_offset_y: f32,
) -> RenderTally {
    let mut tally = RenderTally::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        // 🔴 调的是**生产那个函数**，不是它的副本 —— 见 `show_file_rows` 的注释。
        let mut t = RenderTally::default();
        show_file_rows(ui, rows, &mut t, Some(scroll_offset_y));
        tally = t;
    });
    out.drop_without_applying_deltas();
    tally
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/rows_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/rows_tests.rs"]
mod tests;
