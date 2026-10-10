//! `24e` 的列表：**虚拟滚动**的文件行。
//!
//! # 🔴 这个模块存在的全部理由，是那一个 API 名字
//!
//! 那组对照读数（**同一趟、同一台机器、同一份语料**）：
//!
//! | 行数 | `ScrollArea::show_rows`（虚拟） | `ScrollArea::show`（不虚拟） |
//! |---:|---:|---:|
//! | 10 000 | 0.90 ms | 4.16 ms |
//! | 100 000 | **0.90 ms** | **83.63 ms（12 fps）** |
//! | 640 413 | **0.90 ms** | 刻意没跑（按斜率是秒级/帧） |
//!
//! ⇒：**「egui 扛得住」这句话的主语是 `show_rows`，不是 egui。
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

use copy_core::copy_text;
use egui::{ScrollArea, Ui};

use super::kind;
use super::source::{Listed, Sort, SortBy};
use super::theme::metrics;
use super::theme::palette;

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
    /// 🔴 这一帧被**双击**的那一行的下标（`None` = 没人点）。
    ///
    /// **点击要从这里出来，不许在生产里另画一遍列表** —— [`show_file_rows`] 是
    /// 唯一一条画列表的路，所以「谁被点了」也只能从它带出来。
    pub clicked: Option<usize>,
    /// 🔴这一帧哪一行被画成了**「就是这个文件」**（`None` = 没有）。
    ///
    /// 高亮本身是一块背景色，量具只收**文字** ⇒ 这一格是那件事的可判读出，
    /// 与那块背景色在**同一处**写下（`paint_one_row` 里同一个 `if`）⇒ 两者不可能漂开。
    pub revealed_row: Option<usize>,
    /// 🔴这一帧哪一行被**单击**了，带着当时按着的修饰键。
    ///
    /// 与 [`Self::clicked`]（**双击** = 打开）刻意分开：单击改选中态，双击才动目录。
    pub picked_click: Option<(usize, egui::Modifiers)>,
    /// 🔴这一帧哪一行被**右键**点了（`None` = 没有）。菜单摆在哪儿由窗口读指针位置。
    pub menu_clicked: Option<usize>,
    /// 这一帧哪一行被**拖起**了（带出「从哪一行拖起」）。
    pub drag_started: Option<usize>,
    /// 🔴这一帧被画成「**选中**」的那几行（下标，按画的顺序；只含真画出来的那几行）。
    pub picked_rows: Vec<usize>,
    /// 🔴这一帧被画成「**键盘光标**」的那一行（`None` = 光标不在视野里 / 没有光标）。
    pub cursor_row: Option<usize>,
    /// 这一帧被画成**淡一级**（隐藏文件）的那几行（下标，按画的顺序）。
    pub faded_rows: Vec<usize>,
}

/// 一行在选中态里是什么样子 —— [`paint_one_row`] 要的那两格。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    pub picked: bool,
    pub cursor: bool,
}

/// 命中那一摞这一趟画了什么。
///
/// # 🔴 它**刻意不是** [`RenderTally`]，而这一条是承重的
///
/// 命中行的下标指的是**命中这一摞**，不是 `listing.rows`。两条路共用 [`RenderTally`] 的话，
/// 点第 3 条命中会被当成当前目录第 3 行（删错东西）。⇒ 命中行只交「点了 / 打开了第几条」，
/// 由窗口解成 `(目录, 名字)` 再跳过去；它在类型上够不着那几条写动作的胶水。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitTally {
    pub rows_materialized: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub total_rows: usize,
    /// 这一帧被打开（双击）的那一条命中（下标只指命中这一摞）。
    pub jump: Option<usize>,
    /// 这一帧被单击选中的那一条。
    pub picked: Option<usize>,
    /// 这一帧点了哪一列表头。
    pub sort_click: Option<super::find::SortCol>,
    /// 表尾「加载更多失败」那一行的「重试」被点了。
    pub retry_more: bool,
}

/// 结果表上的一行（名字 · 命中那几段 · 位置 · 修改时间 · 大小 —— 都是后端给的）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitRow {
    pub name: String,
    pub marks: Vec<(usize, usize)>,
    pub location: String,
    /// 修改时间那一格：后端写好的短写法（照抄）。
    pub mtime_text: Option<String>,
    pub size: Option<u64>,
    pub dir: bool,
    pub link: bool,
}

/// 结果表的尾巴。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitTail {
    /// 后面还有（滚到底接着取），不画尾行。
    More,
    /// 到底了：「已到底」。
    End,
    /// 往下取失败：「加载更多失败 ［重试］」。
    Failed,
}

