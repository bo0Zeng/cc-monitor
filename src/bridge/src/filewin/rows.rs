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

use super::copy::{is_copyable, COPY_LABEL};
use super::source::Row;
use super::writeops::{is_writable, CHMOD_LABEL, DELETE_LABEL, RENAME_LABEL};

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
    /// 🔴〔第三刀〕这一帧哪一行的**「复制」**被点了（`None` = 没人点）。
    ///
    /// 与 [`Self::clicked`] **刻意分开两个值**：一个装「双击这一行」，一个装
    /// 「点这一行上那颗按钮」。合成一个就得再编一个「点的是什么」的枚举，
    /// 而那个枚举的两支在窗口那侧走的是两条完全不同的路（换目录 / 摆命名框）。
    pub copy_clicked: Option<usize>,
    /// 🔴〔第五刀〕这一帧哪一行的**「改名」**被点了。
    pub rename_clicked: Option<usize>,
    /// 🔴〔第五刀〕这一帧哪一行的**「删除」**被点了。
    pub delete_clicked: Option<usize>,
    /// 🔴〔第五刀〕这一帧哪一行的**「权限」**被点了。
    pub chmod_clicked: Option<usize>,
}

/// 命中那一摞这一趟画了什么。
///
/// # 🔴 它**刻意不是** [`RenderTally`]，而这一条是承重的
///
/// [`show_hit_rows`] 的头注写着「命中行一个可点控件都不画」，理由是那个下标
/// 索引的是**另一摞东西**（`listing.rows`）。在第五刀之前那句话**靠纪律守**：
/// 两条路共用 [`RenderTally`]，谁哪天在命中行上加一颗按钮、把下标塞进
/// `clicked`，编译器一声不吭，而后果是「点第 3 条命中 ⇒ 对当前目录第 3 行动手」。
///
/// 🔴 **第五刀把那个代价从「复制错地方」升级成「删错东西」**（行上多了三颗写按钮）
/// ⇒ 不许再靠纪律。本类型**连一个「谁被点了」的字段都没有**，于是
/// 「点一条命中之后干什么」这个问题**在类型上不存在** ——
/// 同 `run_drop` 那个 `FnOnce` 的先例：一半由编译器守。
///
/// ⚠ **如实登记为未做**：「点一条命中跳到它所在的目录」是个该有的功能，
/// 第四刀没做，第五刀也没做。它要的是「把命中那条路径解成 `(目录, 名字)`
/// 再换目录」，而不是把下标塞进另一摞的索引里。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitTally {
    pub rows_materialized: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub total_rows: usize,
}

/// [`paint_one_row`] 这一帧从一行上收到的东西。
///
/// 🔴 **回的是一个结构而不是 `bool`**：这一行现在有五处可点（整行 ＋ 四颗按钮），
/// 而 `bool` 只装得下一处 —— 其余要么被挤掉，要么靠 out 参数偷偷带出去。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowHit {
    /// 这一行被**双击**了（＝ 目录进去）。
    pub activated: bool,
    /// 这一行的「复制」被**单击**了。
    pub copy: bool,
    /// 〔第五刀〕这一行的「改名」被**单击**了。
    pub rename: bool,
    /// 〔第五刀〕这一行的「删除」被**单击**了。
    pub delete: bool,
    /// 〔第五刀〕这一行的「权限」被**单击**了。
    pub chmod: bool,
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
            let hit = paint_one_row(ui, i, r);
            if hit.activated {
                tally.clicked = Some(i);
            }
            if hit.copy {
                tally.copy_clicked = Some(i);
            }
            if hit.rename {
                tally.rename_clicked = Some(i);
            }
            if hit.delete {
                tally.delete_clicked = Some(i);
            }
            if hit.chmod {
                tally.chmod_clicked = Some(i);
            }
        }
    });
}

