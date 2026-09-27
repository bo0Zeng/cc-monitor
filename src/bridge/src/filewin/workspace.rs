//! 〔FW34 · 第四波 2026-09-24〕窗口的最外一层：**标签页 ＋ 双栏 ＋ 复制到另一栏**（预览开关也挂在这儿，
//! 预览本身住 [`super::preview`]：右侧一块，跟焦点那一栏）。
//!
//! 设计住 `调研/第四波记录/FW34.md` 第三节；这里只留落地要知道的。
//!
//! # 一、形状：`FileWindow` 不拆，它就是一个标签页
//!
//! ```text
//! Workspace
//!   sides: [Side; 1 或 2]      一栏 / 两栏
//!   focus: 栏号               键盘 · 拖入 · 预览 · 「复制到另一栏」的源，都跟它
//! Side  { tabs: [Tab], active }
//! Tab   { id, pane: FileWindow }   // 一个目录视图的全部状态，一个字段没搬
//! ```
//!
//! ⇒ 从前几百条判据全在 `FileWindow` 上，这一刀一条不动；`eframe::App` 从 `FileWindow` 挪到这里。
//! 同一个窗口里所有标签页 / 两栏看的都是**同一台机器**（同一个 `Source`、同一条通道）：换机器就开另一个窗口。
//!
//! # 二、焦点那道闸（为什么非有不可）
//!
//! 两栏同时画 ⇒ 两个 `frame_body` 都会去接这一帧的按键与拖入。不闸的话按一下 Delete 两边各删一次、
//! 拖一个文件进来两边各传一次。⇒ 只有焦点那一栏的那个标签 `focused = true`（[`Workspace::sync_focus`]），
//! 其余一律 `false`（`FileWindow::keys_blocked` / `take_drops` 多问的就是它）。
//! 在哪一栏里按下鼠标，焦点就给哪一栏（这一帧按下、下一帧生效）。
//!
//! # 三、关标签 / 收右栏的规矩
//!
//! 那个标签手上**有活**（有一问摆着、有东西在传 / 在复制 / 在写 / 在读写文本）⇒ 不关，说是哪一件
//! （`FileWindow::busy_reason`）。后台那几趟任务不随标签走，关掉就再也没人把结局摆给你看。
//! 关**整个窗口**照旧不问（用户裁「窗口生命周期就是销毁」）。
//!
//! # ⚠ 买不到什么
//!
//! - 真窗口真画在屏幕上（要图形会话）；判据跑的是生产那个 [`Workspace::frame`]，读这一帧画出来的字。
//! - 〔W5-FILES〕**拖**一行到另一栏做了（行上命中矩形换成 `click_and_drag`，松手落在另一栏 ⇒ 同一个「复制到另一栏」入口，
//!   [`Workspace::settle_drag`]）；判据喂的是合成指针事件，真鼠标买不到。
//! - 后台标签（不在任何一栏上）的「一问」要切回去才看得见；标签名前那个「●」就是为这个。

use super::copy::CopyJob;
use super::shell::FileWindow;
use super::source::Listed;
use crate::copy_table::copy_text;

/// 一个标签页：一个目录视图。
pub struct Tab {
    /// 这个标签的 egui 身份（不随下标移位：关掉左边那个，右边这个的滚动位置不串）。
    pub id: u64,
    pub pane: FileWindow,
}

/// 一栏：一排标签页，当前显示第几个。
pub struct Side {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

/// 窗口的最外一层。
pub struct Workspace {
    sides: Vec<Side>,
    focus: usize,
    next_id: u64,
    /// 上一件「做不了」时说的那句话（`None` ＝ 没话说）。下一次点工具条 / 标签栏就清。
    notice: Option<String>,
    /// 〔预览〕开着吗 ＋ 它自己的状态（`None` ＝ 关着）。逻辑住 [`super::preview`]。
    pub preview: Option<super::preview::Preview>,
}

/// 工具条上那几颗 —— **唯一住址**（判据按同一个常量去找它画出来的字）。
pub static SPLIT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.split", &[]));
pub static PREVIEW_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.preview", &[]));
pub static COPY_ACROSS_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.copyAcross", &[]));
pub static NEW_TAB_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.newTab", &[]));
pub static CLOSE_TAB_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.closeTab", &[]));
/// 后台标签手上有事等你（有一问摆着 / 有活在跑）时，标签名前那个记号。
pub static BUSY_MARK: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWorkspace.label.busyMark", &[]));

/// 〔W5-FILES · `设计/60 §6.2`〕标签页快捷键想干什么。**只是意图**，做不做由 [`Workspace::apply_tab_keys`] 过闸。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKey {
    /// Ctrl+T（macOS ⌘T）＝ 焦点那一栏的「＋」。
    New,
    /// Ctrl+W（macOS ⌘W）＝ 焦点那一栏当前标签的「×」（关的是标签页，不是窗口）。
    Close,
}