/// 结果表四列的矩形：名称（吃剩下的）· 位置 · 修改时间 · 大小；窄了先收修改时间、再收位置。
fn hit_rects(row: egui::Rect) -> [egui::Rect; 4] {
    let w = row.width();
    let size = 90.0f32.min((w - MIN_NAME).max(0.0));
    let mtime = if w >= 560.0 { 110.0 } else { 0.0 };
    let loc = if w >= 420.0 {
        (w * 0.3).clamp(120.0, 320.0)
    } else {
        0.0
    };
    let name = (w - size - mtime - loc).max(0.0);
    let mut x = row.left();
    let mut out = [row; 4];
    for (k, cw) in [name, loc, mtime, size].into_iter().enumerate() {
        out[k] =
            egui::Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(x + cw, row.bottom()));
        x += cw;
    }
    out
}

/// 结果表的表头：名称（相关度时写「名称 · 按相关度」）· 位置 · 修改时间 · 大小；点了交出那一列（排是后端排的）。
fn show_hit_header(ui: &mut Ui, sort: super::find::FindSort) -> Option<super::find::SortCol> {
    use super::find::SortCol;
    let p = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0, p.border),
    );
    let mut picked = None;
    let cols = [
        SortCol::Name,
        SortCol::Location,
        SortCol::Mtime,
        SortCol::Size,
    ];
    for (k, (col, r)) in cols.into_iter().zip(hit_rects(rect)).enumerate() {
        if r.width() <= 0.0 {
            continue;
        }
        let resp = ui.interact(r, ui.id().with(("filewin-hit-col", k)), egui::Sense::CLICK);
        if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, p.hover);
        }
        if resp.clicked() {
            picked = Some(col);
        }
        let mine = sort.col == col || (col == SortCol::Name && sort.col == SortCol::Relevance);
        let mut text = match col {
            SortCol::Name if sort.col == SortCol::Relevance => {
                copy_text("rsFilewinRows.hitCol.relevance", &[])
            }
            SortCol::Name => column_label(SortBy::Name),
            SortCol::Location => copy_text("rsFilewinRows.hitCol.location", &[]),
            SortCol::Mtime => column_label(SortBy::Mtime),
            SortCol::Size | SortCol::Relevance => column_label(SortBy::Size),
        };
        if mine && sort.col != SortCol::Relevance {
            let caret = if sort.desc {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_UP
            };
            text = format!("{text} {caret}");
        }
        let color = if mine { p.text } else { p.text2 };
        let g = one_line(ui, text, r.width() - 2.0 * PAD, color);
        let x = if col == SortCol::Size {
            r.right() - PAD - g.size().x
        } else {
            r.left() + PAD
        };
        ui.painter().with_clip_rect(r).galley(
            egui::pos2(x, r.center().y - g.size().y / 2.0),
            g,
            color,
        );
    }
    picked
}

/// 画结果表（列表同一张表的样子）：表头 ＋ 虚拟滚动的命中行 ＋ 表尾。单击选中、双击打开。
///
/// 命中是一条**任意深度的路径**，不是当前目录里的一项 ⇒ 行上不接写动作，只交下标（[`HitTally`]）。
/// 同一个 `show_rows`（**虚拟滚动**）、同一个行高（`tests::the_hit_list_materializes_the_same_few_rows_no_matter_how_many_hits` 钉）。
pub fn show_hit_rows(
    ui: &mut Ui,
    hits: &[HitRow],
    sort: super::find::FindSort,
    picked: Option<usize>,
    tail: HitTail,
    tally: &mut HitTally,
) {
    tally.total_rows = hits.len();
    tally.sort_click = show_hit_header(ui, sort);
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let rows = hits.len() + usize::from(tail != HitTail::More);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            // 同一帧里可能还有那条目录列表的 `ScrollArea`（切换时两者不同时在），给它一撮自己的盐。
            .id_salt("filewin-hits")
            .show_rows(ui, m.row_h, rows, |ui, range| {
                tally.first_row = range.start;
                tally.last_row = range.end.min(hits.len());
                for i in range {
                    let Some(h) = hits.get(i) else {
                        tail_row(ui, tail, tally);
                        continue;
                    };
                    tally.rows_materialized += 1;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), m.row_h),
                        egui::Sense::hover(),
                    );
                    let row =
                        ui.interact(rect, ui.id().with(("filewin-hit", i)), egui::Sense::CLICK);
                    let band = rect.shrink2(egui::vec2(2.0, 1.0));
                    if picked == Some(i) {
                        ui.painter().rect_filled(band, m.radius_m, p.picked);
                    } else if row.hovered() {
                        ui.painter().rect_filled(band, m.radius_m, p.hover);
                    }
                    if row.clicked() {
                        tally.picked = Some(i);
                    }
                    if row.double_clicked() {
                        tally.jump = Some(i);
                    }
                    paint_hit(ui, rect, h);
                }
            });
    });
}