/// 〔第四刀〕画一屏**命中**。`files-find` 回来的那一摞路径。
///
/// # 🔴 为什么它不是 [`show_file_rows`]，而这一条又为什么不算「第二条画列表的路」
///
/// 本模块头注那条纪律（**画列表的路只许有一条**）管的是**带点击与写动作的文件行**：
/// 那条路上有「双击进目录」与行上那颗「复制」，两者都要有一个明确的归属 ——
/// 而命中行两者**都归不了**：
///
/// - **双击进目录**：命中是一条**任意深度的路径**，不是当前目录里的一项；
///   把它塞进 [`RenderTally::clicked`] 之后，`FileWindow::activate` 会拿这个下标
///   去索引 `listing.rows`（另一摞东西）⇒ **点第 3 条命中，进的是当前目录第 3 行**。
/// - **那颗「复制」**：`FileWindow::begin_copy` 拿 `self.cwd` 当目标目录，
///   而命中可能在别的目录下 ⇒ **复制到错的地方**，而且它是本窗口唯一的写动作。
///
/// ⇒ 这一刀的裁法是：命中行**一个可点控件都不画**（只有 `ui.label`），
/// 于是「点了之后干什么」这个问题在结构上不存在，不用靠纪律守。
/// ⚠ **如实登记为未做**：「点一条命中跳到它所在的目录」是个该有的功能，这一刀没做。
///
/// 🔴〔第五刀 2026-09-21〕**上面那句「不用靠纪律守」当初只有一半是真的。**
/// 两条路共用 [`RenderTally`] ⇒ 在这儿加一颗按钮、把下标塞进 `clicked`
/// 编译器一声不吭。第五刀在行上加了三颗**写**按钮（改名 · 删除 · 权限）
/// ⇒ 那个代价从「复制到错的地方」升级成「**删错东西**」。
/// ⇒ 本函数的收数口换成了 [`HitTally`]（**连一个「谁被点了」的字段都没有**），
/// 于是那句话从此**由编译器守**。逐条理由住那个类型的头注。
///
/// # 它与 [`show_file_rows`] 共享的那一条性质
///
/// 同一个 `show_rows`（**虚拟滚动**）、同一个 [`ROW_HEIGHT`]。
/// `limit` 默认 1000（`src/doc/IPC-PROTOCOL.md §10`）给了条数一个上界，
/// 但**那个上界不是这一侧给的** ⇒ 不许靠它偷懒用 `show`。
/// 由 `tests::the_hit_list_materializes_the_same_few_rows_no_matter_how_many_hits`
/// 钉成一条**相等**断言（同本模块那条虚拟滚动判据的形状）。
pub fn show_hit_rows(ui: &mut Ui, hits: &[String], tally: &mut HitTally) {
    tally.total_rows = hits.len();
    ScrollArea::vertical()
        .auto_shrink([false; 2])
        // 同一帧里可能还有那条目录列表的 `ScrollArea`（切换时两者不同时在），
        // 给它一撮自己的盐，免得两块区域抢同一个 id。
        .id_salt("filewin-hits")
        .show_rows(ui, ROW_HEIGHT, hits.len(), |ui, range| {
            tally.first_row = range.start;
            tally.last_row = range.end;
            for i in range {
                tally.rows_materialized += 1;
                ui.label(&hits[i]);
            }
        });
}