/// 这一帧的事件 → 标签页快捷键（按到达顺序，只认按下）。
///
/// ⚠ 与 `select::intents` 分住两处是**归属**，不是风格：列表的键归那个目录视图，标签页归工作区
/// （一个目录视图不知道自己在哪一栏、有几个兄弟）。两张键位表不相交（`select` 那张里带 Ctrl 的只有 Ctrl+A），
/// Ctrl 按着时 egui-winit 不发 `Text` ⇒ 也不会同时触发打字跳转。
pub fn tab_keys(events: &[egui::Event]) -> Vec<TabKey> {
    events
        .iter()
        .filter_map(|ev| match ev {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers: m,
                ..
            } if m.command && !m.shift && !m.alt => match key {
                egui::Key::T => Some(TabKey::New),
                egui::Key::W => Some(TabKey::Close),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

impl Workspace {
    /// 从开窗那一个标签页起步（它身上已经挂好了通道 · 运行时 · 书签 · 字体）。
    pub fn new(first: FileWindow) -> Self {
        let mut w = Self {
            sides: vec![Side {
                tabs: vec![Tab { id: 0, pane: first }],
                active: 0,
            }],
            focus: 0,
            next_id: 1,
            notice: None,
            preview: None,
        };
        w.sync_focus();
        w
    }

    /// 几栏（1 或 2）。
    pub fn sides(&self) -> usize {
        self.sides.len()
    }

    /// 第 `side` 栏有几个标签页。
    pub fn tabs_on(&self, side: usize) -> usize {
        self.sides[side].tabs.len()
    }

    /// 第 `side` 栏当前显示第几个标签页。
    pub fn active_on(&self, side: usize) -> usize {
        self.sides[side].active
    }

    /// 焦点在第几栏。
    pub fn focus(&self) -> usize {
        self.focus
    }

    /// 第 `side` 栏正显示的那个目录视图。
    pub fn pane_on(&self, side: usize) -> &FileWindow {
        let s = &self.sides[side];
        &s.tabs[s.active].pane
    }

    /// 同上，可改。
    pub fn pane_on_mut(&mut self, side: usize) -> &mut FileWindow {
        let s = &mut self.sides[side];
        &mut s.tabs[s.active].pane
    }

    /// 第 `side` 栏第 `i` 个标签页（判据用：后台标签也看得到）。
    pub fn tab(&self, side: usize, i: usize) -> &FileWindow {
        &self.sides[side].tabs[i].pane
    }

    /// 上一件做不了时那句话。
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 焦点只给焦点那一栏的那个标签，其余一律关掉（本模块头注 §二）。
    fn sync_focus(&mut self) {
        for (k, s) in self.sides.iter_mut().enumerate() {
            for (i, t) in s.tabs.iter_mut().enumerate() {
                t.pane.focused = k == self.focus && i == s.active;
            }
        }
    }

    /// 把焦点给第 `side` 栏。
    pub fn focus_side(&mut self, side: usize) {
        if side < self.sides.len() {
            self.focus = side;
            self.sync_focus();
        }
    }

    /// 照 `like` 那个目录视图的样子起一个新的，落在 `cwd`：同一台机器、同一条通道、同一份书签、
    /// 同一个字体结论（字体装在整个窗口上，不是装在某一个标签上）。**建完就列一趟目录。**
    fn spawn_pane(like: &FileWindow, cwd: String) -> FileWindow {
        let mut p = FileWindow::seeded(
            like.source.clone(),
            cwd,
            like.rt.clone(),
            Vec::<Listed>::new(),
        );
        if let Some(line) = like.line.clone() {
            p.attach_line(line);
        }
        p.shelf = like.shelf.clone();
        p.machines = like.machines.clone();
        p.font = like.font.clone();
        p.reload();
        p
    }

    fn mint_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 在第 `side` 栏开一个新标签页，落在这一栏当前的目录上，并切过去。
    pub fn open_tab(&mut self, side: usize) -> bool {
        if side >= self.sides.len() {
            return false;
        }
        let cwd = self.pane_on(side).cwd.clone();
        let pane = Self::spawn_pane(self.pane_on(side), cwd);
        self.add_tab(side, pane)
    }

    /// 把一个建好的目录视图作为新标签页挂到第 `side` 栏，并切过去（焦点随之给这一栏）。
    pub fn add_tab(&mut self, side: usize, pane: FileWindow) -> bool {
        if side >= self.sides.len() {
            return false;
        }
        let id = self.mint_id();
        let s = &mut self.sides[side];
        s.tabs.push(Tab { id, pane });
        s.active = s.tabs.len() - 1;
        self.focus = side;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 把一个建好的目录视图挂成右栏（只在一栏时；焦点给它）。回值 ＝ 真的挂上了。
    pub fn add_side(&mut self, pane: FileWindow) -> bool {
        if self.sides.len() != 1 {
            return false;
        }
        let id = self.mint_id();
        self.sides.push(Side {
            tabs: vec![Tab { id, pane }],
            active: 0,
        });
        self.focus = 1;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 切到第 `side` 栏第 `i` 个标签页（焦点随之给这一栏）。
    pub fn select_tab(&mut self, side: usize, i: usize) -> bool {
        if side >= self.sides.len() || i >= self.sides[side].tabs.len() {
            return false;
        }
        self.sides[side].active = i;
        self.focus = side;
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 关掉第 `side` 栏第 `i` 个标签页。**一栏最后一个不关**（要关就关窗口 / 收掉这一栏）；
    /// **手上有活的不关**，出声说是哪一件。
    pub fn close_tab(&mut self, side: usize, i: usize) -> bool {
        if side >= self.sides.len() || i >= self.sides[side].tabs.len() {
            return false;
        }
        if self.sides[side].tabs.len() == 1 {
            self.notice = Some(copy_text("rsFilewinWorkspace.closeTab.last", &[]).into());
            return false;
        }
        if let Some(why) = self.sides[side].tabs[i].pane.busy_reason() {
            self.notice = Some(copy_text(
                "rsFilewinWorkspace.closeTab.busy",
                &[("why", &why.to_string())],
            ));
            return false;
        }
        let s = &mut self.sides[side];
        s.tabs.remove(i);
        if s.active > i || s.active >= s.tabs.len() {
            s.active = s.active.saturating_sub(1);
        }
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 开 / 收双栏。开 ⇒ 右边长出一栏（一个标签页，落在焦点那一栏的目录上），焦点给它；
    /// 收 ⇒ 右栏整栏收掉（里面有哪个标签手上有活就不收，出声）。回值 ＝ 真的变了。
    pub fn set_split(&mut self, on: bool) -> bool {
        match (on, self.sides.len()) {
            (true, 1) => {
                let cwd = self.pane_on(self.focus).cwd.clone();
                let pane = Self::spawn_pane(self.pane_on(self.focus), cwd);
                return self.add_side(pane);
            }
            (false, 2) => {
                if let Some(why) = self.sides[1].tabs.iter().find_map(|t| t.pane.busy_reason()) {
                    self.notice = Some(copy_text(
                        "rsFilewinWorkspace.setSplit.busy",
                        &[("why", &why.to_string())],
                    ));
                    return false;
                }
                self.sides.pop();
                self.focus = 0;
            }
            _ => return false,
        }
        self.notice = None;
        self.sync_focus();
        true
    }

    /// 〔W5-FILES〕这一帧的 Ctrl+T / Ctrl+W。回值 ＝ 认出了几件（做不了的那几形由 `open_tab` / `close_tab` 出声）。
    ///
    /// 🔴 **闸与列表同一道**：焦点那一栏当前那个标签的 `keys_blocked`（`设计/60 §6.3` 四道闸 ——
    /// 模态框 · 右键菜单 · 搜索命中那一摞 · 控件拿着键盘焦点）。不另立一道：分成两份的症状是
    /// 「框开着，按 Ctrl+W 把框底下那个标签关了」。
    pub fn apply_tab_keys(&mut self, ctx: &egui::Context) -> usize {
        if self.pane_on(self.focus).keys_blocked(ctx) {
            return 0;
        }
        let keys = ctx.input(|i| tab_keys(&i.events));
        for k in &keys {
            let side = self.focus;
            match k {
                TabKey::New => {
                    self.open_tab(side);
                }
                TabKey::Close => {
                    let i = self.active_on(side);
                    self.close_tab(side, i);
                }
            }
        }
        keys.len()
    }

    /// 开 / 关预览。关 ＝ 整块状态扔掉（在飞的那一趟回来没人收，无害）。
    pub fn set_preview(&mut self, on: bool) {
        self.preview = on.then(super::preview::Preview::default);
    }

    /// 🔴 **复制到另一栏**：焦点那一栏选中的**那一摞**（文件与目录都行）→ 另一栏当前目录、同名。
    ///
    /// 〔W5-FILES · `设计/60 §6.2`「复制目录 · 批量复制」〕此前只收「恰好一个文件」。今天走一摞复制
    /// （[`super::copy::run_copy_batch`]：逐件探目标 → 撞名的目录整摞不做 → 撞名的文件一次问完 → 逐件发），
    /// 后端 `files-copy`（目录带 `recursive: true`），**起在目标那一栏上** ⇒ 问与结局画在那一侧，跑完那一栏重列目录。
    /// `§6.3`：每一项都能复制才给（有损名那一项在，整摞不做、出声）。回值 ＝ 真的起来了。
    pub fn copy_to_other(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.sides.len() != 2 {
            self.notice = Some(copy_text("rsFilewinWorkspace.copyToOther.needSplit", &[]).into());
            return false;
        }
        let from = self.pane_on(self.focus);
        let rows = from.picked_rows();
        if rows.is_empty() {
            self.notice = Some(copy_text("rsFilewinSelect.refusal.none", &[]));
            return false;
        }
        if let Some(r) = rows.iter().find(|r| !super::copy::copyable(r)) {
            self.notice = Some(copy_text(
                "rsFilewinWorkspace.copyToOther.unaddressable",
                &[("name", &r.name.to_string())],
            ));
            return false;
        }
        let other = 1 - self.focus;
        let dest_dir = self.pane_on(other).cwd.clone();
        let dest_path = self.pane_on(other).cwd_path();
        if dest_path == from.cwd_path() {
            self.notice = Some(copy_text("rsFilewinWorkspace.copyToOther.sameDir", &[]).into());
            return false;
        }
        let jobs: Vec<CopyJob> = rows
            .iter()
            .map(|r| {
                // 〔W5-FILES · 有损名全寻址〕源 ＝ 源那一栏当前目录的字节 ＋ 名字的字节；目标 ＝ 另一栏当前目录的字节 ＋ 同一个名字。
                let src = from.row_path(r);
                let dst = super::shell::join_path(&dest_path, &super::shell::name_bytes(r));
                CopyJob {
                    from: src.shown.clone(),
                    to: if dst.is_lossy() {
                        dst.shown.clone()
                    } else {
                        super::writeops::join_remote(&dest_dir, &r.name)
                    },
                    name: r.name.clone(),
                    is_dir: r.is_dir,
                    from_raw: src.raw,
                    to_raw: dst.raw,
                }
            })
            .collect();
        self.notice = None;
        self.pane_on_mut(other).start_copy_batch(jobs, ctx)
    }

    /// 标签上写什么：当前目录的最后一段（根就写 `/`）；手上有事的前面加「●」。
    pub fn tab_title(pane: &FileWindow) -> String {
        let tail = super::source::remote_basename(&pane.cwd);
        let tail = if tail.is_empty() { "/" } else { tail };
        let mark = if pane.busy_reason().is_some() {
            BUSY_MARK.as_str()
        } else {
            ""
        };
        format!("{mark}{tail}")
    }

    /// 🔴 **每一帧的正文**（`eframe::App::ui` 只剩一句委派，判据直接喂它 —— 同 `FileWindow::frame_body`）。
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        // ── 〔W5-FILES〕标签页快捷键：先于两栏的正文（那里才是列表接键盘的地方）──
        let ctx = ui.ctx().clone();
        self.apply_tab_keys(&ctx);
        // ── 工具条：双栏 · 预览 · 复制到另一栏 ──
        let mut split: Option<bool> = None;
        let mut preview: Option<bool> = None;
        let mut across = false;
        ui.horizontal(|ui| {
            let two = self.sides.len() == 2;
            if ui.selectable_label(two, SPLIT_LABEL.as_str()).clicked() {
                split = Some(!two);
            }
            let on = self.preview.is_some();
            if ui.selectable_label(on, PREVIEW_LABEL.as_str()).clicked() {
                preview = Some(!on);
            }
            if two && ui.button(COPY_ACROSS_LABEL.as_str()).clicked() {
                across = true;
            }
            if let Some(n) = &self.notice {
                ui.colored_label(egui::Color32::from_rgb(0xFF, 0xA5, 0x00), n);
            }
        });
        if let Some(on) = split {
            self.set_split(on);
        }
        if let Some(on) = preview {
            self.set_preview(on);
        }
        if across {
            let ctx = ui.ctx().clone();
            self.copy_to_other(Some(ctx));
        }
        // ── 预览（右侧一块，跟焦点那一栏）──
        if self.preview.is_some() {
            let ctx = ui.ctx().clone();
            let side = &self.sides[self.focus];
            let pane = &side.tabs[side.active].pane;
            if let Some(p) = self.preview.as_mut() {
                p.follow(pane, Some(ctx));
                egui::Panel::right("filewin-preview-panel")
                    .default_size(360.0)
                    .show(ui, |ui| p.ui(ui));
            }
        }
        // ── 一栏 / 两栏 ──
        let whole = ui.available_rect_before_wrap();
        let n = self.sides.len();
        let gap = ui.spacing().item_spacing.x;
        let w = (whole.width() - gap * (n as f32 - 1.0)) / n as f32;
        let pressed_at = ui.input(|i| {
            (i.pointer.primary_pressed() || i.pointer.secondary_pressed())
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let mut focus_to: Option<usize> = None;
        let mut rects: Vec<egui::Rect> = Vec::with_capacity(n);
        for k in 0..n {
            let rect = egui::Rect::from_min_size(
                egui::pos2(whole.left() + k as f32 * (w + gap), whole.top()),
                egui::vec2(w, whole.height()),
            );
            rects.push(rect);
            if pressed_at.is_some_and(|p| rect.contains(p)) {
                focus_to = Some(k);
            }
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("filewin-side", k))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
                |ui| self.side_ui(ui, k),
            );
        }
        ui.allocate_rect(whole, egui::Sense::hover());
        if let Some(k) = focus_to {
            if k != self.focus {
                self.focus_side(k);
            }
        }
        self.settle_drag(ui, &rects);
    }

    /// 〔W5-FILES · `设计/60 §6.2`「行拖到另一栏的手势」〕有一栏在拖：拖着时在指针旁说一句「复制 N 项到另一栏」
    /// （只在指针落在另一栏里时说）；**松手**那一帧落在另一栏 ⇒ 走「复制到另一栏」**那一个入口**（[`Self::copy_to_other`]，
    /// 不另起一条复制路）；落在本栏 / 窗外 ⇒ 什么都不做。只有一栏时没有「另一栏」，拖了也不做（与按钮同）。V122：不往 OS 拖出。
    fn settle_drag(&mut self, ui: &mut egui::Ui, rects: &[egui::Rect]) {
        let Some(k) = (0..self.sides.len()).find(|&k| self.pane_on(k).dragging) else {
            return;
        };
        let (released, pos) = ui.input(|i| {
            (
                i.pointer.any_released(),
                i.pointer.interact_pos().or(i.pointer.latest_pos()),
            )
        });
        let over_other = self.sides.len() == 2
            && pos.is_some_and(|p| rects.get(1 - k).is_some_and(|r| r.contains(p)));
        if released {
            for s in 0..self.sides.len() {
                self.pane_on_mut(s).dragging = false;
            }
            if over_other {
                self.focus_side(k);
                let ctx = ui.ctx().clone();
                self.copy_to_other(Some(ctx));
            }
            return;
        }
        if let (true, Some(p)) = (over_other, pos) {
            let n = self.pane_on(k).selection().len();
            let hint = copy_text("rsFilewinWorkspace.drag.hint", &[("n", &n.to_string())]);
            ui.ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("filewin-drag-hint"),
                ))
                .text(
                    p + egui::vec2(14.0, 14.0),
                    egui::Align2::LEFT_TOP,
                    hint,
                    egui::FontId::proportional(14.0),
                    ui.visuals().strong_text_color(),
                );
        }
    }

    /// 一栏：标签栏 ＋ 当前那个标签页的正文。
    fn side_ui(&mut self, ui: &mut egui::Ui, k: usize) {
        let mut pick: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut new_tab = false;
        ui.horizontal_wrapped(|ui| {
            let s = &self.sides[k];
            let many = s.tabs.len() > 1;
            for (i, t) in s.tabs.iter().enumerate() {
                if ui
                    .selectable_label(i == s.active, Self::tab_title(&t.pane))
                    .on_hover_text(&t.pane.cwd)
                    .clicked()
                {
                    pick = Some(i);
                }
                if many
                    && ui
                        .small_button(CLOSE_TAB_LABEL.as_str())
                        .on_hover_text(&copy_text("rsFilewinWorkspace.sideUi.closeHint", &[]))
                        .clicked()
                {
                    close = Some(i);
                }
            }
            if ui
                .small_button(NEW_TAB_LABEL.as_str())
                .on_hover_text(&copy_text("rsFilewinWorkspace.sideUi.newTabHint", &[]))
                .clicked()
            {
                new_tab = true;
            }
        });
        if let Some(i) = pick {
            self.select_tab(k, i);
        }
        if let Some(i) = close {
            self.close_tab(k, i);
        }
        if new_tab {
            self.open_tab(k);
        }
        let s = &mut self.sides[k];
        let t = &mut s.tabs[s.active];
        ui.push_id(("filewin-tab", t.id), |ui| t.pane.frame_body(ui));
    }
}

/// 两个目录的最长公共前缀。都只在根下相交 ⇒ `/`。
///
/// ⚠ **自己一刀都不切**：拿 [`super::source::breadcrumbs`]（远端路径那一族切法的唯一住址，只认 `/`）
/// 各切一摞前缀，取两摞共有的最长那一个 ⇒ 不多出第四份切法
/// （`source_tests::every_place_that_splits_a_remote_path_is_declared` 钉着那张表）。
pub fn common_dir(a: &str, b: &str) -> String {
    let theirs: Vec<String> = super::source::breadcrumbs(b)
        .into_iter()
        .map(|(_, full)| full)
        .collect();
    super::source::breadcrumbs(a)
        .into_iter()
        .map(|(_, full)| full)
        .take_while(|f| theirs.contains(f))
        .last()
        .unwrap_or_else(|| "/".to_string())
}

/// 跨目录那一形的 `files-copy` 参数：`root` ＝ 两个父目录的公共前缀，`from` / `to` ＝ 各自相对它的那一段。
///
/// ⚠ 同目录那一形（「复制为」那个框）走的是 `copy::copy_args`，它**刻意拒**跨目录（防「当前目录」与
///   「那一行的路径」写法不一致时拼到别处去）—— 这里是另一件事（用户明说「放到另一栏那个目录」），
///   所以另起一个，不去放宽那一道。后端的 `from` / `to` 本来就收多段相对路径（逐段过词法围栏）。
pub fn across_args(job: &CopyJob, overwrite: bool) -> Result<serde_json::Value, String> {
    // 〔W5-FILES · 有损名全寻址〕按**字节**切（合法 UTF-8 时与按串切逐字节同：`common_dir` 与它同一个按段比的口径）。
    let (from, to) = (job.from_path(), job.to_path());
    let root = super::source::common_dir_bytes(&from.parent().bytes(), &to.parent().bytes());
    let root_path = super::source::RemotePath::from_bytes(&root);
    let rel = |p: &super::source::RemotePath| -> Result<serde_json::Value, String> {
        let b = p.bytes();
        let cut = if root == b"/" { 1 } else { root.len() + 1 };
        (b.starts_with(&root)
            && b.len() > cut
            && (root == b"/" || b.get(root.len()) == Some(&b'/')))
        .then(|| super::source::wire_bytes(&b[cut..]))
        .ok_or_else(|| {
            copy_text(
                "rsFilewinWorkspace.acrossArgs.notUnder",
                &[("path", &p.shown), ("root", &root_path.shown)],
            )
        })
    };
    let mut v = serde_json::json!({
        "root": root_path.wire(),
        "from": rel(&from)?,
        "to": rel(&to)?,
        "overwrite": overwrite,
    });
    super::copy::mark_recursive(&mut v, job);
    Ok(v)
}

/// 跨目录复制那一趟（经通道问后端 `files-copy`）。回这一件复制了什么（〔W5-FILES〕目录那一件是整棵）。
pub async fn copy_across(
    line: &super::source::Line,
    origin: &super::source::Origin,
    job: &CopyJob,
    overwrite: bool,
) -> Result<super::copy::Copied, String> {
    let args = across_args(job, overwrite)?;
    let d = super::source::ask(
        line,
        origin,
        super::copy::CMD_COPY,
        &args,
        super::copy::COPY_BUDGET,
    )
    .await?;
    super::copy::copied_from_reply(job, &d)
}

impl eframe::App for Workspace {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/workspace_tests.rs"]
mod tests;