fn tail_row(ui: &mut Ui, tail: HitTail, tally: &mut HitTally) {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), m.row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(PAD);
            match tail {
                HitTail::End => {
                    ui.label(
                        egui::RichText::new(copy_text("rsFilewinRows.hitTail.end", &[]))
                            .color(p.text2),
                    );
                }
                HitTail::Failed => {
                    ui.label(
                        egui::RichText::new(copy_text("rsFilewinRows.hitTail.failed", &[]))
                            .color(p.error_text),
                    );
                    if ui
                        .button(copy_text("rsFilewinFind.action.retry", &[]))
                        .clicked()
                    {
                        tally.retry_more = true;
                    }
                }
                HitTail::More => {}
            }
        },
    );
}

/// 一行命中：图标 ＋ 名字（命中那几段加底色）· 位置（等宽、次级色）· 修改时间 · 大小（右齐）。
fn paint_hit(ui: &Ui, rect: egui::Rect, h: &HitRow) {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let [name_c, loc_c, mtime_c, size_c] = hit_rects(rect);
    let painter = ui.painter().clone();
    let k = kind::kind_of_name(&h.name, h.dir, h.link);
    let icon_color = if k == kind::Kind::Folder {
        p.accent
    } else {
        p.text2
    };
    let icon = painter.layout_no_wrap(
        kind::icon(k).to_string(),
        egui::FontId::proportional(m.icon),
        icon_color,
    );
    let ix = name_c.left() + PAD;
    painter.with_clip_rect(name_c).galley(
        egui::pos2(ix, rect.center().y - icon.size().y / 2.0),
        icon,
        icon_color,
    );
    let nx = ix + m.icon + m.space[3];
    let mut job = super::kit::marked(ui, &h.name, &h.marks, p.text);
    job.wrap = egui::text::TextWrapping {
        max_width: (name_c.right() - nx - PAD).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let g = painter.layout_job(job);
    painter.with_clip_rect(name_c).galley(
        egui::pos2(nx, rect.center().y - g.size().y / 2.0),
        g,
        p.text,
    );
    let mono = egui::TextStyle::Monospace.resolve(ui.style());
    let cell = |c: egui::Rect, text: String, font: Option<egui::FontId>, right: bool| {
        if text.is_empty() || c.width() <= 0.0 {
            return;
        }
        let mut job = egui::text::LayoutJob::simple_singleline(
            text,
            font.unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style())),
            p.text2,
        );
        job.wrap = egui::text::TextWrapping {
            max_width: (c.width() - 2.0 * PAD).max(0.0),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let g = painter.layout_job(job);
        let x = if right {
            c.right() - PAD - g.size().x
        } else {
            c.left() + PAD
        };
        painter.with_clip_rect(c).galley(
            egui::pos2(x, rect.center().y - g.size().y / 2.0),
            g,
            p.text2,
        );
    };
    cell(
        loc_c,
        h.location.clone(),
        Some(egui::FontId::new(mono.size.min(12.0), mono.family.clone())),
        false,
    );
    cell(
        mtime_c,
        h.mtime_text.clone().unwrap_or_default(),
        None,
        false,
    );
    cell(
        size_c,
        if h.dir {
            String::new()
        } else {
            h.size.map(human_size).unwrap_or_default()
        },
        None,
        true,
    );
}

/// 一行上的手势（整行一块命中矩形；行上没有按钮 —— 动作走右键菜单与键盘）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowHit {
    pub activated: bool,
    pub picked: Option<egui::Modifiers>,
    pub menu: bool,
    pub drag_started: bool,
}

// ── 详情视图的列 ──────────────────────────────────────────────────────────

/// 四列：名称（吃剩下的宽度）· 修改时间 · 类型 · 大小。宽度是窗口状态（关窗即没）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Columns {
    pub mtime: f32,
    pub kind: f32,
    pub size: f32,
    /// 「格」那一列（计划反查，稿 06）多宽：目录落在某一片的仓库里 ⇒ [`CELL_COL`]，别处 ⇒ 0（这一列不在）。窗口每帧按反查结果设。
    pub cell: f32,
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            mtime: 150.0,
            kind: 120.0,
            size: 90.0,
            cell: 0.0,
        }
    }
}

/// 「格」那一列的宽（稿 06：120）。
pub const CELL_COL: f32 = 120.0;