/// 一行的长相。**刻意抽出来**：虚拟与不虚拟两条路要画的是同一样东西，
/// 否则对照组比的就不是「虚不虚拟」而是「画得多不多」。
///
/// 回值见 [`RowHit`]：整行被**双击**（同旧面板 `panel.ts` 的 `dblclick`，
/// 别让两个面板两套手感）· 那颗「复制」被**单击**。
///
/// ⚠ **刻意不在这里做「点了之后干什么」** —— 那是窗口状态机的事
/// （[`super::shell::FileWindow::activate`] / [`super::shell::FileWindow::begin_copy`]），
/// 画一行的函数不许知道「换目录」「起一趟复制」这回事。
///
/// # 🔴〔第三刀〕那颗「复制」与整行那块命中矩形**会打架**，而且是按钮输
///
/// 直觉写法是「按钮照画，整行那块矩形照旧拉满整行宽」。**那样按钮是死的。**
/// egui 的命中测试在距离平手（两块矩形都盖着指针）时逐字
/// 「In case of a tie, take the last one = the one on top」
/// （那句话住 `egui-0.36.2/src/hit_test.rs`，在它挑「最近那个可点控件」的私有
/// 辅助函数里；⚠ 本仓刻意**不点那个函数的名字** —— 它是仓外符号，
/// 而散文里的裸符号名由 `structural_scan` 那两条判据管着，点了就得进登记表），
/// 而整行那块矩形是在 `ui.horizontal(…)` **之后**登记的 ⇒ 它永远赢。
/// ⇒ 按钮编得过、画得出、`clicked()` **恒 false**。
///
/// ⇒ 整行那块矩形的右边界**停在按钮左侧**（让开一个 `item_spacing.x`）。
/// 判据两侧都钉：点按钮要回按钮（[`RowHit::copy`]）、点名字那一段双击要回那一行
/// （[`RowHit::activated`]）—— 只钉一侧的话，把矩形改回拉满整行不会红。
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
fn paint_one_row(ui: &mut Ui, index: usize, r: &Row) -> RowHit {
    let inner = ui.horizontal(|ui| {
        ui.label(if r.is_dir { "📁" } else { "📄" });
        ui.label(&r.name);
        if !r.is_dir {
            ui.label(human_size(r.size));
        }
        if r.lossy_name {
            // 非 UTF-8 名：SFTP 那侧寻址不到真字节 ⇒ 写操作要灰置。
            // 这一档下面**四颗按钮一颗都不画**（`is_copyable` / `is_writable`）。
            ui.label("⚠");
        }
        // 〔第三刀〕「复制」——**只对能复制的那一档画**。`is_copyable` 是唯一住址，
        // 窗口状态机那一侧（`begin_copy`）问的是同一个函数。
        // ⚠ `small_button`：普通 `Button` 的最小高度是 `interact_size.y`（默认 18），
        //   一行只有 `ROW_HEIGHT` 高，撑高了行与行会叠在一起（下一行就点不准了）。
        let copy = if is_copyable(r) {
            Some(ui.small_button(COPY_LABEL))
        } else {
            None
        };
        // 🔴〔第五刀〕改名 · 删除 · 权限 —— **三颗都是写操作**。
        //   判准是 `is_writable`（有损名一律不画），而它与 `is_copyable`
        //   **刻意不是同一个函数**：目录能改名/删除/改权限，但不能零流量复制。
        let (rename, delete, chmod) = if is_writable(r) {
            (
                Some(ui.small_button(RENAME_LABEL)),
                Some(ui.small_button(DELETE_LABEL)),
                Some(ui.small_button(CHMOD_LABEL)),
            )
        } else {
            (None, None, None)
        };
        RowButtons {
            copy,
            rename,
            delete,
            chmod,
        }
    });
    let btns = inner.inner;
    // 整行都可点（不是只有名字那几个像素）—— 文件管理器的常规手感。
    // 🔴 **但右边界要停在那几颗按钮里**最左**那一颗**的左侧 —— 拉满整行宽的话，
    //    egui 在平手时取「最后登记的那个」，而这块矩形是后登记的
    //    ⇒ 按钮永远点不到（见上面头注）。
    //    ⚠〔第五刀〕这里原先只让开「复制」那一颗。第五刀之后一行上有四颗，
    //      只让开一颗 = 另外三颗**照旧点不到**，而它们是写操作
    //      ⇒ 取的是**最小**的那个左边界，而不是某一颗的。
    let band = inner.response.rect;
    let left = ui.max_rect().left();
    let right = match btns.leftmost_left() {
        Some(x) => x - ui.spacing().item_spacing.x,
        None => ui.max_rect().right().max(band.right()),
    };
    let full = egui::Rect::from_min_max(
        egui::pos2(left, band.top()),
        egui::pos2(right.max(left), band.bottom()),
    );
    // ⚠ `Id` 按**行下标**造（不是按名字）：下标随滚动是绝对的、且同一行跨帧稳定，
    //   而名字会重（同名文件在不同目录、或列表里刚好两行同名）。
    let row = ui.interact(
        full,
        ui.id().with(("filewin-row", index)),
        egui::Sense::click(),
    );
    RowHit {
        activated: row.double_clicked(),
        copy: btns.copy.is_some_and(|b| b.clicked()),
        rename: btns.rename.is_some_and(|b| b.clicked()),
        delete: btns.delete.is_some_and(|b| b.clicked()),
        chmod: btns.chmod.is_some_and(|b| b.clicked()),
    }
}

/// 这一行上真的画出来的那几颗按钮的响应（`None` = 这一档不画那一颗）。
///
/// 🔴 抽成一个结构是为了让 [`RowButtons::leftmost_left`] 有一个**唯一**的落点 ——
/// 「整行那块矩形让开到哪儿」这件事只许有一个算法。散着四个 `Option` 时，
/// 那个 `min` 写在 `paint_one_row` 里，而漏掉一颗**不会红**（漏掉的那颗
/// 只是点不到，它照样画得出来、编得过 —— 第三刀实测过这一形）。
struct RowButtons {
    copy: Option<egui::Response>,
    rename: Option<egui::Response>,
    delete: Option<egui::Response>,
    chmod: Option<egui::Response>,
}

impl RowButtons {
    /// 这几颗按钮里最靠左的那个左边界（`None` = 一颗都没画）。
    fn leftmost_left(&self) -> Option<f32> {
        [&self.copy, &self.rename, &self.delete, &self.chmod]
            .into_iter()
            .flatten()
            .map(|b| b.rect.left())
            .fold(None, |acc: Option<f32>, x| {
                Some(acc.map_or(x, |a| a.min(x)))
            })
    }
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