/// 一列最窄多少（拖窄到这里就停）。
pub const MIN_COL: f32 = 56.0;
/// 名称那一列最窄多少（别的列拖宽时不许把它挤没）。
pub const MIN_NAME: f32 = 140.0;
/// 表头一行的高度。
pub const HEADER_HEIGHT: f32 = 30.0;
/// 列与列之间、行首的内边距。
const PAD: f32 = 10.0;
/// 列宽拖手的半宽。
const GRIP: f32 = 4.0;

/// 列的先后（表头与每一行都按它排）。
pub const ORDER: [SortBy; 4] = [SortBy::Name, SortBy::Mtime, SortBy::Type, SortBy::Size];

impl Columns {
    /// 先把「格」那一列从最右切下来（它不在 · 放不下 ⇒ `None`，整行照旧给四列）。名称列至少留 [`MIN_NAME`]。
    pub fn split_cell(&self, row: egui::Rect) -> (egui::Rect, Option<egui::Rect>) {
        if self.cell <= 0.0 || row.width() - self.cell < MIN_NAME {
            return (row, None);
        }
        let x = row.right() - self.cell;
        (
            egui::Rect::from_min_max(row.min, egui::pos2(x, row.bottom())),
            Some(egui::Rect::from_min_max(egui::pos2(x, row.top()), row.max)),
        )
    }

    /// 一行（或表头）的矩形 → 四列各自的矩形（按 [`ORDER`]）。名称列吃剩下的，至少 [`MIN_NAME`]。
    /// 一行放不下（双栏 · 窗口窄）⇒ 按「大小 · 类型 · 修改时间」的先后留列：放不下整宽的那一列压到不窄于
    /// [`MIN_COL`]，再窄就收起（宽 0，悬停名字看它）；四列的总宽恒等于那一行（不画出界）。
    pub fn rects(&self, row: egui::Rect) -> [egui::Rect; 4] {
        let mut left = (row.width() - MIN_NAME).max(0.0);
        let mut w = [0.0f32; 3];
        for (k, want) in [(2, self.size), (1, self.kind), (0, self.mtime)] {
            let got = if want <= left {
                want
            } else if left >= MIN_COL {
                left
            } else {
                0.0
            };
            w[k] = got;
            left -= got;
        }
        let [mtime, kind, size] = w;
        let name = row.width() - mtime - kind - size;
        let mut x = row.left();
        let mut out = [row; 4];
        for (k, w) in [name, mtime, kind, size].into_iter().enumerate() {
            out[k] =
                egui::Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(x + w, row.bottom()));
            x += w;
        }
        out
    }

    /// 第 `k` 条分隔线（名称|时间 = 0 · 时间|类型 = 1 · 类型|大小 = 2）被拖了 `dx`：
    /// 线右边那一列让出 / 吃进这么多，线左边那一列（名称列吃剩下的）跟着变。
    pub fn drag(&mut self, k: usize, dx: f32) {
        let clamp = |v: f32| v.max(MIN_COL);
        match k {
            0 => self.mtime = clamp(self.mtime - dx),
            1 => {
                let d = dx.clamp(MIN_COL - self.mtime, self.kind - MIN_COL);
                self.mtime += d;
                self.kind -= d;
            }
            2 => {
                let d = dx.clamp(MIN_COL - self.kind, self.size - MIN_COL);
                self.kind += d;
                self.size -= d;
            }
            _ => {}
        }
    }
}

/// 列名。
pub fn column_label(by: SortBy) -> String {
    by.label()
}

/// 🔴 **表头**：点一列按它排（再点反向），拖两列之间那条线改列宽。回这一帧点了哪一列。
pub fn show_header(ui: &mut Ui, cols: &mut Columns, sort: Sort) -> Option<SortBy> {
    let p = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0, p.border),
    );
    let mut picked = None;
    let (rest, cell_r) = cols.split_cell(rect);
    if let Some(c) = cell_r {
        let g = one_line(
            ui,
            super::plan::column_label(),
            c.width() - 2.0 * PAD,
            p.text2,
        );
        ui.painter().with_clip_rect(c).galley(
            egui::pos2(c.left() + PAD, c.center().y - g.size().y / 2.0),
            g,
            p.text2,
        );
    }
    let rects = cols.rects(rest);
    for (k, by) in ORDER.into_iter().enumerate() {
        let r = rects[k];
        // 不进 Tab 顺序（`FOCUSABLE` 不要）：列表的键盘归窗口自己那一套，egui 的焦点落在这里会把方向键挡住。
        let resp = ui.interact(r, ui.id().with(("filewin-col", k)), egui::Sense::CLICK);
        if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, p.hover);
        }
        if resp.clicked() {
            picked = Some(by);
        }
        let mut text = column_label(by);
        let mine = sort.by == by;
        if mine {
            let caret = if sort.descending() {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_UP
            };
            text = format!("{text} {caret}");
        }
        let color = if mine { p.text } else { p.text2 };
        // 列太窄就截成「…」，悬停看全名。
        let g = one_line(ui, text.clone(), r.width() - 2.0 * PAD, color);
        if g.elided {
            resp.on_hover_text(text);
        }
        let x = if by == SortBy::Size {
            r.right() - PAD - g.size().x
        } else {
            r.left() + PAD
        };
        ui.painter().with_clip_rect(r).galley(
            egui::pos2(x, r.center().y - g.size().y / 2.0),
            g,
            color,
        );
    }
    // 三条分隔线（拖手画在表头上，列宽只在这里改）；收起的那一列两边不画。
    for k in 0..3 {
        if rects[k + 1].width() <= 0.0 {
            continue;
        }
        let x = rects[k].right();
        let grip = egui::Rect::from_min_max(
            egui::pos2(x - GRIP, rect.top()),
            egui::pos2(x + GRIP, rect.bottom()),
        );
        let resp = ui.interact(
            grip,
            ui.id().with(("filewin-col-grip", k)),
            egui::Sense::drag(),
        );
        let hot = resp.hovered() || resp.dragged();
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.dragged() {
            cols.drag(k, resp.drag_delta().x);
        }
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.top() + 7.0),
                egui::pos2(x, rect.bottom() - 7.0),
            ],
            egui::Stroke::new(1.0, if hot { p.accent } else { p.border }),
        );
    }
    picked
}

/// 画一屏文件行（详情视图）。**虚拟滚动**：只物化看得见的那几行。
///
/// 🔴 这是**唯一**一条画文件列表的路。行与行之间不留缝（`item_spacing.y = 0`），
/// 一行恰好一个行高（`theme::Metrics::row_h`）⇒ 「滚到第 i 行」的偏移是 `i × 行高`（[`row_pitch`]），算出来的，不是找出来的。
pub fn show_file_rows(
    ui: &mut Ui,
    rows: &[Listed],
    tally: &mut RenderTally,
    scroll_offset_y: Option<f32>,
    reveal: Option<&str>,
    picked: Option<&super::select::Selection>,
    cols: &Columns,
) {
    show_file_rows_with_tail(
        ui,
        rows,
        tally,
        scroll_offset_y,
        reveal,
        picked,
        cols,
        None,
        None,
        None,
    );
}

/// 就地输入那一格（就地改名 · 就地新建，稿 07 / 08）：第 `row` 行的名字那一格换成一个输入框。
///
/// 填进来的：正在编辑的字 · 出错那一句（红边 ＋ 格子下面挂一句）· 这一帧要不要把焦点给它、选中前几个字（主名）。
/// 交回去的：`outcome` ＝ `Some(true)` 回车 / 点别处（＝改）· `Some(false)` Esc（＝不改）。
/// 点在红字那块里（［复制详情］）不算点别处：不交 `outcome`，焦点还给输入框。
pub struct InlineCell<'a> {
    pub row: usize,
    pub text: &'a mut String,
    pub error: Option<&'a str>,
    /// 出错那一句的复制详情（空 ⇒ 红字块里不出按钮）。
    pub detail: &'a str,
    /// 点别处算不算提交：刚失败、名字没动过 ⇒ `false`（再发一趟只会再失败一次；回车照样重试）。
    pub blur_submits: bool,
    /// `Some(n)` ⇒ 这一帧把焦点给输入框并选中前 `n` 个字（打开那一帧 · 出错回来那一帧）。
    pub select: Option<usize>,
    /// 在飞（改名 / 新建那一趟还没回）：输入框只读，不再交 `outcome`。
    pub busy: bool,
    pub outcome: Option<bool>,
}

/// 同 [`show_file_rows`]，最后一行之后再画一行次级色的字（「有 3 项读不出来」那一句）。
#[allow(clippy::too_many_arguments)]
pub fn show_file_rows_with_tail(
    ui: &mut Ui,
    rows: &[Listed],
    tally: &mut RenderTally,
    scroll_offset_y: Option<f32>,
    reveal: Option<&str>,
    picked: Option<&super::select::Selection>,
    cols: &Columns,
    tail: Option<&str>,
    mut inline: Option<&mut InlineCell>,
    plan: Option<&super::plan::PlanDir>,
) {
    tally.total_rows = rows.len();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut area = ScrollArea::vertical().auto_shrink([false; 2]);
        if let Some(y) = scroll_offset_y {
            area = area.vertical_scroll_offset(y);
        }
        let row_h = metrics(ui.ctx()).row_h;
        let n = rows.len() + usize::from(tail.is_some());
        area.show_rows(ui, row_h, n, |ui, range| {
            tally.first_row = range.start;
            tally.last_row = range.end.min(rows.len());
            for i in range {
                let Some(r) = rows.get(i) else {
                    if let Some(t) = tail {
                        let p = palette(ui.ctx());
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add_space(PAD);
                                ui.label(egui::RichText::new(t).color(p.text2));
                            },
                        );
                    }
                    continue;
                };
                tally.rows_materialized += 1;
                let revealed = reveal.is_some_and(|want| want == r.name);
                if revealed {
                    tally.revealed_row = Some(i);
                }
                let mark = picked.map_or(Mark::default(), |s| Mark {
                    picked: s.is_picked(&super::select::pick_key(r)),
                    cursor: s.is_cursor(&super::select::pick_key(r)),
                });
                if mark.picked {
                    tally.picked_rows.push(i);
                }
                if mark.cursor {
                    tally.cursor_row = Some(i);
                }
                if kind::is_hidden(&r.name) {
                    tally.faded_rows.push(i);
                }
                let cell = inline.as_deref_mut().filter(|c| c.row == i);
                let hit = paint_one_row(ui, i, r, revealed, mark, cols, cell, plan);
                if let Some(m) = hit.picked {
                    tally.picked_click = Some((i, m));
                }
                if hit.menu {
                    tally.menu_clicked = Some(i);
                }
                if hit.drag_started {
                    tally.drag_started = Some(i);
                }
                if hit.activated {
                    tally.clicked = Some(i);
                }
            }
        });
    });
}

/// 一段字排成一行、超出 `width` 就截成「…」。
fn one_line(
    ui: &Ui,
    text: String,
    width: f32,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        color,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// 画一行：一整块命中矩形（单击选中 · 双击打开 · 右键菜单 · 拖起），底下按列摆图标 ＋ 名字 · 时间 · 类型 · 大小。
///
/// 选中是强调色的淡底；悬停一层淡底；键盘光标一圈强调色细线；隐藏文件字色淡一级。
fn paint_one_row(
    ui: &mut Ui,
    index: usize,
    r: &Listed,
    revealed: bool,
    mark: Mark,
    cols: &Columns,
    inline: Option<&mut InlineCell>,
    plan: Option<&super::plan::PlanDir>,
) -> RowHit {
    let p = palette(ui.ctx());
    let m = metrics(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), m.row_h),
        egui::Sense::hover(),
    );
    // 点 · 拖，但不进 Tab 顺序（同表头那一条理由）。
    let row = ui.interact(
        rect,
        ui.id().with(("filewin-row", index)),
        egui::Sense::CLICK | egui::Sense::DRAG,
    );
    let band = rect.shrink2(egui::vec2(2.0, 1.0));
    // 就地输入那一行画成选中（稿 08：冒出来的那一行带选中底）。
    if revealed || mark.picked || inline.is_some() {
        ui.painter().rect_filled(band, m.radius_m, p.picked);
    } else if row.hovered() {
        ui.painter().rect_filled(band, m.radius_m, p.hover);
    }
    if mark.cursor {
        ui.painter().rect_stroke(
            band,
            m.radius_m,
            egui::Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    let faded = kind::is_hidden(&r.name);
    let (main, minor) = if faded {
        (p.faint, p.faint)
    } else {
        (p.text, p.text2)
    };
    let (rest, plan_c) = cols.split_cell(rect);
    let [name_c, mtime_c, kind_c, size_c] = cols.rects(rest);
    let k = kind::icon_kind(r);
    // 名称列：图标 ＋ 名字（＋ 名字读不出来时那个记号）。断了的链接图标淡一级。
    let icon_color = match k {
        _ if faded || r.link_broken => p.faint,
        kind::Kind::Folder => p.accent,
        _ => p.text2,
    };
    let painter = ui.painter().clone();
    let icon = painter.layout_no_wrap(
        kind::icon(k).to_string(),
        egui::FontId::proportional(m.icon),
        icon_color,
    );
    let ix = name_c.left() + PAD;
    painter.with_clip_rect(name_c).galley(
        egui::pos2(ix, rect.center().y - icon.size().y / 2.0),
        icon,
        icon_color,
    );
    // 链接：图标右下角叠一个小箭头。
    if r.link {
        painter.with_clip_rect(name_c).text(
            egui::pos2(ix + m.icon * 0.75, rect.center().y + m.icon * 0.3),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::ARROW_UP_RIGHT,
            egui::FontId::proportional(m.icon * 0.55),
            p.text,
        );
    }
    let nx = ix + m.icon + m.space[3];
    // 名字读不出来（不是 UTF-8）⇒ 名字后面跟一个提醒记号（单独一段，警示色）。
    let warn = r.lossy_name.then(|| {
        painter.layout_no_wrap(
            egui_phosphor::regular::WARNING.to_string(),
            egui::TextStyle::Body.resolve(ui.style()),
            p.warn,
        )
    });
    // 就地输入：名字那一格换成输入框（红边 ＝ 填错了，错句挂在格子下面）；别的格照画。
    let editing = inline.is_some();
    if let Some(c) = inline {
        let field = egui::Rect::from_min_max(
            egui::pos2(nx - 6.0, rect.top() + 2.0),
            egui::pos2(name_c.right() - PAD, rect.bottom() - 2.0),
        );
        let id = ui.id().with(("filewin-inline", index));
        let stroke = if c.error.is_some() { p.error } else { p.accent };
        let edit = egui::TextEdit::singleline(c.text)
            .id(id)
            .margin(egui::Margin::symmetric(6, 2))
            .interactive(!c.busy)
            .background_color(p.bg);
        let resp = ui
            .scope_builder(egui::UiBuilder::new().max_rect(field), |ui| {
                ui.visuals_mut().selection.stroke = egui::Stroke::new(1.0, stroke);
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, stroke);
                ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::new(1.0, stroke);
                ui.add_sized(field.size(), edit)
            })
            .inner;
        if let Some(n) = c.select.take() {
            resp.request_focus();
            if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(n),
                )));
                st.store(ui.ctx(), id);
            }
        }
        // 红字那块上一帧占的地方：这一帧的按下点落在里面 ⇒ 不算点别处。
        let err_id = id.with("err");
        let err_rect: Option<egui::Rect> = ui.data(|d| d.get_temp(err_id));
        if !c.busy && resp.lost_focus() {
            let (esc, enter, at) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::Enter),
                    i.pointer.latest_pos(),
                )
            });
            let in_err =
                c.error.is_some() && matches!((err_rect, at), (Some(r), Some(p)) if r.contains(p));
            if esc {
                c.outcome = Some(false);
            } else if in_err {
                resp.request_focus();
            } else if enter || c.blur_submits {
                c.outcome = Some(true);
            }
        }
        if let Some(e) = c.error {
            let r = super::kit::inline_error(ui.ctx(), err_id, field, e, c.detail);
            ui.data_mut(|d| d.insert_temp(err_id, r));
        }
    }
    let room = name_c.right() - nx - PAD - warn.as_ref().map_or(0.0, |w| w.size().x + 6.0);
    let g = one_line(ui, r.name.clone(), room, main);
    let gw = g.size().x;
    // 截成「…」的那几格：悬停在上面看全文；有列收起了 ⇒ 悬停名字连那几列一起看。
    let when = r.mtime_text.clone().map(|short| super::source::MtimeText {
        full: r.mtime_full.clone().unwrap_or_else(|| short.clone()),
        short,
    });
    let size_text = if r.is_dir {
        String::new()
    } else {
        human_size(r.size)
    };
    let mut tips: Vec<(egui::Rect, String)> = Vec::new();
    let folded: Vec<String> = [
        (
            mtime_c,
            when.as_ref().map(|w| w.full.clone()).unwrap_or_default(),
        ),
        (kind_c, kind::type_text(r)),
        (size_c, size_text.clone()),
    ]
    .into_iter()
    .filter(|(c, t)| c.width() <= 0.0 && !t.is_empty())
    .map(|(_, t)| t)
    .collect();
    if g.elided || !folded.is_empty() {
        let mut t = r.name.clone();
        if !folded.is_empty() {
            t.push('\n');
            t.push_str(&folded.join(&copy_text("rsFilewinRows.tip.sep", &[])));
        }
        tips.push((name_c, t));
    }
    if !editing {
        painter.with_clip_rect(name_c).galley(
            egui::pos2(nx, rect.center().y - g.size().y / 2.0),
            g,
            main,
        );
    }
    if let (Some(w), false) = (warn, editing) {
        painter.with_clip_rect(name_c).galley(
            egui::pos2(nx + gw + 6.0, rect.center().y - w.size().y / 2.0),
            w,
            p.warn,
        );
    }
    // 修改时间 · 类型（左齐）· 大小（右齐；目录不写）。
    let mut cell = |c: egui::Rect, text: String, right: bool| {
        if text.is_empty() || c.width() <= 0.0 {
            return;
        }
        let g = one_line(ui, text.clone(), c.width() - 2.0 * PAD, minor);
        if g.elided {
            tips.push((c, text));
        }
        let x = if right {
            c.right() - PAD - g.size().x
        } else {
            c.left() + PAD
        };
        painter.with_clip_rect(c).galley(
            egui::pos2(x, rect.center().y - g.size().y / 2.0),
            g,
            minor,
        );
    };
    cell(
        mtime_c,
        when.as_ref().map(|w| w.short.clone()).unwrap_or_default(),
        false,
    );
    cell(kind_c, kind::type_text(r), false);
    cell(size_c, size_text, true);
    // 「格」那一列：那一格的状态图标 ＋ 标题；被两格声明 ⇒ 后面一个红 ⚠（悬停写另一格）；那一片读不成 ⇒「—」。文件夹空着。
    if let (Some(c), Some(d)) = (plan_c, plan) {
        if let Some(said) = &d.unreadable {
            let g = one_line(
                ui,
                super::plan::unreadable_mark(),
                c.width() - 2.0 * PAD,
                minor,
            );
            painter.with_clip_rect(c).galley(
                egui::pos2(c.left() + PAD, rect.center().y - g.size().y / 2.0),
                g,
                minor,
            );
            tips.push((
                c,
                copy_text(
                    "rsFilewinPlan.cell.unreadableHint",
                    &[("slice", &d.slice), ("said", said)],
                ),
            ));
        } else if let Some(o) = d.owner(&r.name).filter(|_| !r.opens_as_dir()) {
            let tint = match o.status {
                super::plan::Status::Done => p.success,
                _ => minor,
            };
            let ic = painter.layout_no_wrap(
                super::plan::status_icon(o.status).to_string(),
                egui::TextStyle::Body.resolve(ui.style()),
                tint,
            );
            let iw = ic.size().x;
            painter.with_clip_rect(c).galley(
                egui::pos2(c.left() + PAD, rect.center().y - ic.size().y / 2.0),
                ic,
                tint,
            );
            let warn = (!o.dup.is_empty()).then(|| {
                painter.layout_no_wrap(
                    egui_phosphor::regular::WARNING.to_string(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    p.error,
                )
            });
            let ww = warn.as_ref().map_or(0.0, |w| w.size().x + 6.0);
            let tx = c.left() + PAD + iw + 6.0;
            let g = one_line(ui, o.title.clone(), c.right() - PAD - tx - ww, main);
            let gw = g.size().x;
            if g.elided {
                tips.push((c, o.title.clone()));
            }
            painter.with_clip_rect(c).galley(
                egui::pos2(tx, rect.center().y - g.size().y / 2.0),
                g,
                main,
            );
            if let Some(w) = warn {
                painter.with_clip_rect(c).galley(
                    egui::pos2(tx + gw + 6.0, rect.center().y - w.size().y / 2.0),
                    w,
                    p.error,
                );
                tips.push((c, super::plan::dup_text(o)));
            }
        }
    }
    // 悬停在修改时间那一格上 ⇒ 完整时间；悬停在截成「…」的那一格上 ⇒ 全文。
    let at = ui.input(|i| i.pointer.hover_pos());
    let in_time = at.is_some_and(|p| mtime_c.contains(p));
    let tip = at.and_then(|p| {
        tips.into_iter()
            .find(|(c, _)| c.contains(p))
            .map(|(_, t)| t)
    });
    let row = match (when, tip) {
        (Some(w), _) if in_time && row.hovered() => row.on_hover_text(w.full),
        (_, Some(t)) if row.hovered() => row.on_hover_text(t),
        _ => row,
    };
    let mods = ui.input(|i| i.modifiers);
    RowHit {
        picked: row.clicked().then_some(mods),
        menu: row.secondary_clicked(),
        drag_started: row.drag_started_by(egui::PointerButton::Primary),
        activated: row.double_clicked(),
    }
}

/// 开窗时要高亮的那一行在第几行（按名字找）。
pub fn reveal_index(rows: &[Listed], name: &str) -> Option<usize> {
    rows.iter().position(|r| r.name == name)
}

/// 一行占多高（行间不留缝 ⇒ 就是行高令牌）。「滚到第 i 行」用它算偏移。
pub fn row_pitch(ui: &Ui) -> f32 {
    metrics(ui.ctx()).row_h
}

/// 人读的大小。**不是** `format!("{size}")` —— 列表里一列宽度有限。
pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
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
/// 理由与射程照（本机 `XDG_SESSION_TYPE=tty`，没有图形会话）。
/// 那一节的旁证也照抄下来：虚拟滚动那一档三角形数与总行数无关
/// ⇒ 交给 GPU 的活不随行数涨，**它不是 64 万行的风险点**。
pub fn render_headless(
    ctx: &egui::Context,
    rows: &[Listed],
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
        show_file_rows(
            ui,
            rows,
            &mut t,
            Some(scroll_offset_y),
            None,
            None,
            &Columns::default(),
        );
        tally = t;
    });
    out.drop_without_applying_deltas();
    tally
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/rows_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/rows_tests.rs"]
mod tests;
